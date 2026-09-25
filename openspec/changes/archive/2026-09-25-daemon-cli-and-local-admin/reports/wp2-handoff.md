# coder 报告 · WP2（`vendor/windows-local-ipc` 自研 Win32 FFI wrapper，交付单元 DU1）

- task_id: `2.4`、`2.5`、`2.6`（工作包 WP2）
- role: coder
- phase: implement
- stage: work-package
- agent_context: 独立子 Agent（worker / coder 角色），主 worktree 的分支 `feat/daemon-cli-and-local-admin`；只继承本任务的契约输入（`design.md` 决策 3、`docs/LOCAL_ADMIN_PROTOCOL.md` §2.1/§2.2、`docs/MODULE_ARCHITECTURE.md` §3.1/§5、`docs/SECURITY_DESIGN.md` §20、根 `Cargo.toml` 的 `workspace.exclude`、`AGENTS.md` §7/§8）与仓库现状，不继承规划阶段与 WP1 的实现对话；本变更各波次串行（W1 只有一个写入者），因此没有并发写者，也未使用独立 `CARGO_TARGET_DIR`
- target_revision: `6361f5d2a34d9f6306f62b41c852749e7d86bc4d`（WP2 的代码提交；本报告由后续的报告提交收录，其父提交即 target_revision）
- base_revision: `7c6aea2030a4108859dd3089e55e9371ae3817af`（WP1 交付点，任务单给定的起点；开工前已用 `git log --oneline -3` 核对）
- scope（写入范围，逐字取自任务单）：`vendor/windows-local-ipc/**`（新建）、根 `.gitignore`（仅新增 vendor 相关条目）、`openspec/changes/daemon-cli-and-local-admin/reports/`。**未改**任何 workspace 成员的源码与 manifest、根 `Cargo.toml`/`Cargo.lock`、`deny.toml`，也未改规划文件（`plan.md`/`tasks.md`/`verification.md` 保持主 Agent 写入）
- dependencies（上游输入）：`design.md` 决策 3（FFI 边界定案）与决策 1；`docs/LOCAL_ADMIN_PROTOCOL.md` §2.1/§2.2；`docs/SECURITY_DESIGN.md` §20；`docs/MODULE_ARCHITECTURE.md` §3.1/§5；根 `Cargo.toml` 的 `workspace.exclude`；`AGENTS.md` §7/§8
- result: `PASS`（本工作包的检查与交付条件全部满足；**不代表**独立 review、集成或合并已完成）
- issues: 见「已知限制与偏差」与「未执行项与待主 Agent 决策」——4 条需要主 Agent 知悉的记录（plan 措辞与任务单输入的口径差异、`design.md` 决策 3 的一句事实前提在锁定版 tokio 上已过时、tokio 在 runtime 之外是 panic 而非 `Err`、`cargo-deny`/`gitleaks` 与跨用户拒绝在本机不可得）
- evidence_paths: `openspec/changes/daemon-cli-and-local-admin/reports/pv5-windows-ipc.log`（[PV5] WP2 段原始日志）、`openspec/changes/daemon-cli-and-local-admin/reports/du1-pv1.log`（[PV1] 的 WP2 轮，追加在 W0 轮之后未覆盖）、本报告

## 提交与交付对应

| 提交 | 类型 | 内容 | 覆盖任务 |
| --- | --- | --- | --- |
| `6361f5d2a34d9f6306f62b41c852749e7d86bc4d` | `feat(server)` | `vendor/windows-local-ipc/**`（4 个文件，Cargo.lock/target 按 `.gitignore` 排除）与 `.gitignore`（2 条 vendor 条目） | 2.4、2.5 |
| 本报告提交 | `docs(server)` | `reports/wp2-handoff.md`（本文件） | 2.4/2.5/2.6 的交接 |

`git status --porcelain` 在代码提交前只有：`.gitignore`、`vendor/`（探针与临时文件已删）与主 Agent 自己维护的 `openspec/changes/daemon-cli-and-local-admin/tasks.md`、`verification.md`（**未触碰、未暂存**）。提交后工作区只剩那两个主 Agent 文件的未暂存改动；`reports/*.log` 按仓库约定被 `.gitignore` 排除，不入库。

## 冻结的公开 API（WP3 只依赖这三个签名）

