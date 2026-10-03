//! The sample machine (`--sample`): a deterministic system for screenshots,
//! demos and the E2E script - the machine of the planning doc (s6): 8
//! cores, 16 GB of memory, one disk, one network, and its processes: AzWriter
//! (and its pagination worker), AzMail (3), AzFiles, cargo with 12 rustc
//! children, rust-analyzer (2), pipewire, systemd, sshd, Xwayland and 20 idle
//! system processes; users `user` and `root`.
//!
//! Every reading is a function of the reading's number: no randomness, so
//! two runs show the same numbers. The load MOVES (each busy process follows
//! its own slow wave), so the charts scroll and the rows re-sort like a live
//! machine's. Ending a process removes it from the next reading - nothing is
//! killed; a root process needs administrator rights, as on a real machine.

use crate::model::{ProcSample, Snapshot, Source};

/// The sample machine's cores.
pub const CORES: usize = 8;
/// Its memory, bytes.
pub const MEMORY: u64 = 16 << 30;
/// Its swap, bytes.
pub const SWAP: u64 = 2 << 30;
/// Memory the kernel and the caches hold, beyond the processes.
const BASE_MEMORY: u64 = (5 << 30) / 2;
/// Its uptime at the first reading: 2 d 04:13:00.
const UPTIME: u64 = 2 * 86_400 + 4 * 3600 + 13 * 60;

/// How a sample process uses the machine.
#[derive(Debug, Clone, PartialEq)]
struct Profile {
    pid: u32,
    parent: Option<u32>,
    name: &'static str,
    user: &'static str,
    command: &'static str,
    /// CPU at rest, percent of one core.
    cpu: f32,
    /// How far the wave lifts it above `cpu`, percent of one core.
    swing: f32,
    /// The wave's period, in readings.
    period: f32,
    /// Resident memory, bytes.
    memory: u64,
    /// Disk traffic at the wave's top, bytes per second.
    disk: u64,
}

/// The sample machine.
#[derive(Debug, Clone)]
pub struct SampleMachine {
    processes: Vec<Profile>,
    /// Readings so far.
    reading: u64,
    /// Milliseconds since the first reading.
    clock_ms: u64,
}

impl Default for SampleMachine {
    fn default() -> Self {
        Self::new()
    }
}

/// A process of the sample: `memory` in MB, `disk` in KB per second.
#[allow(clippy::too_many_arguments)]
const fn profile(
    pid: u32,
    parent: Option<u32>,
    name: &'static str,
    user: &'static str,
    command: &'static str,
    cpu: f32,
    swing: f32,
    period: f32,
    memory_mb: u64,
    disk_kb: u64,
) -> Profile {
    Profile {
        pid,
        parent,
        name,
        user,
        command,
        cpu,
        swing,
        period,
        memory: memory_mb << 20,
        disk: disk_kb << 10,
    }
}

/// The 20 idle system processes: name and PID (root, children of systemd).
const IDLE: [(&str, u32); 20] = [
    ("kthreadd", 2),
    ("rcu_sched", 13),
    ("ksoftirqd/0", 14),
    ("migration/0", 15),
    ("kworker/0:1", 41),
    ("systemd-journald", 312),
    ("systemd-udevd", 344),
    ("systemd-logind", 601),
    ("dbus-daemon", 603),
    ("NetworkManager", 640),
    ("polkitd", 655),
    ("avahi-daemon", 661),
    ("cupsd", 690),
    ("cron", 694),
    ("rsyslogd", 698),
    ("wpa_supplicant", 720),
    ("bluetoothd", 731),
    ("upowerd", 1022),
    ("accounts-daemon", 1030),
    ("udisksd", 1041),
];

/// The PID of cargo, the parent of the 12 rustc.
const CARGO: u32 = 5102;

/// 0 at the wave's foot, 1 at its top: reading `reading` of a wave of
/// `period` readings, started `phase` readings early.
#[allow(clippy::cast_precision_loss)] // reading counts far below 2^24
fn wave(reading: u64, phase: u32, period: f32) -> f32 {
    let period = period.max(1.0);
    let t = (reading as f32 + phase as f32 % period) / period;
    (1.0 - (t * core::f32::consts::TAU).cos()) / 2.0
}

