# 独立持久化展示快照库与混合自适应历史恢复实施计划

## Execution Model

- 执行：主执行者统筹，在独立工作树 `.worktrees/deliver-session-transcript-cache`（本地分支 `feat/deliver-session-transcript-cache`，基线 `bc92997c`）推进；待编码前门禁登记为 `passed` 后，由 `coder` 按序完成 `WP1`–`WP3` 编码实现、Red Check、开发单测与实现文档同步，锁定受跟踪文件干净的单一本地集成 `commit SHA` 后停手，在 `WP4` 并行拉起独立 `tester` 执行正式验收与独立 `reviewer` 执行交付检视。
- 隔离：当前工作树与主工作区（`deliver-session-control-takeover`）物理隔离，Rust 编译与测试输出统一绑定独立目录 `D:/Project/acp-remote/.target-wt/deliver-session-transcript-cache`（`CARGO_TARGET_DIR`）；`WP1`–`WP3` 的 `Write Scope` 完全正交不重叠；`tester` 与 `reviewer` 均使用不继承 `coder` 开发对话的独立子会话，运行时测试使用专属临时 `data_dir` 与独立空闲端口。
- 门禁：严格执行两道独立门禁——① 编码前门禁（`node testing/suites/verify-suite.mjs --change deliver-session-transcript-cache` 与 `npm run check:spec` 全绿后，由独立 `reviewer` 一次性合审 `proposal.md`、`specs/`、`design.md`、`tests.md`、`plan.md`、`tasks.md` 并在 `## Code Review` 登记 `passed`）；② 集成交付门禁（固定干净受测 `commit SHA` 后，独立 `tester` 与独立 `reviewer` 并行验收与检视，阻断项零遗留闭环）。

## Parallel Assessment