```rust
// 仅在 cfg(windows) 下存在；非 Windows 平台上本 crate 不导出任何项（见「非 Windows 面」）。
pub fn create_pipe_server(pipe_name: &str) -> std::io::Result<tokio::net::windows::named_pipe::NamedPipeServer>;
pub fn current_user_sid() -> std::io::Result<String>;
pub fn client_user_sid(server: &tokio::net::windows::named_pipe::NamedPipeServer) -> std::io::Result<String>;
```

契约要点（与 `docs/LOCAL_ADMIN_PROTOCOL.md` §2.1/§2.2 对齐）：

- `pipe_name` 是**完整 pipe 路径**（`\\.\pipe\<name>`），命名空间错误由 OS 拒绝（`ERROR_INVALID_NAME`）；内嵌 NUL 由本 crate 拒绝（`ErrorKind::InvalidInput`）。
- `create_pipe_server` 建的是该名字的**首个实例**，带 `FILE_FLAG_FIRST_PIPE_INSTANCE`：同名第二次创建失败（`ERROR_ACCESS_DENIED`），因此「同一 endpoint 起两次」是明确错误而不是静默多开实例。
- `current_user_sid` 与 `client_user_sid` 返回同一空间的值（`S-1-5-21-…` 文本），可直接逐字比较；前者是本进程令牌的用户 SID，后者是 pipe 对端进程令牌的用户 SID。
- 错误类型只有 `std::io::Result`：Win32 失败保留 `raw_os_error`，本 crate 自判的输入错误用 `InvalidInput`/`InvalidData`。不暴露原始 `HANDLE`、不暴露 `SECURITY_ATTRIBUTES`、不接受调用方传入 SDDL。
- 调用点必须在已开启 I/O 的 Tokio runtime 内（`create_pipe_server`）；这与 tokio 自己的 `ServerOptions::create` 是同一条约束，见下「已知限制与偏差」第 3 条。

## 任务 2.4（实现）

| 文件 | 内容 |
| --- | --- |
| `vendor/windows-local-ipc/Cargo.toml` | `name = "windows-local-ipc"`、`version = "0.0.0"`、`edition = "2024"`、`rust-version = "1.85"`、显式 `license = "MIT OR Apache-2.0"`、`publish = false`（**不继承 `[workspace.package]`**：它不是成员）；`[target.'cfg(windows)'.dependencies]` = `tokio`（`net`）+ `windows-sys 0.61`（6 个 feature，逐个对应实际调用）；dev-dependencies 再加 `rt`/`macros`/`io-util`（只用于自身测试，不进依赖方闭包）；`[lints.clippy] all = "deny"` 与 workspace 同门禁 |
| `vendor/windows-local-ipc/src/lib.rs` | crate 文档说明「这是全仓库唯一允许 `unsafe` 的位置及原因」（`unsafe_code = "forbid"` + §2.2 必须有 Win32 FFI + 以 path 依赖存在、不继承 workspace lint），crate 级 `#![allow(unsafe_code)]`，`cfg(windows)` 门控模块与导出；非 Windows 只留文档 |
| `vendor/windows-local-ipc/src/windows.rs` | FFI 实现：`create_pipe_server` / `current_user_sid` / `client_user_sid` 与私有辅助（SDDL 生成、宽字符串、安全描述符转换、SID 转字符串），两个带 `Drop` 的资源守卫 |
| `vendor/windows-local-ipc/src/windows/tests.rs` | 11 个 `cfg(windows)` 用例（见任务 2.5） |

实现要点：

