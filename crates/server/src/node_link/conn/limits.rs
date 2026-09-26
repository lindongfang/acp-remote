//! 连接级限额：`node.ready.limits` 的七个可下调项与它们的固定常量口径
//! （`docs/NODE_LINK_PROTOCOL.md` §2.5、`docs/CONFIG_REFERENCE.md` §3，`design.md` D9）。
//!
//! 两条硬规则：
//!
//! - **只下调**：配置值高于 §2.5 的默认值时取默认值（R44「不得上调」）；低于 wire schema 下限的值
//!   按 schema 下限取值（这种配置本身非法，组合根的配置校验负责拒绝，这里不 panic、也不把它转成
//!   对端可见的差异）；
//! - **固定常量不可配置**：握手超时 15 s、心跳静默超时 90 s、JSON 嵌套 64、单对象字段 1024、
//!   单数组元素 10000、单 IP 新认证尝试 10/分钟、单连接命令速率 120/分钟、单连接并发快照 1 都是
//!   本模块/`conn::session` 的常量，不出现在本类型里，也不随 `node.ready` 下发。

use std::time::Duration;

use node_link_protocol::common::{NodeLinkLimits, UIntAtLeast};

/// §2.5/§3 的默认值（同时是「只下调」的上界）。
pub const DEFAULT_MAX_MESSAGE_BYTES: u64 = 1_048_576;
/// catalog 快照批次的 `exports` 条数上限。
pub const DEFAULT_CATALOG_SNAPSHOT_BATCH_SIZE: u64 = 500;
/// 单个 resource 快照批次的 `items` 条数上限。
pub const DEFAULT_RESOURCE_SNAPSHOT_BATCH_SIZE: u64 = 500;
/// 单连接 in-flight command 数上限。
pub const DEFAULT_MAX_IN_FLIGHT_COMMANDS: u64 = 32;
/// 单连接待发送队列的字节上限。
pub const DEFAULT_MAX_PENDING_QUEUE_BYTES: u64 = 8_388_608;
/// 单连接待发送队列的条数上限。
pub const DEFAULT_MAX_PENDING_QUEUE_MESSAGES: u64 = 2_000;
/// heartbeat 间隔（毫秒）。
pub const DEFAULT_HEARTBEAT_INTERVAL_MS: u64 = 30_000;

/// wire schema 的下限（`schemas/node-link/v1/common.schema.json#/$defs/limits`）。
const MIN_MAX_MESSAGE_BYTES: u64 = 1_024;
const MIN_BATCH_SIZE: u64 = 1;
const MIN_PENDING_QUEUE_BYTES: u64 = 1_024;
const MIN_HEARTBEAT_INTERVAL_MS: u64 = 1_000;
/// `heartbeatIntervalMs` 的 schema 上限。
const MAX_HEARTBEAT_INTERVAL_MS: u64 = 300_000;

/// 组合根注入的部署配置快照（`docs/CONFIG_REFERENCE.md` §1/§3；本模块不读配置文件）。
///
/// 字段名与配置键一一对应（`node_link.*`）；`public_origin` 是 `daemon.public_origin`，用于给
/// **未知节点**推出本机可宣告的 Node Link endpoint（§12.2：不得用错误区分节点是否存在）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeLinkConfig {
    pub max_message_bytes: u64,
    pub catalog_snapshot_batch_size: u64,
    pub resource_snapshot_batch_size: u64,
    pub max_in_flight_commands: u64,
    pub max_pending_queue_bytes: u64,
    pub max_pending_queue_messages: u64,
    pub heartbeat_interval_ms: u64,
    /// `daemon.public_origin`（`https://<authority>`）；未配置时本机没有可宣告的 Node Link endpoint。
    pub public_origin: Option<String>,
}

impl Default for NodeLinkConfig {
    fn default() -> Self {
        Self {
            max_message_bytes: DEFAULT_MAX_MESSAGE_BYTES,
            catalog_snapshot_batch_size: DEFAULT_CATALOG_SNAPSHOT_BATCH_SIZE,
            resource_snapshot_batch_size: DEFAULT_RESOURCE_SNAPSHOT_BATCH_SIZE,
            max_in_flight_commands: DEFAULT_MAX_IN_FLIGHT_COMMANDS,
            max_pending_queue_bytes: DEFAULT_MAX_PENDING_QUEUE_BYTES,
            max_pending_queue_messages: DEFAULT_MAX_PENDING_QUEUE_MESSAGES,
            heartbeat_interval_ms: DEFAULT_HEARTBEAT_INTERVAL_MS,
            public_origin: None,
        }
    }
}

