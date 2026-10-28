# WP5 fix3 最终验收（2026-10-26）

本节为当前结论，后附 fix1 原始 FAIL 记录作为历史，不覆盖或改写历史。

- **PASS**：指定真实 E2E 最终退出码 `0`，`1 passed; 0 failed`，21.79 秒。真实 Daemon、构建后的 PWA、受控 ACP Agent、Edge `156.0.4314.42`。
- 五个浏览器场景全部通过，随后 activation-first / deletion-first 两个跨层竞态断言通过；页面异常与 skipped 均为空。最终日志含 `WP5_BROWSER_EVIDENCE` 和 `WP5_CROSS_LAYER_RACES=PASS`。
- 启动前未激活空会话在新窗口 `-1ms` 存活，在精确终点静默删除并产生不可见墓碑；没有伪造 `session/close`，clean-shutdown fence 消费记录为 true。fresh 对照会话有真实 Agent close 先于删除。
- 单独激活会话的全部相关 resume 通过原 requestId 对应的持久终态事件核对；仍要求新 wire completed、真实 Agent resume/result。释放查看者的 ACK 后，还读取真实 Daemon 在 Owner 会话门内的核心快照，要求 `viewerCount=0`、`retentionStart` 已建立且等于预期 `-1ms` 时刻，才推进时钟。新完整窗口仍检查 `-1ms` 存活和精确终点删除；没有随意增加时间或以固定 sleep 代替断言。
- 初始切换场景仍保留 5 秒受控延迟，验证迟到 A 结果不恢复旧路由/查看者；重启边界使用单独的 1.5 秒受控延迟，避免把切换专用慢响应重复注入边界用例。此配置不代表旧连接租约清理问题已修复。

所有产物位于 `C:/Users/jack/AppData/Local/Temp/acp-wp5-integrated-20261026/`。本轮不修改主区、索引或 commit；未修改 core、server、node-link-client 或四个独立清理文件。

## 本轮确认的原因与修复

1. driver 两处把 `command.result` 的会话标识读成 `body.sessionId`，实际 fixture 明确为 `body.result.sessionId`。已修正；requestId 与 Agent 证据要求保留。
2. driver 原先看到任一完成就继续，不足以覆盖同会话多条 resume。连接替换还可能丢失旧连接的响应帧，不能无限要求所有旧请求收到 result。现在只读 SQLite 的 owned mutation 记录，按相关 requestId 核对 `completed` 和 `terminal_event_id`，并要求没有 accepted 修改命令；不重派副作用。
3. run6/7 运行期观测显示：释放 ACK 后保留起点仍为 None，时钟较晚推进后才出现起点；run7 快照同时观察到旧连接仍被列为查看者。这证明此前推进条件不足，不能把全部失败称为测试错误，亦不能声称已修复通用连接替换清理。最终屏障直接核对真实核心状态。临时 DEBUG 代码已移除，正式测试观察任务只在既有开发模式文件时钟启用时启动，正常生产不启用。
4. **生产缺陷**：run8 明确日志 `expected=2/current=1`。core 给启动空会话分配第 1 代，恢复分配第 2 代；agent-host 未同步启动代、首次恢复仍为第 1 代，因此严格拒绝释放。app 现在于启动收敛完成、入站开放前登记 core 启动代到 agent-host；host 只有已消费 clean-shutdown 证明才确认旧代已释放，首次恢复递增到相同新代并撤销旧代证明。严格 generation 检查完整保留。聚焦测试覆盖有/无证明、重复登记拒绝、递增为第 2 代和旧代证明失效。

## 实际执行

所有 Cargo 测试设置 `CARGO_TARGET_DIR=C:/Users/jack/AppData/Local/Temp/acp-wp5-target`、`CARGO_BUILD_JOBS=2`。输出落盘，每份日志末尾写入真实 `REAL_EXIT_CODE`。

| 检查 | 本轮结果 | 日志 |
| --- | --- | --- |
| `cargo test -p app --test session_create_empty_cleanup_browser -- --ignored --nocapture`（最终含核心状态屏障） | PASS，exit 0，1 test，21.79s | `fix3-e2e-observed-final.log` |
| 同一真实 E2E，run9 | PASS，exit 0，22.87s | `fix3-e2e-run9.log` |
| 同一真实 E2E，增加成功证据输出后 | PASS，exit 0，21.53s | `fix3-e2e-final.log` |
| `cargo test -p agent-host --lib` | PASS，exit 0，63 tests | `fix3-agent-host-lib.log` |
| `cargo test -p agent-host --test supervision` | PASS，exit 0，17 tests | `fix3-agent-host-supervision.log` |
| `cargo test -p agent-host only_a_consumed_fence_can_trust_missing_startup_sessions` | PASS，exit 0，实际执行 1 项，其余被过滤 | `fix3-generation-test.log` |
| `cargo fmt --all -- --check` | PASS，exit 0 | `fix3-fmt.log` |
| `node --check clients/app/e2e/session-create-empty-cleanup-live.mjs` | PASS，exit 0 | `fix3-node-check.log` |

## 保留的历史 FAIL