1. **SDDL 与安全属性**：`owner_only_sddl(sid) = "D:P(A;;GA;;;<sid>)"`——`D:P` 置 `SE_DACL_PROTECTED`（不继承），单条 ACE 只授权当前用户；`ConvertStringSecurityDescriptorToSecurityDescriptorW` 转成自相对描述符，塞进 `SECURITY_ATTRIBUTES`（`bInheritHandle = FALSE`，句柄不继承给 Agent 子进程）后调 `CreateNamedPipeW`。
2. **创建参数与 tokio 对齐**：`dwOpenMode = PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE | FILE_FLAG_OVERLAPPED`、`dwPipeMode = PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS`、`nMaxInstances = PIPE_UNLIMITED_INSTANCES`、in/out 缓冲各 65536、`nDefaultTimeOut = 0`。前三者之外的数值刻意取 tokio `ServerOptions::new()` 的默认值：WP3 用 tokio safe API 创建**后续实例**时必须与之相同（同名实例的参数必须一致，否则后续实例创建会被 OS 拒绝）。`PIPE_REJECT_REMOTE_CLIENTS` 与「本通道不进入远程攻击面」的口径一致。
3. **包装为 tokio 类型**：`NamedPipeServer::from_raw_handle`（unsafe）在 crate 内部调用，句柄所有权无条件转移给 tokio/mio（成功由服务器对象关闭；返回 `Err` 或 panic 时 mio 的 `Drop` 已关闭句柄），因此本 crate 在转移之后再关句柄会重复关闭——不变量写在该 unsafe 块的注释里，并由用例 ⑨ 实证（panic 之后同名首实例仍可创建，等于没有泄漏）。
4. **`current_user_sid`**：`GetCurrentProcess`（伪句柄，不关闭）→ `OpenProcessToken(TOKEN_QUERY)` → 两段式 `GetTokenInformation(TokenUser)`（先取长度再填缓冲）→ `ConvertSidToStringSidW` → `String::from_utf16`。缓冲区按 `needed` 字节分配，`TOKEN_USER` 用 `read_unaligned` 读取（`Vec<u8>` 不保证 8 字节对齐），并先断言 `needed >= size_of::<TOKEN_USER>()`。
5. **`client_user_sid`**：`GetNamedPipeClientProcessId` → `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` → `OpenProcessToken` → 同上取 SID。**失败即失败关闭**：未连接、对端已退出、对端属另一用户而无权查询，都返回 `Err`，由 WP3 当作「凭据不一致」处理。
6. **资源释放**：`OwnedHandle`（`CloseHandle`）与 `LocalAlloc`（`LocalFree`）两个守卫覆盖令牌/进程句柄与 SDDL/SID 的 `LocalAlloc` 内存，因此每条提前返回路径都恰好释放一次；正常路径无 `unwrap()`/`expect()`/`panic!`。
7. **`deny.toml` 复核（只读）**：本 WP 未改 `deny.toml`，但核对过 WP1 的登记仍成立——`[licenses] allow` 覆盖本 crate 与新增传递依赖的全部许可证（tokio/mio/bytes/pin-project-lite/socket2/windows-sys/windows-link 均 MIT 或 MIT OR Apache-2.0，`cargo tree --locked` 逐项可见）；`[bans] allow-wildcard-paths` 覆盖 path 依赖的无版本写法；判定仍只在 CI 生效（本地无 `cargo-deny`）。

## 任务 2.5（用例清单与断言）

原始输出：`reports/pv5-windows-ipc.log` 的 (2) 节（`cargo test -- --nocapture`，退出码 0，`11 passed; 0 failed; 0 ignored`；另有一个 0 用例的 doc-test 目标，本 crate 没有文档示例）。

