//! Background thread implementations for the async font registry.
//!
//! - [`FcFontRegistry::scout_thread`]: Enumerates font directories and populates the build queue.
//! - [`FcFontRegistry::builder_thread`]: Pops jobs from the queue, parses fonts, inserts results.

use alloc::vec::Vec;

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::Duration;

use crate::config;
use crate::registry::FcFontRegistry;
use crate::scoring::{assign_scout_priority, FcBuildJob};
use crate::utils::is_font_file;
use crate::FcParseFont;
#[cfg(target_os = "ios")]
use crate::OperatingSystem;

impl FcFontRegistry {
    /// Scout thread: enumerates font directories and populates the build queue.
    ///
    /// 1. Walks all OS font directories recursively, collecting font file paths.
    /// 2. Tokenizes each filename and assigns a priority (High for common
    ///    OS fonts, Low for everything else).
    /// 3. Populates `known_paths` (family → file paths) and `build_queue`.
    /// 4. Signals `scan_complete` when done.
    pub fn scout_thread(&self) {
        let font_dirs = config::font_directories(self.os);
        let common_token_sets = config::tokenize_common_families(self.os);
        let lazy = self.lazy_scout.load(Ordering::Acquire);

        // iOS: the app sandbox denies `read_dir` on `/System/Library/...`
        // even though every individual font URL is openable. CoreText is
        // the only enumeration path. Branch off here, hand the resulting
        // PathBufs to the same `known_paths` / `build_queue` merge that
        // the per-directory walk uses.
        #[cfg(target_os = "ios")]
        {
            if self.os == OperatingSystem::IOS {
                let ios_paths = crate::mobile_ios::copy_available_font_urls();
                self.publish_ios_font_urls(ios_paths, &common_token_sets, lazy);
                self.scan_complete.store(true, Ordering::Release);
                self.queue_condvar.notify_all();
                self.progress.notify_all();
                return;
            }
        }

        // Per-directory publish: walk one top-level font directory
        // at a time, collect its paths, then take a brief write
        // lock to merge into `known_paths`. Readers blocked on
        // `known_paths.read()` wake up between directories and can
        // immediately probe any family whose file already landed.
        //
        // Before this change the scout held the write lock for the
        // *entire* FS walk — ~130 ms on macOS cold — so every
        // consumer that called `request_fonts_fast` during init
        // stalled the whole time. Now the critical-section per
        // directory is just "insert N paths into a BTreeMap",
        // typically <2 ms per directory on macOS.
        for dir_path in font_dirs {
            if self.shutdown.load(Ordering::Relaxed) {
                return;
            }
            if std::fs::read_dir(&dir_path).is_err() {
                continue;
            }

            let mut dir_paths: Vec<PathBuf> = Vec::new();
            collect_font_files_recursive(dir_path, &mut dir_paths);

            if dir_paths.is_empty() {
                continue;
            }

            let Ok(mut known_paths) = self.known_paths.write() else { return };
            let mut queue_opt = (!lazy).then(|| self.build_queue.lock().ok()).flatten();

            for path in &dir_paths {
                let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                let guessed_family = config::guess_family_from_filename(path);

                known_paths
                    .entry(guessed_family.clone())
                    .or_insert_with(Vec::new)
                    .push(path.clone());

                if let Some(queue) = queue_opt.as_mut() {
                    let all_tokens = config::tokenize_lowercase(stem);
                    let priority = assign_scout_priority(&all_tokens, &common_token_sets);
                    queue.push(FcBuildJob {
                        priority,
                        path: path.clone(),
                        font_index: None,
                        guessed_family,
                    });
                }
            }

            if let Some(mut queue) = queue_opt {
                queue.sort();
                drop(queue);
            }
            drop(known_paths);

            // Notify callers waiting on `progress` that new paths
            // landed. `request_fonts_fast` re-checks its family
            // lookup on every wake-up; a DOM that only needs
            // Helvetica can proceed the moment the directory
            // containing HelveticaNeue.ttc has been merged.
            self.progress.notify_all();
        }

        self.scan_complete.store(true, Ordering::Release);
        self.queue_condvar.notify_all();
        self.progress.notify_all();
    }

