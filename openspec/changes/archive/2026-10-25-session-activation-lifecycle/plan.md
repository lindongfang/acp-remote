# Plan

## Execution Model

- 遵循 docs/WORKFLOW.md：主 Agent 用 omp batch `task` 派发可并行工作包，每包独立 spawn；`isolated: true`，`task.isolation` 为 patch 模式且 `apply: false`。
- 交付 `<id>.patch` + `agent://<id>` + `history://<id>`，不自动提交或合入；主 Agent 按顺序 `git apply --check` 再 `git apply`。
- 每包由内置 reviewer 的另一次只读 spawn 检视，输入 patch 绝对路径和两份报告；无未闭环阻断项才合入，修复/复核按工作流使用新 spawn。
- 隔离目录无 target/node_modules；以下命令、真实 Agent 探针、E2E 全由主检出执行。隔离作者只完成静态检查和可复核交付，不声称运行仓库检查。
- 实施前冻结/核对现有未提交分页与滚动改动；若无法区分基线，先隔离保留，不覆盖用户修改。当前仅规划，不派发实现。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP0 | 真实 ACP 恢复、关闭、上下文及共享会话隔离前置验证 | none | wp-tester | `scripts/probe-session-lifecycle.mjs`, `docs/evidence/session-lifecycle-probe.md` | patch + 探针说明与待执行证据表 | 主检出 PV0；不支持则阻断 WP1–WP6 并修订设计 |
| WP1 | 生命周期 wire/端口/配置合同及文档对齐 | evidence:WP0 | wp-coder | `schemas/sync/v1/`, `fixtures/sync/v1/`, `compatibility/`, `scripts/check-contract-assets.mjs`（仅泛化 `transportFrames` 校验以容纳查看帧族，见下）, `docs/INITIAL_DESIGN.md`, `docs/MODULE_ARCHITECTURE.md`, `docs/FRONTEND_DESIGN.md`, `docs/SYNC_PROTOCOL.md`, `docs/CORE_PORTS_AND_STORAGE.md`, `docs/CONFIG_REFERENCE.md`, `docs/SECURITY_DESIGN.md`, `docs/ACP_COMPATIBILITY_MATRIX.md`, `GLOSSARY.md` | patch + 变更表/fixture/错误分类 | 主检出 PV1，完整合同最终 PV7 |
| WP2 | core 生命周期归约、查看租约、容量准入、命令合并及 owned 提交 | contract:WP1 | wp-coder | `crates/core/`, `crates/storage-sqlite/` | patch + 纯端口和事务/并发测试；若改 DDL 含升级测试 | 主检出 PV2 |
| WP3 | Agent 单会话 close/resume、真实释放与旧代隔离 | contract:WP1 | wp-coder | `crates/agent-host/`, `crates/acp-protocol/` | patch + 真实能力门控、进程树和映射测试 | 主检出 PV3、复跑 PV0 |
| WP4 | Sync 本机 resume/view 接入、兼容投影、app 配置/扫描/关闭装配 | code:WP2, code:WP3 | wp-coder | `crates/sync-protocol/`（含 WP1 已冻结合同的 Rust 镜像：`MessageType`/`CommandName`/`VIEW_TYPES`/`ErrorCode` 与 fixture 计数）, `crates/server/`, `crates/app/src/`, `crates/app/tests/{sync_commands_e2e.rs,support/sync.rs,node_link_e2e.rs}`, `crates/node-link-protocol/tests/session_resume_command.rs`（仅同步 catalog 断言）, `crates/core/src/broker.rs` + `crates/core/src/use_cases.rs`（accepted-only resume 受理与 create 容量预占）, `config.example.toml` | patch + wire/路由/装配及关闭测试 | 主检出 PV4、PV1 |
| WP5 | 前端可见性、查看保活、恢复及活跃展示 | contract:WP1 | wp-coder | `clients/app/src/`, `clients/app/app/`, `clients/app/e2e/page/` | patch + 平台/feature/UI 测试及页面 E2E 检查 | 主检出 PV5 |
| WP6 | 跨层故障/重启/容量验收及 10/25/50 基准证据 | code:WP4, code:WP5 | wp-tester | `crates/app/tests/session_lifecycle.rs`, `crates/app/tests/session_lifecycle_browser.rs`（真实 Daemon+PWA+Chromium 编排）, `clients/app/e2e/session-lifecycle-live.mjs`, `scripts/benchmark-session-lifecycle.mjs`, `docs/evidence/session-lifecycle-acceptance.md`, `docs/SESSION_LIFECYCLE_DISCUSSION.md` | patch + 可重复验收脚本与实测证据；未通过项保留未完成 | 主检出 PV6、PV7；真实 browser/computer E2E |