/// Bytes counted over `elapsed_ms` at `rate` bytes per second.
fn counted(rate: u64, elapsed_ms: u64) -> u64 {
    rate.saturating_mul(elapsed_ms) / 1000
}

impl SampleMachine {
    /// The machine of the planning doc, before its first reading.
    #[must_use]
    pub fn new() -> Self {
        let mut processes = vec![
            profile(
                1,
                None,
                "systemd",
                "root",
                "/sbin/init splash",
                0.1,
                0.2,
                31.0,
                14,
                0,
            ),
            profile(
                702,
                Some(1),
                "sshd",
                "root",
                "/usr/sbin/sshd -D",
                0.0,
                0.1,
                37.0,
                8,
                0,
            ),
            profile(
                812,
                Some(1),
                "pipewire",
                "user",
                "/usr/bin/pipewire",
                0.6,
                0.4,
                5.0,
                24,
                0,
            ),
            profile(
                1204,
                Some(1),
                "Xwayland",
                "user",
                "/usr/bin/Xwayland :0 -rootless",
                2.0,
                3.0,
                13.0,
                180,
                4,
            ),
            profile(
                2288,
                Some(1),
                "rust-analyzer",
                "user",
                "rust-analyzer",
                8.0,
                20.0,
                29.0,
                1800,
                400,
            ),
            profile(
                2291,
                Some(1),
                "rust-analyzer",
                "user",
                "rust-analyzer",
                1.0,
                4.0,
                41.0,
                600,
                40,
            ),
            profile(
                3310,
                Some(1),
                "AzFiles",
                "user",
                "AzFiles",
                0.3,
                0.5,
                23.0,
                120,
                8,
            ),
            profile(
                3920,
                Some(1),
                "AzMail",
                "user",
                "AzMail",
                0.4,
                1.5,
                19.0,
                301,
                16,
            ),
            profile(
                3921,
                Some(3920),
                "azmail-sync",
                "user",
                "AzMail --sync",
                0.2,
                3.0,
                11.0,
                80,
                120,
            ),
            profile(
                3922,
                Some(3920),
                "azmail-index",
                "user",
                "AzMail --index",
                0.1,
                1.0,
                43.0,
                60,
                60,
            ),
            profile(
                4411,
                Some(1),
                "AzWriter",
                "user",
                "AzWriter report.docx",
                3.0,
                4.0,
                15.0,
                212,
                12,
            ),
            profile(
                4415,
                Some(4411),
                "azwriter-pagination",
                "user",
                "AzWriter --paginate",
                1.0,
                2.0,
                9.0,
                88,
                0,
            ),
            profile(
                CARGO,
                Some(1),
                "cargo",
                "user",
                "cargo build --release -p AzWriter",
                20.0,
                60.0,
                17.0,
                940,
                2300,
            ),
        ];
        for i in 0..12_u16 {
            let n = u32::from(i);
            processes.push(profile(
                CARGO + 8 + n,
                Some(CARGO),
                "rustc",
                "user",
                "rustc --crate-type lib --edition 2021",
                5.0,
                35.0,
                9.0 + f32::from(i),
                300,
                600,
            ));
        }
        for (name, pid) in IDLE {
            processes.push(profile(
                pid,
                Some(1),
                name,
                "root",
                name,
                0.0,
                0.2,
                27.0,
                6,
                0,
            ));
        }
        Self {
            processes,
            reading: 0,
            clock_ms: 0,
        }
    }
}