/// 本连接的**生效**限额：`node.ready.limits` 下发的值与连接内部判定（发送队列、心跳、in-flight）共用
/// 同一份数字，避免「下发给对端一套、自己按另一套判定」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionLimits {
    pub(crate) max_message_bytes: u64,
    pub(crate) catalog_snapshot_batch_size: u32,
    pub(crate) resource_snapshot_batch_size: u32,
    pub(crate) max_in_flight_commands: u32,
    pub(crate) max_pending_queue_bytes: u64,
    pub(crate) max_pending_queue_messages: usize,
    pub(crate) heartbeat_interval_ms: u64,
}

impl SessionLimits {
    /// 协商（唯一入口）：每个值夹在 `[wire 下限, §2.5 默认值]` 内。
    ///
    /// 「只下调」因此是构造期不变量：之后的任何路径都不可能把值抬高到默认值以上。
    pub fn negotiate(config: &NodeLinkConfig) -> Self {
        Self {
            max_message_bytes: clamp(
                config.max_message_bytes,
                MIN_MAX_MESSAGE_BYTES,
                DEFAULT_MAX_MESSAGE_BYTES,
            ),
            catalog_snapshot_batch_size: clamp(
                config.catalog_snapshot_batch_size,
                MIN_BATCH_SIZE,
                DEFAULT_CATALOG_SNAPSHOT_BATCH_SIZE,
            ) as u32,
            resource_snapshot_batch_size: clamp(
                config.resource_snapshot_batch_size,
                MIN_BATCH_SIZE,
                DEFAULT_RESOURCE_SNAPSHOT_BATCH_SIZE,
            ) as u32,
            max_in_flight_commands: clamp(
                config.max_in_flight_commands,
                MIN_BATCH_SIZE,
                DEFAULT_MAX_IN_FLIGHT_COMMANDS,
            ) as u32,
            max_pending_queue_bytes: clamp(
                config.max_pending_queue_bytes,
                MIN_PENDING_QUEUE_BYTES,
                DEFAULT_MAX_PENDING_QUEUE_BYTES,
            ),
            max_pending_queue_messages: clamp(
                config.max_pending_queue_messages,
                MIN_BATCH_SIZE,
                DEFAULT_MAX_PENDING_QUEUE_MESSAGES,
            ) as usize,
            heartbeat_interval_ms: clamp(
                config.heartbeat_interval_ms,
                MIN_HEARTBEAT_INTERVAL_MS,
                DEFAULT_HEARTBEAT_INTERVAL_MS.min(MAX_HEARTBEAT_INTERVAL_MS),
            ),
        }
    }

    /// 心跳间隔（`node.ready.limits.heartbeatIntervalMs` 的同值 `Duration`）。
    pub fn heartbeat_interval(self) -> Duration {
        Duration::from_millis(self.heartbeat_interval_ms)
    }

    /// 单条 WebSocket 消息的生效上限（`node.ready.limits.maxMessageBytes`）。
    pub fn max_message_bytes(self) -> u64 {
        self.max_message_bytes
    }

    /// 单连接待发送队列的字节上限。
    pub fn max_pending_queue_bytes(self) -> u64 {
        self.max_pending_queue_bytes
    }

    /// 单连接待发送队列的条数上限。
    pub fn max_pending_queue_messages(self) -> usize {
        self.max_pending_queue_messages
    }

    /// 单连接 in-flight command 数上限（WP6 的判定输入）。
    pub fn max_in_flight_commands(self) -> u32 {
        self.max_in_flight_commands
    }

    /// catalog 快照批次的 `exports` 条数上限（WP5 的切分输入）。
    pub fn catalog_snapshot_batch_size(self) -> u32 {
        self.catalog_snapshot_batch_size
    }

    /// 单个 resource 快照批次的 `items` 条数上限（WP5 的切分输入）。
    pub fn resource_snapshot_batch_size(self) -> u32 {
        self.resource_snapshot_batch_size
    }

    /// `node.ready.limits` 的 wire 形状。
    ///
    /// 返回 `Option` 而不是 panic：`negotiate` 保证每个值落在 schema 下限之上，因此 `None` 只可能来自
    /// 本模块的接线错误；调用方按内部不可用失败关闭，不把「不可能」写成 `expect`。
    pub fn to_wire(self) -> Option<NodeLinkLimits> {
        Some(NodeLinkLimits {
            max_message_bytes: UIntAtLeast::<MIN_MAX_MESSAGE_BYTES>::new(self.max_message_bytes)
                .ok()?,
            catalog_snapshot_batch_size: UIntAtLeast::<MIN_BATCH_SIZE>::new(u64::from(
                self.catalog_snapshot_batch_size,
            ))
            .ok()?,
            resource_snapshot_batch_size: UIntAtLeast::<MIN_BATCH_SIZE>::new(u64::from(
                self.resource_snapshot_batch_size,
            ))
            .ok()?,
            max_in_flight_commands: UIntAtLeast::<MIN_BATCH_SIZE>::new(u64::from(
                self.max_in_flight_commands,
            ))
            .ok()?,
            max_pending_queue_bytes: UIntAtLeast::<MIN_PENDING_QUEUE_BYTES>::new(
                self.max_pending_queue_bytes,
            )
            .ok()?,
            max_pending_queue_messages: UIntAtLeast::<MIN_BATCH_SIZE>::new(
                self.max_pending_queue_messages as u64,
            )
            .ok()?,
            heartbeat_interval_ms: node_link_protocol::common::BoundedU64::<
                MIN_HEARTBEAT_INTERVAL_MS,
                MAX_HEARTBEAT_INTERVAL_MS,
            >::new(self.heartbeat_interval_ms)
            .ok()?,
        })
    }
}

