//! `daemon.status`／`daemon.stop` 的组合根注入口（`docs/LOCAL_ADMIN_PROTOCOL.md` §5.2）。
//!
//! 这两个方法**不经** `core::use_cases`：「由组合根回答」（§5.1 的补充说明、`design.md` 决策 1/6）。
//! `server` 不反向依赖 `app`，因此需求以本模块的窄 trait 表达，由 `app::daemon` 实现并在装配时注入
//! （WP4）。`DaemonStatus` 是 §5.2 的字段集合在 Rust 侧的镜像；它的 JSON 形状由 [`DaemonStatus::to_json`]
//! 逐字段产出，`camelCase`、时间戳与二进制编码遵循 §1.1 的通用编码规则。

use acp_core::model::{AgentId, NodeId, PeerPublicKey, Timestamp};
use base64::Engine as _;
use serde_json::Value;

use crate::local_admin::envelope::JsonObject;
use crate::local_admin::error::AdminError;
use crate::local_admin::view::{object, text, timestamp};

/// `daemon.stop` 的 `graceMs` 上界（§5.2：非空时取值 `0`–`60000`）。
pub const MAX_STOP_GRACE_MS: u64 = 60_000;

/// 组合根为本地管理通道提供的 Daemon 生命周期入口。
///
/// 实现（`app::daemon`）持有运行期状态：监听地址、`publicOrigin`、各 store 的计数、Agent 目录与
/// Node Link 连接的运行时快照。trait 本身不含业务规则，也不允许实现方在此写库。
#[async_trait::async_trait]
pub trait DaemonControl: Send + Sync {
    /// 当前状态快照（§5.2 的 `daemon.status` result）。
    async fn status(&self) -> DaemonStatus;

    /// 启动正常关闭序列；`grace_ms` 是 CLI 显式给出的宽限毫秒，`None` = 用
    /// `daemon.shutdown_grace_ms`（§5.2：`null` 表示使用配置值）。
    ///
    /// 响应在关闭序列**开始时**发出（§5.2），因此本方法返回成功只表示已接受关闭请求；
    /// 失败必须返回 `Err`（`accepted` 恒为 `true`，不得用 `false` 表达失败）。
    async fn stop(&self, grace_ms: Option<u64>) -> Result<(), AdminError>;
}

/// `daemon.status` 的 `counts`（§5.2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DaemonCounts {
    /// 已登记设备数（含 `pending`/`revoked`）。
    pub devices: u64,
    /// 已登记节点角色行数。
    pub nodes: u64,
    /// Export 记录数（含已撤销）。
    pub exports: u64,
    /// Import 记录数。
    pub imports: u64,
}

/// `daemon.status` 的 `agents[]` 条目（§5.2）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonAgent {
    /// Agent selector（与 profile 的 `agentId` 同一命名空间）。
    pub agent_id: AgentId,
    /// 本切片定义为「profile 存在且 `command` 可解析」（`design.md` 决策 6）。
    pub available: bool,
}

/// `daemon.status` 的 `links[].state`（§5.2、`NODE_LINK_PROTOCOL.md` §15）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonLinkState {
    /// 已建立 Node Link 连接。
    Connected,
    /// 正在连接或重连。
    Connecting,
    /// 当前离线。
    Offline,
}

impl DaemonLinkState {
    /// wire 文本（§5.2 的 `"connected" | "connecting" | "offline"`）。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Connected => "connected",
            Self::Connecting => "connecting",
            Self::Offline => "offline",
        }
    }
}

/// `daemon.status` 的 `links[]` 条目（§5.2）：组合根持有的**运行时**状态，不是持久记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonLink {
    /// 对端节点。
    pub node_id: NodeId,
    /// 连接状态。
    pub state: DaemonLinkState,
    /// 最近一次成功连接时间。
    pub last_connected_at: Option<Timestamp>,
    /// 下次重试时间。
    pub next_retry_at: Option<Timestamp>,
}

/// `daemon.status` 的 result（§5.2 的字段集合）。
///
/// 本切片的两个字段恒为空数组，原因写在字段注释里（不是「暂时没填」而是本切片的既定形状）：
/// 网络 listener（`server::sync`／`server::node_link`）尚未落地，Node Link 重连任务也尚未落地。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonStatus {
    /// `acp-remote` 包版本。
    pub version: String,
    /// §2.1 的实例标识（16 字符小写 hex）。
    pub instance_id: String,
    /// 本节点身份。
    pub node_id: NodeId,
    /// 65 字节 SEC1 未压缩 P-256 公钥（wire 上是无填充 base64url）。
    pub node_public_key: PeerPublicKey,
    /// Daemon 启动时间。
    pub started_at: Timestamp,
    /// 已运行毫秒数。
    pub uptime_ms: u64,
    /// `daemon.data_dir`（绝对路径）。
    pub data_dir: String,
    /// 实际监听地址。本切片恒空数组（无网络 listener）。
    pub listen: Vec<String>,
    /// 对外 `publicOrigin`；未配置时为 `None`。
    pub public_origin: Option<String>,
    /// 管理记录计数。
    pub counts: DaemonCounts,
    /// 本地 Agent 目录。
    pub agents: Vec<DaemonAgent>,
    /// Node Link 连接的运行时状态。本切片恒空数组（重连任务未落地，`import.add` 因此返回
    /// `local.unavailable`，§5.2 的该字段注释）。
    pub links: Vec<DaemonLink>,
}

