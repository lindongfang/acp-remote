//! `--file` 的结构化输入与 `provider configure` 的无回显凭据录入（`docs/LOCAL_ADMIN_PROTOCOL.md` §5.2、
//! §5.8、`specs/cli-commands` 的「结构化输入与凭据交互」）。
//!
//! 口径：
//!
//! - `--file` 读到的 JSON 就是方法的 `params`（同名同形，不做字段改名、不补默认值），因此**唯一**的校验
//!   入口仍是方法侧；CLI 只把「文件不是 JSON 对象」和「文件出现 §5.2 之外的字段名」挡在发送之前，错误码
//!   与方法侧一致（`local.invalid_params`）。第二类检查正是「`agent configure` 的文件不得包含凭据值」的
//!   实现：凭据只能以字段值的形式出现，而 §5.2 的六个字段名里没有承载凭据的字段，任何 `values`/
//!   `credentials` 一类的字段名都会在此被拒绝，**不发送、不落日志**。
//! - 凭据值**只**经交互终端逐项无回显读取（`rpassword` 的默认配置即从终端设备读且关回显）。不接受凭据作为
//!   命令行参数（没有这样的 flag）或普通文件输入（`provider configure` 没有 `--file`）；`stdin` 不是交互
//!   终端时在读取之前明确失败（`std::io::IsTerminal`）。
//! - 错误消息只带文件名、不带完整路径（§4 规则 6：消息不得含完整敏感路径）。

use std::io::{IsTerminal as _, Write as _};
use std::path::Path;

use serde_json::Value;
use server::local_admin::{JsonObject, LocalErrorCode};

use crate::cli::Failure;

/// 读取 `--file <path>` 的 JSON 对象作为方法 `params`。
///
/// `allowed` 非空时，顶层字段名必须落在该集合内（§5.2 的 closed object）；出现集合外的字段名即
/// [`LocalErrorCode::InvalidParams`]——与方法侧对未知字段的反应同一个码。
pub(crate) fn read_params_file(path: &Path, allowed: &[&str]) -> Result<JsonObject, Failure> {
    let text = std::fs::read_to_string(path).map_err(|_| {
        Failure::local(
            LocalErrorCode::InvalidParams,
            format!(
                "`--file` 指定的文件不可读（{}）",
                file_name(path.as_os_str().to_string_lossy().as_ref())
            ),
        )
    })?;
    let value: Value = serde_json::from_str(&text).map_err(|error| {
        Failure::local(
            LocalErrorCode::InvalidParams,
            format!(
                "`--file` 的内容不是合法 JSON（{}）：{error}",
                file_name(path.as_os_str().to_string_lossy().as_ref())
            ),
        )
    })?;
    let Value::Object(map) = value else {
        return Err(Failure::local(
            LocalErrorCode::InvalidParams,
            "`--file` 的顶层必须是 JSON 对象（即方法的 `params`）",
        ));
    };
    if let Some(key) = map
        .keys()
        .find(|key| !allowed.is_empty() && !allowed.contains(&key.as_str()))
    {
        return Err(Failure::local(
            LocalErrorCode::InvalidParams,
            format!(
                "`--file` 含未登记的字段 `{key}`（凭据值不得经文件输入；凭据只能用 `provider configure` 在交互终端录入）"
            ),
        ));
    }
    Ok(map)
}

/// 文件名（不含目录）：错误消息里**不**回显完整路径。
fn file_name(text: &str) -> String {
    Path::new(text)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "?".to_owned())
}

/// 凭据值的逐项读取接缝：真实终端与测试注入共用同一形状，使「关回显读取」可单测。
pub(crate) trait SecretPrompt {
    /// 读取一项凭据值（`field` 是 §5.2 `values` 的字段名）。
    fn read_secret(&mut self, field: &str) -> Result<String, Failure>;
}

/// 真实终端录入：构造即要求 `stdin` 是交互终端。
///
/// prompt 写 stderr（不污染 stdout），值只进内存：不落日志、错误、文件或 shell 历史。
pub(crate) struct TerminalPrompt;

impl TerminalPrompt {
    /// 无交互终端时明确失败（不读取凭据、不调用方法）。
    pub(crate) fn new() -> Result<Self, Failure> {
        if std::io::stdin().is_terminal() {
            Ok(Self)
        } else {
            Err(Failure::local(
                LocalErrorCode::InvalidRequest,
                "`provider configure` 需要在交互终端逐项无回显录入凭据：当前 stdin 不是终端",
            ))
        }
    }
}

impl SecretPrompt for TerminalPrompt {
    fn read_secret(&mut self, field: &str) -> Result<String, Failure> {
        if write_prompt(field).is_err() {
            return Err(Failure::local(
                LocalErrorCode::Internal,
                "凭据提示无法写到 stderr",
            ));
        }
        read_secret_line(default_terminal_config()).map_err(|_| {
            Failure::local(
                LocalErrorCode::Internal,
                format!("无法从终端读取凭据字段 `{field}`（凭据值不会回显，也不会进入日志）"),
            )
        })
    }
}

/// prompt 只写 stderr：stdout 留给命令结果，凭据值本身任何情况下都不写。
fn write_prompt(field: &str) -> std::io::Result<()> {
    let mut stderr = std::io::stderr();
    write!(stderr, "凭据 {field}（不回显）: ")?;
    stderr.flush()
}

