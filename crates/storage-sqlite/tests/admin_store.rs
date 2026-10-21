//! WP6：管理 store（`TrustStore`/`LocalConfigStore`）与失败关闭的行为用例。
//!
//! 逐条对应 `openspec/changes/archive/2026-09-23-admin-state-persistence-v2/specs/` 的场景：
//!
//! | 规格 | 用例 |
//! |---|---|
//! | admin-state-persistence：配对认领与落定的单事务语义 | `second_claim_of_the_same_pairing_is_rejected`、`rejected_settlement_creates_no_trust_and_ends_the_pairing`、`expired_pairing_...` |
//! | admin-state-persistence：管理写集的原子性与失败关闭 | `a_failed_audit_write_rolls_back_the_whole_write_set`、`a_constraint_failure_during_approval_leaves_no_half_authorization` |
//! | admin-state-persistence：撤销与重启恢复 | `device_revocation_is_idempotent_and_survives_reopen`、`node_revocation_covers_both_roles_and_survives_reopen`、`expired_pairing_...` |
//! | admin-state-persistence：Export 与 Import 的归属与完整移除 | `import_ownership_is_exclusive_...`、`full_import_removal_and_connection_drop_both_keep_audit` |
//! | admin-state-persistence：撤销与删除后的写入不得复活资源 | `export_revocation_is_terminal_for_plain_writes`；完整移除后的迟到回调见 `imported.rs` 的 `late_callbacks_after_a_full_removal_cannot_rebuild_the_index` |
//! | admin-state-persistence：设备与节点记录的活动时间只前进 | `timestamps_are_advanced_never_erased` |
//! | peer-identity-material：配对携带公钥并绑定到信任材料 | `pairing_approval_persists_identity_material_across_reopen` |
//! | peer-identity-material：双角色身份一致与撤销覆盖 | `node_roles_share_one_identity_material`、`second_node_role_with_a_foreign_fingerprint_is_rejected`、`node_revocation_...` |
//! | peer-identity-material：身份变化不自动接受 | `revoked_or_rekeyed_device_cannot_be_reactivated_or_rebound` |
//! | peer-identity-material：身份材料读取必须核对同行指纹 | `corrupted_identity_material_fails_closed_on_read` |
//! | peer-identity-material：凭据与身份材料的存放边界 | `no_secret_material_lands_in_any_column` |
//! | local-agent-config：Profile 写入校验与唯一默认 | `default_profile_switch_is_atomic_and_unique`、`profile_bindings_must_reference_registered_provider_fields` |
//! | local-agent-config：首次初始化种子的幂等 | `seed_marks_initialized_once_and_ignores_later_seeds` |
//! | local-agent-config：Provider 引用的无凭据与失效关闭 | `provider_reference_version_must_advance` |
//! | local-agent-config：workspace 记录的本机归属 | `workspace_records_stay_local_and_keep_created_at` |
//! | storage-schema-v2-migration：损坏与权限不符时的失败关闭 | `fail_closed_store_rejects_every_admin_write_path` |
//! | storage-schema-v2-migration：管理表纳入保留与容量 | `capacity_limit_refuses_new_admin_writes_without_deleting_trust` |
//! | storage-schema-v2-migration：imported 家族保持无正文 | `full_import_removal_and_connection_drop_both_keep_audit` |
//! | node-link-owner：认证收尾写集的 `last_seen` 落在存储层 | `record_node_connected_advances_the_access_row_once_forward`、`consume_pairing_advances_the_node_row_of_a_node_peer`、`record_node_connected_refuses_missing_rows_and_wrong_roles`、`a_failed_audit_write_leaves_last_connected_at_untouched`（这一行来自 `node-link-owner` 的 spec「节点双向认证与凭据状态」，不属 admin-state-persistence-v2 的场景表） |
//!
//! 断言只针对**可观察结果**：端口返回值、库内行（`table_snapshot` 的逐行文本）与黄金列清单。
//! 两条故障注入用 SQLite 触发器实现（审计表/设备表上 `RAISE(ABORT)`）：它们是「事务中途失败」在
//! 存储边界上最真实的注入点，能验证整事务回滚而不是某个分支的返回值。

mod support;

use std::path::Path;

use sqlx::SqlitePool;

use acp_core::model::{
    Actor, AuditAction, AuditOutcome, ConflictKind, DeviceId, DeviceRecord, DeviceState, Digest,
    DirectoryId, EntityRef, Fingerprint, NodeId, NodeKind, NodeRecord, NodeState, Nonce, PairingId,
    PairingPeer, PairingRecord, PairingSettlement, PairingState, PairingTarget, PeerIdentity,
    PeerPublicKey, PortError, Timestamp, UnavailableKind,
};
use acp_core::ports::{
    DeviceRevocation, DeviceWrite, ExpiryWrite, LocalConfigStore, NodeConnectedWrite,
    NodeRevocation, NodeWrite, PairingClaimWrite, PairingConsumption, PairingSettlementWrite,
    PairingWrite, PendingAudit, RevokeReason, SessionStore, TrustRecordRef, TrustStore,
    WriteContext,
};
use storage_sqlite::error::StorageError;
use storage_sqlite::migrate::StorageConfig;
use storage_sqlite::session_store::SqliteStore;
use support::{
    digest_text, measured_storage_bytes, raw_pool, raw_write_pool, scalar_i64, table_snapshot,
    temp_dir,
};

// ---------------------------------------------------------------------------------------------
// 固定输入
// ---------------------------------------------------------------------------------------------

const DEVICE: &str = "11111111-1111-4111-8111-111111111111";
const OTHER_DEVICE: &str = "22222222-2222-4222-8222-222222222222";
const NODE: &str = "33333333-3333-4333-8333-333333333333";
const PAIRING: &str = "55555555-5555-4555-8555-555555555555";
/// 第二个配对（按标识终结用例：与 `PAIRING` 同时存在）。
const OTHER_PAIRING: &str = "66666666-6666-4666-8666-666666666666";

/// P-256 基点 G 的 SEC1 未压缩编码：曲线上的确定点，构造不需要随机源。
const PUBKEY_HEX: &str = concat!(
    "04",
    "6b17d1f2e12c4247f8bce6e563a440f277037d812deb33a0f4a13945d898c296",
    "4fe342e2fe1a7f9b8ee7eb4a7c0f9e162bce33576b315ececbb6406837bf51f5"
);

/// 另一枚合法指纹文本（与 G 无关），用于换钥/角色指纹不一致的路径。
const FOREIGN_FINGERPRINT: &str =
    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// `owned_pairing` 的公开秘密**摘要**来源：明文只应存在于创建方内存，库里只允许出现它的摘要。
const PAIRING_SECRET: &str = "pairing-secret-plaintext";

/// 本机在配对里宣告的绑定（§7.3：设备为 canonical origin，节点为本机在该配对中的 endpoint）。
const HOST_BINDING: &str = "https://host.example";
/// 另一台机器的绑定：认领时回显它必须被拒（§11.2 第 1 条）。
const FOREIGN_BINDING: &str = "https://other.example";
/// 节点配对里本机（Owner）宣告的 endpoint（§7.3）。
const NODE_ENDPOINT: &str = "wss://owner.example/acpr";
/// 节点配对的对端 nodeId。
const PEER_NODE: &str = "99999999-9999-4999-8999-999999999999";

fn peer_node_id() -> NodeId {
    NodeId::new(PEER_NODE).expect("node id")
}

/// 一个绝不允许出现在任何列里的凭据值（keystore 只经端口取用，持久层只记引用与字段名）。
const CREDENTIAL_VALUE: &str = "sk-live-must-never-be-persisted";

// ---------------------------------------------------------------------------------------------
// 辅助
// ---------------------------------------------------------------------------------------------

/// 具名冲突断言（`PortError` 不带 `PartialEq`：比较走分类而不是整值相等）。
fn assert_conflict(error: PortError, kind: ConflictKind) {
    match error {
        PortError::Conflict(actual) => assert_eq!(actual, kind),
        other => panic!("expected Conflict({kind}), got {other}"),
    }
}

/// 参数类错误必须带上具名原因（调用方据此修正请求）。
fn assert_invalid_request(error: PortError, what: &str) {
    match error {
        PortError::InvalidRequest(actual) => assert_eq!(actual, what),
        other => panic!("expected InvalidRequest({what}), got {other}"),
    }
}

/// 不可用类错误（容量/keystore/远端）：调用方据此重试或降级，不能降级为参数错误。
fn assert_unavailable(error: PortError, kind: UnavailableKind) {
    match error {
        PortError::Unavailable(actual) => assert_eq!(actual, kind),
        other => panic!("expected Unavailable({kind}), got {other}"),
    }
}

fn at(minute: u32) -> Timestamp {
    Timestamp::new(&format!("2026-09-18T00:{minute:02}:00.000Z")).expect("timestamp")
}

fn hex(text: &str) -> Vec<u8> {
    text.as_bytes()
        .chunks(2)
        .map(|pair| {
            let hi = (pair[0] as char).to_digit(16).expect("hex") as u8;
            let lo = (pair[1] as char).to_digit(16).expect("hex") as u8;
            (hi << 4) | lo
        })
        .collect()
}

fn peer_public_key() -> PeerPublicKey {
    PeerPublicKey::try_from_bytes(&hex(PUBKEY_HEX)).expect("基点 G 是合法的 P-256 未压缩点")
}

fn device_id() -> DeviceId {
    DeviceId::new(DEVICE).expect("device id")
}

fn remote_device_id() -> DeviceId {
    DeviceId::new(OTHER_DEVICE).expect("device id")
}

fn node_id() -> NodeId {
    NodeId::new(NODE).expect("node id")
}

fn pairing_id() -> PairingId {
    PairingId::new(PAIRING).expect("pairing id")
}

fn digest(seed: &str) -> Digest {
    Digest::new(&digest_text(seed)).expect("digest")
}

fn nonce(seed: &str) -> Nonce {
    Nonce::new(&digest_text(seed)).expect("nonce")
}

fn audit(action: AuditAction, target: EntityRef, outcome: AuditOutcome) -> PendingAudit {
    PendingAudit {
        action,
        actor: Actor::LocalCli,
        via_node: None,
        local_principal_ref: None,
        target,
        outcome,
        detail_digest: None,
    }
}

fn context(minute: u32, audit: Vec<PendingAudit>) -> WriteContext {
    WriteContext {
        at: at(minute),
        audit,
    }
}

/// 与 `audit()` 同款，但归因主体可指定：`consume_pairing` 要求写集主体与审计归因一致（§11.6 第 8 条）。
fn audit_for(actor: Actor, action: AuditAction, target: EntityRef) -> PendingAudit {
    PendingAudit {
        action,
        actor,
        via_node: None,
        local_principal_ref: None,
        target,
        outcome: AuditOutcome::Success,
        detail_digest: None,
    }
}

/// 消费配对用的设备主体（设备不再有授权集合，构造只需要 id）。
fn device_actor() -> Actor {
    Actor::Device {
        device: device_id(),
    }
}

/// 认证收尾用的节点主体：Node Link 的对端（§5.3 的归因形状——`actor` 与 `via_node` 都是该对端）。
fn node_actor(peer: &NodeId) -> Actor {
    Actor::Node {
        node: peer.clone(),
        access_node: peer.clone(),
    }
}

/// `node.authenticated` 的成功审计行（§5.3/§11.6 第 9 条：`target = Node(对端)`、
/// `localPrincipalRef` 为空、`detailDigest` 为空）。
fn node_authenticated(peer: &NodeId) -> PendingAudit {
    PendingAudit {
        action: AuditAction::NodeAuthenticated,
        actor: node_actor(peer),
        via_node: Some(peer.clone()),
        local_principal_ref: None,
        target: EntityRef::Node(peer.clone()),
        outcome: AuditOutcome::Success,
        detail_digest: None,
    }
}

/// 一次认证收尾的写集（§11.6 第 9 条）：认证时间 + 一条认证成功审计。
fn node_connected(peer: &NodeId, kind: NodeKind, minute: u32) -> NodeConnectedWrite {
    NodeConnectedWrite {
        node: peer.clone(),
        kind,
        context: context(minute, vec![node_authenticated(peer)]),
    }
}

/// 设备配对登记（`created`），`display_name` 在 claim 之前为空。
///
/// 设备配对不携带任何授权集合（`pairing-model`：设备不再有授权维度），因此请求集合恒为空。
fn device_pairing(expires: u32) -> PairingRecord {
    PairingRecord::try_new(
        pairing_id(),
        PairingTarget::Device,
        PairingState::Created,
        None,
        digest(PAIRING_SECRET),
        HOST_BINDING,
        at(0),
        at(expires),
        None,
        None,
        None,
    )
    .expect("pairing record")
}

fn device_claim(id: &DeviceId) -> PairingClaimWrite {
    device_claim_with(id, HOST_BINDING)
}

fn device_claim_with(id: &DeviceId, host_binding: &str) -> PairingClaimWrite {
    let peer = PairingPeer::try_new(
        PeerIdentity::Device(id.clone()),
        "phone",
        peer_public_key(),
        host_binding,
        nonce("client-nonce"),
    )
    .expect("pairing peer");
    let claim = acp_core::model::PairingClaim::try_new(pairing_id(), peer).expect("claim");
    PairingClaimWrite {
        claim,
        context: context(
            1,
            vec![audit(
                AuditAction::PairingClaimed,
                EntityRef::Pairing(pairing_id()),
                AuditOutcome::Success,
            )],
        ),
    }
}