| # | 用例 | 断言 |
| --- | --- | --- |
| ① | `owner_only_pipe_accepts_current_user_client` | 以 SDDL 创建的 pipe，当前用户进程用 tokio 客户端连接成功，并完成 4 字节往返（证明句柄可重叠、已注册进 IOCP） |
| ② | `client_user_sid_matches_current_user_sid` | `client_user_sid(&server) == current_user_sid()`，且值以 `S-1-` 开头（两层访问控制的输入一致） |
| ③ | `owner_only_sddl_grants_only_current_user` | **静态证据**：SDDL 逐字等于 `D:P(A;;GA;;;<当前用户 SID>)`；恰好一条 ACE；恰好一个 `S-1-` 主体；不含 `WD`/`BU`/`AU`/`AN`/`BG`/`IU`/`SU`/`SY`/`BA`/`CO` 任何组/世界主体 |
| ④ | `invalid_sddl_is_rejected` | 非法 SID 的 SDDL → `Err`，`raw_os_error = 1336`（`ERROR_INVALID_ACL`；同族的 1338 `ERROR_INVALID_SID` 也接受） |
| ⑤ | `pipe_name_with_inner_nul_is_rejected` | 含内嵌 NUL 的 pipe 名 → `ErrorKind::InvalidInput`（不静默截断成另一个名字） |
| ⑥ | `duplicate_first_instance_is_rejected` | 同名第二个首实例 → `Err`，`raw_os_error = 5`（`ERROR_ACCESS_DENIED`，证明 `FILE_FLAG_FIRST_PIPE_INSTANCE` 生效） |
| ⑦ | `pipe_name_outside_pipe_namespace_is_rejected` | 缺 `\\.\pipe\` 前缀 → `Err`，`raw_os_error = 123`（`ERROR_INVALID_NAME`） |
| ⑧ | `client_user_sid_requires_a_connected_client` | 未连接的对端 → `Err`，`raw_os_error = 1168`（`ERROR_NOT_FOUND`），不给出「看起来像」的 SID |
| ⑨ | `runtime_requirement_holds_without_leaking_the_pipe_instance` | runtime 之外调用 `create_pipe_server` → panic 且消息含 `no reactor running`；随后在同一名字上重新创建成功 ⇒ **panic 路径没有泄漏已创建的 pipe 实例**（日志里那行 panic 输出是本用例的预期产物，判据是 `test result` 行） |
| ⑩ | `distinct_pipe_names_can_be_created_concurrently` | 同进程内两个不同 pipe 名各自创建成功（首实例约束只针对同名） |
| ⑪ | `subsequent_instance_via_tokio_safe_api_is_allowed` | wrapper 建首实例后，`ServerOptions::new().create(name)`（WP3 的后续实例路径）能创建并完成连接 ⇒ 两处 `nMaxInstances`/缓冲参数一致 |

未发现或跳过的用例：无（`0 failed`、`0 ignored`、无 `#[ignore]`）。用例命名空间隔离：每个用例用 `进程 id + 递增序号 + 纳秒` 组成独占 pipe 名（Named Pipe 命名空间是本机共享资源，计划要求 [PV5] 串行执行）。

### 非 Windows 面（结构性证据）

`src/lib.rs` 用 `#[cfg(windows)]` 门控模块与导出，因此非 Windows 上本 crate 是**空 crate**、零依赖：

| 证据 | 命令 | 结果 |
| --- | --- | --- |
| 目标编译 | `cargo check --target x86_64-unknown-linux-gnu` | 退出码 0，且只编译 `windows-local-ipc` 自身（`tokio`/`windows-sys` 未参与——依赖写在 `[target.'cfg(windows)'.dependencies]`） |
| 负向编译探针 | `cargo check --target x86_64-unknown-linux-gnu --example linux_surface_probe`（探针引用三个公开函数，运行后即删） | 退出码 101，`E0425` × 3「not found in `windows_local_ipc`」并指出 `#[cfg(windows)]` 是门控点 ⇒ 公开面在非 Windows 上不存在 |
| 机器视图 | `cargo metadata --no-deps`（crate 目录） | `edition=2024`、`rust_version=1.85`、`license=MIT OR Apache-2.0`、`publish=[]`、依赖两条均 `target="cfg(windows)"`；`workspace_members` 只有它自己（被根 workspace 排除） |
| 机器视图 | `cargo metadata --no-deps`（仓库根） | 成员数 10，`vendor/windows-local-ipc` **不在**成员中 |

诚实说明：Linux CI 不直接编译本 crate（它不是 workspace 成员、`npm run verify` 的 `--workspace` 不覆盖它），上面的 Linux 目标 check 是本 WP 在本机做的交叉校验，不是「CI 会覆盖」的证据。

## 任务 2.6（WP2 交付前局部验证）