- **fix3 首次 FAIL（run1）**：退出码 101，49.10s，`restart-grants-a-fresh-window: timeout waiting for session.deleted for b550b941-91ec-442a-aff0-103586de8b92`。该 ID 为 restart-activation，Agent ID 为 cleanup-agent-4；seeded 与 fresh 删除已越过，不能再把它描述为启动未激活会话删除失败。临时数据库被测试清理，run1 没有事后数据库读取证据。
- run2：退出码 101，49.17s，仍在激活会话窗口终点超时；失败时真实数据库 `live=true/deletedAt=null/messages=0/turns=0/interactions=0`。
- run3：退出码 101，47.69s；错误要求所有派发都收到 wire result，在连接替换丢响应时等待超时。后续改用真实持久终态。
- run4：退出码 101，50.05s；数据库确认相关修改命令均完成，仍等待删除超时，说明不能只归因为缺响应。
- run5：退出码 101，14.27s；临时诊断插入时误用未导入的 `fs`，`ReferenceError: fs is not defined`；已修正并最终移除该临时诊断。
- run6：退出码 101；观测到保留起点直到后一个窗口 `-1ms` 才建立。run7：退出码 101，49.33s；增加查看者观测佐证释放后仍有旧连接查看者。日志中的 DEBUG 是历史诊断证据，交付代码无 DEBUG。
- run8：退出码 101，46.59s；明确生产资源代不一致，严格拒绝释放，随后修复启动代登记。

对应文件为 `fix3-e2e-run1.log` 至 `fix3-e2e-run8.log`。下方 fix1 和原作者 fix2 的证据仍按其原运行范围理解，不作为本轮所有 target 通过的证明。

## 交付与未运行

累计补丁从 `af1ddcdf00ca8f6ccf247b2beca487516e7bdfa5` 起生成，文件名 `acp-wp5-fix3-review.patch`。精确文件列表、最终规范 Git tree 和补丁 SHA256 在 `fix3-delivery.json`；生成过程不写 Git 索引或对象库，并在独立基线快照执行 apply check、apply 和逐文件字节比对。

以下均为 **SKIP / 本轮未运行**：全 workspace / 全 target Rust 测试或 Clippy、根 npm check、前端 npm check/export、最终 npm verify、其他浏览器/操作系统、真实 Codex/Oh My Pi、正常设备身份跨重启重连。协调者主区已通过的门禁是独立结果，不在本证据中扩大归因。本用例重启后重新配对，且使用测试 TLS 证书。

最终独立 review 由协调者另行 new spawn，作者自检不代替 review。

---

## fix1 历史原始记录（保留，非当前结论）

# Session Create and Empty Cleanup Acceptance

## Result

- **Status: FAIL** for the WP5 review-fix candidate on Windows.
- The latest real-browser run completed the first two browser scenarios, then
  failed the cross-restart startup-seeding scenario. The required command exited
  with code `101`; the browser driver exited with code `1`.
- This evidence does not claim that MAJOR 1 is fixed. Within the four-file review
  patch, the test now observes the startup-seeded failure instead of masking it
  with a post-restart session.

The latest failure is not a browser-driver timing failure. At the restart clock
the pre-restart, never-activated empty session remained listed and authoritative
at the new `-1ms` boundary. At the exact new boundary, its `session.deleted`
event never arrived. The Daemon repeatedly reported:

```text
会话没有运行时 descriptor，且没有同代肯定释放证据
```

The resulting release was `Uncertain`. Reaching the required
`Agent session/close` before `session.deleted` requires restart recovery in the
product lifecycle/runtime layer. That change is outside the four files allowed
for this review patch, so this patch deliberately leaves the target test red.

## Environment

| Item | Observed value |
| --- | --- |
| Platform | Windows x64 |
| Date | 2026-10-26 |
| Workspace | `C:\Users\jack\AppData\Local\Temp\acp-wp5-integrated-20261026` |
| Cargo target | `C:\Users\jack\AppData\Local\Temp\acp-wp5-target` |
| Baseline tree | `af1ddcdf00ca8f6ccf247b2beca487516e7bdfa5` |
| Browser | Headless Microsoft Edge `Edg/156.0.4314.42` |

The run launched the built PWA from `clients/app/dist`, the real Daemon over
loopback TLS, the controlled Node ACP Agent, and headless Edge through CDP. It
used the test file clock and bounded DOM/wire polling; it did not use screenshots
or fixed sleeps as assertions.

The restart path re-paired the replacement Daemon. Normal device
authentication/reconnection across a Daemon restart is **not covered** by this
test.

## Commands and Results

| Command | Result |
| --- | --- |
| `node --check clients/app/e2e/session-create-empty-cleanup-live.mjs` | PASS |
| `cargo fmt --all -- --check` | PASS: exit code `0` |
| `git diff --check` | PASS: exit code `0`; Git emitted LF-to-CRLF notices, with no whitespace error |
| `$env:CARGO_TARGET_DIR='C:\Users\jack\AppData\Local\Temp\acp-wp5-target'; cargo test -p app --test session_create_empty_cleanup_browser -- --ignored --nocapture` | **FAIL**: exit `101`, `0 passed; 1 failed`, `65.33s` |

The full latest terminal output is retained at:

```text
C:\Users\jack\AppData\Local\Temp\acp-wp5-fix1-target-run-2.log
```

