//! 配对编排的注入口（`docs/LOCAL_ADMIN_PROTOCOL.md` §5.3/§5.4、`design.md` 决策 6）。
//!
//! [`PairingSessions`] 把配对方法需要的四件东西拼在一起：组合根注入的 `identity-auth` 配对状态机、
//! 本机 canonical origin、节点身份公钥（经状态机转发的 keystore 端口）与撤销后关闭连接的钩子。
//!
//! 边界纪律：
//!
//! - **内存态只有状态机一处**：`Authority` 持有 pairing secret、失败计数与挑战缓存；对端事实
//!   （公钥、展示名、claim 回显的绑定）的唯一权威是 core 的 `owned_pairing_peer`（经
//!   `UseCases::pairing_peer` 读取）。本类型**不**另建内存注册表——两份事实来源一旦分歧，就会出现
//!   「内存已批准、库里无信任」这类不可判定的状态；
//! - **server 不读配置**：canonical origin 由组合根在构造时注入（`daemon.public_origin`，
//!   `CONFIG_REFERENCE.md` §1）；未配置时配对方法在**任何副作用之前**回 `local.unavailable`，
//!   绝不回落到 localhost、也不伪造 URL；
//! - **无出站调用**：本类型不做任何网络 IO（`node.pair.begin` 的 `mode = "access"` 因此回
//!   `local.unsupported`，不构造 HTTP 调用）；
//! - **`pairingUrl` 只在方法返回值里出现**：它含 pairing secret 的 fragment，绝不进日志、错误消息或
//!   `Debug`（`SECURITY_DESIGN.md` §14.1）。本文件的所有 `Debug` 输出都不含 URL 与 secret。

use std::sync::Arc;

use acp_core::model::{
    DeviceId, NodeId, PairingId, PairingPeer, PairingRecord, PairingState, PeerPublicKey, Timestamp,
};
use base64::Engine as _;
use identity_auth::{
    Authority, CanonicalOrigin, ClaimedPairing, IdentityError, KeystoreError, NodeEndpoint,
    PairingError, PairingSecret, RequestedCapabilities, Sas,
};

use crate::local_admin::error::{AdminError, LocalErrorCode};

/// 配对有效期上限（毫秒）：与 `identity_auth::PAIRING_MAX_SECONDS` 是同一个 5 分钟窗口
/// （`SYNC_PROTOCOL.md` §7、§14；`LOCAL_ADMIN_PROTOCOL.md` §5.3）。`expiresInMs` 只能收窄。
pub const PAIRING_WINDOW_MS: u64 = 300_000;

/// 撤销提交后关闭该设备/节点的 active connection（`§5.3`/`§5.4`、`SECURITY_DESIGN.md` §9.5）。
///
/// 组合根是连接表的唯一持有者，因此 server 只表达「这个身份已被撤销」这一事实，由实现决定关闭哪些
/// 连接、以及如何停止本地重连。两个方法都返回 `()`：撤销已经在 core 的事务里提交，关闭失败只能记日志，
/// 不能让方法失败或回滚（与 §11.6 第 5 条「先提交再通知，发送失败不撤销数据库决定」同款口径）。
#[async_trait::async_trait]
pub trait ConnectionCloser: Send + Sync {
    /// 关闭该设备的全部 active connection（没有连接时是 no-op）。
    async fn close_device(&self, device: &DeviceId);

    /// 关闭该节点的全部 active connection，并停止本地对该节点的重连（没有连接时是 no-op）。
    async fn close_node(&self, node: &NodeId);
}

/// 没有连接表时的 no-op 实现（本切片的网络 listener 与 Node Link 重连都尚未落地）。
pub struct NoConnections;

#[async_trait::async_trait]
impl ConnectionCloser for NoConnections {
    async fn close_device(&self, _device: &DeviceId) {}

    async fn close_node(&self, _node: &NodeId) {}
}

/// 配对方法与 `identity-auth` 状态机之间的注入口。
pub struct PairingSessions {
    authority: Arc<Authority>,
    public_origin: Option<String>,
    closer: Arc<dyn ConnectionCloser>,
}

