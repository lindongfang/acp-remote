//! Access 侧出站全链路（`node pair --mode access` 的 claim/status、信任落盘、启动实例化与撤销）。
//!
//! 组合方式：**真实** Access 组合根（`app::compose::Composition`：真实 SQLite、真实 keystore、真实
//! `AccessManager`）+ **真实** Owner 接入面（`support::owner::OwnerNode`：真实 `server::node_link`
//! listener、真实 `identity_auth::Authority`、TLS `direct`）。没有脚本化传输，也没有替身信任锚：
//! Access 的信任锚是 `[node_link.access] trust_roots` 指向的自签 CA（因此自定义 CA 路径被真正走通）。
//!
//! 覆盖的判据：
//!
//! - claim 用二维码固定的 Owner 公钥验签 `ownerProof`，`--fingerprint` 的来源（`peerPublicKeyFingerprint`）
//!   与 Owner 身份一致；
//! - 观察到 `approved` 之后才落盘 Owner 信任（`kind = owner`、`state = paired`、endpoint 逐字保留）；
//! - 信任落盘会立即建立出站 link；远程 `SessionEndpoint` 由当前 link 惰性构造；
//! - `revoke` 停止重连并让路由立即失效；
//! - 重启后 `start_persisted` 依据持久记录重新建立 link（不依赖内存）；
//! - 错误的 CA 让 claim 以 `local.remote_error` 失败关闭，且不写任何信任。
//!
//! 另覆盖 `session.title.set` 的 Access → Owner 转发（`core.session-title.v1`）：Owner 落盘 trim 后的
//! 自定义标题且不改 Agent 标题列、Access 不写标题/命令/收据/导入索引，同一 `requestId` 重放不重复
//! 修改也不追加 `session.updated`，Access 重启后仍按持久信任建链并再次修改，来源离线时显式失败且零
//! 本地写入。最后一条覆盖 **Owner** 自身的重启（同一 `data_dir`、同一身份种子）：已提交的自定义标题
//! 从新存储连接读回，Agent 标题列不变，且重启不重放 `session.updated`、不重执行同一 `requestId`。

mod support;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use acp_core::model::{
    Actor, ClientCommand, CommandKind, CommandPayload, CommandReceipt, CommandStatus, Digest,
    NodeId, NodeKind, NodeState, PortError, RemoteSessionRef, RequestId, SessionDeletionWrite,
    SessionId, SessionTitle, Timestamp,
};
use acp_core::ports::{RemoteRuntimeDirectory, SessionQuery};
use app::access::{AccessDeps, AccessManager};
use app::compose::Composition;
use app::trust::OutboundTrust;
use serde_json::json;
use server::local_admin::Method;
use server::local_admin::{AccessPairing, LocalErrorCode};
use sqlx::{Connection as _, Row as _};

use support::TempRoot;
use support::owner::OwnerNode;

const SAMPLE_SESSION: &str = "11111111-1111-4111-8111-111111111111";

#[test]
fn access_claims_a_real_owner_persists_trust_and_cancels_routes_on_revoke() {
    support::block_on(async {
        let certificate = TestCertificate::generate("access-claim");
        let port = free_port();
        let owner = OwnerNode::start_tls_on(
            "access-claim",
            &certificate.cert_path,
            &certificate.key_path,
            port,
        )
        .await;
        let begin = owner
            .admin(
                Method::NodePairBegin,
                json!({ "mode": "owner", "pairingUrl": null }),
            )
            .await;
        let pairing_id = begin["pairingId"].as_str().expect("pairingId").to_owned();
        let pairing_url = begin["pairingUrl"].as_str().expect("pairingUrl").to_owned();

        let root = TempRoot::new("access-claim-node");
        let data_dir = root.path().join("data");
        support::create_owner_only_dir(&data_dir);
        let access = AccessNode::start(&data_dir, &certificate.cert_path).await;

        // ① claim：state 是 pending_confirmation，peer 身份来自二维码（已验签 ownerProof）。
        let begun = access
            .manager
            .begin(&pairing_url, None)
            .await
            .expect("claim 必须成功");
        assert_eq!(begun["state"], json!("pending_confirmation"));
        assert_eq!(begun["pairingId"], json!(pairing_id));
        let owner_id =
            NodeId::new(begun["peerNodeId"].as_str().expect("peerNodeId")).expect("owner id");
        assert_eq!(owner_id.as_str(), owner.authority.local_node().as_str());
        let owner_fingerprint = owner
            .authority
            .node_public_key()
            .await
            .expect("owner 公钥")
            .fingerprint();
        assert_eq!(
            begun["peerPublicKeyFingerprint"],
            json!(owner_fingerprint.as_str())
        );
        assert_eq!(
            begun["pairingUrl"],
            json!(pairing_url),
            "access 回显输入的 URL"
        );

        // ② Owner 本机确认 → Access 轮询到 approved 时才落盘信任。
        owner
            .admin(Method::NodePairConfirm, json!({ "pairingId": pairing_id }))
            .await;
        let status = access
            .manager
            .status(&pairing_id)
            .await
            .expect("status 可读")
            .expect("该 pairingId 属于本实现");
        assert_eq!(status["state"], json!("approved"));
        assert_eq!(status["peerNodeId"], json!(owner_id.as_str()));

        let record = access
            .composition
            .use_cases()
            .node(&Actor::LocalCli, &owner_id, NodeKind::Owner)
            .await
            .expect("trust 可读")
            .expect("approved 之后必须有 Owner 信任记录");
        assert_eq!(record.state(), NodeState::Paired);
        assert_eq!(
            record.owner_endpoint(),
            Some(format!("wss://localhost:{port}/node-link/v1").as_str())
        );
        assert_eq!(record.node_public_key_fingerprint(), &owner_fingerprint);

        // ③ 落盘即建链：link 上线，远程端点可达；`daemon.status` 的 links 来源同一条事实。
        wait_until(|| access.manager.is_online(&owner_id)).await;
        assert_eq!(access.manager.links().len(), 1);
        let remote = RemoteSessionRef::new(
            owner_id.clone(),
            SessionId::new(SAMPLE_SESSION).expect("session id"),
        );
        assert!(
            access.manager.endpoint(&remote).is_some(),
            "在线 Owner 的远程会话必须暴露 SessionEndpoint"
        );

        // ④ 撤销：停止重连 + 路由立即失效（endpoint 由现行 link 构造，移除即撤销）。
        access.manager.revoke(&owner_id).await;
        assert!(!access.manager.is_online(&owner_id));
        assert!(access.manager.endpoint(&remote).is_none());
        assert!(access.manager.links().is_empty());

        access.stop().await;
        owner.stop().await;
    });
}

