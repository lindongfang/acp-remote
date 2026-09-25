//! `cfg(windows)` 下的 Win32 FFI 实现（仅在 Windows 构建里参与编译）。
//!
//! FFI 面收敛为三件事：SDDL → 自相对安全描述符、带安全属性创建 pipe 的首个实例、取对端进程用户的 SID。
//! 每个 `unsafe` 块都注明其不变量：句柄所有权（谁能关闭、关几次）、缓冲区大小（读多少才不越界）、
//! 指针有效性（借用的存活期、对齐要求）。
//!
//! 错误一律用 `std::io::Result`：Win32 的失败改成 `io::Error::last_os_error()`（保留 `raw_os_error`），
//! 本 crate 自己判定的输入错误用 `ErrorKind::InvalidInput` / `InvalidData`，调用方按 `io::Error` 一种类型处理。

use std::ffi::c_void;
use std::io;
use std::mem::size_of;
use std::os::windows::io::{AsRawHandle, RawHandle};
use std::ptr;

use tokio::net::windows::named_pipe::NamedPipeServer;
use windows_sys::Win32::Foundation::{
    CloseHandle, FALSE, HANDLE, HLOCAL, INVALID_HANDLE_VALUE, LocalFree,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::{
    GetTokenInformation, PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER,
    TokenUser,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, PIPE_ACCESS_DUPLEX,
};
use windows_sys::Win32::System::Pipes::{
    CreateNamedPipeW, GetNamedPipeClientProcessId, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
    PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::core::PWSTR;

/// `dwOpenMode`：双向、必须是该 pipe 名的首个实例、句柄必须可重叠。
///
/// - `FILE_FLAG_FIRST_PIPE_INSTANCE`：该名字已有实例时创建失败（而不是静默再开一个实例），
///   这样「同一个 Daemon 的 endpoint 被起了两次」表现为明确错误。
/// - `FILE_FLAG_OVERLAPPED`：tokio 的 Named Pipe 必须绑定 IOCP；非重叠句柄会让
///   `NamedPipeServer::connect()` 阻塞线程而不是异步完成。
const OPEN_MODE: u32 = PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE | FILE_FLAG_OVERLAPPED;

/// `dwPipeMode`：字节流（channel 与长度前缀由本地通道自己解释）、阻塞等待、拒绝远端客户端。
const PIPE_MODE: u32 = PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS;

/// `nOutBufferSize`/`nInBufferSize`：与 tokio `ServerOptions::new()` 的默认值同值。
///
/// 第一个实例由本 crate 创建，后续实例由 `server` 用 tokio 的 safe `ServerOptions` 创建；同一 pipe 名的
/// 实例参数必须一致，因此这里的取值刻意对齐 tokio 的默认值（`65536`），而不是另选一个数。
const PIPE_BUFFER_SIZE: u32 = 65536;

/// `nDefaultTimeOut`：与 tokio `ServerOptions::new()` 的默认值一致（由 `WaitNamedPipeW` 使用，本通道不依赖它）。
const PIPE_TIMEOUT: u32 = 0;

/// 以「仅当前 OS 用户」的 DACL 创建 Named Pipe 的首个实例，并包装为 tokio 的服务器类型。
///
/// `pipe_name` 是完整 pipe 路径（`\\.\pipe\<name>`，`docs/LOCAL_ADMIN_PROTOCOL.md` §2.1）。
/// 安全属性：DACL 只含一条 ACE——允许当前进程用户的 SID 完全访问（`GA`），`D:P` 表示不继承父容器的 ACE；
/// 句柄不继承给子进程。拒绝远端客户端由 `dwPipeMode` 的 `PIPE_REJECT_REMOTE_CLIENTS` 承担。
///
/// 失败即失败关闭：pipe 名含内嵌 NUL、名字不在 `\\.\pipe\` 命名空间、该名字已有首个实例、安全描述符
/// 无法生成，都返回 `Err` 且不留下已创建的 Windows 句柄（所有提前返回路径都由守卫释放资源；见下 `Panics`）。
///
/// # Panics
///
/// 必须在已开启 I/O 的 Tokio runtime 内调用——这与 tokio 自己的 `ServerOptions::create` 是同一条约束
/// （两者最后都走 `PollEvented` 的 reactor 断言）。runtime 之外调用时 tokio 会以 “there is no reactor
/// running” panic；该路径上句柄已随内部 mio `NamedPipe` 的 `Drop` 释放，因此 panic 同样不留下残余 pipe 实例
/// （用例 `runtime_requirement_holds_without_leaking_the_pipe_instance` 把这两点都断言下来）。
pub fn create_pipe_server(pipe_name: &str) -> io::Result<NamedPipeServer> {
    let user_sid = current_user_sid()?;
    create_pipe_server_for_sid(pipe_name, &user_sid)
}

/// 当前进程用户的 SID 字符串（`S-1-5-21-…`）。
///
/// 取自本进程的进程令牌（`OpenProcessToken` + `GetTokenInformation(TokenUser)`），不是环境变量、
/// 不是名字查询，也不受进程模拟（impersonation）之外的上下文影响：返回值就是
/// `docs/LOCAL_ADMIN_PROTOCOL.md` §2.1/§2.2 里用于 pipe 名哈希与对端比对的那个标识。
pub fn current_user_sid() -> io::Result<String> {
    // `GetCurrentProcess` 返回伪句柄（值 `-1`）：由内核维护，使用后**不需要也不允许**关闭。
    let process = unsafe { GetCurrentProcess() };
    let mut token: HANDLE = ptr::null_mut();
    // Safety: `process` 是 `GetCurrentProcess` 的伪句柄，始终指向本进程、无需关闭；
    // `token` 是有效的可写输出槽；成功时得到本进程独占拥有的令牌句柄，交给 `OwnedHandle` 关闭。
    let opened = unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) };
    if opened == 0 {
        return Err(io::Error::last_os_error());
    }
    let token = OwnedHandle::new(token);
    token_user_sid(token.raw())
}