An earlier run in this review cycle also failed before reaching the restart
deletion assertion because the browser driver treated a path update as a
completed `workspace.browse`. That run is **FAIL**, not PASS. Its output is
retained at:

```text
C:\Users\jack\AppData\Local\Temp\acp-wp5-fix1-target-run.log
```

The driver now waits for both the requested path and a settled browse state.

## Browser Scenarios

### `direct-create-is-persisted-and-active` - PASS

- The built PWA emitted one `session.create` with only `workspacePath` and
  `agentId`, persisted the formal session, assigned its directory, and navigated
  with the returned session ID.
- The post-create view was active before any prompt.

### `switch-keeps-latest-session` - PASS

- Switching between two real sessions preserved the latest viewer declaration
  and route.
- The delayed response from the previous session did not restore its viewer or
  move the route.

### `restart-grants-a-fresh-window` - FAIL

- Two empty sessions were created before the same-data-directory Daemon restart
  and remained listed after restart:
  - `restart-seeded-boundary`: deliberately never activated;
  - `restart-activation`: reserved for activation at the new `-1ms`.
- A third `restart-fresh-boundary` session was created after restart as a
  comparison case.
- At `restartClock + 30m - 1ms`, both pre-restart sessions and the post-restart
  comparison session remained listed; the startup-seeded session was also
  authoritatively live. It had no controlled-Agent `session_resume`.
- Activation of `restart-activation` succeeded at that `-1ms` point.
- At `restartClock + 30m`, the startup-seeded session did not emit
  `session.deleted` within the bounded wait. The test failed before the close
  ordering and tombstone assertions could be evaluated.
- The retained Daemon log shows no descriptor after restart and no same-generation
  confirmed release, followed by repeated `command.uncertain` activity. Therefore
  the required `Agent session/close -> session.deleted -> tombstone` proof is
  absent.

### `exact-boundary-cleanup-and-retention` - SKIP

Not reached because the restart scenario failed.

### `deleted-session-does-not-resurrect` - SKIP

Not reached because the restart scenario failed.

## Cross-Layer Race Checks - SKIP

The activation-first and deletion-first Rust race checks were not reached because
the browser phase failed before those checks. They are **not PASS** for this
candidate.

## Review-Fix Behavior

### Startup-seeded identity and boundary assertions

The browser scenario now keeps the pre-restart `restart-seeded-boundary` session
unactivated. It asserts:

- presence in the product list and SQLite-backed inspection at the new `-1ms`;
- absence of controlled-Agent resume before the boundary;
- exact-boundary `session.deleted`;
- database tombstone and removal from subsequent product reads;
- controlled-Agent `session_close` before `session.deleted`.

The separate `restart-activation` session is activated at `-1ms` and has its full
new window checked. A third post-restart session distinguishes a fresh runtime
from a startup-seeded session.

### Page exceptions

The CDP listener records `Runtime.exceptionThrown` details, including text, URL,
line and column, exception metadata, and stack trace. Any page error forces the
driver exit code to `1`. The Rust test reads the report before checking the exit
status and explicitly asserts that `report.pageErrors` is empty, while including
the exception details and failure context.

The latest browser report contained `pageErrors: []`; this is not enough to make
the failing acceptance run pass.

### Process cleanup

- `BrowserDriver` owns the Node driver process and uses a `Drop` guard.
- On Windows the guard runs `taskkill /PID <pid> /T /F` when the driver is still
  live, then waits for the Node process. This covers success, failure, timeout,
  and Rust panic paths.
- The Node driver itself closes CDP connections, terminates Edge, waits for Edge
  exit, and removes its temporary profile in `finally`.
- Every browser-phase Daemon restart and the final Daemon shutdown uses
  `stop_daemon_checked`; a shutdown panic or non-success status fails with
  Daemon, browser, and Agent evidence.
- A post-failure `Win32_Process` scan found no dedicated Daemon, Agent, Edge, Node
  driver, or test executable carrying this run's WP5 identifiers.

## Not Executed

The following are **SKIP / not executed**, not PASS:

- Browser scenarios after `restart-grants-a-fresh-window`.
- Cross-layer activation/delete race checks.
- Firefox, Safari, mobile layout, and non-Edge Chromium.
- macOS and Linux process integration.
- Real Codex or Oh My Pi Agent compatibility; the run uses the deterministic
  controlled ACP Agent.
- Normal device authentication/reconnection across Daemon restart; the test
  re-pairs the replacement Daemon.
- Repository-wide post-merge verification.

## Limitations

- The target acceptance test is currently red because startup seeding does not
  provide a provable release path for a never-activated session after Daemon
  restart.
- Fixing that behavior requires product changes outside the allowed four files,
  likely preserving or reconstructing a restart-time runtime/close proof rather
  than synthesizing `Released` in the test.
- TLS uses a generated self-signed certificate with browser certificate checks
  disabled for loopback testing.

## WP5-P release-proof 修复实际检查（2026-10-26，独立窄包）

本节仅追加本次实际结果；上文旧候选的 FAIL、SKIP 与限制保持原样，不能解释成旧运行通过。原累计候选为 b368be2bf38309e99610a3fab7cb7c2e9cd336f1，累计基线为 af1ddcdf00ca8f6ccf247b2beca487516e7bdfa5。