#[test]
fn a_restart_rebuilds_the_link_from_persisted_owner_trust() {
    support::block_on(async {
        let certificate = TestCertificate::generate("access-restart");
        let port = free_port();
        let owner = OwnerNode::start_tls_on(
            "access-restart",
            &certificate.cert_path,
            &certificate.key_path,
            port,
        )
        .await;
        let begin = owner
            .admin(
                Method::NodePairBegin,
                json!({ "mode": "owner", "pairingUrl": null }),
            )
            .await;
        let pairing_id = begin["pairingId"].as_str().expect("pairingId").to_owned();
        let pairing_url = begin["pairingUrl"].as_str().expect("pairingUrl").to_owned();

        let root = TempRoot::new("access-restart-node");
        let data_dir = root.path().join("data");
        support::create_owner_only_dir(&data_dir);
        let access = AccessNode::start(&data_dir, &certificate.cert_path).await;
        let begun = access
            .manager
            .begin(&pairing_url, None)
            .await
            .expect("claim 必须成功");
        let owner_id =
            NodeId::new(begun["peerNodeId"].as_str().expect("peerNodeId")).expect("owner id");
        owner
            .admin(Method::NodePairConfirm, json!({ "pairingId": pairing_id }))
            .await;
        let status = access
            .manager
            .status(&pairing_id)
            .await
            .expect("status")
            .expect("known");
        assert_eq!(status["state"], json!("approved"));
        wait_until(|| access.manager.is_online(&owner_id)).await;
        // 第一轮结束：停链并关闭组合根（存储检查点必须成功）。
        access.stop().await;

        // 第二轮：同一数据目录、新的身份（`ephemeral` keystore 是测试里的显式开发模式），
        // link 仍然按持久信任被实例化——「启动时按持久化 Access 信任创建管理器」的直接断言。
        let restarted = AccessNode::start(&data_dir, &certificate.cert_path).await;
        restarted.manager.start_persisted().await;
        let links = restarted.manager.links();
        assert_eq!(links.len(), 1, "{links:?}");
        assert_eq!(links[0].node_id, owner_id);
        let record = restarted
            .composition
            .use_cases()
            .node(&Actor::LocalCli, &owner_id, NodeKind::Owner)
            .await
            .expect("trust 可读")
            .expect("重启后信任仍在");
        assert_eq!(record.state(), NodeState::Paired);
        restarted.stop().await;
        owner.stop().await;
    });
}

#[test]
fn a_wrong_custom_ca_fails_the_claim_without_writing_trust() {
    support::block_on(async {
        let owner_certificate = TestCertificate::generate("access-wrong-ca");
        let other_certificate = TestCertificate::generate("access-other-ca");
        let port = free_port();
        let owner = OwnerNode::start_tls_on(
            "access-wrong-ca",
            &owner_certificate.cert_path,
            &owner_certificate.key_path,
            port,
        )
        .await;
        let begin = owner
            .admin(
                Method::NodePairBegin,
                json!({ "mode": "owner", "pairingUrl": null }),
            )
            .await;
        let pairing_url = begin["pairingUrl"].as_str().expect("pairingUrl").to_owned();

        let root = TempRoot::new("access-wrong-ca-node");
        let data_dir = root.path().join("data");
        support::create_owner_only_dir(&data_dir);
        let access = AccessNode::start(&data_dir, &other_certificate.cert_path).await;
        let error = access
            .manager
            .begin(&pairing_url, None)
            .await
            .expect_err("证书不受信任时 claim 必须失败");
        assert_eq!(error.code(), LocalErrorCode::RemoteError, "{error:?}");
        assert!(
            access
                .composition
                .use_cases()
                .nodes(&Actor::LocalCli)
                .await
                .expect("nodes 可读")
                .is_empty(),
            "失败的 claim 不得写任何信任记录"
        );
        access.stop().await;
        owner.stop().await;
    });
}

/// Owner 侧会话的 Agent 标题：`session.title.set` **不得**改写这一列。
const OWNER_AGENT_TITLE: &str = "Agent 标题";

