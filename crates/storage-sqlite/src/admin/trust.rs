//! `TrustStore` 的 SQLite 实现（设备、节点双角色、配对、身份材料）。
//!
//! 写集语义逐条对应 `docs/CORE_PORTS_AND_STORAGE.md` §11.6 的第 1–6 条；每条写路径都是一个
//! `BEGIN IMMEDIATE` 事务，状态、集合字段与审计一次提交。读路径（`device`/`devices`/`node`/`nodes`/
//! `nodes_for`/`peer_key`/`pairing`/`pairing_peer`）不写任何行。
//!
//! 配对的两条机器事实都来自**配对记录本身**，不需要在写集里额外携带：
//!
//! 1. 本机绑定（§11.2 第 1 条）：`PairingRecord::host_binding` 在登记时写入，认领时要求对端逐字回显
//!    （`PairingPeer::host_binding`），不一致按身份不匹配拒绝。
//! 2. 节点对端的角色：节点配对行只由 `node.pair.begin --mode owner` 创建，而 `LOCAL_ADMIN_PROTOCOL.md`
//!    §5.4 明确「`--mode access` 本机没有 `confirm` 调用」（claim 的 `nodeKind` 固定为 `access`），因此
//!    批准时的对端角色恒为 `NodeKind::Access`，无需在写集里携带（见 `approve_node`）。
//!
//! `put_device`/`put_node` 拒绝 `revoked` 状态：撤销是 `revoke_device`/`revoke_node` 的职责，只有它们
//! 携带 `RevokeReason`（`owned_device`/`owned_node` 的 CHECK 要求 `state = 'revoked'` 与
//! `revoke_reason` 同有同无）。`owned_device.public_key` 是 BLOB 且长度必须为 65：这条材料只有一个来源
//! ——`owned_peer_key`（配对确认写入），因此 `put_device` 从它读回，绝不把指纹当公钥写入。

use async_trait::async_trait;
use sqlx::SqlitePool;
use sqlx::sqlite::SqliteRow;

use acp_core::model::PairingSettlement;
use acp_core::model::{
    ConflictKind, DeviceRecord, DeviceState, EntityRef, Fingerprint, GrantSet, NodeId, NodeKind,
    NodeRecord, NodeState, PairingId, PairingPeer, PairingRecord, PairingState, PairingTarget,
    PeerIdentity, PeerPublicKey, PortError, ScopeSet, Timestamp,
};
use acp_core::ports::{
    DeviceRevocation, DeviceWrite, ExpiryWrite, NodeRevocation, NodeWrite, PairingClaimOutcome,
    PairingClaimWrite, PairingSettlementWrite, PairingWrite, RevokeReason, TrustRecordRef,
    TrustStore,
};

use crate::admin::{
    decode_strings, encode_strings, enforce_capacity_gate, insert_audit_rows, insert_expiry_audit,
};
use crate::error::StorageError;
use crate::session_store::{Db, SqliteStore, blob, decode, decode_opt, opt_text, text};

/// `owned_device` 的读列（顺序无关；集中一处便于与 §7.3 对照）。
/// `public_key` 参与读取：设备记录的指纹列必须与它派生出的指纹一致（见 `device_from_row`）。
const DEVICE_COLUMNS: &str = "device_id, display_name, public_key, fingerprint, scopes_json, state, \
     created_at, last_seen_at, revoked_at";

/// `owned_node` 的读列。
const NODE_COLUMNS: &str = "node_id, kind, display_name, fingerprint, grants_json, state, \
     owner_endpoint, created_at, last_connected_at, revoked_at";

/// `owned_pairing` 的读列。
const PAIRING_COLUMNS: &str = "pairing_id, target_kind, state, display_name, requested_scopes_json, \
     requested_grants_json, secret_digest, host_binding, created_at, expires_at, claimed_at, \
     approved_at, terminal_at";

/// `owned_pairing_peer` 的读列（不含绑定：该列只在 `owned_pairing` 上）。
const PAIRING_PEER_COLUMNS: &str = "peer_kind, peer_id, display_name, public_key, fingerprint, \
     client_nonce";

// ---------------------------------------------------------------------------------------------
// 行 → 值对象
// ---------------------------------------------------------------------------------------------

fn device_from_row(row: &SqliteRow) -> Result<DeviceRecord, StorageError> {
    // 设备记录的指纹列必须与同行 `public_key` 的派生值一致（§3.5 的唯一指纹入口）：DDL 只有各自列上的
    // 长度/字符集 CHECK，没有跨列约束，因此这里不比对就会把「指纹与公钥互相矛盾」的行当作有效记录
    // 返回。`device()`/`devices()` 共用本函数，两条读取路径一起失败关闭。
    let fingerprint =
        decode::<Fingerprint>(&text(row, "fingerprint")?, "owned_device.fingerprint")?;
    verify_identity_material(
        peer_key_from_bytes(blob(row, "public_key")?, "owned_device.public_key")?,
        &fingerprint,
        "owned_device fingerprint does not match its public key",
    )?;
    DeviceRecord::try_new(
        decode(&text(row, "device_id")?, "owned_device.device_id")?,
        &text(row, "display_name")?,
        fingerprint,
        acp_core::model::ScopeSet::try_from_iter(decode_strings(
            &text(row, "scopes_json")?,
            "owned_device.scopes_json is not a JSON array",
        )?)?,
        decode(&text(row, "state")?, "owned_device.state")?,
        decode(&text(row, "created_at")?, "owned_device.created_at")?,
        decode_opt(opt_text(row, "last_seen_at")?, "owned_device.last_seen_at")?,
        decode_opt(opt_text(row, "revoked_at")?, "owned_device.revoked_at")?,
    )
    .map_err(StorageError::from)
}

fn node_from_row(row: &SqliteRow) -> Result<NodeRecord, StorageError> {
    NodeRecord::try_new(
        decode(&text(row, "node_id")?, "owned_node.node_id")?,
        &text(row, "display_name")?,
        decode(&text(row, "kind")?, "owned_node.kind")?,
        decode(&text(row, "fingerprint")?, "owned_node.fingerprint")?,
        acp_core::model::GrantSet::try_from_iter(decode_strings(
            &text(row, "grants_json")?,
            "owned_node.grants_json is not a JSON array",
        )?)?,
        decode(&text(row, "state")?, "owned_node.state")?,
        opt_text(row, "owner_endpoint")?,
        decode(&text(row, "created_at")?, "owned_node.created_at")?,
        decode_opt(
            opt_text(row, "last_connected_at")?,
            "owned_node.last_connected_at",
        )?,
        decode_opt(opt_text(row, "revoked_at")?, "owned_node.revoked_at")?,
    )
    .map_err(StorageError::from)
}

