# WP4 2.26 交接：`daemon stop` 的排空延迟诊断与修正

```yaml
task_id: 2.26（daemon stop 的排空满宽限等待：定位 + 修正 + 回归断言）
role: 实现 Agent（coder，受限 worker；只写 crates/app/**、reports/wp426*）
phase: apply（RV 前修正）
agent_context: >
  task 2.25 的交接登记了一项未定位的既有现象：本机每次 `daemon stop` 都要等满
  `daemon.shutdown_grace_ms`（默认 10000 ms，日志 `daemon.drain_timeout{remaining:1}` →
  `daemon.task_stop_timeout{maintenance}` → `daemon.stopped{elapsed_ms≈10000}`）。本轮先独立复现，
  再用受控探针定位到**具体是哪条连接**与**它为什么结束不了**，然后在 `crates/app` 内做最小修正，
  并把「无其他客户端在途时不必等满宽限」钉成常驻回归断言。关闭序列顺序未改（§12.1 / §7.1）。
target_revision:
  branch: feat/daemon-cli-and-local-admin
  base: 580ae46（本轮起点；本轮期间另一 Agent 的文档提交 94067e1 先于本轮提交落到分支上）
  implementation_commit: 82d376d   # fix(app): 消除 daemon stop 的排空满宽限等待
  head_at_handoff: 82d376d（本报告为紧随其后的第二个提交）
scope:
  - crates/app/src/cli.rs（`Context::release_runtime` + `daemon_stop` 在等锁之前调用它）
  - crates/app/tests/daemon_lifecycle.rs（新增回归断言 `stop_does_not_wait_for_the_full_grace_without_other_clients`）
  - openspec/changes/daemon-cli-and-local-admin/reports/{wp426.log, du1-pv1.log, wp426-handoff.md}
未改动: crates/server/**、crates/core/**、crates/storage-sqlite/**、docs/**、schemas/**、fixtures/**、
        兼容性词表、权威规划文件（plan/tasks/verification/proposal/design/specs）与关闭序列顺序。
```

## 1. 现象复现（未修正，默认宽限 10000 ms）

真实二进制 + 默认配置 + `daemon start`（子进程）/ `daemon stop`（CLI）：

```text
daemon 已停止（单实例锁已释放）
EXIT(daemon stop)=0 elapsed_ms=10140
日志：daemon.stop_accepted → daemon.shutdown_begin{grace_ms:10000} → daemon.drain{drain_ms:9999}
      → daemon.drain_timeout{remaining:1}（+10.011 s）→ task_stop_timeout{maintenance}
      → task_stop_timeout{signal_watcher} → agents_stopped → storage_closed → stopped{elapsed_ms:10022}
```

**`remaining:1` 恒为「发出 `daemon.stop` 的那条连接本身」**：探针里 Daemon 从启动到关闭只接受过
一条连接（CLI 的停止连接），`JoinSet::len()` 也因此只能是 1。它既不是别的客户端，也不是任务计数假象
（`drain_connections` 的 `join_next` 循环在宽限到期时仍收不回它）。

## 2. 定位：受控探针（一次性文件 `crates/app/tests/zz_probe426.rs`，已删除）

四种客户端收尾方式，真实二进制 + `shutdown_grace_ms = 3000`，测量「`daemon.stop` 被接受 → Daemon 进程退出」：

| 探针 | 客户端收尾方式 | Daemon 退出耗时 | `daemon.drain_timeout` |
|---|---|---|---|
| A | 进程内客户端 `drop(client)` 后**保留但不驱动** current_thread runtime（= CLI 现有形态） | **3019 ms**（= 等满宽限） | 1 |
| B | `drop(client)` 后继续驱动同一 runtime | 驱动 1 s 内 Daemon 已退出（`wait_exit` 立即返回 0 ms） | 0 |
| C | `drop(client)` 后 `drop(runtime)` | **25 ms** | 0 |
| D | `daemon stop` CLI 子进程（对照组） | **3099 ms**（= 等满宽限） | 1 |

对照组：同一条 CLI 路径的 `daemon status` 连接在 CLI 退出时立刻被服务端记为 `peer_eof`
（`local_admin.connection_closed{reason:peer_eof}`）——说明「客户端进程存活期间，句柄没有因为
`drop(client)` 而真正关闭」。

**根因（源码级证据，不是猜想）**：

1. `LocalAdminClient` 持有 tokio 的 `NamedPipeClient`，其内部是 `PollEvented<mio::windows::NamedPipe>`
   （tokio 1.53.1 `src/net/windows/named_pipe.rs:979`）。
