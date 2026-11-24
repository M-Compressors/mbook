//! Work that is computation, on the thread pool of `rayon` (mbook-ui docs,
//! "Threads").
//!
//! [`spawn`] hands a closure to the pool and gives a future of its result.
//! The background executor of GPUI awaits that future and does nothing else
//! meanwhile, so that reading files, the catalogue and the network are not
//! held up by a conversion, and a conversion shares the pool with the AV1
//! encoder it calls. [`init`] builds the pool once, when the application
//! starts.
//!
//! In the unit tests of this crate [`spawn`] does the work when its future is
//! first polled, on the thread that polls it: the test executor of GPUI then
//! decides the order of everything, as it does without the pool. The pool
//! itself is tested here.

use std::any::Any;
use std::future::Future;
use std::num::NonZero;
use std::panic::{self, AssertUnwindSafe};
use std::thread;

use futures::channel::oneshot;

/// The stack of a thread of the pool, in bytes. A thread of the standard
/// library gets 2 MiB, which parsing deeply nested Markdown, laying out a
/// diagram or encoding a picture can exceed; the memory is reserved, and
/// only the pages that are used are taken.
const STACK_BYTES: usize = 16 * 1024 * 1024;

/// What the future of a work says when the pool dropped the work without
/// running it. The global pool of `rayon` drops none.
const DROPPED: &str = "the thread pool dropped the work without running it";

/// The threads of the pool for a system that offers `available`: half of
/// them, and one at least.
fn threads(available: usize) -> usize {
    (available / 2).max(1)
}

/// The settings of the pool: half the threads of the system, a large stack
/// and names that say whose the threads are.
fn builder() -> rayon::ThreadPoolBuilder {
    let available = thread::available_parallelism().map_or(1, NonZero::get);
    rayon::ThreadPoolBuilder::new()
        .num_threads(threads(available))
        .stack_size(STACK_BYTES)
        .thread_name(|index| format!("mbook-compute-{index}"))
}

/// Builds the global pool of `rayon` with the settings of [`builder`].
///
/// It must be called before anything uses the pool: the first use of
/// `rayon` builds a pool with its own settings.
///
/// # Errors
///
/// When the global pool exists already. That pool is then used as it is.
pub(crate) fn init() -> Result<(), rayon::ThreadPoolBuildError> {
    builder().build_global()
}

/// Does `work` on the pool and gives a future of its result.
///
/// Dropping the future before the work starts means the work does not run;
/// work that has started runs to its end, and its result is dropped. A
/// panic of the work is resumed where the future is awaited.
#[cfg(not(test))]
pub(crate) fn spawn<T, F>(work: F) -> impl Future<Output = T> + Send + 'static
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    on_pool(work)
}

/// Does `work` when the future is first polled, on the thread that polls
/// it: the unit tests leave the order of all work to the test executor.
#[cfg(test)]
pub(crate) async fn spawn<T, F>(work: F) -> T
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    work()
}

/// Hands `work` to the global pool of `rayon`.
fn on_pool<T, F>(work: F) -> impl Future<Output = T> + Send + 'static
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let (sender, receiver) = oneshot::channel();
    let job = job(work, sender);
    #[cfg(feature = "visual")]
    let job = {
        let pending = visual::Pending::new();
        move || {
            job();
            // After the result is sent: the awaiting task is woken first.
            drop(pending);
        }
    };
    rayon::spawn(job);
    outcome(receiver)
}

/// What the visual tests need: they draw a screen once the pool has
/// finished the work its last step started.
#[cfg(feature = "visual")]
pub(crate) mod visual {
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// How many works handed to the pool have not ended.
    static PENDING: AtomicUsize = AtomicUsize::new(0);

    /// One work that has not ended, counted while it lives.
    pub(super) struct Pending(());

    impl Pending {
        pub(super) fn new() -> Self {
            PENDING.fetch_add(1, Ordering::SeqCst);
            Self(())
        }
    }

    impl Drop for Pending {
        fn drop(&mut self) {
            PENDING.fetch_sub(1, Ordering::SeqCst);
        }
    }

    /// How many works handed to the pool have not ended.
    pub(crate) fn pending() -> usize {
        PENDING.load(Ordering::SeqCst)
    }
}

/// The closure the pool runs: it does `work` unless nobody waits for it any
/// more, and sends what came of it.
fn job<T, F>(work: F, sender: oneshot::Sender<thread::Result<T>>) -> impl FnOnce() + Send + 'static
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    move || {
        if sender.is_canceled() {
            return;
        }
        let outcome = panic::catch_unwind(AssertUnwindSafe(work));
        // The future may have been dropped while the work ran.
        drop(sender.send(outcome));
    }
}

