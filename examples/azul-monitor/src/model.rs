//! The sampling model: what one reading of the system holds ([`Snapshot`],
//! made by a sampler on a Thread), and what the window shows of the
//! readings so far ([`Model`]): the history of every measure, the process
//! rows in the sort order, the filter, the selected process.
//!
//! Pure Rust, no azul types: the sampler (live or sample) fills snapshots,
//! the UI reads the model. Every number a reading carries is what the OS
//! counted SINCE THE PREVIOUS READING (bytes read, bytes received); the
//! model turns them into rates with the reading's `elapsed_ms`, so a late
//! tick does not show as a spike.
//!
//! CPU: the machine's and each core's usage are percents of that CPU
//! (0..100). A process' CPU arrives as sysinfo counts it - a percent of ONE
//! core (a busy 8-thread build reads 800) - and is shown as its share of the
//! whole machine (Task Manager's rule: the column adds up to the CPU total).

use std::cmp::Ordering;

use crate::history::History;

/// Readings kept for the charts (one a second: a minute).
pub const CHART_READINGS: usize = 60;

/// One process in one reading.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProcSample {
    /// The process id.
    pub pid: u32,
    /// The parent's id, if known.
    pub parent: Option<u32>,
    /// The process' name ("cargo").
    pub name: String,
    /// The user it runs as ("" = unknown).
    pub user: String,
    /// Its command line, joined by spaces ("" = unknown).
    pub command: String,
    /// "Running", "Sleeping", ... ("" = unknown).
    pub status: String,
    /// CPU in percent of ONE core (can exceed 100 on several cores).
    pub cpu: f32,
    /// Resident memory in bytes.
    pub memory: u64,
    /// Bytes it read since the previous reading.
    pub disk_read: u64,
    /// Bytes it wrote since the previous reading.
    pub disk_written: u64,
}

/// One reading of the system, as a sampler hands it to the window.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Snapshot {
    /// Milliseconds since the previous reading: the span every "since the
    /// previous reading" count covers (0 for the first reading).
    pub elapsed_ms: u64,
    /// The CPU's name ("Example 8-core CPU").
    pub cpu_brand: String,
    /// The CPU's clock in MHz (0 = unknown).
    pub cpu_mhz: u64,
    /// The whole CPU's usage, percent.
    pub cpu: f32,
    /// Each core's usage, percent.
    pub cores: Vec<f32>,
    /// Memory in use, bytes.
    pub memory_used: u64,
    /// All memory, bytes.
    pub memory_total: u64,
    /// Swap in use, bytes.
    pub swap_used: u64,
    /// All swap, bytes.
    pub swap_total: u64,
    /// Bytes read from the disks since the previous reading.
    pub disk_read: u64,
    /// Bytes written to the disks since the previous reading.
    pub disk_written: u64,
    /// Bytes received over the network since the previous reading.
    pub net_received: u64,
    /// Bytes sent over the network since the previous reading.
    pub net_sent: u64,
    /// Seconds since the machine started.
    pub uptime: u64,
    /// The processes.
    pub processes: Vec<ProcSample>,
    /// What the sampler did on the window's behalf since the previous
    /// reading ("Ended cargo (5102)", or why it could not).
    pub notices: Vec<String>,
}

/// `bytes` counted over `elapsed_ms`, per second (0 when no time passed).
#[must_use]
#[allow(clippy::cast_precision_loss)] // byte counts far below 2^52
pub fn per_second(bytes: u64, elapsed_ms: u64) -> f64 {
    if elapsed_ms == 0 {
        return 0.0;
    }
    bytes as f64 * 1000.0 / elapsed_ms as f64
}

/// A process' CPU (percent of one core) as its share of a machine of
/// `cores` cores, 0..100.
#[must_use]
#[allow(clippy::cast_precision_loss)] // a core count
pub fn share_of_machine(cpu_one_core: f32, cores: usize) -> f32 {
    let share = cpu_one_core / cores.max(1) as f32;
    if share.is_nan() {
        return 0.0;
    }
    share.clamp(0.0, 100.0)
}

/// `used` of `total` in percent (0 when there is no total).
#[must_use]
#[allow(clippy::cast_precision_loss)] // byte counts far below 2^52
pub fn percent_of(used: u64, total: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    used as f64 * 100.0 / total as f64
}