fn pairing_from_row(row: &SqliteRow) -> Result<PairingRecord, StorageError> {
    let host_binding = text(row, "host_binding")?;
    if host_binding.is_empty() {
        // 空绑定不是「调用方参数错误」：该列在 v2 DDL 里是 NOT NULL，只有外部改写或本轮之前的构建
        // 才会写下空串（`PairingRecord::try_new` 现在拒绝空值）。按损坏给出具名错误，避免把库内
        // 状态问题误报成请求问题。
        return Err(StorageError::Corrupt("owned_pairing.host_binding is empty"));
    }
    PairingRecord::try_new(
        decode(&text(row, "pairing_id")?, "owned_pairing.pairing_id")?,
        decode(&text(row, "target_kind")?, "owned_pairing.target_kind")?,
        decode(&text(row, "state")?, "owned_pairing.state")?,
        opt_text(row, "display_name")?,
        acp_core::model::ScopeSet::try_from_iter(decode_strings(
            &text(row, "requested_scopes_json")?,
            "owned_pairing.requested_scopes_json is not a JSON array",
        )?)?,
        acp_core::model::GrantSet::try_from_iter(decode_strings(
            &text(row, "requested_grants_json")?,
            "owned_pairing.requested_grants_json is not a JSON array",
        )?)?,
        decode(&text(row, "secret_digest")?, "owned_pairing.secret_digest")?,
        &host_binding,
        decode(&text(row, "created_at")?, "owned_pairing.created_at")?,
        decode(&text(row, "expires_at")?, "owned_pairing.expires_at")?,
        decode_opt(opt_text(row, "claimed_at")?, "owned_pairing.claimed_at")?,
        decode_opt(opt_text(row, "approved_at")?, "owned_pairing.approved_at")?,
        decode_opt(opt_text(row, "terminal_at")?, "owned_pairing.terminal_at")?,
    )
    .map_err(StorageError::from)
}

fn peer_identity(row: &SqliteRow, id_column: &'static str) -> Result<PeerIdentity, StorageError> {
    let kind = text(row, "peer_kind")?;
    let id = text(row, id_column)?;
    match kind.as_str() {
        "device" => Ok(PeerIdentity::Device(decode(
            &id,
            "owned_pairing_peer.peer_id",
        )?)),
        "node" => Ok(PeerIdentity::Node(decode(
            &id,
            "owned_pairing_peer.peer_id",
        )?)),
        _ => Err(StorageError::ColumnValue {
            column: "owned_pairing_peer.peer_kind",
            expected: "device|node",
        }),
    }
}

/// `owned_pairing_peer` **不单独存绑定**：认领时已校验对端回显与登记值逐字相等，因此对端行的绑定
/// 恒等于配对行的 `host_binding`，读路径从配对行回填（§11.2 第 1 条；这也让「两条绑定不一致」在
/// 库里不可能存在）。
fn pairing_peer_from_row(row: &SqliteRow, host_binding: &str) -> Result<PairingPeer, StorageError> {
    // 与 `load_peer_key` 同口径：公钥是权威字节，指纹由它派生，但读取时必须与同行指纹列核对，
    // 不一致即损坏（配对确认会把这笔材料转入信任材料，矛盾材料不得继续流动）。
    let stored =
        decode::<Fingerprint>(&text(row, "fingerprint")?, "owned_pairing_peer.fingerprint")?;
    let public_key = verify_identity_material(
        peer_key_from_bytes(blob(row, "public_key")?, "owned_pairing_peer.public_key")?,
        &stored,
        "owned_pairing_peer fingerprint does not match its public key",
    )?;
    PairingPeer::try_new(
        peer_identity(row, "peer_id")?,
        &text(row, "display_name")?,
        public_key,
        host_binding,
        decode(
            &text(row, "client_nonce")?,
            "owned_pairing_peer.client_nonce",
        )?,
    )
    .map_err(StorageError::from)
}

/// `owned_peer_key.public_key`/`owned_pairing_peer.public_key` 是 **BLOB**（§11.7 的跨族列类型约定）：
/// adapter 绑定字节，绑定 hex/base64 文本会被 STRICT 直接拒绝。这里把 BLOB 还原成值对象。
fn peer_key_from_bytes(
    bytes: Vec<u8>,
    column: &'static str,
) -> Result<PeerPublicKey, StorageError> {
    PeerPublicKey::try_from_bytes(&bytes).map_err(|_| StorageError::ColumnValue {
        column,
        expected: "65-byte SEC1 uncompressed P-256 public key",
    })
}

/// 同行指纹核对：`fingerprint` 列 MUST 等于由同行 `public_key` 派生出的指纹（§3.5 的唯一指纹入口，
/// 适配器不得各自现算）。不一致说明该行材料自相矛盾（外部改写或损坏），按 §8 失败关闭：绝不把矛盾
/// 材料当作可用的验签公钥或有效记录返回。
///
/// 这条判定只能在读取期做：DDL 无法把 `fingerprint` 与 `public_key` 绑成跨列 CHECK（SQL 算不了
/// SHA-256），而 `owned_peer_key`/`owned_pairing_peer`/`owned_device` 三张表都各自只有列级 CHECK。
fn verify_identity_material(
    public_key: PeerPublicKey,
    stored: &Fingerprint,
    mismatch: &'static str,
) -> Result<PeerPublicKey, StorageError> {
    if public_key.fingerprint() != *stored {
        return Err(StorageError::Corrupt(mismatch));
    }
    Ok(public_key)
}

async fn load_device(
    pool: &SqlitePool,
    id: &acp_core::model::DeviceId,
) -> Result<Option<DeviceRecord>, StorageError> {
    let sql = format!("SELECT {DEVICE_COLUMNS} FROM owned_device WHERE device_id = ?1");
    let row = sqlx::query(&sql)
        .bind(id.as_str())
        .fetch_optional(pool)
        .await
        .db()?;
    row.as_ref().map(device_from_row).transpose()
}

async fn load_node(
    pool: &SqlitePool,
    id: &NodeId,
    kind: NodeKind,
) -> Result<Option<NodeRecord>, StorageError> {
    let sql = format!("SELECT {NODE_COLUMNS} FROM owned_node WHERE node_id = ?1 AND kind = ?2");
    let row = sqlx::query(&sql)
        .bind(id.as_str())
        .bind(kind.as_str())
        .fetch_optional(pool)
        .await
        .db()?;
    row.as_ref().map(node_from_row).transpose()
}