- **跨变更并行隔离**：本变更在独立工作树 `.worktrees/deliver-session-transcript-cache` 与独立 `CARGO_TARGET_DIR` 中推进，且将 Web 状态横幅抽入 `management` 层新文件 `clients/web/src/management/transcript-cache-banner.ts`、协议采用向后兼容可选扩展，避免触碰 `deliver-session-control-takeover` 重点修改的队列/审批控制器与 `NodeRequestAuthority` 写门禁，最大限度降低后续分支合入冲突。
- **变更内可并行与串行划分**：`WP1`、`WP2`、`WP3` 的文件写范围互不重叠（实现文档拆分为 `WP1` 维护 `docs/implementation/transcript-cache-storage.md`、`WP2` 维护 `docs/implementation/transcript-cache.md`、`WP3` 维护 `docs/implementation/web-workbench.md`）；因 `WP2` 编译期直接调用 `WP1` 导出的 `TranscriptCacheStore` 与离线 `AcpViewProjector` 接口，且 `WP3` 需与 `WP2` 线上状态载荷做前后端联调构建，单工作树内按 `WP1 -> WP2 -> WP3 -> WP4` 串行合入；`WP4` 中独立 `tester` 与独立 `reviewer` 对同一只读 `commit SHA` 完全并行执行。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 完成 `SC1`、`SC3` 存储与配置底座（`STC-001`、`STC-003`、`STC-011`、`STC-012`、`STC-013`）：扩展 `acp-remote.toml` 可选 `[transcript_cache]` 配置（默认 30 天 / 10 GiB），复用既有 `sqlx::sqlite` 实现物理隔离的 `transcript-cache.sqlite3`（WAL、`secure_delete = ON`、Windows 独占 ACL、`protocol_version` 持久化、离线版本感知 `AcpViewProjector`、`content_digest` 防虚刷、30 天滑动过期、10 GiB LRU 淘汰、活跃与在途租约保护、`purge_deleted_native_session` 即时擦除及确定性 `shutdown()`） | none | coder | `app/src/lib.rs`、`app/src/config.rs`、`app/src/data_dir.rs`、`app/src/adapters/acp/shared_view_projection.rs`、`app/src/transcript_cache.rs`、`app/src/transcript_cache/*`、`docs/implementation/transcript-cache-storage.md` | 独立展示快照存储引擎、配置解析、离线投影构造、清理与擦除器、单测及存储实现文档 | `cargo test --manifest-path app/Cargo.toml --locked config`；`cargo test --manifest-path app/Cargo.toml --locked transcript_cache` |
| WP2 | 完成 `SC1`、`SC2`、`SC3` 节点自适应读取、恢复协调与关闭接线（`STC-003`–`STC-010`、`STC-014`、`STC-015`）：将 `TranscriptCacheStore::shutdown` 接入 `OwnedNodeSessionServices::shutdown` / `NodeCloseOperation`，实现实时/重放旁路落盘与故障告警降级，完成冷启动纯元数据配置准入与 `Authority` 来源登记、零名额首屏秒开快照、后台按 `NativeSessionKey` 单飞标准重放校准、满额/异常优雅降级、无重放 `session/resume` 前序快照衔接、纯内存/原生缺失只读兜底、客户端撤销阻断及 50 名额/30 分钟计时规则 | WP1 | coder | `app/src/domain/shared_view.rs`、`app/src/application/shared_views.rs`、`app/src/application/shared_views/*`、`app/src/adapters/acp/shared_view_capture.rs`、`app/src/adapters/acp/native_views.rs`、`app/src/adapters/acp/native_views/*`、`app/src/adapters/acp/native_dispatch.rs`、`app/src/application/native_dispatch/*`、`app/src/node.rs`、`app/src/node/lifecycle.rs`、`app/src/node/session_services.rs`、`app/src/node/native_views.rs`、`app/src/management_ws/session_commands.rs`、`app/tests/*`、`docs/implementation/transcript-cache.md` | 节点混合自适应读取与恢复协调接线、确定性关闭集成、Red Check 与 Rust L5/L6 验收测试及协调层文档 | `cargo test --manifest-path app/Cargo.toml --locked shared_view`；`cargo test --manifest-path app/Cargo.toml --locked transcript_cache`；`cargo clippy --manifest-path app/Cargo.toml --all-targets --locked -- -D warnings` |
| WP3 | 完成 `SC2`、`SC4` Web 协议、跨段无闪烁前缀补齐与状态交互（`STC-002`、`STC-005`、`STC-006`、`STC-007`、`STC-008`、`STC-009`、`STC-016`、`STC-017`）：实现 `closedWithOptional` 与 `SessionViewOpened.status` / `transcript_cache_state` 解析，实现 `SessionViewDisplay` + `Timeline` 离屏暂存与跨 `segmentId` 原位重绑前缀补齐/压缩裁剪，在 `management` 层新增 `transcript-cache-banner.ts` 供 `SessionViewDisplay` 与 `source-session-view.ts` 同层复用，保持单向无环分层、单文件 `<= 650` 行与浏览器夹具零改动 | WP2 | coder | `clients/web/src/management/session-protocol/primitives.ts`、`clients/web/src/management/session-protocol/view-and-approval.ts`、`clients/web/src/management/session-protocol.ts`、`clients/web/src/management/session-view-status.ts`、`clients/web/src/management/session-view-display.ts`、`clients/web/src/management/transcript-cache-banner.ts`、`clients/web/src/management/source-session-view.ts`、`clients/web/src/views/projection-collection.ts`、`clients/web/src/views/content-store.ts`、`clients/web/src/views/timeline.ts`、`clients/web/src/workbench/shared-view-controller.ts`、`clients/web/src/workbench/source-session-controller.ts`、`clients/web/tests/*`（不含只读 `clients/web/tests/browser/`）、`docs/implementation/web-workbench.md` | Web 协议扩展、跨流/跨段原位前缀对账时间线、`management` 层状态横幅模块、Vitest 套件及全仓联调构建通过 | `npm --prefix clients/web run check`；`npm --prefix clients/web test`；`npm --prefix clients/web run build`；`npm run check`；`npm run build:rust` |
| WP4 | 完成集成交付门禁（覆盖 `SC1`–`SC4` 全量 `STC-001`–`STC-017`）：锁定干净受测 `commit SHA`，并行执行独立 `tester` 正式验收与独立 `reviewer` 交付检视，闭环全部阻断项并回填 `plan.md` 摘要 | WP3 | tester + reviewer | `openspec/changes/deliver-session-transcript-cache/plan.md`、`openspec/changes/deliver-session-transcript-cache/tasks.md`（详细原始运行日志写入本地忽略目录 `testing/runs/deliver-session-transcript-cache/`） | 独立验收报告、独立代码检视报告及门禁通过状态回填 | `PV1`–`PV5` 完成执行与核验，阻断项清零且 `Code Review` 集成交付门禁登记为 `passed` |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 先稳定物理分库 `transcript-cache.sqlite3`（基于既有 `sqlx::sqlite`）、配置解析、离线 `V1`/`V2` 投影与清理/擦除器，供上层协调直接复用 | `cargo test --manifest-path app/Cargo.toml --locked config` 与 `transcript_cache` 存储/清理/ACL/关闭单测全绿 |
| 2 | WP2 | 接入节点启动/关闭句柄生命周期、冷启动配置元数据准入、共享视图秒开、后台单飞标准重放、50 名额/30 分钟计时解耦与 `session/resume` 衔接 | Rust 定向与集成测试、`cargo fmt --check`、`cargo clippy --all-targets --locked -- -D warnings` 全绿 |
| 3 | WP3 | 对接后端线上状态契约，落地前端跨 `segmentId` 离屏暂存零闪烁前缀对账与 `management` 层状态横幅，验证单向分层、650 行预算与浏览器夹具不变性 | `npm run check`、`npm run build:rust`、`npm run check:spec`、`git diff --check` 全绿 |
| 4 | WP4 | 在单一锁定集成 `commit SHA` 上并行完成独立验收与独立代码检视闭环（后续与主分支 `deliver-session-control-takeover` 汇合时复跑一次交叉回归与行数门禁） | `node testing/suites/verify-suite.mjs --change deliver-session-transcript-cache` 与 `npm run check:spec` 全绿 |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | 工作树根目录执行 `STC-001`、`STC-002` 静态架构扫描、`npm run check:spec`、`npm --prefix clients/web run check`、`npm --prefix clients/web test`、`npm --prefix clients/web run build` | `main.ts <= 250` 行、`session-protocol.ts <= 120` 行、`workbench/*.ts`、`session-protocol/*.ts`、`management/transcript-cache-banner.ts` 与 `management/source-session-view.ts` 全部 `<= 650` 行、DFS 零循环依赖与零跨层反向引用、Node 导入零 DOM 副作用、`FIXTURE-SHA256.md` 6 个夹具文件 SHA-256 零变动、Vitest 与 Vite 构建退出码 `0` |
| PV2 | 工作树根目录（绑定 `CARGO_TARGET_DIR`）执行 `STC-001`、`STC-003`、`STC-011`、`STC-012`、`STC-013` 对应的 Rust `--locked` 配置与快照存储测试套件 | `[transcript_cache]` 默认/边界解析、`transcript-cache.sqlite3` 物理隔离（`node.sqlite3` 零新表、零新增 Cargo 依赖）、Windows 独占 ACL、离线 `V1`/`V2` 投影还原、30 天 `content_digest` 防虚刷滑动清理、10 GiB 跳过活跃/在途租约 LRU 淘汰、`secure_delete` 即时擦除零明文残留及关闭后立即删目录全部通过 |
| PV3 | 工作树根目录（绑定 `CARGO_TARGET_DIR`）执行 `STC-004`–`STC-010`、`STC-014`、`STC-015` 对应的节点协议、冷启动配置核验、混合恢复、故障隔离、撤销阻断与容量计时集成测试 | 冷会话先校验目录配置再零名额首屏秒开、拒绝错误/过期 `selection`、后台单飞标准重放、50 满额/不可达优雅降级与手动重试、`resume_only` 无重放衔接（零快照文本重灌）、`pure_memory`/原生缺失只读兜底、快照故障不阻断审批/执行、客户端撤销即时 `4410` 阻断及 50 名额/30 分钟计时规则全部通过 |
| PV4 | 工作树根目录执行 `STC-016` Web 跨段离屏暂存重绑与横幅测试，以及 `npm run check`、`npm run build:rust`、`git diff --check` | 跨流/跨 `segmentId` 重绑期间单 `<section>` 零双视图闪烁、前缀一致 `<li>` 引用（`===`）与 `scrollY`/选区零跳动、压缩删改尾部精确裁剪、全仓 `fmt`/`clippy`/测试/二进制构建退出码 `0` |
| PV5 | 工作树根目录在专属隔离 `data_dir` 与隔离端口下执行 `STC-017` 真实 Windows 节点 + 真实 Codex ACP 适配器 + Web 端到端闭环探针 | 真实 Codex 会话落盘后重启节点首屏秒开本地快照，外部追加对话后后台自动重放无感补齐且退出后零残留进程/句柄；若本地环境缺少已授权真实 Codex 凭据或外部网络配额，按规则如实登记 `not-run` 及阻塞依据，绝不伪造通过 |