/// A percentage as the table writes it: "3.2 %" (one decimal; "0 %" for
/// nothing).
#[must_use]
pub fn format_percent(percent: f64) -> String {
    if !(percent > 0.0) {
        // Nothing (or NaN, which a broken reading can carry).
        return "0 %".to_string();
    }
    if percent >= 99.95 {
        return "100 %".to_string();
    }
    format!("{percent:.1} %")
}

/// Seconds as an uptime: "2 d 04:13:05", "04:13:05".
#[must_use]
pub fn format_uptime(seconds: u64) -> String {
    let days = seconds / 86_400;
    let clock = format!(
        "{:02}:{:02}:{:02}",
        (seconds % 86_400) / 3600,
        (seconds % 3600) / 60,
        seconds % 60
    );
    if days == 0 {
        clock
    } else {
        format!("{days} d {clock}")
    }
}

// ---- the process rows ----

/// A column of the process table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Column {
    Name,
    Pid,
    User,
    Cpu,
    Memory,
    Disk,
    Status,
}

/// The columns, in the table's order.
pub const COLUMNS: [Column; 7] = [
    Column::Name,
    Column::Pid,
    Column::User,
    Column::Cpu,
    Column::Memory,
    Column::Disk,
    Column::Status,
];

impl Column {
    /// The header's title.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::Pid => "PID",
            Self::User => "User",
            Self::Cpu => "CPU",
            Self::Memory => "Memory",
            Self::Disk => "Disk",
            Self::Status => "Status",
        }
    }

    /// Whether it sorts by a number (else by the folded text).
    #[must_use]
    pub const fn is_number(self) -> bool {
        matches!(self, Self::Pid | Self::Cpu | Self::Memory | Self::Disk)
    }

    /// Its width in px.
    #[must_use]
    pub const fn width(self) -> f32 {
        match self {
            Self::Name => 240.0,
            Self::Pid => 80.0,
            Self::User => 110.0,
            Self::Cpu => 80.0,
            Self::Memory => 100.0,
            Self::Disk => 100.0,
            Self::Status => 100.0,
        }
    }

    /// Its index in [`COLUMNS`].
    #[must_use]
    pub fn index(self) -> usize {
        COLUMNS.iter().position(|c| *c == self).unwrap_or(0)
    }

    /// The column at `index` in [`COLUMNS`].
    #[must_use]
    pub fn at(index: usize) -> Option<Self> {
        COLUMNS.get(index).copied()
    }
}

/// One process as the table shows it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProcRow {
    pub pid: u32,
    pub parent: Option<u32>,
    pub name: String,
    pub user: String,
    pub command: String,
    pub status: String,
    /// Share of the whole machine's CPU, percent (0..100).
    pub cpu: f32,
    /// Resident memory, bytes.
    pub memory: u64,
    /// Bytes read and written per second.
    pub disk_rate: f64,
}

impl ProcRow {
    /// The row of `sample` in a reading over `elapsed_ms` on `cores` cores.
    #[must_use]
    pub fn of(sample: &ProcSample, elapsed_ms: u64, cores: usize) -> Self {
        Self {
            pid: sample.pid,
            parent: sample.parent,
            name: sample.name.clone(),
            user: sample.user.clone(),
            command: sample.command.clone(),
            status: sample.status.clone(),
            cpu: share_of_machine(sample.cpu, cores),
            memory: sample.memory,
            disk_rate: per_second(
                sample.disk_read.saturating_add(sample.disk_written),
                elapsed_ms,
            ),
        }
    }

    /// The number a number column sorts by (NaN for a text column).
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // memory far below 2^52
    pub fn number(&self, column: Column) -> f64 {
        match column {
            Column::Pid => f64::from(self.pid),
            Column::Cpu => f64::from(self.cpu),
            Column::Memory => self.memory as f64,
            Column::Disk => self.disk_rate,
            Column::Name | Column::User | Column::Status => f64::NAN,
        }
    }

    /// The text a text column sorts by ("" for a number column).
    #[must_use]
    pub fn text(&self, column: Column) -> &str {
        match column {
            Column::Name => &self.name,
            Column::User => &self.user,
            Column::Status => &self.status,
            Column::Pid | Column::Cpu | Column::Memory | Column::Disk => "",
        }
    }
}