/// 节点配对登记（`node.pair.begin --mode owner` 的形状：目标 `node`、不带 scopes、请求 grants）。
fn node_pairing() -> PairingRecord {
    PairingRecord::try_new(
        pairing_id(),
        PairingTarget::Node,
        PairingState::Created,
        None,
        digest(PAIRING_SECRET),
        NODE_ENDPOINT,
        at(0),
        at(30),
        None,
        None,
        None,
    )
    .expect("pairing record")
}

/// 节点配对的认领：对端（Access 节点）回显登记时的 endpoint。
fn node_claim(id: &NodeId) -> PairingClaimWrite {
    let peer = PairingPeer::try_new(
        PeerIdentity::Node(id.clone()),
        "office access",
        peer_public_key(),
        NODE_ENDPOINT,
        nonce("node-client-nonce"),
    )
    .expect("pairing peer");
    let claim = acp_core::model::PairingClaim::try_new(pairing_id(), peer).expect("claim");
    PairingClaimWrite {
        claim,
        context: context(
            1,
            vec![audit(
                AuditAction::PairingClaimed,
                EntityRef::Pairing(pairing_id()),
                AuditOutcome::Success,
            )],
        ),
    }
}

/// 设备配对的批准落定：**不带任何授权集合**（设备配对不授予授权，非空 scope 集合会被拒绝；见
/// `a_pairing_settlement_must_not_carry_a_scope_set`）。
fn approved_settlement() -> PairingSettlement {
    PairingSettlement::approved()
}

/// 不含凭据绑定的 profile：默认唯一性、种子与失败关闭用例只关心 profile 行本身。
fn device_record(fingerprint: Fingerprint, state: DeviceState) -> DeviceRecord {
    DeviceRecord::try_new(
        device_id(),
        "phone",
        fingerprint,
        state,
        at(2),
        None,
        if state == DeviceState::Revoked {
            Some(at(3))
        } else {
            None
        },
    )
    .expect("device record")
}

fn node_record(kind: NodeKind, fingerprint: Fingerprint, state: NodeState) -> NodeRecord {
    NodeRecord::try_new(
        node_id(),
        "peer node",
        kind,
        fingerprint,
        state,
        match kind {
            NodeKind::Owner => Some("wss://owner.example/acpr".to_owned()),
            NodeKind::Access => None,
        },
        at(1),
        None,
        if state == NodeState::Revoked {
            Some(at(3))
        } else {
            None
        },
    )
    .expect("node record")
}

/// 不含凭据绑定的 profile：默认唯一性、种子与失败关闭用例只关心 profile 行本身。
fn device_record_with_seen(fingerprint: Fingerprint, last_seen: Option<Timestamp>) -> DeviceRecord {
    DeviceRecord::try_new(
        device_id(),
        "phone",
        fingerprint,
        DeviceState::Active,
        at(2),
        last_seen,
        None,
    )
    .expect("device record")
}

fn node_record_with_connected(
    kind: NodeKind,
    fingerprint: Fingerprint,
    last_connected: Option<Timestamp>,
) -> NodeRecord {
    NodeRecord::try_new(
        node_id(),
        "peer node",
        kind,
        fingerprint,
        NodeState::Paired,
        match kind {
            NodeKind::Owner => Some("wss://owner.example/acpr".to_owned()),
            NodeKind::Access => None,
        },
        at(1),
        last_connected,
        None,
    )
    .expect("node record")
}

fn absolute_path(name: &str) -> String {
    std::env::temp_dir()
        .join("acpr-admin-store")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

fn workspace_directory(name: &str) -> DirectoryId {
    acp_core::model::derive_directory_id(&absolute_path(name))
}

async fn seed_directory(pool: &sqlx::SqlitePool, name: &str, created_at: Timestamp) {
    let path = absolute_path(name);
    sqlx::query(
        "INSERT INTO owned_directory (directory_id, canonical_path, created_at, last_used_at) \
         VALUES (?1, ?2, ?3, ?3)",
    )
    .bind(workspace_directory(name).as_str())
    .bind(&path)
    .bind(created_at.as_str())
    .execute(pool)
    .await
    .expect("seed directory");
}

async fn open(dir: &Path) -> SqliteStore {
    SqliteStore::open(StorageConfig::new(dir), &at(0))
        .await
        .expect("open store")
}

async fn approve_device(store: &SqliteStore, id: &DeviceId, minute: u32) -> TrustRecordRef {
    store
        .create_pairing(PairingWrite {
            record: device_pairing(30),
            context: context(
                0,
                vec![audit(
                    AuditAction::PairingCreated,
                    EntityRef::Pairing(pairing_id()),
                    AuditOutcome::Success,
                )],
            ),
        })
        .await
        .expect("create pairing");
    store
        .claim_pairing(device_claim(id))
        .await
        .expect("claim pairing");
    store
        .settle_pairing(PairingSettlementWrite {
            pairing: pairing_id(),
            settlement: approved_settlement(),
            context: context(
                minute,
                vec![audit(
                    AuditAction::PairingApproved,
                    EntityRef::Pairing(pairing_id()),
                    AuditOutcome::Success,
                )],
            ),
        })
        .await
        .expect("settle pairing")
}

/// 节点配对批准（§11.6 第 4 条）：写下对端的 `access` 信任行并绑定身份材料。认证时间**不**在这一步写
/// ——`last_connected_at` 是认证收尾的职责（§11.6 第 8/9 条）。
async fn approve_node(store: &SqliteStore, peer: &NodeId, minute: u32) -> TrustRecordRef {
    store
        .create_pairing(PairingWrite {
            record: node_pairing(),
            context: context(
                0,
                vec![audit(
                    AuditAction::PairingCreated,
                    EntityRef::Pairing(pairing_id()),
                    AuditOutcome::Success,
                )],
            ),
        })
        .await
        .expect("create node pairing");
    store
        .claim_pairing(node_claim(peer))
        .await
        .expect("claim node pairing");
    store
        .settle_pairing(PairingSettlementWrite {
            pairing: pairing_id(),
            settlement: PairingSettlement::approved(),
            context: context(
                minute,
                vec![audit(
                    AuditAction::NodePaired,
                    EntityRef::Node(peer.clone()),
                    AuditOutcome::Success,
                )],
            ),
        })
        .await
        .expect("approve node pairing")
}

/// `(node, access)` 行的认证时间（读面断言用；行必须存在）。
async fn last_connected(store: &SqliteStore, node: &NodeId) -> Option<Timestamp> {
    store
        .node(node, NodeKind::Access)
        .await
        .expect("node")
        .expect("node row")
        .last_connected_at()
        .cloned()
}

/// `owned_audit` 里某动作的行数（审计与状态同事务的独立证据）。
async fn audit_rows(pool: &SqlitePool, action: AuditAction) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM owned_audit WHERE action = ?1")
        .bind(action.as_str())
        .fetch_one(pool)
        .await
        .expect("audit count")
}

/// `owned_audit` 里某动作的逐行文本（按写入顺序），用来核对审计归因的**每一列**（§5.3）。
async fn audit_lines(pool: &SqlitePool, action: AuditAction) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT at || '|' || action || '|' || actor_kind || '|' || actor_id || '|' || \
         COALESCE(via_node_id, '∅') || '|' || COALESCE(local_principal_ref, '∅') || '|' || \
         target_kind || '|' || target_id || '|' || outcome || '|' || \
         COALESCE(detail_digest, '∅') FROM owned_audit WHERE action = ?1 ORDER BY audit_id",
    )
    .bind(action.as_str())
    .fetch_all(pool)
    .await
    .expect("audit lines")
}

/// 逐表、逐 TEXT 列扫一个字符串（`instr`，不做模式匹配）；返回命中的 `表.列`。
async fn columns_containing(pool: &SqlitePool, needle: &str) -> Vec<String> {
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )
    .fetch_all(pool)
    .await
    .expect("tables");
    let mut hits = Vec::new();
    for table in tables {
        let columns: Vec<(String, String)> =
            sqlx::query_as("SELECT name, type FROM pragma_table_info(?1)")
                .bind(&table)
                .fetch_all(pool)
                .await
                .expect("columns");
        for (column, kind) in columns {
            if !kind.eq_ignore_ascii_case("TEXT") {
                continue;
            }
            let found: i64 = sqlx::query_scalar(&format!(
                "SELECT COUNT(*) FROM \"{table}\" WHERE instr(\"{column}\", ?1) > 0"
            ))
            .bind(needle)
            .fetch_one(pool)
            .await
            .expect("scan column");
            if found > 0 {
                hits.push(format!("{table}.{column}"));
            }
        }
    }
    hits
}

/// 末页破坏（保留文件头与早期页）：`quick_check` 失败但 `meta` 仍可读 → 只读失败关闭。
fn corrupt_last_page(path: &Path) {
    let mut bytes = std::fs::read(path).expect("read database");
    let start = bytes.len().saturating_sub(4096);
    for byte in &mut bytes[start..] {
        *byte = 0xA5;
    }
    std::fs::write(path, &bytes).expect("write corrupt database");
}

// ---------------------------------------------------------------------------------------------
// 配对：认领、落定、身份材料（§11.6 第 3/4 条，§11.5）
// ---------------------------------------------------------------------------------------------

/// spec：认领后重启仍能取到验签公钥（报告序号 → 事件序号 之外，身份材料必须逐字节保留）。
#[tokio::test]
async fn pairing_approval_persists_identity_material_across_reopen() {
    let dir = temp_dir("admin-pairing-approve");
    let store = open(&dir).await;
    assert!(matches!(
        approve_device(&store, &device_id(), 2).await,
        TrustRecordRef::Device(id) if id == device_id()
    ));

    let device = store
        .device(&device_id())
        .await
        .expect("device")
        .expect("device row");
    assert_eq!(device.state(), DeviceState::Active);
    assert_eq!(
        device.public_key_fingerprint(),
        &peer_public_key().fingerprint()
    );
    assert_eq!(device.created_at(), &at(2));
    let bound = store
        .peer_key(&PeerIdentity::Device(device_id()))
        .await
        .expect("peer key")
        .expect("bound material");
    assert_eq!(bound.as_bytes(), peer_public_key().as_bytes());
    let pairing = store
        .pairing(&pairing_id())
        .await
        .expect("pairing")
        .expect("pairing row");
    assert_eq!(pairing.state(), PairingState::Approved);
    assert_eq!(pairing.approved_at(), Some(&at(2)));
    assert_eq!(pairing.claimed_at(), Some(&at(1)));
    assert!(
        store
            .pairing_peer(&pairing_id())
            .await
            .expect("peer")
            .expect("peer row")
            .public_key_fingerprint()
            == peer_public_key().fingerprint()
    );
    let path = dir.join(storage_sqlite::migrate::DATABASE_FILE);
    store.close().await;

    // 重启：验签公钥只来自持久化材料，不依赖对端重发。
    let reopened = open(&dir).await;
    let again = reopened
        .peer_key(&PeerIdentity::Device(device_id()))
        .await
        .expect("peer key")
        .expect("bound material");
    assert_eq!(again.as_bytes(), peer_public_key().as_bytes());
    assert_eq!(
        reopened
            .device(&device_id())
            .await
            .expect("device")
            .expect("device row")
            .state(),
        DeviceState::Active
    );
    reopened.close().await;

    // 库内只出现公钥字节与该配对的摘要，不出现 pairing secret 明文（§11.5）。
    let pool = raw_pool(&path).await;
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_peer_key").await,
        1
    );
    assert!(columns_containing(&pool, PAIRING_SECRET).await.is_empty());
    let stored_digest: String =
        sqlx::query_scalar("SELECT secret_digest FROM owned_pairing WHERE pairing_id = ?1")
            .bind(PAIRING)
            .fetch_one(&pool)
            .await
            .expect("secret digest");
    assert_eq!(stored_digest, digest_text(PAIRING_SECRET));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT length(public_key) FROM owned_device WHERE device_id = ?1"
        )
        .bind(DEVICE)
        .fetch_one(&pool)
        .await
        .expect("public key length"),
        65
    );
    assert_eq!(audit_rows(&pool, AuditAction::PairingApproved).await, 1);
    pool.close().await;
}

/// spec：并发认领只有一个成功（写入唯一对端行，第二个拿到 `AlreadyClaimed`）。
#[tokio::test]
async fn second_claim_of_the_same_pairing_is_rejected() {
    let dir = temp_dir("admin-pairing-claim-conflict");
    let store = open(&dir).await;
    store
        .create_pairing(PairingWrite {
            record: device_pairing(30),
            context: context(0, Vec::new()),
        })
        .await
        .expect("create pairing");
    let first = store
        .claim_pairing(device_claim(&device_id()))
        .await
        .expect("first claim");
    assert_eq!(first.pairing.state(), PairingState::PendingConfirmation);
    assert_eq!(first.pairing.claimed_at(), Some(&at(1)));

    let error = store
        .claim_pairing(device_claim(&remote_device_id()))
        .await
        .expect_err("a second claim must be refused");
    assert_conflict(error, ConflictKind::AlreadyClaimed);

    assert_eq!(
        store
            .pairing_peer(&pairing_id())
            .await
            .expect("peer")
            .expect("peer row")
            .id(),
        &PeerIdentity::Device(device_id())
    );
    let path = dir.join(storage_sqlite::migrate::DATABASE_FILE);
    store.close().await;
    let pool = raw_pool(&path).await;
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_pairing_peer").await,
        1
    );
    pool.close().await;
}

