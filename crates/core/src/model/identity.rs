//! 身份、信任与审计（`docs/CORE_PORTS_AND_STORAGE.md` §3.5）。
//!
//! 形状来源：`LOCAL_ADMIN_PROTOCOL.md` §5.3（`DeviceRecord`）、§5.4（`NodeRecord`）、§5.5（Export/Import）、
//! `SECURITY_DESIGN.md` §10.2（scope 与 grant 的取值族）、§14.2（审计动作的闭合枚举）。
//!
//! `PairingRecord`/`PairingClaim`/`PairingSettlement` 在合同里标为 `[open]`（只有方法级的 `params`/`result`）。
//! 本文件按 §5.3/§5.4 的配对方法与 `SYNC_PROTOCOL.md` §7.0 的状态机给出**首个实现**的记录形状，并把
//! 「created/claimed/pending_confirmation/approved/rejected/expired/consumed」与各时间戳的一致性写成构造
//! 校验；形状随首个实现冻结（合同 §3.5 允许）。
//!
//! 凭据（Provider secret、Node key、pairing secret）**不在此层**：表与模型最多保存引用与指纹
//! （`SECURITY_DESIGN.md` §13.1）。

use std::collections::BTreeSet;

use super::error::InvalidValue;
use super::ids::{
    DeviceId, EntityRef, Fingerprint, NodeId, PairingId, is_endpoint, is_grant_name, is_scope_name,
    require_bounded,
};
use super::scalars::{Digest, Nonce, Timestamp};
use std::fmt;
use std::str::FromStr;

/// 生成一个「名称集合」：逐项校验、去重、按字典序迭代（scope 与 grant 各自的取值族不同）。
macro_rules! name_set {
    ($(#[$meta:meta])* $name:ident, $check:path, $invalid:expr) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Default, PartialEq, Eq)]
        pub struct $name(BTreeSet<String>);

        impl $name {
            /// 空集合。
            pub fn empty() -> Self {
                Self::default()
            }

            /// 逐项校验并去重。
            pub fn try_from_iter<I, S>(items: I) -> Result<Self, InvalidValue>
            where
                I: IntoIterator<Item = S>,
                S: AsRef<str>,
            {
                let mut set = Self::empty();
                for item in items {
                    set.insert(item.as_ref())?;
                }
                Ok(set)
            }

            /// 插入一个名称；重复插入返回 `false`，非法名称返回具名错误。
            pub fn insert(&mut self, name: &str) -> Result<bool, InvalidValue> {
                if !($check)(name) {
                    return Err($invalid);
                }
                Ok(self.0.insert(name.to_owned()))
            }

            /// 是否包含该名称。
            pub fn contains(&self, name: &str) -> bool {
                self.0.contains(name)
            }

            /// 是否为空。
            pub fn is_empty(&self) -> bool {
                self.0.is_empty()
            }

            /// 元素个数。
            pub fn len(&self) -> usize {
                self.0.len()
            }

            /// 按字典序迭代（天然去重）。
            pub fn iter(&self) -> impl Iterator<Item = &str> + '_ {
                self.0.iter().map(String::as_str)
            }

            /// 取出底层名称。
            pub fn into_strings(self) -> impl Iterator<Item = String> {
                self.0.into_iter()
            }
        }
    };
}

name_set!(
    /// 展开后的命令级 scope 集合（等于命令名，`SECURITY_DESIGN.md` §10.2）。`pack.*`/`preset.*` 必须先展开。
    ScopeSet,
    is_scope_name,
    InvalidValue::ScopeName
);

name_set!(
    /// `grant.*` 集合（Export scopes 与 Node grants）。
    GrantSet,
    is_grant_name,
    InvalidValue::GrantName
);

token_enum!(
    /// 调用者类别（`owned_command.actor_kind` 的 CHECK 取值）。
    ActorKind {
        Device => "device",
        Node => "node",
        Cli => "cli",
    }
);

/// 一次调用的身份（§3.5）。授权只在 core 判定，因此 scope/节点身份随 actor 一起传入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Actor {
    /// 已认证的 Sync 设备及其当前 scopes。
    Device { device: DeviceId, scopes: ScopeSet },
    /// Node Link 对端。`node` 是主 principal，`access_node` 是该连接的 Access 端点；两者共同构成
    /// §6 第 6 条要求的 `(ownerNodeId, accessNodeId)` 幂等键。
    Node { node: NodeId, access_node: NodeId },
    /// 本机 CLI/桌面入口（`local.*` 能力，不经远程身份）。
    LocalCli,
}

