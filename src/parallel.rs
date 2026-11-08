//! Runs per-file work on several threads, returning the results in input order (ADR-021).

use std::num::NonZeroUsize;
use std::panic;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

/// Fewer items than this per thread run on fewer threads, down to the calling thread alone:
/// starting a thread costs more than it saves. About 64 files is where two threads stop being
/// slower than one in the glob view, the cheapest per-file work.
const MIN_ITEMS_PER_THREAD: usize = 32;

/// `items.iter().map(work).collect()`, run on up to `threads` threads (the calling one
/// included). The results keep the order of `items`, so output is the same for any thread
/// count.
///
/// Every result is ready before the first is printed. Streaming them through a channel instead
/// cost more than it saved: each result woke the printing thread, and on small trees those
/// wakeups made the glob view slower than one thread.
pub(crate) fn map_in_order<T: Sync, R: Send>(
    items: &[T],
    threads: NonZeroUsize,
    work: impl Fn(&T) -> R + Sync,
) -> Vec<R> {
    let thread_count = threads.get().min(items.len() / MIN_ITEMS_PER_THREAD);
    if thread_count <= 1 {
        return items.iter().map(work).collect();
    }

    // Each thread claims the next unclaimed index until none is left, so threads that get
    // small files take more of them. The index alone orders the results, so the counter needs
    // no stronger ordering than `Relaxed`: joining the threads synchronizes everything else.
    let next_index = AtomicUsize::new(0);
    let claim_and_work = || {
        let mut done = Vec::new();
        loop {
            let index = next_index.fetch_add(1, Ordering::Relaxed);
            let Some(item) = items.get(index) else {
                return done;
            };
            done.push((index, work(item)));
        }
    };
    // A scope joins its threads before it returns, so they may borrow `items`, `work`, and the
    // counter without `Arc`.
    let mut results: Vec<(usize, R)> = thread::scope(|scope| {
        let helpers: Vec<_> = (1..thread_count)
            .map(|_| scope.spawn(claim_and_work))
            .collect();
        let mut results = claim_and_work();
        for helper in helpers {
            // A helper's panic is a bug: raise it again here, where the caller sees it.
            results.extend(
                helper
                    .join()
                    .unwrap_or_else(|payload| panic::resume_unwind(payload)),
            );
        }
        results
    });
    results.sort_unstable_by_key(|&(index, _)| index);
    results.into_iter().map(|(_, result)| result).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn results_keep_the_input_order_for_any_thread_count() {
        let items: Vec<usize> = (0..1000).collect();
        let work = |&item: &usize| {
            // Uneven work, so later items often finish before earlier ones.
            std::hint::black_box((0..(item % 7) * 1000).sum::<usize>());
            item * 2
        };
        let expected: Vec<usize> = items.iter().map(work).collect();
        for threads in [1, 2, 8, 64] {
            let threads = NonZeroUsize::new(threads).unwrap();
            assert_eq!(map_in_order(&items, threads, work), expected, "{threads}");
            assert_eq!(
                map_in_order(&items[..10], threads, work),
                expected[..10],
                "{threads}"
            );
        }
    }
}