/// 库内出现空绑定时按**损坏**报告（不是「调用方参数错误」）：该列是 NOT NULL，空串只可能来自外部
/// 改写或本轮之前写空绑定的构建；错误分类必须指向库内状态，才能被正确排障。
#[tokio::test]
async fn an_empty_stored_binding_is_reported_as_corrupt() {
    let dir = temp_dir("admin-pairing-empty-binding");
    let store = open(&dir).await;
    store
        .create_pairing(PairingWrite {
            record: device_pairing(30),
            context: context(0, Vec::new()),
        })
        .await
        .expect("create pairing");
    let path = dir.join(storage_sqlite::migrate::DATABASE_FILE);
    store.close().await;

    // 绕过模型直接改库（模型已拒绝空绑定，这里模拟历史/外部改写）。
    let raw = raw_write_pool(&path).await;
    sqlx::query("UPDATE owned_pairing SET host_binding = '' WHERE pairing_id = ?1")
        .bind(PAIRING)
        .execute(&raw)
        .await
        .expect("blank the stored binding");
    raw.close().await;

    let store = open(&dir).await;
    assert!(
        matches!(
            store
                .pairing(&pairing_id())
                .await
                .expect_err("an empty stored binding is a store-side defect"),
            PortError::Corrupt(_)
        ),
        "空绑定必须报损坏，不能落进通用 InvalidRequest 文案"
    );
    store.close().await;
}

/// 规格：身份材料读取必须核对同行指纹（peer-identity-material）。
///
/// DDL 只有列级 CHECK（长度/字符集），没有把 `fingerprint` 绑到 `public_key` 的跨列约束，因此
/// 「指纹与公钥互相矛盾」的行只能在读取期发现：`owned_peer_key`、`owned_device`（单读与列表读）
/// 与 `owned_pairing_peer` 三条读取路径都 MUST 失败关闭，不把矛盾材料交出去。
#[tokio::test]
async fn corrupted_identity_material_fails_closed_on_read() {
    let dir = temp_dir("admin-identity-corrupt-read");
    let store = open(&dir).await;
    let path = dir.join(storage_sqlite::migrate::DATABASE_FILE);
    approve_device(&store, &device_id(), 2).await;

    // 正向对照：材料一致时三条读取路径都照常返回（避免整组断言恒真）。
    assert!(
        store
            .peer_key(&PeerIdentity::Device(device_id()))
            .await
            .expect("peer key")
            .is_some()
    );
    assert!(store.device(&device_id()).await.expect("device").is_some());
    assert_eq!(store.devices().await.expect("devices").len(), 1);
    assert!(
        store
            .pairing_peer(&pairing_id())
            .await
            .expect("pairing peer")
            .is_some()
    );

    // 外部改写：三张表的指纹列都改成合法格式（64 位小写 hex，能过列级 CHECK）但与公钥派生值不符的值。
    let wrong = "0".repeat(64);
    let raw = raw_write_pool(&path).await;
    for statement in [
        "UPDATE owned_peer_key SET fingerprint = ?1 WHERE peer_kind = 'device'",
        "UPDATE owned_device SET fingerprint = ?1",
        "UPDATE owned_pairing_peer SET fingerprint = ?1",
    ] {
        sqlx::query(statement)
            .bind(&wrong)
            .execute(&raw)
            .await
            .expect("corrupt fingerprint column");
    }
    raw.close().await;

    assert!(
        matches!(
            store
                .peer_key(&PeerIdentity::Device(device_id()))
                .await
                .expect_err("trust material with a foreign fingerprint"),
            PortError::Corrupt(_)
        ),
        "信任材料读取必须失败关闭"
    );
    assert!(
        matches!(
            store
                .device(&device_id())
                .await
                .expect_err("device row with a foreign fingerprint"),
            PortError::Corrupt(_)
        ),
        "设备记录单读必须失败关闭"
    );
    assert!(
        matches!(
            store
                .devices()
                .await
                .expect_err("device list with a foreign fingerprint"),
            PortError::Corrupt(_)
        ),
        "设备记录列表读必须失败关闭"
    );
    assert!(
        matches!(
            store
                .pairing_peer(&pairing_id())
                .await
                .expect_err("pairing peer with a foreign fingerprint"),
            PortError::Corrupt(_)
        ),
        "配对对端记录读取必须失败关闭"
    );
    store.close().await;
}

/// WP6-2 回归：节点配对的批准路径必须真的落信任行。`node.pair.begin --mode owner` 创建的节点配对
/// 被 `node.pair.confirm` 批准后，应写 `owned_node`（角色 `access`、`owner_endpoint` 为空）、把对端
/// 公钥转入身份材料、推进配对到 `approved` 并写 `node.paired` 审计；重启后仍能取到验签公钥。
#[tokio::test]
async fn node_pairing_approval_persists_access_trust() {
    let dir = temp_dir("admin-node-pairing-approval");
    let store = open(&dir).await;
    let peer = peer_node_id();
    store
        .create_pairing(PairingWrite {
            record: node_pairing(),
            context: context(
                0,
                vec![audit(
                    AuditAction::PairingCreated,
                    EntityRef::Pairing(pairing_id()),
                    AuditOutcome::Success,
                )],
            ),
        })
        .await
        .expect("create node pairing");
    store.claim_pairing(node_claim(&peer)).await.expect("claim");
    assert_eq!(
        store
            .settle_pairing(PairingSettlementWrite {
                pairing: pairing_id(),
                settlement: PairingSettlement::approved(),
                context: context(
                    2,
                    vec![audit(
                        AuditAction::NodePaired,
                        EntityRef::Node(peer.clone()),
                        AuditOutcome::Success,
                    )],
                ),
            })
            .await
            .expect("approve node pairing"),
        TrustRecordRef::Node(peer.clone())
    );
    let row = store
        .node(&peer, NodeKind::Access)
        .await
        .expect("node")
        .expect("node row");
    assert_eq!(
        row.kind(),
        NodeKind::Access,
        "Owner 侧确认的对端恒为 access（§5.4）"
    );
    assert_eq!(row.state(), NodeState::Paired);
    assert_eq!(row.owner_endpoint(), None);
    assert_eq!(
        row.node_public_key_fingerprint(),
        &peer_public_key().fingerprint()
    );
    assert_eq!(
        store
            .pairing(&pairing_id())
            .await
            .expect("pairing")
            .expect("pairing row")
            .state(),
        PairingState::Approved
    );
    assert!(
        store
            .peer_key(&PeerIdentity::Node(peer.clone()))
            .await
            .expect("peer key")
            .is_some()
    );

    let path = dir.join(storage_sqlite::migrate::DATABASE_FILE);
    store.close().await;
    // 重启后验签公钥逐字节相同（§11.5：后续握手只读持久化材料）。
    let reopened = open(&dir).await;
    assert_eq!(
        reopened
            .peer_key(&PeerIdentity::Node(peer.clone()))
            .await
            .expect("peer key")
            .expect("bound material")
            .as_bytes(),
        peer_public_key().as_bytes()
    );
    reopened.close().await;
    let pool = raw_pool(&path).await;
    assert_eq!(audit_rows(&pool, AuditAction::NodePaired).await, 1);
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_node").await,
        1
    );
    pool.close().await;
}

/// `node.pair.confirm` 的 `exportIds`（`design.md` D2/D4）：清单经 `ALTER TABLE` 追加的列落盘，并被
/// 归一化为去重 + 字典序；空集合编成 `'[]'`，读回（`node.list`/`node` 的路径）得到同一份集合。
#[tokio::test]
async fn claim_with_a_foreign_host_binding_is_rejected() {
    let dir = temp_dir("admin-pairing-binding");
    let store = open(&dir).await;
    store
        .create_pairing(PairingWrite {
            record: device_pairing(30),
            context: context(0, Vec::new()),
        })
        .await
        .expect("create pairing");

    assert_conflict(
        store
            .claim_pairing(device_claim_with(&device_id(), FOREIGN_BINDING))
            .await
            .expect_err("a claim echoing another host must be refused"),
        ConflictKind::IdentityMismatch,
    );
    let path = dir.join(storage_sqlite::migrate::DATABASE_FILE);
    assert_eq!(
        store
            .pairing(&pairing_id())
            .await
            .expect("pairing")
            .expect("pairing row")
            .state(),
        PairingState::Created,
        "被拒的认领不得推进状态"
    );
    assert!(
        store
            .pairing_peer(&pairing_id())
            .await
            .expect("peer")
            .is_none()
    );
    store.close().await;
    let pool = raw_pool(&path).await;
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_pairing_peer").await,
        0
    );
    assert_eq!(audit_rows(&pool, AuditAction::PairingClaimed).await, 0);
    pool.close().await;

    // 正确的回显仍可认领（证明拒绝来自绑定不一致，而不是写集本身非法）。
    let store = open(&dir).await;
    store
        .claim_pairing(device_claim(&device_id()))
        .await
        .expect("matching binding");
    store.close().await;
}

/// spec：撤销后只能经**协议重新配对**恢复（§11.2 第 2 条）——普通写入一律拒绝，而 `settle_pairing`
/// 的批准路径可以复活同一身份：状态回到活动、撤销时间与原因清空，但撤销审计保留（tombstone）。
#[tokio::test]
async fn only_a_fresh_pairing_can_lift_a_revocation() {
    let dir = temp_dir("admin-revival");
    let store = open(&dir).await;
    approve_device(&store, &device_id(), 2).await;
    store
        .revoke_device(DeviceRevocation {
            device: device_id(),
            reason: RevokeReason::Compromised,
            context: context(
                3,
                vec![audit(
                    AuditAction::DeviceRevoked,
                    EntityRef::Device(device_id()),
                    AuditOutcome::Success,
                )],
            ),
        })
        .await
        .expect("revoke device");
    let path = dir.join(storage_sqlite::migrate::DATABASE_FILE);

    // 普通写入不能复活（§11.6 第 1 条）。
    assert_conflict(
        store
            .put_device(DeviceWrite {
                record: device_record(peer_public_key().fingerprint(), DeviceState::Active),
                context: context(4, Vec::new()),
            })
            .await
            .expect_err("a plain write must not lift a revocation"),
        ConflictKind::IdentityMismatch,
    );

    // 协议重新配对：新登记 + 认领 + 批准。
    let pairing = PairingId::new("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb").expect("pairing id");
    store
        .create_pairing(PairingWrite {
            record: PairingRecord::try_new(
                pairing.clone(),
                PairingTarget::Device,
                PairingState::Created,
                None,
                digest(PAIRING_SECRET),
                HOST_BINDING,
                at(5),
                at(10),
                None,
                None,
                None,
            )
            .expect("pairing record"),
            context: context(5, Vec::new()),
        })
        .await
        .expect("create re-pairing");
    store
        .claim_pairing(PairingClaimWrite {
            claim: {
                let peer = PairingPeer::try_new(
                    PeerIdentity::Device(device_id()),
                    "phone",
                    peer_public_key(),
                    HOST_BINDING,
                    nonce("re-pair-nonce"),
                )
                .expect("peer");
                acp_core::model::PairingClaim::try_new(pairing.clone(), peer).expect("claim")
            },
            context: context(6, Vec::new()),
        })
        .await
        .expect("claim re-pairing");
    store
        .settle_pairing(PairingSettlementWrite {
            pairing,
            settlement: approved_settlement(),
            context: context(
                7,
                vec![audit(
                    AuditAction::PairingApproved,
                    EntityRef::Pairing(pairing_id()),
                    AuditOutcome::Success,
                )],
            ),
        })
        .await
        .expect("a fresh pairing lifts the revocation");

    let revived = store
        .device(&device_id())
        .await
        .expect("device")
        .expect("device row");
    assert_eq!(revived.state(), DeviceState::Active);
    assert_eq!(revived.revoked_at(), None);
    store.close().await;

    let pool = raw_pool(&path).await;
    assert_eq!(
        sqlx::query_scalar::<_, Option<String>>(
            "SELECT revoke_reason FROM owned_device WHERE device_id = ?1"
        )
        .bind(DEVICE)
        .fetch_one(&pool)
        .await
        .expect("revoke reason"),
        None,
        "复活后撤销原因清空（DDL 的 CHECK 也要求 active 行无原因）"
    );
    assert_eq!(
        audit_rows(&pool, AuditAction::DeviceRevoked).await,
        1,
        "撤销审计是 tombstone，不因复活消失"
    );
    pool.close().await;
}

