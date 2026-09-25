//! `logging.*` 的消费：组合根初始化 `tracing` 订阅器（`CONFIG_REFERENCE.md` §11）。
//!
//! 为什么自研而不是用 `tracing-subscriber`：本变更登记的依赖只有 `clap`/`toml`/`fs4`（`docs/MODULE_ARCHITECTURE.md`
//! §3.1），新增依赖必须走同一登记与 CI 判定，不属本工作包范围。`tracing` 本身已 re-export 实现订阅器
//! 所需的全部类型（`Subscriber`/`field::Visit`/`Event`/`LevelFilter`），因此这里实现一个**最小**订阅器：
//! 只做级别过滤与单行格式化，不实现 span、采样、layer 或重载。根 `Cargo.toml` 的注释「订阅器由组合根
//! 初始化」正是这条职责。
//!
//! 允许字段（`SECURITY_DESIGN.md` §14.1）：本模块**不裁剪字段**，字段纪律由各调用点负责——只写事件名、
//! 计数、状态码与方法名，不写 pairingUrl、secret、凭据值、完整 prompt 或完整敏感路径。

use std::io::Write as _;
use std::sync::Mutex;

use tracing::field::{Field, Visit};
use tracing::level_filters::LevelFilter;
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Level, Metadata, Subscriber};

use crate::clock::format_unix_millis;
use crate::config::{LogFormat, LoggingConfig};

/// 订阅器初始化失败。
#[derive(Debug, thiserror::Error)]
pub enum LogError {
    /// `logging.file` 打不开。
    #[error("日志文件不可用：{source}")]
    Unwritable {
        /// 底层错误。
        #[source]
        source: std::io::Error,
    },
    /// 进程内已经安装过全局订阅器。
    #[error("日志订阅器已经初始化")]
    AlreadyInstalled,
}

/// 安装全局订阅器（进程内只允许一次）。
///
/// `logging.file` 为 `null` 时写 stderr（`CONFIG_REFERENCE.md` §11）；写文件时按 `SECURITY_DESIGN.md`
/// §13.2 以 `0600` 创建（unix），并只追加不截断。
pub fn init(config: &LoggingConfig) -> Result<(), LogError> {
    let writer: Box<dyn std::io::Write + Send> = match &config.file {
        Some(path) => Box::new(open_log_file(path)?),
        None => Box::new(std::io::stderr()),
    };
    let subscriber = Formatter::new(config.level, config.format, writer);
    tracing::subscriber::set_global_default(subscriber).map_err(|_| LogError::AlreadyInstalled)
}

/// 以 `0600` 追加打开日志文件。
fn open_log_file(path: &std::path::Path) -> Result<std::fs::File, LogError> {
    let mut options = std::fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    options
        .open(path)
        .map_err(|source| LogError::Unwritable { source })
}

/// 单行格式化订阅器：`text` 输出 `key=value`，`json` 输出一个 JSON 对象（字段集合相同）。
pub struct Formatter {
    level: Level,
    format: LogFormat,
    writer: Mutex<Box<dyn std::io::Write + Send>>,
    next_span: std::sync::atomic::AtomicU64,
}

impl Formatter {
    /// 构造（`writer` 通常来自 [`init`]，测试里可以是内存缓冲）。
    pub fn new(level: Level, format: LogFormat, writer: Box<dyn std::io::Write + Send>) -> Self {
        Self {
            level,
            format,
            writer: Mutex::new(writer),
            next_span: std::sync::atomic::AtomicU64::new(1),
        }
    }

    /// 写一条已格式化的记录；写失败只丢弃（日志不得让进程失败，也不得 panic）。
    fn emit(&self, line: String) {
        match self.writer.lock() {
            Ok(mut writer) => {
                let _ = writer.write_all(line.as_bytes());
                let _ = writer.write_all(b"\n");
                let _ = writer.flush();
            }
            Err(poisoned) => {
                let mut writer = poisoned.into_inner();
                let _ = writer.write_all(line.as_bytes());
                let _ = writer.write_all(b"\n");
            }
        }
    }
}

