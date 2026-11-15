//! Independent original Storage/pool probes. No A1 store, foreign pool or invented ACK.
use super::Storage;
use anyhow::{Context as _, Result, ensure};
use futures_util::{
    FutureExt,
    future::poll_fn,
    task::{ArcWake, waker},
};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use std::{
    fs::{self, OpenOptions},
    future::Future,
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    task::{Context, Poll, Waker},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::time::{Instant, timeout};

const WATCH: Duration = Duration::from_secs(20);
static SERIAL: AtomicU64 = AtomicU64::new(0);
type Connection = sqlx::pool::PoolConnection<sqlx::Sqlite>;

struct Fixture {
    storage: Arc<Storage>,
    directory: PathBuf,
    root: PathBuf,
    marker: String,
    label: &'static str,
    physical: Mutex<Option<bool>>,
}

impl Fixture {
    async fn new(max2: bool, label: &'static str) -> Result<Self> {
        let root = std::env::var_os("RPC_FIXTURE_ROOT")
            .filter(|v| !v.is_empty())
            .map_or_else(
                || {
                    let dir = std::env::temp_dir()
                        .join(format!("acpr-rpc-fixture-{}", std::process::id()));
                    fs::create_dir_all(&dir)?;
                    Ok::<_, anyhow::Error>(dir)
                },
                |v| Ok(PathBuf::from(v)),
            )?
            .canonicalize()?;
        let marker = format!(
            "{}-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos(),
            SERIAL.fetch_add(1, Ordering::SeqCst)
        );
        let directory = root.join(format!("{label}-{marker}"));
        fs::create_dir(&directory)?;
        fs::write(directory.join("owner"), marker.as_bytes())?;
        eprintln!("[RPC-AREA] {}", directory.display());
        let path = directory.join("node.sqlite3");
        let options = SqlitePoolOptions::new().max_connections(if max2 { 2 } else { 1 });
        let storage = Storage::open_with_pool_options(&path, options).await?;
        Ok(Self {
            storage: Arc::new(storage),
            directory,
            root,
            marker,
            label,
            physical: Mutex::new(None),
        })
    }
    fn pool(&self) -> &SqlitePool {
        &self.storage.pool
    }
    fn probe_once(&self) -> Result<()> {
        ensure!(
            self.physical.lock().unwrap().is_none(),
            "no second physical probe"
        );
        let result = probe_directory(&self.directory, self.label);
        *self.physical.lock().unwrap() = Some(result.is_ok());
        result
    }
    fn finish_after_close(self, body: Result<()>) -> Result<()> {
        // This function never calls close or repeats the physical probe.
        let physical = *self.physical.lock().unwrap();
        let status = self.storage.registry_close_status();
        let converged = status.is_complete() && self.pool().is_closed() && self.pool().size() == 0;
        let remove = body.is_ok() && physical == Some(true) && converged;
        if remove {
            let actual = self.directory.canonicalize()?;
            ensure!(
                actual.parent() == Some(self.root.as_path()),
                "only own direct child"
            );
            ensure!(
                fs::read(actual.join("owner"))? == self.marker.as_bytes(),
                "own marker"
            );
            ensure!(
                fs::symlink_metadata(&actual)?.file_attributes() & 0x400 == 0,
                "no reparse child"
            );
            for entry in fs::read_dir(&actual)? {
                let entry = entry?;
                ensure!(
                    matches!(
                        entry.file_name().to_str(),
                        Some(
                            "owner"
                                | "node.sqlite3"
                                | "node.sqlite3-wal"
                                | "node.sqlite3-shm"
                                | "node.sqlite3-journal"
                        )
                    ),
                    "no unknown fixture file"
                );
                let metadata = fs::symlink_metadata(entry.path())?;
                ensure!(
                    metadata.is_file() && metadata.file_attributes() & 0x400 == 0,
                    "only own plain file"
                );
                fs::remove_file(entry.path())?;
            }
            fs::remove_dir(actual)?;
        }
        eprintln!(
            "[RPC-CLEANUP] {}",
            serde_json::json!({"label":self.label,
            "body_ok":body.is_ok(),"first_physical":physical,"pool_closed":self.pool().is_closed(),
            "pool_size":self.pool().size(),"registry_status":format!("{status:?}"),"removed":remove})
        );
        body?;
        ensure!(
            physical == Some(true),
            "first exclusive absent/failed; do not retry"
        );
        ensure!(converged, "original Storage pool/registry not converged");
        Ok(())
    }
}

fn probe_directory(directory: &Path, label: &str) -> Result<()> {
    let mut okay = true;
    let mut files = Vec::new();
    for name in [
        "node.sqlite3",
        "node.sqlite3-wal",
        "node.sqlite3-shm",
        "node.sqlite3-journal",
    ] {
        let path = directory.join(name);
        let row = match fs::symlink_metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && name != "node.sqlite3" => {
                serde_json::json!({"file":name,"exists":false})
            }
            Err(e) => {
                okay = false;
                serde_json::json!({"file":name,"metadata_error":e.raw_os_error()})
            }
            Ok(m) if !m.is_file() || m.file_attributes() & 0x400 != 0 => {
                okay = false;
                serde_json::json!({"file":name,"unsafe_file":true})
            }
            Ok(_) => match OpenOptions::new()
                .read(true)
                .write(true)
                .share_mode(0)
                .open(path)
            {
                Ok(handle) => {
                    drop(handle);
                    serde_json::json!({"file":name,"exists":true,"first_exclusive":true})
                }
                Err(e) => {
                    okay = false;
                    serde_json::json!({"file":name,"exists":true,"first_exclusive":false,"os_error":e.raw_os_error()})
                }
            },
        };
        files.push(row);
    }
    eprintln!(
        "[RPC-PHYSICAL] {}",
        serde_json::json!({"label":label,"files":files,"all":okay})
    );
    ensure!(
        okay,
        "original first exclusive failed; preserved, never retried"
    );
    Ok(())
}