## Code Review

| Gate / Scope | Reviewer Agent | Review Focus | Pass Condition | Status | Reviewed Snapshot / Notes |
| --- | --- | --- | --- | --- | --- |
| 编码前门禁 | 独立 reviewer | 合审 `proposal.md`、`specs/`、`design.md`、`tests.md`、`plan.md`、`tasks.md`：用户确认依据完整性、物理分库与不伪造上下文安全红线、50 名额/30 分钟计时解耦、Web 650 行与夹具保护、27 个增量场景 100% 映射及断言真实性 | `verify-suite.mjs --change deliver-session-transcript-cache` 与 `npm run check:spec` 通过，且独立 reviewer 零未闭环阻断问题 | passed | 基线 `bc92997c7ed36924f1ff8d79a17bab24ede2a564`（含工作树 6 份规划产物）；独立 `reviewer` 第 2 轮增量复核确认 `RV-PRE-001`–`RV-PRE-004` 全部闭环（0 未关闭问题），详见 `testing/runs/deliver-session-transcript-cache/reviewer/pre-code-review.md` |
| 集成交付门禁 | 独立 reviewer | 审查受测集成 `commit SHA` 全量 `git diff`：协议与规范符合性、故障旁路与 Windows 句柄/进程树清理、密钥与敏感正文零泄漏、核心不变量 Red Check 证据、验收测试防假阳性及 `docs/pitfalls.md` 晋升核查 | 全部阻断问题在修复 commit 上经原提出者增量复查关闭 | passed | 锁定集成提交 `0dfbb4676a6a332bfdf3a3dcc04d057ef7af7679`；独立 `reviewer`（`a9a825d1`）静态代码审阅结论 `passed`（0 Critical / 0 Major / 0 Minor），详见 `testing/runs/deliver-session-transcript-cache/reviewer/code-review.md` |

