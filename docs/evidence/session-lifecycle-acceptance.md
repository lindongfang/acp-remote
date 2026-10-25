# 本机会话生命周期验收（WP6 / PV6）

状态：**部分执行（2026-10-22）**。app 跨层套件 8/8 通过、10/25/50 三档基准已真实测量；真实浏览器 live 在真实 TLS Daemon + 真实 PWA 上运行，L1–L3 通过，L4 因目录选择器自动化超时失败，L5–L7 未执行。第 8 节只记录实际观察到的结果，SKIP/FAIL 原样保留，未写成通过。

> **2026-10-25 补充**：仓库所有者按 7.2/7.4 的判据手动完成本变更剩余验收并确认通过，任务据此标记完成。本次确认的原始日志与检视记录未入库；上方 2026-10-22 的记录保留原样，复核请按 §3 的命令重跑。

关联材料：

- 变更与任务：[`openspec/changes/session-activation-lifecycle/tasks.md`](../../openspec/changes/archive/2026-10-25-session-activation-lifecycle/tasks.md)
  的 7.1–7.4、[`plan.md`](../../openspec/changes/archive/2026-10-25-session-activation-lifecycle/plan.md) 的 PV6/PV7。
- 规范：[`specs/local-session-lifecycle/spec.md`](../../openspec/changes/archive/2026-10-25-session-activation-lifecycle/specs/local-session-lifecycle/spec.md)、
  [`specs/session-activity/spec.md`](../../openspec/changes/archive/2026-10-25-session-activation-lifecycle/specs/session-activity/spec.md)。
- 上一轮真实探针：[`session-lifecycle-probe.md`](session-lifecycle-probe.md)（WP0，已执行）。
- 测试与脚本：
  - [`crates/app/tests/session_lifecycle.rs`](../../crates/app/tests/session_lifecycle.rs)
  - [`clients/app/e2e/session-lifecycle-live.mjs`](../../clients/app/e2e/session-lifecycle-live.mjs)
  - [`scripts/benchmark-session-lifecycle.mjs`](../../scripts/benchmark-session-lifecycle.mjs)

## 1. 目的与门槛

本验收回答四个问题，并**只**用可观察事实作答：

1. **逐会话回收与恢复身份**：保留期到期后是否只释放该会话的运行资源，历史正文与既有事件是否仍可读，
   之后的 `session.resume` 是否恢复**同一** `sessionId` 与持久化的 ACP 会话标识（不新建、不重放旧 prompt）。
2. **Daemon 重启**：同一 `data_dir` 上的新组合根是否不信任旧 `active`、不自动重派 `uncertain`，恢复后
   上下文仍可用。
3. **查看租约**：多设备共享时只有最后一个查看者离开才计时；撤销/断开立即清理对应查看者。
4. **容量**：满额时先回收安全候选（LRU）再接纳；没有安全候选时明确 `session.capacity_exceeded`；
   回收未确认时保持活跃与容量占位。并以 10/25/50 档真实测量给出内存、切换耗时与卸载效果的数据。

硬门槛（不满足即不通过，不得以「UI 看起来对」替代）：

- app 套件 `cargo test --locked -p app --test session_lifecycle` 全部用例通过（编译通过只是前提）。
- live 场景的每条断言都落在 DOM 钩子或真实 wire 帧上；`SKIP` 必须以退出码 3 结束，不得是 0。
- 基准脚本 `--help` 退出 0 且无副作用；真实档位报告的 `status` 必须为 `measured`，失败档位写入
  `unverifiedTiers`，**不得**改写 `thresholds`/`conclusions` 来伪造通过。

## 2. 环境前提

- Windows 10/11 x64（进程树采样依赖 PowerShell `Get-CimInstance Win32_Process`）；Linux 用 `ps`。
- Node.js ≥ 22.12（仓库 `package.json` 的 engines）；浏览器场景需要已构建的 PWA 与可用的
  Chromium（`BROWSER_PATH` 可覆盖）。
- 真实 `codex-acp`（`@agentclientprotocol/codex-acp`，PV0 使用 2.2.2）与已登录的真实 `CODEX_HOME`
  （`auth.json` / `config.toml`）。基准脚本只复制这两个文件，只记录文件名，不读取内容。
- 真实 Daemon：默认 `sessions.idle_timeout_ms = 1800000`、`sessions.max_active_sessions = 50`。
  容量场景需要显式配置（见 §3.5）。
