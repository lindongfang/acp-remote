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

mod support;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use acp_core::model::{Actor, NodeId, NodeKind, NodeState, RemoteSessionRef, SessionId};
use app::access::{AccessDeps, AccessManager};
use app::compose::Composition;
use app::trust::OutboundTrust;
use serde_json::json;
use server::local_admin::Method;
use server::local_admin::{AccessPairing, LocalErrorCode};

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

/// Access 侧组合根（真实存储 + 真实身份 + `AccessManager`）。
struct AccessNode {
    composition: Composition,
    manager: std::sync::Arc<AccessManager>,
    /// 发布端的接收端：必须活到关闭，否则 imported 事件的扇出会丢（本测试不消费也无妨）。
    _events: tokio::sync::mpsc::Receiver<acp_core::model::CommittedEvent>,
}

impl AccessNode {
    async fn start(data_dir: &Path, ca: &Path) -> Self {
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
        let trust = OutboundTrust::load(composition.config()).expect("信任锚");
        let manager = std::sync::Arc::new(
            AccessManager::new(
                AccessDeps {
                    identity: composition.identity().clone(),
                    keystore: std::sync::Arc::clone(composition.keystore()),
                    core: std::sync::Arc::clone(composition.use_cases()),
                    broker: std::sync::Arc::clone(composition.broker()),
                    deliveries: composition.deliveries(),
                    clock: std::sync::Arc::clone(composition.clock()),
                    entropy: std::sync::Arc::new(identity_keystore::OsEntropy::new()),
                    node_name: composition.config().node_name.clone(),
                },
                trust,
            )
            .expect("AccessManager"),
        );
        Self {
            composition,
            manager,
            _events: events,
        }
    }

    async fn stop(self) {
        self.manager.shutdown_all().await;
        let Self {
            composition,
            manager,
            _events,
        } = self;
        drop(manager);
        composition.close().await.expect("关闭序列");
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
