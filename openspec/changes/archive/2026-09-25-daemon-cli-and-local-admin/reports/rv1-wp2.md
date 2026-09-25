# reviewer 报告 · RV1-WP2（vendor/windows-local-ipc FFI wrapper，交付前 branch review）

```
task_id:            3.4（RV1-WP2，重派；上一运行 ecc2d081 无报告）
role:               reviewer
phase:              work-package review（工作包交付前）
stage:              work-package
agent_context:      独立子 Agent（reviewer），不继承实现/规划对话；只读，未修改任何文件
                    工具集仅 read/grep/find/ls/watchdog_diff，无 shell/git 写权限 ⇒ 无法执行 `git diff`
target_revision:    6361f5d2a34d9f6306f62b41c852749e7d86bc4d
base_revision:      7c6aea2030a4108859dd3089e55e9371ae3817af
scope:              vendor/windows-local-ipc/Cargo.toml、src/lib.rs、src/windows.rs、
                    src/windows/tests.rs、根 .gitignore（vendor 条目）
checks:             V0 版本识别、V1–V5 独立复核、E1–E3 证据核对（见「Review Context」）
issues:             RV1-WP2-F1/F2/F3，全部 MINOR（P2，report-only）；无 P0/P1
result:             PASS
evidence_paths:     vendor/windows-local-ipc/**、.gitignore、Cargo.toml（根）、Cargo.lock、.git/logs/HEAD、
                    reports/{wp2-handoff.md,pv5-windows-ipc.log,du1-pv1.log}、verification.md、
                    docs/LOCAL_ADMIN_PROTOCOL.md §2.1/§2.2、docs/MODULE_ARCHITECTURE.md §3.1/§5、
                    docs/SECURITY_DESIGN.md §20、design.md 决策 3、
                    cargo registry 源码：tokio-1.53.1（net/windows/named_pipe.rs、io/poll_evented.rs）、
                    mio-1.2.3（sys/windows/{named_pipe,handle}.rs）、
                    windows-sys-0.61.2（Win32/{Foundation,Security,Security/Authorization,
                    Storage/FileSystem,System/Pipes,System/Threading}/mod.rs）
resource_cleanup:   未创建任何资源；未运行命令
```

## Review Context

### 版本识别（含限制声明）

- 无 shell/git 权限，无法执行 `git diff 7c6aea2..6361f5d`，也读不到 git 对象（zlib 压缩）。替代方法：
  1. **完整 reflog 已读**（`.git/logs/HEAD` 为明文）：目标 `6361f5d` 存在；其后提交依次为 `d9c7c6e`（WP2 报告）、`2636dbe`、`c12957e`（WP3a server crate）、`7449868e`→`3390e9c`（WP3a 报告）、`51dc6b1`、`f1d7e6a`。**没有任何提交声明改动 `vendor/windows-local-ipc/**`**；WP3a 报告自述写范围明确排除 `vendor/`。
  2. **内容与目标版本的实测记录逐点一致**：`wp2-handoff.md` 冻结点与工作区内容完全吻合——3 个公开函数签名、11 个用例名（`src/windows/tests.rs:50/71/90/108/126/135/149/162/179/214/229` 与 `pv5-windows-ipc.log` (2) 节逐一对应）、6 个 windows-sys feature、`src/lib.rs` 4 处 / `src/windows.rs` 17 处 `unsafe` 字符串（grep 独立复算一致）、`OPEN_MODE/PIPE_MODE/PIPE_BUFFER_SIZE/PIPE_TIMEOUT` 取值（`src/windows.rs:45/48/54/57`）。
  3. `watchdog_diff` 报告工作树相对 launch HEAD（`51dc6b1`）无变化。
- 限制：以上是「内容指纹 + 提交链消息」证据，非字节级 diff；若其后有提交静默改动 `vendor/**`，本报告适用性失效（见 Residual Risks 3）。

### 契约输入

`design.md` 决策 3；`docs/LOCAL_ADMIN_PROTOCOL.md` §2.1/§2.2；`docs/SECURITY_DESIGN.md` §20；`docs/MODULE_ARCHITECTURE.md` §3.1/§5；`AGENTS.md` §3/§7。

## Findings

### RV1-WP2-F1 — MINOR（P2，report-only，注释事实错误）