- 写入仅发生在 `C:/Users/jack/AppData/Local/Temp/acp-wp5-integrated-20261026`；未提交、未发布、未向主检出应用补丁。
- 原写范围内修复 host/process/supervision 及两份权威文档对应段落；用户补充审计后明确授权 daemon 启动登记段的最小调整（3 行）。现有 browser driver、断言、compose、core/server/node-link-client 与四个旧测试清理文件均未追加修改。
- `CARGO_TARGET_DIR=C:/Users/jack/AppData/Local/Temp/acp-wp5-target`，`CARGO_BUILD_JOBS=2`；Windows、PowerShell、现有 Node 与 Rust 工具链。未运行 bash、全 workspace 测试或 npm verify。

### 两个 MAJOR 与补充审计点

1. 无有效旧关闭证明时，启动登记的遗留不确定状态在本宿主内不可逆；换代和空服务目录不能使它消失。宿主独立持有所有已启动但未确认退出的 supervisor，覆盖提前移出目录、关闭在途、初始化失败或取消；每次启动/关闭及时清除已确认项。
2. grace 超时、显式强杀与协议失败强杀均在共享监督代码中设置不可逆标记。本次 supervisor 此后永不提供可信退出证明；Unix 后续可能观测到进程组不存在也不能重新获得资格。正常优雅退出仍需根进程 wait 与空树两项证据。
3. daemon 对启动列表中的每个持久化会话先调用 `register_restart_session`，再独立对齐可用的非零资源代。禁用 timeout 的空会话、非空会话与第 0 代均不能漏记；登记不创建资源代，`seed_restart_generation(..., 0)` 仍被拒绝，配置语义不变。`SessionQuery::default()` 无行数上限，此处读取 owned 全量会话。

### 回归与反证

- 原实现实际运行 `unconfirmed_legacy_resources_survive_two_restarts`：FAIL（空 runtime 目录错误报告关闭成功）。日志：`wp5-proof-red.log`。
- 临时移除强杀否决后，`forced_stop_cannot_be_rehabilitated_by_later_exit_observations`：预期 FAIL，退出 101。日志：`wp5-proof-red-policy.log`。这是在 Windows 执行的共享政策测试，不冒充 Linux 进程实测。
- 临时去掉全体遗留会话登记后，`timeout_disabled_zero_generation_sessions_cannot_launder_restart_proof`：预期 FAIL，退出 101。日志：`wp5-proof-red-zero.log`。
- 临时去掉独立 supervisor 检查后，`removed_unconfirmed_runtime_blocks_fence`：预期 FAIL，退出 101。日志：`wp5-proof-red-retired.log`。
- 三次临时改动均在 finally 中逐字节恢复，再执行以下最终检查；没有保留调试旁路、allow 或固定 sleep 代替状态断言。
- 开发中一次测试 helper 的 `String`/`&str` 编译错误已修正；旧失败日志 `wp5-proof-lib.log` 保留。首次 fmt --check 报出新增测试排版差异，随后格式化并通过最终检查。

### 最终实际结果

| 检查 | 实际结果 | 原始记录 |
| --- | --- | --- |
| `cargo test -p agent-host --lib` | PASS：69 passed，0 failed，0 ignored | `wp5-proof-final-lib.log` |
| `cargo test -p agent-host --test supervision` | PASS：17 passed，0 failed，0 ignored | `wp5-proof-final-supervision.log` |
| `cargo test -p app --test session_create_empty_cleanup_browser -- --ignored --nocapture` | PASS：1 passed，含 5 个真实 browser 场景及 2 个跨层竞态 | `wp5-proof-browser-final.log`、`wp5-proof-browser-report.json` |
| `cargo fmt -p agent-host --check` | PASS | `wp5-proof-final-fmt.log` |
| `rustfmt --check --edition 2024 crates/app/src/daemon.rs` | PASS | `wp5-proof-final-daemon-fmt.log` |
| `node --check clients/app/e2e/session-create-empty-cleanup-live.mjs` | PASS | `wp5-proof-final-nodecheck.log` |

库回归包含：遗留资源跨连续两次重启保持 Uncertain；禁用 timeout/第 0 代启动两轮后重新启用 timeout 仍不信任旧资源；已移出目录的强杀进程以及关闭在途进程阻止发布全局证明；连续三次优雅回收不累积已确认 supervisor。严格旧代释放检查及既有取消/消费失败测试一并通过。

真实浏览器为 `Edg/156.0.4314.42`，使用现有构建 PWA、真实 Daemon、受控 ACP stdio Agent 与现有驱动，未改驱动断言。以下 5 场景均 passed：

- `direct-create-is-persisted-and-active`
- `switch-keeps-latest-session`
- `restart-grants-a-fresh-window`
- `exact-boundary-cleanup-and-retention`
- `deleted-session-does-not-resurrect`

`pageErrors: []`、`skipped: []`、`cleanShutdownFenceConsumed: true`；activation-first 与 deletion-first 两个竞态均 PASS。这里的 browser PASS 来自加入 daemon 全体登记后的重跑，不沿用旧候选结果。

### 尚未执行与交付边界