/// 已连接 pipe 对端的用户 SID（`S-1-5-21-…`）。
///
/// 取对端进程（`GetNamedPipeClientProcessId`）的进程令牌用户。**失败即失败关闭**：连接尚未建立、
/// 对端进程已退出、对端属于另一个用户而本进程无权查询其令牌，都返回 `Err`；调用方
/// （`server::transport::local`）必须把它当作「凭据不一致」处理——不发任何 frame、立即关闭连接。
pub fn client_user_sid(server: &NamedPipeServer) -> io::Result<String> {
    let mut pid: u32 = 0;
    // Safety: `server.as_raw_handle()` 借用自 `server`，在本次调用期间句柄保持有效；
    // `pid` 是有效的可写输出槽。
    let ok =
        unsafe { GetNamedPipeClientProcessId(server.as_raw_handle().cast::<c_void>(), &mut pid) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    process_user_sid(pid)
}

/// 以给定 SID 生成「仅该用户」的安全描述符并创建 pipe 的首个实例。
///
/// 单独拆出来是为了让用例能用非法/其它 SID 直接验 SDDL 到安全描述符这一步的错误分类。
fn create_pipe_server_for_sid(pipe_name: &str, user_sid: &str) -> io::Result<NamedPipeServer> {
    let name = to_wide(pipe_name)?;
    let sddl = to_wide(&owner_only_sddl(user_sid))?;
    let descriptor = security_descriptor_from_sddl(&sddl)?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0,
        // 句柄不继承：Agent 子进程不应拿到本地管理通道的句柄。
        bInheritHandle: FALSE,
    };
    // Safety:
    // * `name` 是 NUL 结尾的 UTF-16 缓冲区，由局部变量持有，调用期间存活；
    // * `attributes` 在调用期间存活，其 `lpSecurityDescriptor` 指向活的自相对安全描述符
    //   （由 `descriptor` 守卫持有，本函数返回时才释放，而 Win32 在调用内已复制该描述符）；
    // * 返回 `INVALID_HANDLE_VALUE` 表示失败，其余返回值是本次调用新建、尚无其它所有者的句柄。
    let handle = unsafe {
        CreateNamedPipeW(
            name.as_ptr(),
            OPEN_MODE,
            PIPE_MODE,
            PIPE_UNLIMITED_INSTANCES,
            PIPE_BUFFER_SIZE,
            PIPE_BUFFER_SIZE,
            PIPE_TIMEOUT,
            &attributes,
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // Safety: `handle` 由上面的 `CreateNamedPipeW` 刚创建成功，尚无其它所有者。
    // tokio 的 `from_raw_handle` **无条件**接管句柄所有权：它先把句柄交给内部 mio `NamedPipe`，再用
    // `PollEvented::new` 把它注册进 I/O driver。成功时由返回的服务器对象关闭；返回 `Err` 或 panic
    // （runtime 之外、reactor 缺失）时，句柄已随 mio `NamedPipe` 的 `Drop` 关闭。因此这里在任何分支上
    // 都不能再关闭句柄——重复关闭可能关掉已被复用的句柄值（对应用例：
    // `runtime_requirement_holds_without_leaking_the_pipe_instance`）。
    unsafe { NamedPipeServer::from_raw_handle(handle as RawHandle) }
}

/// 只授权给定用户的 SDDL 字符串。
///
/// `D:P` = 只含显式 DACL 且置 `SE_DACL_PROTECTED`（不从父容器继承 ACE）；`A;;GA;;;<sid>` = 单条允许 ACE，
/// 把完全访问（`GA`）给这一个 SID。**不含** `WD`（Everyone）、`BU`（Builtin Users）、`AU`（Authenticated
/// Users）等组主体，因此其它 OS 用户既不会因默认 DACL 被放行，也没有可继承的 ACE 可用。
fn owner_only_sddl(user_sid: &str) -> String {
    format!("D:P(A;;GA;;;{user_sid})")
}

/// `&str` → NUL 结尾的 UTF-16 缓冲区。
///
/// 内嵌 NUL 必须拒绝：Win32 只看到第一个 NUL 之前的部分，等于把名字静默截断成另一个名字
/// （可能落到另一个合法 pipe 名上），所以这不是「无害的宽松」，而是必须失败关闭的输入错误。
fn to_wide(value: &str) -> io::Result<Vec<u16>> {
    if value.contains('\0') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "字符串含内嵌 NUL",
        ));
    }
    let mut wide: Vec<u16> = value.encode_utf16().collect();
    wide.push(0);
    Ok(wide)
}