impl Source for SampleMachine {
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    fn read(&mut self, elapsed_ms: u64) -> Snapshot {
        let n = self.reading;
        self.reading += 1;
        self.clock_ms = self.clock_ms.saturating_add(elapsed_ms);
        let mut used_cpu = 0.0_f32;
        let mut memory_used = BASE_MEMORY;
        let mut disk_read = 0_u64;
        let mut disk_written = 0_u64;
        let processes: Vec<ProcSample> = self
            .processes
            .iter()
            .map(|p| {
                let w = wave(n, p.pid, p.period);
                let cpu = p.cpu + p.swing * w;
                let memory = p.memory + (p.memory as f32 / 32.0 * w) as u64;
                let disk_rate = (p.disk as f32 * w) as u64;
                let read = counted(disk_rate * 2 / 3, elapsed_ms);
                let written = counted(disk_rate / 3, elapsed_ms);
                used_cpu += cpu;
                memory_used += memory;
                disk_read += read;
                disk_written += written;
                ProcSample {
                    pid: p.pid,
                    parent: p.parent,
                    name: p.name.to_string(),
                    user: p.user.to_string(),
                    command: p.command.to_string(),
                    status: if cpu > 1.0 { "Running" } else { "Sleeping" }.to_string(),
                    cpu,
                    memory,
                    disk_read: read,
                    disk_written: written,
                }
            })
            .collect();
        let cpu = (used_cpu / CORES as f32).clamp(0.0, 100.0);
        // Each core carries the machine's load, unevenly, on its own wave.
        let cores = (0..CORES)
            .map(|i| {
                let phase = u32::try_from(i * 3).unwrap_or(0);
                (cpu * (0.6 + 0.8 * wave(n, phase, 7.0 + i as f32))).clamp(0.0, 100.0)
            })
            .collect();
        let net_in_rate = 150_000 + (900_000.0 * wave(n, 0, 13.0)) as u64;
        let net_out_rate = 20_000 + (60_000.0 * wave(n, 5, 19.0)) as u64;
        Snapshot {
            elapsed_ms,
            cpu_brand: "Example 8-core CPU".to_string(),
            cpu_mhz: 3400,
            cpu,
            cores,
            memory_used: memory_used.min(MEMORY),
            memory_total: MEMORY,
            swap_used: 300 << 20,
            swap_total: SWAP,
            disk_read,
            disk_written,
            net_received: counted(net_in_rate, elapsed_ms),
            net_sent: counted(net_out_rate, elapsed_ms),
            uptime: UPTIME + self.clock_ms / 1000,
            processes,
            notices: Vec::new(),
        }
    }