- Linux 进程集成：SKIP。本机 WSL 不可用，PATH 未发现 docker/podman；没有安装工具、启用云服务或宣称 Linux 已运行。Unix 相关政策通过共享代码与 Windows 上的反证测试覆盖，仍需可用 Linux 环境做进程实测。
- macOS、其他浏览器、真实 Codex/Oh My Pi Agent、完整 workspace/npm 合同门禁：未执行，交由主协调按最终合入流程运行。
- 这是新的 WP5-P 修复候选，作者自测不代替独立 reviewer；独立复核与主区最终门禁仍待主协调执行。
- 本候选补丁的文件清单以最终 tree 为准确认，避免从历史副本的交付元数据推断范围；交付元数据曾出现不一致，已在最终合入检查时以实际文件内容重新核对。

交付校验按 Git 文本规范统一 CRLF 为 LF 后逐文件比对；隔离区继承的纯行尾差异不作为 WP5-P 新改动纳入窄增量。旧证据正文保持不变，仅在末尾追加本节。

## WP5 final integrated gate and browser recheck (2026-10-26)

This section records the final integrated WP5 candidate only; all earlier FAIL and SKIP entries above remain historical records.

- The browser acceptance was rerun in this integrated WP5 workspace. It passed exit 0 with one Rust test result (`1 passed; 0 failed`), five required scenarios, and both cross-layer races PASS. Chromium was `Edg/156.0.4314.42`; browser errors and skipped scenarios were empty; `cleanShutdownFenceConsumed: true`. The run stopped all test-owned processes cleanly.
- The actual browser fixes are included: `SpawnedChild::into_supervisor` transfers the tree/child before clean shutdown, and confirmed exit plus an empty process tree produce the clean-shutdown fence. The new `host_clean_shutdown_waits_for_the_root_child_exit` regression test verified that a successfully started real Agent can be shut down and the fence written. Uncertain startup or forced-stop evidence remains unable to produce a clean fence.
- Final WP5 verification in this workspace: `npm run verify` exited 0. It covered all contract checks, formatting, Clippy with warnings denied, and `cargo test --locked --workspace --all-features`. The focused final test runs reported `74 passed` for `agent-host` library tests and `19 passed` for its supervision integration tests.
- The documentation gate found two stale references in `docs/INITIAL_DESIGN.md`. The Q16 core reference now names `CORE_PORTS_AND_STORAGE.md §5.8`, and the Q21 terminal reference now names `MODULE_ARCHITECTURE.md §4.5`; `npm run check` passes. Temporary proof copies were moved outside the repository so the documentation scan only covered product files.
- This is still an unmerged WP5 candidate, pending independent review and serial application to the main checkout. It has not been archived or published.

**Still not executed:** Linux, macOS, additional browsers, a normal device-authentication/reconnection flow across Daemon restart (the test re-pairs), and real Codex or Oh My Pi Agent compatibility. Do not treat any earlier SKIP or limitation above as a pass.
## 主检出最终复核（2026-10-27）

本次验证直接来自 `D:/Project/acp-remote` 主检出；上面的作者修复记录、FAIL 与 SKIP 均保留为历史，下面只记录本轮实际执行结果。浏览器验收依赖已构建的 PWA，实际环境为 Windows 上的 Microsoft Edge，而不是对其他平台或 Agent 的通用兼容证明。

### 合并修复

合并 12 个 WP5 运行期与测试文件后，首次 `cargo test -p agent-host --lib` 因进程监督支持文件内容没有完整进入主工作区而失败（`HostConfig` 缺少关闭证明字段、`ProcessTree` 缺少退出确认方法）。按已通过测试的候选版本逐文件恢复完整内容后重新验证通过。这里修正的是主检出合并状态；后续独立检视仍需确认最终差异。

### 本轮真实结果

| 检查 | 结果 |
| --- | --- |
| `cargo test -p agent-host --lib` | PASS：74 passed，0 failed |
| `cargo test -p agent-host --test supervision --features test-support` | PASS：19 passed，0 failed |
| `cargo test -p agent-host --test close` | PASS：7 passed，0 failed |
| `cargo test -p app --test session_create_empty_cleanup_browser -- --ignored --nocapture` | PASS：1 passed，0 failed，21.37 秒 |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | PASS，exit 0 |

真实浏览器报告为 Edge `Edg/156.0.4314.42`，`pageErrors: []`、`skipped: []`、`cleanShutdownFenceConsumed: true`。五条场景 `direct-create-is-persisted-and-active`、`switch-keeps-latest-session`、`restart-grants-a-fresh-window`、`exact-boundary-cleanup-and-retention`、`deleted-session-does-not-resurrect` 均通过；`activation-first` 与 `deletion-first` 两个竞争顺序均通过。

重启场景确认启动时重新计算保留期限，旧窗口终点删除后产生新窗口，新窗口终点再次删除；没有因旧候选的运行时代不一致而释放失败。空会话在期限前仍可激活，边界后删除，已发送首条消息的会话保留。删除后的会话读取得到 `session.not_found`，不能复活。

### 仍未覆盖

- Linux、macOS 与 Windows 以外的操作系统没有在本轮执行；其他浏览器、真实 Codex 或 Oh My Pi Agent 也没有执行。
- 重启跨接的浏览器用例需要重新配对，不能代表正常设备身份续连流程已通过重启场景的验收。
- 创建成功导航后立即发送第一条消息没有单独做成浏览器场景；现有的相邻持久化与竞争测试不替代这一项浏览器观察。

