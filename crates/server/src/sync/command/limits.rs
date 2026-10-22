//! 命令级限流（§14「单设备业务命令速率 120/分钟，按 `deviceId` 与命令族分别统计」；任务 2.5）。
//!
//! 三条口径：
//!
//! - **键是 `(deviceId, 命令族)`**，不是连接、也不是网络来源：同一设备重连不会重置额度，而网络身份
//!   不能替代设备授权（§14 末条）。同一设备的多标签页共享一份额度（`design.md` D4 的取舍：误伤的代价是
//!   一条带 `retryAfterMs` 的可重试错误，而不是静默失败）。
//! - **命令族是服务端自持的分组**（`CommandFamily`：`observe`/`interact`/`configure-session`/`approve`/
//!   `create-session`），因此高频查询（`observe`）与 prompt（`interact`）**各占各的额度**——这正是分族
//!   统计的目的。分组不由任何机器资产下发（`commands.json` 已不再有 `packs`），它只由本文件的
//!   [`CommandFamily::of`] 一份定义持有；**命令名这一词表仍以 `compatibility/commands/v1/commands.json`
//!   为唯一来源**，用例逐条断言「registry 里每个 sync 命令名恰好被一个族覆盖」，新增命令名而不归类即红。
//! - **单次超限只回可重试错误**（`resource.rate_limited` + `details.retryAfterMs`）；**连续**超限达
//!   [`MAX_CONSECUTIVE_RATE_LIMIT_REJECTIONS`] 才以 `4429` 关闭连接（与
//!   `server::node_link::command` 同形态、数值各自持有）。
//!
//! 机制复用 [`crate::transport::net::RateLimit`]（判定结果的三态形状）与
//! `transport::net::ratelimit::MAX_TRACKED_KEYS` 的有界性纪律，但**不能**复用
//! [`crate::transport::net::SlidingWindowLimiter`]：它的键被定死为 `IpAddr`，而本处的键必须是设备标识
//! （用 IP 当键正是 §14 禁止的那种替代）。因此这里自带一张同算法的键表：单键只保留窗口内最多 `limit`
//! 条记录、键总数以 `MAX_TRACKED_KEYS` 为界（先淘汰空窗口、仍满则拒绝新键——失败关闭，不重置既有计数）。

use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use sync_protocol::command::CommandName;

use crate::transport::net::ratelimit::MAX_TRACKED_KEYS;

/// 单设备单命令族的业务命令速率（§14：120 次/分钟，固定 v1 常量、不可配置）。
pub const COMMANDS_PER_MINUTE: u32 = 120;

/// 命令限流的窗口（§14 的固定 1 分钟）。
pub const COMMAND_WINDOW: Duration = Duration::from_secs(60);

/// 连续被限流多少次即按 §14 以 `4429` 关闭连接：连续超限说明对端没有遵守 `retryAfterMs`。
pub const MAX_CONSECUTIVE_RATE_LIMIT_REJECTIONS: u32 = 3;

/// 单连接去重表的条数上限（连接期状态的界：查询类去重是许可而非义务，取一个够用的上界即可）。
pub const MAX_DEDUP_ENTRIES: usize = 64;

/// 命令族（服务端自持的分组，见模块文档）。
///
/// 分族的意义是「一个族的高频命令不会挤掉另一个族的额度」：`observe` 是高频只读，`interact` 承载
/// prompt/cancel，二者共用一个额度会让一台在时间线上疯狂翻页的客户端把 prompt 饿死（`design.md` D4）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CommandFamily {
    /// 十条查询命令（`workspace.browse` 也在其中：它同样是只读、高频的浏览）。
    Observe,
    /// `session.prompt`、`session.cancel`、`elicitation.respond`。
    Interact,
    /// `session.mode.set`、`session.config.set`、`session.title.set`。
    ConfigureSession,
    /// `permission.resolve`。
    Approve,
    /// `session.create`。
    CreateSession,
    /// 主机文件系统的一层目录创建。
    CreateDirectory,
}

impl CommandFamily {
    /// 全部族（`of` 的取值域；每个族都必须至少覆盖一条命令，否则用例会红）。
    pub const ALL: [CommandFamily; 6] = [
        CommandFamily::Observe,
        CommandFamily::Interact,
        CommandFamily::ConfigureSession,
        CommandFamily::Approve,
        CommandFamily::CreateSession,
        CommandFamily::CreateDirectory,
    ];

    /// 该命令所属的族（`CommandName` 是闭合词表，因此每条命令**恰好**落进一个族：`match` 的穷尽性
    /// 由编译器保证，「恰好一个」由值唯一性保证）。
    pub fn of(command: CommandName) -> Self {
        match command {
            CommandName::SessionList
            | CommandName::SessionRead
            | CommandName::CommandStatus
            | CommandName::SessionModeList
            | CommandName::SessionConfigList
            | CommandName::WorkspaceBrowse
            | CommandName::SessionFilesList
            | CommandName::SessionFilesRead
            | CommandName::SessionGitStatus
            | CommandName::SessionGitDiff => CommandFamily::Observe,
            CommandName::SessionPrompt
            | CommandName::SessionCancel
            | CommandName::ElicitationRespond => CommandFamily::Interact,
            // `session.title.set` 与两条会话配置 mutation 同族：它们都改会话元数据、都不是高频查询，
            // 因此共用一份额度，不另立第七个族。
            CommandName::SessionModeSet
            | CommandName::SessionConfigSet
            | CommandName::SessionTitleSet => CommandFamily::ConfigureSession,
            CommandName::PermissionResolve => CommandFamily::Approve,
            CommandName::SessionCreate | CommandName::SessionResume | CommandName::SessionStart => {
                CommandFamily::CreateSession
            }
            CommandName::WorkspaceMkdir => CommandFamily::CreateDirectory,
        }
    }