2. `mio::windows::NamedPipe` 是 `{ inner: Arc<Inner> }`，`Inner` 持有 `handle: Handle`
   （`Handle::drop` 才 `CloseHandle`）；读取一律**在后台排队成一个 overlapped 读**，该挂起操作在
   completion 处理时会 `Arc::from_raw` 把 `Inner` 复活（mio 1.2.3 `src/sys/windows/named_pipe.rs`
   的 `Inner`/`schedule_read`/`on_read_completion`）。
3. `mio::windows::NamedPipe` 的 `Drop` 只**取消**挂起操作、**不关闭句柄**（同上文件 `impl Drop for NamedPipe`）；
   `Source::deregister` 也只把 `io.token` 置空。因此 `drop(client)` 之后句柄仍被挂起的 completion 持有，
   只有 **runtime 的 I/O driver 处理完该 completion（`block_on` 期间）或 runtime 被销毁**才会 `CloseHandle`。
4. CLI 把 runtime 缓存在 `Context.runtime`（current_thread），`daemon_stop` 在 `drop(client)` 之后进入
   `wait_for_lock_release`——**全程 `std::thread::sleep` 轮询，从不驱动那个 runtime**。于是句柄一直开到
   CLI 进程退出，Daemon 侧 `serve_connection` 的读永远看不到 EOF，`drain_connections` 只能等满宽限。

这也解释了 2.25 报告里「集成用例全绿、只有 CLI 路径慢」：测试里的进程内调用（`support::block_on`）
每次调用都新建并销毁 runtime（探针 C 的形态），句柄随 runtime 析构立即释放。

## 3. 修正（`crates/app`，最小改动）

| 位置 | 改动 |
|---|---|
| `crates/app/src/cli.rs`（`Context`） | 新增 `fn release_runtime(&mut self)`：取出并**显式析构**缓存的 runtime，文档注释写明「句柄由挂起的 overlapped 读持有、只有 driver 处理完 completion 才 `CloseHandle`」这一约束与调用时机要求 |
| `crates/app/src/cli.rs`（`daemon_stop`） | 在 `drop(client)` 之后、`wait_for_lock_release` 之前调用 `context.release_runtime()`：让停止连接随它的 runtime 一起消失，Daemon 立刻看到对端 EOF |

- **未改**关闭序列顺序（停接入层 → 排空 → 取消周期任务 → 停 Agent → checkpoint → 释放锁），
  `crates/server/**` 一行未动（`serve_connection`/`drain_connections` 本身是对的：它等的就是对端关闭）。
- 修正只作用于「CLI 主动停止」这条路径；`daemon status`/`doctor` 等命令不等待，进程退出即释放句柄，
  因此不需要改动（保持最小范围）。

修正后的实测（同一探针，grace = 3000 ms）：CLI 子进程 `elapsed_ms = 102`、`daemon.drain_timeout = 0`。

## 4. 回归断言（硬要求）

新增 `crates/app/tests/daemon_lifecycle.rs::stop_does_not_wait_for_the_full_grace_without_other_clients`：

- 真实二进制 + 独立数据目录；**无其它在途客户端**，只用 CLI 子进程执行
  `daemon stop --grace-ms 3000`（显式较小宽限，`grace_ms_override` 在日志中可见）；
- 断言 ① `daemon.drain_timeout` 事件为空（排空没有走到超时分支）、② `elapsed < grace_ms * 2 / 3`
  （2000 ms；本机实测修正后 < 200 ms，修正前 3023 ms，两侧相差一个数量级，阈值不低于 1 s）、
  ③ CLI 返回后单实例锁可再取（`DaemonLock::acquire`）、④ Daemon 进程正常退出。

RED（修正临时禁用，同一断言）：

```text
panicked at crates\app\tests\daemon_lifecycle.rs:344:5:
无其他客户端在途时不得出现排空超时：日志=… "grace_ms_override":"Some(3000)" …
  drain{drain_ms:2999} → drain_timeout{remaining:1}(+3.009 s) → stopped{elapsed_ms:3023}
test result: FAILED. 0 passed; 1 failed; 0 ignored
EXIT(cargo test … [RED])=101
```

GREEN（修正后）：`1 passed; 0 failed; 0 ignored … finished in 0.29s`，`EXIT=0`。

## 5. checks（逐检查 ID 的 handoff_index）