/// 同一规则覆盖节点：`put_node` 拒绝已撤销行，而协议批准（对端为 `access`）可以复活该角色行。
#[tokio::test]
async fn only_a_fresh_node_pairing_can_lift_a_node_revocation() {
    let dir = temp_dir("admin-node-revival");
    let store = open(&dir).await;
    let peer = peer_node_id();
    let fingerprint = peer_public_key().fingerprint();
    // 这一行属于**对端**节点（`peer`），不是本节点的 `node_id()`。
    let access_record = |fingerprint: Fingerprint, connected: Option<Timestamp>| {
        NodeRecord::try_new(
            peer.clone(),
            "office access",
            NodeKind::Access,
            fingerprint,
            NodeState::Paired,
            None,
            at(1),
            connected,
            None,
        )
        .expect("node record")
    };
    store
        .put_node(NodeWrite {
            record: access_record(fingerprint.clone(), None),
            public_key: peer_public_key(),
            context: context(1, Vec::new()),
        })
        .await
        .expect("put access role");
    store
        .revoke_node(NodeRevocation {
            node: peer.clone(),
            reason: RevokeReason::UserRequested,
            context: context(
                3,
                vec![audit(
                    AuditAction::NodeTrustRevoked,
                    EntityRef::Node(peer.clone()),
                    AuditOutcome::Success,
                )],
            ),
        })
        .await
        .expect("revoke node");
    assert_conflict(
        store
            .put_node(NodeWrite {
                record: access_record(fingerprint, None),
                public_key: peer_public_key(),
                context: context(4, Vec::new()),
            })
            .await
            .expect_err("a plain write must not lift a node revocation"),
        ConflictKind::IdentityMismatch,
    );

    // 协议重新配对：登记（节点目标）+ 认领 + 批准。
    store
        .create_pairing(PairingWrite {
            record: node_pairing(),
            context: context(5, Vec::new()),
        })
        .await
        .expect("create re-pairing");
    store.claim_pairing(node_claim(&peer)).await.expect("claim");
    store
        .settle_pairing(PairingSettlementWrite {
            pairing: pairing_id(),
            settlement: PairingSettlement::approved(),
            context: context(7, Vec::new()),
        })
        .await
        .expect("a fresh node pairing lifts the revocation");

    let row = store
        .node(&peer, NodeKind::Access)
        .await
        .expect("node")
        .expect("node row");
    assert_eq!(row.state(), NodeState::Paired);
    assert_eq!(row.revoked_at(), None);
    let path = dir.join(storage_sqlite::migrate::DATABASE_FILE);
    store.close().await;
    let pool = raw_pool(&path).await;
    assert_eq!(
        sqlx::query_scalar::<_, Option<String>>(
            "SELECT revoke_reason FROM owned_node WHERE node_id = ?1 AND kind = 'access'"
        )
        .bind(PEER_NODE)
        .fetch_one(&pool)
        .await
        .expect("revoke reason"),
        None
    );
    assert_eq!(audit_rows(&pool, AuditAction::NodeTrustRevoked).await, 1);
    pool.close().await;
}

/// 落定只能发生一次：重复批准不得改写首次 `approved_at`，批准后再拒绝必须是具名冲突（不得把
/// `owned_pairing` 的 `(state IN ('approved','consumed')) = (approved_at IS NOT NULL)` CHECK 失败变成
/// 未具名的 `PortError::Backend`）。
#[tokio::test]
async fn a_pairing_can_be_settled_only_once() {
    let dir = temp_dir("admin-pairing-settle-once");
    let store = open(&dir).await;
    approve_device(&store, &device_id(), 2).await;
    let approved = store
        .pairing(&pairing_id())
        .await
        .expect("pairing")
        .expect("pairing row");
    assert_eq!(approved.approved_at(), Some(&at(2)));

    // 重复批准：具名冲突，且首次批准时间与审计都不变。
    assert_conflict(
        store
            .settle_pairing(PairingSettlementWrite {
                pairing: pairing_id(),
                settlement: approved_settlement(),
                context: context(5, Vec::new()),
            })
            .await
            .expect_err("a repeated approval must be refused"),
        ConflictKind::Consumed,
    );
    // 批准后拒绝：同样具名冲突（不是 CHECK 失败落进 `Backend`）。
    assert_conflict(
        store
            .settle_pairing(PairingSettlementWrite {
                pairing: pairing_id(),
                settlement: PairingSettlement::rejected(Some("too late")).expect("rejection"),
                context: context(5, Vec::new()),
            })
            .await
            .expect_err("rejecting an approved pairing must be refused"),
        ConflictKind::Consumed,
    );
    let after = store
        .pairing(&pairing_id())
        .await
        .expect("pairing")
        .expect("pairing row");
    assert_eq!(after.state(), PairingState::Approved);
    assert_eq!(after.approved_at(), Some(&at(2)), "首次批准时间不得被改写");

    let path = dir.join(storage_sqlite::migrate::DATABASE_FILE);
    store.close().await;
    let pool = raw_pool(&path).await;
    assert_eq!(audit_rows(&pool, AuditAction::PairingApproved).await, 1);
    assert_eq!(audit_rows(&pool, AuditAction::PairingRejected).await, 0);
    pool.close().await;
}

/// §11.6 第 8 条 / design D12：消费把 `approved` 推进到 `consumed`、写 `terminal_at` 并在同一写集里
/// 追加 `device.authenticated`；同一对端重复调用幂等成功（不覆盖首次时间、不重复写审计）；重启后保持。
#[tokio::test]
async fn consume_pairing_advances_an_approved_pairing_once() {
    let dir = temp_dir("admin-pairing-consume");
    let store = open(&dir).await;
    approve_device(&store, &device_id(), 2).await;

    let consumed = store
        .consume_pairing(PairingConsumption {
            pairing: pairing_id(),
            actor: device_actor(),
            context: context(
                3,
                vec![audit_for(
                    device_actor(),
                    AuditAction::DeviceAuthenticated,
                    EntityRef::Pairing(pairing_id()),
                )],
            ),
        })
        .await
        .expect("consume an approved pairing");
    assert_eq!(consumed.state(), PairingState::Consumed);
    assert_eq!(consumed.terminal_at(), Some(&at(3)));
    assert_eq!(consumed.approved_at(), Some(&at(2)), "批准时间不得被改写");

    // 幂等重试：同一对端的重复消费成功，且不改写首次时间。
    let again = store
        .consume_pairing(PairingConsumption {
            pairing: pairing_id(),
            actor: device_actor(),
            context: context(
                4,
                vec![audit_for(
                    device_actor(),
                    AuditAction::DeviceAuthenticated,
                    EntityRef::Pairing(pairing_id()),
                )],
            ),
        })
        .await
        .expect("a repeated consumption is idempotent");
    assert_eq!(again.state(), PairingState::Consumed);
    assert_eq!(again.terminal_at(), Some(&at(3)), "首次消费时间不得被改写");

    let path = dir.join(storage_sqlite::migrate::DATABASE_FILE);
    store.close().await;
    let pool = raw_pool(&path).await;
    assert_eq!(audit_rows(&pool, AuditAction::DeviceAuthenticated).await, 1);
    pool.close().await;

    // 重启后状态保持（同一库再打开）。
    let store = open(&dir).await;
    let after = store
        .pairing(&pairing_id())
        .await
        .expect("pairing")
        .expect("pairing row");
    assert_eq!(after.state(), PairingState::Consumed);
    assert_eq!(after.terminal_at(), Some(&at(3)));
    store.close().await;
}

/// 消费的授权面：只接受与该配对已批准对端同类同 id 的主体；写集主体与审计归因分歧按接线错误拒绝，
/// 且任何被拒路径都不得改动配对行或留下审计。
#[tokio::test]
async fn consume_pairing_refuses_a_mismatched_peer_or_audit_actor() {
    let dir = temp_dir("admin-pairing-consume-peer");
    let store = open(&dir).await;
    approve_device(&store, &device_id(), 2).await;

    // 同类别但不同 id。
    assert_conflict(
        store
            .consume_pairing(PairingConsumption {
                pairing: pairing_id(),
                actor: Actor::Device {
                    device: remote_device_id(),
                },
                context: context(3, Vec::new()),
            })
            .await
            .expect_err("a foreign device must not consume the pairing"),
        ConflictKind::IdentityMismatch,
    );
    // 类别不对：设备配对不能被节点主体消费。
    assert_conflict(
        store
            .consume_pairing(PairingConsumption {
                pairing: pairing_id(),
                actor: Actor::Node {
                    node: node_id(),
                    access_node: peer_node_id(),
                },
                context: context(3, Vec::new()),
            })
            .await
            .expect_err("a node must not consume a device pairing"),
        ConflictKind::IdentityMismatch,
    );
    // 写集主体与审计归因分歧（`audit()` 的归因恒为 `Actor::LocalCli`）。
    assert_invalid_request(
        store
            .consume_pairing(PairingConsumption {
                pairing: pairing_id(),
                actor: device_actor(),
                context: context(
                    3,
                    vec![audit(
                        AuditAction::DeviceAuthenticated,
                        EntityRef::Pairing(pairing_id()),
                        AuditOutcome::Success,
                    )],
                ),
            })
            .await
            .expect_err("a divergent audit attribution must fail closed"),
        "consume_pairing actor must match every audit row",
    );

    let after = store
        .pairing(&pairing_id())
        .await
        .expect("pairing")
        .expect("pairing row");
    assert_eq!(
        after.state(),
        PairingState::Approved,
        "被拒路径不得推进状态"
    );
    assert_eq!(after.terminal_at(), None);

    let path = dir.join(storage_sqlite::migrate::DATABASE_FILE);
    store.close().await;
    let pool = raw_pool(&path).await;
    assert_eq!(audit_rows(&pool, AuditAction::DeviceAuthenticated).await, 0);
    pool.close().await;
}

/// 消费的状态面：未批准、已拒绝、已过期都具名拒绝；配对行或对端行缺失各自具名。
#[tokio::test]
async fn consume_pairing_refuses_unapproved_terminal_and_missing_rows() {
    // 未批准（只 claim 过）。
    let dir = temp_dir("admin-pairing-consume-unapproved");
    let store = open(&dir).await;
    store
        .create_pairing(PairingWrite {
            record: device_pairing(30),
            context: context(0, Vec::new()),
        })
        .await
        .expect("create pairing");
    store
        .claim_pairing(device_claim(&device_id()))
        .await
        .expect("claim");
    assert_invalid_request(
        store
            .consume_pairing(PairingConsumption {
                pairing: pairing_id(),
                actor: device_actor(),
                context: context(3, Vec::new()),
            })
            .await
            .expect_err("an unapproved pairing must not be consumed"),
        "pairing has not been approved",
    );
    store.close().await;

    // 已拒绝的配对。
    let dir = temp_dir("admin-pairing-consume-rejected");
    let store = open(&dir).await;
    store
        .create_pairing(PairingWrite {
            record: device_pairing(30),
            context: context(0, Vec::new()),
        })
        .await
        .expect("create pairing");
    store
        .claim_pairing(device_claim(&device_id()))
        .await
        .expect("claim");
    store
        .settle_pairing(PairingSettlementWrite {
            pairing: pairing_id(),
            settlement: PairingSettlement::rejected(Some("wrong device")).expect("rejection"),
            context: context(2, Vec::new()),
        })
        .await
        .expect("reject");
    assert_conflict(
        store
            .consume_pairing(PairingConsumption {
                pairing: pairing_id(),
                actor: device_actor(),
                context: context(3, Vec::new()),
            })
            .await
            .expect_err("a rejected pairing must not be consumed"),
        ConflictKind::Consumed,
    );
    store.close().await;

    // 已过期的配对（登记时 `expires` = t2，认领在 t1，扫捕在 t5 把它终结）。
    let dir = temp_dir("admin-pairing-consume-expired");
    let store = open(&dir).await;
    store
        .create_pairing(PairingWrite {
            record: device_pairing(2),
            context: context(0, Vec::new()),
        })
        .await
        .expect("create pairing");
    store
        .claim_pairing(device_claim(&device_id()))
        .await
        .expect("claim");
    assert_eq!(
        store
            .expire_pairings(ExpiryWrite {
                context: context(5, Vec::new()),
                only: None,
            })
            .await
            .expect("expire sweep"),
        1
    );
    assert_conflict(
        store
            .consume_pairing(PairingConsumption {
                pairing: pairing_id(),
                actor: device_actor(),
                context: context(6, Vec::new()),
            })
            .await
            .expect_err("an expired pairing must not be consumed"),
        ConflictKind::Expired,
    );
    store.close().await;

    // 配对行缺失。
    let dir = temp_dir("admin-pairing-consume-missing");
    let store = open(&dir).await;
    let missing = PairingId::new("abcdefab-1111-4111-8111-abcdefabcdef").expect("pairing id");
    assert!(matches!(
        store
            .consume_pairing(PairingConsumption {
                pairing: missing.clone(),
                actor: device_actor(),
                context: context(3, Vec::new()),
            })
            .await
            .expect_err("an unknown pairing must be refused"),
        PortError::NotFound(EntityRef::Pairing(id)) if id == missing
    ));
    store.close().await;

    // 对端行缺失（库被外部改写）：失败关闭。
    let dir = temp_dir("admin-pairing-consume-headless");
    let store = open(&dir).await;
    approve_device(&store, &device_id(), 2).await;
    let path = dir.join(storage_sqlite::migrate::DATABASE_FILE);
    store.close().await;
    let raw = raw_write_pool(&path).await;
    sqlx::query("DELETE FROM owned_pairing_peer WHERE pairing_id = ?1")
        .bind(PAIRING)
        .execute(&raw)
        .await
        .expect("drop peer row");
    raw.close().await;
    let store = open(&dir).await;
    assert!(matches!(
        store
            .consume_pairing(PairingConsumption {
                pairing: pairing_id(),
                actor: device_actor(),
                context: context(3, Vec::new()),
            })
            .await
            .expect_err("a pairing without a peer row must fail closed"),
        PortError::Corrupt(_)
    ));
    store.close().await;
}