## Main E2E

```yaml
mode: required
```

- **真实端到端入口（对应 `STC-017` / `PV5`）**：在 Windows 本机使用专属临时 `data_dir`、独立随机空闲 HTTPS/WSS 端口、隔离 `CARGO_TARGET_DIR` 与非用户默认的隔离 Codex 会话目录启动真实 `acp-remote` 节点与同源 Web，完成客户端配对、真实 Codex 会话多轮消息/工具执行、节点重启后冷会话首屏秒开 `transcript-cache.sqlite3` 快照、外部会话追加后后台自动标准重放无感补齐。
- **数据隔离与清理验证**：严禁读取或污染用户默认 `~/.codex` 或默认节点数据目录；结束（含异常退出）时通过 Windows Job Object 验证受管子进程数为 `0`、监听端口已释放，并在同一测试末尾立即删除临时 `data_dir` 验证 `node.sqlite3` 与 `transcript-cache.sqlite3` 文件句柄 100% 释放。若执行环境缺少真实 Codex 登录凭据或外部网络配额，按硬规则第 2 条如实标记 `STC-017` 为 `not-run` 并记录阻塞证据，不以受控模拟对端冒充真实 Harness。

## Risks

- **Windows 下双 `sqlx::sqlite` 连接池关闭顺序与临时目录文件锁风险**：若后台异步写队列或定时清理任务晚于 `TranscriptCacheStore` 关闭仍在访问 `transcript-cache.sqlite3-wal`，会导致节点退出后临时目录无法立即删除；通过复用带确定性 worker join 补丁的 `vendor/sqlx-sqlite`，在 `OwnedNodeSessionServices::shutdown` 先封存写通道、join 全部后台任务再关闭连接池，并在 `STC-003` 强制执行 `remove_dir_all` 断言消除。
- **后台自动重放与手动/并发打开的竞态风险**：多标签页同时打开同一冷会话若重复拉起标准重放会浪费名额并引发代际抖动；在 `TranscriptCacheCoordinator` 按 `NativeSessionKey` 实施单飞合并（Singleflight）共享同一在途重放任务。
- **Web 分层方向与 `source-session-controller.ts` 650 行红线（当前 637 行）风险**：将横幅辅助模块置于 `clients/web/src/management/transcript-cache-banner.ts`，供同层 `session-view-display.ts` 与 `source-session-view.ts` 复用并由上层 `workbench/*` 薄委托调用，既保持严格单向分层又使全部文件保持在 `<= 650` 行安全区间。
- **与并行变更 `deliver-session-control-takeover` 后续汇合时的默认只读门禁交互**：`takeover` 合入后向未接管会话发起 `input.register` 恢复输入需先调用 `session.control.takeover`；两分支汇合时按 `Merge Order` 补跑交叉回归测试对齐夹具。