impl Actor {
    /// 调用者类别。
    pub fn kind(&self) -> ActorKind {
        match self {
            Self::Device { .. } => ActorKind::Device,
            Self::Node { .. } => ActorKind::Node,
            Self::LocalCli => ActorKind::Cli,
        }
    }

    /// 写入存储的单列 actor id（§7.3 的 `actor_id`）：
    /// device → 设备 UUID；node → `"{node}/{access_node}"`（承载复合幂等键）；cli → `"cli"`。
    pub fn id_text(&self) -> String {
        match self {
            Self::Device { device, .. } => device.as_str().to_owned(),
            Self::Node { node, access_node } => {
                format!("{}/{}", node.as_str(), access_node.as_str())
            }
            Self::LocalCli => "cli".to_owned(),
        }
    }

    /// 设备 scopes；非设备 actor 返回 `None`（Node 的授权由 grant ∩ Export 决定）。
    pub fn scopes(&self) -> Option<&ScopeSet> {
        match self {
            Self::Device { scopes, .. } => Some(scopes),
            Self::Node { .. } | Self::LocalCli => None,
        }
    }

    /// 设备标识。
    pub fn device_id(&self) -> Option<&DeviceId> {
        match self {
            Self::Device { device, .. } => Some(device),
            Self::Node { .. } | Self::LocalCli => None,
        }
    }

    /// Node Link 对端的两个节点标识。
    pub fn node_ids(&self) -> Option<(&NodeId, &NodeId)> {
        match self {
            Self::Node { node, access_node } => Some((node, access_node)),
            Self::Device { .. } | Self::LocalCli => None,
        }
    }

    /// 是否为本机入口。
    pub fn is_local_cli(&self) -> bool {
        matches!(self, Self::LocalCli)
    }
}

token_enum!(
    /// 设备记录状态（`LOCAL_ADMIN_PROTOCOL.md` §5.3）。
    DeviceState {
        Pending => "pending",
        Active => "active",
        Revoked => "revoked",
    }
);

/// 设备记录（`device.list` 与配对结果共用）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceRecord {
    device_id: DeviceId,
    display_name: String,
    public_key_fingerprint: Fingerprint,
    scopes: ScopeSet,
    state: DeviceState,
    created_at: Timestamp,
    last_seen_at: Option<Timestamp>,
    revoked_at: Option<Timestamp>,
}

impl DeviceRecord {
    /// 构造。`display_name` ≤128 字符；`revoked_at` 必须与 `state = revoked` 一致。
    #[allow(clippy::too_many_arguments)] // 与 §5.3 的记录字段一一对应
    pub fn try_new(
        device_id: DeviceId,
        display_name: &str,
        public_key_fingerprint: Fingerprint,
        scopes: ScopeSet,
        state: DeviceState,
        created_at: Timestamp,
        last_seen_at: Option<Timestamp>,
        revoked_at: Option<Timestamp>,
    ) -> Result<Self, InvalidValue> {
        require_bounded(display_name, 1, 128)?;
        if revoked_at.is_some() != (state == DeviceState::Revoked) {
            return Err(InvalidValue::Field);
        }
        Ok(Self {
            device_id,
            display_name: display_name.to_owned(),
            public_key_fingerprint,
            scopes,
            state,
            created_at,
            last_seen_at,
            revoked_at,
        })
    }

    /// 设备标识。
    pub fn device_id(&self) -> &DeviceId {
        &self.device_id
    }

    /// 展示名。
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    /// 公钥指纹（64 字符小写十六进制）。
    pub fn public_key_fingerprint(&self) -> &Fingerprint {
        &self.public_key_fingerprint
    }

    /// 展开后的 scopes。
    pub fn scopes(&self) -> &ScopeSet {
        &self.scopes
    }

    /// 状态。
    pub fn state(&self) -> DeviceState {
        self.state
    }

    /// 创建时间。
    pub fn created_at(&self) -> &Timestamp {
        &self.created_at
    }

    /// 最近一次认证成功时间。
    pub fn last_seen_at(&self) -> Option<&Timestamp> {
        self.last_seen_at.as_ref()
    }