- 位置：`vendor/windows-local-ipc/Cargo.toml:32`。
- 内容：注释写「版本与 workspace lock 中已有的 `windows-sys 0.61.2` 同族（`fs4`/`clap` 已引入该版本族）」。
- 实际：`Cargo.lock` 中**不存在** `fs4`/`clap`/`toml`（`grep` 零命中）——它们只登记在根 `Cargo.toml` 的 `[workspace.dependencies]`，尚无成员消费，故不入 lock。0.61.2 实际由 `mio 1.2.3`（`Cargo.lock:919-922`）与 `tokio 1.53.1`（`Cargo.lock:1766-1769`）引入，另本 crate 自身（`Cargo.lock:2092-2095`）。
- 影响：仅注释；结论「不新增版本族」仍成立。
- 最小修复：括号改为实际来源（如「`mio`/`tokio` 已引入该版本族」）。

### RV1-WP2-F2 — MINOR（P2，report-only，许可证元数据与仓库不一致）

- 位置：`vendor/windows-local-ipc/Cargo.toml:16`（`license = "MIT OR Apache-2.0"`）。
- 实际：仓库根 `LICENSE` 只有 Apache-2.0 正文；根 `Cargo.toml:29` 为 `license = "Apache-2.0"`。该值是刻意手写的（同文件第 9 行说明不能继承 `[workspace.package]`），但给出了仓库内无对应文本的 MIT 分支。
- 影响：仅元数据；`publish = false`，`deny.toml` allow 列表同时含 `MIT` 与 `Apache-2.0`，不触发门禁。
- 最小修复：改为 `license = "Apache-2.0"`；若确有意双许可，需在仓库内补 MIT 文本。

### RV1-WP2-F3 — MINOR（P2，report-only，证据记录缺口）

- 位置：`reports/du1-pv1.log` 的两个 WP2 轮块（第 61 行起、第 125 行起）；`reports/pv5-windows-ipc.log` 第 (3) 节；`wp2-handoff.md`（LC2 行与 PV1 行）。
- 实际：
  - (a) 两个 `[PV1]` WP2 轮块**没有**显式退出码行（同文件其它轮次有：W0 轮 `PV1_NPM_CHECK_EXIT=0`、WP3a 轮 `EXIT(npm run check)=0`），而 handoff 写成「退出码 0」。10 道门禁成功输出与 agentic 门 `Installation: PASS / 13 passed, 0 failed` 均在，因此 0 极可能成立，但**日志未记录**。
  - (b) handoff 的 LC2 主张含 `cargo fmt -- --check` 退出码 0，但该日志段与正文**都没有 fmt 的运行记录**。
- 影响：不影响代码正确性，但自评退出码无可复查日志，属证据强度缺口。
- 最小修复：补退出码行 / 补 fmt 记录。
- **主 Agent 处置（2026-09-25）**：已补录两条显式退出码轮次到同一工作区日志——`reports/du1-pv1.log` 追加「命令: npm run check … EXIT(npm run check)=0」，`reports/pv5-windows-ipc.log` 追加「命令: cargo fmt -- --check … EXIT(cargo fmt -- --check)=0」（均在 `vendor/windows-local-ipc` 与仓库根执行；日志按 `.gitignore:27` 不入库，属工作区过程证据）。F3 关闭。

## Verified correct（逐条对应关注点）