impl Subscriber for Formatter {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        *metadata.level() <= self.level
    }

    fn max_level_hint(&self) -> Option<LevelFilter> {
        Some(LevelFilter::from_level(self.level))
    }

    fn new_span(&self, _span: &Attributes<'_>) -> Id {
        // 本项目不依赖 span 语义（日志只用事件）；给一个单调 id 以满足契约，不记录 span 内容。
        Id::from_u64(
            self.next_span
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        )
    }

    fn record(&self, _span: &Id, _values: &Record<'_>) {}

    fn record_follows_from(&self, _span: &Id, _follows: &Id) {}

    fn enter(&self, _span: &Id) {}

    fn exit(&self, _span: &Id) {}

    fn event(&self, event: &Event<'_>) {
        if !self.enabled(event.metadata()) {
            return;
        }
        let mut fields = FieldCollector::default();
        event.record(&mut fields);
        let timestamp = format_unix_millis(unix_millis_now());
        let level = event.metadata().level().as_str();
        let target = event.metadata().target();
        let line = match self.format {
            LogFormat::Text => {
                let mut line = format!("{timestamp} {level} {target}");
                for (name, value) in &fields.fields {
                    line.push(' ');
                    line.push_str(name);
                    line.push('=');
                    line.push_str(&value.as_text());
                }
                line
            }
            LogFormat::Json => {
                let mut object = serde_json::Map::with_capacity(fields.fields.len() + 3);
                object.insert("ts".to_owned(), serde_json::Value::from(timestamp));
                object.insert("level".to_owned(), serde_json::Value::from(level));
                object.insert("target".to_owned(), serde_json::Value::from(target));
                for (name, value) in fields.fields {
                    object.insert(name, value.into_json());
                }
                serde_json::Value::Object(object).to_string()
            }
        };
        self.emit(line);
    }
}

/// 当前 Unix 毫秒（日志时间戳；与 `Clock` 端口同源实现）。
fn unix_millis_now() -> i64 {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(elapsed) => i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX),
        Err(_) => 0,
    }
}

/// `text` 格式里需要引号的值（含空白或引号）。
fn quote_if_needed(value: &str) -> String {
    if value.is_empty() || value.chars().any(char::is_whitespace) {
        return format!("{value:?}");
    }
    value.to_owned()
}

/// 一个日志字段值（`text` 输出一律按文本；`json` 保留数字/布尔的类型）。
enum FieldValue {
    /// 文本（含 `Debug` 格式化结果）：`text` 下必要时加引号。
    Text(String),
    /// 数字/布尔：两种格式下都按字面量输出。
    Json(serde_json::Value),
}

impl FieldValue {
    fn as_text(&self) -> String {
        match self {
            Self::Text(text) => quote_if_needed(text),
            Self::Json(value) => value.to_string(),
        }
    }

    fn into_json(self) -> serde_json::Value {
        match self {
            Self::Text(text) => serde_json::Value::from(text),
            Self::Json(value) => value,
        }
    }
}

/// 收集事件字段（保持事件里的书写顺序）。
#[derive(Default)]
struct FieldCollector {
    fields: Vec<(String, FieldValue)>,
}

impl Visit for FieldCollector {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.fields.push((
            field.name().to_owned(),
            FieldValue::Text(format!("{value:?}")),
        ));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.fields
            .push((field.name().to_owned(), FieldValue::Text(value.to_owned())));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.fields.push((
            field.name().to_owned(),
            FieldValue::Json(serde_json::Value::from(value)),
        ));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields.push((
            field.name().to_owned(),
            FieldValue::Json(serde_json::Value::from(value)),
        ));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.fields.push((
            field.name().to_owned(),
            FieldValue::Json(serde_json::Value::from(value)),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// 内存写端（测试断言输出）。
    #[derive(Clone, Default)]
    struct Buffer(Arc<Mutex<Vec<u8>>>);

    impl Buffer {
        fn text(&self) -> String {
            let bytes = self.0.lock().expect("未中毒").clone();
            String::from_utf8(bytes).expect("UTF-8")
        }
    }

    impl std::io::Write for Buffer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().expect("未中毒").extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn text_and_json_lines_carry_the_documented_fields() {
        let text_buffer = Buffer::default();
        let subscriber =
            Formatter::new(Level::INFO, LogFormat::Text, Box::new(text_buffer.clone()));
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(
                event = "daemon.maintenance",
                expired_pairings = 2,
                "周期清理"
            );
            tracing::debug!(event = "daemon.hidden", "低于级别的事件不得输出");
        });
        let text = text_buffer.text();
        assert!(text.contains(" INFO "), "{text}");
        assert!(text.contains("event=daemon.maintenance"), "{text}");
        assert!(text.contains("expired_pairings=2"), "{text}");
        assert!(text.contains("周期清理"), "{text}");
        assert!(!text.contains("daemon.hidden"), "级别过滤必须生效：{text}");

        let json_buffer = Buffer::default();
        let subscriber =
            Formatter::new(Level::DEBUG, LogFormat::Json, Box::new(json_buffer.clone()));
        tracing::subscriber::with_default(subscriber, || {
            tracing::warn!(event = "daemon.stopping", grace_ms = 10_000, "关闭中");
        });
        let json_text = json_buffer.text();
        let line = json_text.lines().next().expect("一行");
        let parsed: serde_json::Value = serde_json::from_str(line).expect("合法 JSON");
        assert_eq!(parsed["level"], serde_json::Value::from("WARN"));
        assert_eq!(parsed["event"], serde_json::Value::from("daemon.stopping"));
        assert_eq!(parsed["grace_ms"], serde_json::Value::from(10_000u64));
        assert_eq!(parsed["message"], serde_json::Value::from("关闭中"));
        assert!(parsed["ts"].as_str().expect("时间戳").ends_with('Z'));
    }
}