async fn load_nodes_for<'e, E>(executor: E, id: &NodeId) -> Result<Vec<NodeRecord>, StorageError>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let sql = format!("SELECT {NODE_COLUMNS} FROM owned_node WHERE node_id = ?1 ORDER BY kind ASC");
    let rows = sqlx::query(&sql)
        .bind(id.as_str())
        .fetch_all(executor)
        .await
        .db()?;
    rows.iter().map(node_from_row).collect()
}

/// 已绑定的身份材料；`None` 表示该对端还没有绑定材料。指纹列不作**权威来源**（权威是公钥字节本身，
/// 指纹由 [`PeerPublicKey::fingerprint`] 从同一份字节派生，§3.5：适配器不得各自现算指纹），但读取时
/// MUST 与同行 `fingerprint` 列核对：不一致就是损坏材料，失败关闭而不是把公钥交出去。
async fn load_peer_key<'e, E>(
    executor: E,
    peer_kind: &str,
    peer_id: &str,
) -> Result<Option<PeerPublicKey>, StorageError>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let row = sqlx::query(
        "SELECT public_key, fingerprint FROM owned_peer_key WHERE peer_kind = ?1 AND peer_id = ?2",
    )
    .bind(peer_kind)
    .bind(peer_id)
    .fetch_optional(executor)
    .await
    .db()?;
    let Some(row) = row else {
        return Ok(None);
    };
    let stored = decode::<Fingerprint>(&text(&row, "fingerprint")?, "owned_peer_key.fingerprint")?;
    Ok(Some(verify_identity_material(
        peer_key_from_bytes(blob(&row, "public_key")?, "owned_peer_key.public_key")?,
        &stored,
        "owned_peer_key fingerprint does not match its public key",
    )?))
}

/// 该设备是否已存在、其公钥指纹与状态；`put_device` 的换绑/复活判据。
async fn device_identity<'e, E>(
    executor: E,
    id: &acp_core::model::DeviceId,
) -> Result<Option<(Fingerprint, DeviceState)>, StorageError>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let row = sqlx::query("SELECT fingerprint, state FROM owned_device WHERE device_id = ?1")
        .bind(id.as_str())
        .fetch_optional(executor)
        .await
        .db()?;
    row.map(|row| {
        Ok((
            decode::<Fingerprint>(&text(&row, "fingerprint")?, "owned_device.fingerprint")?,
            decode::<DeviceState>(&text(&row, "state")?, "owned_device.state")?,
        ))
    })
    .transpose()
}

#[async_trait]
impl TrustStore for SqliteStore {
    async fn device(
        &self,
        id: &acp_core::model::DeviceId,
    ) -> Result<Option<DeviceRecord>, PortError> {
        Ok(load_device(&self.pools().read, id).await?)
    }

    async fn devices(&self) -> Result<Vec<DeviceRecord>, PortError> {
        let sql = format!(
            "SELECT {DEVICE_COLUMNS} FROM owned_device ORDER BY created_at ASC, device_id ASC"
        );
        let rows = sqlx::query(&sql).fetch_all(&self.pools().read).await.db()?;
        let mut records = Vec::with_capacity(rows.len());
        for row in &rows {
            records.push(device_from_row(row)?);
        }
        Ok(records)
    }

    async fn node(&self, id: &NodeId, kind: NodeKind) -> Result<Option<NodeRecord>, PortError> {
        Ok(load_node(&self.pools().read, id, kind).await?)
    }

    async fn nodes(&self) -> Result<Vec<NodeRecord>, PortError> {
        let sql = format!(
            "SELECT {NODE_COLUMNS} FROM owned_node ORDER BY created_at ASC, node_id ASC, kind ASC"
        );
        let rows = sqlx::query(&sql).fetch_all(&self.pools().read).await.db()?;
        let mut records = Vec::with_capacity(rows.len());
        for row in &rows {
            records.push(node_from_row(row)?);
        }
        Ok(records)
    }

    async fn nodes_for(&self, id: &NodeId) -> Result<Vec<NodeRecord>, PortError> {
        Ok(load_nodes_for(&self.pools().read, id).await?)
    }

    async fn peer_key(&self, peer: &PeerIdentity) -> Result<Option<PeerPublicKey>, PortError> {
        Ok(load_peer_key(&self.pools().read, peer.kind(), peer.id_text()).await?)
    }

    async fn pairing(&self, id: &PairingId) -> Result<Option<PairingRecord>, PortError> {
        let sql = format!("SELECT {PAIRING_COLUMNS} FROM owned_pairing WHERE pairing_id = ?1");
        let row = sqlx::query(&sql)
            .bind(id.as_str())
            .fetch_optional(&self.pools().read)
            .await
            .db()?;
        Ok(row.as_ref().map(pairing_from_row).transpose()?)
    }

    async fn pairing_peer(&self, id: &PairingId) -> Result<Option<PairingPeer>, PortError> {
        let sql =
            format!("SELECT {PAIRING_PEER_COLUMNS} FROM owned_pairing_peer WHERE pairing_id = ?1");
        let row = sqlx::query(&sql)
            .bind(id.as_str())
            .fetch_optional(&self.pools().read)
            .await
            .db()?;
        let Some(row) = row else {
            return Ok(None);
        };
        // 对端行的绑定从配对行回填（两条绑定在库内不可能不一致）。
        let binding: Option<String> =
            sqlx::query_scalar("SELECT host_binding FROM owned_pairing WHERE pairing_id = ?1")
                .bind(id.as_str())
                .fetch_optional(&self.pools().read)
                .await
                .db()?;
        let binding = binding.ok_or(StorageError::Corrupt("peer row without its pairing row"))?;
        Ok(Some(pairing_peer_from_row(&row, &binding)?))
    }