| index | 检查 ID | 命令 / 判据 | 结果 | 证据 |
|---|---|---|---|---|
| 1 | 2.26 完成条件① | `cargo fmt --all -- --check` | **PASS**（EXIT 0） | `reports/wp426.log`（`EXIT(cargo fmt --all -- --check)=0`） |
| 2 | 2.26 完成条件② | `cargo clippy --locked -p app --all-targets --all-features -- -D warnings` | **PASS**（EXIT 0） | `reports/wp426.log`（`EXIT(cargo clippy -p app)=0`） |
| 3 | 2.26 完成条件③ | `cargo test --locked -p app --all-features`：49 lib + 1 audit_export + 11 cli_commands + **10** daemon_lifecycle，全部 0 failed / **0 ignored** | **PASS**（EXIT 0） | `reports/wp426.log`（`EXIT(cargo test --locked -p app --all-features)=0 wall_ms=65549`） |
| 4 | PV1（合同门禁） | `npm run check`（acp / docs / boundaries / drift / schema / fixture / commands / agentic 全量串行） | **PASS**（EXIT 0） | `reports/du1-pv1.log`（`EXIT(npm run check)=0`） |
| 5 | RED→GREEN | 同一断言在修正禁用时失败（EXIT 101，`drain_timeout{remaining:1}`、elapsed 3023 ms）、修正后通过（0.29 s） | **PASS** | `reports/wp426.log` §2/§3 |
| 6 | 定位证据（A/B/C/D） | 一次性探针 `crates/app/tests/zz_probe426.rs`（已删除）：A 3019 ms / B 驱动即结束 / C 25 ms / D 3099 ms；修正后 D = 102 ms 且 `drain_timeout=0` | **PASS** | `reports/wp426.log` §1/§4 |
| 7 | 提交前钩子 | `.husky/pre-commit`（fmt + `npm run check` + clippy） | **PASS**（3 步通过；本地 gitleaks 未安装，已如实提示） | 提交输出（本报告 §7） |
| 8 | PV3（core/storage） | `core`/`storage-sqlite` 未改动；workspace 级 `npm run verify`（另一 Agent 的进度）已包含 `check:rust`，不在本任务范围内 | **NOT RUN（未改动，超出本 WP 范围）** | 本报告 §8 的 `git diff --stat` |
| 9 | PV5（Windows IPC 语义） | 本轮**触及** Windows IPC 的读取者语义（句柄释放时机），但结论由本机探针（index 6）判定；CI 的 Linux runner 不编译该路径，且修正不依赖 `cfg(windows)`（跨平台语义一致） | **PASS（本机判定）** | `reports/wp426.log` §1/§4 |

**跳过数**：`-p app` 全套的 `ignored` 为 **0**（lib 0 / audit_export 0 / cli_commands 0 / daemon_lifecycle 0）。
条件编译用例（`#[cfg(windows)]`/`#[cfg(unix)]`）既不计 passed 也不计 ignored。
本地未执行的判定：`gitleaks`（未安装）与 `cargo-deny`（只在 CI 的 `deps`/`advisories`/`secrets` job 运行）。

## 6. 耗时（修正前后，同一台机器、同一条命令）

| 目标 | 修正前（2.25 归档基线 `reports/wp425.log` / 本轮 RED） | 修正后（本轮 GREEN） |
|---|---|---|
| `cli_commands`（含 `daemon_stop_waits_for_the_lock_and_the_process_exit`） | 10.66 s | **2.73 s** |
| `daemon_lifecycle`（10 个用例；其中 1 个按设计跑满 ~60 s 的周期任务） | 60.45 s（9 个用例） | 60.65 s（10 个用例） |
| 新增回归断言单跑 | 3.26 s（RED） | **0.29 s**（GREEN） |
| `cargo test -p app --all-features` 总墙钟 | ~71.4 s（2.25 基线四套之和） | **65.5 s** |
| 真实 CLI `daemon stop --grace-ms 3000` | 3099 ms | **102 ms** |

## 7. 改动清单与提交

```text
 crates/app/src/cli.rs                | 19 +++++++++++++
 crates/app/tests/daemon_lifecycle.rs | 53 +++++++++++++++++++++++++++++++++++-
 2 files changed, 71 insertions(+), 1 deletion(-)
```

- `82d376d` `fix(app): 消除 daemon stop 的排空满宽限等待`（提交前钩子 3 步通过）。
- 提交用 `git commit --only <显式路径>`（并行 Agent 正在改 `docs/**`、`README.md`、`AGENTS.md`，
  本轮**未**卷入任何他人的暂存内容：提交前 `git diff --cached --stat` 为空）。
- **未 push、未开 PR、未合并**（无授权）。

## 8. issues（未执行项、残余风险与待澄清）