/// The result of the work, or its panic resumed.
async fn outcome<T>(receiver: oneshot::Receiver<thread::Result<T>>) -> T {
    let outcome = receiver
        .await
        .unwrap_or_else(|oneshot::Canceled| Err(Box::new(DROPPED) as Box<dyn Any + Send>));
    match outcome {
        Ok(value) => value,
        Err(payload) => panic::resume_unwind(payload),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::hint::black_box;
    use std::panic::{self, AssertUnwindSafe};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread;

    use futures::channel::oneshot;
    use futures::executor::block_on;

    use super::{DROPPED, builder, init, job, on_pool, outcome, spawn, threads};

    #[test]
    fn the_pool_takes_half_the_threads_of_the_system() {
        assert_eq!(threads(16), 8);
        assert_eq!(threads(9), 4);
        assert_eq!(threads(2), 1);
    }

    #[test]
    fn a_system_of_one_thread_still_gets_a_thread() {
        assert_eq!(threads(1), 1);
        assert_eq!(threads(0), 1);
    }

    #[test]
    fn the_threads_of_the_pool_are_named_and_counted() {
        let pool = builder().build().unwrap();
        let available = thread::available_parallelism().unwrap().get();
        assert_eq!(pool.current_num_threads(), threads(available));
        let name = pool.install(|| thread::current().name().map(str::to_owned));
        assert!(name.unwrap().starts_with("mbook-compute-"));
    }

    /// Recurses through 1500 frames of 4 KiB each, about 6 MiB of stack.
    fn deep(depth: u32) -> u64 {
        let frame = black_box([u8::try_from(depth % 251).unwrap(); 4096]);
        let below = if depth == 0 { 0 } else { deep(depth - 1) };
        black_box(below + u64::from(frame[usize::try_from(depth).unwrap() % 4096]))
    }

    #[test]
    fn a_thread_of_the_pool_has_room_for_deep_recursion() {
        let pool = builder().build().unwrap();
        assert!(pool.install(|| deep(1500)) > 0);
    }

    #[test]
    fn the_global_pool_is_built_once() {
        // Another test may have used the pool first; either way it exists
        // after the first call.
        drop(init());
        assert!(init().is_err());
    }

    #[test]
    fn work_on_the_pool_gives_its_result_from_another_thread() {
        let caller = thread::current().id();
        let (value, ran_on) = block_on(on_pool(|| (6 * 7, thread::current().id())));
        assert_eq!(value, 42);
        assert_ne!(ran_on, caller);
    }

    #[test]
    fn a_panic_of_the_work_is_resumed_where_it_is_awaited() {
        let caught =
            panic::catch_unwind(|| block_on(on_pool(|| -> u8 { panic!("the work broke") })));
        let payload = caught.unwrap_err();
        assert_eq!(payload.downcast_ref::<&str>(), Some(&"the work broke"));
    }

    #[cfg(feature = "visual")]
    #[test]
    fn a_work_on_the_pool_is_counted_until_it_ends() {
        let (entered, release) = (
            Arc::new(std::sync::Barrier::new(2)),
            Arc::new(std::sync::Barrier::new(2)),
        );
        let (inside, leave) = (Arc::clone(&entered), Arc::clone(&release));
        let future = on_pool(move || {
            inside.wait();
            leave.wait();
        });
        entered.wait();
        assert!(super::visual::pending() >= 1);
        release.wait();
        block_on(future);
    }

    #[test]
    fn work_nobody_waits_for_does_not_run() {
        let ran = Arc::new(AtomicBool::new(false));
        let (sender, receiver) = oneshot::channel::<thread::Result<()>>();
        drop(receiver);
        let flag = Arc::clone(&ran);
        job(move || flag.store(true, Ordering::SeqCst), sender)();
        assert!(!ran.load(Ordering::SeqCst));
    }

    #[test]
    fn work_whose_future_goes_while_it_runs_ends_quietly() {
        let (sender, receiver) = oneshot::channel::<thread::Result<u8>>();
        job(
            move || {
                drop(receiver);
                7
            },
            sender,
        )();
    }

    #[test]
    fn work_the_pool_dropped_is_reported_where_it_is_awaited() {
        let (sender, receiver) = oneshot::channel::<thread::Result<u8>>();
        drop(sender);
        let caught = panic::catch_unwind(AssertUnwindSafe(|| block_on(outcome(receiver))));
        let payload = caught.unwrap_err();
        assert_eq!(payload.downcast_ref::<&str>(), Some(&DROPPED));
    }

    #[test]
    fn in_unit_tests_the_work_runs_when_its_future_is_polled() {
        thread_local!(static RAN: Cell<bool> = const { Cell::new(false) });
        let future = spawn(|| RAN.with(|ran| ran.set(true)));
        assert!(!RAN.with(Cell::get));
        block_on(future);
        assert!(RAN.with(Cell::get));
    }
}
