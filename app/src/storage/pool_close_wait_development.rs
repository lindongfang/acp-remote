//! Ordinary development regressions; independent PC acceptance remains separately owned.
use std::{
    fs,
    future::Future,
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::PathBuf,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::task::JoinHandle;

type Check = Result<(), String>;
type HeldReturn = Pin<Box<dyn Future<Output = ()> + Send>>;

struct Case {
    storage: Arc<crate::sqlite_fixture_scope::FixtureStorage>,
    scope: crate::sqlite_fixture_scope::SqliteFixtureScope,
    roots: Arc<CaseRoots>,
}
impl std::ops::Deref for Case {
    type Target = CaseRoots;
    fn deref(&self) -> &CaseRoots {
        &self.roots
    }
}
struct CaseRoots {
    directory: PathBuf,
    parent: PathBuf,
    marker: String,
}
impl Case {
    async fn new(max2: bool) -> Self {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let marker = format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let directory = parent.join(format!("acp-pool-close-v2-{marker}"));
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join("owner"), &marker).unwrap();
        let path = directory.join("owned.sqlite3");
        let roots = Arc::new(CaseRoots {
            directory,
            parent,
            marker,
        });
        let scope = crate::sqlite_fixture_scope::SqliteFixtureScope::prepare(&path, roots.clone());
        let storage = if max2 {
            scope
                .open_with(sqlx::sqlite::SqlitePoolOptions::new().max_connections(2))
                .await
        } else {
            scope.open().await
        }
        .unwrap();
        eprintln!(
            "pool-close-v2 area={} max2={max2}",
            roots.directory.display()
        );
        Self {
            storage: Arc::new(storage),
            scope,
            roots,
        }
    }

    async fn held(&self) -> HeldReturn {
        let pool = self.storage.pool_for_test();
        assert!(!pool.is_closed(), "same original pool begins open");
        let mut idle = pool.acquire().await.unwrap();
        let mut returning = pool.acquire().await.unwrap();
        idle.return_to_pool().await;
        let held = Box::pin(returning.return_to_pool());
        drop(returning);
        drop(idle);
        held
    }

    async fn finish(self, mut errors: Vec<String>) {
        self.scope.stop_and_observe().await;
        let pool = self.storage.pool_for_test();
        let owner = self
            .storage
            .request_registry()
            .observe_authority_slots_for_test();
        eprintln!(
            "pool-close-v2 tail closed={} size={} sealed={} leases={} slots={}",
            pool.is_closed(),
            pool.size(),
            owner.sealed,
            owner.leases,
            owner.slots.len()
        );
        if !self.storage.registry_close_status().is_complete()
            || !pool.is_closed()
            || pool.size() != 0
            || !owner.sealed
            || owner.leases != 0
            || !owner.slots.is_empty()
        {
            errors.push("original pool/registry tail did not converge".into());
        }
        for suffix in ["", "-wal", "-shm", "-journal"] {
            let path = self.directory.join(format!("owned.sqlite3{suffix}"));
            if !path.exists() && !suffix.is_empty() {
                eprintln!("pool-close-v2 first-exclusive {suffix}=absent");
                continue;
            }
            match fs::OpenOptions::new()
                .read(true)
                .write(true)
                .share_mode(0)
                .open(&path)
            {
                Ok(handle) => {
                    drop(handle);
                    eprintln!("pool-close-v2 first-exclusive {suffix}=ok");
                }
                Err(error) => errors.push(format!("first-exclusive {suffix}: {error:?}")),
            }
        }
        drop(owner);
        if errors.is_empty() {
            self.cleanup();
        } else {
            panic!(
                "pool-close-v2 preserved={} errors={errors:?}",
                self.directory.display()
            );
        }
    }

    fn cleanup(&self) {
        let actual = self.directory.canonicalize().unwrap();
        assert_eq!(actual, self.directory);
        assert_eq!(actual.parent(), Some(self.parent.as_path()));
        assert_eq!(
            fs::read_to_string(actual.join("owner")).unwrap(),
            self.marker
        );
        assert_eq!(
            fs::symlink_metadata(&actual).unwrap().file_attributes() & 0x400,
            0
        );
        for name in [
            "owned.sqlite3",
            "owned.sqlite3-wal",
            "owned.sqlite3-shm",
            "owned.sqlite3-journal",
            "owner",
        ] {
            let file = actual.join(name);
            if file.exists() {
                let metadata = fs::symlink_metadata(&file).unwrap();
                assert!(metadata.is_file());
                assert_eq!(metadata.file_attributes() & 0x400, 0);
                fs::remove_file(file).unwrap();
            }
        }
        fs::remove_dir(actual).unwrap();
    }
}

struct Counted<F> {
    future: Pin<Box<F>>,
    polls: Arc<AtomicUsize>,
}
impl<F: Future> Future for Counted<F> {
    type Output = F::Output;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        this.polls.fetch_add(1, Ordering::SeqCst);
        this.future.as_mut().poll(cx)
    }
}
fn start_close(
    storage: &Arc<crate::sqlite_fixture_scope::FixtureStorage>,
) -> (JoinHandle<()>, Arc<AtomicUsize>) {
    let storage = Arc::clone(storage);
    let polls = Arc::new(AtomicUsize::new(0));
    let handle = tokio::spawn(Counted {
        future: Box::pin(async move { storage.close().await }),
        polls: Arc::clone(&polls),
    });
    (handle, polls)
}