    /// 撤销时间。
    pub fn revoked_at(&self) -> Option<&Timestamp> {
        self.revoked_at.as_ref()
    }
}

token_enum!(
    /// 节点记录代表的对端角色（`LOCAL_ADMIN_PROTOCOL.md` §5.4）。
    NodeKind {
        Access => "access",
        Owner => "owner",
    }
);

token_enum!(
    /// 节点记录状态。
    NodeState {
        Pending => "pending",
        Paired => "paired",
        Revoked => "revoked",
    }
);

/// 节点记录（`node.list` 与配对结果共用）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeRecord {
    node_id: NodeId,
    display_name: String,
    kind: NodeKind,
    node_public_key_fingerprint: Fingerprint,
    grants: GrantSet,
    state: NodeState,
    owner_endpoint: Option<String>,
    created_at: Timestamp,
    last_connected_at: Option<Timestamp>,
    revoked_at: Option<Timestamp>,
}

impl NodeRecord {
    /// 构造。`display_name` ≤128 字符；`revoked_at` 与 `state = revoked` 一致；
    /// `owner_endpoint` 仅在 `kind = owner` 时存在，且必须是 `wss://` 端点。
    #[allow(clippy::too_many_arguments)] // 与 §5.4 的记录字段一一对应
    pub fn try_new(
        node_id: NodeId,
        display_name: &str,
        kind: NodeKind,
        node_public_key_fingerprint: Fingerprint,
        grants: GrantSet,
        state: NodeState,
        owner_endpoint: Option<String>,
        created_at: Timestamp,
        last_connected_at: Option<Timestamp>,
        revoked_at: Option<Timestamp>,
    ) -> Result<Self, InvalidValue> {
        require_bounded(display_name, 1, 128)?;
        if revoked_at.is_some() != (state == NodeState::Revoked) {
            return Err(InvalidValue::Field);
        }
        match (kind, &owner_endpoint) {
            (NodeKind::Owner, Some(endpoint)) => {
                if !is_endpoint(endpoint) {
                    return Err(InvalidValue::Endpoint);
                }
            }
            (NodeKind::Owner, None) | (NodeKind::Access, Some(_)) => {
                return Err(InvalidValue::Field);
            }
            (NodeKind::Access, None) => {}
        }
        Ok(Self {
            node_id,
            display_name: display_name.to_owned(),
            kind,
            node_public_key_fingerprint,
            grants,
            state,
            owner_endpoint,
            created_at,
            last_connected_at,
            revoked_at,
        })
    }

    /// 节点标识。
    pub fn node_id(&self) -> &NodeId {
        &self.node_id
    }

    /// 对端展示名。
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    /// 该记录代表的对端角色。
    pub fn kind(&self) -> NodeKind {
        self.kind
    }

    /// 公钥指纹。
    pub fn node_public_key_fingerprint(&self) -> &Fingerprint {
        &self.node_public_key_fingerprint
    }

    /// `grant.*` 集合。
    pub fn grants(&self) -> &GrantSet {
        &self.grants
    }

    /// 状态。
    pub fn state(&self) -> NodeState {
        self.state
    }

    /// Owner 端点（仅 `kind = owner`）。
    pub fn owner_endpoint(&self) -> Option<&str> {
        self.owner_endpoint.as_deref()
    }

    /// 创建时间。
    pub fn created_at(&self) -> &Timestamp {
        &self.created_at
    }

    /// 最近一次连接时间。
    pub fn last_connected_at(&self) -> Option<&Timestamp> {
        self.last_connected_at.as_ref()
    }

    /// 撤销时间。
    pub fn revoked_at(&self) -> Option<&Timestamp> {
        self.revoked_at.as_ref()
    }
}

token_enum!(
    /// 配对的目标族（设备配对 vs 节点配对，`LOCAL_ADMIN_PROTOCOL.md` §5.3/§5.4）。
    PairingTarget {
        Device => "device",
        Node => "node",
    }
);

token_enum!(
    /// 配对状态机（`SYNC_PROTOCOL.md` §7.0 的扩展，含 Node Link 的 `pending_confirmation` 与 `consumed`）。
    PairingState {
        Created => "created",
        Claimed => "claimed",
        PendingConfirmation => "pending_confirmation",
        Approved => "approved",
        Rejected => "rejected",
        Expired => "expired",
        Consumed => "consumed",
    }
);