impl PairingSessions {
    /// 装配。`public_origin` 来自 `daemon.public_origin`（组合根注入，可为 `None`）；`closer` 由组合根
    /// 实现（没有网络连接时传 [`NoConnections`]）。
    pub fn new(
        authority: Arc<Authority>,
        public_origin: Option<String>,
        closer: Arc<dyn ConnectionCloser>,
    ) -> Self {
        Self {
            authority,
            public_origin,
            closer,
        }
    }

    /// 配对状态机（组合根同时用它跑重启终结与过期扫描；配对方法用它取得记录草稿、落定与 SAS）。
    pub fn authority(&self) -> &Authority {
        &self.authority
    }

    /// 本机 canonical origin。
    ///
    /// 未配置 `daemon.public_origin` 时返回 `local.unavailable`：§6 的错误表是封闭的，没有「配置缺失」
    /// 专用码，而「依赖未就绪」最贴近——**重试前必须先配置 `daemon.public_origin`**。配置值不是
    /// canonical origin（例如缺 `https://`）属部署缺陷，回 `local.internal`；两种情况都不回落到
    /// localhost，也不构造不完整的 URL。
    pub fn canonical_origin(&self, operation: &str) -> Result<CanonicalOrigin, AdminError> {
        let Some(origin) = self.public_origin.as_deref() else {
            return Err(AdminError::new(
                LocalErrorCode::Unavailable,
                format!(
                    "{operation}: daemon.public_origin is not configured; configure it before retrying"
                ),
            ));
        };
        CanonicalOrigin::parse(origin).map_err(|_| {
            AdminError::new(
                LocalErrorCode::Internal,
                format!("{operation}: daemon.public_origin is not a canonical origin"),
            )
        })
    }

    /// 本机 Node Link endpoint：`wss://<public_origin 的 authority>/node-link/v1`
    /// （`NODE_LINK_PROTOCOL.md` §2.1/§13.1；host 的权威是 `public_origin`）。
    pub fn node_endpoint(&self, operation: &str) -> Result<NodeEndpoint, AdminError> {
        let origin = self.canonical_origin(operation)?;
        let authority = origin.as_str().trim_start_matches("https://");
        NodeEndpoint::parse(&format!("wss://{authority}/node-link/v1")).map_err(|_| {
            AdminError::new(
                LocalErrorCode::Internal,
                format!("{operation}: cannot derive the node endpoint from daemon.public_origin"),
            )
        })
    }

    /// 本节点长期身份公钥（二维码 payload 的 `hostPublicKey`/`ownerPublicKey`）。
    pub async fn node_public_key(&self, operation: &str) -> Result<PeerPublicKey, AdminError> {
        self.authority
            .node_public_key()
            .await
            .map_err(|error| map_identity_error(operation, error))
    }

    /// 主机侧 SAS（`SYNC_PROTOCOL.md` §7.2、`NODE_LINK_PROTOCOL.md` §9.4）：claim 之后展示给本地用户
    /// 的 6 位短验证码，由 `identity-auth` 的 transcript 派生——本层不自行计算，也不下传任何「对端结果」。
    ///
    /// 前置条件与状态机一致：记录处于 `pending_confirmation` 且内存仍持有该配对的 secret；不满足时返回
    /// 具名错误（调用方据此区分「暂时不可用」与「实现缺陷」，而不是把 `null` 当结果）。
    pub async fn host_sas(
        &self,
        record: &PairingRecord,
        peer: &PairingPeer,
    ) -> Result<Sas, PairingError> {
        if record.state() != PairingState::PendingConfirmation {
            return Err(PairingError::WrongState);
        }
        if !self.authority.has_secret(record.id()) {
            return Err(PairingError::SecretUnavailable);
        }
        self.authority
            .pairing_sas(record, &claimed_pairing(record, peer))
            .await
    }