| Check ID | 命令（cwd） | 配置/环境 | 退出码 | 日志 | 结论 |
| --- | --- | --- | --- | --- | --- |
| PV5（WP2 段） | `cargo build` 与 `cargo test -- --nocapture`（`vendor\windows-local-ipc`） | Windows x64 / cargo 1.98.1（`rust-toolchain.toml` 固定）/ 该目录独立 `Cargo.lock` | 0 / 0 | `reports/pv5-windows-ipc.log` (1)(2) | PASS：11 用例全过、无零用例目标（除既有形态的空 doc-test 目标）、无跳过 |
| PV5（复现轮） | `cargo test --locked`、`cargo tree --locked` | 同上，`--locked` 证明版本由本目录 `Cargo.lock` 固定（tokio 1.53.1 / windows-sys 0.61.2，与根 `Cargo.lock` 同版本族） | 0 / 0 | `reports/pv5-windows-ipc.log` (9) | PASS |
| PV1（WP2 轮） | `npm run check`（仓库根） | Node v24.19.0 / npm 12.0.2；离线 | 0 | `reports/du1-pv1.log`（追加在 W0 轮之后，另有一次提交后复核轮） | PASS：10 道合同门禁全绿（含 `crate boundaries OK: 10 个 crate`、`doc links OK: 374 relative links`、agentic 13/13） |
| LC1（附加） | `cargo check --target x86_64-unknown-linux-gnu` + 负向探针 | 已安装 `x86_64-unknown-linux-gnu` std | 0 / 101（预期失败） | `reports/pv5-windows-ipc.log` (4)(5) | PASS：非 Windows 面为空（见上表） |
| LC2（附加） | `cargo fmt -- --check`、`cargo clippy --all-targets -- -D warnings`（crate 目录） | 默认 rustfmt 配置（仓库无 `rustfmt.toml`）；clippy 走该 crate 的 `[lints.clippy] all = "deny"` | 0 / 0 | `reports/pv5-windows-ipc.log` (3) | PASS：格式与静态检查零告警（该 crate 不在 `--workspace` 内，故此两条必须单独跑） |
| LC3（附加） | `rg -n "unsafe" crates/` 与 `rg -n "unsafe[[:space:]]*(\{|fn\|impl\|extern\|trait)" crates/`（仓库根） | ripgrep | 0（6 处命中）/ 1（零命中） | `reports/pv5-windows-ipc.log` (8) | **见下「自检口径」**：字符串层面 6 处命中全部是注释与 manifest 注释里的散文（引用 `unsafe_code = "forbid"`），**代码层面零命中** |

自检口径（如实记录，不用「零命中」掩盖）：任务单与 `plan.md` 的 Local Checks 期望 `rg -n "unsafe" crates/` 零命中，但该字符串在 `crates/agent-host/src/platform.rs`、`crates/agent-host/Cargo.toml`、`crates/storage-sqlite/src/migrate.rs` 的**注释**里各出现若干次（WP1 之前就存在，且都在本 WP 写范围之外，不得为过检查而改）。因此本 WP 报告两条命令的原始输出：字符串层面 6 处（全为注释），并用「unsafe 后跟 `{`/`fn`/`impl`/`extern`/`trait`」的精确判据证明 workspace crate 内**没有 unsafe 代码**（退出码 1 = 零命中）。对照：`vendor/windows-local-ipc/src/lib.rs` 4 处、`src/windows.rs` 17 处（含 `#![allow(unsafe_code)]` 与逐处不变量注释），`src/windows/tests.rs` 0 处。

## 对 WP3 的交接要点（接线时要知道的行为）

1. **endpoint 名与哈希输入**：`current_user_sid()` 的返回值就是 §2.1 要求的 `<userSidHashHex>` 输入（SID 字符串的 UTF-8 字节，不加换行）；`<instanceId>` 由单实例锁负责，本 crate 不参与。
2. **首实例 vs 后续实例**：首实例必须用本 crate 的 `create_pipe_server`（只有它能带 SDDL）；后续实例用 tokio `ServerOptions::new().create(name)`（用例 ⑪ 证明可行）——但**必须保持 tokio 的默认 `nMaxInstances` 与缓冲参数**（本 crate 首实例用的就是同一组值），否则 OS 会拒绝创建后续实例。
3. **两次创建同名的行为**：同名第二个「首实例」失败（`ERROR_ACCESS_DENIED`），可用于「同一 endpoint 已被占用」的失败关闭判定。
4. **对端凭据校验的接线**：`client_user_sid(&server)` 返回 `Err` 时就是「凭据不一致或无法确认」——按 §2.2 不发任何 frame、立即关闭并记 `authorization.denied`；不要把它降级成「允许连接」。
5. **runtime 前提**：在 `create_pipe_server` 之前确认已处于 Daemon 的 runtime（Daemon 主流程天然满足）；不要在 runtime 之外先建句柄再移交。
6. **本 crate 不提供**：pipe 读写、framing、channel 绑定、`FacadeAttachmentId`、审计——全部归 `server::transport::local`。

## 已知限制与偏差

