//! 单会话释放的结果语义（core 的 `SessionRelease` 适配用的唯一事实来源）。
//!
//! 语义与 `docs/CORE_PORTS_AND_STORAGE.md` §5.10 的 `ReleaseOutcome` **同名同义**：只有 Agent 确认
//! 释放（真实 `session/close` 的 ack，或进程已经退出使资源随之消失）才是 [`ReleaseOutcome::Released`]；
//! Agent 明确拒绝或未宣告能力是 [`ReleaseOutcome::Failed`]；超时/掉线等说不清的情况是
//! [`ReleaseOutcome::Uncertain`]。后两者都**保留**真实资源占用与容量名额。
//!
//! core 的端口类型由 WP2 在 `acp_core` 落地；本 crate 是出站适配器，不能依赖尚未落地的 core 类型，
//! 因此在这里给出同形取值，由组合根做一一映射（`Released` → `Released` 等）。
//!
//! 本层的「已关闭」位**不是**释放证据：重绑与进程级回收也会把绑定标记为 closed。因此释放路径先用
//! [`ReleaseAction`] 区分「Agent 已确认释放」与「只是让出给新代」，再把动作映射成这份对 core 的事实。

use crate::error::HostError;

/// 一次逐会话释放的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseOutcome {
    /// Agent 已确认释放（或进程已经退出，资源随之消失）：可以交还名额。
    Released,
    /// 释放没有发生（Agent 明确拒绝、未宣告 `session/close`）：资源仍然占用。
    Failed,
    /// 结果说不清（超时、掉线、协议破坏）：资源可能仍占用，**不得**交还名额。
    Uncertain,
}

impl ReleaseOutcome {
    /// 是否已确认真实释放。
    #[must_use]
    pub const fn is_released(self) -> bool {
        matches!(self, Self::Released)
    }

    /// 把一次 `session/close` 的失败分类成释放结果。
    ///
    /// 只有「Agent 明确说不」「能力未宣告（因此根本没发请求）」与「这一代绑定已被取代（旧端点不得
    /// 作用于新代）」算 `Failed`；其余（超时、进程在请求期间退出、响应解不开、协议破坏）都说不清，
    /// 必须按 `Uncertain` 保留占位。
    #[must_use]
    pub(crate) const fn classify_failure(error: &HostError) -> Self {
        match error {
            HostError::AgentRejected { .. }
            | HostError::CapabilityNotDeclared { .. }
            | HostError::SessionClosed => Self::Failed,
            _ => Self::Uncertain,
        }
    }
}

/// 一次**本地**释放动作的结果（区别于 [`ReleaseOutcome`] 这份对 core 的事实）。
///
/// `session/close` 已 ack、进程已退出 ⇒ [`ReleaseAction::Released`]；绑定只是被本地让出（重绑、运行时
/// 回收）时 Agent 侧可能仍在占用，本层没有可 ack 的事实，因此是 [`ReleaseAction::Yielded`]：
///
/// - `SessionEndpoint::close` 对旧代端点把它当作**幂等成功**（不重发请求、不破坏新代）；
/// - `SessionRelease` 端口把它映射成 [`ReleaseOutcome::Failed`]（资源仍占用，不得交还名额）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReleaseAction {
    /// Agent 已确认释放（或进程已退出，资源随进程树消失）。
    Released,
    /// 这一代绑定已在本地让出，但没有任何「Agent 已释放」的证据。
    Yielded,
}

impl ReleaseAction {
    /// 对 core 的事实：只有确认释放才能交还容量名额。
    #[must_use]
    pub(crate) const fn outcome(self) -> ReleaseOutcome {
        match self {
            Self::Released => ReleaseOutcome::Released,
            Self::Yielded => ReleaseOutcome::Failed,
        }
    }
}

/// 释放未完成时的返回：语义结果 + 可映射到端口错误的具名原因。
#[derive(Debug)]
pub(crate) struct ReleaseRefusal {
    /// 对 core 的事实：资源是否仍占用。
    pub outcome: ReleaseOutcome,
    /// 具体原因（`Endpoint::close` 用它映射 `PortError`）。
    pub error: HostError,
}

impl ReleaseRefusal {
    /// 按错误分类构造。
    pub(crate) fn from_error(error: HostError) -> Self {
        let outcome = ReleaseOutcome::classify_failure(&error);
        Self { outcome, error }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_failures_are_classified_into_three_release_outcomes() {
        let failed = [
            HostError::CapabilityNotDeclared {
                capability: "session/close".to_owned(),
            },
            HostError::AgentRejected {
                code: -32000,
                message: "不能关闭".to_owned(),
            },
            HostError::SessionClosed,
        ];
        for error in failed {
            assert_eq!(
                ReleaseOutcome::classify_failure(&error),
                ReleaseOutcome::Failed,
                "明确失败必须保留占用：{error}"
            );
        }

        let uncertain = [
            HostError::Timeout {
                method: "session/close".to_owned(),
            },
            HostError::AgentExited {
                status: "exit code 1".to_owned(),
            },
            HostError::NotRunning,
            HostError::ShutdownPending,
            HostError::MalformedResponse {
                method: "session/close",
                detail: "响应解不开".to_owned(),
            },
        ];
        for error in uncertain {
            assert_eq!(
                ReleaseOutcome::classify_failure(&error),
                ReleaseOutcome::Uncertain,
                "说不清必须保留占用：{error}"
            );
        }

        assert!(ReleaseOutcome::Released.is_released());
        assert!(!ReleaseOutcome::Failed.is_released());
        assert!(!ReleaseOutcome::Uncertain.is_released());
    }

    /// 本地让出的绑定**不能**被当成释放：只有 Agent 确认（或进程已退出）才交还名额。
    #[test]
    fn a_locally_yielded_binding_is_not_released() {
        assert_eq!(
            ReleaseAction::Released.outcome(),
            ReleaseOutcome::Released,
            "确认释放才交还名额"
        );
        assert_eq!(
            ReleaseAction::Yielded.outcome(),
            ReleaseOutcome::Failed,
            "只是让出给新代的绑定不构成释放证据"
        );
    }
}