#[test]
fn access_forwards_a_remote_title_and_persists_nothing_of_its_own() {
    support::block_on(async {
        let certificate = TestCertificate::generate("access-title");
        let port = free_port();
        let owner = OwnerNode::start_tls_on(
            "access-title",
            &certificate.cert_path,
            &certificate.key_path,
            port,
        )
        .await;
        let session = SessionId::new(SAMPLE_SESSION).expect("session id");
        // 会话先于配对存在于 Owner：Access 的 catalog 快照（建链时订阅一次）因此包含它。
        seed_owner_session(&owner.data_dir(), &session, OWNER_AGENT_TITLE).await;

        let root = TempRoot::new("access-title-node");
        let data_dir = root.path().join("data");
        support::create_owner_only_dir(&data_dir);
        let access = AccessNode::start(&data_dir, &certificate.cert_path).await;
        let owner_id = pair(&access, &owner).await;
        let runtime = access.manager.runtime();
        wait_for_remote_entry(&runtime, &session).await;

        // ① 首次转发：Access 用调用方给的 requestId 把标题交给 Owner。
        let request = RequestId::new(&support::nodelink::uuid_text()).expect("request id");
        let title = SessionTitle::parse("  排查 登录  ").expect("标题合法");
        assert_eq!(title.as_str(), "排查 登录", "值对象只保存 trim 后的文本");
        let record = access
            .composition
            .use_cases()
            .submit_remote_session_title(&Actor::LocalCli, &session, request.clone(), title.clone())
            .await
            .expect("Owner 在线且已协商 core.session-title.v1：转发必须成功");
        assert_eq!(record.request, request);
        assert_eq!(record.target.owner_node_id, owner_id);
        assert_eq!(record.target.session_id.as_ref(), Some(&session));
        assert_eq!(record.status, CommandStatus::Completed);
        assert!(
            record.accepted_at.is_none(),
            "command.terminal 不携带接受时间，Access 不得用本地时钟补齐"
        );
        let terminal_event = record
            .terminal_event_id
            .clone()
            .expect("Owner 必须给出终态事件标识");

        // ② Owner 的持久事实：只有自定义标题列变了。
        let mut owner_db = open_sqlite(&owner.data_dir()).await;
        let row = owner_session(&mut owner_db, &session).await;
        assert_eq!(
            row.custom_title.as_deref(),
            Some("排查 登录"),
            "owned_session.custom_title 必须落盘为 trim 后的标题"
        );
        assert_eq!(
            row.title.as_deref(),
            Some(OWNER_AGENT_TITLE),
            "Agent 标题列不得被自定义标题覆盖"
        );
        assert_eq!(row.version, 2, "一次标题修改推进一个版本");
        assert_eq!(
            owner_title_event_count(&mut owner_db, &session).await,
            1,
            "一次修改只追加一条 session.updated"
        );
        let payload = owner_last_title_event(&mut owner_db, &session).await;
        assert_eq!(payload["session"]["sessionId"], json!(session.as_str()));
        assert_eq!(
            payload["session"]["title"],
            json!("排查 登录"),
            "有效标题投影 = 自定义标题"
        );
        assert_eq!(payload["session"]["version"], json!("2"));
        let commands = owner_commands(&mut owner_db, &request).await;
        assert_eq!(commands.len(), 1, "Owner 为 (access, requestId) 只落一行");
        assert_eq!(commands[0].0, "session.title.set");
        assert_eq!(commands[0].1, "completed");
        assert_eq!(
            commands[0].2.as_deref(),
            Some(terminal_event.as_str()),
            "返回的终态事件标识必须是 Owner 持久记录里的那一个"
        );

        // ③ Access 的存储：零行，且没有 imported 索引族。
        let mut access_db = open_sqlite(&data_dir).await;
        assert_eq!(
            access_remote_rows(&mut access_db, &session, &request).await,
            (0, 0, 0),
            "Access 不得为远程标题写 owned_session/owned_command/owned_event"
        );
        assert_eq!(
            access_imported_tables(&mut access_db).await,
            0,
            "Access 的库里不得出现 imported_* 表（无导入索引）"
        );
        // 全表口径：本次转发之后 Access 的命令行与事件行**一行都没有**（没有收据、命令状态或
        // 转发进度的持久副本）。
        assert_eq!(
            table_rows(&mut access_db, "SELECT COUNT(*) FROM owned_command").await,
            0,
            "Access 不得落任何命令行"
        );
        assert_eq!(
            table_rows(&mut access_db, "SELECT COUNT(*) FROM owned_event").await,
            0,
            "Access 不得落任何事件行"
        );

        // ④ 同一 requestId 重放：回同一份 Owner 结果，且不重复修改、不追加事件。
        let replayed = access
            .composition
            .use_cases()
            .submit_remote_session_title(&Actor::LocalCli, &session, request.clone(), title.clone())
            .await
            .expect("同一 requestId 的重放必须返回既有结果");
        assert_eq!(replayed.status, CommandStatus::Completed);
        assert_eq!(replayed.request, request);
        assert_eq!(
            replayed.terminal_event_id,
            Some(terminal_event),
            "重放返回的是 Owner 的同一条终态记录"
        );
        assert_eq!(
            replayed.terminal_at, record.terminal_at,
            "重放返回 Owner 记录的同一个终态时刻"
        );
        let row = owner_session(&mut owner_db, &session).await;
        assert_eq!(row.version, 2, "重放不得推进版本");
        assert_eq!(row.custom_title.as_deref(), Some("排查 登录"));
        assert_eq!(
            owner_title_event_count(&mut owner_db, &session).await,
            1,
            "重放不得追加 session.updated"
        );
        assert_eq!(
            owner_commands(&mut owner_db, &request).await.len(),
            1,
            "重放不得新增命令行"
        );
        assert_eq!(
            access_remote_rows(&mut access_db, &session, &request).await,
            (0, 0, 0)
        );

        owner_db.close().await.expect("关闭 Owner 的原始连接");
        access_db.close().await.expect("关闭 Access 的原始连接");
        access.stop().await;
        // ⑤ 标题正文没有以任何形式落进 Access 的数据目录。
        let leaked = data_dir_contains(&data_dir, "排查 登录".as_bytes());
        assert!(
            leaked.is_empty(),
            "Access 的数据目录里出现了标题正文：{leaked:?}"
        );
        owner.stop().await;
    });
}