    /// `device.pair.begin` 的二维码 URL：`https://<origin>/pair#data=<base64url-json>`
    /// （`SYNC_PROTOCOL.md` §7.1；payload 形状由 `sync-protocol` 的 `QrPayload` 拥有）。
    ///
    /// `host_public_key` 由调用方先取好：keystore 不可用时必须在**任何写入之前**失败，而不是先写
    /// 内存 secret 再发现公钥取不到。
    pub fn device_pairing_url(
        &self,
        operation: &str,
        pairing: &PairingId,
        secret: &PairingSecret,
        expires_at: &Timestamp,
        host_public_key: &PeerPublicKey,
    ) -> Result<String, AdminError> {
        let origin = self.canonical_origin(operation)?;
        let payload = sync_protocol::pairing::QrPayload {
            pairing_protocol: sync_protocol::pairing::PairingProtocolV1,
            host_id: wire_uuid(operation, self.authority.local_node().as_str())?,
            host_public_key: wire_base64url_65(operation, host_public_key.as_bytes())?,
            canonical_origin: sync_protocol::pairing::CanonicalOrigin::parse(origin.as_str())
                .map_err(|_| cannot_assemble(operation))?,
            pairing_id: wire_uuid(operation, pairing.as_str())?,
            pairing_secret: wire_base64url_32(operation, secret.as_bytes())?,
            expires_at: wire_timestamp(operation, expires_at)?,
        };
        pairing_url(operation, origin.as_str(), "/pair", &payload)
    }

    /// `node.pair.begin` 的二维码 URL：`https://<origin>/node-link/pair#data=<base64url-json>`
    /// （`NODE_LINK_PROTOCOL.md` §13.1；payload 形状由 `node-link-protocol` 的 `QrPayload` 拥有）。
    pub fn node_pairing_url(
        &self,
        operation: &str,
        pairing: &PairingId,
        secret: &PairingSecret,
        expires_at: &Timestamp,
        owner_public_key: &PeerPublicKey,
    ) -> Result<String, AdminError> {
        let origin = self.canonical_origin(operation)?;
        let payload = node_link_protocol::pairing::QrPayload {
            pairing_protocol: node_link_protocol::pairing::PairingProtocolV1,
            owner_node_id: wire_uuid(operation, self.authority.local_node().as_str())?,
            owner_public_key: wire_base64url_65(operation, owner_public_key.as_bytes())?,
            endpoint: node_link_protocol::pairing::Endpoint::parse(
                self.node_endpoint(operation)?.as_str(),
            )
            .map_err(|_| cannot_assemble(operation))?,
            pairing_id: wire_uuid(operation, pairing.as_str())?,
            pairing_secret: wire_base64url_32(operation, secret.as_bytes())?,
            expires_at: wire_timestamp(operation, expires_at)?,
        };
        pairing_url(operation, origin.as_str(), "/node-link/pair", &payload)
    }

    /// `device.revoke` 提交后关闭该设备的 active connection。
    pub async fn close_device(&self, device: &DeviceId) {
        self.closer.close_device(device).await;
    }

    /// `node.revoke` 提交后关闭该节点的 active connection（并停止本地重连）。
    pub async fn close_node(&self, node: &NodeId) {
        self.closer.close_node(node).await;
    }
}

impl std::fmt::Debug for PairingSessions {
    /// 不打印 origin 与钩子实现（前者可能含内网主机名，后者是组合根对象）。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PairingSessions")
            .field("has_public_origin", &self.public_origin.is_some())
            .finish_non_exhaustive()
    }
}

/// `device.pair.status` 的 `state`（§5.3 的闭合词表：`pending|claimed|approved|rejected|expired`）。
pub fn device_pairing_state_token(state: PairingState) -> &'static str {
    match state {
        PairingState::Created => "pending",
        // `claimed` 是服务端瞬时态（`SYNC_PROTOCOL.md` §7.0），落库形态只有 `pending_confirmation`；
        // §5.3 的词表里没有 `pending_confirmation`，因此「已认领、等待本地确认」统一呈现为 `claimed`。
        PairingState::Claimed | PairingState::PendingConfirmation => "claimed",
        // `consumed`（首次 WSS 认证成功）只发生在 `approved` 之后，设备记录在两种状态下都是 `active`；
        // §5.3 的词表没有 `consumed`，按 `approved` 呈现，不新增词表取值。
        PairingState::Approved | PairingState::Consumed => "approved",
        PairingState::Rejected => "rejected",
        PairingState::Expired => "expired",
    }
}

/// `node.pair.status` 的 `state`（§5.4 的闭合词表：`pending|claimed|pending_confirmation|approved|
/// rejected|expired`）。
pub fn node_pairing_state_token(state: PairingState) -> &'static str {
    match state {
        PairingState::Created => "pending",
        PairingState::Claimed => "claimed",
        PairingState::PendingConfirmation => "pending_confirmation",
        // 同 `device`：`consumed` 晚于 `approved`，§5.4 的词表没有它。
        PairingState::Approved | PairingState::Consumed => "approved",
        PairingState::Rejected => "rejected",
        PairingState::Expired => "expired",
    }
}

