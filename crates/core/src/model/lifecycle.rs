//! 本机生命周期的纯类型（`docs/CORE_PORTS_AND_STORAGE.md` §3.3 的 `LocalActivity` 与
//! `ViewLease`、§6 第 23 条）。
//!
//! 三个硬约束：
//!
//! - `LocalActivity` 是**本机展示**的活跃程度，与 `SessionState` 是两个维度。取值只由
//!   [`LocalActivity::reduce`] 归约，任何调用方都不得维护第二套转换表（与 [`SessionActivity`] 的
//!   唯一归约规则同一口径）。
//! - 本模块不含时间读取、I/O 或 runtime：`ViewLease` 的到期只做纯比较，时间由调用方经 `Clock`
//!   或端口参数提供。
//! - 查看租约、查看者与资源代都只存在于运行期目录，**不落库、不进快照或事件**。

use std::fmt;
use std::str::FromStr;

use super::InvalidValue;
use super::ids::{ConnectionId, SessionId};
use super::scalars::Timestamp;

token_enum!(
    /// 本机展示的活跃程度（`core.local-session-lifecycle.v1`，`SYNC_PROTOCOL.md` §9.8）。
    ///
    /// 与 [`SessionState`](super::SessionState) 是两个维度：任务三态回答「有没有任务/交互」，本类型回答
    /// 「本机是否还有可用/可能存在的运行资源」。wire 取值与 `common.schema.json#/$defs/localActivity`
    /// 逐字相同。
    LocalActivity {
        Idle => "idle",
        Active => "active",
        Pending => "pending",
    }
);

/// 归约的全部输入：四个布尔事实分别对应 §3.3 的四个条件。
///
/// 调用方负责把 `SessionActivityFacts`（有效未解决交互、未结束任务）与运行期目录（资源代状态）投影成
/// 这四个事实，**不得**在这里之外另写归约顺序。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct LocalActivityFacts {
    /// 至少一个有效未解决交互（`pending` 或已仲裁未确认）。
    pub pending_interaction: bool,
    /// 至少一个未结束任务（`queued`/`running`/`waiting_*`，含停止未确认的执行）。
    pub unfinished_task: bool,
    /// 资源代已确认 `ready`（会话的运行资源可由当前进程直接复用）。
    pub ready_resource: bool,
    /// 从 `ready` 开始的回收尚未确认释放（关闭失败或结果不确定）。
    pub release_unconfirmed: bool,
}

impl LocalActivity {
    /// **唯一**归约规则（§3.3/§9.8）：有效交互优先 `Pending`；否则存在未结束任务、已确认 ready 的资源、
    /// 或回收未确认的资源时 `Active`；否则 `Idle`。
    ///
    /// 首次 `activating` 因而不提前变成 `Active`：那时 `ready_resource` 仍为 `false`。
    pub const fn reduce(facts: LocalActivityFacts) -> Self {
        if facts.pending_interaction {
            Self::Pending
        } else if facts.unfinished_task || facts.ready_resource || facts.release_unconfirmed {
            Self::Active
        } else {
            Self::Idle
        }
    }

    /// 本机展示的活跃程度是否仍代表一个可发送的会话。
    ///
    /// **不得**用它单独决定是否允许发送：发送依据实际 `ready` 资源、任务与排队策略（§9.8）；本方法只
    /// 供 UI/投影表达「不是空闲」。
    pub const fn is_busy(&self) -> bool {
        !matches!(self, Self::Idle)
    }
}

/// 查看租约（§3.3）：某个已认证连接当前正在查看某会话的声明与到期时刻。
///
/// 每连接最多一个当前查看者由 [`ViewRegistry`](crate::ports::ViewRegistry) 的实现保证；本类型只承载
/// 一条合法租约的形状与「是否仍有效」的纯比较。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewLease {
    /// 已认证连接的标识（运行期，不持久化）。
    pub connection: ConnectionId,
    /// 当前查看的会话。
    pub session: SessionId,
    /// 客户端单调递增的查看版本。
    pub view_version: u64,
    /// 租约到期时刻（服务端时钟）。
    pub expires_at: Timestamp,
}

impl ViewLease {
    /// 构造一条租约。调用方保证 `expires_at` 晚于本次声明时刻（HTTP 端点层的判定在
    /// [`ViewDecision`](crate::ports::ViewDecision) 里）。
    pub fn new(
        connection: ConnectionId,
        session: SessionId,
        view_version: u64,
        expires_at: Timestamp,
    ) -> Self {
        Self {
            connection,
            session,
            view_version,
            expires_at,
        }
    }

    /// 在 `now` 时刻是否仍有效（到期时刻本身不再有效：`now < expires_at`）。
    pub fn is_live_at(&self, now: &Timestamp) -> bool {
        now < &self.expires_at
    }
}