#[test]
fn owner_deletion_refreshes_the_access_catalog_without_persisting_remote_state() {
    support::block_on(async {
        let certificate = TestCertificate::generate("access-empty-delete");
        let port = free_port();
        let owner = OwnerNode::start_tls_on(
            "access-empty-delete",
            &certificate.cert_path,
            &certificate.key_path,
            port,
        )
        .await;
        let session = SessionId::new(SAMPLE_SESSION).expect("session id");
        seed_owner_session(&owner.data_dir(), &session, OWNER_AGENT_TITLE).await;

        let root = TempRoot::new("access-empty-delete-node");
        let data_dir = root.path().join("data");
        support::create_owner_only_dir(&data_dir);
        let access = AccessNode::start(&data_dir, &certificate.cert_path).await;
        let owner_id = pair(&access, &owner).await;
        let runtime = access.manager.runtime();
        wait_for_remote_entry(&runtime, &session).await;

        let request = RequestId::new(&support::nodelink::uuid_text()).expect("request id");
        let fingerprint = request_digest("access-empty-delete");
        assert!(matches!(
            owner
                .core
                .admit_session_delete(
                    &Actor::LocalCli,
                    request.clone(),
                    fingerprint.clone(),
                    session.clone(),
                )
                .await
                .expect("owner deletion admission"),
            acp_core::broker::SessionDeleteAdmission::Prepared
        ));
        let outcome = owner
            .core
            .delete_session(SessionDeletionWrite::new(
                session.clone(),
                Actor::LocalCli,
                request,
                fingerprint,
                Timestamp::new("2026-10-26T10:00:00.000Z").expect("timestamp"),
            ))
            .await
            .expect("owner deletion transaction");
        let deletion_event_id = match outcome {
            acp_core::model::SessionDeletionOutcome::Deleted { deletion_event, .. } => {
                deletion_event.id
            }
            other => panic!("the owner must commit the deletion, got {other:?}"),
        };

        wait_for_remote_absence(&runtime, &session).await;
        assert!(
            !runtime
                .observed_events()
                .iter()
                .any(|event_id| event_id == deletion_event_id.as_str()),
            "session.deleted must never cross Node Link as resource.event"
        );
        assert!(
            runtime.list().await.expect("runtime catalog").is_empty(),
            "the refreshed owner catalog must remove the tombstone"
        );
        assert!(
            access.manager.is_online(&owner_id),
            "catalog convergence must happen on the existing connection"
        );

        let later_request = RequestId::new(&support::nodelink::uuid_text()).expect("request id");
        let later_title = SessionTitle::parse("不应到达 Owner").expect("title");
        let error = access
            .composition
            .use_cases()
            .submit_remote_session_title(
                &Actor::LocalCli,
                &session,
                later_request.clone(),
                later_title,
            )
            .await
            .expect_err("a removed imported session must fail through normal not-found");
        assert!(
            matches!(&error, PortError::NotFound(_) | PortError::Unavailable(_)),
            "the command must fail explicitly without rebuilding the source: {error:?}"
        );

        let mut access_db = open_sqlite(&data_dir).await;
        assert_eq!(
            access_remote_rows(&mut access_db, &session, &later_request).await,
            (0, 0, 0),
            "Access must not persist a remote deletion, command, or mirrored event"
        );
        assert_eq!(
            access_imported_tables(&mut access_db).await,
            0,
            "Access must not create an imported deletion index"
        );
        access_db.close().await.expect("close Access SQLite");

        let leaked = data_dir_contains(&data_dir, session.as_str().as_bytes());
        assert!(
            leaked.is_empty(),
            "the remote session id must remain runtime-only in Access storage: {leaked:?}"
        );

        access.stop().await;
        owner.stop().await;
    });
}

#[test]
fn a_restarted_access_keeps_setting_the_remote_title_from_persisted_trust() {
    support::block_on(async {
        let certificate = TestCertificate::generate("access-title-restart");
        let port = free_port();
        let owner = OwnerNode::start_tls_on(
            "access-title-restart",
            &certificate.cert_path,
            &certificate.key_path,
            port,
        )
        .await;
        let session = SessionId::new(SAMPLE_SESSION).expect("session id");
        seed_owner_session(&owner.data_dir(), &session, OWNER_AGENT_TITLE).await;

        let root = TempRoot::new("access-title-restart-node");
        let data_dir = root.path().join("data");
        support::create_owner_only_dir(&data_dir);
        let access = AccessNode::start(&data_dir, &certificate.cert_path).await;
        let owner_id = pair(&access, &owner).await;
        wait_for_remote_entry(&access.manager.runtime(), &session).await;

        let first_request = RequestId::new(&support::nodelink::uuid_text()).expect("request id");
        let first_title = SessionTitle::parse("第一轮 标题").expect("标题合法");
        let first = access
            .composition
            .use_cases()
            .submit_remote_session_title(
                &Actor::LocalCli,
                &session,
                first_request.clone(),
                first_title,
            )
            .await
            .expect("第一轮必须成功");
        assert_eq!(first.status, CommandStatus::Completed);

        // 第一轮结束：关闭组合根与出站连接，只把进程身份交给下一轮。
        let identity = access.stop_keeping_identity().await;

        // 第二轮：同一 data_dir、新的组合根/storage 连接/RemoteRuntime/链接实例。
        let restarted =
            AccessNode::start_with_identity(&data_dir, &certificate.cert_path, Some(identity))
                .await;
        // 生产启动序列的动作：按持久化 Owner 信任实例化出站连接（不依赖任何内存残留）。
        restarted.manager.start_persisted().await;
        wait_until(|| restarted.manager.is_online(&owner_id)).await;
        let links = restarted.manager.links();
        assert_eq!(links.len(), 1, "{links:?}");
        assert_eq!(
            links[0].node_id, owner_id,
            "只按持久信任记录重建该 Owner 的连接"
        );
        wait_for_remote_entry(&restarted.manager.runtime(), &session).await;

        let second_request = RequestId::new(&support::nodelink::uuid_text()).expect("request id");
        let second_title = SessionTitle::parse("重启后的 标题").expect("标题合法");
        let second = restarted
            .composition
            .use_cases()
            .submit_remote_session_title(
                &Actor::LocalCli,
                &session,
                second_request.clone(),
                second_title,
            )
            .await
            .expect("重启后的 Access 仍能修改该远程标题");
        assert_eq!(second.status, CommandStatus::Completed);
        assert_ne!(
            second.terminal_event_id, first.terminal_event_id,
            "不同的 requestId 是不同的命令"
        );

        let mut owner_db = open_sqlite(&owner.data_dir()).await;
        let row = owner_session(&mut owner_db, &session).await;
        assert_eq!(row.custom_title.as_deref(), Some("重启后的 标题"));
        assert_eq!(
            row.title.as_deref(),
            Some(OWNER_AGENT_TITLE),
            "第二次修改同样不得碰 Agent 标题列"
        );
        assert_eq!(row.version, 3, "重启后的修改是第二次推进");
        assert_eq!(
            owner_title_event_count(&mut owner_db, &session).await,
            2,
            "两轮各追加一条 session.updated"
        );
        let payload = owner_last_title_event(&mut owner_db, &session).await;
        assert_eq!(payload["session"]["title"], json!("重启后的 标题"));
        assert_eq!(payload["session"]["version"], json!("3"));
        assert_eq!(owner_commands(&mut owner_db, &first_request).await.len(), 1);
        assert_eq!(
            owner_commands(&mut owner_db, &second_request).await.len(),
            1
        );

        // 重启也没有在 Access 侧留下任何行：运行期数据只活在新连接上。
        let mut access_db = open_sqlite(&data_dir).await;
        assert_eq!(
            access_remote_rows(&mut access_db, &session, &first_request).await,
            (0, 0, 0)
        );
        assert_eq!(
            access_remote_rows(&mut access_db, &session, &second_request).await,
            (0, 0, 0)
        );
        assert_eq!(access_imported_tables(&mut access_db).await, 0);

        owner_db.close().await.expect("关闭 Owner 的原始连接");
        access_db.close().await.expect("关闭 Access 的原始连接");
        restarted.stop().await;
        owner.stop().await;
    });
}