/// Two numbers, blanks (NaN) last whichever way the key runs.
fn compare_numbers(a: f64, b: f64, descending: bool) -> Ordering {
    match (a.is_nan(), b.is_nan()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => {
            let o = a.partial_cmp(&b).unwrap_or(Ordering::Equal);
            if descending {
                o.reverse()
            } else {
                o
            }
        }
    }
}

/// Two texts, case folded, blanks last whichever way the key runs.
fn compare_texts(a: &str, b: &str, descending: bool) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => {
            let o = a.to_lowercase().cmp(&b.to_lowercase());
            if descending {
                o.reverse()
            } else {
                o
            }
        }
    }
}

/// One sort key: a column and its direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SortKey {
    pub column: Column,
    pub descending: bool,
}

impl SortKey {
    #[must_use]
    pub const fn new(column: Column, descending: bool) -> Self {
        Self { column, descending }
    }
}

/// The sort a fresh window shows: the busiest process first.
pub const DEFAULT_SORT: [SortKey; 1] = [SortKey::new(Column::Cpu, true)];

/// Two rows by `keys` in turn (texts case folded, blanks last either way),
/// then by PID: two readings of the same processes always sort the same.
#[must_use]
pub fn compare_rows(a: &ProcRow, b: &ProcRow, keys: &[SortKey]) -> Ordering {
    for key in keys {
        let o = if key.column.is_number() {
            compare_numbers(a.number(key.column), b.number(key.column), key.descending)
        } else {
            compare_texts(a.text(key.column), b.text(key.column), key.descending)
        };
        if o != Ordering::Equal {
            return o;
        }
    }
    a.pid.cmp(&b.pid)
}

/// Sorts `rows` by `keys` (see [`compare_rows`]).
pub fn sort_rows(rows: &mut [ProcRow], keys: &[SortKey]) {
    rows.sort_by(|a, b| compare_rows(a, b, keys));
}

/// Whether `row` passes the filter `query`: its name or user contains it
/// (case folded), or its PID starts with it. An empty query passes all.
#[must_use]
pub fn matches(row: &ProcRow, query: &str) -> bool {
    let query = query.trim();
    if query.is_empty() {
        return true;
    }
    let folded = query.to_lowercase();
    row.name.to_lowercase().contains(&folded)
        || row.user.to_lowercase().contains(&folded)
        || row.pid.to_string().starts_with(query)
}

// ---- the model ----

/// The latest machine-wide figures (the cards, the status bar, the stats).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Summary {
    pub cpu_brand: String,
    pub cpu_mhz: u64,
    pub cpu: f64,
    pub memory_used: u64,
    pub memory_total: u64,
    pub swap_used: u64,
    pub swap_total: u64,
    /// Bytes read per second.
    pub disk_read_rate: f64,
    /// Bytes written per second.
    pub disk_write_rate: f64,
    /// Bytes received per second.
    pub net_in_rate: f64,
    /// Bytes sent per second.
    pub net_out_rate: f64,
    pub uptime: u64,
    pub processes: usize,
}

/// Everything the window shows of the readings so far.
#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    /// The whole CPU, percent.
    pub cpu: History,
    /// Each core, percent.
    pub cores: Vec<History>,
    /// Memory in use, percent.
    pub memory: History,
    /// Disk reads, bytes per second.
    pub disk_read: History,
    /// Disk writes, bytes per second.
    pub disk_write: History,
    /// Network in, bytes per second.
    pub net_in: History,
    /// Network out, bytes per second.
    pub net_out: History,
    /// The latest machine-wide figures.
    pub summary: Summary,
    /// Every process of the latest reading, in the sort order.
    rows: Vec<ProcRow>,
    /// The rows that pass the filter, in the sort order: indices into
    /// `rows`. The table's rows ARE these positions.
    shown: Vec<usize>,
    /// The sort keys (empty = by PID).
    sort: Vec<SortKey>,
    /// The filter as typed.
    filter: String,
    /// The selected process (kept across readings while it lives).
    selected: Option<u32>,
    /// How many readings arrived.
    pub readings: u64,
    /// The sampler's latest notices (shown in the status bar).
    pub notices: Vec<String>,
}

impl Default for Model {
    fn default() -> Self {
        Self::new()
    }
}