## Handoff

- **独立工作树与基线**：工作树 `D:/Project/acp-remote/.worktrees/deliver-session-transcript-cache`，本地分支 `feat/deliver-session-transcript-cache`，父基线提交 `bc92997c`，构建目录 `D:/Project/acp-remote/.target-wt/deliver-session-transcript-cache`。
- **权威输入路径**：[proposal.md](proposal.md)、[design.md](design.md)、[tests.md](tests.md)、[tasks.md](tasks.md)、[session-transcript-cache](specs/session-transcript-cache/spec.md)、[structured-history](specs/structured-history/spec.md)、[native-session-recovery](specs/native-session-recovery/spec.md)、[session-resource-policy](specs/session-resource-policy/spec.md)、[AGENTS.md](../../../../AGENTS.md)、[硬规则清单](../../../../docs/hard-rules.md)、[总体设计](../../../../docs/architecture/overview.md)。
- **读写边界**：严禁触碰主工作区 `D:/Project/acp-remote` 根下的并行变更文件，严禁修改 `openspec/specs/**`、`testing/suites/**`（归档时原子同步）及只读浏览器夹具 `clients/web/tests/browser/`；详细运行与审查原始日志统一写入本地忽略目录 `testing/runs/deliver-session-transcript-cache/`。
- **验证入口**：见 `## Project Verify` 表中 `PV1`–`PV5`。

## Findings

