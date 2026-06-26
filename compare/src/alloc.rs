//! A counting global allocator for the benchmark binary.
//!
//! Wraps [`std::alloc::System`] and atomically tallies the number of
//! allocations and the total bytes requested. This lets the benchmark report an
//! apples-to-apples runtime-memory metric (heap allocs + bytes) for each engine
//! around its perft call.
//!
//! The counters are plain relaxed atomics, so the per-allocation overhead is a
//! single relaxed fetch-add. To keep the CPU timing table pristine, the
//! allocation pass in `main.rs` is run separately from the timing pass; the
//! allocator is always installed, but the timing pass simply ignores the
//! counters.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};

/// Number of successful allocations since process start.
static ALLOC_COUNT: AtomicU64 = AtomicU64::new(0);
/// Total bytes requested across all successful allocations since process start.
static ALLOC_BYTES: AtomicU64 = AtomicU64::new(0);

/// A snapshot of the global allocation counters.
#[derive(Clone, Copy, Debug)]
pub struct AllocSnapshot {
    /// Cumulative number of allocations at the time of the snapshot.
    pub count: u64,
    /// Cumulative bytes requested at the time of the snapshot.
    pub bytes: u64,
}

/// Capture the current cumulative allocation counters.
pub fn snapshot() -> AllocSnapshot {
    AllocSnapshot {
        count: ALLOC_COUNT.load(Ordering::Relaxed),
        bytes: ALLOC_BYTES.load(Ordering::Relaxed),
    }
}

/// The allocation activity that happened between two snapshots.
#[derive(Clone, Copy, Debug)]
pub struct AllocDelta {
    /// Allocations performed in the interval.
    pub count: u64,
    /// Bytes requested in the interval.
    pub bytes: u64,
}

/// Compute `end - start` for two snapshots taken around a region of work.
pub fn delta(start: AllocSnapshot, end: AllocSnapshot) -> AllocDelta {
    AllocDelta {
        count: end.count.wrapping_sub(start.count),
        bytes: end.bytes.wrapping_sub(start.bytes),
    }
}

/// A `System`-backed allocator that counts allocations and bytes.
///
/// Only the allocating entry points (`alloc`, `alloc_zeroed`, `realloc`) bump
/// the counters; deallocation is forwarded untouched. We deliberately count
/// *requested* bytes (the `Layout` size) rather than rounded-up usable sizes so
/// the figure is deterministic across runs and platforms.
pub struct CountingAlloc;

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = System.alloc(layout);
        if !ptr.is_null() {
            ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
            ALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = System.alloc_zeroed(layout);
        if !ptr.is_null() {
            ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
            ALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_ptr = System.realloc(ptr, layout, new_size);
        if !new_ptr.is_null() && new_size > layout.size() {
            // Count the growth as a fresh allocation of the additional bytes.
            ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
            ALLOC_BYTES.fetch_add((new_size - layout.size()) as u64, Ordering::Relaxed);
        }
        new_ptr
    }
}
