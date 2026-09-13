//! Test-only, calling-thread allocation measurements for the synchronous NES
//! core. libtest and other tests may allocate concurrently on other threads.
use stats_alloc::{Region, Stats, StatsAlloc};
use std::alloc::{GlobalAlloc, Layout, System};

thread_local! {
    // Const initialization and no destructor: looking up the counter must not
    // allocate or register thread-local cleanup from inside the allocator.
    static LOCAL: StatsAlloc<System> = const { StatsAlloc::system() };
}

pub struct ThreadAlloc;

// SAFETY: Forward every operation unchanged to the same System allocator.
// Thread-local state contains only counters; allocations may still be freed on
// a different thread. The hooks do not format, lock, allocate bookkeeping, or
// panic. If TLS is unavailable during teardown, use System directly.
unsafe impl GlobalAlloc for ThreadAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        LOCAL
            .try_with(|allocator| unsafe { allocator.alloc(layout) })
            .unwrap_or_else(|_| unsafe { System.alloc(layout) })
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        LOCAL
            .try_with(|allocator| unsafe { allocator.alloc_zeroed(layout) })
            .unwrap_or_else(|_| unsafe { System.alloc_zeroed(layout) })
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LOCAL
            .try_with(|allocator| unsafe { allocator.dealloc(ptr, layout) })
            .unwrap_or_else(|_| unsafe { System.dealloc(ptr, layout) });
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        LOCAL
            .try_with(|allocator| unsafe { allocator.realloc(ptr, layout, new_size) })
            .unwrap_or_else(|_| unsafe { System.realloc(ptr, layout, new_size) })
    }
}

/// Keep both snapshots and the workload on the calling thread. Unlike an
/// exposed Region, this cannot accidentally be moved to a different thread.
pub fn measure(work: impl FnOnce()) -> Stats {
    LOCAL.with(|allocator| {
        let region = Region::new(allocator);
        work();
        region.change()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        alloc::{GlobalAlloc, Layout, handle_alloc_error},
        hint::black_box,
        sync::atomic::{AtomicBool, Ordering},
        time::{Duration, Instant},
    };

    // Exercise the allocator methods directly so optimization cannot remove a
    // Box/Vec and make the positive control accidentally allocation-free.
    fn heap_activity() {
        let original = Layout::from_size_align(32, 8).unwrap();
        let grown = Layout::from_size_align(96, 8).unwrap();
        let shrunk = Layout::from_size_align(48, 8).unwrap();
        let zeroed = Layout::from_size_align(16, 8).unwrap();
        // SAFETY: Every request has a nonzero valid layout. Reallocations retain
        // alignment, null is handled before reuse, and each live final pointer
        // is freed once with its current layout through the same allocator.
        unsafe {
            let ptr = crate::GLOBAL.alloc(original);
            if ptr.is_null() {
                handle_alloc_error(original);
            }
            let ptr = crate::GLOBAL.realloc(black_box(ptr), original, grown.size());
            if ptr.is_null() {
                handle_alloc_error(grown);
            }
            let ptr = crate::GLOBAL.realloc(black_box(ptr), grown, shrunk.size());
            if ptr.is_null() {
                handle_alloc_error(shrunk);
            }
            crate::GLOBAL.dealloc(black_box(ptr), shrunk);
            let ptr = crate::GLOBAL.alloc_zeroed(zeroed);
            if ptr.is_null() {
                handle_alloc_error(zeroed);
            }
            crate::GLOBAL.dealloc(black_box(ptr), zeroed);
        }
    }

    fn expected_activity() -> Stats {
        Stats {
            allocations: 2,
            deallocations: 2,
            reallocations: 2,
            bytes_allocated: 112,
            bytes_deallocated: 112,
            bytes_reallocated: 16,
        }
    }

    #[test]
    fn measures_every_allocator_operation_on_the_current_thread() {
        assert_eq!(measure(heap_activity), expected_activity());
        // A subsequent region gets a fresh baseline, not the earlier counts.
        assert_eq!(measure(|| {}), Stats::default());
    }

    #[test]
    fn rust_heap_operations_use_the_instrumented_global_allocator() {
        let stats = measure(|| {
            let bytes = vec![42_u8; 64];
            drop(black_box(bytes));
        });
        assert_eq!(
            stats,
            Stats {
                allocations: 1,
                deallocations: 1,
                bytes_allocated: 64,
                bytes_deallocated: 64,
                ..Stats::default()
            }
        );
    }

    #[test]
    fn nested_regions_keep_independent_baselines() {
        let mut inner = Stats::default();
        let outer = measure(|| inner = measure(heap_activity));
        assert_eq!(inner, expected_activity());
        assert_eq!(outer, expected_activity());
    }

    fn wait_for(flag: &AtomicBool) {
        let start = Instant::now();
        while !flag.load(Ordering::Acquire) {
            assert!(
                start.elapsed() < Duration::from_secs(30),
                "allocation-counter worker did not reach its synchronization point"
            );
            std::thread::yield_now();
        }
    }

    #[test]
    fn other_threads_do_not_pollute_the_measured_region() {
        let start = AtomicBool::new(false);
        let finished = AtomicBool::new(false);
        std::thread::scope(|scope| {
            let worker = scope.spawn(|| {
                wait_for(&start);
                let stats = measure(heap_activity);
                finished.store(true, Ordering::Release);
                stats
            });
            let stats = measure(|| {
                start.store(true, Ordering::Release);
                wait_for(&finished);
            });
            let worker_stats = worker.join().unwrap();
            assert_eq!(stats, Stats::default());
            // Ignoring another thread must not mean ignoring its own region.
            assert_eq!(worker_stats, expected_activity());
        });
    }
}