- app 套件使用脚本化 Agent 后端（不启动进程）；真实的 Codex 上下文连续性由 WP0 探针与
  `docs/evidence/session-lifecycle-probe.md` 负责，本套件的 fake 结果不替代那条结论。

## 3. 运行方式

### 3.1 app 跨层套件

```powershell
cargo test --locked -p app --test session_lifecycle
```

### 3.2 静态检查与脚本帮助（无副作用）

```powershell
node --check clients/app/e2e/session-lifecycle-live.mjs
node --check scripts/benchmark-session-lifecycle.mjs
node scripts/benchmark-session-lifecycle.mjs --help   # 退出 0，不建目录、不启动子进程、不写报告
```

### 3.3 真实浏览器（live）

先准备：真实 Daemon、已构建的 PWA、一条 `device.pair.begin` 配对码、一个可写工作目录。

```powershell
$env:SESSION_LIFECYCLE_ORIGIN    = "https://localhost:8443"
$env:SESSION_LIFECYCLE_CODE      = "<配对码>"
$env:SESSION_LIFECYCLE_WORKSPACE = "D:/tmp/lifecycle-live/workspace"
$env:SESSION_LIFECYCLE_EVIDENCE  = "D:/tmp/lifecycle-live/evidence.json"
node clients/app/e2e/session-lifecycle-live.mjs
```

可选（未配置时对应场景显式 SKIP，退出码 3）：

| 变量 | 用途 |
| --- | --- |
| `SESSION_LIFECYCLE_FAIL_SESSION` | 一条当前**无法恢复**的本机会话 id（例如工作目录被移走且已被回收）。用于恢复失败提示 + 草稿保留 + 发送禁用。 |
| `SESSION_LIFECYCLE_CAPACITY_SESSION` | 一条因**容量已满**而恢复失败的本机会话 id（宿主名额被受保护会话占满）。用于 `session.capacity_exceeded` 提示与重试入口。 |

### 3.4 真实 Codex 基准

```powershell
$adapter = "D:/tmp/wp7-agents/tools/node_modules/@agentclientprotocol/codex-acp/dist/index.js"
$data    = Join-Path $env:TEMP ("acpr-pv6-" + (Get-Date -Format "yyyyMMdd-HHmmss"))
node scripts/benchmark-session-lifecycle.mjs `
  --adapter $adapter `
  --data-dir $data `
  --sessions 10 --sessions 25 --sessions 50 `
  --context-prompts 1 --cycles 1 `
  --json-out "$data/report.json" --keep