/// §11.2 第 6 条 / §9 判据 23：审计写失败时整个消费写集回滚——配对仍停在 `approved`、`terminal_at`
/// 为空、也没有半条审计；触发器移除后同一写集成功。
#[tokio::test]
async fn a_failed_audit_write_leaves_the_pairing_approved() {
    let dir = temp_dir("admin-pairing-consume-rollback");
    let store = open(&dir).await;
    approve_device(&store, &device_id(), 2).await;

    let path = dir.join(storage_sqlite::migrate::DATABASE_FILE);
    let raw = raw_write_pool(&path).await;
    sqlx::query(
        "CREATE TRIGGER block_audit BEFORE INSERT ON owned_audit BEGIN \
         SELECT RAISE(ABORT, 'audit blocked'); END",
    )
    .execute(&raw)
    .await
    .expect("install trigger");

    let error = store
        .consume_pairing(PairingConsumption {
            pairing: pairing_id(),
            actor: device_actor(),
            context: context(
                3,
                vec![audit_for(
                    device_actor(),
                    AuditAction::DeviceAuthenticated,
                    EntityRef::Pairing(pairing_id()),
                )],
            ),
        })
        .await
        .expect_err("an unwritable audit row must fail the write set");
    assert!(
        matches!(error, PortError::Backend(_)),
        "约束/触发器失败必须给出具名错误，got {error}"
    );
    let after = store
        .pairing(&pairing_id())
        .await
        .expect("pairing")
        .expect("pairing row");
    assert_eq!(
        after.state(),
        PairingState::Approved,
        "失败写集不得留下半状态"
    );
    assert_eq!(after.terminal_at(), None);

    sqlx::query("DROP TRIGGER block_audit")
        .execute(&raw)
        .await
        .expect("drop trigger");
    raw.close().await;

    // 触发器移除后同一写集成功（证明失败来自审计写入，而不是这条写集本身非法）。
    let consumed = store
        .consume_pairing(PairingConsumption {
            pairing: pairing_id(),
            actor: device_actor(),
            context: context(
                4,
                vec![audit_for(
                    device_actor(),
                    AuditAction::DeviceAuthenticated,
                    EntityRef::Pairing(pairing_id()),
                )],
            ),
        })
        .await
        .expect("the same write set succeeds without the trigger");
    assert_eq!(consumed.state(), PairingState::Consumed);
    store.close().await;
}

/// §11.6 第 9 条 / spec `node-link-owner`「节点双向认证与凭据状态」的收尾副作用：重复认证的写集把
/// `(node, access)` 行的 `last_connected_at` 从 NULL 推进到本次时间、`node.authenticated` 与它同事务
/// 落库且归因逐列一致；只前进不倒退（时间是否推进都不免除留痕）；时间列以外一个字段都不动；重启后保持。
#[tokio::test]
async fn record_node_connected_advances_the_access_row_once_forward() {
    let dir = temp_dir("admin-node-connected");
    let store = open(&dir).await;
    let peer = peer_node_id();
    approve_node(&store, &peer, 2).await;
    let approved = store
        .node(&peer, NodeKind::Access)
        .await
        .expect("node")
        .expect("node row");
    assert_eq!(
        approved.last_connected_at(),
        None,
        "配对批准不写认证时间，那是认证收尾的职责（§11.6 第 4/9 条）"
    );

    // 首次写入：NULL → 本次时间。
    store
        .record_node_connected(node_connected(&peer, NodeKind::Access, 3))
        .await
        .expect("record the first authentication");
    let first = store
        .node(&peer, NodeKind::Access)
        .await
        .expect("node")
        .expect("node row");
    assert_eq!(
        first.last_connected_at(),
        Some(&at(3)),
        "旧值为空时首次写入不得被丢成 NULL"
    );
    let expected = NodeRecord::try_new(
        peer.clone(),
        approved.display_name(),
        approved.kind(),
        approved.node_public_key_fingerprint().clone(),
        approved.state(),
        approved.owner_endpoint().map(str::to_owned),
        approved.created_at().clone(),
        Some(at(3)),
        None,
    )
    .expect("expected node record");
    assert_eq!(
        first, expected,
        "本写集只推进 last_connected_at，其余字段逐字段不变"
    );

    let path = dir.join(storage_sqlite::migrate::DATABASE_FILE);
    store.close().await;
    let pool = raw_pool(&path).await;
    assert_eq!(
        audit_lines(&pool, AuditAction::NodeAuthenticated).await,
        vec![format!(
            "{}|node.authenticated|node|{peer}/{peer}|{peer}|∅|node|{peer}|success|∅",
            at(3)
        )],
        "认证成功的审计行必须与时间同事务、归因逐列一致（§5.3）"
    );
    pool.close().await;

    // 重启后保持，并验证「只前进」的两个方向。
    let store = open(&dir).await;
    store
        .record_node_connected(node_connected(&peer, NodeKind::Access, 2))
        .await
        .expect("record an authentication with an earlier timestamp");
    assert_eq!(
        last_connected(&store, &peer).await,
        Some(at(3)),
        "更早的认证时间不得让已存值倒退"
    );
    store
        .record_node_connected(node_connected(&peer, NodeKind::Access, 4))
        .await
        .expect("record a later authentication");
    assert_eq!(
        last_connected(&store, &peer).await,
        Some(at(4)),
        "更晚的认证时间必须推进已存值"
    );
    store.close().await;

    let pool = raw_pool(&path).await;
    assert_eq!(
        audit_rows(&pool, AuditAction::NodeAuthenticated).await,
        3,
        "每一次认证各留一行：时间列是否推进不改变留痕义务"
    );
    pool.close().await;
}

/// §11.6 第 8 条：首次认证（已批准配对 → `consumed`）在**同一写集**里推进对端 `access` 行的
/// `last_connected_at`——节点配对的 `Actor::Node` 打的就是配对批准写下的 `(node, access)` 行；同一次
/// 消费的重复提交不得改写它；`Actor::Device` 的配对没有节点行。
#[tokio::test]
async fn consume_pairing_advances_the_node_row_of_a_node_peer() {
    let dir = temp_dir("admin-pairing-consume-node");
    let store = open(&dir).await;
    let peer = peer_node_id();
    approve_node(&store, &peer, 2).await;
    let consumed = store
        .consume_pairing(PairingConsumption {
            pairing: pairing_id(),
            actor: node_actor(&peer),
            context: context(3, vec![node_authenticated(&peer)]),
        })
        .await
        .expect("consume an approved node pairing");
    assert_eq!(consumed.state(), PairingState::Consumed);
    assert_eq!(
        last_connected(&store, &peer).await,
        Some(at(3)),
        "首次认证在配对消费的写集里推进认证时间"
    );
    store
        .consume_pairing(PairingConsumption {
            pairing: pairing_id(),
            actor: node_actor(&peer),
            context: context(5, vec![node_authenticated(&peer)]),
        })
        .await
        .expect("a repeated consumption is idempotent");
    assert_eq!(
        last_connected(&store, &peer).await,
        Some(at(3)),
        "同一次消费的重复提交不得改写认证时间"
    );
    store.close().await;

    // 设备配对：`Actor::Device` 没有节点行（§11.6 第 8 条），因此消费前后 `owned_node` 始终为空。
    let dir = temp_dir("admin-pairing-consume-device-rows");
    let store = open(&dir).await;
    approve_device(&store, &device_id(), 2).await;
    let path = dir.join(storage_sqlite::migrate::DATABASE_FILE);
    store.close().await;
    let pool = raw_pool(&path).await;
    assert_eq!(
        table_snapshot(&pool, "owned_node").await,
        Vec::<String>::new()
    );
    pool.close().await;
    let store = open(&dir).await;
    store
        .consume_pairing(PairingConsumption {
            pairing: pairing_id(),
            actor: device_actor(),
            context: context(
                3,
                vec![audit_for(
                    device_actor(),
                    AuditAction::DeviceAuthenticated,
                    EntityRef::Pairing(pairing_id()),
                )],
            ),
        })
        .await
        .expect("consume an approved device pairing");
    store.close().await;
    let pool = raw_pool(&path).await;
    assert_eq!(
        table_snapshot(&pool, "owned_node").await,
        Vec::<String>::new(),
        "设备配对不产生节点行"
    );
    pool.close().await;
}

/// §11.6 第 9 条的失败关闭：`(node, kind)` 行不存在（未知对端，或该对端没有这个角色的行）→
/// `NotFound(EntityRef::Node)`，零审计、不推进任何认证时间。Node Link 只传 `access`，这里的 `owner`
/// 行用来钉死「角色是谓词的一部分」：命中角色的那一侧同一调用成功。
#[tokio::test]
async fn record_node_connected_refuses_missing_rows_and_wrong_roles() {
    let dir = temp_dir("admin-node-connected-missing");
    let store = open(&dir).await;
    let peer = peer_node_id();
    approve_node(&store, &peer, 2).await;
    let unknown = NodeId::new("abcdefab-2222-4222-8222-abcdefabcdef").expect("node id");
    assert!(
        matches!(
            store
                .record_node_connected(node_connected(&unknown, NodeKind::Access, 3))
                .await
                .expect_err("an unknown node must be refused"),
            PortError::NotFound(EntityRef::Node(id)) if id == unknown
        ),
        "信任面不存在的对端必须具名失败关闭"
    );
    // 已知对端、角色不符：该对端只有配对批准写下的 `access` 行。
    assert!(
        matches!(
            store
                .record_node_connected(node_connected(&peer, NodeKind::Owner, 3))
                .await
                .expect_err("a node without an owner row must be refused"),
            PortError::NotFound(EntityRef::Node(id)) if id == peer
        ),
        "角色不符的记录必须具名失败关闭"
    );
    // 另一个对端只有 `owner` 行（`put_node`）：反方向的角色不符同样失败，命中角色则成功。
    store
        .put_node(NodeWrite {
            record: node_record_with_connected(
                NodeKind::Owner,
                peer_public_key().fingerprint(),
                None,
            ),
            public_key: peer_public_key(),
            context: context(3, Vec::new()),
        })
        .await
        .expect("write an owner row");
    assert!(
        matches!(
            store
                .record_node_connected(node_connected(&node_id(), NodeKind::Access, 4))
                .await
                .expect_err("an owner row must not answer for the access role"),
            PortError::NotFound(EntityRef::Node(id)) if id == node_id()
        ),
        "角色不符的记录必须具名失败关闭"
    );

    let path = dir.join(storage_sqlite::migrate::DATABASE_FILE);
    store.close().await;
    let pool = raw_pool(&path).await;
    assert_eq!(
        audit_rows(&pool, AuditAction::NodeAuthenticated).await,
        0,
        "被拒路径不得留下审计行"
    );
    assert_eq!(
        scalar_i64(
            &pool,
            "SELECT COUNT(*) FROM owned_node WHERE last_connected_at IS NOT NULL"
        )
        .await,
        0,
        "被拒路径不得推进任何认证时间"
    );
    pool.close().await;

    let store = open(&dir).await;
    store
        .record_node_connected(node_connected(&node_id(), NodeKind::Owner, 4))
        .await
        .expect("the owner row is addressable");
    assert_eq!(
        store
            .node(&node_id(), NodeKind::Owner)
            .await
            .expect("node")
            .expect("node row")
            .last_connected_at(),
        Some(&at(4)),
        "被拒是因为角色行不存在，不是因为入口本身不可用"
    );
    store.close().await;
}

/// §11.2 第 6 条 / §9 判据 23：审计写失败时整个认证收尾写集回滚——未被认证过的行仍为 NULL、已有值
/// 不得被推进、也没有半条审计；触发器移除后同一写集成功。
#[tokio::test]
async fn a_failed_audit_write_leaves_last_connected_at_untouched() {
    let dir = temp_dir("admin-node-connected-rollback");
    let store = open(&dir).await;
    let peer = peer_node_id();
    approve_node(&store, &peer, 2).await;

    let path = dir.join(storage_sqlite::migrate::DATABASE_FILE);
    let raw = raw_write_pool(&path).await;
    sqlx::query(
        "CREATE TRIGGER block_audit BEFORE INSERT ON owned_audit BEGIN \
         SELECT RAISE(ABORT, 'audit blocked'); END",
    )
    .execute(&raw)
    .await
    .expect("install trigger");

    let error = store
        .record_node_connected(node_connected(&peer, NodeKind::Access, 3))
        .await
        .expect_err("an unwritable audit row must fail the write set");
    assert!(
        matches!(error, PortError::Backend(_)),
        "约束/触发器失败必须给出具名错误，got {error}"
    );
    assert_eq!(
        last_connected(&store, &peer).await,
        None,
        "首次写入失败不得留下半状态"
    );

    sqlx::query("DROP TRIGGER block_audit")
        .execute(&raw)
        .await
        .expect("drop trigger");
    store
        .record_node_connected(node_connected(&peer, NodeKind::Access, 3))
        .await
        .expect("the same write set succeeds without the trigger");
    assert_eq!(last_connected(&store, &peer).await, Some(at(3)));

    sqlx::query(
        "CREATE TRIGGER block_audit BEFORE INSERT ON owned_audit BEGIN \
         SELECT RAISE(ABORT, 'audit blocked'); END",
    )
    .execute(&raw)
    .await
    .expect("reinstall trigger");
    let error = store
        .record_node_connected(node_connected(&peer, NodeKind::Access, 5))
        .await
        .expect_err("an unwritable audit row must fail the later write set");
    assert!(
        matches!(error, PortError::Backend(_)),
        "约束/触发器失败必须给出具名错误，got {error}"
    );
    assert_eq!(
        last_connected(&store, &peer).await,
        Some(at(3)),
        "推进失败必须整事务回滚"
    );
    raw.close().await;
    store.close().await;

    let pool = raw_pool(&path).await;
    assert_eq!(
        audit_rows(&pool, AuditAction::NodeAuthenticated).await,
        1,
        "只有成功的那一次留下审计行"
    );
    pool.close().await;
}