### 最终门禁（本轮）

- 根目录 `npm run check`：PASS，exit 0。
- `clients/app` 的 `npm run check`：PASS，exit 0；102 个测试文件、1503 项测试通过，生产 Web export 成功。
- 根目录 `npm run verify`：PASS，exit 0；合同检查、格式检查、全工作区 Clippy 和 `cargo test --locked --workspace --all-features` 均通过。
- 根目录全量 Rust 测试中的真实浏览器测试仍属于 ignored 用例，输出为 `0 passed; 1 ignored`。五条真实浏览器场景与两条竞争场景的 PASS 来自本轮在同一主检出单独执行 `--ignored` 的命令，未由 `npm run verify` 再次运行。

独立检视及最终工作区差异核对完成之前不归档 OpenSpec 变更。

## 未发消息空会话独立验收（2026-10-27）

默认边界场景会先发送消息，不能单独证明“创建、进入、离开后从未发送消息”的空会话也会清理。为此补充了第六个真实浏览器场景 `untouched-empty-session-is-cleaned`，并将其加入 Rust harness 的必需场景清单。

该场景通过真实 Web 创建入口进入正式会话，再由 SQLite 读取确认会话已落库、已分配目录，消息、轮次和交互均为 `0`。导航回目录并释放 viewer 后，以会话注销时记录的 `retentionStart` 计算 30 分钟期限。期限前 `1ms` 会话仍为 live 且出现在目录中；恰在期限时收到 `session.deleted`，留下 tombstone、从产品读取中消失，页面没有新增提示或原生对话框。

本次在 `D:/Project/acp-remote` 主检出执行：

- `node --check e2e/session-create-empty-cleanup-live.mjs`（工作目录 `clients/app`）：PASS。
- `npm run check`（工作目录 `clients/app`）：PASS，exit 0；102 个测试文件、1503 项测试通过，生产 Web export 成功。
- `cargo fmt --all -- --check`：PASS。
- `cargo test -p app --test session_create_empty_cleanup_browser -- --ignored --nocapture`：PASS，`1 passed; 0 failed`，23.59 秒。精确文件时钟 `1793040023149` 被用于新建未发消息会话；保存的注销 `retentionStart` 为 `2026-10-26T19:40:23.149Z`，对应期限 `1793045423149`。

真实浏览器仍为 Edge `Edg/156.0.4314.42`，六条必需场景全部 PASS，`pageErrors: []`、`skipped: []`、`cleanShutdownFenceConsumed: true`。`activation-first` 和 `deletion-first` 两条竞争顺序也 PASS。新增场景报告 `durationMs: 923`；本次整体测试耗时不等同新场景耗时。

随后校正了权威文档中残留的工作包状态和已实现端口说明：README、Sync、前端、产品、模块和存储文档不再把本次直接创建、激活、删除与 30 分钟清理描述为“待 WP”的能力。`docs/CORE_PORTS_AND_STORAGE.md` §5.10 已按实际 `core::ports` 签名登记，修正了 `ReclaimCandidate.delete_pending` 的漂移。根目录 `npm run check` 再次 PASS，合同漂移门禁报告 29 个 trait / 129 个方法签名与实现一致，v19 的 35 条 DDL 与实现一致。

## 生命周期并发与失败回滚补验（2026-10-27）

本节记录本轮继续实现后的实际结果；上文历史 PASS 不代表这些修改已经完成最终独立检视。

- 首条消息的存储屏障与删除受理原先在测试中互相等待。现用有界等待构造“输入先持有生命周期门”的交错，验证保存完成后旧空会话候选让路、正文可读且没有外部关闭。移除了不会用于正常产品路径的重复只读门和删除存储屏障。
- 修正了请求尚未取得执行标记就创建清理对象的问题，防止拒绝第二个请求时清除第一个请求的标记。消息保存期间连续两个竞争请求都必须被明确拒绝，原请求继续完成。
- 生命周期门的复制句柄现在共享同一次占用，最后一个持有者退出才允许下一操作；新增纯 core 测试分别覆盖先丢弃原句柄和先丢弃复制句柄。
- 删除执行完成后先释放自己的会话受理标记与派发屏障，再恢复消息派发。预检发现其他修改命令尚未结束时，不再把原 ready 资源错误降为 inactive 或交还容量。
- 删除受理不刷新保留期限；已确认释放但删除事务失败时保留同代待删除状态。新请求可以重试数据库事务，不能重复关闭 Agent；成功后移除运行期目录项。
- 满额时仍允许向已占名额的会话发送消息。确定未建立资源的创建失败使用预占回滚；普通输入先做授权、旧请求回放和存在性检查，再预占容量。失败输入只交还本次新增的预占，不能误交还原有占位。

主检出执行结果：

| 命令 | 实际结果 |
| --- | --- |
| `cargo check --workspace --tests` | PASS，exit 0 |
| `cargo test -p app --test session_lifecycle -- --test-threads=1` | PASS，26 passed，0 failed，32.25 秒 |
| `cargo test -p core --lib -- --quiet` | PASS，324 passed，0 failed；此结果在后续容量回滚修正之前取得，最终状态须看下方全量门禁 |
| `npm run check`（`clients/app`） | PASS，102 个测试文件、1503 项测试通过，生产 Web export 成功 |
| `npm run check`（仓库根目录） | PASS，包含合同、边界与 OpenSpec 严格校验 |
| `git diff --check`（仓库默认行尾配置） | PASS，exit 0 |