/// 重启前后的固定身份种子：`EphemeralKeystore` 只活在进程内，不固定种子重启就会换 node id
/// （`session_activity_cross_layer` 的重启用例同一手法）。
fn restart_seed() -> [u8; 32] {
    let mut seed = [0u8; 32];
    seed[31] = 7;
    seed
}

/// 请求指纹（`Digest`：32 字节摘要的无填充 base64url）。
fn request_digest(label: &str) -> Digest {
    use base64::Engine as _;
    use sha2::Digest as _;
    let mut hasher = sha2::Sha256::new();
    hasher.update(label.as_bytes());
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hasher.finalize());
    Digest::new(&encoded).expect("32 字节摘要")
}

/// 本机 owned 会话的 `session.title.set` 提交（与 Node Link 适配器为同一条命令构造的同一形状：
/// `Mutation`、`sessionId` 必填、`expectedVersion` 缺席）。
fn title_set_command(session: &SessionId, request: RequestId, title: &str) -> ClientCommand {
    let fingerprint = format!("owner-title-restart-{}", request.as_str());
    ClientCommand {
        actor: Actor::LocalCli,
        request,
        command: "session.title.set".to_owned(),
        kind: CommandKind::Mutation,
        session: Some(session.clone()),
        expected_version: None,
        request_fingerprint: request_digest(&fingerprint),
        payload: CommandPayload::TitleSet {
            title: SessionTitle::parse(title).expect("标题合法"),
        },
    }
}

/// Owner 进程重启（同一 `data_dir` + 同一身份种子）后，已提交的自定义标题仍是权威值。
///
/// 覆盖 `session-custom-title`「保存后多端重连」里**所属节点重启**的一半：`custom_title` 由新起的存储
/// 连接从库里读回、Agent `title` 列不变、有效标题（`load` 的聚合与 `session.list` 的投影）仍是自定义
/// 值，并且重启既不重放 `session.updated`，也不重新执行同一个 `requestId`。
///
/// 判别力：若标题只活在进程内（或加载把有效标题写回 Agent 列），重启后的 `custom_title`/`title`/
/// `effective_title`/列表 `title` 断言红；若重启路径重放事件或重执行命令，`session.updated` 计数、
/// 事件载荷或 `owned_command` 行数红。
#[test]
fn an_owner_restart_keeps_the_committed_custom_title_without_replaying_the_update() {
    support::block_on(async {
        // 临时根的所有权留在用例：两轮 Owner 共用同一个 `data_dir`（重启而不是新建）。
        let root = TempRoot::new("owner-title-restart");
        let seed = restart_seed();
        let first = OwnerNode::start_in_root(root.path(), seed).await;
        let node_id = first.owner_node_id().clone();
        let session = SessionId::new(SAMPLE_SESSION).expect("session id");
        seed_owner_session(&first.data_dir(), &session, OWNER_AGENT_TITLE).await;

        let request = RequestId::new(&support::nodelink::uuid_text()).expect("request id");
        let receipt = first
            .core
            .set_session_title(
                &Actor::LocalCli,
                title_set_command(&session, request.clone(), "  重启后保留  "),
            )
            .await
            .expect("本机 owned 会话的标题写入必须成功");
        match receipt {
            CommandReceipt::Accepted {
                request: accepted,
                turn,
            } => {
                assert_eq!(accepted, request, "回执的 requestId 必须等于请求");
                assert!(turn.is_none(), "标题写入不建 turn");
            }
            CommandReceipt::Rejected { error } => panic!("标题写入被拒绝：{error:?}"),
        }

        // 第一轮的持久事实：trim 后的值落在自定义列，Agent 列原样，且只追加一条事件。
        let mut db = open_sqlite(&first.data_dir()).await;
        let before = owner_session(&mut db, &session).await;
        assert_eq!(before.custom_title.as_deref(), Some("重启后保留"));
        assert_eq!(before.title.as_deref(), Some(OWNER_AGENT_TITLE));
        assert_eq!(before.version, 2, "一次标题修改推进一个版本");
        assert_eq!(owner_title_event_count(&mut db, &session).await, 1);
        let payload = owner_last_title_event(&mut db, &session).await;
        db.close().await.expect("关闭第一轮的原始连接");

        // 重启：关掉第一轮的进程，在同一 data_dir / 同一身份种子上再起一个 Owner。
        first.stop().await;
        let restarted = OwnerNode::start_in_root(root.path(), seed).await;
        assert_eq!(restarted.owner_node_id(), &node_id, "重启必须是同一个节点");

        // ① 产品读路径（新起的存储连接）读回持久化事实；有效标题仍取自定义值。
        let snapshot = restarted
            .sessions
            .load(&session)
            .await
            .expect("读取会话")
            .expect("会话行必须仍在");
        assert_eq!(
            snapshot.session.custom_title(),
            Some("重启后保留"),
            "已提交的自定义标题必须从库里读回"
        );
        assert_eq!(
            snapshot.session.title(),
            Some(OWNER_AGENT_TITLE),
            "重启不得把自定义标题写回 Agent 标题列"
        );
        assert_eq!(
            snapshot.session.effective_title(),
            Some("重启后保留"),
            "自定义标题仍是展示用的有效标题"
        );
        assert_eq!(snapshot.session.version().get(), 2, "重启不得推进版本");

        // ② 列表投影：消费方看到的 `title` 是有效标题（自定义优先），取值仍来自库里的自定义列。
        let listed = restarted
            .core
            .list_sessions(
                &Actor::LocalCli,
                SessionQuery {
                    only: Some(vec![session.clone()]),
                    ..SessionQuery::default()
                },
            )
            .await
            .expect("会话列表可读");
        assert_eq!(listed.len(), 1, "重启后该会话仍在列表里");
        assert_eq!(
            listed[0].title(),
            Some("重启后保留"),
            "列表投影的有效标题必须是自定义值（不是 Agent 标题）"
        );
        assert_eq!(listed[0].version().get(), 2);

        // ③ 原始行：两列与版本逐字保留；重启没有重放事件，也没有重执行同一 requestId。
        let mut db = open_sqlite(&restarted.data_dir()).await;
        let after = owner_session(&mut db, &session).await;
        assert_eq!(after.custom_title.as_deref(), Some("重启后保留"));
        assert_eq!(after.title.as_deref(), Some(OWNER_AGENT_TITLE));
        assert_eq!(after.version, 2, "重启不得推进版本");
        assert_eq!(
            owner_title_event_count(&mut db, &session).await,
            1,
            "重启不得重放 session.updated"
        );
        assert_eq!(
            owner_last_title_event(&mut db, &session).await,
            payload,
            "已有事件的载荷原样保留（重启不重写）"
        );
        let commands = owner_commands(&mut db, &request).await;
        assert_eq!(commands.len(), 1, "重启不得为同一 requestId 再落一行");
        assert_eq!(commands[0].0, "session.title.set");
        assert_eq!(commands[0].1, "completed");

        db.close().await.expect("关闭重启后的原始连接");
        restarted.stop().await;
    });
}

