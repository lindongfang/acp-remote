//! imported 交付索引的端到端集成测试（切片 5 任务 5.3 的验收闭环）。
//!
//! 驱动面：真实组合根 `app::compose::Composition`（真实 SQLite）+ 一个**在发布点读回存储**的
//! `EventPublisher`。要证明的语义都在 `CORE_PORTS_AND_STORAGE.md` §6.2/§7.4 与
//! `NODE_LINK_PROTOCOL.md` §6 里：
//!
//! - **先提交收据、再广播**：广播回调里能读到该 `origin_event_id` 的索引行（顺序不是靠调用点顺序
//!   推断出来的，而是在广播那一刻读存储）；
//! - **重复事件不重复追加**：同一 `origin_event_id` 再投递一次不新增 `local_sequence`、也不再广播；
//! - **ACK 与 origin/local 映射持久化**：`ack` 后 `load_ack` 读回同一 origin 游标，重启后仍是同一
//!   `local_sequence`；
//! - **Access 磁盘无远程正文**：正文（view）只经内存交给广播，关闭后再扫数据目录，标记文本不存在于
//!   任何文件里。

mod support;

use std::sync::{Arc, Mutex};

use acp_core::model::{
    CommittedDelivery, Digest, EventId, EventPayload, EventType, NodeId, OriginCursor, OriginEpoch,
    OriginEventRef, RemoteSessionRef, Sequence, SessionId, Timestamp, ViewJson,
};
use acp_core::ports::{EventPublisher, RemoteDeliveryStore, ReplayLimit};
use app::compose::Composition;

/// 正文标记：只在内存 payload 里出现，**不得**落到 Access 的磁盘上。
const BODY_MARKER: &str = "remote-body-marker-5c0f";

/// 在广播点读回存储的发布端口。
struct ReceiptCheckingPublisher {
    /// 组合根装配完成后再接线（存储由组合根自己打开）。
    store: Mutex<Option<Arc<storage_sqlite::session_store::SqliteStore>>>,
    /// 已广播的 `eventType`（顺序即广播顺序）。
    published: Mutex<Vec<String>>,
    runtime: tokio::runtime::Handle,
}

impl ReceiptCheckingPublisher {
    fn new(runtime: tokio::runtime::Handle) -> Self {
        Self {
            store: Mutex::new(None),
            published: Mutex::new(Vec::new()),
            runtime,
        }
    }

    fn connect(&self, store: Arc<storage_sqlite::session_store::SqliteStore>) {
        *self.store.lock().expect("store") = Some(store);
    }

    fn published(&self) -> Vec<String> {
        self.published.lock().expect("published").clone()
    }
}

impl EventPublisher for ReceiptCheckingPublisher {
    fn publish(&self, delivery: CommittedDelivery) {
        let CommittedDelivery::Imported {
            origin,
            event_type,
            payload_digest,
            ..
        } = &delivery
        else {
            panic!("imported 投递只能走 CommittedDelivery::Imported");
        };
        let store = self
            .store
            .lock()
            .expect("store")
            .clone()
            .expect("发布端口必须已接线存储");
        // 广播发生在**收据提交之后**：此刻读存储必须能看到这一行。`publish` 是同步签名而端口是异步的，
        // 因此用 `block_in_place` 在同一个多线程 runtime 上驱动一次读（测试进程独占该 runtime）。
        let entries = tokio::task::block_in_place(|| {
            self.runtime
                .block_on(store.local_replay(None, ReplayLimit::new(64)))
        })
        .expect("广播点读回交付索引");
        assert!(
            entries.iter().any(|entry| {
                entry.origin.origin_event_id == origin.origin_event_id
                    && entry.event_type == *event_type
                    && entry.payload_digest == *payload_digest
            }),
            "广播必须先于此处可见：发布时存储里没有 origin 事件 {} 的收据",
            origin.origin_event_id.as_str()
        );
        self.published
            .lock()
            .expect("published")
            .push(event_type.as_str().to_owned());
    }
}

/// 只含 `[daemon]`/身份/存储的最小配置：本文件不启动 Agent。
fn imported_config(data_dir: &std::path::Path) -> app::Config {
    let text = format!(
        "[daemon]\ndata_dir = {dir:?}\n[identity]\nkeystore = \"ephemeral\"\n\
         [dev_mode]\nenabled = true\n",
        dir = data_dir.display()
    );
    app::Config::from_toml(&text).expect("导入配置合法")
}