1. **跨用户拒绝本轮无法真实执行（平台限制，如实记录）**：本机只有一个可用 OS 账号，因此「另一个用户连接被拒绝」这一条（§2.2 的第二层）无法在本机复现。本轮提供的是**静态证据**（用例 ③：生成的 DACL 只有当前用户一条 ACE、无组主体、不继承）与**失败关闭证据**（用例 ⑧：拿不到对端凭据即 `Err`）。真实跨用户拒绝属 [PV5] 在 WP3/WP4 的验收范围，届时若仍只有单一账号，同样只能记限制而不是 PASS。
2. **`plan.md` WP2 行与 `tasks.md` 2.4 的「非 Windows 运行期明确失败」**：本 WP 按任务单输入的明确口径实现为「非 Windows 编译为空 crate、不提供跨平台统一的失败签名，调用点由 `server` 侧 cfg 门控」。两者的差别是「运行期报错」与「编译期根本不存在」，后者更强也更简单（不会出现「忘了 cfg 但运行期才发现」）。**建议主 Agent** 在 WP3 接线后核对 `plan.md`/`tasks.md` 这句措辞是否要改成「编译期为空」；本 WP 不修改规划文件。证据见「非 Windows 面」小节的负向编译探针。
3. **`design.md` 决策 3 的一句事实前提在锁定版 tokio 上已过时**：决策 3 写「tokio 的 `ServerOptions` 不暴露 security attributes」，而锁定版 `tokio 1.53.1` 提供了 `ServerOptions::create_with_security_attributes_raw`（`unsafe`，接受 `*mut c_void` 的安全属性）。本实现**仍按批准方向**自己调 `CreateNamedPipeW`（这样 `dwOpenMode`/`dwPipeMode` 等参数不依赖 tokio 的内部约定，且 SDDL→安全属性这一段完全在本 crate 内可审）。结论不变（FFI 收敛在 wrapper、对外只暴露 safe API），只是这句话本身不再准确，WP3 与自己写 ACP/transport 代码时无需依赖它。
4. **runtime 之外是 panic 而不是 `Err`（与 tokio 同构）**：`NamedPipeServer::from_raw_handle` 在 reactor 缺失时 panic（tokio 的 doc 写的是「errors」，实测是 panic），tokio 自己的 `ServerOptions::create` 在 runtime 之外**同样 panic**（本 WP 用临时探针实测并记录在日志中，探针随后删除）。因此 `create_pipe_server` 的 rustdoc 写了 `# Panics` 段，并有用例 ⑨ 同时断言 panic 消息与「无句柄泄漏」。若主 Agent 希望公共 API 只返回 `Err`，代价是给本 crate 的 tokio 依赖增加 `rt` feature 以调用 `Handle::try_current` 预检——那会偏离任务单给定的 feature 面（`net`），故本 WP 未擅自加。
5. **同名后续实例的 DACL 归属未验证（转交 WP3）**：`CreateNamedPipeW` 的 ACL 在首实例创建时确定，本 WP 只验证了「后续实例能创建、能连接」（用例 ⑪），**没有**验证后续实例是否继承首实例的限制性 DACL（需要 `GetSecurityInfo` 一类额外 FFI，超出本 WP 的 API 面）。WP3 落 endpoint 时应在本机 [PV5] 里把这条钉死（跨用户拒绝用例或查询 DACL），否则「首实例带 SDDL」这个安全论证会有一个未证的环节。
6. **本地不可执行的判定（如实登记，不算通过）**：`cargo-deny`（`deps`/`advisories` 的许可证、来源、advisory）与本 crate 的 `gitleaks` 扫描只在 CI 运行，本地无等价物；本 WP 只核对了许可证落在 `deny.toml` 的 allow 列表内（`cargo tree --locked` 逐项），没有执行判定。
7. **未新增/未放宽任何 lint 或检查**：该 crate 的 `[lints.clippy] all = "deny"` 与 workspace 同门禁；`#![allow(unsafe_code)]` 是**该 crate 唯一**的放宽，且是它在 workspace 之外存在的理由，未影响任何 workspace crate。

## 未执行项与待主 Agent 决策