    /// 诊断用的稳定取值（与族名逐字相同，也是限流键的一部分）。
    pub fn as_str(self) -> &'static str {
        match self {
            CommandFamily::Observe => "observe",
            CommandFamily::Interact => "interact",
            CommandFamily::ConfigureSession => "configure-session",
            CommandFamily::Approve => "approve",
            CommandFamily::CreateSession => "create-session",
            CommandFamily::CreateDirectory => "create-directory",
        }
    }
}

/// 一次限流判定的结果（与 [`crate::transport::net::RateLimit`] 同形，但键是设备与命令族）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateOutcome {
    /// 允许本次命令。
    Allowed,
    /// 拒绝本次命令；`retry_after` 是窗口内最早一条记录过期所需的时间（`details.retryAfterMs`）。
    Denied {
        /// 建议退避时长。
        retry_after: Duration,
    },
}

/// 按 `(deviceId, 命令族)` 计数的滑动窗口限流器。
#[derive(Debug)]
pub struct CommandRateLimiter {
    limit: u32,
    window: Duration,
    state: Mutex<HashMap<(String, CommandFamily), VecDeque<Instant>>>,
}

impl CommandRateLimiter {
    /// 建立限流器：每个键 `limit` 次/`window`。`limit = 0` 表示全部拒绝（配置错误时失败关闭的兜底）。
    pub fn new(limit: u32, window: Duration) -> Self {
        Self {
            limit,
            window,
            state: Mutex::new(HashMap::new()),
        }
    }

    /// 记录一次尝试并判定是否允许。
    pub fn check(&self, device: &str, family: CommandFamily) -> RateOutcome {
        self.check_at(device, family, Instant::now())
    }

    /// 与 [`Self::check`] 相同，但由调用方给出时刻（单测用，避免依赖真实时钟）。
    pub fn check_at(&self, device: &str, family: CommandFamily, now: Instant) -> RateOutcome {
        let mut state = lock(&self.state);
        if self.limit == 0 {
            return RateOutcome::Denied {
                retry_after: self.window,
            };
        }
        let key = (device.to_owned(), family);
        if !state.contains_key(&key) && state.len() >= MAX_TRACKED_KEYS {
            evict_idle(&mut state, self.window, now);
            if state.len() >= MAX_TRACKED_KEYS {
                // 表满且没有可淘汰的空窗口：拒绝新键（失败关闭），不放弃对既有键的计数。
                return RateOutcome::Denied {
                    retry_after: self.window,
                };
            }
        }
        let attempts = state.entry(key).or_default();
        while let Some(front) = attempts.front() {
            if now.duration_since(*front) < self.window {
                break;
            }
            attempts.pop_front();
        }
        let recorded = u32::try_from(attempts.len()).unwrap_or(u32::MAX);
        if recorded >= self.limit {
            let retry_after = attempts
                .front()
                .map(|front| self.window.saturating_sub(now.duration_since(*front)))
                .unwrap_or(self.window);
            return RateOutcome::Denied { retry_after };
        }
        attempts.push_back(now);
        RateOutcome::Allowed
    }
}

/// 淘汰窗口内已无记录的键。
fn evict_idle(
    state: &mut HashMap<(String, CommandFamily), VecDeque<Instant>>,
    window: Duration,
    now: Instant,
) {
    state.retain(|_, attempts| {
        attempts
            .back()
            .is_some_and(|last| now.duration_since(*last) < window)
    });
}

/// 锁中毒时取回内层数据而不是 panic。
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    /// registry（`CommandName::ALL`，与 `commands.json` 的 sync 子集逐条相等）里的每条命令都必须落进
    /// 一个**已登记**的族，且每个族至少覆盖一条命令。
    ///
    /// 穷尽性由 `CommandFamily::of` 的 `match` 保证（新增命令名而不归类即编译失败）；本用例补的是
    /// 「族本身都还在用」——否则 `ALL` 会慢慢长出一批永不命中的族，限流键空间与文档就会偏离。
    #[test]
    fn every_registry_command_is_covered_by_exactly_one_family() {
        let mut covered = BTreeSet::new();
        for command in CommandName::ALL {
            let family = CommandFamily::of(command);
            assert!(
                CommandFamily::ALL.contains(&family),
                "{command:?} maps to an unregistered family {family:?}"
            );
            covered.insert(family);
        }
        assert_eq!(
            covered.len(),
            CommandFamily::ALL.len(),
            "every family must cover at least one registry command"
        );
    }

    /// `session.title.set` 与两条会话配置 mutation 同族（§14：按命令族分账，不新增族）。
    #[test]
    fn title_set_shares_the_configure_session_family() {
        assert_eq!(
            CommandFamily::of(CommandName::SessionTitleSet),
            CommandFamily::ConfigureSession
        );
    }
}