impl Model {
    /// No readings yet, sorted by [`DEFAULT_SORT`], no filter.
    #[must_use]
    pub fn new() -> Self {
        Self {
            cpu: History::new(CHART_READINGS),
            cores: Vec::new(),
            memory: History::new(CHART_READINGS),
            disk_read: History::new(CHART_READINGS),
            disk_write: History::new(CHART_READINGS),
            net_in: History::new(CHART_READINGS),
            net_out: History::new(CHART_READINGS),
            summary: Summary::default(),
            rows: Vec::new(),
            shown: Vec::new(),
            sort: DEFAULT_SORT.to_vec(),
            filter: String::new(),
            selected: None,
            readings: 0,
            notices: Vec::new(),
        }
    }

    /// Takes a reading: the histories grow by one, the rows are the
    /// reading's processes in the sort order, the filter applies, and the
    /// selection stays on its process while it lives.
    pub fn apply(&mut self, snapshot: Snapshot) {
        let s = snapshot;
        let disk_read_rate = per_second(s.disk_read, s.elapsed_ms);
        let disk_write_rate = per_second(s.disk_written, s.elapsed_ms);
        let net_in_rate = per_second(s.net_received, s.elapsed_ms);
        let net_out_rate = per_second(s.net_sent, s.elapsed_ms);
        let memory_percent = percent_of(s.memory_used, s.memory_total);

        self.cpu.push(f64::from(s.cpu));
        while self.cores.len() < s.cores.len() {
            self.cores.push(History::new(CHART_READINGS));
        }
        for (history, usage) in self.cores.iter_mut().zip(s.cores.iter()) {
            history.push(f64::from(*usage));
        }
        self.memory.push(memory_percent);
        self.disk_read.push(disk_read_rate);
        self.disk_write.push(disk_write_rate);
        self.net_in.push(net_in_rate);
        self.net_out.push(net_out_rate);

        let cores = s.cores.len();
        self.rows = s
            .processes
            .iter()
            .map(|p| ProcRow::of(p, s.elapsed_ms, cores))
            .collect();
        self.summary = Summary {
            cpu_brand: s.cpu_brand,
            cpu_mhz: s.cpu_mhz,
            cpu: f64::from(s.cpu),
            memory_used: s.memory_used,
            memory_total: s.memory_total,
            swap_used: s.swap_used,
            swap_total: s.swap_total,
            disk_read_rate,
            disk_write_rate,
            net_in_rate,
            net_out_rate,
            uptime: s.uptime,
            processes: self.rows.len(),
        };
        if !s.notices.is_empty() {
            self.notices = s.notices;
        }
        // The selection ends with its process.
        if let Some(pid) = self.selected {
            if !self.rows.iter().any(|r| r.pid == pid) {
                self.selected = None;
            }
        }
        self.readings += 1;
        self.reorder();
    }

