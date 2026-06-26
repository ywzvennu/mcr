//! Peak resident-set-size (RSS) reporting on Linux.
//!
//! We read `VmHWM` ("high water mark" for resident memory) from
//! `/proc/self/status`. `VmHWM` is the peak resident set size the process has
//! reached since it started (or since the counter was last reset), reported in
//! kibibytes by the kernel. This is the canonical Linux source for "peak RSS"
//! and matches `getrusage(RUSAGE_SELF).ru_maxrss` without needing `libc`.
//!
//! The value is process-wide and monotonic, so it cannot be attributed to a
//! single engine's perft call; the benchmark reports it once, after all work is
//! done, as the overall peak for the process.

use std::fs;

/// Read the process's peak resident set size (`VmHWM`) in kibibytes.
///
/// Returns `None` if `/proc/self/status` is unavailable or the field is absent
/// (e.g. on a non-Linux platform), so callers can degrade gracefully.
pub fn peak_rss_kib() -> Option<u64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmHWM:") {
            // Format: "VmHWM:\t   12345 kB"
            let kib = rest.split_whitespace().next()?.parse().ok()?;
            return Some(kib);
        }
    }
    None
}