    /// Merge a batch of CoreText-discovered font URLs into the registry,
    /// mirroring the per-directory publish path used by `scout_thread`.
    ///
    /// iOS-only: the standard `read_dir` walk returns nothing inside the app
    /// sandbox, so this is the only way the async registry sees system fonts.
    #[cfg(target_os = "ios")]
    fn publish_ios_font_urls(
        &self,
        ios_paths: Vec<PathBuf>,
        common_token_sets: &[Vec<alloc::string::String>],
        lazy: bool,
    ) {
        // Filter to recognized font extensions. CoreText also returns app-bundled
        // resources occasionally, so the filter keeps us pruning anything not
        // parseable.
        let filtered: Vec<PathBuf> = ios_paths
            .into_iter()
            .filter(|p| is_font_file(p))
            .collect();

        if filtered.is_empty() {
            return;
        }

        let Ok(mut known_paths) = self.known_paths.write() else { return };
        let mut queue_opt = (!lazy).then(|| self.build_queue.lock().ok()).flatten();

        for path in &filtered {
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let guessed_family = config::guess_family_from_filename(path);

            known_paths
                .entry(guessed_family.clone())
                .or_insert_with(Vec::new)
                .push(path.clone());

            if let Some(queue) = queue_opt.as_mut() {
                let all_tokens = config::tokenize_lowercase(stem);
                let priority = assign_scout_priority(&all_tokens, common_token_sets);
                queue.push(FcBuildJob {
                    priority,
                    path: path.clone(),
                    font_index: None,
                    guessed_family,
                });
            }
        }

        if let Some(mut queue) = queue_opt {
            queue.sort();
            drop(queue);
        }
        drop(known_paths);

        self.progress.notify_all();
    }

    /// Builder thread loop: pops jobs from the priority queue, parses fonts,
    /// and inserts results into the registry.
    ///
    /// Exit conditions:
    ///
    /// - `shutdown` is set (registry is dropping).
    /// - In **eager** mode: once the scout finishes the initial
    ///   directory walk, queue empties, and every queued path is
    ///   processed. At that point `build_complete` flips and the
    ///   thread returns.
    /// - In **lazy-scout** mode: the thread keeps waiting on
    ///   `queue_condvar` indefinitely, because the scout does not
    ///   pre-queue anything — all jobs come in later from
    ///   [`FcFontRegistry::request_fonts`]. Exiting on the
    ///   "queue empty + scan complete" condition (as the eager
    ///   path does) would race the Critical job push and cause the
    ///   request to hang forever.
    pub fn builder_thread(&self) {
        loop {
            if self.shutdown.load(Ordering::Relaxed) {
                return;
            }

            let lazy = self.lazy_scout.load(Ordering::Acquire);

            // Pop the highest-priority job
            let job = {
                let mut queue = match self.build_queue.lock() {
                    Ok(q) => q,
                    Err(_) => return,
                };

                loop {
                    if self.shutdown.load(Ordering::Relaxed) {
                        return;
                    }

                    if let Some(job) = queue.pop() {
                        break job;
                    }

                    // Eager mode: exit once the scout is done and
                    // everything it queued has drained.
                    //
                    // Lazy mode: keep waiting — `request_fonts` is
                    // the sole source of jobs and can fire at any
                    // time during the layout pass.
                    if !lazy
                        && self.scan_complete.load(Ordering::Acquire)
                        && queue.is_empty()
                    {
                        self.build_complete.store(true, Ordering::Release);
                        self.progress.notify_all();
                        return;
                    }

                    // Wait for new jobs
                    queue = match self
                        .queue_condvar
                        .wait_timeout(queue, Duration::from_millis(100))
                    {
                        Ok(result) => result.0,
                        Err(_) => return,
                    };
                }
            };

            // Deduplication: skip if already processed
            {
                let mut processed = match self.processed_paths.lock() {
                    Ok(p) => p,
                    Err(_) => continue,
                };
                if processed.contains(&job.path) {
                    continue;
                }
                processed.insert(job.path.clone());
            }

            // Parse the font file
            if let Some(results) = FcParseFont(&job.path) {
                for (pattern, font_path) in results {
                    self.insert_font(pattern, font_path);
                }
            }

            // Mark this file as fully completed (patterns inserted)
            if let Ok(mut completed) = self.completed_paths.lock() {
                completed.insert(job.path.clone());
            }

            // Notify waiting threads that a font has been completed
            self.progress.notify_all();
        }
    }
}

/// Recursively collect font files from a directory.
fn collect_font_files_recursive(dir: PathBuf, results: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() {
            collect_font_files_recursive(path, results);
        } else if is_font_file(&path) {
            results.push(path);
        }
    }
}