#[test]
fn an_unreachable_owner_fails_the_remote_title_without_any_local_write() {
    support::block_on(async {
        let certificate = TestCertificate::generate("access-title-offline");
        let port = free_port();
        let owner = OwnerNode::start_tls_on(
            "access-title-offline",
            &certificate.cert_path,
            &certificate.key_path,
            port,
        )
        .await;
        let session = SessionId::new(SAMPLE_SESSION).expect("session id");
        seed_owner_session(&owner.data_dir(), &session, OWNER_AGENT_TITLE).await;

        let root = TempRoot::new("access-title-offline-node");
        let data_dir = root.path().join("data");
        support::create_owner_only_dir(&data_dir);
        let access = AccessNode::start(&data_dir, &certificate.cert_path).await;
        let owner_id = pair(&access, &owner).await;
        wait_for_remote_entry(&access.manager.runtime(), &session).await;
        // 前置：来源确实在线且目录里有该会话，否则「离线」这条判定就是空转。
        assert!(access.manager.is_online(&owner_id));
        assert_eq!(
            access
                .manager
                .runtime()
                .list()
                .await
                .expect("运行期目录可读")
                .len(),
            1
        );

        // 停掉 Access 的出站 link（等价于 Owner 不再可达，但不必等 90 s 静默窗口）：
        // 运行期目录随代际一起清除，此后没有可用的来源。
        access.manager.shutdown_all().await;
        wait_until(|| !access.manager.is_online(&owner_id)).await;
        assert!(
            access
                .manager
                .runtime()
                .list()
                .await
                .expect("运行期目录可读")
                .is_empty(),
            "断线即整代清除：不得留下可用的远程目录条目"
        );

        let request = RequestId::new(&support::nodelink::uuid_text()).expect("request id");
        let title = SessionTitle::parse("离线时不该落盘").expect("标题合法");
        let error = access
            .composition
            .use_cases()
            .submit_remote_session_title(&Actor::LocalCli, &session, request.clone(), title)
            .await
            .expect_err("没有在线来源时不得返回成功");
        assert!(
            matches!(&error, PortError::NotFound(_) | PortError::Unavailable(_)),
            "故障必须显式分类（不是成功、也不是参数错误）：{error:?}"
        );

        // Access：零本地行（不写标题、命令、收据或索引）。
        let mut access_db = open_sqlite(&data_dir).await;
        assert_eq!(
            access_remote_rows(&mut access_db, &session, &request).await,
            (0, 0, 0),
            "失败的命令不得在 Access 留下任何行"
        );
        assert_eq!(access_imported_tables(&mut access_db).await, 0);

        // Owner：命令根本没有到达（没有命令行、没有事件、标题与版本不变）。
        let mut owner_db = open_sqlite(&owner.data_dir()).await;
        let row = owner_session(&mut owner_db, &session).await;
        assert!(
            row.custom_title.is_none(),
            "失败的转发不得修改 Owner 的标题"
        );
        assert_eq!(row.version, 1, "失败的转发不得推进版本");
        assert_eq!(
            owner_title_event_count(&mut owner_db, &session).await,
            0,
            "失败的转发不得追加 session.updated"
        );
        assert!(
            owner_commands(&mut owner_db, &request).await.is_empty(),
            "失败的转发不得在 Owner 落命令行"
        );

        owner_db.close().await.expect("关闭 Owner 的原始连接");
        access_db.close().await.expect("关闭 Access 的原始连接");
        // 先关 Access（link 已停，Owner 的接入任务随之结束），再关 Owner。
        access.stop().await;
        owner.stop().await;
    });
}

/// 完成一次真实的配对（Owner 本机确认之后 Access 才落盘信任）并等待出站 link 上线。
async fn pair(access: &AccessNode, owner: &OwnerNode) -> NodeId {
    let begin = owner
        .admin(
            Method::NodePairBegin,
            json!({ "mode": "owner", "pairingUrl": null }),
        )
        .await;
    let pairing_id = begin["pairingId"].as_str().expect("pairingId").to_owned();
    let pairing_url = begin["pairingUrl"].as_str().expect("pairingUrl").to_owned();
    let begun = access
        .manager
        .begin(&pairing_url, None)
        .await
        .expect("claim 必须成功");
    let owner_id =
        NodeId::new(begun["peerNodeId"].as_str().expect("peerNodeId")).expect("owner id");
    owner
        .admin(Method::NodePairConfirm, json!({ "pairingId": pairing_id }))
        .await;
    let status = access
        .manager
        .status(&pairing_id)
        .await
        .expect("status 可读")
        .expect("该 pairingId 属于本实现");
    assert_eq!(status["state"], json!("approved"), "Owner 确认后才落盘信任");
    wait_until(|| access.manager.is_online(&owner_id)).await;
    owner_id
}