/// spec：拒绝或过期不创建信任；配对进入终态且此后不能被再次认领。
#[tokio::test]
async fn rejected_settlement_creates_no_trust_and_ends_the_pairing() {
    let dir = temp_dir("admin-pairing-reject");
    let store = open(&dir).await;
    store
        .create_pairing(PairingWrite {
            record: device_pairing(30),
            context: context(0, Vec::new()),
        })
        .await
        .expect("create pairing");
    store
        .claim_pairing(device_claim(&device_id()))
        .await
        .expect("claim");

    let outcome = store
        .settle_pairing(PairingSettlementWrite {
            pairing: pairing_id(),
            settlement: PairingSettlement::rejected(Some("wrong device")).expect("rejection"),
            context: context(
                2,
                vec![audit(
                    AuditAction::PairingRejected,
                    EntityRef::Pairing(pairing_id()),
                    AuditOutcome::Denied,
                )],
            ),
        })
        .await
        .expect("rejection settles");
    assert_eq!(outcome, TrustRecordRef::Device(device_id()));

    // 不新增信任记录：设备行与身份材料都不存在。
    assert!(store.device(&device_id()).await.expect("device").is_none());
    assert!(
        store
            .peer_key(&PeerIdentity::Device(device_id()))
            .await
            .expect("peer key")
            .is_none()
    );
    let pairing = store
        .pairing(&pairing_id())
        .await
        .expect("pairing")
        .expect("pairing row");
    assert_eq!(pairing.state(), PairingState::Rejected);
    assert_eq!(pairing.terminal_at(), Some(&at(2)));
    assert_eq!(pairing.approved_at(), None);

    // 终态后不能再认领，也不能再落定。
    assert_conflict(
        store
            .claim_pairing(device_claim(&device_id()))
            .await
            .expect_err("terminal pairing rejects claims"),
        ConflictKind::Consumed,
    );
    assert_conflict(
        store
            .settle_pairing(PairingSettlementWrite {
                pairing: pairing_id(),
                settlement: approved_settlement(),
                context: context(3, Vec::new()),
            })
            .await
            .expect_err("terminal pairing rejects settlement"),
        ConflictKind::Consumed,
    );

    let path = dir.join(storage_sqlite::migrate::DATABASE_FILE);
    store.close().await;
    let pool = raw_pool(&path).await;
    assert_eq!(audit_rows(&pool, AuditAction::PairingRejected).await, 1);
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_device").await,
        0
    );
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_peer_key").await,
        0
    );
    pool.close().await;
}

/// spec：重启终结已过期的未确认配对（已批准的信任不受影响）；过期配对在认领时即被拒绝。
#[tokio::test]
async fn expired_pairing_is_refused_on_claim_and_terminated_by_restart_sweep() {
    let dir = temp_dir("admin-pairing-expiry");
    let store = open(&dir).await;
    // 已批准的信任：过期扫描不得动它。
    approve_device(&store, &device_id(), 2).await;
    // 未确认且已过期的配对（有效期 5 分钟）。
    store
        .create_pairing(PairingWrite {
            record: PairingRecord::try_new(
                PairingId::new("88888888-8888-4888-8888-888888888888").expect("pairing id"),
                PairingTarget::Device,
                PairingState::Created,
                None,
                digest(PAIRING_SECRET),
                HOST_BINDING,
                at(0),
                at(5),
                None,
                None,
                None,
            )
            .expect("pairing record"),
            context: context(0, Vec::new()),
        })
        .await
        .expect("create pairing");
    store.close().await;

    // 重启后的恢复流程：`expire_pairings` 是它唯一入口。
    let reopened = open(&dir).await;
    let stale = PairingId::new("88888888-8888-4888-8888-888888888888").expect("pairing id");
    assert_conflict(
        reopened
            .claim_pairing(PairingClaimWrite {
                claim: {
                    let peer = PairingPeer::try_new(
                        PeerIdentity::Device(remote_device_id()),
                        "late phone",
                        peer_public_key(),
                        HOST_BINDING,
                        nonce("late-nonce"),
                    )
                    .expect("peer");
                    acp_core::model::PairingClaim::try_new(stale.clone(), peer).expect("claim")
                },
                context: context(6, Vec::new()),
            })
            .await
            .expect_err("an expired pairing must not be claimable"),
        ConflictKind::Expired,
    );
    assert_eq!(
        reopened
            .expire_pairings(ExpiryWrite {
                // `UseCases::expire_pairings` 传空的 `context.audit`：`pairing.expired` 由存储层
                // 按配对写入，actor 只从这里的首条取。
                context: context(6, Vec::new()),
                only: None,
            })
            .await
            .expect("expiry sweep"),
        1
    );
    let expired = reopened
        .pairing(&stale)
        .await
        .expect("pairing")
        .expect("pairing row");
    assert_eq!(expired.state(), PairingState::Expired);
    assert_eq!(expired.terminal_at(), Some(&at(6)));
    // 已批准的配对与信任记录不受影响。
    assert_eq!(
        reopened
            .pairing(&pairing_id())
            .await
            .expect("pairing")
            .expect("pairing row")
            .state(),
        PairingState::Approved
    );
    assert_eq!(
        reopened
            .device(&device_id())
            .await
            .expect("device")
            .expect("device row")
            .state(),
        DeviceState::Active
    );
    let path = dir.join(storage_sqlite::migrate::DATABASE_FILE);
    reopened.close().await;
    let pool = raw_pool(&path).await;
    assert_eq!(audit_rows(&pool, AuditAction::PairingExpired).await, 1);
    pool.close().await;
}

/// 按标识终结：只动列出的**未确认**配对，且不要求已到期（重启终结的形态）；已批准记录不受影响。
#[tokio::test]
async fn expire_pairings_by_id_terminates_only_the_listed_unconfirmed_rows() {
    let dir = temp_dir("admin-pairing-expiry-by-id");
    let store = open(&dir).await;
    // 已批准的配对（不在到期时刻）：按标识终结也不得改写它。
    approve_device(&store, &device_id(), 2).await;
    // 未确认的第二个配对：有效期到 t30，但重启后已无法验密，因此在 t2 就被终结。
    let pending = PairingId::new(OTHER_PAIRING).expect("pairing id");
    store
        .create_pairing(PairingWrite {
            record: PairingRecord::try_new(
                pending.clone(),
                PairingTarget::Device,
                PairingState::Created,
                None,
                digest(PAIRING_SECRET),
                HOST_BINDING,
                at(0),
                at(30),
                None,
                None,
                None,
            )
            .expect("pairing record"),
            context: context(0, Vec::new()),
        })
        .await
        .expect("create pairing");
    let peer = PairingPeer::try_new(
        PeerIdentity::Device(remote_device_id()),
        "late phone",
        peer_public_key(),
        HOST_BINDING,
        nonce("by-id-nonce"),
    )
    .expect("pairing peer");
    store
        .claim_pairing(PairingClaimWrite {
            claim: acp_core::model::PairingClaim::try_new(pending.clone(), peer).expect("claim"),
            context: context(
                1,
                vec![audit(
                    AuditAction::PairingClaimed,
                    EntityRef::Pairing(pending.clone()),
                    AuditOutcome::Success,
                )],
            ),
        })
        .await
        .expect("claim");

    // 空列表：零写入、不产生审计。
    assert_eq!(
        store
            .expire_pairings(ExpiryWrite {
                context: context(2, Vec::new()),
                only: Some(Vec::new()),
            })
            .await
            .expect("empty by-id sweep"),
        0
    );
    // 列出未确认的 pending 与已批准的 PAIRING：只有前者被终结。
    assert_eq!(
        store
            .expire_pairings(ExpiryWrite {
                context: context(2, Vec::new()),
                only: Some(vec![pending.clone(), pairing_id()]),
            })
            .await
            .expect("by-id sweep"),
        1
    );
    let terminated = store
        .pairing(&pending)
        .await
        .expect("pairing")
        .expect("pairing row");
    assert_eq!(terminated.state(), PairingState::Expired);
    assert_eq!(terminated.terminal_at(), Some(&at(2)));
    assert!(terminated.claimed_at().is_some(), "已认领的时间必须保留");
    assert_eq!(
        store
            .pairing(&pairing_id())
            .await
            .expect("pairing")
            .expect("pairing row")
            .state(),
        PairingState::Approved,
        "按标识终结不得改写已批准记录"
    );
    let all = store.pairings().await.expect("pairings");
    assert_eq!(all.len(), 2);
    assert!(
        all.windows(2)
            .all(|pair| pair[0].created_at() <= pair[1].created_at())
    );

    // 重复提交同一标识是幂等的：状态已终态，不再终结、不再写审计。
    assert_eq!(
        store
            .expire_pairings(ExpiryWrite {
                context: context(3, Vec::new()),
                only: Some(vec![pending.clone()]),
            })
            .await
            .expect("repeat by-id sweep"),
        0
    );

    let path = dir.join(storage_sqlite::migrate::DATABASE_FILE);
    store.close().await;
    let pool = raw_pool(&path).await;
    assert_eq!(audit_rows(&pool, AuditAction::PairingExpired).await, 1);
    pool.close().await;
}

// ---------------------------------------------------------------------------------------------
// 节点双角色与撤销（§11.6 第 2 条，§11.5）
// ---------------------------------------------------------------------------------------------

/// spec：双角色共享一条身份材料；两种角色都能按角色读取且指纹一致。
#[tokio::test]
async fn node_roles_share_one_identity_material() {
    let dir = temp_dir("admin-node-roles");
    let store = open(&dir).await;
    let fingerprint = peer_public_key().fingerprint();
    for kind in [NodeKind::Access, NodeKind::Owner] {
        store
            .put_node(NodeWrite {
                record: node_record(kind, fingerprint.clone(), NodeState::Paired),
                public_key: peer_public_key(),
                context: context(
                    1,
                    vec![audit(
                        AuditAction::NodePaired,
                        EntityRef::Node(node_id()),
                        AuditOutcome::Success,
                    )],
                ),
            })
            .await
            .expect("put node role");
    }

    assert_eq!(store.nodes_for(&node_id()).await.expect("nodes").len(), 2);
    assert_eq!(store.nodes().await.expect("nodes").len(), 2);
    for kind in [NodeKind::Access, NodeKind::Owner] {
        let row = store
            .node(&node_id(), kind)
            .await
            .expect("node")
            .expect("node row");
        assert_eq!(row.kind(), kind);
        assert_eq!(row.node_public_key_fingerprint(), &fingerprint);
    }

    let path = dir.join(storage_sqlite::migrate::DATABASE_FILE);
    store.close().await;
    let pool = raw_pool(&path).await;
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_node").await,
        2
    );
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_peer_key").await,
        1,
        "两种角色共享一条身份材料"
    );
    pool.close().await;
}

/// spec：角色指纹不一致被拒——冲突返回，已存在的角色行与身份材料都不变。
#[tokio::test]
async fn second_node_role_with_a_foreign_fingerprint_is_rejected() {
    let dir = temp_dir("admin-node-role-mismatch");
    let store = open(&dir).await;
    let fingerprint = peer_public_key().fingerprint();
    store
        .put_node(NodeWrite {
            record: node_record(NodeKind::Access, fingerprint, NodeState::Paired),
            public_key: peer_public_key(),
            context: context(1, Vec::new()),
        })
        .await
        .expect("put access role");
    let path = dir.join(storage_sqlite::migrate::DATABASE_FILE);
    let before = {
        let pool = raw_pool(&path).await;
        let snapshot = table_snapshot(&pool, "owned_node").await;
        pool.close().await;
        snapshot
    };
    let foreign = Fingerprint::new(FOREIGN_FINGERPRINT).expect("fingerprint");

    let error = store
        .put_node(NodeWrite {
            record: node_record(NodeKind::Owner, foreign, NodeState::Paired),
            public_key: peer_public_key(),
            context: context(2, Vec::new()),
        })
        .await
        .expect_err("a second role with a foreign fingerprint must be refused");
    assert_conflict(error, ConflictKind::IdentityMismatch);
    assert_eq!(store.nodes_for(&node_id()).await.expect("nodes").len(), 1);
    store.close().await;

    let pool = raw_pool(&path).await;
    assert_eq!(table_snapshot(&pool, "owned_node").await, before);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT fingerprint FROM owned_peer_key")
            .fetch_one(&pool)
            .await
            .expect("fingerprint"),
        peer_public_key().fingerprint().as_str()
    );
    pool.close().await;
}