| ID | Source | Severity | Commit SHA | Finding | Fix Commit | Status | Retest / Re-review |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `RV-PRE-001` | Pre-code Gate (`reviewer`) | `major` | `bc92997c` (working tree) | `design.md:130` 与 `tests.md:9` 引用 `rusqlite::Connection`，但 `app/Cargo.toml` 未引入 `rusqlite` 且受 `vendor/sqlx-sqlite`（`libsqlite3-sys` 唯一链接）与 `--locked` 约束，需改为复用既有 `sqlx` 独立连接池并确定性关闭 (`design.md`, `tests.md`) | working-tree | closed | 原提出者 `reviewer` Round 2 复核通过：全份 `design.md`/`plan.md`/`tasks.md`/`tests.md` 已清除 `rusqlite` 引用，统一复用 `sqlx::sqlite`（`vendor/sqlx-sqlite` + `--locked`）独立连接池与确定性 `shutdown()` |
| `RV-PRE-002` | Pre-code Gate (`reviewer`) | `major` | `bc92997c` (working tree) | `transcript_sessions` 缺少 `protocol_version`（`1`/`2`）导致重启后无实时 `Connection` 时离线 `AcpViewProjector` 无法保真还原 `V1`/`V2` 差异；且冷启动快照直读前需先经 `ReadConfigurationPort::resolve_read` / `resolve_source_read` 校验目录配置并向 `SocketViewAuthority` / `LocalViewAuthority` 注册 `NativeSourceId` (`design.md`) | working-tree | closed | 原提出者 `reviewer` Round 2 复核通过：`transcript_sessions` 已持久化 `protocol_version (1|2)` 并配套离线 `Version` 感知 `AcpViewProjector`（`STC-003` 覆盖 `V1`/`V2` 重启还原），且冷启动已前置 `resolve_read` / `resolve_source_read` 与 `Authority` 来源注册 |
| `RV-PRE-003` | Pre-code Gate (`reviewer`) | `major` | `bc92997c` (working tree) | `WP1` 与 `WP2` 在 `docs/implementation/transcript-cache.md` 上 `Write Scope` 重叠；`WP1`/`WP2` 遗漏 `shared_view_projection.rs`、`shared_views.rs`、`native_views.rs`、`native_dispatch.rs`、`node.rs`；`WP3` 将 `transcript-cache-banner.ts` 放在 `workbench/` 导致 `management/`（`SessionViewDisplay`、`source-session-view.ts`）无法在不违反 `STC-002` 单向分层的前提下复用，且 `WP3` 遗漏 `source-session-view.ts` (`plan.md`, `design.md`, `tasks.md`, `tests.md`) | working-tree | closed | 原提出者 `reviewer` Round 2 复核通过：`WP1`–`WP3` `Write Scope` 已完全互不重叠，Rust 模块根与投影文件已补齐，`clients/web/src/management/transcript-cache-banner.ts` 与 `source-session-view.ts` 严格满足 `STC-002` 单向分层与 650 行预算 |
| `RV-PRE-004` | Pre-code Gate (`reviewer`) | `minor` | `bc92997c` (working tree) | 建议在 `design.md` Decision 4.2 明确跨代际 `Seg_1 -> Seg_2` 原位重绑主 `Timeline` 实例以保持 `<li>` 引用同一性，并在 `STC-005`/`STC-010` 补充 `view.open_source` 错误/过期 `selection` 的冷快照拒绝断言 (`design.md`, `tests.md`) | working-tree | closed | 原提出者 `reviewer` Round 2 复核通过：`design.md` Decision 4.2 与 `STC-016` 已明确跨 `segmentId`（`Seg_1 -> Seg_2`）主 `Timeline` 原位重绑与 `<li>` `===` 保持，`STC-005` 已补充过期/不匹配 `selection` 零快照泄露负向断言 |

## Verification Results