impl PairingState {
    /// 终态：不再接受 claim，也不再有后续状态。
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Rejected | Self::Expired | Self::Consumed)
    }

    /// 外部可见状态：`claimed` 只用于服务端事务与审计（`SYNC_PROTOCOL.md` §7.0）。
    pub const fn is_visible(self) -> bool {
        !matches!(self, Self::Claimed)
    }
}

/// 一次性配对记录。`secret_digest` 是 pairing secret 的 SHA-256；明文只存在于创建方内存
/// （`SECURITY_DESIGN.md` §13.1：凭据不进数据库）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingRecord {
    id: PairingId,
    target: PairingTarget,
    state: PairingState,
    display_name: Option<String>,
    requested_scopes: ScopeSet,
    requested_grants: GrantSet,
    secret_digest: Digest,
    created_at: Timestamp,
    expires_at: Timestamp,
    claimed_at: Option<Timestamp>,
    approved_at: Option<Timestamp>,
    terminal_at: Option<Timestamp>,
}

impl PairingRecord {
    /// 构造。`display_name` ≤128 字符；时间戳与状态必须自洽：
    /// `claimed_at` 非空 ⟺ 状态不是 `created`；`approved_at` 非空 ⟺ 状态是 `approved`/`consumed`；
    /// `terminal_at` 非空 ⟺ 状态是 `rejected`/`expired`/`consumed`；
    /// 设备配对不带 grants、节点配对不带 scopes。
    #[allow(clippy::too_many_arguments)] // 与配对状态机的时间戳一一对应
    pub fn try_new(
        id: PairingId,
        target: PairingTarget,
        state: PairingState,
        display_name: Option<String>,
        requested_scopes: ScopeSet,
        requested_grants: GrantSet,
        secret_digest: Digest,
        created_at: Timestamp,
        expires_at: Timestamp,
        claimed_at: Option<Timestamp>,
        approved_at: Option<Timestamp>,
        terminal_at: Option<Timestamp>,
    ) -> Result<Self, InvalidValue> {
        if let Some(display_name) = &display_name {
            require_bounded(display_name, 1, 128)?;
        }
        let claimed = state != PairingState::Created;
        if claimed_at.is_some() != claimed {
            return Err(InvalidValue::Field);
        }
        let approved = matches!(state, PairingState::Approved | PairingState::Consumed);
        if approved_at.is_some() != approved {
            return Err(InvalidValue::Field);
        }
        if terminal_at.is_some() != state.is_terminal() {
            return Err(InvalidValue::Field);
        }
        match target {
            PairingTarget::Device if !requested_grants.is_empty() => {
                return Err(InvalidValue::Field);
            }
            PairingTarget::Node if !requested_scopes.is_empty() => {
                return Err(InvalidValue::Field);
            }
            PairingTarget::Device | PairingTarget::Node => {}
        }
        Ok(Self {
            id,
            target,
            state,
            display_name,
            requested_scopes,
            requested_grants,
            secret_digest,
            created_at,
            expires_at,
            claimed_at,
            approved_at,
            terminal_at,
        })
    }

    /// 配对标识。
    pub fn id(&self) -> &PairingId {
        &self.id
    }

    /// 目标族。
    pub fn target(&self) -> PairingTarget {
        self.target
    }

    /// 状态。
    pub fn state(&self) -> PairingState {
        self.state
    }

    /// 对端在后面声明的展示名（claim 之前为 `None`）。
    pub fn display_name(&self) -> Option<&str> {
        self.display_name.as_deref()
    }

    /// claim 请求的 scopes（仅设备配对）。
    pub fn requested_scopes(&self) -> &ScopeSet {
        &self.requested_scopes
    }

    /// claim 请求的 grants（仅节点配对）。
    pub fn requested_grants(&self) -> &GrantSet {
        &self.requested_grants
    }

    /// pairing secret 的摘要。
    pub fn secret_digest(&self) -> &Digest {
        &self.secret_digest
    }

    /// 创建时间。
    pub fn created_at(&self) -> &Timestamp {
        &self.created_at
    }

    /// 过期时间（固定 5 分钟窗口，`SYNC_PROTOCOL.md` §7）。
    pub fn expires_at(&self) -> &Timestamp {
        &self.expires_at
    }