/// 关回显读取一行的**唯一**实现（`rpassword` 只做这一件事）。
///
/// 默认配置从终端设备读（unix `/dev/tty`、Windows `CONIN$`），因此管道/stdin 无法注入凭据值；配置作为
/// 参数传入，使测试可以注入内存输入与输出汇并以断言「值不被回显」。
fn read_secret_line(config: rpassword::Config) -> std::io::Result<String> {
    rpassword::read_password_with_config(config)
}

/// 真实路径使用的配置：终端设备 + 无回显。
fn default_terminal_config() -> rpassword::Config {
    rpassword::ConfigBuilder::new().build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// 收集「本该出现在终端上的输出」的汇（测试断言值没有被回显到这里）。
    #[derive(Clone, Default)]
    struct SharedSink(Arc<Mutex<Vec<u8>>>);

    impl SharedSink {
        fn contents(&self) -> String {
            let bytes = self.0.lock().expect("汇锁").clone();
            String::from_utf8_lossy(&bytes).into_owned()
        }
    }

    impl std::io::Write for SharedSink {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().expect("汇锁").extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// 关回显读取：返回注入的值，且**不把值写回**输出汇（`specs/cli-commands` R68 的机制级断言）。
    #[test]
    fn the_no_echo_reader_returns_the_value_without_echoing_it() {
        let sink = SharedSink::default();
        let config = rpassword::ConfigBuilder::new()
            .input_data("s3cret-value\n")
            .output_writer(sink.clone())
            .build();
        let value = read_secret_line(config).expect("从注入输入读取");
        assert_eq!(value, "s3cret-value");
        assert!(
            !sink.contents().contains("s3cret"),
            "关回显读取不得把凭据写回终端：{:?}",
            sink.contents()
        );
    }

    /// 假终端读取器：断言 `provider configure` 是**逐项**读取，且值原样进入 `values`。
    struct FakePrompt {
        values: Vec<String>,
        asked: Arc<Mutex<Vec<String>>>,
    }

    impl SecretPrompt for FakePrompt {
        fn read_secret(&mut self, field: &str) -> Result<String, Failure> {
            self.asked
                .lock()
                .expect("问询记录锁")
                .push(field.to_owned());
            if self.values.is_empty() {
                return Err(Failure::local(
                    LocalErrorCode::Internal,
                    "假读取器没有更多值",
                ));
            }
            Ok(self.values.remove(0))
        }
    }

    #[test]
    fn credentials_are_read_one_field_at_a_time_and_never_trimmed() {
        let asked = Arc::new(Mutex::new(Vec::new()));
        let mut prompt = FakePrompt {
            values: vec!["sk-one".to_owned(), " sk-two ".to_owned()],
            asked: Arc::clone(&asked),
        };
        let fields = ["api_key".to_owned(), "base_url".to_owned()];
        let values = crate::cli::provider_values(&fields, &mut prompt).expect("逐项读取");
        assert_eq!(
            asked.lock().expect("锁").clone(),
            vec!["api_key", "base_url"]
        );
        assert_eq!(
            values,
            vec![
                ("api_key".to_owned(), "sk-one".to_owned()),
                ("base_url".to_owned(), " sk-two ".to_owned())
            ],
            "凭据值原样发送，不做 trim/规范化"
        );
    }

    #[test]
    fn a_non_object_file_and_an_unregistered_field_are_rejected_with_invalid_params() {
        let dir = std::env::temp_dir().join(format!("acpr-wp4b-input-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("临时目录");
        let array = dir.join("array.json");
        std::fs::write(&array, b"[1, 2]").expect("写入");
        let error = read_params_file(&array, &[]).expect_err("顶层不是对象必须失败");
        assert_eq!(error.code(), LocalErrorCode::InvalidParams.as_str());

        let broken = dir.join("broken.json");
        std::fs::write(&broken, b"{ not json").expect("写入");
        assert_eq!(
            read_params_file(&broken, &[])
                .expect_err("非法 JSON")
                .code(),
            LocalErrorCode::InvalidParams.as_str()
        );

        let credentials = dir.join("credentials.json");
        std::fs::write(
            &credentials,
            br#"{"agentId":"a","displayName":"A","command":"c","args":[],"envAllowlist":[],"default":false,"values":{"api_key":"sk-x"}}"#,
        )
        .expect("写入");
        let error = read_params_file(
            &credentials,
            &[
                "agentId",
                "displayName",
                "command",
                "args",
                "envAllowlist",
                "default",
            ],
        )
        .expect_err("凭据字段必须被拒绝");
        assert_eq!(error.code(), LocalErrorCode::InvalidParams.as_str());
        assert!(
            !error.message().contains("sk-x"),
            "错误消息不得回显凭据值：{}",
            error.message()
        );
        assert!(
            !error.message().contains(&dir.display().to_string()),
            "错误消息不得回显完整路径：{}",
            error.message()
        );

        let ok = dir.join("ok.json");
        std::fs::write(
            &ok,
            br#"{"agentId":"a","displayName":"A","command":"c","args":[],"envAllowlist":[],"default":false}"#,
        )
        .expect("写入");
        let params = read_params_file(
            &ok,
            &[
                "agentId",
                "displayName",
                "command",
                "args",
                "envAllowlist",
                "default",
            ],
        )
        .expect("合法文件");
        assert_eq!(params.get("agentId"), Some(&Value::from("a")));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