- 受测版本：`0dfbb4676a6a332bfdf3a3dcc04d057ef7af7679`（独立工作树 `.worktrees/deliver-session-transcript-cache`，分支 `feat/deliver-session-transcript-cache`，父基线 `bc92997c`）
- 执行者与环境：独立 `reviewer`（`a9a825d1`）完成静态源码审查（`passed`）；独立 `tester`（`35366323`）在沙盒内因 `run_command`/`write_file` 权限审批超时（60s）仅完成静态核查，全部动态验证命令由主代理在同一干净提交 `0dfbb4676a6a332bfdf3a3dcc04d057ef7af7679` 上直接执行并记录（详见 `testing/runs/deliver-session-transcript-cache/`）。
- 验证命令及退出码：
  - `PV1`（`npm run check:spec`、`npm run check:web`、行数预算与零 `innerHTML`）：exit `0`（`openspec` 20+5 通过、`verify-suite.mjs` 17 用例通过、`tsc --noEmit` 0 错误、`vitest` 43 文件 / 1049 用例通过、`vite build` 成功；`main.ts` 62 行、`session-protocol.ts` 78 行、其余目标 TS 文件均 `<= 650` 行）
  - `PV2`（`clients/web/tests/browser/` 夹具不变性与隔离存储测试）：exit `0`（`git diff --name-only bc92997c..0dfbb4676a6a3dcc04d057ef7af7679 -- clients/web/tests/browser` 为空；`cargo test --lib transcript_cache` 14 passed；`cargo test --test transcript_cache_acceptance` 4 passed）
  - `PV3`（`app/src/storage.rs` `SCHEMA_VERSION == 15` 与 5 个真节点 HTTPS/WSS 验收）：exit `0`（`stc_003`–`stc_010`、`stc_014`、`stc_015` 全部通过，`node.sqlite3` 零 `transcript_*` 表）
  - `PV4`（WP2 Red Check 与全量检查/构建）：`red-check-wp2.log` 记录 RED 失败（`0 passed; 1 failed`）与恢复后 GREEN 通过（`1 passed; 0 failed`）；`cargo fmt --check`、`cargo clippy --all-targets --locked -- -D warnings`、`npm run build:rust`、`git diff --check` 均 exit `0`
  - `PV5`（`STC-017` 真实 Codex E2E）：`not-run`（当前隔离环境缺少已授权真实 Codex 凭据与配额，不以受控模拟对端冒充）

| Case ID | Executor | Commit SHA | Status | Evidence / Limitations |
| --- | --- | --- | --- | --- |
| `STC-001` | 主代理（复核 `tester` 静态检查） | `0dfbb4676` | passed | `config` / `data_dir` ACL / `transcript_cache::tests::stc_001`（`node.sqlite3` `SCHEMA_VERSION = 15`，零新表、零新依赖） |
| `STC-002` | 主代理 | `0dfbb4676` | passed | `clients/web/tests/transcript-cache-web.test.ts`、行数预算 `<= 650`、零 `innerHTML`、`FIXTURE-SHA256.md` 6 夹具零变动 |
| `STC-003`–`STC-006`, `STC-010`, `STC-014`, `STC-015` | 主代理 | `0dfbb4676` | passed | `cargo test --lib transcript_cache`（14/14）与 `cargo test --test transcript_cache_acceptance`（4/4）真节点 WSS + 公共 API 验收通过 |
| `STC-007`–`STC-009` | 主代理 | `0dfbb4676` | passed | `stc_007_and_stc_008_and_stc_009_resume_only_pure_memory_unrecoverable_and_purge` 真节点 WSS 通过 |
| `STC-011`–`STC-013` | 主代理 | `0dfbb4676` | passed | 30 天防虚刷滑动清理、10 GiB LRU 保护淘汰、`purge_deleted_native_session` WAL 擦除通过 |
| `STC-016` | 主代理 | `0dfbb4676` | passed | `STC-016` 跨 `segmentId` 离屏暂存、`<li>` `===` 引用保持、`scrollY`/Selection 与横幅降级重试通过 |
| `STC-017` | — | `0dfbb4676` | not-run | 当前隔离环境无真实 Codex 凭据与配额，未执行真实 Codex E2E |

- 未执行项与已知限制：`STC-017` 真实 Codex E2E 为 `not-run`；独立 `tester` 子代理受沙盒交互权限超时限制未能亲自运行命令；`npm run check:rust` 中 `cargo test --all-targets` 会在基线既有未对齐集成测试 `app/tests/acp_tool_projection_acceptance.rs` 编译失败（核心库 `--lib` 1055 项与 `transcript_cache_acceptance` 4 项均通过）。
- 问题关闭依据：编码前门禁 `RV-PRE-001`–`RV-PRE-004` 已经独立 `reviewer` Round 2 关闭；集成交付门禁 `0dfbb4676a6a332bfdf3a3dcc04d057ef7af7679` 经独立 `reviewer`（`a9a825d1`）静态审阅无新增阻断项（0 Critical / 0 Major / 0 Minor）。
