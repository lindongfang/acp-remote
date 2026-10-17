//! 连接级限额：`auth.authenticated.limits` 的三项可下调值与它们的固定常量口径
//! （`docs/SYNC_PROTOCOL.md` §13/§14、`docs/CONFIG_REFERENCE.md` §3）。
//!
//! 两条硬规则（§14）：
//!
//! - **只下调**：配置值高于 §14 默认值时取默认值，低于 wire schema 下限时按 schema 下限取值
//!   （这种配置本身非法，组合根的配置校验负责拒绝；这里不 panic、也不把它转成对端可见的差异）；
//! - **固定常量不可配置**：单连接待发送队列（8 MiB 或 2,000 条）、认证超时 15 秒、心跳静默超时
//!   90 秒、`heartbeatIntervalMs` 30 秒都是本模块/`conn` 的常量，既不从配置读取，也不随
//!   `auth.authenticated` 之外的下发面变化（§14 明确「单连接待发送队列」是固定 v1 常量）；
//! - **认证前的固定上限**（§14 的第二张表）同样写在这里：首条消息 5 秒（[`FIRST_MESSAGE_TIMEOUT`]）、
//!   首条 `auth.client_hello` 32 KiB（[`HELLO_MAX_BYTES`]）、单节点并发 WSS 连接 256
//!   （[`MAX_CONCURRENT_CONNECTIONS`]）。它们不能等 `auth.authenticated` 下发，更不得被
//!   `sync.*` 配置放宽或收窄。
//!
//! Sync 与 Node Link 的限额**形状不同**（三项 vs 七项），因此这里不复用 `node_link::conn::limits`
//! 的类型（`AGENTS.md` §6：两个边界各自定义自己的 wire 形状）。

use std::time::Duration;

use sync_protocol::auth::Limits;

/// §14 的三项默认值（同时是「只下调」的上界）。
pub const DEFAULT_MAX_MESSAGE_BYTES: u64 = 1_048_576;
/// 单次 prompt payload 的默认上限（§14）。
pub const DEFAULT_MAX_PROMPT_BYTES: u64 = 262_144;
/// 单批 replay event 数的默认上限（§14）。
pub const DEFAULT_MAX_REPLAY_EVENTS_PER_BATCH: u64 = 500;

/// `auth.authenticated.heartbeatIntervalMs` 的默认值（§13；固定常量、不可配置）。
pub const DEFAULT_HEARTBEAT_INTERVAL_MS: u64 = 30_000;

/// 单连接待发送队列的字节上限（§14；固定常量、不可配置）。
pub const MAX_PENDING_QUEUE_BYTES: u64 = 8_388_608;

/// 单连接待发送队列的条数上限（§14；固定常量、不可配置）。
pub const MAX_PENDING_QUEUE_MESSAGES: u64 = 2_000;

/// 连接建立后**第一条消息**（必须是 `auth.client_hello`）的到达上限（§14 的「5 秒内完整到达」；
/// 固定常量、不可配置）。它与 [`HANDSHAKE_TIMEOUT`](super::HANDSHAKE_TIMEOUT) 是两条独立的时限：
/// 首条消息之后，剩下的认证仍受 15 秒整窗兜住。
pub const FIRST_MESSAGE_TIMEOUT: Duration = Duration::from_secs(5);

/// 首条 `auth.client_hello` 的消息体上限（§14 的 32 KiB；固定常量、不可配置）。
///
/// 量的是信封 `body` 字段的原始 UTF-8 字节数（不含信封本身），与 §12.2 的
/// `protocol.message_too_large` 配套：这是**应用层**的认证前上限，接入层的单条消息上限（1 MiB /
/// `sync.max_message_bytes`）在它之上，因此超限的 hello 是被本层拒绝的，不是被接入层 1009 关掉的。
pub const HELLO_MAX_BYTES: u64 = 32 * 1024;

/// 单节点并发 WSS 连接上限（§14；固定常量、不可配置）。
///
/// 计数覆盖**已建立**的连接（含尚未认证的），因此它比「单 IP 在途认证」与「设备槽位」都更靠外：
/// 前者限制节点的连接总量，后两者限制同一来源/设备的并发。
pub const MAX_CONCURRENT_CONNECTIONS: usize = 256;

/// wire schema 的下限（`schemas/sync/v1/auth.schema.json#/$defs/authenticated.body.limits`）。
const MIN_MAX_MESSAGE_BYTES: u64 = 1_024;
const MIN_MAX_PROMPT_BYTES: u64 = 1;
const MIN_REPLAY_EVENTS_PER_BATCH: u64 = 1;

