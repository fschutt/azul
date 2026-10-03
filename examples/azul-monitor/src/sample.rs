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

impl SampleMachine {
    /// The machine of the planning doc, before its first reading.
    #[must_use]
    pub fn new() -> Self {
        todo!("GREEN: SampleMachine::new")
    }
}

impl Source for SampleMachine {
    fn read(&mut self, elapsed_ms: u64) -> Snapshot {
        let _ = elapsed_ms;
        todo!("GREEN: SampleMachine::read")
    }

    fn end(&mut self, pid: u32, force: bool) -> Result<String, String> {
        let _ = (pid, force);
        todo!("GREEN: SampleMachine::end")
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