/// 一次 imported 投递的全部输入（同一 `origin_event_id` 用于重复投递）。
struct Origin {
    session: RemoteSessionRef,
    origin: OriginEventRef,
    origin_sequence: Sequence,
    event_type: EventType,
    digest: Digest,
}

fn origin() -> Origin {
    let owner = NodeId::new("bdb2ec20-f98c-4d87-b789-e540d527ef87").expect("owner");
    let session = RemoteSessionRef::new(
        owner.clone(),
        SessionId::new("2bc8b944-2a4f-46a7-8c31-b2c40923f60a").expect("session"),
    );
    let origin = OriginEventRef::new(
        owner,
        OriginEpoch::new("6f1d0f0e-0f5a-4a3f-9f3e-4c5c8b7a1d20").expect("epoch"),
        EventId::new("a3d1e5b0-6a4c-4b5a-8f1e-2d3c4b5a6f70").expect("event"),
    );
    // 摘要只是索引列：本文件断言的是「同一摘要被原样保留」，不是 ACPR-CJ1 的复算（那条路径由
    // `node-link-client` 的 `DeliveryBridge` 负责，并在其自带用例里覆盖）。
    let digest = Digest::new("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA").expect("32 字节摘要");
    Origin {
        session,
        origin,
        origin_sequence: Sequence::new(7).expect("序号"),
        event_type: EventType::new("agent.message.chunk").expect("事件类型"),
        digest,
    }
}

fn payload() -> EventPayload {
    // 正文只在内存里流转：`deliver_imported` 把它交给广播，写入存储的只有摘要。
    let view = ViewJson::new(&format!(r#"{{"text":"{BODY_MARKER}"}}"#)).expect("视图");
    EventPayload::new(view, None)
}

/// 登记 imported 会话的无正文索引行（catalog/import 的生产写入路径）。
///
/// `commit_receipt` 以 `(owner, session)` 外键约束到 `imported_session`，因此事件只能在索引行之后提交——
/// 这条依赖本身就是「先 import、后交付」的合同，测试按同一顺序驱动。
async fn register(
    composition: &Composition,
    session: &RemoteSessionRef,
    epoch: &OriginEpoch,
) -> Result<(), acp_core::model::PortError> {
    let at = Timestamp::new("2026-10-20T00:00:00.000Z").expect("时间戳");
    composition
        .attachments()
        .upsert_session(
            acp_core::ports::ImportedSessionRecord {
                session: session.clone(),
                origin_epoch: Some(epoch.clone()),
                title: Some("Remote session".to_owned()),
                agent: Some(
                    acp_core::model::AgentRef::try_new(
                        acp_core::model::AgentId::new("codex").expect("agent"),
                        "Codex",
                    )
                    .expect("agent ref"),
                ),
                state: Some(acp_core::model::SessionState::Idle),
                version: Some(acp_core::model::Version::new(1)),
                created_at: Some(at.clone()),
                last_origin_sequence: None,
                acked: None,
                attachment: None,
                updated_at: at,
            },
            Timestamp::new("2026-10-20T00:00:00.000Z").expect("时间戳"),
        )
        .await
}

/// 把来源事件的**全部**输入提交一次；返回核心的「是否首次提交」。
async fn deliver(composition: &Composition, origin: &Origin) -> bool {
    composition
        .broker()
        .deliver_imported(
            origin.session.clone(),
            origin.origin.clone(),
            origin.origin_sequence,
            origin.event_type.clone(),
            origin.digest.clone(),
            Some(payload()),
        )
        .await
        .expect("提交 imported 收据")
}

/// 数据目录里所有文件的字节（主库 + WAL/SHM，以及附件目录；正文若落盘一定在这里）。
fn data_files_bytes(directory: &std::path::Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![directory.to_path_buf()];
    while let Some(path) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(bytes) = std::fs::read(&path) {
                out.push((path, bytes));
            }
        }
    }
    out
}