/// 组合根注入的部署配置快照（`docs/CONFIG_REFERENCE.md` §3 的三个 `sync.*` 键）。
///
/// 字段名与配置键一一对应；本模块不读配置文件，也不缓存配置文件的其它内容。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncConfig {
    /// `sync.max_message_bytes`。
    pub max_message_bytes: u64,
    /// `sync.max_prompt_bytes`。
    pub max_prompt_bytes: u64,
    /// `sync.max_replay_events_per_batch`。
    pub max_replay_events_per_batch: u64,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            max_message_bytes: DEFAULT_MAX_MESSAGE_BYTES,
            max_prompt_bytes: DEFAULT_MAX_PROMPT_BYTES,
            max_replay_events_per_batch: DEFAULT_MAX_REPLAY_EVENTS_PER_BATCH,
        }
    }
}

/// 本连接的**生效**限额：`auth.authenticated.limits` 下发的值与连接内部判定共用同一份数字，
/// 避免「下发给对端一套、自己按另一套判定」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionLimits {
    pub(crate) max_message_bytes: u64,
    pub(crate) max_prompt_bytes: u64,
    pub(crate) max_replay_events_per_batch: u32,
}

impl SessionLimits {
    /// 协商（唯一入口）：每个值夹在 `[wire 下限, §14 默认值]` 内。
    ///
    /// 「只下调」因此是构造期不变量：之后的任何路径都不可能把值抬高到默认值以上。
    pub fn negotiate(config: &SyncConfig) -> Self {
        Self {
            max_message_bytes: clamp(
                config.max_message_bytes,
                MIN_MAX_MESSAGE_BYTES,
                DEFAULT_MAX_MESSAGE_BYTES,
            ),
            max_prompt_bytes: clamp(
                config.max_prompt_bytes,
                MIN_MAX_PROMPT_BYTES,
                DEFAULT_MAX_PROMPT_BYTES,
            ),
            max_replay_events_per_batch: clamp(
                config.max_replay_events_per_batch,
                MIN_REPLAY_EVENTS_PER_BATCH,
                DEFAULT_MAX_REPLAY_EVENTS_PER_BATCH,
            ) as u32,
        }
    }

    /// 心跳间隔（§13 的默认 30 秒；固定常量，不随配置变化）。
    pub fn heartbeat_interval(self) -> Duration {
        Duration::from_millis(DEFAULT_HEARTBEAT_INTERVAL_MS)
    }

    /// 单条 WebSocket 消息的生效上限（`auth.authenticated.limits.maxMessageBytes` 的同值）。
    pub fn max_message_bytes(self) -> u64 {
        self.max_message_bytes
    }

    /// 单次 prompt payload 的生效上限（`auth.authenticated.limits.maxPromptBytes` 的同值）。
    pub fn max_prompt_bytes(self) -> u64 {
        self.max_prompt_bytes
    }

    /// 单批 replay event 数的生效上限（`auth.authenticated.limits.maxReplayEventsPerBatch` 的同值）。
    pub fn max_replay_events_per_batch(self) -> u32 {
        self.max_replay_events_per_batch
    }

    /// `auth.authenticated.limits` 的 wire 形状。
    ///
    /// 返回 `Option` 而不是 panic：`negotiate` 保证每个值落在 schema 下限之上，因此 `None` 只可能来自
    /// 本模块的接线错误；调用方按内部不可用失败关闭，不把「不可能」写成 `expect`。
    pub fn to_wire(self) -> Option<Limits> {
        Some(Limits {
            max_message_bytes: sync_protocol::common::UIntAtLeast::<MIN_MAX_MESSAGE_BYTES>::new(
                self.max_message_bytes,
            )
            .ok()?,
            max_prompt_bytes: sync_protocol::common::UIntAtLeast::<MIN_MAX_PROMPT_BYTES>::new(
                self.max_prompt_bytes,
            )
            .ok()?,
            max_replay_events_per_batch: sync_protocol::common::UIntAtLeast::<
                MIN_REPLAY_EVENTS_PER_BATCH,
            >::new(u64::from(
                self.max_replay_events_per_batch,
            ))
            .ok()?,
        })
    }
}

/// `value` 夹在 `[min, ceiling]` 内（`ceiling` 是 §14 默认值：只下调的上界）。
fn clamp(value: u64, min: u64, ceiling: u64) -> u64 {
    value.max(min).min(ceiling.max(min))
}