async fn reach_pool_wait(case: &Case, close: &JoinHandle<()>) -> Check {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if close.is_finished() { return Err("close returned before real held-return was polled".into()); }
            let pool = case.storage.pool_for_test();
            let owner = case.storage.request_registry().observe_authority_slots_for_test();
            let reached = pool.is_closed() && pool.size() == 1 && case.storage.required_close_status_in_scope().is_complete() && owner.sealed && owner.leases == 0 && owner.slots.is_empty();
            drop(owner);
            if reached {
                eprintln!("pool-close-v2 reached original pool closed/size1/registry sealed+empty; held real return unpolled");
                return Ok(());
            }
            tokio::task::yield_now().await;
        }
    }).await.map_err(|_| "original pool waiting stage not reached".to_owned())?
}

fn record(errors: &mut Vec<String>, check: Check) {
    if let Err(error) = check {
        errors.push(error);
    }
}
async fn join_close(handle: JoinHandle<()>, errors: &mut Vec<String>) {
    if let Err(error) = handle.await {
        errors.push(format!("real close task join: {error}"));
    }
}

#[tokio::test]
async fn held_real_return_blocks_close_and_limits_polls() {
    let case = Case::new(true).await;
    let held = case.held().await;
    let (close, polls) = start_close(&case.storage);
    let mut errors = Vec::new();
    record(&mut errors, reach_pool_wait(&case, &close).await);
    let before = polls.load(Ordering::SeqCst);
    let started = Instant::now();
    tokio::time::sleep(Duration::from_millis(250)).await;
    let elapsed = started.elapsed();
    let sampled = polls.load(Ordering::SeqCst) - before;
    let limit = 4 + elapsed.as_millis().div_ceil(10);
    eprintln!("pool-close-v2 stable polls={sampled} elapsed={elapsed:?} oracle={limit}");
    if close.is_finished() {
        errors.push("close completed while real return future was still unpolled".into());
    }
    if sampled as u128 > limit {
        errors.push(format!("unexpected stable-window polls {sampled}>{limit}"));
    }
    held.await;
    eprintln!(
        "pool-close-v2 same real return future completed normal closed-return/worker ACK path"
    );
    join_close(close, &mut errors).await;
    // In an already-converged state, this fully owned repeated close must be Ready in one poll.
    let mut repeated = Box::pin(case.storage.close());
    let ready = std::future::poll_fn(|cx| Poll::Ready(repeated.as_mut().poll(cx).is_ready())).await;
    if !ready {
        errors.push("already-converged repeat close was not immediately Ready".into());
        repeated.as_mut().await;
    }
    drop(repeated);
    case.finish(errors).await;
}

#[tokio::test]
async fn cancel_real_close_waiter_then_resume_same_owner() {
    let case = Case::new(true).await;
    let held = case.held().await;
    let (first, _) = start_close(&case.storage);
    let mut errors = Vec::new();
    record(&mut errors, reach_pool_wait(&case, &first).await);
    first.abort();
    match first.await {
        Err(error) if error.is_cancelled() => eprintln!(
            "pool-close-v2 actual owned first close future cancelled at pool stage; exact Sleep poll not claimed"
        ),
        other => errors.push(format!("first close cancellation result: {other:?}")),
    }
    let (second, _) = start_close(&case.storage);
    record(&mut errors, reach_pool_wait(&case, &second).await);
    held.await;
    join_close(second, &mut errors).await;
    case.finish(errors).await;
}

#[tokio::test]
async fn concurrent_close_callers_preserve_original_owner() {
    let case = Case::new(true).await;
    let held = case.held().await;
    let (first, _) = start_close(&case.storage);
    let mut errors = Vec::new();
    record(&mut errors, reach_pool_wait(&case, &first).await);
    let (second, _) = start_close(&case.storage);
    tokio::task::yield_now().await;
    if first.is_finished() || second.is_finished() {
        errors.push("a concurrent close caller returned before actual held return".into());
    }
    eprintln!("pool-close-v2 second caller pending; mutex stage not mislabelled pool arrival");
    held.await;
    join_close(first, &mut errors).await;
    join_close(second, &mut errors).await;
    case.finish(errors).await;
}

#[tokio::test]
async fn default_and_max2_normal_return_and_ordinary_drop() {
    for max2 in [false, true] {
        for explicit in [false, true] {
            let case = Case::new(max2).await;
            let mut first = case.storage.pool_for_test().acquire().await.unwrap();
            let second = if max2 {
                Some(case.storage.pool_for_test().acquire().await.unwrap())
            } else {
                None
            };
            if explicit {
                first.return_to_pool().await;
            }
            drop(first);
            if let Some(mut second) = second {
                if explicit {
                    second.return_to_pool().await;
                }
                drop(second);
            }
            case.storage.close().await;
            eprintln!("pool-close-v2 normal explicit={explicit} max2={max2}");
            case.finish(Vec::new()).await;
        }
    }
}
