use super::{OwnedDatabase, Storage, prepare_storage};
use futures_util::task::{ArcWake, waker_ref};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use std::{
    fs,
    future::Future,
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
};

struct Signal {
    ready: AtomicBool,
    polls: AtomicUsize,
    wakes: AtomicUsize,
    parent: Mutex<Option<Waker>>,
    start: Instant,
    events: Mutex<Vec<(&'static str, Duration)>>,
}
impl Signal {
    fn record(&self, kind: &'static str) {
        self.events
            .lock()
            .expect("test events")
            .push((kind, self.start.elapsed()));
    }
}
impl ArcWake for Signal {
    fn wake_by_ref(signal: &Arc<Self>) {
        signal.wakes.fetch_add(1, Ordering::SeqCst);
        signal.record("wake");
        signal.ready.store(true, Ordering::SeqCst);
        let parent = signal.parent.lock().expect("test parent waker").clone();
        if let Some(parent) = parent {
            parent.wake();
        }
    }
}
struct Counted<F> {
    future: Pin<Box<F>>,
    signal: Arc<Signal>,
}
impl<F> Counted<F> {
    fn new(future: F) -> Self {
        Self {
            future: Box::pin(future),
            signal: Arc::new(Signal {
                ready: AtomicBool::new(true),
                polls: AtomicUsize::new(0),
                wakes: AtomicUsize::new(0),
                parent: Mutex::new(None),
                start: Instant::now(),
                events: Mutex::new(Vec::new()),
            }),
        }
    }
}
impl<F: Future<Output = ()>> Future for Counted<F> {
    type Output = ();
    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<()> {
        let this = self.get_mut();
        *this.signal.parent.lock().expect("test parent waker") = Some(context.waker().clone());
        // An unrelated sample timer may poll this wrapper, but cannot poll the owned close.
        if !this.signal.ready.swap(false, Ordering::SeqCst) {
            return Poll::Pending;
        }
        this.signal.polls.fetch_add(1, Ordering::SeqCst);
        this.signal.record("poll");
        let waker = waker_ref(&this.signal);
        this.future.as_mut().poll(&mut Context::from_waker(&waker))
    }
}

#[derive(Debug)]
enum FileTail {
    Absent,
    Exclusive,
    Failed(Option<i32>),
    Reparse,
}
#[derive(Debug)]
struct Tail {
    size: u32,
    files: Vec<(&'static str, FileTail)>,
}
impl Tail {
    fn closed(&self) -> bool {
        self.size == 0
            && matches!(self.files.first(), Some(("", FileTail::Exclusive)))
            && self
                .files
                .iter()
                .all(|(_, state)| matches!(state, FileTail::Absent | FileTail::Exclusive))
    }
}
fn first_tail(area: &OwnedDatabase, pool: &SqlitePool) -> Tail {
    let mut files = Vec::new();
    for suffix in ["", "-wal", "-shm"] {
        let path = area.directory.join(format!("owned.sqlite3{suffix}"));
        let state = match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_attributes() & 0x400 != 0 => FileTail::Reparse,
            Ok(_) => match fs::OpenOptions::new()
                .read(true)
                .write(true)
                .share_mode(0)
                .open(path)
            {
                Ok(file) => {
                    drop(file);
                    FileTail::Exclusive
                }
                Err(error) => FileTail::Failed(error.raw_os_error()),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => FileTail::Absent,
            Err(error) => FileTail::Failed(error.raw_os_error()),
        };
        files.push((suffix, state));
    }
    let tail = Tail {
        size: pool.size(),
        files,
    };
    for (suffix, state) in &tail.files {
        if let FileTail::Failed(code) = state {
            eprintln!("first exclusive {suffix} failed with OS code {code:?}");
        }
    }
    eprintln!(
        "first physical probe: {tail:?}; owned area={:?}",
        area.directory
    );
    tail
}
async fn open_two(
    area: &Arc<OwnedDatabase>,
) -> (
    Storage,
    sqlx::pool::PoolConnection<sqlx::Sqlite>,
    sqlx::pool::PoolConnection<sqlx::Sqlite>,
) {
    let (host, ticket) = prepare_storage(area, SqlitePoolOptions::new().max_connections(2));
    let storage = ticket
        .ready()
        .await
        .expect("existing private Storage factory")
        .projection(host);
    let mut first = storage
        .pool_for_test()
        .acquire()
        .await
        .expect("first real connection");
    let mut second = storage
        .pool_for_test()
        .acquire()
        .await
        .expect("second simultaneous real connection");
    for connection in [&mut first, &mut second] {
        let version: i64 = sqlx::query_scalar("PRAGMA user_version")
            .fetch_one(&mut **connection)
            .await
            .expect("read actual schema");
        assert_eq!(
            version,
            crate::storage::SCHEMA_VERSION,
            "test never writes the schema version"
        );
        let _: i64 = sqlx::query_scalar("SELECT revision FROM node_state WHERE singleton=1")
            .fetch_one(&mut **connection)
            .await
            .expect("real SQL on each connection");
    }
    assert_eq!(storage.pool_for_test().size(), 2);
    (storage, first, second)
}
async fn pool_stage(storage: &Storage, pool: &SqlitePool) {
    pool.close_event().await;
    loop {
        if storage.required_close_status_in_scope().is_complete()
            && pool.is_closed()
            && pool.size() == 1
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
}
async fn reach<F: Future<Output = ()> + Unpin>(
    storage: &Storage,
    area: &OwnedDatabase,
    closing: &mut F,
) -> Result<(), Tail> {
    tokio::select! {
        biased;
        () = closing => Err(first_tail(area, storage.pool_for_test())),
        () = pool_stage(storage, storage.pool_for_test()) => Ok(()),
    }
}
fn finish(area: &OwnedDatabase, storage: &Storage, tail: &Tail) {
    assert!(tail.closed(), "first physical tail failed: {tail:?}");
    assert!(storage.registry_close_status().is_complete());
    area.cleanup();
}

#[tokio::test]
async fn development_held_return_is_paced_until_real_ack() {
    let area = Arc::new(OwnedDatabase::create());
    let (storage, mut first, mut second) = open_two(&area).await;
    first.return_to_pool().await;
    drop(first);
    let held = second.return_to_pool();
    let mut closing = Counted::new(storage.close());
    let stage =
        tokio::time::timeout(Duration::from_secs(2), reach(&storage, &area, &mut closing)).await;
    if !matches!(stage, Ok(Ok(()))) {
        held.await;
        panic!("close failed before the held-return pool phase: {stage:?}");
    }
    let polls = closing.signal.polls.load(Ordering::SeqCst);
    let wakes = closing.signal.wakes.load(Ordering::SeqCst);
    let start = Instant::now();
    let early = tokio::select! {
        biased;
        () = &mut closing => Some(first_tail(&area, storage.pool_for_test())),
        () = tokio::time::sleep(Duration::from_millis(250)) => None,
    };
    let elapsed = start.elapsed();
    let actual_polls = closing.signal.polls.load(Ordering::SeqCst) - polls;
    let actual_wakes = closing.signal.wakes.load(Ordering::SeqCst) - wakes;
    let limit = 4 + elapsed.as_millis().div_ceil(10);
    eprintln!(
        "sample={elapsed:?} polls={actual_polls} wakes={actual_wakes} limit={limit}; events={:?}",
        closing.signal.events.lock().expect("sample events")
    );
    held.await;
    drop(second);
    assert!(early.is_none(), "early ready before return: {early:?}");
    assert!(actual_polls as u128 <= limit && actual_wakes as u128 <= limit);
    tokio::time::timeout(Duration::from_secs(2), &mut closing)
        .await
        .expect("same close after real normal return ACK");
    let tail = first_tail(&area, storage.pool_for_test());
    let mut repeat = Box::pin(storage.close());
    let fast =
        std::future::poll_fn(|context| Poll::Ready(repeat.as_mut().poll(context).is_ready())).await;
    assert!(fast, "converged close must not add a sleep");
    finish(&area, &storage, &tail);
}

#[tokio::test]
async fn development_two_callers_are_retained_until_first_physical_probe() {
    let area = Arc::new(OwnedDatabase::create());
    let (storage, mut first, mut second) = open_two(&area).await;
    first.return_to_pool().await;
    drop(first);
    let held = second.return_to_pool();
    let mut closing = Box::pin(storage.close());
    let stage =
        tokio::time::timeout(Duration::from_secs(2), reach(&storage, &area, &mut closing)).await;
    if !matches!(stage, Ok(Ok(()))) {
        held.await;
        panic!("first close early: {stage:?}");
    }
    let mut other = Box::pin(storage.close());
    let pending =
        std::future::poll_fn(|context| Poll::Ready(other.as_mut().poll(context).is_pending()))
            .await;
    assert!(
        pending,
        "second caller must really wait on the original mutex"
    );
    held.await;
    drop(second);
    let (which, tail) = tokio::time::timeout(Duration::from_secs(2), async {
        tokio::select! {
            biased;
            () = &mut closing => (0, first_tail(&area, storage.pool_for_test())),
            () = &mut other => (1, first_tail(&area, storage.pool_for_test())),
        }
    })
    .await
    .expect("one natural close returns after normal ACK");
    // The first physical probe above is synchronous, before polling the other caller again.
    if which == 0 {
        other.await;
    } else {
        closing.await;
    }
    finish(&area, &storage, &tail);
}

#[tokio::test]
async fn development_cancelled_waiter_reenters_the_same_owner() {
    let area = Arc::new(OwnedDatabase::create());
    let (storage, mut first, mut second) = open_two(&area).await;
    first.return_to_pool().await;
    drop(first);
    let held = second.return_to_pool();
    let mut closing = Box::pin(storage.close());
    let stage =
        tokio::time::timeout(Duration::from_secs(2), reach(&storage, &area, &mut closing)).await;
    if !matches!(stage, Ok(Ok(()))) {
        held.await;
        panic!("first close early: {stage:?}");
    }
    drop(closing);
    let mut reentry = Box::pin(storage.close());
    let pending =
        std::future::poll_fn(|context| Poll::Ready(reentry.as_mut().poll(context).is_pending()))
            .await;
    assert!(
        pending
            && storage.pool_for_test().size() == 1
            && storage.required_close_status_in_scope().is_complete()
    );
    held.await;
    drop(second);
    tokio::time::timeout(Duration::from_secs(2), reentry)
        .await
        .expect("same owner completes after waiter cancellation");
    let tail = first_tail(&area, storage.pool_for_test());
    finish(&area, &storage, &tail);
}