    /// claim 占用时间。
    pub fn claimed_at(&self) -> Option<&Timestamp> {
        self.claimed_at.as_ref()
    }

    /// 本地确认时间。
    pub fn approved_at(&self) -> Option<&Timestamp> {
        self.approved_at.as_ref()
    }

    /// 终态时间（`rejected`/`expired`/`consumed`）。
    pub fn terminal_at(&self) -> Option<&Timestamp> {
        self.terminal_at.as_ref()
    }
}

/// 配对对端的身份：设备配对用 `DeviceId`，节点配对用 `NodeId`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerIdentity {
    Device(DeviceId),
    Node(NodeId),
}

impl PeerIdentity {
    /// 类别 token（`"device"`/`"node"`）。
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Device(_) => "device",
            Self::Node(_) => "node",
        }
    }

    /// id 文本。
    pub fn id_text(&self) -> &str {
        match self {
            Self::Device(id) => id.as_str(),
            Self::Node(id) => id.as_str(),
        }
    }
}

/// claim 声明的对端信息（`SYNC_PROTOCOL.md` §7.2、`NODE_LINK_PROTOCOL.md` §13.2）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingPeer {
    id: PeerIdentity,
    display_name: String,
    public_key_fingerprint: Fingerprint,
    client_nonce: Nonce,
}

impl PairingPeer {
    /// 构造。`display_name` ≤128 字符。
    pub fn try_new(
        id: PeerIdentity,
        display_name: &str,
        public_key_fingerprint: Fingerprint,
        client_nonce: Nonce,
    ) -> Result<Self, InvalidValue> {
        require_bounded(display_name, 1, 128)?;
        Ok(Self {
            id,
            display_name: display_name.to_owned(),
            public_key_fingerprint,
            client_nonce,
        })
    }

    /// 对端身份。
    pub fn id(&self) -> &PeerIdentity {
        &self.id
    }

    /// 对端展示名。
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    /// 对端公钥指纹。
    pub fn public_key_fingerprint(&self) -> &Fingerprint {
        &self.public_key_fingerprint
    }

    /// 对端 nonce（同一 nonce 重试必须返回原 pairing request，`SYNC_PROTOCOL.md` §7.4）。
    pub fn client_nonce(&self) -> &Nonce {
        &self.client_nonce
    }
}

/// 原子 claim 的输入（`TrustStore::claim_pairing`）。HMAC/proof 由调用方验证，进入本类型时只剩已核对的事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingClaim {
    pairing: PairingId,
    peer: PairingPeer,
    requested_scopes: ScopeSet,
    requested_grants: GrantSet,
}

impl PairingClaim {
    /// 构造。设备配对不得携带 grants，节点配对不得携带 scopes。
    pub fn try_new(
        pairing: PairingId,
        peer: PairingPeer,
        requested_scopes: ScopeSet,
        requested_grants: GrantSet,
    ) -> Result<Self, InvalidValue> {
        match (
            peer.id(),
            requested_scopes.is_empty(),
            requested_grants.is_empty(),
        ) {
            (PeerIdentity::Device(_), _, false) => Err(InvalidValue::Field),
            (PeerIdentity::Node(_), false, _) => Err(InvalidValue::Field),
            (PeerIdentity::Device(_) | PeerIdentity::Node(_), _, _) => Ok(Self {
                pairing,
                peer,
                requested_scopes,
                requested_grants,
            }),
        }
    }

    /// 目标配对。
    pub fn pairing(&self) -> &PairingId {
        &self.pairing
    }

    /// 对端。
    pub fn peer(&self) -> &PairingPeer {
        &self.peer
    }

    /// 请求的 scopes（仅设备配对）。
    pub fn requested_scopes(&self) -> &ScopeSet {
        &self.requested_scopes
    }

    /// 请求的 grants（仅节点配对）。
    pub fn requested_grants(&self) -> &GrantSet {
        &self.requested_grants
    }
}

/// 本地确认的结果（`TrustStore::settle_pairing`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairingSettlement {
    /// 批准：`granted_*` 是用户确认的最终集合（不是请求值）。
    Approved {
        granted_scopes: ScopeSet,
        granted_grants: GrantSet,
    },
    /// 拒绝：可选简短原因（≤256 字符），不进审计正文。
    Rejected { reason: Option<String> },
}