- 独立 review RV1（WP2）尚未返回（`tasks.md` 3.4），本报告不把它写成 PASS。
- [PV3]/[PV4] 与本 WP 无关（属 WP3/WP4）；`npm run verify` 的 Rust 半本 WP 未跑（未改任何 workspace Rust 源码与成员，跑它只是重复 WP1 的证据，且计划把它放在各 WP 的 3.x/7.1）。若主 Agent 要求 WP2 也出 `npm run verify` 的全量证据，请在路由里指明（可复用 WP1 的 `reports/wp1-verify.log` 并注明适用性，或重跑）。
- `plan.md`/`tasks.md` 的措辞同步（偏差 2）与 `design.md` 决策 3 的事实前提（偏差 3）由主 Agent 决定是否改文档；本 WP 不写规划文件。

## checks（逐 Check ID 汇总）

| Check ID | 命令 / 目录 | 配置与环境 | 结果 | 日志 |
| --- | --- | --- | --- | --- |
| [PV5]（WP2 段） | `cargo build`；`cargo test -- --nocapture`；`cargo test --locked`；`cargo tree --locked` @ `vendor\windows-local-ipc` | Windows x64；cargo/rustc 1.98.1；独立 `Cargo.lock` | 退出码均为 0；11 passed / 0 failed / 0 ignored | `reports/pv5-windows-ipc.log` (1)(2)(9) |
| [PV1]（WP2 轮） | `npm run check` @ 仓库根 | Node v24.19.0 / npm 12.0.2；离线 | 退出码 0；10 道门禁逐项通过 | `reports/du1-pv1.log`（WP2 轮 + 提交后复核轮） |
| LC1 | `cargo check --target x86_64-unknown-linux-gnu`（+ 负向探针） @ `vendor\windows-local-ipc` | 已装 Linux std；探针已删 | 0 / 101（预期失败） | `reports/pv5-windows-ipc.log` (4)(5) |
| LC2 | `cargo fmt -- --check`；`cargo clippy --all-targets -- -D warnings` @ `vendor\windows-local-ipc` | 默认 rustfmt；`[lints.clippy] all = "deny"` | 退出码 0 / 0 | `reports/pv5-windows-ipc.log` (3) |
| LC3 | `rg -n "unsafe" crates/`；`rg -n "unsafe[[:space:]]*(\{|fn\|impl\|extern\|trait)" crates/` @ 仓库根 | ripgrep | 6 处（全为注释）/ 1（零命中） | `reports/pv5-windows-ipc.log` (8) |

## 资源释放

| 资源 | 归属 | 状态 |
| --- | --- | --- |
| 临时探针 `vendor/windows-local-ipc/tests/scratch_probe.rs`、`examples/linux_surface_probe.rs` | 本 WP | **已删除**（连同临时 `tests/`、`examples/` 目录），`git status -uall` 里不再出现 |
| `vendor/windows-local-ipc/target/`（172 MB）与 `vendor/windows-local-ipc/Cargo.lock` | 本 WP | 保留为构建缓存与版本固定（已在 `.gitignore` 登记，不入库）；WP3/WP4 的 [PV5] 轮次可直接复用 |
| 仓库根 `target/`（4.2 GB） | 共享 | 未清空（本 WP 只新增 vendor 侧独立 target；根 target 的清理归各执行者自行决定，避免清掉他人待用的缓存） |
| 本机 Named Pipe 实例 | 本 WP | 全部随用例句柄关闭而消失（用例 ⑨ 专门证明失败路径也不残留实例）；未创建任何后台进程、端口、临时数据目录或数据库 |
| pipe 名隔离 | 本 WP | 每个用例一个独占名字（进程 id + 序号 + 纳秒），满足计划「同一时刻只允许一轮 Windows IPC 用例」的约定 |