    /// §11.6 第 1 条：同 ID 不得换绑公钥，也不得把 `revoked` 改回 `active`。
    async fn put_device(&self, write: DeviceWrite) -> Result<(), PortError> {
        self.writable()?;
        let record = write.record;
        let device_id = record.device_id().clone();
        let mut tx = self
            .pools()
            .write
            .begin_with("BEGIN IMMEDIATE")
            .await
            .db()?;
        if record.state() == DeviceState::Revoked {
            // 撤销是 `revoke_device` 的职责：只有那条写集携带 `RevokeReason`。
            return Err(PortError::InvalidRequest(
                "use device.revoke to revoke a device",
            ));
        }
        let fingerprint = record.public_key_fingerprint().clone();
        // 已绑定的身份材料是「同 ID 不得换绑公钥」的判据：比对的是公钥字节派生出的指纹，而不是
        // 指纹列的文本（列可以被外部改写，材料才是权威）。
        let bound = load_peer_key(&mut *tx, "device", device_id.as_str()).await?;
        if let Some(key) = &bound {
            if key.fingerprint() != fingerprint {
                return Err(PortError::Conflict(ConflictKind::IdentityMismatch));
            }
        }
        if let Some((stored, state)) = device_identity(&mut *tx, &device_id).await? {
            if stored != fingerprint || state == DeviceState::Revoked {
                return Err(PortError::Conflict(ConflictKind::IdentityMismatch));
            }
        }
        // `owned_device.public_key` 的长度 CHECK 要求 65 字节：材料只有一个来源（配对确认写入的
        // `owned_peer_key`），既有行也允许沿用，除此之外不能凭空生成。
        let public_key = match bound {
            Some(key) => key,
            None => load_existing_device_key(&mut *tx, device_id.as_str())
                .await?
                .ok_or(StorageError::InvalidRequest(
                    "device identity material is not bound",
                ))?,
        };
        if public_key.fingerprint() != fingerprint {
            // 既有行的公钥与记录指纹不一致 = 库被外部改写（§9 判据 29 的失败关闭）。
            return Err(PortError::Corrupt(
                "device public key does not match its fingerprint",
            ));
        }
        upsert_device(&mut tx, &record, public_key.as_bytes()).await?;
        insert_audit_rows(&mut tx, &write.context.at, &write.context.audit).await?;
        enforce_capacity_gate(self, &mut tx, &write.context.at).await?;
        tx.commit().await.db()?;
        Ok(())
    }

    /// §11.6 第 2 条：写节点角色行与身份材料；同一 NodeId 的两种角色必须指纹一致，撤销过的身份
    /// 不能经普通写入激活。
    async fn put_node(&self, write: NodeWrite) -> Result<(), PortError> {
        self.writable()?;
        let record = write.record;
        let node_id = record.node_id().clone();
        let mut tx = self
            .pools()
            .write
            .begin_with("BEGIN IMMEDIATE")
            .await
            .db()?;
        if record.state() == NodeState::Revoked {
            return Err(PortError::InvalidRequest(
                "use node.revoke to revoke a node",
            ));
        }
        let fingerprint = write.public_key.fingerprint();
        if &fingerprint != record.node_public_key_fingerprint() {
            // 记录里的指纹必须由同一枚公钥派生（§3.5）：不一致说明调用方拼错了写集。
            return Err(PortError::Conflict(ConflictKind::IdentityMismatch));
        }
        if let Some(key) = load_peer_key(&mut *tx, "node", node_id.as_str()).await? {
            if key.fingerprint() != fingerprint {
                return Err(PortError::Conflict(ConflictKind::IdentityMismatch));
            }
        }
        for row in load_nodes_for(&mut *tx, &node_id).await? {
            if row.node_public_key_fingerprint() != &fingerprint {
                return Err(PortError::Conflict(ConflictKind::IdentityMismatch));
            }
            if row.state() == NodeState::Revoked {
                return Err(PortError::Conflict(ConflictKind::IdentityMismatch));
            }
        }
        upsert_node(&mut tx, &record).await?;
        sqlx::query(
            "INSERT INTO owned_peer_key (peer_kind, peer_id, public_key, fingerprint, bound_at) \
             VALUES ('node', ?1, ?2, ?3, ?4) \
             ON CONFLICT(peer_kind, peer_id) DO UPDATE SET public_key = excluded.public_key, \
             fingerprint = excluded.fingerprint, bound_at = excluded.bound_at",
        )
        .bind(node_id.as_str())
        .bind(write.public_key.as_bytes().to_vec())
        .bind(fingerprint_text(&fingerprint))
        .bind(write.context.at.as_str())
        .execute(&mut *tx)
        .await
        .db()
        .map_err(|error| error.into_conflict(ConflictKind::IdentityMismatch))?;
        insert_audit_rows(&mut tx, &write.context.at, &write.context.audit).await?;
        enforce_capacity_gate(self, &mut tx, &write.context.at).await?;
        tx.commit().await.db()?;
        Ok(())
    }

    /// §11.6 第 5 条：单事务写撤销时间、状态与审计；提交后由组合根关闭连接。重复撤销幂等——
    /// 保留首次的撤销时间与原因。
    async fn revoke_device(&self, write: DeviceRevocation) -> Result<(), PortError> {
        self.writable()?;
        let mut tx = self
            .pools()
            .write
            .begin_with("BEGIN IMMEDIATE")
            .await
            .db()?;
        let exists: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM owned_device WHERE device_id = ?1")
                .bind(write.device.as_str())
                .fetch_optional(&mut *tx)
                .await
                .db()?;
        if exists.is_none() {
            return Err(PortError::NotFound(EntityRef::Device(write.device.clone())));
        }
        sqlx::query(
            "UPDATE owned_device SET state = 'revoked', revoked_at = COALESCE(revoked_at, ?2), \
             revoke_reason = COALESCE(revoke_reason, ?3) WHERE device_id = ?1",
        )
        .bind(write.device.as_str())
        .bind(write.context.at.as_str())
        .bind(revoke_token(write.reason))
        .execute(&mut *tx)
        .await
        .db()?;
        insert_audit_rows(&mut tx, &write.context.at, &write.context.audit).await?;
        tx.commit().await.db()?;
        Ok(())
    }

    /// §11.6 第 5 条：按 NodeId 撤销，同一事务覆盖两种角色（`owned_peer_key` 作为 tombstone 保留）。
    async fn revoke_node(&self, write: NodeRevocation) -> Result<(), PortError> {
        self.writable()?;
        let mut tx = self
            .pools()
            .write
            .begin_with("BEGIN IMMEDIATE")
            .await
            .db()?;
        let rows =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM owned_node WHERE node_id = ?1")
                .bind(write.node.as_str())
                .fetch_one(&mut *tx)
                .await
                .db()?;
        if rows == 0 {
            return Err(PortError::NotFound(EntityRef::Node(write.node.clone())));
        }
        sqlx::query(
            "UPDATE owned_node SET state = 'revoked', revoked_at = COALESCE(revoked_at, ?2), \
             revoke_reason = COALESCE(revoke_reason, ?3) WHERE node_id = ?1",
        )
        .bind(write.node.as_str())
        .bind(write.context.at.as_str())
        .bind(revoke_token(write.reason))
        .execute(&mut *tx)
        .await
        .db()?;
        insert_audit_rows(&mut tx, &write.context.at, &write.context.audit).await?;
        tx.commit().await.db()?;
        Ok(())
    }