/// spec：按节点撤销覆盖两种角色，并在重启后继续生效；已撤销身份不能经普通写入复活。
#[tokio::test]
async fn node_revocation_covers_both_roles_and_survives_reopen() {
    let dir = temp_dir("admin-node-revoke");
    let store = open(&dir).await;
    let fingerprint = peer_public_key().fingerprint();
    for kind in [NodeKind::Access, NodeKind::Owner] {
        store
            .put_node(NodeWrite {
                record: node_record(kind, fingerprint.clone(), NodeState::Paired),
                public_key: peer_public_key(),
                context: context(1, Vec::new()),
            })
            .await
            .expect("put node role");
    }
    store
        .revoke_node(NodeRevocation {
            node: node_id(),
            reason: RevokeReason::UserRequested,
            context: context(
                3,
                vec![audit(
                    AuditAction::NodeTrustRevoked,
                    EntityRef::Node(node_id()),
                    AuditOutcome::Success,
                )],
            ),
        })
        .await
        .expect("revoke node");
    for row in store.nodes_for(&node_id()).await.expect("nodes") {
        assert_eq!(row.state(), NodeState::Revoked);
        assert_eq!(row.revoked_at(), Some(&at(3)));
    }
    let path = dir.join(storage_sqlite::migrate::DATABASE_FILE);
    store.close().await;

    let reopened = open(&dir).await;
    for kind in [NodeKind::Access, NodeKind::Owner] {
        assert_eq!(
            reopened
                .node(&node_id(), kind)
                .await
                .expect("node")
                .expect("node row")
                .state(),
            NodeState::Revoked
        );
    }
    // 普通写入不得复活已撤销身份。
    let error = reopened
        .put_node(NodeWrite {
            record: node_record(
                NodeKind::Owner,
                peer_public_key().fingerprint(),
                NodeState::Paired,
            ),
            public_key: peer_public_key(),
            context: context(4, Vec::new()),
        })
        .await
        .expect_err("a revoked node must not be reactivated by a plain write");
    assert_conflict(error, ConflictKind::IdentityMismatch);
    reopened.close().await;

    let pool = raw_pool(&path).await;
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT revoke_reason FROM owned_node WHERE node_id = ?1 AND kind = 'access'"
        )
        .bind(NODE)
        .fetch_one(&pool)
        .await
        .expect("revoke reason"),
        "user_requested"
    );
    assert_eq!(audit_rows(&pool, AuditAction::NodeTrustRevoked).await, 1);
    pool.close().await;
}

/// spec：重启后撤销仍然有效；重复撤销保留首次的时间与原因；已撤销设备不能复活也不能换钥。
#[tokio::test]
async fn device_revocation_is_idempotent_and_survives_reopen() {
    let dir = temp_dir("admin-device-revoke");
    let store = open(&dir).await;
    approve_device(&store, &device_id(), 2).await;
    let path = dir.join(storage_sqlite::migrate::DATABASE_FILE);

    store
        .revoke_device(DeviceRevocation {
            device: device_id(),
            reason: RevokeReason::UserRequested,
            context: context(
                3,
                vec![audit(
                    AuditAction::DeviceRevoked,
                    EntityRef::Device(device_id()),
                    AuditOutcome::Success,
                )],
            ),
        })
        .await
        .expect("revoke device");
    // 重复撤销幂等：保留首次时间与原因。
    store
        .revoke_device(DeviceRevocation {
            device: device_id(),
            reason: RevokeReason::Compromised,
            context: context(4, Vec::new()),
        })
        .await
        .expect("second revoke is idempotent");
    let revoked = store
        .device(&device_id())
        .await
        .expect("device")
        .expect("device row");
    assert_eq!(revoked.state(), DeviceState::Revoked);
    assert_eq!(revoked.revoked_at(), Some(&at(3)));
    store.close().await;

    let pool = raw_pool(&path).await;
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT revoke_reason FROM owned_device WHERE device_id = ?1"
        )
        .bind(DEVICE)
        .fetch_one(&pool)
        .await
        .expect("revoke reason"),
        "user_requested"
    );
    assert_eq!(audit_rows(&pool, AuditAction::DeviceRevoked).await, 1);
    let before = table_snapshot(&pool, "owned_device").await;
    pool.close().await;

    // 重启后撤销仍生效，且不会被普通写入改写。
    let reopened = open(&dir).await;
    assert_eq!(
        reopened
            .device(&device_id())
            .await
            .expect("device")
            .expect("device row")
            .state(),
        DeviceState::Revoked
    );
    let error = reopened
        .put_device(DeviceWrite {
            record: device_record(peer_public_key().fingerprint(), DeviceState::Active),
            context: context(5, Vec::new()),
        })
        .await
        .expect_err("a revoked device must not be reactivated");
    assert_conflict(error, ConflictKind::IdentityMismatch);
    // 已撤销设备也不能经 `put_device` 重写为 revoked（撤销是 `revoke_device` 的职责）。
    assert_invalid_request(
        reopened
            .put_device(DeviceWrite {
                record: device_record(peer_public_key().fingerprint(), DeviceState::Revoked),
                context: context(5, Vec::new()),
            })
            .await
            .expect_err("revocation is not a plain write"),
        "use device.revoke to revoke a device",
    );
    reopened.close().await;

    let pool = raw_pool(&path).await;
    assert_eq!(table_snapshot(&pool, "owned_device").await, before);
    pool.close().await;
}

/// 回归：`RevokeReason::KeyChanged` 的落库 token 必须是 `key_changed`。
///
/// 设备与节点都走 `TrustStore::revoke_device`/`revoke_node`（`revoke_token()` 的唯一调用路径）：
/// 撤销请求只带 `KeyChanged`，库内的 token 只能由该映射产生。映射一旦拼错，UPDATE 会撞
/// `revoke_reason` 的取值 CHECK，事务回滚，本用例在 `expect` 处失败。DDL 允许集合另由
/// `enum_coverage.rs` 锚在 §7.3 合同文本上独立断言，两者来源不同，共同发现拼写错误。
#[tokio::test]
async fn key_changed_revocation_is_persisted_for_devices_and_nodes() {
    let dir = temp_dir("admin-key-changed");
    let store = open(&dir).await;
    let path = dir.join(storage_sqlite::migrate::DATABASE_FILE);

    approve_device(&store, &device_id(), 2).await;
    let fingerprint = peer_public_key().fingerprint();
    for kind in [NodeKind::Access, NodeKind::Owner] {
        store
            .put_node(NodeWrite {
                record: node_record(kind, fingerprint.clone(), NodeState::Paired),
                public_key: peer_public_key(),
                context: context(1, Vec::new()),
            })
            .await
            .expect("put node role");
    }

    store
        .revoke_device(DeviceRevocation {
            device: device_id(),
            reason: RevokeReason::KeyChanged,
            context: context(
                3,
                vec![audit(
                    AuditAction::DeviceRevoked,
                    EntityRef::Device(device_id()),
                    AuditOutcome::Success,
                )],
            ),
        })
        .await
        .expect("key_changed must be an accepted device revocation reason");
    store
        .revoke_node(NodeRevocation {
            node: node_id(),
            reason: RevokeReason::KeyChanged,
            context: context(
                3,
                vec![audit(
                    AuditAction::NodeTrustRevoked,
                    EntityRef::Node(node_id()),
                    AuditOutcome::Success,
                )],
            ),
        })
        .await
        .expect("key_changed must be an accepted node revocation reason");
    store.close().await;

    let pool = raw_pool(&path).await;
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT revoke_reason FROM owned_device WHERE device_id = ?1"
        )
        .bind(DEVICE)
        .fetch_one(&pool)
        .await
        .expect("device revoke reason"),
        "key_changed",
        "device revocation must round-trip the key_changed token"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT revoke_reason FROM owned_node WHERE node_id = ?1 AND kind = 'access'"
        )
        .bind(NODE)
        .fetch_one(&pool)
        .await
        .expect("node revoke reason"),
        "key_changed",
        "node revocation must round-trip the key_changed token"
    );
    pool.close().await;
}

/// 回归：`owned_device.revoke_reason` 的取值 CHECK 由数据库真正强制执行。
///
/// 控制组先用合法 token 走同一形状的 UPDATE（必须成功），再写非法 token（必须被拒）：
/// 语句同时满足 `(state='revoked') = (revoked_at IS NOT NULL)`，因此拒绝只可能来自取值 CHECK。
/// 断言针对真实约束的拒绝，不重复实现判定逻辑。
#[tokio::test]
async fn an_out_of_vocabulary_revoke_reason_is_rejected_by_the_check_constraint() {
    let dir = temp_dir("admin-revoke-reason-check");
    let store = open(&dir).await;
    approve_device(&store, &device_id(), 2).await;
    let path = dir.join(storage_sqlite::migrate::DATABASE_FILE);
    store.close().await;

    let pool = raw_write_pool(&path).await;
    let update = "UPDATE owned_device SET state = 'revoked', revoked_at = ?2, revoke_reason = ?3 \
                  WHERE device_id = ?1";
    sqlx::query(update)
        .bind(DEVICE)
        .bind(at(3).as_str())
        .bind("compromised")
        .execute(&pool)
        .await
        .expect("a legal token must be accepted");
    let rejected = sqlx::query(update)
        .bind(DEVICE)
        .bind(at(4).as_str())
        .bind("keychange")
        .execute(&pool)
        .await
        .expect_err("an out-of-vocabulary revoke reason must be rejected by the DDL CHECK");
    assert!(
        rejected.to_string().to_ascii_lowercase().contains("check"),
        "the rejection must come from the CHECK constraint, got: {rejected}"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT revoke_reason FROM owned_device WHERE device_id = ?1"
        )
        .bind(DEVICE)
        .fetch_one(&pool)
        .await
        .expect("revoke reason"),
        "compromised",
        "the rejected write must leave the control-group row untouched"
    );
    pool.close().await;
}

/// spec：已绑定身份换钥被拒——既不换材料也不改状态。
#[tokio::test]
async fn revoked_or_rekeyed_device_cannot_be_reactivated_or_rebound() {
    let dir = temp_dir("admin-device-rekey");
    let store = open(&dir).await;
    approve_device(&store, &device_id(), 2).await;
    let path = dir.join(storage_sqlite::migrate::DATABASE_FILE);
    store.close().await;
    let pool = raw_pool(&path).await;
    let before_device = table_snapshot(&pool, "owned_device").await;
    let before_key = table_snapshot(&pool, "owned_peer_key").await;
    pool.close().await;

    let store = open(&dir).await;
    let error = store
        .put_device(DeviceWrite {
            record: device_record(
                Fingerprint::new(FOREIGN_FINGERPRINT).expect("fingerprint"),
                DeviceState::Active,
            ),
            context: context(4, Vec::new()),
        })
        .await
        .expect_err("a bound identity must not accept a new key");
    assert_conflict(error, ConflictKind::IdentityMismatch);
    let device = store
        .device(&device_id())
        .await
        .expect("device")
        .expect("device row");
    assert_eq!(device.state(), DeviceState::Active);
    assert_eq!(
        device.public_key_fingerprint(),
        &peer_public_key().fingerprint()
    );
    store.close().await;

    let pool = raw_pool(&path).await;
    assert_eq!(table_snapshot(&pool, "owned_device").await, before_device);
    assert_eq!(table_snapshot(&pool, "owned_peer_key").await, before_key);
    pool.close().await;
}

/// `owned_device.last_seen_at`/`owned_node.last_connected_at` 只前进：旧值为空时首次写入不得被丢成 NULL、
/// 更早的时间戳不得让已存值倒退、空值不得抹掉已存值（spec：admin-state-persistence「设备与节点记录的
/// 活动时间只前进」）。列赋值被静默丢掉时该用例会失败。
#[tokio::test]
async fn timestamps_are_advanced_never_erased() {
    let dir = temp_dir("admin-timestamps");
    let store = open(&dir).await;
    let fingerprint = peer_public_key().fingerprint();
    approve_device(&store, &device_id(), 2).await;
    store
        .put_device(DeviceWrite {
            record: device_record_with_seen(fingerprint.clone(), Some(at(3))),
            context: context(3, Vec::new()),
        })
        .await
        .expect("advance last_seen_at");
    assert_eq!(
        store
            .device(&device_id())
            .await
            .expect("device")
            .expect("device row")
            .last_seen_at(),
        Some(&at(3))
    );
    // 更早的时间戳：保留已存的「最近」值，但同一写集的其他字段照常更新。
    store
        .put_device(DeviceWrite {
            record: DeviceRecord::try_new(
                device_id(),
                "phone-renamed",
                fingerprint.clone(),
                DeviceState::Active,
                at(2),
                Some(at(2)),
                None,
            )
            .expect("device record"),
            context: context(4, Vec::new()),
        })
        .await
        .expect("write an earlier timestamp");
    let device = store
        .device(&device_id())
        .await
        .expect("device")
        .expect("device row");
    assert_eq!(
        device.last_seen_at(),
        Some(&at(3)),
        "更早的 last_seen_at 不得覆盖已存值"
    );
    assert_eq!(
        device.display_name(),
        "phone-renamed",
        "同一写集的其他字段仍要更新"
    );
    store
        .put_device(DeviceWrite {
            record: device_record_with_seen(fingerprint.clone(), None),
            context: context(5, Vec::new()),
        })
        .await
        .expect("write without a new timestamp");
    assert_eq!(
        store
            .device(&device_id())
            .await
            .expect("device")
            .expect("device row")
            .last_seen_at(),
        Some(&at(3)),
        "既有 last_seen_at 不得被抹掉"
    );

    store
        .put_node(NodeWrite {
            record: node_record_with_connected(NodeKind::Owner, fingerprint.clone(), Some(at(4))),
            public_key: peer_public_key(),
            context: context(4, Vec::new()),
        })
        .await
        .expect("advance last_connected_at");
    // 更早的连接时间同样不得让已存值倒退。
    store
        .put_node(NodeWrite {
            record: node_record_with_connected(NodeKind::Owner, fingerprint.clone(), Some(at(3))),
            public_key: peer_public_key(),
            context: context(5, Vec::new()),
        })
        .await
        .expect("write an earlier connection time");
    assert_eq!(
        store
            .node(&node_id(), NodeKind::Owner)
            .await
            .expect("node")
            .expect("node row")
            .last_connected_at(),
        Some(&at(4)),
        "更早的 last_connected_at 不得覆盖已存值"
    );
    store
        .put_node(NodeWrite {
            record: node_record_with_connected(NodeKind::Owner, fingerprint, None),
            public_key: peer_public_key(),
            context: context(6, Vec::new()),
        })
        .await
        .expect("write without a new timestamp");
    assert_eq!(
        store
            .node(&node_id(), NodeKind::Owner)
            .await
            .expect("node")
            .expect("node row")
            .last_connected_at(),
        Some(&at(4)),
        "既有 last_connected_at 不得被抹掉"
    );
    store.close().await;
}

