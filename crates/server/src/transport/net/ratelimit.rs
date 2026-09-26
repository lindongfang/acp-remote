//! 进程内滑动窗口限流器（`design.md` D9）：键 = 对端真实 IP。
//!
//! 本模块只提供**机制**：`NODE_LINK_PROTOCOL.md` §2.5 的固定限额（单 IP 新认证尝试 10/分钟、配对
//! claim 10/分钟/IP、status 60/分钟/`pairingId`）由 `server::node_link` 在接线处取用并决定超限响应
//! （429 / `rate_limited` + `retryAfterMs` / `4429`）。键的取值必须来自 [`crate::transport::net::PeerInfo`]
//! 的 `client_ip`（转发头已被可信代理边界过滤），不能用未经验证的客户端自报地址。
//!
//! 内存有界性（`AGENTS.md` §7：「异步任务/资源必须有所有者与上限」的同一原则）：
//!
//! - 每个键只保留窗口内最多 `limit` 条尝试记录，因此单键占用有上限；
//! - 键总数达到 [`MAX_TRACKED_KEYS`] 时先淘汰窗口已空的键；仍满则**拒绝新键**（失败关闭），
//!   不会因为伪造源地址的洪泛而无限增长，也不会重置既有键的计数。

use std::collections::HashMap;
use std::collections::VecDeque;
use std::net::IpAddr;
use std::sync::Mutex;
use std::sync::PoisonError;
use std::time::Duration;
use std::time::Instant;

/// 同时跟踪的键上限（超出后先淘汰空窗口，再拒绝新键）。
pub const MAX_TRACKED_KEYS: usize = 4096;

/// 一次限流判定的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimit {
    /// 允许本次尝试；`remaining` 是窗口内剩余次数。
    Allowed {
        /// 本次尝试计入后窗口内的剩余次数。
        remaining: u32,
    },
    /// 拒绝本次尝试；`retry_after` 是窗口内最早一条记录过期所需的时间。
    Denied {
        /// 建议退避时长（对应协议里的 `retryAfterMs`）。
        retry_after: Duration,
    },
}

/// 固定窗口长度的滑动窗口限流器。
#[derive(Debug)]
pub struct SlidingWindowLimiter {
    limit: u32,
    window: Duration,
    state: Mutex<HashMap<IpAddr, VecDeque<Instant>>>,
}

impl SlidingWindowLimiter {
    /// 建立限流器：`limit` 次/`window`。`limit = 0` 表示全部拒绝（配置错误时失败关闭的兜底）。
    pub fn new(limit: u32, window: Duration) -> Self {
        Self {
            limit,
            window,
            state: Mutex::new(HashMap::new()),
        }
    }

    /// 记录一次尝试并判定是否允许。
    pub fn check(&self, key: IpAddr) -> RateLimit {
        self.check_at(key, Instant::now())
    }