    /// §11.6 第 3 条：登记一次性配对（`created` 状态；重复 ID 显式冲突）。
    async fn create_pairing(&self, write: PairingWrite) -> Result<(), PortError> {
        self.writable()?;
        let record = write.record;
        if record.state() != PairingState::Created {
            return Err(PortError::InvalidRequest(
                "a pairing must be registered in the created state",
            ));
        }
        let mut tx = self
            .pools()
            .write
            .begin_with("BEGIN IMMEDIATE")
            .await
            .db()?;
        sqlx::query(
            "INSERT INTO owned_pairing (pairing_id, target_kind, state, display_name, \
             requested_scopes_json, requested_grants_json, secret_digest, host_binding, created_at, \
             expires_at, claimed_at, approved_at, terminal_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        )
        .bind(record.id().as_str())
        .bind(record.target().as_str())
        .bind(record.state().as_str())
        .bind(record.display_name())
        .bind(encode_strings(record.requested_scopes().iter().map(str::to_owned)))
        .bind(encode_strings(record.requested_grants().iter().map(str::to_owned)))
        .bind(record.secret_digest().as_str())
        // §7.3：设备写 canonical origin，节点写本机在该配对中的 endpoint；认领时必须被逐字回显。
        .bind(record.host_binding())
        .bind(record.created_at().as_str())
        .bind(record.expires_at().as_str())
        .bind(record.claimed_at().map(Timestamp::as_str))
        .bind(record.approved_at().map(Timestamp::as_str))
        .bind(record.terminal_at().map(Timestamp::as_str))
        .execute(&mut *tx)
        .await
        .db()
        .map_err(|error| error.into_conflict(ConflictKind::AlreadyExists))?;
        insert_audit_rows(&mut tx, &write.context.at, &write.context.audit).await?;
        enforce_capacity_gate(self, &mut tx, &write.context.at).await?;
        tx.commit().await.db()?;
        Ok(())
    }

    /// §11.6 第 3 条：并发只有一个成功；过期/已终态 → `Conflict(Expired/Consumed)`；插入唯一 peer
    /// 行、推进到 `pending_confirmation` 与 `pairing.claimed` 审计同一事务。
    async fn claim_pairing(
        &self,
        write: PairingClaimWrite,
    ) -> Result<PairingClaimOutcome, PortError> {
        self.writable()?;
        let claim = write.claim;
        let pairing_id = claim.pairing().clone();
        let mut tx = self
            .pools()
            .write
            .begin_with("BEGIN IMMEDIATE")
            .await
            .db()?;
        let sql = format!("SELECT {PAIRING_COLUMNS} FROM owned_pairing WHERE pairing_id = ?1");
        let row = sqlx::query(&sql)
            .bind(pairing_id.as_str())
            .fetch_optional(&mut *tx)
            .await
            .db()?;
        let Some(row) = row else {
            return Err(PortError::NotFound(EntityRef::Pairing(pairing_id)));
        };
        let record = pairing_from_row(&row)?;
        // 配对目标与对端身份必须同类：设备配对不能被节点对端认领。
        let peer = claim.peer();
        if record.target().as_str() != peer.id().kind() {
            return Err(PortError::Conflict(ConflictKind::IdentityMismatch));
        }
        if record.state().is_terminal() {
            return Err(terminal_conflict(record.state()));
        }
        if record.state() != PairingState::Created {
            return Err(PortError::Conflict(ConflictKind::AlreadyClaimed));
        }
        // §11.2 第 1 条：认领必须核对「本机绑定一致」——对端要逐字回显登记时宣告的绑定
        // （设备 `canonicalOrigin`、节点 `endpoint`）。不一致说明 claim 指向的不是本次登记的本机，
        // 按身份不匹配拒绝，绝不推进状态。
        if peer.host_binding() != record.host_binding() {
            return Err(PortError::Conflict(ConflictKind::IdentityMismatch));
        }
        if write.context.at.as_str() >= record.expires_at().as_str() {
            return Err(PortError::Conflict(ConflictKind::Expired));
        }
        // claim 只能请求不超过登记时的集合（登记值才是本机承诺的上限）。
        if !is_subset(
            claim.requested_scopes().iter(),
            record.requested_scopes().iter(),
        ) || !is_subset(
            claim.requested_grants().iter(),
            record.requested_grants().iter(),
        ) {
            return Err(PortError::InvalidRequest(
                "claim requests more than the registered pairing",
            ));
        }
        let peer_id = peer.id().id_text().to_owned();
        let peer_kind = peer.id().kind();
        let public_key = peer.public_key().as_bytes().to_vec();
        let fingerprint = fingerprint_text(&peer.public_key().fingerprint());
        let updated = sqlx::query(
            "UPDATE owned_pairing SET state = 'pending_confirmation', claimed_at = ?2, \
             display_name = ?3 WHERE pairing_id = ?1 AND state = 'created'",
        )
        .bind(pairing_id.as_str())
        .bind(write.context.at.as_str())
        .bind(peer.display_name())
        .execute(&mut *tx)
        .await
        .db()?
        .rows_affected();
        if updated == 0 {
            // 条件更新的回读判定（§6 第 13 条的同一手法）：按当前行给出具名冲突。
            let sql = format!("SELECT {PAIRING_COLUMNS} FROM owned_pairing WHERE pairing_id = ?1");
            let current = sqlx::query(&sql)
                .bind(pairing_id.as_str())
                .fetch_optional(&mut *tx)
                .await
                .db()?;
            let state = match current.as_ref() {
                Some(row) => pairing_from_row(row)?.state(),
                None => return Err(PortError::NotFound(EntityRef::Pairing(pairing_id))),
            };
            return Err(if state.is_terminal() {
                terminal_conflict(state)
            } else {
                PortError::Conflict(ConflictKind::AlreadyClaimed)
            });
        }
        sqlx::query(
            "INSERT INTO owned_pairing_peer (pairing_id, peer_kind, peer_id, display_name, \
             public_key, fingerprint, client_nonce, claimed_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .bind(pairing_id.as_str())
        .bind(peer_kind)
        .bind(&peer_id)
        .bind(peer.display_name())
        .bind(public_key)
        .bind(fingerprint)
        .bind(peer.client_nonce().as_str())
        .bind(write.context.at.as_str())
        .execute(&mut *tx)
        .await
        .db()
        .map_err(|error| error.into_conflict(ConflictKind::AlreadyClaimed))?;
        insert_audit_rows(&mut tx, &write.context.at, &write.context.audit).await?;
        let claimed = PairingRecord::try_new(
            record.id().clone(),
            record.target(),
            PairingState::PendingConfirmation,
            Some(peer.display_name().to_owned()),
            record.requested_scopes().clone(),
            record.requested_grants().clone(),
            record.secret_digest().clone(),
            record.host_binding(),
            record.created_at().clone(),
            record.expires_at().clone(),
            Some(write.context.at.clone()),
            None,
            None,
        )?;
        enforce_capacity_gate(self, &mut tx, &write.context.at).await?;
        tx.commit().await.db()?;
        Ok(PairingClaimOutcome { pairing: claimed })
    }

    /// §11.6 第 4 条：拒绝或过期**不创建**信任；`Approved` 时创建信任行、把 peer 公钥转入
    /// `owned_peer_key`、更新配对为 `approved`、写 `pairing.approved`；任一步失败全回滚。
    async fn settle_pairing(
        &self,
        write: PairingSettlementWrite,
    ) -> Result<TrustRecordRef, PortError> {
        self.writable()?;
        let mut tx = self
            .pools()
            .write
            .begin_with("BEGIN IMMEDIATE")
            .await
            .db()?;
        let sql = format!("SELECT {PAIRING_COLUMNS} FROM owned_pairing WHERE pairing_id = ?1");
        let row = sqlx::query(&sql)
            .bind(write.pairing.as_str())
            .fetch_optional(&mut *tx)
            .await
            .db()?;
        let Some(row) = row else {
            return Err(PortError::NotFound(EntityRef::Pairing(write.pairing)));
        };
        let record = pairing_from_row(&row)?;
        if record.state().is_terminal() {
            return Err(terminal_conflict(record.state()));
        }
        if record.claimed_at().is_none() {
            // 未认领就没有对端事实可以落定（也没有可返回的信任引用）。
            return Err(PortError::InvalidRequest("pairing has not been claimed"));
        }
        // 落定只能发生一次，因此只有 `pending_confirmation` 能进入下面的分支（`claim_pairing` 的
        // `AND state = 'created'` 同款正向谓词）。`approved` 不是终态，若放过它：重复批准会改写首次
        // `approved_at` 并重写信任行；「批准后再拒绝」会撞 `owned_pairing` 的
        // `(state IN ('approved','consumed')) = (approved_at IS NOT NULL)` CHECK，把约束失败变成未具名
        // 的 `PortError::Backend`。两条路径都在任何写入之前以具名冲突拒绝。
        match record.state() {
            PairingState::PendingConfirmation => {}
            state if state.is_terminal() => return Err(terminal_conflict(state)),
            _ => return Err(PortError::Conflict(ConflictKind::Consumed)),
        }
        let peer_sql =
            format!("SELECT {PAIRING_PEER_COLUMNS} FROM owned_pairing_peer WHERE pairing_id = ?1");
        let peer_row = sqlx::query(&peer_sql)
            .bind(write.pairing.as_str())
            .fetch_optional(&mut *tx)
            .await
            .db()?;
        let Some(peer_row) = peer_row else {
            // 认领过但 peer 行缺失 = 库被外部改写。
            return Err(PortError::Corrupt("claimed pairing has no peer row"));
        };
        let peer = pairing_peer_from_row(&peer_row, record.host_binding())?;
        let peer_ref = match peer.id() {
            PeerIdentity::Device(id) => TrustRecordRef::Device(id.clone()),
            PeerIdentity::Node(id) => TrustRecordRef::Node(id.clone()),
        };
        match &write.settlement {
            PairingSettlement::Rejected { .. } => {
                // 拒绝不创建信任：只把配对推进到终态并写审计（返回值是该对端身份，调用方的
                // `device.pair.reject`/`node.pair.reject` 结果里不含它）。
                let rejected = sqlx::query(
                    "UPDATE owned_pairing SET state = 'rejected', terminal_at = ?2 \
                     WHERE pairing_id = ?1 AND state = 'pending_confirmation'",
                )
                .bind(write.pairing.as_str())
                .bind(write.context.at.as_str())
                .execute(&mut *tx)
                .await
                .db()?
                .rows_affected();
                if rejected == 0 {
                    // 同一事务内已被上面的状态守卫排除，这里只作失败关闭。
                    return Err(PortError::Conflict(ConflictKind::Consumed));
                }
                insert_audit_rows(&mut tx, &write.context.at, &write.context.audit).await?;
                tx.commit().await.db()?;
                return Ok(peer_ref);
            }
            PairingSettlement::Approved {
                granted_scopes,
                granted_grants,
            } => {
                if write.context.at.as_str() >= record.expires_at().as_str() {
                    // 过期不创建信任，也不把行推进到 approved。
                    return Err(PortError::Conflict(ConflictKind::Expired));
                }
                if !is_subset(granted_scopes.iter(), record.requested_scopes().iter())
                    || !is_subset(granted_grants.iter(), record.requested_grants().iter())
                {
                    return Err(PortError::InvalidRequest(
                        "granted sets must not exceed the requested sets",
                    ));
                }
                match record.target() {
                    PairingTarget::Device => {
                        if !granted_grants.is_empty() {
                            return Err(PortError::InvalidRequest(
                                "a device pairing must not carry grants",
                            ));
                        }
                        approve_device(&mut tx, &peer, granted_scopes, &write.context.at).await?;
                    }
                    PairingTarget::Node => {
                        approve_node(&mut tx, &peer, granted_grants, &write.context.at).await?;
                    }
                }
                let approved = sqlx::query(
                    "UPDATE owned_pairing SET state = 'approved', approved_at = ?2 \
                     WHERE pairing_id = ?1 AND state = 'pending_confirmation'",
                )
                .bind(write.pairing.as_str())
                .bind(write.context.at.as_str())
                .execute(&mut *tx)
                .await
                .db()?
                .rows_affected();
                if approved == 0 {
                    return Err(PortError::Conflict(ConflictKind::Consumed));
                }
                insert_audit_rows(&mut tx, &write.context.at, &write.context.audit).await?;
                enforce_capacity_gate(self, &mut tx, &write.context.at).await?;
                tx.commit().await.db()?;
                Ok(peer_ref)
            }
        }
    }

    /// §11.6 第 6 条：只终结「未确认且 `expires_at <= at`」的行，为每条被终结的配对补写
    /// `pairing.expired`；已批准信任不受影响。
    async fn expire_pairings(&self, write: ExpiryWrite) -> Result<u64, PortError> {
        self.writable()?;
        let mut tx = self
            .pools()
            .write
            .begin_with("BEGIN IMMEDIATE")
            .await
            .db()?;
        let expired: Vec<String> = sqlx::query_scalar(
            "SELECT pairing_id FROM owned_pairing WHERE state IN ('created','claimed',\
             'pending_confirmation') AND expires_at <= ?1 ORDER BY pairing_id ASC",
        )
        .bind(write.context.at.as_str())
        .fetch_all(&mut *tx)
        .await
        .db()?;
        let mut terminated = 0_u64;
        for pairing_id in &expired {
            // `claimed_at` 必须一并落值：`owned_pairing` 的 `(state = 'created') = (claimed_at IS NULL)`
            // CHECK（§7.3）不接受「非 created 但 claimed_at 为空」的行。对从未被认领的配对，扫描时刻
            // 就是它离开 `created` 的时刻，因此如实写扫描时刻，而不是把它留在 `created` 让后续认领
            // 凭 `expires_at` 判定（那会让「已过期」不再是一个可见终态）。
            let updated = sqlx::query(
                "UPDATE owned_pairing SET state = 'expired', terminal_at = ?2, \
                 claimed_at = COALESCE(claimed_at, ?2) \
                 WHERE pairing_id = ?1 AND state IN ('created','claimed','pending_confirmation') AND \
                 expires_at <= ?2",
            )
            .bind(pairing_id)
            .bind(write.context.at.as_str())
            .execute(&mut *tx)
            .await
            .db()?
            .rows_affected();
            if updated == 0 {
                continue;
            }
            terminated += updated;
            let id = decode::<PairingId>(pairing_id, "owned_pairing.pairing_id")?;
            insert_expiry_audit(&mut tx, &write.context.at, &write.context.audit, &id).await?;
        }
        // `pairing.expired` 由本路径**按配对**写入（`UseCases::expire_pairings` 传空的 `context.audit`，
        // 只用它作出处）；因此这里不再把 `context.audit` 整体追加一遍——那会让每条被终结的配对出
        // 现两条同动作审计。没有任何行被终结时本次写集不产生审计（没有发生安全动作）。
        tx.commit().await.db()?;
        Ok(terminated)
    }
}

// ---------------------------------------------------------------------------------------------
// 写集内部
// ---------------------------------------------------------------------------------------------

fn fingerprint_text(fingerprint: &Fingerprint) -> String {
    fingerprint.as_str().to_owned()
}

/// `RevokeReason` 的落库标记（`owned_device`/`owned_node` 的 `revoke_reason` CHECK 取值，§7.3）。
///
/// `RevokeReason` 是 core 的值对象、不带 wire 标记；标记只在持久层使用，因此映射留在这里，不给
/// core 增加一个只被适配器使用的公开方法。
fn revoke_token(reason: RevokeReason) -> &'static str {
    match reason {
        RevokeReason::UserRequested => "user_requested",
        RevokeReason::KeyChanged => "key_changed",
        RevokeReason::Compromised => "compromised",
    }
}

/// 终态配对上的操作给出具名冲突（§11.6 第 3/4 条）：`expired` → `Expired`，其余终态 → `Consumed`。
fn terminal_conflict(state: PairingState) -> PortError {
    if state == PairingState::Expired {
        PortError::Conflict(ConflictKind::Expired)
    } else {
        PortError::Conflict(ConflictKind::Consumed)
    }
}

/// `left ⊆ right`（两边都是去重集合；用于校验请求/授予集合不得超过登记值）。
fn is_subset<'a>(
    left: impl Iterator<Item = &'a str>,
    right: impl Iterator<Item = &'a str>,
) -> bool {
    let right: Vec<&str> = right.collect();
    left.into_iter().all(|item| right.contains(&item))
}