/// 配对窗口：`now + expiresInMs`，上限 [`PAIRING_WINDOW_MS`]（`None` = 用满 5 分钟）。
///
/// 返回的文本是 §1.1 的 RFC 3339 毫秒形状，可由 `Timestamp::new` 复核；超出可表示的窗口返回
/// `local.invalid_params`（调用方给的时间窗非法，不是内部错误）。
pub fn pairing_expires_at(
    now: &Timestamp,
    expires_in_ms: Option<u64>,
) -> Result<Timestamp, AdminError> {
    let window = expires_in_ms.unwrap_or(PAIRING_WINDOW_MS);
    if window == 0 || window > PAIRING_WINDOW_MS {
        return Err(AdminError::new(
            LocalErrorCode::InvalidParams,
            format!("pairing window must be 1..={PAIRING_WINDOW_MS} ms"),
        ));
    }
    let shifted = window::add_millis(now.as_str(), i64::try_from(window).unwrap_or(i64::MAX))
        .ok_or_else(|| {
            AdminError::new(
                LocalErrorCode::Internal,
                "pairing expiry is outside the supported timestamp window",
            )
        })?;
    Timestamp::new(&shifted).map_err(|_| {
        AdminError::new(
            LocalErrorCode::Internal,
            "pairing expiry is not a canonical timestamp",
        )
    })
}

/// 持久化的对端事实 → 状态机视图输入（`ClaimedPairing` 是 `identity-auth` 的视图类型，只在
/// claim 的 HTTP 路径上由 `ClaimFields` 构造；本地通道从库里读回同样的四个字段）。
pub fn claimed_pairing(record: &PairingRecord, peer: &PairingPeer) -> ClaimedPairing {
    ClaimedPairing {
        pairing: record.id().clone(),
        peer: peer.id().clone(),
        display_name: peer.display_name().to_owned(),
        public_key: peer.public_key().clone(),
        host_binding: peer.host_binding().to_owned(),
        client_nonce: peer.client_nonce().clone(),
        requested: RequestedCapabilities {
            scopes: record.requested_scopes().clone(),
            grants: record.requested_grants().clone(),
        },
    }
}

/// identity 层的失败 → `local.*`：keystore 不可用即 `local.unavailable`（正式模式不得降级），
/// 其余（身份材料缺失/损坏）收敛成 `local.internal`，且不转述内层文本。
fn map_identity_error(operation: &str, error: IdentityError) -> AdminError {
    match error {
        IdentityError::Keystore(KeystoreError::Unavailable) => AdminError::new(
            LocalErrorCode::Unavailable,
            format!("{operation}: platform keystore is not available"),
        ),
        _ => AdminError::new(
            LocalErrorCode::Internal,
            format!("{operation}: node identity is not usable"),
        ),
    }
}

/// 组装失败（二维码 payload 的字段装配）——都是实现缺陷，消息不含任何字段值。
fn cannot_assemble(operation: &str) -> AdminError {
    AdminError::new(
        LocalErrorCode::Internal,
        format!("{operation}: cannot assemble the pairing url"),
    )
}

fn wire_uuid(operation: &str, text: &str) -> Result<sync_protocol::common::Uuid, AdminError> {
    // 两个协议 crate 都从 `acpr-wire` 再导出同一个 `Uuid`/`Timestamp`，因此这一处足以服务两边。
    sync_protocol::common::Uuid::parse(text).map_err(|_| cannot_assemble(operation))
}

fn wire_timestamp(
    operation: &str,
    at: &Timestamp,
) -> Result<sync_protocol::common::Timestamp, AdminError> {
    sync_protocol::common::Timestamp::parse(at.as_str()).map_err(|_| cannot_assemble(operation))
}

fn wire_base64url_32(
    operation: &str,
    bytes: &[u8; 32],
) -> Result<sync_protocol::common::Base64Url<32>, AdminError> {
    sync_protocol::common::Base64Url::parse(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes),
    )
    .map_err(|_| cannot_assemble(operation))
}