1. **unsafe 不变量成立、资源无泄漏**（独立复算）：句柄先判失败再包 `OwnedHandle`（`src/windows.rs:90/223/311/323-327`），`GetCurrentProcess` 伪句柄不关闭（`85-86`），所有早退路径都在守卫作用域内 ⇒ 恰好关闭一次；`LocalAlloc` 两条来源（SDDL 描述符 `183-208`、SID 字符串 `271-305`）都由具名绑定守卫覆盖（`289` 附近为具名，非 `let _`），UTF-16 解码失败走 `map_err`（`303-305`）；两阶段 `GetTokenInformation`（`232-268`）先取长度、`needed == 0` 失败关闭、先断言 `needed >= size_of::<TOKEN_USER>()` 再分配、`read_unaligned::<TOKEN_USER>()`（`266`，`Vec<u8>` 不保证对齐，用法正确）、`User.Sid` 指向缓冲内部且缓冲在返回前存活；`sid_to_string` 手工扫描 NUL（`297`）后 `from_raw_parts`（`302`），输出为 NUL 结尾的 `LocalAlloc` 内存。
2. **`from_raw_handle` 所有权注释与 tokio/mio 实际行为一致**（逐行核对源码）：tokio 1.53.1 `named_pipe.rs:128-134` 先 `mio::windows::NamedPipe::from_raw_handle`（不失败），再 `PollEvented::new(...)?`；mio `Handle::drop` 调 `CloseHandle`（`mio-1.2.3/.../handle.rs:26-30`），`NamedPipe::drop` 取消在途操作（`named_pipe.rs:693-710`）；`PollEvented::new_with_interest_and_handle`（`tokio-1.53.1/src/io/poll_evented.rs:114-124`）按值接收 `io`，失败时 `io` 在函数内 drop；无 runtime 时 panic 位置即 `named_pipe.rs:132`，unwind 同样 drop `io` ⇒ **Ok/Err/panic 三条路径都由 mio 关闭句柄**，wrapper 之后再关即重复关闭，注释结论正确。mio `register` 只把既有句柄加入完成端口（`named_pipe.rs:622-656`），不 `DuplicateHandle`。
3. **SDDL 不过宽也不过窄**：`owner_only_sddl` = `D:P(A;;GA;;;{user_sid})`（`src/windows.rs:162-164`），单条显式 ACE、`P` 置 `SE_DACL_PROTECTED`、无 `WD/BU/AU/AN/BG/IU/SU/SY/BA/CO`；`GA` 对 pipe 含 `FILE_CREATE_PIPE_INSTANCE`（后续实例创建权限不缺）。SDDL 来自本进程令牌，公开 API 不接受调用方 SID/SDDL，测试用的 `create_pipe_server_for_sid`/`owner_only_sddl` 均为私有 fn（`118`/`162`）。运行时样本：`D:P(A;;GA;;;S-1-5-21-2039568721-3854868400-2889826567-1002)`。
4. **pipe 常量语义正确**：`OPEN_MODE`（`45`）= `PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE | FILE_FLAG_OVERLAPPED`、`PIPE_MODE`（`48`）= `PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS`（常量取值经 windows-sys 源码核对），与 tokio `ServerOptions::new()` 的 open/pipe mode 逐位相同（tokio `named_pipe.rs:1768-1778`），即 WP3 后续实例参数一致；`bInheritHandle: FALSE`（`126`）与 tokio `create()` 传 `NULL` 安全属性（`named_pipe.rs:2275-2278`）都不可继承 ⇒ 全实例一致。
5. **公开面最小**：导出只有 `src/lib.rs:36-39` 三个 safe 函数；WM 调用全封在私有函数内；`#![allow(unsafe_code)]`（`src/lib.rs:33`）附完整理由与收敛说明。
6. **错误一律失败关闭**：所有 `BOOL`/`HANDLE` 返回都被判定（`90/107-111/137-140/215-218/223-226/237-243/251-258/281-286`），无「默认 DACL」回退；`ConvertStringSecurityDescriptorToSecurityDescriptorW` 声称成功但返回 null 判 `InvalidData`（`198-206`）；pipe 名内嵌 NUL 由 `to_wide` 拒绝（`170-180`）。
7. **测试断言非永真**（与日志实数比对，11 passed / 0 failed / 0 ignored）：真实 4 字节往返（`50`）、SDDL 逐字比对 + 单 ACE/单 SID（`90`）、非法 SDDL 断言 OS 错误码 1336/1338（`108`）、重复首实例断言 `raw_os_error == Some(5)`（`135`，证明 `FIRST_PIPE_INSTANCE` 生效）、命名空间外名字断言 123（`149`）、未连接时取凭据返回 Err（`162`）、`catch_unwind` + 同名首实例可再创建证明 panic 路径不泄漏句柄（`179`，**具备反例能力**）、tokio safe API 后续实例可建（`229`）。
8. **windows-sys feature 最小且逐点对应**：6 个 feature 全被用到，导入项存在性与所属模块经源码核对（`Foundation`/`Security`/`Security_Authorization`/`Storage_FileSystem`/`System_Pipes`/`System_Threading`）；非 Windows 面 `cfg(windows)` 门控，依赖在 `[target.'cfg(windows)'.dependencies]` ⇒ Linux 上为空 crate（负向探针 `E0425` 与门控点一致）。
9. **无越范围改动**：`vendor/windows-local-ipc/` 只有 4 个文件（无遗留探针/临时文件）；`publish = false`；非正常运行路径无 `unwrap/expect/panic`（`expect` 仅在测试）；根 `.gitignore:5-10` 的 vendor 条目是**根锚定**的，未写成 `vendor/` 通配。
10. **与 §5 矩阵一致**：本 crate 行全空 ✓；`server` 行含本 crate ✓。
11. **「unsafe 只在 vendor crate」的独立复算**：当前 HEAD 下 `crates/` 内 `unsafe` 命中 9 处（WP2 时 6 处，多出 3 处来自 WP3a 的 `crates/server/**`），逐条均为注释或测试函数名；精确判据 `unsafe[[:space:]]*(\{|fn|impl|extern|trait)` **零命中** ⇒ 不变量仍成立。