```yaml
handoff_index:
  - task_id: "2.4"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "6361f5d2a34d9f6306f62b41c852749e7d86bc4d"
    evidence_type: CHECK
    evidence_id: PV5
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp2-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "WP2 段：vendor/windows-local-ipc 自身测试在 Windows x64 本机执行（cargo build + cargo test -- --nocapture + cargo test --locked），11 passed/0 failed/0 ignored，覆盖 SDDL 创建、同用户连接、对端 SID 往返；工具链 cargo/rustc 1.98.1（rust-toolchain.toml）；日志 reports/pv5-windows-ipc.log 的 (1)(2)(9) 节。跨用户拒绝本机不可得，已按限制登记。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.5"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "6361f5d2a34d9f6306f62b41c852749e7d86bc4d"
    evidence_type: CHECK
    evidence_id: PV5
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp2-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "同一轮 [PV5] 日志被 2.4/2.5 两行分别引用（同一 Check ID 同一目标版本、同一原始日志）：本行核对的是负向与边界用例（非法 SDDL 1336、内嵌 NUL、重复首实例 5、非 pipe 命名空间 123、未连接对端 1168、runtime 之外 panic 且不泄漏实例、后续实例经 tokio safe API）；无零用例、无 skip。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.6"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "6361f5d2a34d9f6306f62b41c852749e7d86bc4d"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp2-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "WP2 轮：crate 成员集合未变（仍是 10 个，vendor crate 不在 members），npm run check 在仓库根执行、追加在 W0 轮之后且提交后又复核一次，10 道合同门禁全绿（含 check:boundaries 对 §5 新列的解析、check:doc-links 374 链接/219 文件）；Node v24.19.0 / npm 12.0.2；日志 reports/du1-pv1.log 的 WP2 两段。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.6"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "6361f5d2a34d9f6306f62b41c852749e7d86bc4d"
    evidence_type: VALIDATION
    evidence_id: LC1
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp2-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "本 WP 附加的非 Windows 结构性证据：cargo check --target x86_64-unknown-linux-gnu 退出码 0（只编译本 crate，依赖均 cfg(windows) 门控），负向探针 cargo check --example linux_surface_probe 退出码 101 并报 E0425（公开项被 cfg(windows) 门控掉），cargo metadata --no-deps 显示 edition/rust_version/license/publish 与 cfg(windows) 依赖。日志 reports/pv5-windows-ipc.log 的 (4)(5)(6)(7) 节。Linux CI 不直接编译本 crate，该证据来自本机交叉校验。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.6"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "6361f5d2a34d9f6306f62b41c852749e7d86bc4d"
    evidence_type: VALIDATION
    evidence_id: LC2
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp2-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "本 WP 附加的静态检查：cargo fmt -- --check 与 cargo clippy --all-targets -- -D warnings 在该 crate 目录执行（它不在 workspace，`cargo fmt --all`/`clippy --workspace` 不覆盖它），退出码 0/0；clippy 走该 crate 的 [lints.clippy] all = deny。日志 reports/pv5-windows-ipc.log 的 (3) 节。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.6"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "6361f5d2a34d9f6306f62b41c852749e7d86bc4d"
    evidence_type: VALIDATION
    evidence_id: LC3
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp2-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "本 WP 附加的 unsafe 边界自检：rg -n \"unsafe\" crates/ 在字符串层面有 6 处命中，逐条核对全部是既有注释/manifest 注释（引用 unsafe_code = \"forbid\" 的散文，位于本 WP 写范围外），精确判据 rg -n \"unsafe[[:space:]]*(\\{|fn|impl|extern|trait)\" crates/ 零命中（退出码 1）；对照 vendor crate 内 unsafe 全部落在本次新增文件并逐处附不变量注释。日志 reports/pv5-windows-ipc.log 的 (8) 节。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.6"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "6361f5d2a34d9f6306f62b41c852749e7d86bc4d"
    evidence_type: DELIVERY
    evidence_id: NOT_APPLICABLE
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp2-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "WP2 交付物：提交 6361f5d 的 5 个文件（vendor/windows-local-ipc 的 Cargo.toml + src/lib.rs + src/windows.rs + src/windows/tests.rs，以及 .gitignore 的 2 条 vendor 条目）；冻结的公开 API 就是本报告「冻结的公开 API」一节的三个签名，供 WP3 以 path 依赖消费。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.4"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: "6361f5d2a34d9f6306f62b41c852749e7d86bc4d"
    evidence_type: REVIEW
    evidence_id: RV1
    report_path: NOT_AVAILABLE
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "WP2 的交付前独立 review 尚未返回；必须由不继承实现对话的执行者按 roles/reviewer.md 对 6361f5d 检视（unsafe 收敛范围与不变量注释、safe API 语义与错误分类、非 Windows 失败关闭、公开面不暴露原始句柄）。待补 ID：RV1（tasks.md 3.4）；本报告不把自检当作其结论。"
    source_evidence: NOT_APPLICABLE
```