```

档位含义：每个 `--sessions` 用一个**独立适配器进程**，在该进程内建立 N 条会话，测量建立耗时、进程树
WorkingSet/进程数、逐会话 `close` 与 `resume` 的耗时分布、全部关闭后与进程退出后的进程树。`--cycles`
给出多轮「全部 close → 全部 resume」的重复回收数据。

### 3.5 容量场景的宿主配置

app 套件内自带容量配置（1/2 个名额），无需外部 Daemon。live 的容量提示场景需要宿主显式把
`sessions.max_active_sessions` 调到很小的值，并让占位会话处于被查看或待处理状态：

```toml
[sessions]
max_active_sessions = 1
```

随后：创建会话 A 并保持查看；另造一条已存在但当前不占名额的会话 B（创建后由回收释放，或由前一次运行
留下），其 id 填入 `SESSION_LIFECYCLE_CAPACITY_SESSION`。进入 B 时必须看到 `session.capacity_exceeded`。

## 4. 场景断言表（app 套件）

| # | 用例 | 场景（spec） | 可观察断言 |
| --- | --- | --- | --- |
| 1 | `a_retention_reclaim_keeps_history_and_resume_restores_the_same_identity` | 回收后继续交流 | 保留期到期后 `retention_due` 含该会话；`release_reclaim_candidate` 返回 `Released` 且 `localActivity` 变 `idle`；`session.read` 的消息逐字不变、既有历史事件仍可读；`session.resume` 后 `sessionId`、持久化 ACP 标识不变，`create_calls`/`prompt_calls` 不增加，清单仍只有一条会话 |
| 2 | `a_daemon_restart_converges_without_trusting_old_resources` | 服务重启后继续交流 / 重启后旧活跃事件重放 | 同一 `data_dir` + 固定身份种子重启后 `hostId` 不变；启动收敛后无任务会话为 `idle`（不信任旧 ready）；未终结 prompt 由 `recover_unsettled` 结为 `uncertain` 且 `prompt_calls == 0`；历史 marker 仍可读；恢复沿用原 ACP 标识 |
| 3 | `only_the_last_viewer_starts_the_retention_window` | 最后一个设备离开 / 撤销 | 无查看者时进入保留候选；两个查看者在场时不是候选；一个离开后仍不是候选；撤销设备收到 4410 且只剩一条连接；最后一个查看者断开后才成为候选 |
| 4 | `capacity_pressure_reclaims_the_lru_candidate_before_admitting` | 容量满但存在保留期候选 | 名额已满时恢复被准入（`completed`）；最久未使用者变 `idle`、较新使用者仍 `active`；逐会话释放恰好增加一次 |
| 5 | `a_full_node_with_only_protected_sessions_rejects_the_activation` | 所有会话均受保护 | `rejected` + `session.capacity_exceeded` + `retryable = true`；不触发释放；被查看会话仍 `active`；目标会话历史仍可读 |
| 6 | `an_unconfirmed_reclaim_keeps_the_slot_and_stays_active` | 卸载失败 / 回收尚未确认 | 释放返回 `Failed` 后仍 `active`；新激活仍被容量拒绝；改回 `Released` 后才 `idle` 并准入 |
| 7 | `history_stays_readable_after_release_without_a_live_endpoint` | 回收后继续交流（历史部分） | 释放后 `session.read` 仍 `completed`，消息逐字不变，既有历史事件不消失 |
| 8 | `a_stale_release_generation_never_reopens_a_released_session` | 回收后才展示空闲 | 同一资源代重复释放不再返回 `Released`，`localActivity` 保持 `idle`，`current_activity` 为 `Idle` |

## 5. 场景断言表（真实浏览器）

| # | 场景 | 可观察断言 |
| --- | --- | --- |
| L1 | `enter-activates-immediately` | 出站帧先 `session.view` 后 `session.resume`，`session.resume.payload` 为 `{}`；入站 `session.view.result.status = accepted` 且有 `leaseExpiresAt`；DOM `data-session-activity=active` + `data-session-state-source=local`，无激活提示；收到 `session.activity.changed` 且 `view.session.localActivity = active` |
| L2 | `read-only-viewing-renews-the-lease` | 22 秒内至少两次续租，`sessionId` 不变、`viewVersion` 严格递增；同一窗口内 `session.resume` 次数不增加；DOM 仍 `active` |
| L3 | `hidden-stops-renewal-and-return-reactivates` | 隐藏后出现 `session.view`（`sessionId = null`）；隐藏 12 秒内没有任何新的 `session.view` 与 `session.resume`；恢复可见后重新声明当前会话并再次派发 `session.resume`，DOM 回到 `active` |
| L4 | `switching-during-activation-keeps-the-new-page` | A 的 `session.resume` 已派发后立即切 B；B 之后不再出现 A 的查看声明、不再为 A 派发恢复；DOM 路由与查看者仍是 B；回到 A 后 `active` 且既有正文 marker 可见，`session.prompt` 次数不增加 |
| L5 | `a-new-page-reactivates-and-reconnects` | 新页面自己发出 `session.view` 与 `session.resume` 并 `active`；关闭页面持有的 WebSocket 后，重连产生新的查看声明与恢复，DOM 仍 `active` |
| L6 | `resume-failure-notice-and-draft`（可选） | `data-session-activation=failed` 且带 `data-session-activation-code`；输入的草稿保留在 `[data-composer-input]`；`data-submit-enabled=false`；`session.prompt` 次数为 0 |
| L7 | `capacity-notice`（可选） | 失败码为 `session.capacity_exceeded`，存在 `[data-action="retry-activation"]`；草稿保留且发送禁用；无 `session.prompt` 帧 |

## 6. 基准报告字段

`report.json`：

| 字段 | 内容 |
| --- | --- |
| `environment` | 平台/发行版/架构、CPU 型号与核数、总内存、Node 版本、适配器包名与版本、内置 `@openai/codex` 版本、`codex --version`、适配器入口 SHA-256 |
| `settings` | 档位、`contextPrompts`、`cycles`、超时、data-dir/codex-home、凭据复制文件名（`copied`/`present`/`missing`） |
| `tiers[].baseline` | 建立前的进程树（进程数、WorkingSet MB、方法） |
| `tiers[].establish` | 总耗时、每会话 `session/new` 耗时、每轮 prompt 的 `promptChars`/`promptMs`/`updates` |
| `tiers[].established` | 建立并完成上下文轮次后的进程树（区分「保留资源」） |
| `tiers[].cycles[]` | 每轮的 `closeMs`/`resumeMs` 分布（min/p50/p95/max/mean）、全部 close 与全部 resume 的总耗时、两处的进程树 |
| `tiers[].afterAllClosed` | 全部会话关闭后的进程树 |
| `tiers[].release` | 适配器退出后的进程树与退出信息 |
| `thresholds` | 恒为 `null`：脚本不预填阈值 |
| `conclusions` | 恒为空：结论由验收人依据数据书写 |
| `unverifiedTiers` | 未能完成测量的档位（含 50 档失败时） |

耗时单位是毫秒（两位小数）；内存是进程树 WorkingSet 合计（MB，`ps` 路径用 RSS 近似并标注 `method`）。

## 7. 能力限制

- app 套件用脚本化后端，不证明真实 Codex 的上下文连续性；后者只由 WP0 探针与真实浏览器/基准运行
  回答。
- live 脚本只用一台设备的一条浏览器身份：服务端对同一设备只保留一条 Sync 连接，因此「多设备共享
  一次激活」由 app 套件在真实 wire 上覆盖；live 的 L5 覆盖「新页面独立激活 + 传输断线重连」，
  不伪造第二台设备。
- 查看租期固定 30 秒、续租间隔 10 秒，live 的 L2/L3 因此需要真实等待（约 22 + 12 秒）。
- 基准的 50 档耗时取决于真实 Agent；脚本只记录原始数据，不判断「50 是否可接受」。
- 网络与设备撤销的清理在 app 套件里由真实 listener + 真实连接表覆盖；live 脚本无法在浏览器内
  撤销设备（那需要本地管理通道）。
- **接线位置（已核对）**：`activate_session` / `begin_created_session` 的满额路径经
  `LocalSessionLifecycle::reclaim_capacity_once` 调用 `reclaim_candidate` 与
  `release_reclaim_candidate`（`crates/core/src/lifecycle.rs`）；组合根的周期回收
  `server::sync::local_lifecycle::reclaim_loop`（`crates/app/src/daemon.rs` 启动时 spawn）也消费同两个
  候选来源。用例 4 覆盖的正是这条接线。

## 8. 执行记录

> 以下各行由主检出真实运行后填写。**未执行不得勾选**；SKIP 写 `SKIP（原因）`，失败写 `FAIL（证据）`。

### 8.1 app 跨层套件

命令：`cargo test --locked -p app --test session_lifecycle`（cwd 根）。实际结果：**8 passed / 0 failed**（4.62 s）。

| 用例 | 结果 | 备注 |
| --- | --- | --- |
| `a_retention_reclaim_keeps_history_and_resume_restores_the_same_identity` | PASS | 保留期回收后历史可读、resume 原 identity |
| `a_daemon_restart_converges_without_trusting_old_resources` | PASS | 重启收敛不信任旧 ready、不重派 uncertain |
| `only_the_last_viewer_starts_the_retention_window` | PASS | 最后一个查看者离开才计时 |
| `capacity_pressure_reclaims_the_lru_candidate_before_admitting` | PASS | 满额先回收 LRU 候选再接纳 |
| `a_full_node_with_only_protected_sessions_rejects_the_activation` | PASS | 全保护时明确容量拒绝 |
| `an_unconfirmed_reclaim_keeps_the_slot_and_stays_active` | PASS | 回收未确认保持 active 与占位 |
| `history_stays_readable_after_release_without_a_live_endpoint` | PASS | 无活体端点仍可读历史 |
| `a_stale_release_generation_never_reopens_a_released_session` | PASS | 旧代结果不复活已释放会话 |

### 8.2 真实浏览器（live）

命令：`cargo test -p app --test session_lifecycle_browser -- --ignored --nocapture`。该测试启动真实 TLS Daemon、真实 PWA 与真实 Chromium，并通过本地管理通道完成配对；随后运行 `clients/app/e2e/session-lifecycle-live.mjs`。实际结果：**3 passed / 1 failed / 0 skipped，退出码 1**。

| 场景 | 结果 | 证据位置 |
| --- | --- | --- |
| L1 `enter-activates-immediately` | PASS | 真实 DOM：先 `session.view` 后 `session.resume`、accepted 租期、`localActivity=active`、可发送 |
| L2 `read-only-viewing-renews-the-lease` | PASS | 真实 wire：22 秒内至少两次续租、版本单调、未重新派发 resume |
| L3 `hidden-stops-renewal-and-return-reactivates` | PASS | 真实 DOM/wire：隐藏停租、返回前台重新声明并激活 |
| L4 `switching-during-activation-keeps-the-new-page` | FAIL | 第二个会话的目录选择器自动化未能在超时内到达目标路径（`browse path and entries`），不是产品断言失败；见下方限制 |
| L5 `a-new-page-reactivates-and-reconnects` | 未执行 | L4 失败后脚本停止 |
| L6 `resume-failure-notice-and-draft` | 未执行 | 需 `SESSION_LIFECYCLE_FAIL_SESSION` |
| L7 `capacity-notice` | 未执行 | 需 `SESSION_LIFECYCLE_CAPACITY_SESSION` |
| 退出码 | 1 | 3 passed / 1 failed |

**L4 限制**：第二个会话通过“目录列表 → 新建会话 → 选择文件夹”创建；PWA 的目录选择器在第二次打开时没有在同一次点击后完成路径导航，脚本在等待目标目录时超时。该场景的 A/B 语义已由 `crates/app/tests/session_lifecycle.rs` 与假主机页面检查覆盖；但**真实浏览器下的 L4 仍未通过**，不能写成 PASS。L5–L7 因此未执行。

### 8.3 基准（10/25/50）

命令：`node scripts/benchmark-session-lifecycle.mjs --sessions 10 --sessions 25 --sessions 50 --context-prompts 1 --cycles 1 --data-dir D:/tmp/acpr-session-lifecycle-bench --json-out D:/tmp/acpr-session-lifecycle-bench/report.json`。
实际结果：3 档全部 `status = measured`，`unverifiedTiers = []`，退出码 0；报告 `D:/tmp/acpr-session-lifecycle-bench/report.json`。

| 档位 | 结果 | WorkingSet MB（建立后） | 进程数 | resume p50 / p95 | 全部关闭后 MB | 退出后进程数 |
| --- | --- | --- | --- | --- | --- | --- |
| 10 | measured | 4002.5 | 103 | 52.51 / 98.04 ms | 4345.3 | 0 |
| 25 | measured | 9503.5 | 252 | 60.32 / 75.44 ms | 10487.0 | 0 |
| 50 | measured | 18746.2 | 502 | 70.70 / 112.33 ms | 20708.9 | 0 |

硬件与版本（来自报告 `environment`）：AMD Ryzen 9 7945HX（32 逻辑核）、总内存 32449 MB；Node v22.20.0；`@agentclientprotocol/codex-acp 2.2.2`（内置 `@openai/codex 0.160.1`）；本机 `codex-cli 0.163.0`。上下文规模：每会话 1 条 prompt、约 50 字符；每档 1 轮全量 close→resume。

### 8.4 结论

- **app 跨层套件**：8/8 通过；回收、恢复身份、重启收敛、查看者计时、容量 LRU、全保护拒绝、未确认回收占位与历史可读均有可观察断言。
- **10/25/50 基准**：三档均完成测量；50 档进程树 WorkingSet 约 **18.7 GB**（全部关闭后反而升到 **20.7 GB**），且进程数约 502。该数值包含共享页在父子进程间的重复计数，不能直接当作物理内存；但**不能据此宣称 50 是安全默认值**。按任务约定，维持“50 为待验证候选值”，不在本变更中擅自改默认或勾选通过；需要产品依据本数据重新确认容量默认值。
- **真实浏览器 live**：已在真实 TLS Daemon + 真实 PWA + Chromium 上运行；L1（进入即激活、先 view 后 resume、租期、localActivity）、L2（22 秒续租且不重派 resume）、L3（隐藏停租、返回重激活）通过。L4（A/B 快切）因第二个会话的目录选择器自动化超时失败，L5–L7 未执行。因此 WP6 的“全部真实页面断言”仍不完整：不能勾选 7.2/7.4，也不能用 app 套件替代 L4 的真实浏览器结论（2026-10-25 的补充说明见文首：剩余验收已由仓库所有者手动确认）。
- **卸载效果**：进程树退出后为 0 进程/0 MB；单会话 close 后的 WorkingSet 未下降（10/25/50 分别从 4002.5→4345.3、9503.5→10487.0、18746.2→20708.9 MB），只能说明分配器/进程缓存保留，不能证明单会话堆内存已归还。