## Residual Risks（随变更带走）

1. **后续实例 DACL 归属未证实**：首实例「仅当前用户」SDDL 已证；WP3 的后续实例走 tokio `ServerOptions::new().create()`（`crates/server/src/transport/local/platform/windows.rs`，`accept()` 内）传 `NULL` 安全属性，其实际 DACL 是否继承首实例**未被证明**（需 `GetSecurityInfo` 或第二账号）。现实影响有界：`accept()` 每次对 `client_user_sid == current_user_sid` 逐字比对、拿不到凭据即 `PeerRejected`（失败关闭），第二层仍拦；但 endpoint 层的证据仍有缺口。建议 WP3/WP4 的 [PV5] 用 `GetNamedSecurityInfo`/`GetSecurityInfo` 或跨账号用例钉死。
2. **跨用户拒绝本机不可真实执行**（已如实登记，未写成 PASS）；WP2 提供静态 SDDL 证据 + 失败关闭证据。
3. **版本识别限制**：本报告基于 reflog 提交链 + 内容指纹，非字节级 diff；若其后存在静默改动 `vendor/**` 的提交，需重跑本检视。
4. **方法论固有 TOCTOU（契约自带，非本 diff 引入）**：`GetNamedPipeClientProcessId` 取 PID 后再 `OpenProcess` 取令牌存在 PID 复用窗口；`LOCAL_ADMIN_PROTOCOL.md` §2.2 规定的方法如此，第一层 DACL 仍在拦，仅登记。

## Assessment

目标版本的 FFI 实现在正确性、资源生命周期、失败关闭、最小公开面与可审计性上均成立；逐条溯源 tokio/mio/windows-sys 源码验证了注释中的断言，未发现 P0/P1：无不成立的 unsafe 不变量、无句柄/内存泄漏路径、SDDL 既不过宽也不过窄、错误路径全部失败关闭、无永真断言、无越范围改动。三条 MINOR 均为注释事实 / 许可证元数据 / 证据记录层面，不阻断交付（F3 已由主 Agent 补录证据关闭；F1/F2 建议并入 WP5 顺手收口）。**结论：PASS**（target `6361f5d2a34d9f6306f62b41c852749e7d86bc4d`）。Merge verdict：OK with notes。

## handoff_index

```yaml
handoff_index:
  - task_id: "3.4"
    role: reviewer
    phase: work-package review
    stage: work-package
    target_revision: "6361f5d2a34d9f6306f62b41c852749e7d86bc4d"
    evidence_type: REVIEW
    evidence_id: RV1
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/rv1-wp2.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "WP2（vendor/windows-local-ipc FFI wrapper + .gitignore vendor 条目）在 6361f5d 上的交付前独立检视：逐条复核 unsafe 不变量（对照 tokio 1.53.1 / mio 1.2.3 / windows-sys 0.61.2 源码）、SDDL 过宽/过窄、失败关闭、公开面、windows-sys feature 逐点对应、测试反例能力与证据日志一致性；无 P0/P1，仅 3 条 P2（注释事实、许可证元数据、PV1 退出码与 fmt 未记录，后者已补录）。版本识别依据 reflog 提交链与内容指纹（无法执行 git diff，已在报告中声明限制）。"
    source_evidence: NOT_APPLICABLE
```
