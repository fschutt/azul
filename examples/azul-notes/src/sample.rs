//! The sample library (`--sample`): a few notebooks, nested; tagged notes
//! with headings, lists, checklists, code, quotes and links; two pinned.
//! The ids are fixed, so scripts can name a note; the dates are relative to
//! `now`, so the list's sections ("Today", "Yesterday", ...) are filled.

use crate::{
    markdown::{self, Meta},
    model,
};

const HOUR: u64 = 3_600;
const DAY: u64 = 86_400;

/// A sample note: id suffix, notebook, title, tags, pinned, age, body.
struct Sample {
    n: u32,
    notebook: &'static str,
    title: &'static str,
    tags: &'static [&'static str],
    pinned: bool,
    age: u64,
    body: &'static str,
}

/// The id of sample note `n`.
#[must_use]
pub fn sample_id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012}")
}

const SAMPLES: &[Sample] = &[
    Sample {
        n: 1,
        notebook: "Work/Offsite",
        title: "Offsite agenda",
        tags: &["planning", "work"],
        pinned: true,
        age: 2 * HOUR,
        body: "# Goals\n\n- Agree on **Q4 priorities**\n- Pick the release date\n  - before the holidays\n\n## Checklist\n\n- [x] Book the room\n- [ ] Print agendas\n- [ ] Send the dial-in link\n\n> Keep the morning for decisions, the afternoon for the walk.\n\nVenue notes: [the lake house](https://example.org/lake-house).\n",
    },
    Sample {
        n: 2,
        notebook: "Work/Meetings",
        title: "Standup notes",
        tags: &["daily"],
        pinned: false,
        age: 30 * 60,
        body: "**Kai:** blocked on the build cache.\n\n**Mia:** shipping the *search* fix today.\n\n1. Retry the cache with a clean volume\n2. Pair on the flaky test\n",
    },
    Sample {
        n: 3,
        notebook: "Personal",
        title: "Packing list",
        tags: &["travel"],
        pinned: true,
        age: DAY + 3 * HOUR,
        body: "- [x] Passport\n- [ ] Charger\n- [ ] Rain jacket\n- [ ] Book for the train\n",
    },
    Sample {
        n: 4,
        notebook: "Personal/Recipes",
        title: "Pancakes",
        tags: &["recipes", "breakfast"],
        pinned: false,
        age: 3 * DAY,
        body: "## Ingredients\n\n- 200 g flour\n- 2 eggs\n- 300 ml milk\n- a pinch of salt\n\n## Steps\n\n1. Whisk everything into a smooth batter\n2. Rest it for 20 minutes\n3. Fry thin, flip once\n",
    },
    Sample {
        n: 5,
        notebook: "Work",
        title: "Release script",
        tags: &["ideas", "work"],
        pinned: false,
        age: 5 * DAY,
        body: "The release is three commands:\n\n```sh\ncargo build --release\n./scripts/sign.sh target/release/AzNotes\n./scripts/upload.sh\n```\n\nRun `sign.sh` on the build machine only.\n",
    },
    Sample {
        n: 6,
        notebook: "Reading",
        title: "Book club: Dune",
        tags: &["reading"],
        pinned: false,
        age: 9 * DAY,
        body: "Chapters 1-12 for Thursday.\n\n> I must not fear. Fear is the mind-killer.\n\nQuestions:\n\n- Why does Paul trust Jessica's training?\n- What does the spice *cost*?\n",
    },
    Sample {
        n: 7,
        notebook: "Reading",
        title: "Articles to read",
        tags: &["reading", "ideas"],
        pinned: false,
        age: 35 * DAY,
        body: "- [Local-first software](https://www.inkandswitch.com/local-first/)\n- [CommonMark spec](https://spec.commonmark.org/)\n- ~~The old wiki page~~ (moved)\n",
    },
    Sample {
        n: 8,
        notebook: "Personal",
        title: "Garden plan",
        tags: &["ideas"],
        pinned: false,
        age: 60 * DAY,
        body: "Two beds by the fence, herbs by the door.\n\n---\n\nOrder seeds before **March**.\n",
    },
];

/// The sample library as `(key, file text)` pairs, dated relative to `now`,
/// with an empty `Archive` notebook.
#[must_use]
pub fn sample_files(now: u64) -> Vec<(String, String)> {
    let mut files: Vec<(String, String)> = SAMPLES
        .iter()
        .map(|s| {
            let modified = now.saturating_sub(s.age);
            let meta = Meta {
                title: s.title.to_string(),
                tags: s.tags.iter().map(|t| (*t).to_string()).collect(),
                pinned: s.pinned,
                created: modified.saturating_sub(DAY),
                modified,
                extra: Vec::new(),
            };
            let doc = markdown::markdown_to_doc(s.body);
            (model::note_key(s.notebook, &sample_id(s.n)), markdown::note_to_file(&meta, &doc))
        })
        .collect();
    files.push((model::marker_key("Archive"), String::new()));
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Library, Note};

    #[test]
    fn every_sample_file_reads_back_as_a_note_and_the_library_is_filled() {
        let now = 1_790_769_600;
        let mut lib = Library::default();
        for (key, text) in sample_files(now) {
            if let Some(notebook) = model::parse_marker_key(&key) {
                lib.notebooks.insert(notebook);
                continue;
            }
            let note = Note::from_file(&key, &text, 0).expect("a note key");
            assert!(!note.meta.title.is_empty(), "{key}");
            assert_eq!(note.to_file(), text, "{key}: the file is canonical");
            lib.upsert(note);
        }
        assert_eq!(lib.notes.len(), SAMPLES.len());
        assert_eq!(lib.counts().pinned, 2);
        assert!(lib.notebook_paths().contains(&"Archive".to_string()));
        assert!(lib.notebook_paths().contains(&"Work/Offsite".to_string()));
        let offsite = lib.get(&sample_id(1)).expect("the offsite note");
        assert_eq!(offsite.doc.checklist(), (1, 3));
    }
}