WP2、WP3、WP5 在 WP1 合同确定后可用同一 batch 并行派发；WP4 待 WP2/WP3 完成，WP6 最后派发。写范围无互相重叠；WP1 的现有前端文档以及 WP5 的已有用户修改需在派发基线中明确，不能视为允许覆盖。

WP1 实施时新增的唯一共享文件是 `scripts/check-contract-assets.mjs`：`transportFrames` 的旧校验假设每个协议只有一个附件帧族且 schema 必有 `$defs.frameType`；登记 `session.view` 帧族必须把它泛化为按 `(protocol, feature)` 唯一并允许非附件族。该文件不在其他工作包写范围内，后合入包无需因它复验；PV1 已包含 `check:assets`。

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 0 | WP0 | 必须先验证真实能力，不能让“假卸载”贯穿实现 | PV0，证据失败即停止依赖包 |
| 1 | WP1 | 冻结合约后其余包才能消费；无包间写重叠 | PV1；尚待实现的漂移项记录为未通过，最终 PV7 必须通过 |
| 2 | WP2 | core/存储接口先于入站装配 | PV2 |
| 3 | WP3 | adapter 满足 WP2 已冻结端口；与 WP2 无写重叠 | PV3、PV2、PV0 |
| 4 | WP4 | 装配两个后端工作包并实现 wire | PV4、PV1 |
| 5 | WP5 | 前端消费真实服务端，避免假主机替代唯一验收 | PV5、PV4 |
| 6 | WP6 | 收口跨层测试/证据、刷新访谈草稿入口 | PV6、PV7 |

若实施检视新增共享文件，先更新本表列出所有重叠包及后合入包复验，再重新派发；不得静默扩大 Write Scope。

## Project Verify

所有命令在仓库主检出执行，前端命令明确其工作目录；只在依赖齐备时认定通过。真实 Agent 探针可通过配置参数选择测试 profile，不能读取/修改用户生产会话或输出秘密。

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV0 | `node scripts/probe-session-lifecycle.mjs --help`，按该脚本说明在隔离测试数据目录运行真实 Codex ACP 探针；cwd 根 | 明确版本/能力；原身份 resume；close 后可恢复；A 卸载不影响 B；无历史重派；真实资源释放可验证。脚本由 WP0 创建，本次未执行 |
| PV1 | `npm.cmd run check:schemas`、`npm.cmd run check:commands`、`npm.cmd run check:features`、`npm.cmd run check:errors`、`npm.cmd run check:assets`、`npm.cmd run check:docs`；cwd 根 | 合同词表/schema/共享 fixture 与文档一致，无 Node Link 意外扩展 |
| PV2 | `cargo test --locked -p core -p storage-sqlite`；cwd 根 | 租约假时钟、无保护后的完整计时、任务/交互保护、并发名额、LRU 候选、故障与重启归约；持久事件先提交再发布 |
| PV3 | `cargo test --locked -p agent-host -p acp-protocol`；cwd 根 | 能力门控、close/resume、旧映射清理、会话隔离、Windows 进程树及超时资源收口 |
| PV4 | `cargo test --locked -p sync-protocol -p server -p app`；cwd 根 | 认证/撤销/旧 peer、命令终态、view 版本/过期/断开、历史可读和具体错误、配置默认与显式 0 |
| PV5 | `npm.cmd run typecheck`、`npm.cmd run typecheck:e2e`、`npm.cmd test`、`npm.cmd run export:web`；cwd `clients/app` | 前台/后台、命令恢复、草稿与输入门控、页面切换、多设备假主机及共享 fixture；导出成功不是 E2E 通过 |
| PV6 | `cargo test --locked -p app --test session_lifecycle`、`node clients/app/e2e/session-lifecycle-live.mjs`、`node scripts/benchmark-session-lifecycle.mjs --help`，按脚本说明运行三档基准；cwd 根 | 新测试/脚本由 WP6 创建；真实上下文两条路径、容量边界、可见性/多设备与资源释放；报告耗时分布及进程树内存，50 不可接受则重新确认 |
| PV7 | `npm.cmd run verify` 与 `npm.cmd run check`（仅在有新变更或失败时补跑）；cwd 根 | 合同/边界/漂移/OpenSpec/Rust 全绿；不得宣称 CI 供应链项已本地执行 |