/// 配对批准（设备）：写设备行并把 peer 公钥转入 `owned_peer_key`（§11.6 第 4 条）。
///
/// 与 `put_device` 的差别只有一处：**本路径允许复活已撤销的身份**。§11.2 第 2 条的规则是「已撤销身份
/// 不能经**普通 upsert** 自动激活，只能按协议重新配对」——本路径就是那条协议路径（对端已出示配对
/// secret 的 HMAC/proof 且本机用户确认），因此这里只要求指纹与既有身份材料一致（同一 `deviceId`
/// 不得换绑公钥，§11.6 第 1 条），撤销时间与原因由 upsert 一并清空（状态回到 `active`）。撤销的
/// 审计行不因复活消失（§11.3 的 tombstone 语义）。
async fn approve_device(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    peer: &PairingPeer,
    granted_scopes: &ScopeSet,
    at: &Timestamp,
) -> Result<(), PortError> {
    let device_id = match peer.id() {
        PeerIdentity::Device(id) => id.clone(),
        PeerIdentity::Node(_) => return Err(PortError::Conflict(ConflictKind::IdentityMismatch)),
    };
    let fingerprint = peer.public_key().fingerprint();
    // 指纹必须与既有行/身份材料一致；`state` 不影响本路径（撤销由协议重新配对恢复）。
    if let Some((stored, _)) = device_identity(&mut **tx, &device_id).await? {
        if stored != fingerprint {
            return Err(PortError::Conflict(ConflictKind::IdentityMismatch));
        }
    }
    if let Some(key) = load_peer_key(&mut **tx, "device", device_id.as_str()).await? {
        if key.fingerprint() != fingerprint {
            return Err(PortError::Conflict(ConflictKind::IdentityMismatch));
        }
    }
    let record = DeviceRecord::try_new(
        device_id.clone(),
        peer.display_name(),
        fingerprint,
        granted_scopes.clone(),
        DeviceState::Active,
        at.clone(),
        None,
        None,
    )?;
    upsert_device(tx, &record, peer.public_key().as_bytes()).await?;
    bind_peer_key(tx, "device", device_id.as_str(), peer.public_key(), at).await?;
    Ok(())
}

