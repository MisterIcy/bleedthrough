//! Peak resident memory, where the platform exposes it.

/// Reads `VmHWM` (peak resident set size, in KiB) from the text of `/proc/self/status`.
pub fn parse_peak_rss_kb(status: &str) -> Option<u64> {
    status.lines().find_map(|line| {
        let rest = line.strip_prefix("VmHWM:")?;
        rest.split_whitespace().next()?.parse().ok()
    })
}

/// The peak resident set size of this process in KiB; `None` where `/proc/self/status` is absent.
pub fn peak_rss_kb() -> Option<u64> {
    parse_peak_rss_kb(&std::fs::read_to_string("/proc/self/status").ok()?)
}