#[test]
fn imported_delivery_commits_the_receipt_before_broadcast_and_survives_restart() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("多线程运行时");
    runtime.block_on(async {
        let root = support::TempRoot::new("imported-delivery");
        let data_dir = root.join("data");
        support::create_owner_only_dir(&data_dir);

        let publisher = Arc::new(ReceiptCheckingPublisher::new(
            tokio::runtime::Handle::current(),
        ));
        let composition = Composition::assemble(imported_config(&data_dir), publisher.clone())
            .await
            .expect("组合根装配");
        // 组合根持有同一个 SQLite 实例：把它的句柄交给发布端口（测试结束后必须先释放这一份）。
        let store = composition.attachments();
        publisher.connect(Arc::clone(&store));

        let origin = origin();
        register(&composition, &origin.session, &origin.origin.origin_epoch)
            .await
            .expect("登记 imported 会话");

        // ① 首次投递：收据提交成功且广播一次（广播点已断言行可见）。
        assert!(deliver(&composition, &origin).await, "首次投递必须提交");
        assert_eq!(publisher.published().len(), 1, "首次投递必须广播一次");
        let entries = store
            .local_replay(None, ReplayLimit::new(16))
            .await
            .expect("本地重放");
        assert_eq!(entries.len(), 1, "一条来源事件只对应一条交付索引");
        let local_sequence = entries[0].local_sequence;
        assert_eq!(
            entries[0].origin.origin_event_id, origin.origin.origin_event_id,
            "索引行必须保留来源事件标识"
        );
        assert_eq!(
            entries[0].origin_sequence, origin.origin_sequence,
            "索引行必须保留 origin 序号（local 映射不改写来源序号）"
        );

        // ② 同一来源事件重复投递：不新增行、不再广播。
        assert!(
            !deliver(&composition, &origin).await,
            "同一 origin_event_id 的重复投递必须被判为重复"
        );
        assert_eq!(
            publisher.published().len(),
            1,
            "重复事件不得二次广播（客户端按 eventId 去重的前提是服务端不重复投递）"
        );
        let after_duplicate = store
            .local_replay(None, ReplayLimit::new(16))
            .await
            .expect("本地重放");
        assert_eq!(after_duplicate.len(), 1, "重复事件不得追加索引行");

        // ③ ACK：累计游标持久化，且回退请求不生效。
        let cursor = OriginCursor::new(origin.origin.origin_epoch.clone(), origin.origin_sequence);
        let outcome = store
            .ack(
                &origin.session,
                cursor.clone(),
                Timestamp::new("2026-10-20T00:00:00.000Z").expect("时间戳"),
            )
            .await
            .expect("累计 ACK");
        assert!(outcome.applied, "首次 ACK 必须前进");
        assert_eq!(
            store.load_ack(&origin.session).await.expect("读回 ACK"),
            Some(cursor.clone()),
            "ACK 必须持久化"
        );
        let backwards = store
            .ack(
                &origin.session,
                OriginCursor::new(
                    origin.origin.origin_epoch.clone(),
                    Sequence::new(1).expect("序号"),
                ),
                Timestamp::new("2026-10-20T00:00:01.000Z").expect("时间戳"),
            )
            .await
            .expect("回退 ACK");
        assert!(!backwards.applied, "回退请求必须被忽略");
        assert_eq!(
            store.load_ack(&origin.session).await.expect("读回 ACK"),
            Some(cursor),
            "回退请求不得改写既有 ACK"
        );

        // ④ 关闭（同一关闭序列：先停 Agent，再关存储），并释放测试持有的句柄。
        composition.host().shutdown_all().await;
        drop(store);
        drop(publisher);
        composition.close().await.expect("组合根可关闭");

        // ⑤ 重启恢复：索引与 ACK 都来自持久化，不依赖断线前的内存队列。
        let publisher = Arc::new(ReceiptCheckingPublisher::new(
            tokio::runtime::Handle::current(),
        ));
        let restarted = Composition::assemble(imported_config(&data_dir), publisher.clone())
            .await
            .expect("重启后装配");
        let store = restarted.attachments();
        publisher.connect(Arc::clone(&store));
        let recovered = store
            .local_replay(None, ReplayLimit::new(16))
            .await
            .expect("重启后本地重放");
        assert_eq!(recovered.len(), 1, "重启后索引行必须原样恢复（不重复追加）");
        assert_eq!(
            recovered[0].local_sequence, local_sequence,
            "重启后 local sequence 映射必须稳定"
        );
        assert!(
            store
                .load_ack(&origin.session)
                .await
                .expect("重启后读回 ACK")
                .is_some(),
            "重启后必须恢复累计 ACK（重连续读点）"
        );
        assert!(
            publisher.published().is_empty(),
            "重启本身不得广播任何 imported 事件"
        );

        restarted.host().shutdown_all().await;
        drop(store);
        drop(publisher);
        restarted.close().await.expect("组合根可关闭");

        // ⑥ Access 磁盘没有远程正文：标记文本不得出现在任何文件里。
        for (path, bytes) in data_files_bytes(&data_dir) {
            assert!(
                !bytes
                    .windows(BODY_MARKER.len())
                    .any(|window| window == BODY_MARKER.as_bytes()),
                "远程正文标记必须不落盘，但出现在 {}",
                path.display()
            );
        }
    });
}