    fn end(&mut self, pid: u32, force: bool) -> Result<String, String> {
        let Some(at) = self.processes.iter().position(|p| p.pid == pid) else {
            return Err(format!("No process {pid} is running."));
        };
        let name = self.processes[at].name;
        if self.processes[at].user == "root" {
            return Err(format!("Ending {name} ({pid}) needs administrator rights."));
        }
        self.processes.remove(at);
        Ok(if force {
            format!("Killed {name} ({pid})")
        } else {
            format!("Ended {name} ({pid})")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named<'a>(s: &'a Snapshot, name: &str) -> Vec<&'a ProcSample> {
        s.processes.iter().filter(|p| p.name == name).collect()
    }

    #[test]
    fn the_sample_machine_has_eight_cores_sixteen_gigabytes_and_an_uptime() {
        let s = SampleMachine::new().read(0);
        assert_eq!(s.cores.len(), CORES);
        assert_eq!(s.memory_total, MEMORY);
        assert_eq!(s.swap_total, SWAP);
        assert!(s.memory_used > BASE_MEMORY && s.memory_used < MEMORY);
        assert_eq!(s.uptime, UPTIME);
        assert!(!s.cpu_brand.is_empty());
        assert!(s.cpu_mhz > 0);
    }

    #[test]
    fn the_sample_machine_runs_the_planned_processes() {
        let s = SampleMachine::new().read(0);
        assert_eq!(s.processes.len(), 45);
        let systemd = named(&s, "systemd");
        assert_eq!(systemd.len(), 1);
        assert_eq!(systemd[0].pid, 1);
        assert_eq!(systemd[0].user, "root");
        let cargo = named(&s, "cargo");
        assert_eq!(cargo.len(), 1);
        let rustc = named(&s, "rustc");
        assert_eq!(rustc.len(), 12);
        assert!(rustc.iter().all(|r| r.parent == Some(cargo[0].pid)));
        assert_eq!(named(&s, "rust-analyzer").len(), 2);
        assert!(s
            .processes
            .iter()
            .all(|p| p.user == "user" || p.user == "root"));
        // Every PID once.
        let mut pids: Vec<u32> = s.processes.iter().map(|p| p.pid).collect();
        pids.sort_unstable();
        pids.dedup();
        assert_eq!(pids.len(), 45);
    }

    #[test]
    fn two_sample_machines_read_the_same() {
        let mut a = SampleMachine::new();
        let mut b = SampleMachine::new();
        for _ in 0..5 {
            assert_eq!(a.read(1000), b.read(1000));
        }
    }

    #[test]
    fn the_load_moves_from_one_reading_to_the_next() {
        let mut m = SampleMachine::new();
        let first = m.read(0);
        let second = m.read(1000);
        let cpu = |s: &Snapshot| named(s, "cargo")[0].cpu;
        assert!((cpu(&first) - cpu(&second)).abs() > 0.01);
        assert!((first.cpu - second.cpu).abs() > 0.001);
    }

    #[test]
    fn the_machine_cpu_is_what_its_processes_use() {
        let mut m = SampleMachine::new();
        for _ in 0..3 {
            let s = m.read(1000);
            let used: f32 = s.processes.iter().map(|p| p.cpu).sum();
            let expected = (used / CORES as f32).clamp(0.0, 100.0);
            assert!((s.cpu - expected).abs() < 0.01, "{} vs {expected}", s.cpu);
            assert!(s.cores.iter().all(|c| (0.0..=100.0).contains(c)));
        }
    }

    #[test]
    fn a_reading_counts_its_traffic_over_the_elapsed_time() {
        let one = SampleMachine::new().read(1000);
        let two = SampleMachine::new().read(2000);
        assert!(one.disk_read > 0 && one.net_received > 0 && one.net_sent > 0);
        assert_eq!(two.disk_read, one.disk_read * 2);
        assert_eq!(two.disk_written, one.disk_written * 2);
        assert_eq!(two.net_received, one.net_received * 2);
        assert_eq!(two.net_sent, one.net_sent * 2);
        // The first reading of all covers no time: no traffic yet.
        let first = SampleMachine::new().read(0);
        assert_eq!(first.disk_read + first.net_received, 0);
    }

    #[test]
    fn the_uptime_runs_with_the_readings() {
        let mut m = SampleMachine::new();
        m.read(0);
        m.read(1000);
        let s = m.read(1000);
        assert_eq!(s.uptime, UPTIME + 2);
    }

    #[test]
    fn ending_a_user_process_removes_it_from_the_next_reading() {
        let mut m = SampleMachine::new();
        let s = m.read(0);
        let pipewire = named(&s, "pipewire")[0].pid;
        let done = m.end(pipewire, false);
        assert_eq!(done, Ok(format!("Ended pipewire ({pipewire})")));
        let next = m.read(1000);
        assert!(named(&next, "pipewire").is_empty());
        assert_eq!(next.processes.len(), 44);
        // Killing says so.
        let xwayland = named(&next, "Xwayland")[0].pid;
        assert_eq!(
            m.end(xwayland, true),
            Ok(format!("Killed Xwayland ({xwayland})"))
        );
    }

    #[test]
    fn ending_a_root_process_needs_administrator_rights() {
        let mut m = SampleMachine::new();
        let s = m.read(0);
        let sshd = named(&s, "sshd")[0].pid;
        let refused = m.end(sshd, true);
        assert!(
            refused
                .as_ref()
                .is_err_and(|why| why.contains("administrator")),
            "{refused:?}"
        );
        let next = m.read(1000);
        assert_eq!(named(&next, "sshd").len(), 1);
    }

    #[test]
    fn ending_a_process_that_is_gone_says_so() {
        let mut m = SampleMachine::new();
        m.read(0);
        let gone = m.end(999_999, false);
        assert!(gone.is_err_and(|why| why.contains("999999")));
    }
}