/// SDDL 字符串 → 自相对安全描述符（`LocalAlloc` 内存，由 [`LocalAlloc`] 守卫释放）。
fn security_descriptor_from_sddl(sddl: &[u16]) -> io::Result<LocalAlloc> {
    let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
    // Safety:
    // * `sddl.as_ptr()` 指向 NUL 结尾的 UTF-16 缓冲区，由调用方持有、调用期间存活；
    // * `descriptor` 是有效的可写输出槽；第四个参数（所需长度）按 MSDN 允许为 null；
    // * 失败（返回 0）时不写入输出槽，也就没有需要释放的内存。
    let ok = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    if descriptor.is_null() {
        // 声称成功却不返回描述符：按失败关闭，而不是拿 null 去创建 pipe（那会退回系统默认 DACL）。
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "SDDL 未转换为安全描述符",
        ));
    }
    Ok(LocalAlloc(descriptor))
}

/// 取指定进程的用户 SID 字符串。
fn process_user_sid(pid: u32) -> io::Result<String> {
    // `PROCESS_QUERY_LIMITED_INFORMATION` 是读取进程令牌所需的最小权限（MSDN 推荐它替代
    // `PROCESS_QUERY_INFORMATION`）；对端属于另一个用户时 `OpenProcess` 返回 null，调用方据此失败关闭。
    // Safety: 只需进程 id；失败返回 null，不产生需要关闭的句柄。
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid) };
    if process.is_null() {
        return Err(io::Error::last_os_error());
    }
    let process = OwnedHandle::new(process);
    let mut token: HANDLE = ptr::null_mut();
    // Safety: `process.raw()` 是本值独占拥有的、已打开的进程句柄（带查询令牌所需的访问权限）；
    // `token` 是有效的可写输出槽，成功时得到由 `OwnedHandle` 关闭的令牌句柄。
    let opened = unsafe { OpenProcessToken(process.raw(), TOKEN_QUERY, &mut token) };
    if opened == 0 {
        return Err(io::Error::last_os_error());
    }
    let token = OwnedHandle::new(token);
    token_user_sid(token.raw())
}