impl PairingSettlement {
    /// 批准。
    pub fn approved(granted_scopes: ScopeSet, granted_grants: GrantSet) -> Self {
        Self::Approved {
            granted_scopes,
            granted_grants,
        }
    }

    /// 拒绝。
    pub fn rejected(reason: Option<&str>) -> Result<Self, InvalidValue> {
        if let Some(reason) = reason {
            require_bounded(reason, 0, 256)?;
        }
        Ok(Self::Rejected {
            reason: reason.map(str::to_owned),
        })
    }

    /// 是否批准。
    pub fn is_approved(&self) -> bool {
        matches!(self, Self::Approved { .. })
    }

    /// 校验（拒绝原因的字段是公有的，读取路径应调用一次）。
    pub fn validate(&self) -> Result<(), InvalidValue> {
        match self {
            Self::Approved { .. } => Ok(()),
            Self::Rejected { reason } => match reason {
                Some(reason) => require_bounded(reason, 0, 256),
                None => Ok(()),
            },
        }
    }
}

token_enum!(
    /// 审计动作的闭合枚举（`SECURITY_DESIGN.md` §14.2）。
    AuditAction {
        PairingCreated => "pairing.created",
        PairingClaimed => "pairing.claimed",
        PairingApproved => "pairing.approved",
        PairingRejected => "pairing.rejected",
        PairingExpired => "pairing.expired",
        DeviceAuthenticated => "device.authenticated",
        DeviceAuthFailed => "device.auth_failed",
        DeviceRevoked => "device.revoked",
        DeviceScopesChanged => "device.scopes_changed",
        NodePaired => "node.paired",
        NodeTrustRevoked => "node.trust_revoked",
        NodeIdentityChanged => "node.identity_changed",
        AuthorizationDenied => "authorization.denied",
        RateLimitTriggered => "rate_limit.triggered",
        StorageIntegrityFailed => "storage.integrity_failed",
    }
);

token_enum!(
    /// 审计结果（本合同 §3.5）。
    AuditOutcome {
        Success => "success",
        Denied => "denied",
        Failed => "failed",
    }
);

/// 审计行（§3.5）：**不含任何内容**（`SECURITY_DESIGN.md` §14.2）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditRecord {
    at: Timestamp,
    action: AuditAction,
    actor: Actor,
    via_node: Option<NodeId>,
    local_principal_ref: Option<String>,
    target: EntityRef,
    outcome: AuditOutcome,
    detail_digest: Option<Digest>,
}

impl AuditRecord {
    /// 构造。`local_principal_ref` ≤128 字符；`detail_digest` 是细节的摘要而非细节本身。
    #[allow(clippy::too_many_arguments)] // 与 §3.5/§7.3 的审计列一一对应
    pub fn try_new(
        at: Timestamp,
        action: AuditAction,
        actor: Actor,
        via_node: Option<NodeId>,
        local_principal_ref: Option<String>,
        target: EntityRef,
        outcome: AuditOutcome,
        detail_digest: Option<Digest>,
    ) -> Result<Self, InvalidValue> {
        if let Some(principal) = &local_principal_ref {
            require_bounded(principal, 1, 128)?;
        }
        Ok(Self {
            at,
            action,
            actor,
            via_node,
            local_principal_ref,
            target,
            outcome,
            detail_digest,
        })
    }

    /// 发生时间。
    pub fn at(&self) -> &Timestamp {
        &self.at
    }

    /// 动作。
    pub fn action(&self) -> AuditAction {
        self.action
    }

    /// 调用者。
    pub fn actor(&self) -> &Actor {
        &self.actor
    }

    /// 经哪个节点（Node Link 路径）。
    pub fn via_node(&self) -> Option<&NodeId> {
        self.via_node.as_ref()
    }

    /// 本地 principal 的不透明引用（不是凭据）。
    pub fn local_principal_ref(&self) -> Option<&str> {
        self.local_principal_ref.as_deref()
    }

    /// 目标实体。
    pub fn target(&self) -> &EntityRef {
        &self.target
    }

    /// 结果。
    pub fn outcome(&self) -> AuditOutcome {
        self.outcome
    }

    /// 细节摘要。
    pub fn detail_digest(&self) -> Option<&Digest> {
        self.detail_digest.as_ref()
    }
}