/// `value` 夹在 `[min, ceiling]` 内（`ceiling` 是 §2.5 默认值：只下调的上界）。
fn clamp(value: u64, min: u64, ceiling: u64) -> u64 {
    value.max(min).min(ceiling.max(min))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_protocol_table() {
        // §2.5/§3 的七个默认值：配置默认值直接给出默认限额（不经过任何下调）。
        let limits = SessionLimits::negotiate(&NodeLinkConfig::default());
        assert_eq!(limits.max_message_bytes, DEFAULT_MAX_MESSAGE_BYTES);
        assert_eq!(
            limits.catalog_snapshot_batch_size,
            DEFAULT_CATALOG_SNAPSHOT_BATCH_SIZE as u32
        );
        assert_eq!(
            limits.resource_snapshot_batch_size,
            DEFAULT_RESOURCE_SNAPSHOT_BATCH_SIZE as u32
        );
        assert_eq!(
            limits.max_in_flight_commands,
            DEFAULT_MAX_IN_FLIGHT_COMMANDS as u32
        );
        assert_eq!(
            limits.max_pending_queue_bytes,
            DEFAULT_MAX_PENDING_QUEUE_BYTES
        );
        assert_eq!(
            limits.max_pending_queue_messages,
            DEFAULT_MAX_PENDING_QUEUE_MESSAGES as usize
        );
        assert_eq!(
            limits.heartbeat_interval(),
            Duration::from_millis(DEFAULT_HEARTBEAT_INTERVAL_MS)
        );
        let wire = limits.to_wire().expect("默认值必须落在 schema 域内");
        assert_eq!(wire.max_message_bytes.get(), DEFAULT_MAX_MESSAGE_BYTES);
        assert_eq!(
            wire.heartbeat_interval_ms.get(),
            DEFAULT_HEARTBEAT_INTERVAL_MS
        );
    }

    #[test]
    fn configuration_can_lower_but_never_raise_a_limit() {
        // R45：`max_in_flight_commands = 8` 生效。
        let lowered = SessionLimits::negotiate(&NodeLinkConfig {
            max_in_flight_commands: 8,
            heartbeat_interval_ms: 15_000,
            max_pending_queue_messages: 10,
            ..NodeLinkConfig::default()
        });
        assert_eq!(lowered.max_in_flight_commands, 8);
        assert_eq!(lowered.heartbeat_interval(), Duration::from_millis(15_000));
        assert_eq!(lowered.max_pending_queue_messages, 10);

        // 高于默认值一律回落默认值（不得上调）。
        let raised = SessionLimits::negotiate(&NodeLinkConfig {
            max_in_flight_commands: 4_096,
            max_message_bytes: u64::MAX,
            heartbeat_interval_ms: 900_000,
            ..NodeLinkConfig::default()
        });
        assert_eq!(
            raised.max_in_flight_commands,
            DEFAULT_MAX_IN_FLIGHT_COMMANDS as u32
        );
        assert_eq!(raised.max_message_bytes, DEFAULT_MAX_MESSAGE_BYTES);
        assert_eq!(
            raised.heartbeat_interval(),
            Duration::from_millis(DEFAULT_HEARTBEAT_INTERVAL_MS)
        );
    }

    #[test]
    fn values_below_the_wire_minimum_are_clamped_to_the_schema_floor() {
        // 非法配置不 panic、也不产生对端可见的差异：取下限（`to_wire` 仍必须成功）。
        let limits = SessionLimits::negotiate(&NodeLinkConfig {
            max_message_bytes: 1,
            max_pending_queue_bytes: 0,
            max_pending_queue_messages: 0,
            heartbeat_interval_ms: 0,
            catalog_snapshot_batch_size: 0,
            ..NodeLinkConfig::default()
        });
        assert_eq!(limits.max_message_bytes, MIN_MAX_MESSAGE_BYTES);
        assert_eq!(limits.max_pending_queue_bytes, MIN_PENDING_QUEUE_BYTES);
        assert_eq!(limits.max_pending_queue_messages, 1);
        assert_eq!(limits.heartbeat_interval_ms, MIN_HEARTBEAT_INTERVAL_MS);
        assert!(limits.to_wire().is_some());
    }
}