/// 令牌的用户 SID 字符串。
fn token_user_sid(token: HANDLE) -> io::Result<String> {
    let mut needed: u32 = 0;
    // 第一次以 null/0 调用是 MSDN 规定的取长度方式：它返回 0 并把所需字节数写入 `needed`。
    // Safety: `token` 是活的、带 `TOKEN_QUERY` 的令牌句柄；缓冲区为 null 时 `GetTokenInformation`
    // 只写 `needed`，不写任何缓冲区。
    unsafe { GetTokenInformation(token, TokenUser, ptr::null_mut(), 0, &mut needed) };
    if needed == 0 {
        return Err(io::Error::last_os_error());
    }
    if (needed as usize) < size_of::<TOKEN_USER>() {
        // 后面对缓冲区的 `read_unaligned::<TOKEN_USER>()` 以「至少一个 `TOKEN_USER`」为前提，
        // 这里先挡住长度不足的情况，而不是依赖 OS 的承诺。
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "令牌缓冲区小于 TOKEN_USER",
        ));
    }
    let mut buffer: Vec<u8> = vec![0; needed as usize];
    // Safety: `token` 同上；`buffer` 至少有 `needed` 字节且可写，`needed` 是上一步取得的所需长度。
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast::<c_void>(),
            needed,
            &mut needed,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    // Safety: `buffer` 已被 `GetTokenInformation` 按 `TokenUser` 布局填充，长度不小于
    // `size_of::<TOKEN_USER>()`（上面已判）；`read_unaligned` 不假设 `Vec<u8>` 的首地址满足
    // `TOKEN_USER` 的对齐要求。`User.Sid` 指向 `buffer` 内部的 SID 字节，`buffer` 在本函数返回前不会被回收。
    let token_user = unsafe { ptr::read_unaligned(buffer.as_ptr().cast::<TOKEN_USER>()) };
    sid_to_string(token_user.User.Sid)
}

/// SID → 字符串（`S-1-5-…`）。
fn sid_to_string(sid: PSID) -> io::Result<String> {
    if sid.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "令牌没有用户 SID",
        ));
    }
    let mut raw: PWSTR = ptr::null_mut();
    // Safety: `sid` 指向活的自相对 SID（调用方保证其内存块在本次调用期间存活）；
    // `raw` 是有效的可写输出槽，失败（返回 0）时不写入它。
    let ok = unsafe { ConvertSidToStringSidW(sid, &mut raw) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    if raw.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "SID 未转换为字符串",
        ));
    }
    // `ConvertSidToStringSidW` 的输出是 `LocalAlloc` 内存；守卫持有所有权，因此下面任何提前返回
    // （含 UTF-16 非法）都不会泄漏，也不会重复释放。
    let _allocated = LocalAlloc(raw.cast::<c_void>());
    let mut len: usize = 0;
    // Safety: `raw` 指向 NUL 结尾的 UTF-16 缓冲区（`_allocated` 保证其在作用域内不被释放），
    // 从 0 递增读到 NUL 为止，不会越界。
    while unsafe { *raw.add(len) } != 0 {
        len += 1;
    }
    // Safety: `raw..raw+len` 是上面刚确认存在的 `len` 个 UTF-16 码元；缓冲区由 `_allocated` 持有，
    // 在本函数返回前保持存活。
    let units = unsafe { std::slice::from_raw_parts(raw, len) };
    String::from_utf16(units)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "SID 不是合法 UTF-16"))
}

/// 独占拥有一个需要 `CloseHandle` 释放的 Windows 句柄，在作用域结束时恰好关闭一次。
///
/// 用它包住 `OpenProcessToken` / `OpenProcess` 的返回值，这样「提前返回」的每条错误路径都自动释放，
/// 不会出现某一条 `?` 上漏掉 `CloseHandle` 的形态。
struct OwnedHandle(HANDLE);

impl OwnedHandle {
    fn new(handle: HANDLE) -> Self {
        Self(handle)
    }

    fn raw(&self) -> HANDLE {
        self.0
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // Safety: `self.0` 是本值独占拥有的、已打开的句柄（只由 `OpenProcess`/`OpenProcessToken` 产生），
        // 没有第二处关闭它；`CloseHandle` 是这类句柄的文档化释放路径。
        unsafe { CloseHandle(self.0) };
    }
}

/// 独占拥有一段 `LocalAlloc` 内存（SDDL 转换与 SID 转字符串的输出），在作用域结束时恰好 `LocalFree` 一次。
struct LocalAlloc(*mut c_void);

impl Drop for LocalAlloc {
    fn drop(&mut self) {
        // Safety: `self.0` 来自 `ConvertStringSecurityDescriptorToSecurityDescriptorW` 或
        // `ConvertSidToStringSidW` 的 `LocalAlloc` 内存，没有第二处释放它；`LocalFree` 是这两个 API
        // 文档化的释放路径，返回值（释放后的指针或 null）无需检查。
        unsafe { LocalFree(self.0 as HLOCAL) };
    }
}

#[cfg(test)]
mod tests;