/// 轮询到运行期目录里出现该远程会话（建链后 `catalog.subscribe` 的快照到达；上限 10 s）。
async fn wait_for_remote_entry(runtime: &node_link_client::RemoteRuntime, session: &SessionId) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let entries = runtime
            .find_by_session(session)
            .await
            .expect("运行期目录可读");
        if !entries.is_empty() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "远程目录未在 10 s 内出现会话 {}",
            session.as_str()
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn wait_for_remote_absence(runtime: &node_link_client::RemoteRuntime, session: &SessionId) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let entries = runtime
            .find_by_session(session)
            .await
            .expect("runtime catalog");
        if entries.is_empty() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "remote catalog did not converge after 10 s for {}",
            session.as_str()
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// 打开一条指向 `data_dir` 主库的原始 SQL 连接（只用于断言持久事实，产品代码不读它）。
async fn open_sqlite(data_dir: &Path) -> sqlx::SqliteConnection {
    let storage = storage_sqlite::migrate::StorageConfig::new(data_dir.to_path_buf());
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(storage.database_path())
        .create_if_missing(false)
        .busy_timeout(Duration::from_secs(10));
    sqlx::Connection::connect_with(&options)
        .await
        .expect("打开 data_dir 的原始 SQL 连接")
}

/// 直接在 Owner 的库里写一条 owned 会话行（`title` 是 Agent 标题，`custom_title` 留空）。
///
/// 用 SQL 而不是 `session.create`：被测面是**标题转发**，会话创建路径不参与；直写行让「Agent 标题列
/// 不被覆盖」有一个确定的初始值，也不必先拉起脚本化的 Agent 会话。
async fn seed_owner_session(data_dir: &Path, session: &SessionId, agent_title: &str) {
    let mut connection = open_sqlite(data_dir).await;
    let inserted = sqlx::query(
        "INSERT INTO owned_session (session_id, title, agent_id, agent_name, state, origin_epoch, \
         version, created_at, updated_at) VALUES (?1, ?2, 'codex', 'Codex', 'idle', ?3, 1, ?4, ?4)",
    )
    .bind(session.as_str())
    .bind(agent_title)
    .bind(support::nodelink::uuid_text())
    .bind("2026-10-21T09:00:00.000Z")
    .execute(&mut connection)
    .await
    .expect("写入 Owner 的 owned 会话行")
    .rows_affected();
    assert_eq!(inserted, 1, "会话行必须恰好写入一行");
    connection.close().await.expect("关闭原始 SQL 连接");
}

/// `owned_session` 的标题/版本三列：`title` 是 Agent 标题，`custom_title` 是用户覆盖值。
struct OwnerSessionRow {
    title: Option<String>,
    custom_title: Option<String>,
    version: i64,
}

async fn owner_session(
    connection: &mut sqlx::SqliteConnection,
    session: &SessionId,
) -> OwnerSessionRow {
    let row =
        sqlx::query("SELECT title, custom_title, version FROM owned_session WHERE session_id = ?1")
            .bind(session.as_str())
            .fetch_one(&mut *connection)
            .await
            .expect("读取 Owner 的 owned 会话行");
    OwnerSessionRow {
        title: row.get("title"),
        custom_title: row.get("custom_title"),
        version: row.get("version"),
    }
}

async fn owner_title_event_count(
    connection: &mut sqlx::SqliteConnection,
    session: &SessionId,
) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM owned_event WHERE session_id = ?1 AND event_type = 'session.updated'",
    )
    .bind(session.as_str())
    .fetch_one(&mut *connection)
    .await
    .expect("统计 Owner 的 session.updated")
}

async fn owner_last_title_event(
    connection: &mut sqlx::SqliteConnection,
    session: &SessionId,
) -> serde_json::Value {
    let payload: String = sqlx::query_scalar(
        "SELECT payload_json FROM owned_event WHERE session_id = ?1 \
         AND event_type = 'session.updated' ORDER BY global_sequence DESC LIMIT 1",
    )
    .bind(session.as_str())
    .fetch_one(&mut *connection)
    .await
    .expect("Owner 必须有一条 session.updated");
    serde_json::from_str(&payload).expect("事件 payload 是 JSON")
}

/// Owner 为某个 `requestId` 落下的命令行：`(command, status, terminalEventId)`。
async fn owner_commands(
    connection: &mut sqlx::SqliteConnection,
    request: &RequestId,
) -> Vec<(String, String, Option<String>)> {
    sqlx::query_as::<_, (String, String, Option<String>)>(
        "SELECT command, status, terminal_event_id FROM owned_command WHERE request_id = ?1",
    )
    .bind(request.as_str())
    .fetch_all(&mut *connection)
    .await
    .expect("读取 Owner 的 owned_command")
}

/// Access 库里与一次远程标题请求相关的行数：`(owned_session, owned_command, owned_event)`。
///
/// 三个计数都必须为零：Access 不持久保存远程标题、命令状态，也不把远程事件投影成本地事件。
async fn access_remote_rows(
    connection: &mut sqlx::SqliteConnection,
    session: &SessionId,
    request: &RequestId,
) -> (i64, i64, i64) {
    let sessions =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM owned_session WHERE session_id = ?1")
            .bind(session.as_str())
            .fetch_one(&mut *connection)
            .await
            .expect("统计 owned_session");
    let commands =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM owned_command WHERE request_id = ?1")
            .bind(request.as_str())
            .fetch_one(&mut *connection)
            .await
            .expect("统计 owned_command");
    let events =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM owned_event WHERE session_id = ?1")
            .bind(session.as_str())
            .fetch_one(&mut *connection)
            .await
            .expect("统计 owned_event");
    (sessions, commands, events)
}

/// 一张表的行数（`sql` 是常量 COUNT 查询；只用于测试的「零行」断言）。
async fn table_rows(connection: &mut sqlx::SqliteConnection, sql: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(sql)
        .fetch_one(&mut *connection)
        .await
        .expect("统计表行数")
}

/// Access 库里 `imported_*` 表的数量（无导入索引）。
async fn access_imported_tables(connection: &mut sqlx::SqliteConnection) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name LIKE 'imported%'",
    )
    .fetch_one(&mut *connection)
    .await
    .expect("统计 sqlite_master")
}