fn wire_base64url_65(
    operation: &str,
    bytes: &[u8; PeerPublicKey::SEC1_UNCOMPRESSED_LEN],
) -> Result<sync_protocol::common::Base64Url<65>, AdminError> {
    sync_protocol::common::Base64Url::parse(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes),
    )
    .map_err(|_| cannot_assemble(operation))
}

/// `https://<origin><path>#data=<base64url(json)>`：payload 由协议 crate 的类型序列化，secret 只在
/// URL 的 fragment 里（§7.1/§13.1），不进日志——返回值由路由直接放进 `result`。
fn pairing_url<T: serde::Serialize>(
    operation: &str,
    origin: &str,
    path: &str,
    payload: &T,
) -> Result<String, AdminError> {
    let json = serde_json::to_string(payload).map_err(|_| cannot_assemble(operation))?;
    let data = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json.as_bytes());
    Ok(format!("{origin}{path}#data={data}"))
}

/// `Timestamp` 的毫秒加减。
///
/// core 的 `Timestamp` 是不透明文本（固定 `%Y-%m-%dT%H:%M:%S%.3fZ`，§3.2），不暴露任何日期算术；
/// 本 crate 没有日期库依赖，也不能读系统时间，因此这里只做**纯函数**的公历换算：解析 → 加毫秒 →
/// 按同一格式重新格式化。`storage-sqlite` 的 `migrate::window` 出于同一原因做了同款实现（保留窗口
/// 算术），两处都只覆盖 Hinnant 的 `days_from_civil`/`civil_from_days`，不涉及时区。
mod window {
    const DAY_MILLIS: i64 = 86_400_000;

    /// 解析 `YYYY-MM-DDTHH:MM:SS.mmmZ` 为 Unix 毫秒；形状非法时返回 `None`（`Timestamp` 已由 core
    /// 校验，这里防御被外部改写的库）。日历有效性（闰日、月长）**刻意不校验**，与 core 的
    /// `is_timestamp` 同口径，避免比 core 更严格。
    fn parse_millis(text: &str) -> Option<i64> {
        let bytes = text.as_bytes();
        if bytes.len() != 24
            || !matches!(bytes[4], b'-')
            || !matches!(bytes[7], b'-')
            || !matches!(bytes[10], b'T')
            || !matches!(bytes[13], b':')
            || !matches!(bytes[16], b':')
            || !matches!(bytes[19], b'.')
            || !matches!(bytes[23], b'Z')
        {
            return None;
        }
        let field = |from: usize, to: usize| -> Option<i64> {
            let slice = text.get(from..to)?;
            if !slice.bytes().all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            slice.parse::<i64>().ok()
        };
        let (year, month, day) = (field(0, 4)?, field(5, 7)?, field(8, 10)?);
        let (hour, minute, second, milli) = (
            field(11, 13)?,
            field(14, 16)?,
            field(17, 19)?,
            field(20, 23)?,
        );
        if !(1..=12).contains(&month)
            || !(1..=31).contains(&day)
            || !(0..=23).contains(&hour)
            || !(0..=59).contains(&minute)
            || !(0..=59).contains(&second)
        {
            return None;
        }
        let days = days_from_civil(year, month, day);
        days.checked_mul(DAY_MILLIS)?
            .checked_add(hour * 3_600_000 + minute * 60_000 + second * 1_000 + milli)
    }

    /// 按 `YYYY-MM-DDTHH:MM:SS.mmmZ` 格式化 Unix 毫秒。
    fn format_millis(millis: i64) -> String {
        let days = millis.div_euclid(DAY_MILLIS);
        let rest = millis.rem_euclid(DAY_MILLIS);
        let (year, month, day) = civil_from_days(days);
        let hour = rest / 3_600_000;
        let minute = rest % 3_600_000 / 60_000;
        let second = rest % 60_000 / 1_000;
        let milli = rest % 1_000;
        format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{milli:03}Z")
    }

    /// `text + millis` 毫秒；越界或形状非法返回 `None`。
    pub fn add_millis(text: &str, millis: i64) -> Option<String> {
        let base = parse_millis(text)?;
        Some(format_millis(base.checked_add(millis)?))
    }