    /// 与 [`Self::check`] 相同，但由调用方给出时刻（单测用，避免依赖真实时钟）。
    pub fn check_at(&self, key: IpAddr, now: Instant) -> RateLimit {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if self.limit == 0 {
            return RateLimit::Denied {
                retry_after: self.window,
            };
        }
        if !state.contains_key(&key) && state.len() >= MAX_TRACKED_KEYS {
            evict_idle(&mut state, self.window, now);
            if state.len() >= MAX_TRACKED_KEYS {
                // 满了且没有可淘汰的空窗口：拒绝新键（失败关闭），不放弃对既有键的计数。
                return RateLimit::Denied {
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
            return RateLimit::Denied { retry_after };
        }
        attempts.push_back(now);
        RateLimit::Allowed {
            remaining: self.limit - recorded - 1,
        }
    }

    /// 当前跟踪的键数（诊断与单测用）。
    pub fn tracked_keys(&self) -> usize {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }
}

/// 淘汰窗口内已无记录的键。
fn evict_idle(state: &mut HashMap<IpAddr, VecDeque<Instant>>, window: Duration, now: Instant) {
    state.retain(|_, attempts| {
        attempts
            .back()
            .is_some_and(|last| now.duration_since(*last) < window)
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(text: &str) -> IpAddr {
        text.parse().expect("合法地址")
    }

    #[test]
    fn allows_up_to_the_limit_then_denies() {
        let limiter = SlidingWindowLimiter::new(3, Duration::from_secs(60));
        let start = Instant::now();
        assert!(matches!(
            limiter.check_at(ip("203.0.113.9"), start),
            RateLimit::Allowed { remaining: 2 }
        ));
        assert!(matches!(
            limiter.check_at(ip("203.0.113.9"), start + Duration::from_secs(1)),
            RateLimit::Allowed { remaining: 1 }
        ));
        assert!(matches!(
            limiter.check_at(ip("203.0.113.9"), start + Duration::from_secs(2)),
            RateLimit::Allowed { remaining: 0 }
        ));
        let denied = limiter.check_at(ip("203.0.113.9"), start + Duration::from_secs(3));
        // 最早一条记录在 start+60s 过期，因此建议退避约 57 秒。
        match denied {
            RateLimit::Denied { retry_after } => {
                assert!(retry_after <= Duration::from_secs(57));
                assert!(retry_after >= Duration::from_secs(56));
            }
            RateLimit::Allowed { .. } => panic!("超过上限必须拒绝"),
        }
    }

    #[test]
    fn window_slides_after_the_oldest_attempt_expires() {
        let limiter = SlidingWindowLimiter::new(2, Duration::from_secs(60));
        let start = Instant::now();
        assert!(matches!(
            limiter.check_at(ip("203.0.113.9"), start),
            RateLimit::Allowed { .. }
        ));
        assert!(matches!(
            limiter.check_at(ip("203.0.113.9"), start),
            RateLimit::Allowed { .. }
        ));
        assert!(matches!(
            limiter.check_at(ip("203.0.113.9"), start + Duration::from_secs(30)),
            RateLimit::Denied { .. }
        ));
        assert!(matches!(
            limiter.check_at(ip("203.0.113.9"), start + Duration::from_secs(61)),
            RateLimit::Allowed { .. }
        ));
    }

    #[test]
    fn keys_are_independent() {
        let limiter = SlidingWindowLimiter::new(1, Duration::from_secs(60));
        let start = Instant::now();
        assert!(matches!(
            limiter.check_at(ip("203.0.113.9"), start),
            RateLimit::Allowed { .. }
        ));
        assert!(matches!(
            limiter.check_at(ip("203.0.113.10"), start),
            RateLimit::Allowed { .. }
        ));
        assert!(matches!(
            limiter.check_at(ip("203.0.113.9"), start),
            RateLimit::Denied { .. }
        ));
        assert_eq!(limiter.tracked_keys(), 2);
    }

    #[test]
    fn zero_limit_denies_everything() {
        let limiter = SlidingWindowLimiter::new(0, Duration::from_secs(60));
        assert!(matches!(
            limiter.check(ip("127.0.0.1")),
            RateLimit::Denied { .. }
        ));
        assert_eq!(limiter.tracked_keys(), 0);
    }

    #[test]
    fn tracked_keys_stay_bounded_and_new_keys_fail_closed_when_full() {
        let limiter = SlidingWindowLimiter::new(1, Duration::from_secs(60));
        let start = Instant::now();
        // 填满跟踪表（每个键一条未过期记录）。
        for index in 0..MAX_TRACKED_KEYS {
            let key = IpAddr::V4(std::net::Ipv4Addr::from(
                u32::try_from(index + 1).expect("小整数"),
            ));
            assert!(matches!(
                limiter.check_at(key, start),
                RateLimit::Allowed { .. }
            ));
        }
        assert_eq!(limiter.tracked_keys(), MAX_TRACKED_KEYS);
        // 新键在表满且无空窗口可淘汰时失败关闭，而不是无限增长或重置既有计数。
        let extra = ip("198.51.100.1");
        assert!(matches!(
            limiter.check_at(extra, start),
            RateLimit::Denied { .. }
        ));
        // 空窗口可以被淘汰，于是新键重新可用。
        let later = start + Duration::from_secs(120);
        assert!(matches!(
            limiter.check_at(extra, later),
            RateLimit::Allowed { .. }
        ));
        assert_eq!(limiter.tracked_keys(), 1);
    }

    #[test]
    fn a_key_only_records_up_to_the_limit() {
        let limiter = SlidingWindowLimiter::new(2, Duration::from_secs(60));
        let start = Instant::now();
        for _ in 0..10 {
            let _ = limiter.check_at(ip("203.0.113.9"), start);
        }
        // 被拒的尝试不写入记录，因此单键内存有上限。
        assert!(matches!(
            limiter.check_at(ip("203.0.113.9"), start + Duration::from_secs(59)),
            RateLimit::Denied { .. }
        ));
        assert!(matches!(
            limiter.check_at(ip("203.0.113.9"), start + Duration::from_secs(61)),
            RateLimit::Allowed { .. }
        ));
    }
}