1. **修正的适用边界**：本轮只修「CLI 主动停止」这条路径。任何**长期存活的 CLI 连接**（例如将来的
   `acp-stdio` 字节泵或长连接管理子命令）在 Windows 上同样会遇到「`drop(client)` 不立即关句柄」，
   它们的 runtime 生命周期由各自路径决定；本轮没有改动它们，也没有把 `release_runtime` 变成通用收尾
   （那会改变 `acp-stdio` 的语义面，超出本任务）。
2. **未修改 `crates/server`**：`serve_connection` 只等对端关闭、`drain_connections` 的 `join_next` 循环
   与超时语义都正确；本轮定位结论是「客户端句柄没释放」，不是服务端缺陷，因此按任务要求只记录、不改 server。
3. **`daemon.task_stop_timeout{maintenance}`/`{signal_watcher}` 是次生现象**：`close()` 把任务取消期限写成
   `deadline.min(now + TASK_STOP_TIMEOUT)`，而排空恰好用尽 `deadline`，所以期限落在过去、立刻超时。
   修正后该路径不再触发（本机 GREEN 运行里已无该事件）。剩余风险：若将来出现「确实有在途连接」的正常场景
   （客户端主动保持连接），仍会看到 `task_stop_timeout` 两行——那是既有设计的可接受表现，本轮未改动它，
   以免改变已由 R13 断言的关闭顺序。
4. **`du1-pv1.log` 是共享日志**：本轮追加的 `npm run check` 段落位于第 1–43 行（含 `EXIT(npm run check)=0`）；
   同一文件里另有并行 Agent 的 `npm run verify` 输出（第 44 行起，含 `EXIT(npm run verify)=0`）。两者互不覆盖，
   复核时按各自的 `EXIT(...)` 行定位。
5. **本地未执行**：`gitleaks`（未安装）与 `cargo-deny`（只在 CI）——不视为通过，由 CI 的 `secrets`/`deps`/`advisories`
   job 判定。Linux CI 会额外编译 `#[cfg(unix)]` 路径；本修正不含平台分支，Unix 上句柄随 `drop` 立即关闭，
   新断言在 Unix 上同样成立（且更宽裕）。
6. **无 `#[ignore]`、无弱化断言**：既有断言一字未改；新增断言是「修正后必须通过、修正前必然失败」的形式。

## 9. result

- **定位结论**：`remaining:1` 是 **CLI 自己那条 `daemon.stop` 连接**。它结束不了的原因不是服务端读不到 EOF，
  而是 Windows 上 **Named Pipe 句柄的释放被挂起的 overlapped 读推迟到 I/O driver 处理完 completion 时**，
  而 CLI 的 current_thread runtime 在 `wait_for_lock_release` 期间全程不被驱动（也不被销毁），句柄一直开到进程退出。
- **修正**：`Context::release_runtime()` + `daemon_stop` 在等锁前释放 runtime（最小改动，未改关闭顺序、
  未改 server）；实测 CLI 停止耗时 3099 ms → **102 ms**，`drain_timeout` 事件消失。
- **判定**：本任务（2.26）检查 **PASS**；未通过项 0；未执行项见 §8.5（本地无法执行的 CI 专属判定）。

## 10. evidence_paths

- `openspec/changes/daemon-cli-and-local-admin/reports/wp426.log`：默认宽限下的复现、四路探针 A/B/C/D、
  RED（EXIT 101，含 daemon.log 关键行）与 GREEN（EXIT 0）、fmt/clippy/全量 `-p app` 的显式退出码行与耗时。
- `openspec/changes/daemon-cli-and-local-admin/reports/du1-pv1.log`：`npm run check` 全量输出与 `EXIT(...)` 行（第 1–43 行）。
- `openspec/changes/daemon-cli-and-local-admin/reports/wp426-handoff.md`：本报告。
- 两份 `.log` 按 `.gitignore`（`openspec/changes/**/reports/**/*.log`）不入库；复核时按上表命令重新生成即可。

## 11. resource_cleanup

- 一次性探针文件 `crates/app/tests/zz_probe426.rs` 已删除（`git status` 无残留；`crates/app/tests/` 只剩
  `audit_export.rs`、`cli_commands.rs`、`daemon_lifecycle.rs`、`support/`）。
- 探针用的两个临时根目录（`%TEMP%\wp426probe`、`%TEMP%\wp426probe2`，含 data/日志）已删除；
  `tasklist` 确认没有遗留的 `acp-remote.exe` 进程。
- 未创建 worktree、未改动 `CARGO_TARGET_DIR`；构建目录仍是仓库共享 `target/`。
- 本轮 `git status --porcelain` 在提交后只剩本报告（写完立即提交）。