impl DaemonStatus {
    /// 按 §5.2 的 `result` 形状序列化（`camelCase`、时间戳为 §1.1 的 RFC 3339 毫秒文本、
    /// 公钥为无填充 base64url）。
    pub fn to_json(&self) -> JsonObject {
        object(vec![
            ("version", text(&self.version)),
            ("instanceId", text(&self.instance_id)),
            ("nodeId", text(self.node_id.as_str())),
            (
                "nodePublicKey",
                text(base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .encode(self.node_public_key.as_bytes())),
            ),
            ("startedAt", timestamp(&self.started_at)),
            ("uptimeMs", Value::from(self.uptime_ms)),
            ("dataDir", text(&self.data_dir)),
            (
                "listen",
                Value::Array(self.listen.iter().map(text).collect()),
            ),
            (
                "publicOrigin",
                match &self.public_origin {
                    Some(origin) => text(origin),
                    None => Value::Null,
                },
            ),
            ("counts", Value::Object(self.counts.to_json())),
            (
                "agents",
                Value::Array(
                    self.agents
                        .iter()
                        .map(|agent| {
                            Value::Object(object(vec![
                                ("agentId", text(agent.agent_id.as_str())),
                                ("available", Value::Bool(agent.available)),
                            ]))
                        })
                        .collect(),
                ),
            ),
            (
                "links",
                Value::Array(
                    self.links
                        .iter()
                        .map(|link| {
                            Value::Object(object(vec![
                                ("nodeId", text(link.node_id.as_str())),
                                ("state", text(link.state.as_str())),
                                (
                                    "lastConnectedAt",
                                    match &link.last_connected_at {
                                        Some(at) => timestamp(at),
                                        None => Value::Null,
                                    },
                                ),
                                (
                                    "nextRetryAt",
                                    match &link.next_retry_at {
                                        Some(at) => timestamp(at),
                                        None => Value::Null,
                                    },
                                ),
                            ]))
                        })
                        .collect(),
                ),
            ),
        ])
    }
}

impl DaemonCounts {
    fn to_json(self) -> JsonObject {
        object(vec![
            ("devices", Value::from(self.devices)),
            ("nodes", Value::from(self.nodes)),
            ("exports", Value::from(self.exports)),
            ("imports", Value::from(self.imports)),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn status() -> DaemonStatus {
        DaemonStatus {
            version: "1.2.3".to_owned(),
            instance_id: "0123456789abcdef".to_owned(),
            node_id: NodeId::new("2ae1c07c-0000-4000-8000-000000000001").expect("node id"),
            node_public_key: public_key(),
            started_at: Timestamp::new("2026-09-18T09:12:03.412Z").expect("timestamp"),
            uptime_ms: 4321,
            data_dir: "D:\\data".to_owned(),
            listen: Vec::new(),
            public_origin: None,
            counts: DaemonCounts {
                devices: 2,
                nodes: 1,
                exports: 3,
                imports: 0,
            },
            agents: vec![DaemonAgent {
                agent_id: AgentId::new("codex").expect("agent id"),
                available: true,
            }],
            links: vec![DaemonLink {
                node_id: NodeId::new("2ae1c07c-0000-4000-8000-000000000002").expect("node id"),
                state: DaemonLinkState::Offline,
                last_connected_at: None,
                next_retry_at: Some(
                    Timestamp::new("2026-09-18T09:13:03.412Z").expect("timestamp"),
                ),
            }],
        }
    }

    #[test]
    fn status_serializes_the_documented_shape() {
        let json = status().to_json();
        assert_eq!(json["version"], Value::from("1.2.3"));
        assert_eq!(json["instanceId"], Value::from("0123456789abcdef"));
        assert_eq!(json["nodeId"], Value::from("2ae1c07c-0000-4000-8000-000000000001"));
        assert_eq!(json["startedAt"], Value::from("2026-09-18T09:12:03.412Z"));
        assert_eq!(json["uptimeMs"], Value::from(4321u64));
        assert_eq!(json["listen"], Value::Array(vec![]));
        assert_eq!(json["publicOrigin"], Value::Null);
        assert_eq!(json["counts"]["devices"], Value::from(2u64));
        assert_eq!(json["counts"]["imports"], Value::from(0u64));
        assert_eq!(json["agents"][0]["agentId"], Value::from("codex"));
        assert_eq!(json["agents"][0]["available"], Value::Bool(true));
        assert_eq!(json["links"][0]["state"], Value::from("offline"));
        assert_eq!(json["links"][0]["lastConnectedAt"], Value::Null);
        assert_eq!(
            json["links"][0]["nextRetryAt"],
            Value::from("2026-09-18T09:13:03.412Z")
        );
        assert_eq!(json.len(), 13, "字段集合与 §5.2 逐项一致");
    }

    #[test]
    fn node_public_key_is_unpadded_base64url_of_the_65_bytes() {
        let json = status().to_json();
        let encoded = json["nodePublicKey"].as_str().expect("string").to_owned();
        assert!(!encoded.contains('='), "无填充：{encoded}");
        assert!(!encoded.contains('+') && !encoded.contains('/'));
        let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(&encoded)
            .expect("合法 base64url");
        assert_eq!(decoded.len(), PeerPublicKey::SEC1_UNCOMPRESSED_LEN);
        assert_eq!(decoded, status().node_public_key.as_bytes().to_vec());
    }

    #[test]
    fn link_state_tokens_match_the_document() {
        assert_eq!(DaemonLinkState::Connected.as_str(), "connected");
        assert_eq!(DaemonLinkState::Connecting.as_str(), "connecting");
        assert_eq!(DaemonLinkState::Offline.as_str(), "offline");
        assert_eq!(MAX_STOP_GRACE_MS, 60_000);
    }
}
