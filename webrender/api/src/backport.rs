/// Backport of the precise_time_ns function which
/// was removed from the time 0.3.x API.
pub fn precise_time_ns() -> u64 {
    (time::OffsetDateTime::now_utc() - time::OffsetDateTime::UNIX_EPOCH)
    .whole_nanoseconds() // returns i128
    .abs() as u64
}