    /// Howard Hinnant, `days_from_civil`：`1970-01-01` 为 0。
    fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
        let year = if month <= 2 { year - 1 } else { year };
        let era = if year >= 0 { year } else { year - 399 } / 400;
        let year_of_era = year - era * 400;
        let shifted_month = if month > 2 { month - 3 } else { month + 9 };
        let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
        let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
        era * 146_097 + day_of_era - 719_468
    }

    /// Howard Hinnant, `civil_from_days`。
    fn civil_from_days(days: i64) -> (i64, i64, i64) {
        let shifted = days + 719_468;
        let era = if shifted >= 0 {
            shifted
        } else {
            shifted - 146_096
        } / 146_097;
        let day_of_era = shifted - era * 146_097;
        let year_of_era =
            (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
        let year = year_of_era + era * 400;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let shifted_month = (5 * day_of_year + 2) / 153;
        let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
        let month = if shifted_month < 10 {
            shifted_month + 3
        } else {
            shifted_month - 9
        };
        let year = if month <= 2 { year + 1 } else { year };
        (year, month, day)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn millis_shift_handles_every_component_rollover() {
            assert_eq!(
                add_millis("2026-09-18T09:12:03.412Z", 1_000).expect("下移 1 秒"),
                "2026-09-18T09:12:04.412Z"
            );
            assert_eq!(
                add_millis("2026-09-18T09:12:03.412Z", 300_000).expect("5 分钟"),
                "2026-09-18T09:17:03.412Z"
            );
            assert_eq!(
                add_millis("2026-12-31T23:59:59.999Z", 1).expect("跨年 1 毫秒"),
                "2027-01-01T00:00:00.000Z"
            );
            assert_eq!(
                add_millis("2026-02-28T23:59:59.999Z", 1).expect("跨月"),
                "2026-03-01T00:00:00.000Z"
            );
            assert_eq!(
                add_millis("2024-02-28T23:59:59.999Z", 1).expect("闰日"),
                "2024-02-29T00:00:00.000Z"
            );
            // 与 `identity_auth` 的秒级换算互检（同一套公历算法，两个实现独立）。
            let at = identity_auth::timestamp_from_unix_seconds(1_800_000_000).expect("秒换算");
            assert_eq!(at.as_str(), "2027-01-15T08:00:00.000Z");
            assert_eq!(
                add_millis(at.as_str(), 300_000).expect("秒级基准上移"),
                "2027-01-15T08:05:00.000Z"
            );
            assert_eq!(add_millis("not-a-timestamp", 1), None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Mutex;

    use acp_core::ports::Clock;
    use identity_auth::{EntropyError, EntropySource, KeyHandle, KeyPurpose, SecretBytes};

    /// 与生产组合根无关的最小熵源：确定性字节序列（只为可判定，不作密码学用途）。
    struct DeterministicEntropy {
        state: Mutex<u64>,
    }

    impl EntropySource for DeterministicEntropy {
        fn fill(&self, out: &mut [u8]) -> Result<(), EntropyError> {
            let mut state = self.state.lock().expect("熵源锁");
            for byte in out.iter_mut() {
                *state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                *byte = (*state >> 33) as u8;
            }
            Ok(())
        }
    }

    /// 只实现本文件需要的方法（`public_key`）；其余一律 `unreachable!`。
    struct FixedKeyKeystore {
        key: PeerPublicKey,
    }

    #[async_trait::async_trait]
    impl identity_auth::IdentityKeystore for FixedKeyKeystore {
        async fn generate(
            &self,
            _purpose: KeyPurpose,
            _label: &str,
        ) -> Result<KeyHandle, KeystoreError> {
            unreachable!("本文件的测试不需要生成密钥")
        }

        async fn public_key(&self, _handle: &KeyHandle) -> Result<PeerPublicKey, KeystoreError> {
            Ok(self.key.clone())
        }

        async fn sign(
            &self,
            _handle: &KeyHandle,
            _transcript: &[u8],
        ) -> Result<identity_auth::P1363Signature, KeystoreError> {
            unreachable!("本文件的测试不签名")
        }

        async fn delete(&self, _handle: &KeyHandle) -> Result<(), KeystoreError> {
            unreachable!("本文件的测试不删除密钥")
        }

        async fn get_secret(
            &self,
            _purpose: identity_auth::SecretPurpose,
            _key: &str,
        ) -> Result<Option<SecretBytes>, KeystoreError> {
            unreachable!("本文件的测试不读凭据")
        }

        async fn put_secret(
            &self,
            _purpose: identity_auth::SecretPurpose,
            _key: &str,
            _value: &SecretBytes,
        ) -> Result<(), KeystoreError> {
            unreachable!("本文件的测试不写凭据")
        }

        async fn delete_secret(
            &self,
            _purpose: identity_auth::SecretPurpose,
            _key: &str,
        ) -> Result<(), KeystoreError> {
            unreachable!("本文件的测试不删凭据")
        }
    }

    struct FixedClock(Timestamp);

    impl Clock for FixedClock {
        fn now(&self) -> Timestamp {
            self.0.clone()
        }
    }

    /// 基点 G 的 SEC1 未压缩编码（曲线上的确定点，测试不需要随机源）。
    const TEST_PUBLIC_KEY_HEX: &str = concat!(
        "04",
        "6b17d1f2e12c4247f8bce6e563a440f277037d812deb33a0f4a13945d898c296",
        "4fe342e2fe1a7f9b8ee7eb4a7c0f9e162bce33576b315ececbb6406837bf51f5"
    );

    fn public_key() -> PeerPublicKey {
        let bytes: Vec<u8> = TEST_PUBLIC_KEY_HEX
            .as_bytes()
            .chunks(2)
            .map(|pair| {
                let hi = (pair[0] as char).to_digit(16).expect("hex") as u8;
                let lo = (pair[1] as char).to_digit(16).expect("hex") as u8;
                (hi << 4) | lo
            })
            .collect();
        PeerPublicKey::try_from_bytes(&bytes).expect("基点 G 是合法的 P-256 未压缩点")
    }

    fn sessions(origin: Option<&str>) -> PairingSessions {
        let authority = Arc::new(Authority::new(
            NodeId::new("bdb2ec20-f98c-4d87-b789-e540d527ef87").expect("node id"),
            KeyHandle::new("node-identity").expect("key handle"),
            Arc::new(FixedKeyKeystore { key: public_key() }),
            Arc::new(DeterministicEntropy {
                state: Mutex::new(7),
            }),
            Arc::new(FixedClock(
                Timestamp::new("2026-09-18T09:12:03.412Z").expect("timestamp"),
            )),
        ));
        PairingSessions::new(
            authority,
            origin.map(str::to_owned),
            Arc::new(NoConnections),
        )
    }

    fn pairing() -> (PairingId, PairingSecret, Timestamp) {
        (
            PairingId::new("2bc8b944-2a4f-46a7-8c31-b2c40923f60a").expect("pairing id"),
            PairingSecret::try_from_bytes(&[7u8; 32]).expect("secret"),
            Timestamp::new("2026-09-18T09:17:03.412Z").expect("timestamp"),
        )
    }

    #[tokio::test]
    async fn device_url_round_trips_through_the_sync_qr_payload() {
        let sessions = sessions(Some("https://work-pc.example.ts.net"));
        let (id, secret, expires_at) = pairing();
        let url = sessions
            .device_pairing_url(
                "device.pair.begin",
                &id,
                &secret,
                &expires_at,
                &public_key(),
            )
            .expect("可组装");
        let data = url
            .strip_prefix("https://work-pc.example.ts.net/pair#data=")
            .expect("URL 形状与 SYNC_PROTOCOL §7.1 一致");
        let json = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(data)
            .expect("fragment 是无填充 base64url");
        let payload: sync_protocol::pairing::QrPayload =
            serde_json::from_slice(&json).expect("sync 侧可反序列化");
        assert_eq!(
            payload.host_id.as_str(),
            "bdb2ec20-f98c-4d87-b789-e540d527ef87"
        );
        assert_eq!(payload.pairing_id.as_str(), id.as_str());
        assert_eq!(payload.pairing_secret.as_bytes(), secret.as_bytes());
        assert_eq!(payload.expires_at.as_str(), expires_at.as_str());
        assert_eq!(
            payload.canonical_origin.as_str(),
            "https://work-pc.example.ts.net"
        );
        assert_eq!(payload.host_public_key.as_bytes(), public_key().as_bytes());
        assert!(
            !format!("{sessions:?}").contains("work-pc"),
            "Debug 不打印 origin"
        );
    }

    #[tokio::test]
    async fn node_url_round_trips_through_the_node_link_qr_payload() {
        let sessions = sessions(Some("https://work-pc.example.ts.net"));
        let (id, secret, expires_at) = pairing();
        let url = sessions
            .node_pairing_url("node.pair.begin", &id, &secret, &expires_at, &public_key())
            .expect("可组装");
        let data = url
            .strip_prefix("https://work-pc.example.ts.net/node-link/pair#data=")
            .expect("URL 形状与 NODE_LINK_PROTOCOL §13.1 一致");
        let json = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(data)
            .expect("fragment 是无填充 base64url");
        let payload: node_link_protocol::pairing::QrPayload =
            serde_json::from_slice(&json).expect("Node Link 侧可反序列化");
        assert_eq!(
            payload.owner_node_id.as_str(),
            "bdb2ec20-f98c-4d87-b789-e540d527ef87"
        );
        assert_eq!(
            payload.endpoint.as_str(),
            "wss://work-pc.example.ts.net/node-link/v1"
        );
        assert_eq!(payload.pairing_secret.as_bytes(), secret.as_bytes());
        assert_eq!(payload.expires_at.as_str(), expires_at.as_str());
        assert_eq!(payload.owner_public_key.as_bytes(), public_key().as_bytes());
    }

    #[test]
    fn a_missing_or_malformed_origin_fails_closed() {
        let missing = sessions(None)
            .canonical_origin("device.pair.begin")
            .expect_err("未配置必须失败");
        assert_eq!(missing.code(), LocalErrorCode::Unavailable);
        assert!(missing.message().contains("daemon.public_origin"));

        let malformed = sessions(Some("work-pc.example.ts.net"))
            .canonical_origin("device.pair.begin")
            .expect_err("非法 origin 必须失败");
        assert_eq!(malformed.code(), LocalErrorCode::Internal);
        assert!(
            sessions(Some("work-pc.example.ts.net"))
                .node_endpoint("node.pair.begin")
                .is_err(),
            "非法 origin 不派生 endpoint"
        );
        assert_eq!(
            sessions(Some("https://work-pc.example.ts.net"))
                .node_endpoint("node.pair.begin")
                .expect("可派生")
                .as_str(),
            "wss://work-pc.example.ts.net/node-link/v1"
        );
    }

    #[test]
    fn the_pairing_window_is_capped_at_five_minutes() {
        let now = Timestamp::new("2026-09-18T09:12:03.412Z").expect("timestamp");
        assert_eq!(
            PAIRING_WINDOW_MS,
            identity_auth::PAIRING_MAX_SECONDS * 1_000,
            "本地通道的窗口上限必须与身份状态机同源"
        );
        assert_eq!(
            pairing_expires_at(&now, None).expect("默认窗口").as_str(),
            "2026-09-18T09:17:03.412Z"
        );
        assert_eq!(
            pairing_expires_at(&now, Some(60_000))
                .expect("收窄")
                .as_str(),
            "2026-09-18T09:13:03.412Z"
        );
        for rejected in [Some(0), Some(PAIRING_WINDOW_MS + 1)] {
            assert_eq!(
                pairing_expires_at(&now, rejected)
                    .expect_err("只能收窄")
                    .code(),
                LocalErrorCode::InvalidParams,
                "{rejected:?}"
            );
        }
    }

    #[test]
    fn state_tokens_stay_inside_the_documented_vocabularies() {
        // 枚举没有 `ALL` 常量（core 只为存储层穷举的枚举提供它），这里逐项列出全部七个取值。
        for state in [
            PairingState::Created,
            PairingState::Claimed,
            PairingState::PendingConfirmation,
            PairingState::Approved,
            PairingState::Rejected,
            PairingState::Expired,
            PairingState::Consumed,
        ] {
            assert!(
                ["pending", "claimed", "approved", "rejected", "expired",]
                    .contains(&device_pairing_state_token(state)),
                "{state}"
            );
            assert!(
                [
                    "pending",
                    "claimed",
                    "pending_confirmation",
                    "approved",
                    "rejected",
                    "expired",
                ]
                .contains(&node_pairing_state_token(state)),
                "{state}"
            );
        }
        assert_eq!(device_pairing_state_token(PairingState::Created), "pending");
        assert_eq!(
            device_pairing_state_token(PairingState::Consumed),
            "approved"
        );
        assert_eq!(
            node_pairing_state_token(PairingState::PendingConfirmation),
            "pending_confirmation"
        );
    }
}
