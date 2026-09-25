//! `windows-local-ipc`：ACP Remote 本地管理通道在 Windows 上的最小 Win32 FFI wrapper。
//!
//! # 为什么这里允许 `unsafe`
//!
//! 本 crate 是**全仓库唯一允许写 `unsafe` 的位置**。workspace 的 lint 固定 `unsafe_code = "forbid"`
//! （根 `Cargo.toml` 的 `[workspace.lints.rust]`），而 `docs/LOCAL_ADMIN_PROTOCOL.md` §2.2 要求的两层
//! 访问控制在 Windows 上没有 safe 实现可拼：创建 Named Pipe 时必须带「只允许当前 OS 用户」的安全描述符
//! （`CreateNamedPipeW` 的 `SECURITY_ATTRIBUTES`），连接建立后还必须取对端进程的用户 SID 做比对
//! （`GetNamedPipeClientProcessId` → `OpenProcess` → `OpenProcessToken` → `GetTokenInformation(TokenUser)`
//! → `ConvertSidToStringSidW`）。因此本 crate 以仓库内 **path 依赖**存在：登记在根 `Cargo.toml` 的
//! `workspace.exclude`，不是 workspace 成员、不发布、不继承 workspace lint，把 FFI 收敛在这一个目录里，
//! 对外只暴露 safe 函数（`daemon-cli-and-local-admin` 变更的 design.md 决策 3）。
//!
//! 收敛手段：`#![allow(unsafe_code)]` 只在本 crate 生效；每个 `unsafe` 块都写明其不变量
//! （句柄所有权、缓冲区大小或指针有效性）；需要释放的资源（`HANDLE`、`LocalAlloc` 内存）
//! 由带 `Drop` 的守卫持有，因此所有提前返回路径也只在守卫作用域结束时释放一次。
//!
//! # 公开面（仅 `cfg(windows)`）
//!
//! - [`create_pipe_server`]：以「仅当前 OS 用户」的 DACL 创建 Named Pipe 的首个实例，并包装为
//!   `tokio::net::windows::named_pipe::NamedPipeServer`；
//! - [`current_user_sid`]：当前进程用户的 SID 字符串（`S-1-5-21-…`），供调用方比对与 pipe 名哈希；
//! - [`client_user_sid`]：已连接 pipe 对端的用户 SID。
//!
//! 非 Windows 平台上本 crate 编译为**空** crate：这三个函数不存在，调用方（`server::transport::local`）
//! 用 `cfg(windows)` 门控调用点，因此这里不需要、也不提供跨平台统一的失败签名。
//!
//! # 有意不做的
//!
//! 不暴露原始 `HANDLE` 或 `SECURITY_ATTRIBUTES`，不接受调用方传入的 SDDL（安全描述符由本 crate 按
//! 当前用户 SID 生成），不提供 pipe 读写——字节的 framing 与 channel 绑定归 `server::transport::local`。

#![allow(unsafe_code)]

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::{client_user_sid, create_pipe_server, current_user_sid};
