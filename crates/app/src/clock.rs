//! 本机时钟（`core::ports::Clock` 的实现）与 `core` 时间戳文本的格式化。
//!
//! `core` 的 `Timestamp` 是**不透明定宽文本**（`%Y-%m-%dT%H:%M:%S%.3fZ`），不暴露日期算术；本 crate
//! 不能依赖日期库，因此这里只做**纯函数**的公历换算（Hinnant 的 `days_from_civil`/`civil_from_days`）。
//! 仓库内已有两处同款算术（`crates/server/src/local_admin/pairing.rs` 的毫秒加减、
//! `crates/storage-sqlite/src/migrate.rs` 的保留窗口），本次是第三处，理由与那次相同：`core` 不提供
//! 时间工具，而反方向的「让 `core` 依赖日期库」是更大的取舍。三处都只覆盖公历换算，不涉及时区。

use std::time::{SystemTime, UNIX_EPOCH};

use acp_core::model::Timestamp;
use acp_core::ports::Clock;

/// Unix epoch 的规范时间戳文本。
///
/// 常量、定宽 24 字符、分隔符固定、全数字——`core` 的 `is_timestamp` 只做形状判定，因此构造不会失败。
const EPOCH_TEXT: &str = "1970-01-01T00:00:00.000Z";

const DAY_MILLIS: i64 = 86_400_000;

/// 系统时钟：`Clock` 端口的唯一实现（`core` 不读系统时间，时间一律由本实现提供）。
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl SystemClock {
    /// 构造。
    pub const fn new() -> Self {
        Self
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        let millis = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(elapsed) => i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX),
            // 系统时间早于 Unix epoch：不 panic，也不编造「现在」，按 epoch 记（审计时间戳退化但形状合法）。
            Err(_) => 0,
        };
        timestamp_from_unix_millis(millis)
    }
}

/// Unix 毫秒 → 规范时间戳文本。
///
/// 形状由 [`format_unix_millis`] 保证；走到 `Err` 只可能说明该函数被改坏，此时退回 epoch 常量并记
/// 一条错误日志——**不 panic**，也不静默跳过一个时间戳。
pub fn timestamp_from_unix_millis(millis: i64) -> Timestamp {
    match Timestamp::new(&format_unix_millis(millis)) {
        Ok(timestamp) => timestamp,
        Err(_) => {
            tracing::error!(
                event = "clock.timestamp_invalid",
                "系统时间无法格式化为规范时间戳（按 epoch 记录）"
            );
            // 见 `EPOCH_TEXT`：该常量必然通过形状校验，这里的 `expect` 不是可失败路径。
            Timestamp::new(EPOCH_TEXT).expect("常量文本必然通过 Timestamp 的形状校验")
        }
    }
}

/// 按 `%Y-%m-%dT%H:%M:%S%.3fZ` 格式化 Unix 毫秒（负数按 floor 借位，不产生负的分/秒字段）。
pub fn format_unix_millis(millis: i64) -> String {
    let days = millis.div_euclid(DAY_MILLIS);
    let rest = millis.rem_euclid(DAY_MILLIS);
    let (year, month, day) = civil_from_days(days);
    let hour = rest / 3_600_000;
    let minute = rest % 3_600_000 / 60_000;
    let second = rest % 60_000 / 1_000;
    let milli = rest % 1_000;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{milli:03}Z")
}

/// Howard Hinnant, `civil_from_days`：`1970-01-01` 为第 0 天。
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

    /// 与 `identity-auth` 的秒级换算互检（同一套公历算法，两个实现独立）。
    #[test]
    fn formatting_matches_the_canonical_shape() {
        assert_eq!(format_unix_millis(0), EPOCH_TEXT);
        assert_eq!(format_unix_millis(1_000), "1970-01-01T00:00:01.000Z");
        assert_eq!(
            format_unix_millis(1_789_722_723_412),
            "2026-09-18T09:12:03.412Z"
        );
        assert_eq!(
            format_unix_millis(1_800_000_000_000),
            identity_auth::timestamp_from_unix_seconds(1_800_000_000)
                .expect("秒换算")
                .as_str()
        );
        // 闰日与跨年（负数也按 floor 借位）。
        assert_eq!(
            format_unix_millis(1_709_164_800_000),
            "2024-02-29T00:00:00.000Z"
        );
        assert_eq!(format_unix_millis(-1), "1969-12-31T23:59:59.999Z");
        assert_eq!(timestamp_from_unix_millis(0).as_str(), EPOCH_TEXT);
    }

    #[test]
    fn the_system_clock_advances() {
        let clock = SystemClock::new();
        let first = clock.now();
        let second = clock.now();
        assert!(second.as_str() >= first.as_str(), "{first} → {second}");
        assert!(first.as_str() >= "2024-01-01T00:00:00.000Z");
    }
}