/// 数据目录里是否存在给定字节序列；返回命中的文件（「Access 不持久保存标题」的直接证据）。
fn data_dir_contains(directory: &Path, needle: &[u8]) -> Vec<PathBuf> {
    assert!(!needle.is_empty(), "被查找的字节序列不能为空");
    let mut found = Vec::new();
    let mut pending = vec![directory.to_path_buf()];
    while let Some(next) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&next) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.is_dir() {
                pending.push(path);
                continue;
            }
            if let Ok(bytes) = std::fs::read(&path)
                && bytes.windows(needle.len()).any(|window| window == needle)
            {
                found.push(path);
            }
        }
    }
    found
}

/// Access 侧组合根（真实存储 + 真实身份 + `AccessManager`）。
struct AccessNode {
    composition: Composition,
    manager: Arc<AccessManager>,
    /// 本次实例使用的进程身份（重启用例把它交给下一轮）。
    identity: AccessIdentity,
    /// 发布端的接收端：必须活到关闭，否则 imported 事件的扇出会丢（本测试不消费也无妨）。
    _events: tokio::sync::mpsc::Receiver<acp_core::model::CommittedEvent>,
}

/// 跨实例带走的 Access 进程身份。
///
/// 生产里身份由平台 keystore 持久化（重启后仍是同一 node id 与公钥）；本测试配置用 `ephemeral`
/// keystore（CI 的 Linux runner 没有平台后端），它只在进程内生成条目。因此「重新启动 Access」的
/// 用例把第一轮的 `NodeIdentity` 与 keystore 交给第二轮，只替代「身份持久化」这一项生产事实：
/// SQLite（重开）、`AccessManager`、`RemoteRuntime` 与全部出站连接都是新实例。
struct AccessIdentity {
    identity: app::NodeIdentity,
    keystore: Arc<dyn identity_auth::IdentityKeystore>,
}

impl AccessNode {
    async fn start(data_dir: &Path, ca: &Path) -> Self {
        Self::start_with_identity(data_dir, ca, None).await
    }

    /// 用给定的进程身份起一轮 Access（`None` = 本轮的 `Composition` 现生成身份）。
    async fn start_with_identity(
        data_dir: &Path,
        ca: &Path,
        identity: Option<AccessIdentity>,
    ) -> Self {
        assert!(data_dir.is_absolute(), "data_dir 必须是绝对路径");
        let config = app::Config::from_toml(&format!(
            concat!(
                "[daemon]\ndata_dir = \"{}\"\nport = 0\nbind = \"127.0.0.1\"\n",
                "[dev_mode]\nenabled = true\n",
                "[identity]\nkeystore = \"ephemeral\"\n",
                "[node_link.access]\ntrust_roots = [\"{}\"]\n",
            ),
            data_dir.display().to_string().replace('\\', "/"),
            ca.display().to_string().replace('\\', "/"),
        ))
        .expect("测试配置合法");
        let (publisher, events) = app::compose::forked_publisher();
        let composition = Composition::assemble(config, publisher)
            .await
            .expect("组合根装配");
        let identity = identity.unwrap_or_else(|| AccessIdentity {
            identity: composition.identity().clone(),
            keystore: Arc::clone(composition.keystore()),
        });
        let trust = OutboundTrust::load(composition.config()).expect("信任锚");
        let manager = Arc::new(
            AccessManager::new(
                AccessDeps {
                    identity: identity.identity.clone(),
                    keystore: Arc::clone(&identity.keystore),
                    core: Arc::clone(composition.use_cases()),
                    runtime: node_link_client::RemoteRuntime::new(),
                    clock: Arc::clone(composition.clock()),
                    entropy: Arc::new(identity_keystore::OsEntropy::new()),
                    node_name: composition.config().node_name.clone(),
                },
                trust,
            )
            .expect("AccessManager"),
        );
        // 组合根接线：`with_access` 把**同一个** `AccessManager`（因此同一个 `RemoteRuntime` 实例）
        // 接成 core 的远程目录/回源读取/命令查询/标题写出四个端口。少了这一步，`UseCases` 的远程
        // 面全部不可用（`Unavailable(BackendUnsupported)`），Access 就只是「连上了但不转发」。
        let composition = composition.with_access(Arc::clone(&manager));
        Self {
            composition,
            manager,
            identity,
            _events: events,
        }
    }

    /// 关闭组合根并交回进程身份（重启用例用同一身份起下一轮）。
    async fn stop_keeping_identity(self) -> AccessIdentity {
        let Self {
            composition,
            manager,
            identity,
            _events,
        } = self;
        manager.shutdown_all().await;
        drop(manager);
        composition.close().await.expect("关闭序列");
        identity
    }

    async fn stop(self) {
        let _ = self.stop_keeping_identity().await;
    }
}

/// 自签证书（`localhost` + `127.0.0.1`），仓库不提交任何私钥材料。
struct TestCertificate {
    directory: PathBuf,
    cert_path: PathBuf,
    key_path: PathBuf,
}

impl TestCertificate {
    fn generate(label: &str) -> Self {
        let directory = std::env::temp_dir().join(format!(
            "acpr-access-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or_default()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("创建临时目录");
        let certified = rcgen::generate_simple_self_signed(vec![
            "localhost".to_owned(),
            "127.0.0.1".to_owned(),
        ])
        .expect("自签证书");
        let cert_path = directory.join("cert.pem");
        let key_path = directory.join("key.pem");
        std::fs::write(&cert_path, certified.cert.pem()).expect("写证书");
        std::fs::write(&key_path, certified.signing_key.serialize_pem()).expect("写私钥");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&cert_path, std::fs::Permissions::from_mode(0o600))
                .expect("证书权限");
            std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600))
                .expect("私钥权限");
        }
        Self {
            directory,
            cert_path,
            key_path,
        }
    }
}

impl Drop for TestCertificate {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

/// 一个当前空闲的 loopback 端口（绑定后立即释放；测试串行使用，不与之竞争）。
fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("绑定临时端口");
    listener.local_addr().expect("本地地址").port()
}

/// 轮询直到条件成立（上限 10 s；失败即 panic，避免用例挂住）。
async fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if condition() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("条件未在 10 s 内成立");
}