// ---------------------------------------------------------------------------------------------
// Export / Import（§11.6 第 7 条，§7.4）
// ---------------------------------------------------------------------------------------------

/// spec：同一 Export 归属冲突被拒；写集内部两处集合分歧被拒（实现偏差 3）。
#[tokio::test]
async fn directory_records_stay_local_and_keep_created_at() {
    let dir = temp_dir("admin-workspace");
    let store = open(&dir).await;
    let path = dir.join(storage_sqlite::migrate::DATABASE_FILE);
    let raw = raw_write_pool(&path).await;
    seed_directory(&raw, "project", at(1)).await;
    raw.close().await;

    let directories = store.directories().await.expect("directories");
    assert_eq!(directories.len(), 1);
    assert_eq!(
        directories[0].directory_id(),
        &workspace_directory("project")
    );
    assert_eq!(directories[0].canonical_path(), absolute_path("project"));
    assert_eq!(directories[0].created_at(), &at(1));

    store.close().await;
    let pool = raw_pool(&path).await;
    // canonical path 只允许落在本机的 `owned_directory.canonical_path`：Export（对端可见）与审计都不含它。
    assert_eq!(
        columns_containing(&pool, &absolute_path("project")).await,
        vec!["owned_directory.canonical_path".to_owned()],
        "规范化路径只能出现在本机目录行里"
    );
    pool.close().await;
}

/// spec：库内不出现秘密材料——只有公钥、指纹、名称、集合与时间戳。
#[tokio::test]
async fn no_secret_material_lands_in_any_column() {
    let dir = temp_dir("admin-secrets");
    let store = open(&dir).await;
    approve_device(&store, &device_id(), 2).await;
    store
        .revoke_device(DeviceRevocation {
            device: device_id(),
            reason: RevokeReason::Compromised,
            context: context(3, Vec::new()),
        })
        .await
        .expect("revoke device");
    let path = dir.join(storage_sqlite::migrate::DATABASE_FILE);
    store.close().await;

    let pool = raw_pool(&path).await;
    for secret in [PAIRING_SECRET, CREDENTIAL_VALUE] {
        assert!(
            columns_containing(&pool, secret).await.is_empty(),
            "{secret} must never appear in any column"
        );
    }
    // 身份材料只以 65 字节公钥 + 摘要形式落库。
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM owned_peer_key WHERE length(public_key) <> 65"
        )
        .fetch_one(&pool)
        .await
        .expect("public key length"),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT secret_digest FROM owned_pairing")
            .fetch_one(&pool)
            .await
            .expect("digest"),
        digest_text(PAIRING_SECRET)
    );
    pool.close().await;
}

// ---------------------------------------------------------------------------------------------
// 写集原子性与失败关闭
// ---------------------------------------------------------------------------------------------

/// spec：审计写入失败时状态不落库（整事务回滚，没有任何审计行被写入）。
#[tokio::test]
async fn a_failed_audit_write_rolls_back_the_whole_write_set() {
    let dir = temp_dir("admin-audit-rollback");
    let store = open(&dir).await;
    let path = dir.join(storage_sqlite::migrate::DATABASE_FILE);
    let raw = raw_write_pool(&path).await;
    sqlx::query(
        "CREATE TRIGGER block_audit BEFORE INSERT ON owned_audit BEGIN \
         SELECT RAISE(ABORT, 'audit blocked'); END",
    )
    .execute(&raw)
    .await
    .expect("install trigger");

    let error = store
        .put_node(NodeWrite {
            record: node_record(
                NodeKind::Access,
                peer_public_key().fingerprint(),
                NodeState::Paired,
            ),
            public_key: peer_public_key(),
            context: context(
                1,
                vec![audit(
                    AuditAction::NodePaired,
                    EntityRef::Node(node_id()),
                    AuditOutcome::Success,
                )],
            ),
        })
        .await
        .expect_err("an unwritable audit row must fail the write set");
    assert!(
        matches!(error, PortError::Backend(_)),
        "约束/触发器失败必须给出具名错误，got {error}"
    );
    assert!(store.nodes().await.expect("nodes").is_empty());
    sqlx::query("DROP TRIGGER block_audit")
        .execute(&raw)
        .await
        .expect("drop trigger");
    raw.close().await;
    let pool = raw_pool(&path).await;
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_audit").await,
        0,
        "失败写集不得留下审计行"
    );
    pool.close().await;

    // 触发器移除后同一写集成功（证明失败来自审计写入，而不是这条写集本身非法）。
    store
        .put_node(NodeWrite {
            record: node_record(
                NodeKind::Access,
                peer_public_key().fingerprint(),
                NodeState::Paired,
            ),
            public_key: peer_public_key(),
            context: context(1, Vec::new()),
        })
        .await
        .expect("the same write set succeeds without the trigger");
    assert_eq!(store.nodes().await.expect("nodes").len(), 1);
    store.close().await;
}

/// spec：约束冲突时不留下半条授权——不存在「配对已批准但没有持久信任」的组合。
///
/// 两次故障注入分别落在落定写集的**第一个**写（`owned_device` INSERT）与**最后一个**写
/// （`owned_audit` INSERT）上：后者必须在设备行与身份材料已经写入之后失败，因此能证明整事务回滚
/// （四个面都回到调用前的内容）；前者只能证明检测发生在首个写入之前。
#[tokio::test]
async fn a_constraint_failure_during_approval_leaves_no_half_authorization() {
    approval_rollback_under(
        "admin-approval-rollback-device",
        "CREATE TRIGGER block_device BEFORE INSERT ON owned_device BEGIN \
         SELECT RAISE(ABORT, 'device insert blocked'); END",
        "DROP TRIGGER block_device",
    )
    .await;
    approval_rollback_under(
        "admin-approval-rollback-audit",
        "CREATE TRIGGER block_audit BEFORE INSERT ON owned_audit BEGIN \
         SELECT RAISE(ABORT, 'audit blocked'); END",
        "DROP TRIGGER block_audit",
    )
    .await;
}

/// 一次故障注入的完整核对：写集失败后设备行、身份材料、配对状态与审计都停在调用前的内容；移除了
/// 注入之后同一写集必须成功（证明失败来自注入，而不是这条写集本身非法）。
async fn approval_rollback_under(name: &str, install: &str, drop_trigger: &str) {
    let dir = temp_dir(name);
    let store = open(&dir).await;
    store
        .create_pairing(PairingWrite {
            record: device_pairing(30),
            context: context(0, Vec::new()),
        })
        .await
        .expect("create pairing");
    store
        .claim_pairing(device_claim(&device_id()))
        .await
        .expect("claim");
    let path = dir.join(storage_sqlite::migrate::DATABASE_FILE);
    let raw = raw_write_pool(&path).await;
    sqlx::query(install)
        .execute(&raw)
        .await
        .expect("install trigger");

    let error = store
        .settle_pairing(PairingSettlementWrite {
            pairing: pairing_id(),
            settlement: approved_settlement(),
            context: context(
                2,
                vec![audit(
                    AuditAction::PairingApproved,
                    EntityRef::Pairing(pairing_id()),
                    AuditOutcome::Success,
                )],
            ),
        })
        .await
        .expect_err("a failing trust write must roll the whole settlement back");
    assert!(matches!(error, PortError::Backend(_)), "got {error}");
    let pairing = store
        .pairing(&pairing_id())
        .await
        .expect("pairing")
        .expect("pairing row");
    assert_eq!(
        pairing.state(),
        PairingState::PendingConfirmation,
        "配对不得停在 approved"
    );
    assert_eq!(pairing.approved_at(), None);
    assert!(store.device(&device_id()).await.expect("device").is_none());
    assert!(
        store
            .peer_key(&PeerIdentity::Device(device_id()))
            .await
            .expect("peer key")
            .is_none()
    );
    sqlx::query(drop_trigger)
        .execute(&raw)
        .await
        .expect("drop trigger");
    raw.close().await;
    let pool = raw_pool(&path).await;
    assert_eq!(
        audit_rows(&pool, AuditAction::PairingApproved).await,
        0,
        "失败写集不得留下审计行"
    );
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_device").await,
        0
    );
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_peer_key").await,
        0
    );
    pool.close().await;

    // 移除障碍后同一落定成功。
    approve_settlement_retry(&store).await;
    assert_eq!(
        store
            .device(&device_id())
            .await
            .expect("device")
            .expect("device row")
            .state(),
        DeviceState::Active
    );
    assert!(
        store
            .peer_key(&PeerIdentity::Device(device_id()))
            .await
            .expect("peer key")
            .is_some()
    );
    store.close().await;
}

/// 落定重试（前一个用例清掉故障注入后调用）。
async fn approve_settlement_retry(store: &SqliteStore) {
    store
        .settle_pairing(PairingSettlementWrite {
            pairing: pairing_id(),
            settlement: approved_settlement(),
            context: context(3, Vec::new()),
        })
        .await
        .expect("settlement succeeds once the blocker is gone");
}

/// spec：损坏库的写路径全部被拒，只读查询仍可用（管理写集与 owned 路径同门）。
#[tokio::test]
async fn fail_closed_store_rejects_every_admin_write_path() {
    let dir = temp_dir("admin-fail-closed");
    let store = open(&dir).await;
    let path = dir.join(storage_sqlite::migrate::DATABASE_FILE);
    store.close().await;
    corrupt_last_page(&path);

    let opened = SqliteStore::open(StorageConfig::new(&dir), &at(5)).await;
    let Ok(store) = opened else {
        // 具名错误也是可接受结局（见 `migration.rs` 的同名判据）；这里只要求不是 panic。
        let error = opened.expect_err("either a named error or read-only fail-closed");
        assert!(
            matches!(
                error,
                StorageError::Corrupt(_)
                    | StorageError::Sql(_)
                    | StorageError::FileFormatTooNew { .. }
            ),
            "must be a named error, got {error}"
        );
        return;
    };
    let health = store.health().await.expect("health");
    assert!(health.read_only && !health.integrity_ok);

    // 管理写路径全部被拒。
    assert!(matches!(
        store
            .create_pairing(PairingWrite {
                record: device_pairing(30),
                context: context(5, Vec::new()),
            })
            .await
            .expect_err("pairing write must be refused"),
        PortError::Corrupt(_)
    ));
    assert!(matches!(
        store
            .put_node(NodeWrite {
                record: node_record(
                    NodeKind::Access,
                    peer_public_key().fingerprint(),
                    NodeState::Paired
                ),
                public_key: peer_public_key(),
                context: context(5, Vec::new()),
            })
            .await
            .expect_err("node write must be refused"),
        PortError::Corrupt(_)
    ));
    // 只读查询要么仍可用，要么以具名损坏错误失败关闭；不得 panic 或返回伪造数据。
    match store.directories().await {
        Ok(directories) => assert!(directories.is_empty()),
        Err(PortError::Corrupt(_)) | Err(PortError::Unavailable(_)) => {}
        Err(other) => panic!("只读查询只允许具名损坏/不可用错误，实际 {other:?}"),
    }
    store.close().await;
}

/// spec：超限时拒绝新写入而不是删信任；撤销等安全动作不被容量卡住。
#[tokio::test]
async fn capacity_limit_refuses_new_admin_writes_without_deleting_trust() {
    let dir = temp_dir("admin-capacity");
    let store = open(&dir).await;
    approve_device(&store, &device_id(), 2).await;
    let path = dir.join(storage_sqlite::migrate::DATABASE_FILE);
    store.close().await;

    let pool = raw_pool(&path).await;
    let measured = measured_storage_bytes(&pool).await;
    pool.close().await;

    let mut config = StorageConfig::new(&dir);
    config.max_total_size_bytes = measured;
    let store = SqliteStore::open(config, &at(3)).await.expect("reopen");

    assert_unavailable(
        store
            .put_node(NodeWrite {
                record: node_record(
                    NodeKind::Access,
                    peer_public_key().fingerprint(),
                    NodeState::Paired,
                ),
                public_key: peer_public_key(),
                context: context(3, Vec::new()),
            })
            .await
            .expect_err("a new write must be refused at capacity"),
        UnavailableKind::StorageFull,
    );
    // 撤销不增长库，必须仍然可用。
    store
        .revoke_device(DeviceRevocation {
            device: device_id(),
            reason: RevokeReason::UserRequested,
            context: context(
                3,
                vec![audit(
                    AuditAction::DeviceRevoked,
                    EntityRef::Device(device_id()),
                    AuditOutcome::Success,
                )],
            ),
        })
        .await
        .expect("revocation must not be blocked by capacity");
    assert_eq!(
        store
            .device(&device_id())
            .await
            .expect("device")
            .expect("device row")
            .state(),
        DeviceState::Revoked
    );
    store.close().await;

    let pool = raw_pool(&path).await;
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_node").await,
        0,
        "被拒绝的写集不得留下任何行"
    );
    assert_eq!(
        scalar_i64(&pool, "SELECT COUNT(*) FROM owned_device").await,
        1,
        "清理不得删除活动信任与撤销记录"
    );
    assert_eq!(
        scalar_i64(
            &pool,
            "SELECT COUNT(*) FROM owned_audit WHERE action = 'device.revoked'"
        )
        .await,
        1
    );
    pool.close().await;
}
