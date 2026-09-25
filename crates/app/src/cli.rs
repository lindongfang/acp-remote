//! `acp-remote` 的命令行入口。
//!
//! **本切片只有 `daemon start`**（前台进程）：其余子命令（`daemon stop|status`、`workspace select`、
//! `agent configure`、`provider configure`、配对、Export/Import、`doctor`、`acp-stdio`）属 WP4b
//! （`tasks.md` 2.15–2.17），此处不提供「占位子命令」——未实现的子命令由 clap 直接以用法错误结束。
//!
//! 退出码与错误输出（`cli-commands` 规格的「退出码与错误输出契约」，WP4b 会在此基础上补齐）：
//! 成功 `0`；失败非零，stdout 一行人类可读说明，stderr **恰好一行**含 `code` 的结构化 JSON，`code`
//! 取自 `LOCAL_ADMIN_PROTOCOL.md` §6 的本地错误码表。`error.message` 不是稳定契约。

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};

use crate::config::{Config, config_path_from_env};
use crate::daemon;

/// ACP Remote 的本地 Daemon 与管理 CLI。
#[derive(Debug, Parser)]
#[command(
    name = "acp-remote",
    version,
    about = "ACP Remote 的本地 Daemon 与管理 CLI",
    disable_help_subcommand = true
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Daemon 生命周期（`daemon start` 是前台进程）。
    Daemon {
        #[command(subcommand)]
        command: DaemonCommand,
    },
}

#[derive(Debug, Subcommand)]
enum DaemonCommand {
    /// 前台启动 Daemon（取得单实例锁、创建本地管理 endpoint 并开始接受连接）。
    Start(DaemonStart),
}

#[derive(Debug, Args)]
struct DaemonStart {
    /// 配置文件路径（默认取 `ACP_REMOTE_CONFIG`，再退到平台默认位置）。
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,
}

/// 解析参数、加载配置并运行。返回值即进程退出码。
pub fn run() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Daemon {
            command: DaemonCommand::Start(start),
        } => daemon_start(start),
    }
}

/// `daemon start`：前台运行 Daemon，直到关闭序列完成。
fn daemon_start(start: DaemonStart) -> ExitCode {
    let path = start.config.or_else(config_path_from_env);
    let loaded = match Config::load(path.as_deref()) {
        Ok(loaded) => loaded,
        Err(error) => return fail(code_for_config_error(&error), &error.to_string()),
    };
    if let Err(error) = crate::logging::init(&loaded.config.logging) {
        return fail("local.internal", &error.to_string());
    }
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => return fail("local.internal", &error.to_string()),
    };
    match runtime.block_on(daemon::run(loaded)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => fail(error.code(), &error.message()),
    }
}

/// 配置解析失败的错误码：`local.invalid_params`（「出现未知字段/类型不符/越界」正是配置文件的失败形态）。
fn code_for_config_error(error: &crate::config::ConfigError) -> &'static str {
    match error {
        crate::config::ConfigError::Unreadable { .. }
        | crate::config::ConfigError::NoDefaultDirectory => "local.unavailable",
        crate::config::ConfigError::Invalid { .. }
        | crate::config::ConfigError::ManagedSectionInStartupConfig { .. } => {
            "local.invalid_params"
        }
    }
}

/// 失败的统一出口：stdout 一行人类可读说明 + stderr 恰好一行结构化 JSON（含 `code`）。
fn fail(code: &str, message: &str) -> ExitCode {
    println!("acp-remote: {message}");
    let line = serde_json::json!({ "code": code, "message": message });
    eprintln!("{line}");
    ExitCode::FAILURE
}

/// 供测试断言使用的错误行形状（与 [`fail`] 共用同一构造，避免两处漂移）。
#[cfg(test)]
pub(crate) fn error_line(code: &str, message: &str) -> String {
    serde_json::json!({ "code": code, "message": message }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory as _;

    #[test]
    fn the_cli_exposes_only_daemon_start() {
        let command = Cli::command();
        let subcommands: Vec<&str> = command
            .get_subcommands()
            .map(|subcommand| subcommand.get_name())
            .collect();
        assert_eq!(subcommands, vec!["daemon"]);
        let daemon = command
            .get_subcommands()
            .find(|subcommand| subcommand.get_name() == "daemon")
            .expect("daemon 子命令");
        let nested: Vec<&str> = daemon
            .get_subcommands()
            .map(|subcommand| subcommand.get_name())
            .collect();
        assert_eq!(nested, vec!["start"], "本切片只提供 `daemon start`");
        // 用法错误必须能被 clap 检出（未实现的子命令不是静默 no-op）。
        assert!(Cli::try_parse_from(["acp-remote", "daemon", "stop"]).is_err());
        assert!(Cli::try_parse_from(["acp-remote", "daemon", "start"]).is_ok());
        assert!(Cli::try_parse_from(["acp-remote", "daemon", "start", "--config", "x"]).is_ok());
    }

    #[test]
    fn the_failure_line_is_one_json_object_with_a_code() {
        let line = error_line("local.conflict", "another acp-remote daemon is running");
        assert!(!line.contains('\n'), "必须恰好一行：{line}");
        let parsed: serde_json::Value = serde_json::from_str(&line).expect("合法 JSON");
        assert_eq!(parsed["code"], serde_json::Value::from("local.conflict"));
        assert!(parsed["message"].is_string());
    }

    #[test]
    fn config_errors_map_into_the_local_error_table() {
        assert_eq!(
            code_for_config_error(&crate::config::ConfigError::Invalid {
                detail: String::new()
            }),
            "local.invalid_params"
        );
        assert_eq!(
            code_for_config_error(&crate::config::ConfigError::Unreadable {
                source: std::io::Error::from(std::io::ErrorKind::NotFound)
            }),
            "local.unavailable"
        );
    }
}
