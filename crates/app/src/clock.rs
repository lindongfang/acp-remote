//! 本机时钟（`core::ports::Clock` 的实现）与 `core` 时间戳文本的格式化。
//!
//! `core` 的 `Timestamp` 是**不透明定宽文本**（`%Y-%m-%dT%H:%M:%S%.3fZ`），不暴露日期算术；本 crate
//! 不能依赖日期库，因此这里只做**纯函数**的公历换算（Hinnant 的 `days_from_civil`/`civil_from_days`）。
//! 仓库内已有两处同款算术（`crates/server/src/local_admin/pairing.rs` 的毫秒加减、
//! `crates/storage-sqlite/src/migrate.rs` 的保留窗口），本次是第三处，理由与那次相同：`core` 不提供
//! 时间工具，而反方向的「让 `core` 依赖日期库」是更大的取舍。三处都只覆盖公历换算，不涉及时区。

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use acp_core::model::Timestamp;
use acp_core::ports::Clock;

/// Unix epoch 的规范时间戳文本。
///
/// 常量、定宽 24 字符、分隔符固定、全数字——`core` 的 `is_timestamp` 只做形状判定，因此构造不会失败。
const EPOCH_TEXT: &str = "1970-01-01T00:00:00.000Z";

const DAY_MILLIS: i64 = 86_400_000;
const MIN_UNIX_MILLIS: i64 = -62_167_219_200_000;
const MAX_UNIX_MILLIS: i64 = 253_402_300_799_999;

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

/// 验收专用文件时钟构造失败。只给稳定分类，错误文本不携带路径。
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FileClockError {
    #[error("the test clock file cannot be read")]
    Unreadable,
    #[error("the test clock file must contain a canonical timestamp or Unix milliseconds")]
    InvalidValue,
}

/// 由外部文件推进的验收时钟。
///
/// 每个文件内容要么是 24 字节规范 [`Timestamp`]，要么是 Unix 毫秒整数。测试更新文件时必须先写同目录临时文件，
/// 再用原子 rename 替换，避免进程读到半写内容。读取失败或内容非法时保持在上一个有效值并记录错误，测试
/// 时间因此不会回落到系统时间或在 29:59.999/30:00.000 边界附近自行滑动。
#[derive(Debug)]
pub struct FileClock {
    path: PathBuf,
    last_valid: Mutex<Timestamp>,
}

impl FileClock {
    /// 打开并验证文件。生产组合根只会在显式开发模式且设置验收环境变量时调用它。
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, FileClockError> {
        let path = path.into();
        let initial = read_clock_file(&path)?;
        Ok(Self {
            path,
            last_valid: Mutex::new(initial),
        })
    }
}

impl Clock for FileClock {
    fn now(&self) -> Timestamp {
        let mut last_valid = lock(&self.last_valid);
        match read_clock_file(&self.path) {
            Ok(timestamp) => {
                *last_valid = timestamp.clone();
                timestamp
            }
            Err(error) => {
                tracing::error!(
                    event = "clock.test_file_invalid",
                    error = ?error,
                    "验收时钟文件不可读或内容非法；保持上一个有效时间，不推进扫描"
                );
                last_valid.clone()
            }
        }
    }
}

fn read_clock_file(path: &Path) -> Result<Timestamp, FileClockError> {
    let text = std::fs::read_to_string(path).map_err(|_| FileClockError::Unreadable)?;
    parse_clock_value(text.trim())
}

fn parse_clock_value(text: &str) -> Result<Timestamp, FileClockError> {
    if let Ok(millis) = text.parse::<i64>() {
        return timestamp_from_unix_millis_checked(millis).ok_or(FileClockError::InvalidValue);
    }
    Timestamp::new(text).map_err(|_| FileClockError::InvalidValue)
}

fn timestamp_from_unix_millis_checked(millis: i64) -> Option<Timestamp> {
    if !(MIN_UNIX_MILLIS..=MAX_UNIX_MILLIS).contains(&millis) {
        return None;
    }
    let text = format_unix_millis(millis);
    if text.len() != 24 {
        return None;
    }
    Timestamp::new(&text).ok()
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
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
    fn file_clock_accepts_the_complete_canonical_year_range() {
        assert_eq!(
            parse_clock_value("-62167219200000")
                .expect("minimum timestamp")
                .as_str(),
            "0000-01-01T00:00:00.000Z"
        );
        assert_eq!(
            parse_clock_value("253402300799999")
                .expect("maximum timestamp")
                .as_str(),
            "9999-12-31T23:59:59.999Z"
        );
    }

    #[test]
    fn file_clock_rejects_milliseconds_outside_the_canonical_year_range() {
        assert_eq!(
            parse_clock_value("-62167219200001"),
            Err(FileClockError::InvalidValue)
        );
        assert_eq!(
            parse_clock_value("253402300800000"),
            Err(FileClockError::InvalidValue)
        );

        let root = std::env::temp_dir().join(format!(
            "acpr-file-clock-invalid-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time after epoch")
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("temporary clock directory");
        let path = root.join("clock");
        for value in ["-62167219200001", "253402300800000"] {
            std::fs::write(&path, value).expect("invalid clock value");
            assert_eq!(
                FileClock::open(&path).expect_err("out-of-range value must fail closed"),
                FileClockError::InvalidValue
            );
        }
        std::fs::remove_dir_all(&root).expect("remove temporary clock directory");
    }

    #[test]
    fn the_system_clock_advances() {
        let clock = SystemClock::new();
        let first = clock.now();
        let second = clock.now();
        assert!(second.as_str() >= first.as_str(), "{first} → {second}");
        assert!(first.as_str() >= "2024-01-01T00:00:00.000Z");
    }

    #[test]
    fn the_file_clock_crosses_the_retention_boundary_exactly() {
        let root = std::env::temp_dir().join(format!(
            "acpr-file-clock-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time after epoch")
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("temporary clock directory");
        let path = root.join("clock");
        std::fs::write(&path, "2026-10-26T00:29:59.999Z").expect("initial clock value");
        let clock = FileClock::open(&path).expect("valid file clock");
        assert_eq!(clock.now().as_str(), "2026-10-26T00:29:59.999Z");

        let replacement = root.join("clock.next");
        std::fs::write(&replacement, "2026-10-26T00:30:00.000Z").expect("next clock value");
        std::fs::rename(&replacement, &path).expect("atomic clock replacement");
        assert_eq!(clock.now().as_str(), "2026-10-26T00:30:00.000Z");

        std::fs::write(&path, "not-a-timestamp").expect("invalid clock value");
        assert_eq!(
            clock.now().as_str(),
            "2026-10-26T00:30:00.000Z",
            "invalid input must keep the last valid value"
        );
        std::fs::remove_dir_all(&root).expect("remove temporary clock directory");
    }
}