## Code Review

每行均为独立于作者的新 reviewer spawn，输入对应 `<id>.patch` 的绝对路径、agent/history 报告。结果 `pass|changes-requested|blocked`；作者自评不计。

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP0 | reviewer | 探针不注入旧上下文、真实能力、资源释放和共享进程隔离证据 | 无未闭环 CRITICAL/MAJOR，失败门槛诚实保留 |
| WP1 | reviewer | 用户需求追踪、机器合同唯一性、旧 peer/远程隔离、端口纯度 | 无未闭环 CRITICAL/MAJOR |
| WP2 | reviewer | 互斥/并发名额、原子持久化、重启/uncertain、无 detached task | 无未闭环 CRITICAL/MAJOR |
| WP3 | reviewer | ACP 能力、原上下文、卸载真实效果、完整进程树和代隔离 | 无未闭环 CRITICAL/MAJOR |
| WP4 | reviewer | 认证与撤销、限流/租约、词表/投影、配置与后台任务关闭 | 无未闭环 CRITICAL/MAJOR |
| WP5 | reviewer | 页面不越层、可见性/切页竞态、草稿保留、旧 peer/远程不变 | 无未闭环 CRITICAL/MAJOR |
| WP6 | reviewer | 真实 E2E 和数据、50 候选不冒充结论、故障场景与可重现性 | 无未闭环 CRITICAL/MAJOR，未通过测试不被勾选 |

## Main E2E

```yaml
mode: required
```

- Web/PWA 使用 omp browser prelude，真实 Daemon 同源托管：进入/返回激活、只读保活、后台租约失效、双设备/页共享、A/B 快切、恢复中/失败/容量提示及草稿。
- 桌面/原生使用 omp computer prelude，按 `--config .omp/e2e.yml` 收紧审批：真实 Codex ACP 的回收/Daemon 重启后原上下文继续、旧任务不重派、共享会话 B 不受影响、Windows 进程树释放。
- 10/25/50 档基准使用独立测试数据目录，不触碰用户会话；分别记录保留资源与任务负载。只有截图、启动成功或脚本退出零不能判定通过；每个场景必须有可观察断言及证据路径。

## Risks

- WP0 能力门槛失败：停止依赖包，保存探针结论并更新 proposal/design/plan/tasks，不继续实现看似可用的空壳。
- 未提交基线可能变化：每批派发前重新核对 git diff，禁止重置用户文件；补丁上下文不匹配则重派而非强行覆盖。
- 合同先于代码会短暂触发漂移门禁：如实记录，最终整体 PV7 通过前不发布/归档。
- 本机缺少真实浏览器、adapter 或硬件条件：保留相应任务未完成；fake 测试不替代真实上下文与容量结论。