    /// The rows in the sort order, then the ones the filter shows.
    fn reorder(&mut self) {
        sort_rows(&mut self.rows, &self.sort);
        let filter = self.filter.as_str();
        self.shown = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| matches(r, filter))
            .map(|(i, _)| i)
            .collect();
    }

    /// Sorts by `keys` (empty = by PID).
    pub fn set_sort(&mut self, keys: Vec<SortKey>) {
        self.sort = keys;
        self.reorder();
    }

    /// The sort keys.
    #[must_use]
    pub fn sort(&self) -> &[SortKey] {
        &self.sort
    }

    /// Filters by `query` (see [`matches`]).
    pub fn set_filter(&mut self, query: &str) {
        query.clone_into(&mut self.filter);
        self.reorder();
    }

    /// The filter as typed.
    #[must_use]
    pub fn filter(&self) -> &str {
        &self.filter
    }

    /// Every process of the latest reading.
    #[must_use]
    pub fn process_count(&self) -> usize {
        self.rows.len()
    }

    /// How many rows pass the filter.
    #[must_use]
    pub fn shown_count(&self) -> usize {
        self.shown.len()
    }

    /// The row at `position` among the rows shown.
    #[must_use]
    pub fn shown_row(&self, position: usize) -> Option<&ProcRow> {
        self.shown.get(position).and_then(|i| self.rows.get(*i))
    }

    /// Where process `pid` is among the rows shown.
    #[must_use]
    pub fn position_of(&self, pid: u32) -> Option<usize> {
        self.shown
            .iter()
            .position(|i| self.rows.get(*i).is_some_and(|r| r.pid == pid))
    }

    /// Selects the process shown at `position` (`None`: nothing).
    pub fn select_position(&mut self, position: Option<usize>) {
        self.selected = position.and_then(|p| self.shown_row(p)).map(|r| r.pid);
    }

    /// The selected process' id.
    #[must_use]
    pub fn selected(&self) -> Option<u32> {
        self.selected
    }

    /// The selected process' row, if it is shown.
    #[must_use]
    pub fn selected_row(&self) -> Option<&ProcRow> {
        self.selected_position().and_then(|p| self.shown_row(p))
    }

    /// The selected process' position among the rows shown.
    #[must_use]
    pub fn selected_position(&self) -> Option<usize> {
        self.selected.and_then(|pid| self.position_of(pid))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(pid: u32, name: &str, user: &str, cpu: f32, memory: u64) -> ProcSample {
        ProcSample {
            pid,
            name: name.to_string(),
            user: user.to_string(),
            cpu,
            memory,
            ..ProcSample::default()
        }
    }

    fn reading(elapsed_ms: u64, processes: Vec<ProcSample>) -> Snapshot {
        Snapshot {
            elapsed_ms,
            cpu: 25.0,
            cores: vec![10.0, 20.0, 30.0, 40.0],
            memory_used: 4 << 30,
            memory_total: 16 << 30,
            processes,
            ..Snapshot::default()
        }
    }

    fn shown_pids(m: &Model) -> Vec<u32> {
        (0..m.shown_count())
            .filter_map(|p| m.shown_row(p))
            .map(|r| r.pid)
            .collect()
    }

    // ---- rates and units ----

    #[test]
    fn a_count_over_half_a_second_is_twice_that_per_second() {
        assert_eq!(per_second(1000, 500), 2000.0);
        assert_eq!(per_second(4096, 1000), 4096.0);
    }

    #[test]
    fn a_count_over_no_time_is_no_rate() {
        assert_eq!(per_second(123_456, 0), 0.0);
    }

    #[test]
    fn a_process_cpu_is_its_share_of_the_whole_machine() {
        // 800 % of one core on an 8-core machine is the whole machine.
        assert_eq!(share_of_machine(800.0, 8), 100.0);
        assert_eq!(share_of_machine(50.0, 4), 12.5);
        // Never more than the machine, never less than nothing.
        assert_eq!(share_of_machine(900.0, 8), 100.0);
        assert_eq!(share_of_machine(-3.0, 8), 0.0);
        // A machine that reports no cores counts as one.
        assert_eq!(share_of_machine(40.0, 0), 40.0);
    }

    #[test]
    fn memory_in_use_is_a_percent_of_all_memory() {
        assert_eq!(percent_of(4, 16), 25.0);
        assert_eq!(percent_of(5, 0), 0.0);
    }

    #[test]
    fn percents_have_one_decimal() {
        assert_eq!(format_percent(3.24), "3.2 %");
        assert_eq!(format_percent(71.0), "71.0 %");
        assert_eq!(format_percent(0.0), "0 %");
        assert_eq!(format_percent(100.0), "100 %");
    }

    #[test]
    fn an_uptime_reads_as_days_and_a_clock() {
        assert_eq!(format_uptime(59), "00:00:59");
        assert_eq!(format_uptime(4 * 3600 + 13 * 60 + 5), "04:13:05");
        assert_eq!(
            format_uptime(2 * 86_400 + 4 * 3600 + 13 * 60),
            "2 d 04:13:00"
        );
    }

    // ---- the rows ----

    #[test]
    fn a_row_shows_the_share_of_the_machine_and_the_disk_rate() {
        let mut s = proc(7, "cargo", "user", 400.0, 940 << 20);
        s.disk_read = 1500;
        s.disk_written = 500;
        let row = ProcRow::of(&s, 500, 8);
        assert_eq!(row.pid, 7);
        assert_eq!(row.cpu, 50.0);
        assert_eq!(row.memory, 940 << 20);
        assert_eq!(row.disk_rate, 4000.0);
    }

    #[test]
    fn number_columns_sort_by_numbers_and_text_columns_by_text() {
        let row = ProcRow::of(&proc(42, "Xorg", "root", 8.0, 1000), 1000, 4);
        assert_eq!(row.number(Column::Pid), 42.0);
        assert_eq!(row.number(Column::Cpu), 2.0);
        assert_eq!(row.number(Column::Memory), 1000.0);
        assert!(row.number(Column::Name).is_nan());
        assert_eq!(row.text(Column::Name), "Xorg");
        assert_eq!(row.text(Column::User), "root");
        assert_eq!(row.text(Column::Cpu), "");
    }

    // ---- sorting ----

    fn rows(samples: &[ProcSample]) -> Vec<ProcRow> {
        samples.iter().map(|s| ProcRow::of(s, 1000, 1)).collect()
    }

    #[test]
    fn sorting_by_cpu_descending_puts_the_busiest_process_first() {
        let mut r = rows(&[
            proc(1, "systemd", "root", 0.1, 10),
            proc(2, "cargo", "user", 71.0, 900),
            proc(3, "rust-analyzer", "user", 12.5, 1800),
        ]);
        sort_rows(&mut r, &[SortKey::new(Column::Cpu, true)]);
        assert_eq!(r.iter().map(|x| x.pid).collect::<Vec<_>>(), vec![2, 3, 1]);
    }

    #[test]
    fn sorting_by_name_ignores_case() {
        let mut r = rows(&[
            proc(1, "zsh", "user", 0.0, 1),
            proc(2, "AzWriter", "user", 0.0, 1),
            proc(3, "bash", "user", 0.0, 1),
        ]);
        sort_rows(&mut r, &[SortKey::new(Column::Name, false)]);
        assert_eq!(
            r.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(),
            vec!["AzWriter", "bash", "zsh"]
        );
        sort_rows(&mut r, &[SortKey::new(Column::Name, true)]);
        assert_eq!(
            r.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(),
            vec!["zsh", "bash", "AzWriter"]
        );
    }

    #[test]
    fn equal_keys_fall_back_to_the_pid_so_a_tick_never_shuffles_ties() {
        let mut r = rows(&[
            proc(30, "idle", "root", 0.0, 1),
            proc(10, "idle", "root", 0.0, 1),
            proc(20, "idle", "root", 0.0, 1),
        ]);
        sort_rows(&mut r, &[SortKey::new(Column::Cpu, true)]);
        assert_eq!(
            r.iter().map(|x| x.pid).collect::<Vec<_>>(),
            vec![10, 20, 30]
        );
        // Descending on the key does not reverse the tie-break.
        sort_rows(&mut r, &[SortKey::new(Column::Name, true)]);
        assert_eq!(
            r.iter().map(|x| x.pid).collect::<Vec<_>>(),
            vec![10, 20, 30]
        );
    }

    #[test]
    fn a_second_key_orders_what_the_first_leaves_equal() {
        let mut r = rows(&[
            proc(1, "b", "user", 0.0, 300),
            proc(2, "a", "root", 0.0, 100),
            proc(3, "c", "user", 0.0, 200),
            proc(4, "d", "root", 0.0, 400),
        ]);
        sort_rows(
            &mut r,
            &[
                SortKey::new(Column::User, false),
                SortKey::new(Column::Memory, true),
            ],
        );
        assert_eq!(
            r.iter().map(|x| x.pid).collect::<Vec<_>>(),
            vec![4, 2, 1, 3]
        );
    }

    #[test]
    fn no_sort_keys_means_pid_order() {
        let mut r = rows(&[
            proc(9, "a", "", 5.0, 1),
            proc(3, "b", "", 1.0, 1),
            proc(5, "c", "", 9.0, 1),
        ]);
        sort_rows(&mut r, &[]);
        assert_eq!(r.iter().map(|x| x.pid).collect::<Vec<_>>(), vec![3, 5, 9]);
    }

    #[test]
    fn blank_texts_sort_last_either_way() {
        let mut r = rows(&[
            proc(1, "a", "", 0.0, 1),
            proc(2, "b", "root", 0.0, 1),
            proc(3, "c", "user", 0.0, 1),
        ]);
        sort_rows(&mut r, &[SortKey::new(Column::User, false)]);
        assert_eq!(r.iter().map(|x| x.pid).collect::<Vec<_>>(), vec![2, 3, 1]);
        sort_rows(&mut r, &[SortKey::new(Column::User, true)]);
        assert_eq!(r.iter().map(|x| x.pid).collect::<Vec<_>>(), vec![3, 2, 1]);
    }

    // ---- the filter ----

    #[test]
    fn the_filter_finds_a_name_or_a_user_in_any_case() {
        let row = ProcRow::of(&proc(5102, "rust-analyzer", "felix", 0.0, 1), 1000, 1);
        assert!(matches(&row, ""));
        assert!(matches(&row, "  "));
        assert!(matches(&row, "Analyzer"));
        assert!(matches(&row, "FEL"));
        assert!(!matches(&row, "cargo"));
    }

    #[test]
    fn the_filter_finds_a_pid_by_its_first_digits() {
        let row = ProcRow::of(&proc(5102, "cargo", "user", 0.0, 1), 1000, 1);
        assert!(matches(&row, "5102"));
        assert!(matches(&row, "51"));
        assert!(!matches(&row, "102"));
    }

    // ---- the model ----

    #[test]
    fn a_new_model_has_no_readings_and_sorts_by_cpu() {
        let m = Model::new();
        assert_eq!(m.readings, 0);
        assert_eq!(m.shown_count(), 0);
        assert_eq!(m.sort(), &DEFAULT_SORT);
        assert!(m.cpu.is_empty());
        assert_eq!(m.cpu.capacity(), CHART_READINGS);
    }

    #[test]
    fn a_reading_grows_every_history_by_one() {
        let mut m = Model::new();
        let mut r = reading(1000, vec![]);
        r.disk_read = 2048;
        r.disk_written = 1024;
        r.net_received = 500;
        r.net_sent = 250;
        m.apply(r);
        assert_eq!(m.readings, 1);
        assert_eq!(m.cpu.to_vec(), vec![25.0]);
        assert_eq!(m.cores.len(), 4);
        assert_eq!(m.cores[3].to_vec(), vec![40.0]);
        assert_eq!(m.memory.to_vec(), vec![25.0]);
        assert_eq!(m.disk_read.to_vec(), vec![2048.0]);
        assert_eq!(m.disk_write.to_vec(), vec![1024.0]);
        assert_eq!(m.net_in.to_vec(), vec![500.0]);
        assert_eq!(m.net_out.to_vec(), vec![250.0]);
        assert_eq!(m.summary.memory_total, 16 << 30);
        assert_eq!(m.summary.disk_read_rate, 2048.0);
    }

    #[test]
    fn the_histories_keep_the_last_minute() {
        let mut m = Model::new();
        for i in 0..(CHART_READINGS + 15) {
            let mut r = reading(1000, vec![]);
            r.cpu = i as f32;
            m.apply(r);
        }
        assert_eq!(m.cpu.len(), CHART_READINGS);
        assert_eq!(m.cpu.latest(), Some((CHART_READINGS + 14) as f64));
        assert_eq!(m.cores[0].len(), CHART_READINGS);
    }

    #[test]
    fn a_reading_with_more_cores_starts_their_histories() {
        let mut m = Model::new();
        m.apply(reading(1000, vec![]));
        let mut r = reading(1000, vec![]);
        r.cores = vec![1.0; 8];
        m.apply(r);
        assert_eq!(m.cores.len(), 8);
        assert_eq!(m.cores[7].to_vec(), vec![1.0]);
        assert_eq!(m.cores[0].len(), 2);
    }

    #[test]
    fn the_rows_follow_the_sort_on_every_reading() {
        let mut m = Model::new();
        m.apply(reading(
            1000,
            vec![proc(1, "a", "u", 10.0, 1), proc(2, "b", "u", 90.0, 1)],
        ));
        assert_eq!(shown_pids(&m), vec![2, 1]);
        // The next reading: process 1 got busy - it moves up without a click.
        m.apply(reading(
            1000,
            vec![proc(1, "a", "u", 95.0, 1), proc(2, "b", "u", 5.0, 1)],
        ));
        assert_eq!(shown_pids(&m), vec![1, 2]);
    }

    #[test]
    fn a_new_sort_reorders_the_rows_at_once() {
        let mut m = Model::new();
        m.apply(reading(
            1000,
            vec![proc(1, "zed", "u", 10.0, 1), proc(2, "amp", "u", 90.0, 1)],
        ));
        m.set_sort(vec![SortKey::new(Column::Name, false)]);
        assert_eq!(shown_pids(&m), vec![2, 1]);
        m.set_sort(vec![]);
        assert_eq!(shown_pids(&m), vec![1, 2]);
    }

    #[test]
    fn the_filter_narrows_the_rows_and_survives_a_reading() {
        let mut m = Model::new();
        let procs = vec![
            proc(1, "cargo", "u", 1.0, 1),
            proc(2, "rustc", "u", 2.0, 1),
            proc(3, "rustdoc", "u", 3.0, 1),
        ];
        m.apply(reading(1000, procs.clone()));
        m.set_filter("rust");
        assert_eq!(m.filter(), "rust");
        assert_eq!(shown_pids(&m), vec![3, 2]);
        assert_eq!(m.process_count(), 3);
        m.apply(reading(1000, procs));
        assert_eq!(shown_pids(&m), vec![3, 2]);
        m.set_filter("");
        assert_eq!(m.shown_count(), 3);
    }

    #[test]
    fn the_selection_follows_its_process_when_the_rows_move() {
        let mut m = Model::new();
        m.apply(reading(
            1000,
            vec![proc(1, "a", "u", 10.0, 1), proc(2, "b", "u", 90.0, 1)],
        ));
        m.select_position(Some(1)); // "a", second
        assert_eq!(m.selected(), Some(1));
        assert_eq!(m.selected_position(), Some(1));
        m.apply(reading(
            1000,
            vec![proc(1, "a", "u", 95.0, 1), proc(2, "b", "u", 5.0, 1)],
        ));
        assert_eq!(m.selected(), Some(1));
        assert_eq!(m.selected_position(), Some(0));
        assert_eq!(m.selected_row().map(|r| r.name.as_str()), Some("a"));
    }

    #[test]
    fn the_selection_ends_with_its_process() {
        let mut m = Model::new();
        m.apply(reading(
            1000,
            vec![proc(1, "a", "u", 10.0, 1), proc(2, "b", "u", 90.0, 1)],
        ));
        m.select_position(Some(0));
        assert_eq!(m.selected(), Some(2));
        m.apply(reading(1000, vec![proc(1, "a", "u", 10.0, 1)]));
        assert_eq!(m.selected(), None);
        assert_eq!(m.selected_row(), None);
    }

    #[test]
    fn a_filtered_out_selection_is_kept_but_not_shown() {
        let mut m = Model::new();
        m.apply(reading(
            1000,
            vec![
                proc(1, "cargo", "u", 10.0, 1),
                proc(2, "rustc", "u", 90.0, 1),
            ],
        ));
        m.select_position(Some(1));
        assert_eq!(m.selected(), Some(1));
        m.set_filter("rustc");
        assert_eq!(m.selected(), Some(1));
        assert_eq!(m.selected_position(), None);
        assert_eq!(m.selected_row(), None);
        m.set_filter("");
        assert_eq!(m.selected_position(), Some(1));
    }

    #[test]
    fn selecting_past_the_rows_selects_nothing() {
        let mut m = Model::new();
        m.apply(reading(1000, vec![proc(1, "a", "u", 1.0, 1)]));
        m.select_position(Some(5));
        assert_eq!(m.selected(), None);
        m.select_position(Some(0));
        m.select_position(None);
        assert_eq!(m.selected(), None);
    }

    #[test]
    fn the_summary_counts_the_processes_and_the_notices_are_kept() {
        let mut m = Model::new();
        let mut r = reading(
            1000,
            vec![proc(1, "a", "u", 1.0, 1), proc(2, "b", "u", 1.0, 1)],
        );
        r.notices = vec!["Ended b (2)".to_string()];
        r.uptime = 99;
        m.apply(r);
        assert_eq!(m.summary.processes, 2);
        assert_eq!(m.summary.uptime, 99);
        assert_eq!(m.notices, vec!["Ended b (2)".to_string()]);
        // A reading without notices keeps the last ones (the status bar says it until the next).
        m.apply(reading(1000, vec![]));
        assert_eq!(m.notices, vec!["Ended b (2)".to_string()]);
    }
}