/// 配对批准（节点）：对端角色恒为 `NodeKind::Access` —— 只有 `node.pair.begin --mode owner` 会创建
/// 节点配对行，而 `LOCAL_ADMIN_PROTOCOL.md` §5.4 明确「`--mode access` 本机没有 `confirm` 调用」
/// （claim 的 `nodeKind` 也固定为 `access`），因此不需要在写集里再携带角色。`owner_endpoint` 只在
/// `owner` 角色上存在，Access 行必须为 `None`（§3.5）。
///
/// 与 `put_node` 的差别同上：**本路径允许复活已撤销的身份**（§11.2 第 2 条把「按协议重新配对」定为
/// 唯一的恢复入口，本路径即该入口）；指纹必须与既绑定材料及既有角色行一致，撤销时间与原因由
/// upsert 清空，撤销审计保留。
async fn approve_node(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    peer: &PairingPeer,
    granted_grants: &GrantSet,
    at: &Timestamp,
) -> Result<(), PortError> {
    let node_id = match peer.id() {
        PeerIdentity::Node(id) => id.clone(),
        PeerIdentity::Device(_) => return Err(PortError::Conflict(ConflictKind::IdentityMismatch)),
    };
    let fingerprint = peer.public_key().fingerprint();
    if let Some(key) = load_peer_key(&mut **tx, "node", node_id.as_str()).await? {
        if key.fingerprint() != fingerprint {
            return Err(PortError::Conflict(ConflictKind::IdentityMismatch));
        }
    }
    for row in load_nodes_for(&mut **tx, &node_id).await? {
        if row.node_public_key_fingerprint() != &fingerprint {
            return Err(PortError::Conflict(ConflictKind::IdentityMismatch));
        }
    }
    let record = NodeRecord::try_new(
        node_id.clone(),
        peer.display_name(),
        NodeKind::Access,
        fingerprint,
        granted_grants.clone(),
        NodeState::Paired,
        None,
        at.clone(),
        None,
        None,
    )?;
    upsert_node(tx, &record).await?;
    bind_peer_key(tx, "node", node_id.as_str(), peer.public_key(), at).await?;
    Ok(())
}