本轮修复期间曾运行失败的 core 单元测试：同会话并发提交、创建落账前事件排队、创建满额回收。修正后上述 324 项 core 测试全部通过。初次全量门禁还检出了合同字段漂移及 Clippy 问题；这些失败不计作 PASS，最终全量结果另记。

额外只读诊断发现的三项容量问题已修复并复核。关于任意手工取消 core future 的两项怀疑，经真实调用链核对后撤回产品缺陷结论：Sync 断线、撤销和连接替换不取消正在执行的 route；扫描器在执行删除时也不立即响应 shutdown。Daemon 关闭排空超时后可能强制取消，但下一次启动先把未决命令记为 `uncertain`，再丢弃旧运行期目录并给持久空会话重新播种保留期。本次只读诊断不替代规定的最终检视。

规定的 omp 内置 reviewer 未能启动：本机配置的 DeepSeek 模型两次返回 HTTP 402 `Insufficient Balance`。未获得最终独立 `VERDICT: approved`，因此任务 5.7、6.3 保持未勾选。尚未执行的平台、其他浏览器、真实 Codex/Oh My Pi 和正常设备身份跨重启续连等限制仍按上文保留。

### 本轮真实 Edge 重跑

`CARGO_TARGET_DIR=C:/Users/jack/AppData/Local/Temp/acp-wp5-target`、`CARGO_BUILD_JOBS=2` 下执行 `cargo test -p app --test session_create_empty_cleanup_browser -- --ignored --nocapture`：PASS，exit 0，`1 passed; 0 failed`，测试耗时 22.47 秒（编译另耗时 1 分 08 秒）。

Edge 版本 `Edg/156.0.4314.42`；六条必需浏览器场景全部 passed，`pageErrors: []`、`skipped: []`、`cleanShutdownFenceConsumed: true`。输出另有 `WP5_CROSS_LAYER_RACES=PASS activation-first deletion-first`。未发消息的空会话场景读库确认消息、轮次、交互均为 0，保留起点为 `2026-10-27T02:29:00.941Z`，终点为 `1793069940941`；终点前保留、精确终点删除且无提示。原始运行输出保存于 `C:/Users/jack/AppData/Local/Temp/acp-session-browser-20261027.log`。

全量 `npm run verify` 的一次运行已通过格式检查和全工作区 Clippy，但 Rust 测试编译中断：C 盘当时仅余约 0.9GB，无法写入构建 fingerprint。把该任务构建缓存移到 D 盘后，旧缓存中的 SQLite 生成文件仍无法读取；这两次环境失败不计作 PASS。随后使用新的 `D:/Project/acp-remote-verify-20261027` 构建目录重跑，关闭增量编译和 debug 符号以控制缓存体积；迁移构建环境本身未修改 Rust 工具链、功能开关、测试断言或仓库配置。最终结果必须以下续记录为准。

### 后续回归收口

实际创建路径补回已有的满额安全回收策略；新增测试覆盖“已有会话受查看保护而拒绝创建、解除保护、沿用同一请求重试后真实回收并创建”。创建结果合并还区分了 owner 和等待方的清理责任，结束时核对具体的 `Arc` 身份，防止旧 owner 的退出清理移除同请求号的新一轮创建。额外只读复核确认这两项修复未留下原发现的问题；不作为规定的最终 reviewer 结论。

随后全量运行曾在存储测试 `a_consumed_delete_generation_terminates_the_old_request_without_deleting_content` 失败。其夹具在删除受理后保存一条 Agent 消息，实际结果为 `Failed { blocker: NotEmpty }`，原断言却要求 `Protected`。按 §13.1/§13.2 的内容分类合同，将其重命名为 `content_arriving_after_delete_admission_terminates_the_old_request_without_deletion` 并精确要求 `NotEmpty`；终态事件引用、消息保留、没有 `session.deleted` 的断言继续保留。该用例单独重跑 PASS，`1 passed; 0 failed`。原失败输出保存在 `C:/Users/jack/AppData/Local/Temp/acp-session-verify-late-content-failure-20261027.log`，不计作全量通过。

### 最终固定代码的本地结果（2026-10-27）

后续固定 Rust 代码后，在 `D:/Project/acp-remote` 主检出重新执行以下检查，均实际结束并取得退出码 0：

| 检查 | 最终结果 |
| --- | --- |
| `npm run verify` | PASS：合同检查、格式检查、全工作区 Clippy（所有 target/feature，警告视为错误）、`cargo test --locked --workspace --all-features` 全部通过；日志汇总 142 组测试结果，1805 passed、0 failed、6 ignored |
| `cargo test -p app --test session_create_empty_cleanup_browser -- --ignored --nocapture` | PASS：1 passed、0 failed，22.25 秒；六个必需浏览器场景全部 passed，activation-first / deletion-first 两种竞争顺序全部 PASS |
| `git diff --check` | PASS，exit 0 |
| 受保护的 `docs/evidence/omp-acp-tool-order.md` | 磁盘原始字节与 HEAD 完全相同；本轮没有修改该文件 |

