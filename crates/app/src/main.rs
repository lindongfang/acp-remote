//! `acp-remote` 可执行程序的入口。
//!
//! 二进制只负责把参数与退出码交给操作系统：解析、配置加载与 runtime 装配都在 [`app::cli`]，业务规则在
//! `core`，入站协议在 `server`，生命周期语义在 `app::daemon`。

fn main() -> std::process::ExitCode {
    app::cli::run()
}