/// 设备行的 upsert。`last_seen_at` 只推进不倒退也不抹掉：新值为空时保留旧值、旧值为空时写入新值、
/// 两者非空时取较大的那个（`Timestamp` 是固定宽度 UTC 毫秒文本，字典序即时间序，§3.2）。
/// 不能用标量 `max(a, b)` 代替：SQLite 的标量 `max` 在任一参数为 NULL 时返回 NULL，旧值为空时会把
/// 首次写入的新时间错成 NULL。`revoke_reason`/`revoked_at` 恒为 NULL（撤销只走 `revoke_device`，
/// `put_device` 拒绝 `revoked` 记录）。
async fn upsert_device(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    record: &DeviceRecord,
    public_key: &[u8],
) -> Result<(), StorageError> {
    sqlx::query(
        "INSERT INTO owned_device (device_id, display_name, public_key, fingerprint, scopes_json, \
         state, created_at, last_seen_at, revoked_at, revoke_reason) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, NULL) \
         ON CONFLICT(device_id) DO UPDATE SET display_name = excluded.display_name, \
         public_key = excluded.public_key, fingerprint = excluded.fingerprint, \
         scopes_json = excluded.scopes_json, state = excluded.state, \
         last_seen_at = CASE WHEN excluded.last_seen_at IS NULL THEN last_seen_at \
         WHEN last_seen_at IS NULL THEN excluded.last_seen_at \
         WHEN excluded.last_seen_at > last_seen_at THEN excluded.last_seen_at \
         ELSE last_seen_at END, \
         revoked_at = excluded.revoked_at, \
         revoke_reason = excluded.revoke_reason",
    )
    .bind(record.device_id().as_str())
    .bind(record.display_name())
    .bind(public_key.to_vec())
    .bind(record.public_key_fingerprint().as_str())
    .bind(encode_strings(record.scopes().iter().map(str::to_owned)))
    .bind(record.state().as_str())
    .bind(record.created_at().as_str())
    .bind(record.last_seen_at().map(Timestamp::as_str))
    .execute(&mut **tx)
    .await
    .map(|_| ())
    .map_err(StorageError::from)
}

/// 节点角色行的 upsert。`last_connected_at` 同 `owned_device.last_seen_at`：只推进、不倒退、不抹掉
/// （三分支 `CASE`，任一侧为空时保留非空的那一侧）。
async fn upsert_node(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    record: &NodeRecord,
) -> Result<(), StorageError> {
    sqlx::query(
        "INSERT INTO owned_node (node_id, kind, display_name, fingerprint, grants_json, state, \
         owner_endpoint, created_at, last_connected_at, revoked_at, revoke_reason) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, NULL) \
         ON CONFLICT(node_id, kind) DO UPDATE SET display_name = excluded.display_name, \
         fingerprint = excluded.fingerprint, grants_json = excluded.grants_json, \
         state = excluded.state, owner_endpoint = excluded.owner_endpoint, \
         last_connected_at = CASE WHEN excluded.last_connected_at IS NULL THEN last_connected_at \
         WHEN last_connected_at IS NULL THEN excluded.last_connected_at \
         WHEN excluded.last_connected_at > last_connected_at THEN excluded.last_connected_at \
         ELSE last_connected_at END, \
         revoked_at = excluded.revoked_at, \
         revoke_reason = excluded.revoke_reason",
    )
    .bind(record.node_id().as_str())
    .bind(record.kind().as_str())
    .bind(record.display_name())
    .bind(record.node_public_key_fingerprint().as_str())
    .bind(encode_strings(record.grants().iter().map(str::to_owned)))
    .bind(record.state().as_str())
    .bind(record.owner_endpoint())
    .bind(record.created_at().as_str())
    .bind(record.last_connected_at().map(Timestamp::as_str))
    .execute(&mut **tx)
    .await
    .map(|_| ())
    .map_err(StorageError::from)
}

/// `owned_peer_key` 的绑定（配对确认写入身份材料的唯一入口）。
async fn bind_peer_key(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    peer_kind: &str,
    peer_id: &str,
    public_key: &PeerPublicKey,
    at: &Timestamp,
) -> Result<(), StorageError> {
    sqlx::query(
        "INSERT INTO owned_peer_key (peer_kind, peer_id, public_key, fingerprint, bound_at) \
         VALUES (?1, ?2, ?3, ?4, ?5) \
         ON CONFLICT(peer_kind, peer_id) DO UPDATE SET public_key = excluded.public_key, \
         fingerprint = excluded.fingerprint, bound_at = excluded.bound_at",
    )
    .bind(peer_kind)
    .bind(peer_id)
    .bind(public_key.as_bytes().to_vec())
    .bind(fingerprint_text(&public_key.fingerprint()))
    .bind(at.as_str())
    .execute(&mut **tx)
    .await
    .map(|_| ())
    .map_err(StorageError::from)
}

/// 既有 `owned_device.public_key`（`put_device` 在身份材料行缺失时的兜底来源）。
/// 与所有身份材料读取路径同口径：同一行的 `fingerprint` 必须与公钥派生值一致，不一致即损坏，
/// 不给写路径留下「用矛盾材料继续比对」的机会。
async fn load_existing_device_key<'e, E>(
    executor: E,
    device_id: &str,
) -> Result<Option<PeerPublicKey>, StorageError>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let row = sqlx::query("SELECT public_key, fingerprint FROM owned_device WHERE device_id = ?1")
        .bind(device_id)
        .fetch_optional(executor)
        .await
        .db()?;
    let Some(row) = row else {
        return Ok(None);
    };
    let stored = decode::<Fingerprint>(&text(&row, "fingerprint")?, "owned_device.fingerprint")?;
    Ok(Some(verify_identity_material(
        peer_key_from_bytes(blob(&row, "public_key")?, "owned_device.public_key")?,
        &stored,
        "owned_device fingerprint does not match its public key",
    )?))
}