最终两条 Rust 命令均设置 `CARGO_TARGET_DIR=D:/Project/acp-remote-verify-20261027`、`CARGO_BUILD_JOBS=2`、`CARGO_INCREMENTAL=0`、`CARGO_PROFILE_DEV_DEBUG=0`、`CARGO_PROFILE_TEST_DEBUG=0`。该运行未更改仓库中的构建配置。最终浏览器仍为 Edge `Edg/156.0.4314.42`，`pageErrors: []`、`skipped: []`、`cleanShutdownFenceConsumed: true`。

最终原始输出分别保存于 `C:/Users/jack/AppData/Local/Temp/acp-session-verify-final-20261027.log` 和 `C:/Users/jack/AppData/Local/Temp/acp-session-browser-final-20261027.log`。全量命令中的 6 个 ignored 用例不计作执行通过；本变更要求的真实浏览器验收来自紧随其后显式运行 `--ignored` 的单独命令。此前前端检查的 102 个文件、1503 项测试与生产 Web export 的 PASS 仍适用，本轮之后没有再修改前端代码。

当前实现与本地验证已完成；规定的 omp 最终独立检视仍因模型余额不足未执行，任务 5.7、6.3 保持待办，不提交或归档本变更。额外只读复核不能替代该门禁。


### 最终独立检视与交付补验（2026-10-27）

通过 omp 内置 reviewer 完成最终只读检视，返回 `VERDICT: approved`，无 CRITICAL/MAJOR，任务 5.7、6.3 已完成。报告为 `agent://FinalReview` / `history://FinalReview`，宿主会话为 `C:/Users/jack/.omp/agent/sessions/--D--Project-acp-remote--/2026-10-27T14-25-25-490Z_01a17dc0-6df2-711d-9254-3211d16aeb7e.jsonl`。检视核实真实组件、精确期限、重启、两种竞态、最终日志及受保护文档无实际差异，没有重跑测试。此前余额不足与待办结论为历史状态，已由本次检视收口。

三项 MINOR：前端原日志早于最后改动；最终 Rust 日志缺少显式退出码尾行，reviewer 从完整成功链推断通过；删除处理器存在不影响行为的死分支。本轮未改生产代码。针对第一项，最终工作区重新执行 `clients/app` 的 `npm run check`，实际退出码 0，生产导出完成；日志 `C:/Users/jack/AppData/Local/Temp/acp-session-frontend-final-20261027.log` 末尾写入 `REAL_EXIT_CODE=0`。其余两项作为非阻断建议保留。

九份增量规范逐项合并并核对；旧 `web-session-preparation` 的全部 16 条要求被移除，其主规格与 Purpose 删除，其他既有主规格 Purpose 保持不变。历史归档保留，可用 `git restore --source=a3bae5e4 -- openspec/specs/web-session-preparation/spec.md` 恢复旧规范用于查阅。

### PR #194 Linux 启动证明修复（2026-10-28）

两轮 Linux CI 均在 `an_attach_failure_never_publishes_a_clean_fence` 失败：启动子进程后 attach 故障的清理在 Unix 上可确认退出，旧分支因此返回 `SpawnFailed`，被错误归类为从未启动。按既有权威合同，spawn 后的启动失败即使清理完成也必须永久否决 clean fence。现于清理 await 前登记否决，清理完成与否都返回 `SpawnCleanupUnconfirmed`；未确认清理时继续保留资源所有权。测试断言与平台安全判据均未放宽。

omp 内置 reviewer 新实例 `agent://ReviewSpawnFenceFix` / `history://ReviewSpawnFenceFix` 返回 `VERDICT: approved`，无阻断项。实际检查：agent-host lib 74 passed；supervision 19 passed；`npm run verify` exit 0；真实 Edge 验收 1 passed、0 failed，21.58 秒，六场景与两种竞态全部通过。全量与浏览器日志分别为 `C:/Users/jack/AppData/Local/Temp/acp-pr194-startup-verify.log`、`C:/Users/jack/AppData/Local/Temp/acp-pr194-startup-browser.log`，末尾均有 `REAL_EXIT_CODE=0`。后续 Linux CI 已越过该失败点。

### PR #194 迁移测试夹具权限修复与流程调整（2026-10-28）

后续两轮 Linux CI 在 `migration_v19` 的三项测试报 `InsecurePermissions`。测试用 create_dir_all 手工建立旧附件目录，Unix 默认权限允许其他用户读取，未满足既有私有目录合同。仅将测试夹具 attachments 目录设为 0700；生产权限检查、迁移实现和所有断言保持不变。本机 `cargo test -p storage-sqlite --test migration_v19` 实际通过，4 passed、0 failed；`cargo fmt --all -- --check` 通过。Linux 结果以修复推送后的 PR 检查为准。

用户明确要求去掉 omp 独立检视并与 omp 解绑。当前执行按更新后的 WORKFLOW 进行，不再要求专用 reviewer；上述历史检视记录保留，不继续形成工具依赖。工作流、OpenSpec 指引与计划模板已同步为工具无关，移除仓库内专用宿主配置。根 `npm run check` 实际通过。先前追加记录因 PowerShell 管道编码变成问号，本节恢复其原意；没有改写更早的原始验收记录。