async fn sql_connection(f: &Fixture) -> Result<Connection> {
    let mut connection = f.pool().acquire().await?;
    let version: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(&mut *connection)
        .await?;
    ensure!(
        version == crate::storage::SCHEMA_VERSION,
        "query/assert current schema only; never assign PRAGMA"
    );
    let _: i64 = sqlx::query_scalar("SELECT revision FROM node_state WHERE singleton=1")
        .fetch_one(&mut *connection)
        .await?;
    Ok(connection)
}
async fn two(f: &Fixture) -> Result<(Connection, Connection)> {
    let first = sql_connection(f).await?;
    let second = sql_connection(f).await?;
    ensure!(
        f.pool().size() == 2,
        "two simultaneously held actual connections"
    );
    Ok((first, second))
}

async fn wait_for_pool_closed<F: Future<Output = ()> + Unpin>(
    f: &Fixture,
    closing: &mut F,
) -> Result<()> {
    timeout(WATCH, async {
        loop {
            ensure!(
                poll_fn(|cx| Poll::Ready(closing.poll_unpin(cx).is_pending())).await,
                "Storage.close returned while the actual connection was held"
            );
            if f.pool().is_closed() && f.pool().size() == 1 {
                return Ok(());
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .context("pool close stage watchdog")?
}

#[derive(Default)]
struct Signal {
    ready: AtomicBool,
    polls: AtomicU64,
    wakes: AtomicU64,
    parent: Mutex<Option<Waker>>,
    events: Mutex<Vec<(&'static str, u128)>>,
}
struct Wake {
    signal: Arc<Signal>,
    start: Instant,
}
impl ArcWake for Wake {
    fn wake_by_ref(me: &Arc<Self>) {
        me.signal.wakes.fetch_add(1, Ordering::SeqCst);
        me.signal
            .events
            .lock()
            .unwrap()
            .push(("wake", me.start.elapsed().as_micros()));
        me.signal.ready.store(true, Ordering::SeqCst);
        let parent = me.signal.parent.lock().unwrap().clone();
        if let Some(parent) = parent {
            parent.wake();
        }
    }
}
struct Counted<F> {
    future: Pin<Box<F>>,
    signal: Arc<Signal>,
    start: Instant,
}
impl<F: Future<Output = ()>> Future for Counted<F> {
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let me = self.get_mut();
        *me.signal.parent.lock().unwrap() = Some(cx.waker().clone());
        if !me.signal.ready.swap(false, Ordering::SeqCst) {
            return Poll::Pending;
        }
        me.signal.polls.fetch_add(1, Ordering::SeqCst);
        me.signal
            .events
            .lock()
            .unwrap()
            .push(("poll", me.start.elapsed().as_micros()));
        let w = waker(Arc::new(Wake {
            signal: Arc::clone(&me.signal),
            start: me.start,
        }));
        me.future.as_mut().poll(&mut Context::from_waker(&w))
    }
}
impl<F> Drop for Counted<F> {
    fn drop(&mut self) {
        self.signal.parent.lock().unwrap().take();
    }
}

#[tokio::test]
async fn rpc001_002_original_floating_blocks_until_normal_ack_first_tail() -> Result<()> {
    let f = Fixture::new(true, "rpc-held").await?;
    let (mut a, mut b) = two(&f).await?;
    a.return_to_pool().await;
    drop(a);
    let mut closing = Box::pin(f.storage.close());
    wait_for_pool_closed(&f, &mut closing).await?;
    ensure!(
        f.pool().is_closed() && f.pool().size() == 1,
        "first close did not seal the pool at the held-return stage"
    );
    eprintln!(
        "[RPC-STAGE] {}",
        serde_json::json!({"label":f.label,"closed":true,"size":1,
        "registry_complete":false,"checked_out_connection_held":true})
    );
    timeout(WATCH, b.return_to_pool())
        .await
        .context("return first checked-out connection after close starts")?;
    timeout(WATCH, &mut closing)
        .await
        .context("same primary close after actual normal return")?;
    let physical = f.probe_once();
    drop(closing);
    f.finish_after_close(physical)
}

#[tokio::test]
async fn rpc003_own_wakes_paced_and_converged_repeat_ready() -> Result<()> {
    let f = Fixture::new(true, "rpc-paced").await?;
    let (mut a, mut b) = two(&f).await?;
    a.return_to_pool().await;
    drop(a);
    let signal = Arc::new(Signal::default());
    signal.ready.store(true, Ordering::SeqCst);
    let mut closing = Box::pin(Counted {
        future: Box::pin(f.storage.close()),
        signal: Arc::clone(&signal),
        start: Instant::now(),
    });
    wait_for_pool_closed(&f, &mut closing).await?;
    let polls = signal.polls.load(Ordering::SeqCst);
    let wakes = signal.wakes.load(Ordering::SeqCst);
    let start = Instant::now();
    let early = tokio::select! {
        biased;
        ()=&mut closing=>Some(f.probe_once()),
        ()=tokio::time::sleep(Duration::from_millis(250))=>None,
    };
    let elapsed = start.elapsed();
    let polls = signal.polls.load(Ordering::SeqCst) - polls;
    let wakes = signal.wakes.load(Ordering::SeqCst) - wakes;
    let bound = 4 + elapsed.as_millis().div_ceil(10);
    eprintln!(
        "[RPC-PACING] {}",
        serde_json::json!({"label":f.label,"micros":elapsed.as_micros(),
        "polls":polls,"wakes":wakes,"bound":bound,"events":*signal.events.lock().unwrap()})
    );
    ensure!(
        early.is_none(),
        "primary close completed while the normal return was held"
    );
    ensure!(
        u128::from(polls) <= bound && u128::from(wakes) <= bound,
        "unexplained excessive self-polls/wakes"
    );
    timeout(WATCH, b.return_to_pool())
        .await
        .context("return paced checked-out connection after close starts")?;
    timeout(WATCH, &mut closing)
        .await
        .context("same sampled close after actual normal return")?;
    let physical = f.probe_once();
    ensure!(
        f.storage.close().now_or_never().is_some(),
        "already converged repeat close adds pending/sleep"
    );
    drop(closing);
    f.finish_after_close(physical)
}

#[tokio::test]
async fn rpc004_two_retained_callers_first_ready_probe_then_other_poll() -> Result<()> {
    let f = Fixture::new(true, "rpc-natural-pair").await?;
    let (mut a, mut b) = two(&f).await?;
    a.return_to_pool().await;
    drop(a);
    let mut first = Box::pin(f.storage.close());
    wait_for_pool_closed(&f, &mut first).await?;
    let mut second = Box::pin(f.storage.close());
    let pending = poll_fn(|cx| Poll::Ready(second.as_mut().poll(cx).is_pending())).await;
    ensure!(
        pending,
        "second retained caller did not remain pending at the close stage"
    );
    eprintln!(
        "[RPC-PAIR-STATE] {}",
        serde_json::json!({"label":f.label,"closed":f.pool().is_closed(),
        "size":f.pool().size(),"registry":format!("{:?}",f.storage.registry_close_status())})
    );
    ensure!(
        f.pool().is_closed() && f.pool().size() == 1,
        "retained callers did not observe the same checked-out connection"
    );
    timeout(WATCH, b.return_to_pool())
        .await
        .context("return paired checked-out connection after close starts")?;
    timeout(WATCH, async { tokio::join!(&mut first, &mut second) })
        .await
        .context("retained close completion")?;
    let physical = f.probe_once();
    eprintln!(
        "[RPC-NATURAL] {}",
        serde_json::json!({"retained_both":true,"cancelled_callers":0,
        "second_naturally_ready":true,"held_after_release":true})
    );
    drop(first);
    drop(second);
    f.finish_after_close(physical)
}

async fn cancelled_case(second_already_waiting: bool) -> Result<()> {
    let f = Fixture::new(
        true,
        if second_already_waiting {
            "rpc-cancel-pair"
        } else {
            "rpc-reentry"
        },
    )
    .await?;
    let (mut a, mut b) = two(&f).await?;
    a.return_to_pool().await;
    drop(a);
    let mut first = Box::pin(f.storage.close());
    wait_for_pool_closed(&f, &mut first).await?;
    let mut next = if second_already_waiting {
        Some(Box::pin(f.storage.close()))
    } else {
        None
    };
    if let Some(next) = next.as_mut() {
        ensure!(
            poll_fn(|cx| Poll::Ready(next.as_mut().poll(cx).is_pending())).await,
            "second mutex wait"
        );
    }
    drop(first); // Actual owning Box, not a Pin reference; no task is detached.
    let mut next = next.unwrap_or_else(|| Box::pin(f.storage.close()));
    ensure!(
        poll_fn(|cx| Poll::Ready(next.as_mut().poll(cx).is_pending())).await,
        "reentry returned while same Floating remained held"
    );
    eprintln!(
        "[RPC-REENTRY-STATE] {}",
        serde_json::json!({"label":f.label,"closed":f.pool().is_closed(),
        "size":f.pool().size(),"registry":format!("{:?}",f.storage.registry_close_status())})
    );
    ensure!(
        f.pool().is_closed()
            && f.pool().size() == 1
            && !f.storage.registry_close_status().is_complete(),
        "same original pool reached its close stage while the connection remained checked out"
    );
    timeout(WATCH, b.return_to_pool())
        .await
        .context("return checked-out connection after owner cancellation")?;
    timeout(WATCH, &mut next)
        .await
        .context("surviving same-owner close")?;
    let physical = f.probe_once();
    drop(next);
    f.finish_after_close(physical)
}
#[tokio::test]
async fn rpc005_drop_actual_close_owner_reentry_and_waiting_second() -> Result<()> {
    cancelled_case(false).await?;
    cancelled_case(true).await
}

async fn ordinary(max2: bool, drop_return: bool, label: &'static str) -> Result<()> {
    let f = Fixture::new(max2, label).await?;
    if max2 {
        let (mut a, mut b) = two(&f).await?;
        if !drop_return {
            a.return_to_pool().await;
            b.return_to_pool().await;
        }
        drop(a);
        drop(b);
    } else {
        let mut a = sql_connection(&f).await?;
        if !drop_return {
            a.return_to_pool().await;
        }
        drop(a);
    }
    timeout(WATCH, f.storage.close())
        .await
        .context("ordinary original Storage.close")?;
    let physical = f.probe_once(); // Before any repeat or cleanup.
    timeout(WATCH, f.storage.close())
        .await
        .context("ordinary repeat after first probe")?;
    f.finish_after_close(physical)
}
#[tokio::test]
async fn rpc006_original_default_max1_explicit_and_drop_return() -> Result<()> {
    ordinary(false, false, "rpc-max1-explicit").await?;
    ordinary(false, true, "rpc-max1-drop").await
}
#[tokio::test]
async fn rpc007_original_max2_two_explicit_and_two_drop_returns() -> Result<()> {
    ordinary(true, false, "rpc-max2-explicit").await?;
    ordinary(true, true, "rpc-max2-drop").await
}
