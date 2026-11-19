# Web 工作台控制器与会话协议层模块化实施计划

## Execution Model

- 执行：主执行者统筹，在编码前门禁（`passed`）通过后委派 `coder` 完成 `WP1`–`WP3` 实现、自测与实现文档同步；锁定单一本地集成 `commit SHA` 后，在 `WP4` 并行拉起独立 `tester` 执行正式验收与独立 `reviewer` 执行交付检视。
- 隔离：`tester` 与 `reviewer` 均使用不继承 `coder` 开发上下文的独立子会话；`WP1`–`WP3` 均聚焦 `clients/web/` 同一 TypeScript 编译单元与测试集，默认在当前工作分支串行推进以避免重复依赖安装与跨模块类型合并冲突；`WP4` 在同一只读受测 `commit SHA` 上并行验收与检视。
- 门禁：严格执行两道独立门禁——① 编码前门禁（`node testing/suites/verify-suite.mjs --change refactor-web-workbench` 与 `npm run check:spec` 通过后，由独立 `reviewer` 一次性合审 `proposal.md`、`design.md`、`tests.md`、`plan.md`、`tasks.md` 并登记 `passed`）；② 集成交付门禁（受跟踪工作区干净且固定 `commit SHA` 后，独立 `tester` 跑通 `RWW-001`–`RWW-009` 且独立 `reviewer` 检视全量 diff 零阻断闭环）。

## Parallel Assessment

- **可并行部分**：`WP1`（`clients/web/src/management/` 下的 `session-protocol/*` 与 `catalog-documents.ts` / `upload-transfer.ts`）与 `WP2` 中的无状态原语模块（`dom-primitives.ts`、`error-formatting.ts`）写范围完全正交，若使用独立工作树可并行起草；`WP4` 中独立 `tester`（执行 `RWW-001`–`RWW-009`）与独立 `reviewer`（审查集成 diff 与架构边界）互不修改受跟踪文件，完全并行执行。
- **串行部分及原因**：`WP2` 的共享视图/队列正文/会话队列控制器与 `WP3` 的其余领域控制器、顶层协调器 `management-workbench.ts` 及薄入口 `src/main.ts` 共享 `clients/web/src/workbench/` 宿主端口类型与 `tsconfig.json` 全量类型检查面，且 `WP3` 需将 `src/main.ts` 彻底切换至 `WP1`+`WP2` 产出的全部子模块后再统一运行去切片化回归测试，因此按 `WP1 -> WP2 -> WP3 -> WP4` 顺序合入最稳妥高效。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 完成 `SC2`（`RWW-003`、`RWW-004`）：将 `session-protocol.ts` 拆分为 6 个领域子模块并保留薄重导出入口（`<= 120` 行），从 `client.ts` 提取 `catalog-documents.ts` 与 `upload-transfer.ts` 且保持全部既有公共导出 100% 兼容 | none | coder | `clients/web/src/management/session-protocol.ts`、`clients/web/src/management/session-protocol/*`、`clients/web/src/management/client.ts`、`clients/web/src/management/catalog-documents.ts`、`clients/web/src/management/upload-transfer.ts`、`clients/web/tests/session-protocol.test.ts`、`clients/web/tests/management.test.ts`、`clients/web/tests/catalog-workspace.test.ts` | 协议子模块与上传/目录文档辅助模块拆分实现及导出兼容性单测 | `npm --prefix clients/web run check`；`npm --prefix clients/web test -- tests/session-protocol.test.ts tests/management.test.ts tests/catalog-workspace.test.ts` |
| WP2 | 完成 `SC1`、`SC3`、`SC4` 核心会话流与原语部分（`RWW-001`、`RWW-005`、`RWW-006`、`RWW-008`）：建立 `workbench/dom-primitives.ts`、`error-formatting.ts`、`shared-view-controller.ts`、`queued-body-controller.ts`、`session-queue-controller.ts`，完成 `management-error-messages.test.ts` 与 `session-view-main.test.ts` 去切片化迁移及焦点选区恢复单测 | WP1 | coder | `clients/web/src/workbench/dom-primitives.ts`、`clients/web/src/workbench/error-formatting.ts`、`shared-view-controller.ts`、`queued-body-controller.ts`、`session-queue-controller.ts`、`clients/web/tests/management-error-messages.test.ts`、`clients/web/tests/session-view-main.test.ts`、`clients/web/tests/workbench-dom-and-view.test.ts` | 核心无状态原语、共享视图/正文读取/会话队列控制器及去 `new Function` 切片单测 | `npm --prefix clients/web run check`；`npm --prefix clients/web test -- tests/management-error-messages.test.ts tests/session-view-main.test.ts tests/workbench-dom-and-view.test.ts` |
| WP3 | 完成 `SC1`、`SC3`、`SC4` 全量工作台收口（`RWW-001`、`RWW-002`、`RWW-005`、`RWW-007`、`RWW-008`、`RWW-009`）：实现配对/身份、设备信任、审批、容量释放、目录、来源会话控制器与 `management-workbench.ts` 协调器，将 `src/main.ts` 收敛至 `<= 250` 行，完成 `r3-source-session.test.ts` 去切片化与删除 `session-view-node.d.ts`、协调器生命周期单测与 `docs/implementation/web-workbench.md` 文档 | WP2 | coder | `clients/web/src/main.ts`、`clients/web/src/workbench/pairing-access-controller.ts`、`clients/web/src/workbench/device-trust-panel.ts`、`clients/web/src/workbench/approval-controller.ts`、`clients/web/src/workbench/capacity-release-controller.ts`、`clients/web/src/workbench/workspace-catalog-controller.ts`、`clients/web/src/workbench/source-session-controller.ts`、`clients/web/src/workbench/management-workbench.ts`、`clients/web/tests/r3-source-session.test.ts`、`clients/web/tests/session-view-node.d.ts`、`clients/web/tests/workbench-controllers.test.ts`、`docs/implementation/web-workbench.md` | 完整 12 模块 `workbench/` 体系、薄入口 `main.ts`、删除切片类型补丁、控制器边界测试及 Web 工作台分层实现文档 | `npm --prefix clients/web test`；`npm --prefix clients/web run build`；`npm run check`；`npm run check:spec` |
| WP4 | 完成集成交付门禁（覆盖 `SC1`–`SC4` 全量 `RWW-001`–`RWW-009`）：锁定干净受测 `commit SHA`，并行执行独立 `tester` 验收与独立 `reviewer` 检视，闭环全部阻断项并回填 `plan.md` 摘要 | WP3 | tester + reviewer | `openspec/changes/refactor-web-workbench/plan.md`、`openspec/changes/refactor-web-workbench/tasks.md`（详细原始日志写本地被忽略路径 `testing/runs/refactor-web-workbench/`） | 独立验收报告、独立代码检视结论及门禁通过状态回填 | `PV1`–`PV5` 全绿且 `Code Review` 集成交付门禁登记为 `passed` |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 先稳定 `src/management/` 协议与文档/上传辅助重导出边界，供上层控制器无感消费 | `npm --prefix clients/web run check` 与协议/客户端单测通过 |
| 2 | WP2 | 在稳定协议层之上建立 `workbench/` 基础原语与核心流/视图/队列控制器并率先消除前两份切片测试 | `npm --prefix clients/web test -- tests/management-error-messages.test.ts tests/session-view-main.test.ts tests/workbench-dom-and-view.test.ts` |
| 3 | WP3 | 补齐其余领域控制器与顶层协调器，切换 `src/main.ts` 为薄入口并消除最后一处 `main.ts` 切片测试及 `session-view-node.d.ts` | `npm --prefix clients/web test`、`npm --prefix clients/web run build`、`npm run check`、`npm run check:spec` |
| 4 | WP4 | 在单一锁定集成 `commit SHA` 上完成独立验收与独立代码检视闭环 | `node testing/suites/verify-suite.mjs --change refactor-web-workbench` 与 `npm run check:spec` |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | 仓库根目录 PowerShell 执行 `RWW-001`、`RWW-003`、`RWW-004` 文件清单与行数上限检查及导入零副作用测试 | `src/main.ts <= 250` 行、`src/management/session-protocol.ts <= 120` 行、`src/workbench/*.ts`（12 个模块）、`src/management/session-protocol/*.ts`（6 个子模块）、`catalog-documents.ts` 与 `upload-transfer.ts` 单文件均 `<= 650` 行，Node 环境下导入 `src/workbench/*.ts` 零 DOM 异常 |
| PV2 | 仓库根目录执行 `RWW-002` 与 `RWW-005` 静态依赖图谱（DFS 环路 + 分层方向）与反切片扫描脚本 | `clients/web/src/**/*.ts` 强连通环与自环数为 `0`、跨层反向引用数为 `0`；`clients/web/src/main.ts` 与 `clients/web/src/workbench/*.ts` 中 `product.status` 匹配数为 `0`；`clients/web/tests/*.ts` 中 `new Function`、`stripTypeScriptTypes` 及对 `../src/main.ts` 的函数体切片匹配数为 `0`（且 `session-view-node.d.ts` 已删除） |
| PV3 | 仓库根目录执行 `npm --prefix clients/web test`（覆盖 `RWW-003`、`RWW-004`、`RWW-005`、`RWW-006`、`RWW-007`、`RWW-008`） | Vitest 全量套件退出码 `0`，实跑测试文件与用例数大于重构前基线，代际围栏、断线 `data-stale="true"` 保留、逆序响应去重、焦点选区无滚动恢复与 `sessionStorage` 恢复断言全部通过 |
| PV4 | 仓库根目录执行 `RWW-009` 中 `FIXTURE-SHA256.md` 所列 6 文件与基线 `a69da69e5d1cc6af6a15b3aa393359429c45ba58` 的字节/SHA-256 核对及 `git diff --exit-code a69da69e5d1cc6af6a15b3aa393359429c45ba58 -- clients/web/tests/browser app clients/web/package.json` | 6 个独立浏览器夹具文件相对于重构前基线的字节数与 SHA-256 哈希 100% 不变，`clients/web/tests/browser/`、`app/` 与 `clients/web/package.json` 零改动 |
| PV5 | 仓库根目录依次执行 `npm --prefix clients/web run build`、`npm run check`、`npm run check:spec`、`git diff --check`（覆盖 `RWW-009`） | 全部命令退出码 `0`、零编译/静态检查/空白字符错误，Vite 生产入口正常产出构建包且 OpenSpec 规范与套件门禁全绿 |

## Code Review

| Gate / Scope | Reviewer Agent | Review Focus | Pass Condition | Status | Reviewed Snapshot / Notes |
| --- | --- | --- | --- | --- | --- |
| 编码前门禁 | 独立 reviewer | 核对 `proposal.md`、`design.md`、`tests.md`、`plan.md`、`tasks.md` 边界一致性、模块无环依赖设计、公共导出兼容契约、`FIXTURE-SHA256` 只读保护与 `SC1`–`SC4` 用例覆盖充分性 | `verify-suite.mjs --change refactor-web-workbench` 与 `npm run check:spec` 通过，且独立 reviewer 无未闭环阻断问题 | passed | 受审基线 `HEAD` `a69da69e5d1cc6af6a15b3aa393359429c45ba58`，经增量复核 `RV-PRE-001`–`RV-PRE-003` 均已闭环消除阻断，详见本地审查记录 `testing/runs/refactor-web-workbench/reviewer/pre-code-gate-review.md` |
| 集成交付门禁 | 独立 reviewer | 审查受测集成 `commit SHA` 全量 diff：模块内聚度与行数硬上限、`verbatimModuleSyntax` 合规、去切片单测真实性、生命周期资源释放完整性及零未授权行为变更 | 全部阻断问题在修复 commit 上经原提出者复核关闭 | pending | 首轮受审快照 `d6ac98d7f4f33149456472022d190d8543375227` 独立 reviewer 判定 `passed`（0 blocker / 0 major / 3 minor：`RV-DEL-001`–`RV-DEL-003`，记录于 `testing/runs/refactor-web-workbench/reviewer/delivery-gate-review-d6ac98d7.md`）；独立 tester 首轮检出 `TST-RWW-001`–`TST-RWW-006`（记录于 `testing/runs/refactor-web-workbench/tester/acceptance-d6ac98d7.md`），已完成代码与单测修复，正对修复提交执行增量复核与复测 |

## Main E2E

```yaml
mode: not-applicable
reason: pure-refactor
basis: 本变更声明 skip_specs: true，仅对 clients/web/src/ 内部 TypeScript 文件实施等价模块化拆分并将单元测试从源码字符串切片升级为真实模块直连，不新增或修改任何网络协议帧、节点后端逻辑、DOM 选择器契约或浏览器存储结构，且严格保护独立浏览器夹具字节级不变。
alternative_checks:
  - PV1-PV2 静态验证文件行数上限、导入期零 DOM 副作用、全量模块 DFS 零循环依赖及测试零 new Function 切片
  - PV3 运行 Vitest 全量协议、控制器生命周期、代际围栏竞态、焦点选区恢复与 sessionStorage 恢复测试套件
  - PV4-PV5 校验 FIXTURE-SHA256.md 6 个浏览器夹具字节哈希零改动并跑通 vite build、npm run check 与 npm run check:spec
```

## Risks

- **重绘焦点与光标选区回归风险**：仪表盘拆入 `ManagementWorkbench` 后若遗漏保存/恢复当前聚焦输入控件选区会破坏 WV-15 行为；通过在 `dom-primitives.ts` 固化 `captureActiveControlSelection` / `restoreActiveControlSelection` 并在 `RWW-008` 建立专项单测兜底。
- **跨控制器异步流与定时器泄漏风险**：页面隐藏、断线重连或身份重置时需同时中止共享视图、来源结果流、队列正文读取器与原始检查器；通过 `ManagementWorkbench` 统一收敛 `suspendForDisconnect`、`disposeAllSessionResources` 与 `resetAllSessionState` 生命周期入口并在 `RWW-007` 逐项验证。
- **`verbatimModuleSyntax` 类型重导出错误风险**：barrel 重导出若混淆值导出与类型导出会导致 `tsc --noEmit` 报错；所有类型符号强制使用 `export type` 并在每个工作包合入时执行 `npm --prefix clients/web run check`。

## Handoff

- **基线分支与工作区**：分支 `v2`，父基线提交 `a69da69e5d1cc6af6a15b3aa393359429c45ba58`（首轮集成快照 `d6ac98d7f4f33149456472022d190d8543375227`，已提交修复快照进入第二轮增量复核/复测）。
- **权威输入路径**：[proposal.md](proposal.md)、[design.md](design.md)、[tests.md](tests.md)、[tasks.md](tasks.md)、[AGENTS.md](../../../AGENTS.md)、[硬规则清单](../../../docs/hard-rules.md)、[总体设计](../../../docs/architecture/overview.md)。
- **读写边界**：`coder` 仅允许修改相应工作包声明的 `clients/web/src/`、`clients/web/tests/`（不含只读 `clients/web/tests/browser/`）与 `docs/implementation/web-workbench.md`；`tester` 与 `reviewer` 对受跟踪源码只读，详细运行日志输出至本地忽略目录 `testing/runs/refactor-web-workbench/`。
- **验证入口**：见 `## Project Verify` 表中 `PV1`–`PV5`。

## Findings

| ID | Source | Severity | Commit SHA | Finding | Fix Commit | Status | Retest / Re-review |
| --- | --- | --- | --- | --- | --- | --- | --- |
| RV-PRE-001 | reviewer | blocker | a69da69e5d1cc6af6a15b3aa393359429c45ba58 | `tests.md` `RWW-005` 与 `plan.md` `PV2` 要求全量 `clients/web/src/**/*.ts` 中 `"product.status"` 匹配数为 `0`（`design.md` 第 92 行亦称其为“已移除的动作字面量”），但真实代码中 `src/management/session-protocol.ts:1885` 的 `SHAPES` 仍定义有效线上协议动作 `"product.status": {}`（对应 Rust `app/src/management_ws/session_protocol.rs:120` 且由 `tests/session-protocol.test.ts` 验证），`src/management/source-session-view.ts:323` 说明文案亦包含 `product.status`；原 `r3-source-session.test.ts:1190` 仅约束工作台层不依赖 `product.status`。建议将 `design.md`、`tests.md` `RWW-005` 与 `plan.md` `PV2` 中 `"product.status"` 零匹配扫描范围精确收窄至 `clients/web/src/main.ts` 与 `clients/web/src/workbench/*.ts` | working-tree | closed | 增量复核通过：`design.md`（第 92 行）、`tests.md`（`RWW-005`）与 `plan.md`（`PV2`）已统一收窄扫描范围至 `src/main.ts` 与 `src/workbench/*.ts`，保留线上协议保留动作与视图说明文案，闭环通过 |
| RV-PRE-002 | reviewer | blocker | a69da69e5d1cc6af6a15b3aa393359429c45ba58 | `clients/web/tests/session-view-node.d.ts`（第 2 行含 `export function stripTypeScriptTypes`）系专为 `main.ts` 源码切片测试提供的模块类型补丁，在 PowerShell 执行 `RWW-005`/`PV2` 的 `Select-String -Path clients/web/tests/*.ts -Pattern "new Function","stripTypeScriptTypes"` 时会被命中并导致门禁失败，但 `plan.md` `WP3` `Write Scope`（及 `tasks.md` `3.2`、`design.md` 第 4 节）遗漏了该文件。建议将 `clients/web/tests/session-view-node.d.ts` 加入 `WP3` `Write Scope`（在 `r3-source-session.test.ts` 去切片化后一并删除） | working-tree | closed | 增量复核通过：`plan.md` `WP3`、`tasks.md` `3.2`、`design.md` 第 4 节与 `tests.md` `RWW-005` 均已补入删除 `clients/web/tests/session-view-node.d.ts`，闭环通过 |
| RV-PRE-003 | reviewer | major | a69da69e5d1cc6af6a15b3aa393359429c45ba58 | 规划与用例多处模块契约与真实源码失真：① `tests.md` `RWW-002` 要求 `error-formatting.ts` “不导入连接或有状态控制器模块”，但 `managementErrorMessage`（`main.ts:3553-3608`）必须从连接模块 `src/management/client.ts` 导入 `ManagementRequestError`、`ManagementConnectionError`、`SessionConsumeOutcomeError` 做 `instanceof` 判断，需改为明确路径边界（允许导入 `protocol/*`、`pairing/*`、`management/*` 错误类，禁止导入 `src/workbench/*` 与 `src/main.ts`）；② `client.ts` 现公开导出 `utf16leBase64`、`CatalogLaunchEnvInput`、`CatalogAdditionalDirectoryInput`、`CatalogProfileRecipeInput`（被 `src/management/workspace-catalog-view.ts` 与 `tests/catalog-workspace.test.ts` 导入），且前端不存在任何“TOML 转义”实现或测试，需在 `proposal.md`、`design.md`、`tasks.md` `1.2`、`tests.md` `RWW-004` 移除虚构的 TOML 转义要求并补齐 `client.ts` 既有导出符号清单；③ `design.md` 第 1/2.1 节漏列 `session-protocol/*` 对 `src/management/protocol`（`MAX_MANAGEMENT_FRAME_BYTES`）的依赖，且误写了不存在的符号名 | working-tree | closed | 增量复核通过：`tests.md`（`RWW-002`/`RWW-004`）、`proposal.md`、`design.md` 与 `tasks.md` `1.2` 已全量对齐真实模块路径契约、补齐 `client.ts` 全部既有导出符号并移除虚构 TOML 描述，符号名与依赖边界准确一致，闭环通过 |
| TST-RWW-001 | tester | major | d6ac98d7f4f33149456472022d190d8543375227 | `management-error-messages.test.ts` 未按 `RWW-005` 步骤 3 导入并验证 `pairingErrorMessage` 与 `sessionActionErrorMessage`，且未构造和断言 `ManagementProtocolError` 分支 | pending-commit | fixed | 待独立 tester 与 reviewer 增量复核 |
| TST-RWW-002 | tester | major | d6ac98d7f4f33149456472022d190d8543375227 | `session-view-main.test.ts` 中 `QueuedBodyController` 测试未触发 `30_000ms` 超时回调（`"读取正文超时。"`、`reader.seal()`、`detachSessionStream` 零覆盖），且目标切换/同目标重开均发生在 `#activeBodyRead === null` 阶段 | pending-commit | fixed | 待独立 tester 与 reviewer 增量复核 |
| TST-RWW-003 | tester | major | d6ac98d7f4f33149456472022d190d8543375227 | `workbench-dom-and-view.test.ts` 缺失 `HTMLTextAreaElement` `[3, 9]`、控件移除及未提交 `value` 保留边界测试，且绕过 `SharedViewController.restorePersistedSourceViewIfIdle` 与 `ManagementWorkbench` 空闲重连自动恢复链路 | pending-commit | fixed | 待独立 tester 与 reviewer 增量复核 |
| TST-RWW-004 | tester | major | d6ac98d7f4f33149456472022d190d8543375227 | `ManagementWorkbench.resetAllSessionState()` 遗漏调用 `approval.reset()` 与 `capacityRelease.resetAll()`；`workbench-controllers.test.ts` 未覆盖未完结来源结果流（`last: false`）定时器清理、已挂载 `RawInspector` 销毁与 `SessionResumeCoordinator.clear()` 行为验证 | pending-commit | fixed | 待独立 tester 与 reviewer 增量复核 |
| TST-RWW-005 | tester | minor | d6ac98d7f4f33149456472022d190d8543375227 | `SessionQueueController`、`ApprovalController` 与 `CapacityReleaseController.nativeReleasePanel` 未透传宿主注入 `doc?: Document`，隐式回退 `globalThis.document` | pending-commit | fixed | 待独立 tester 与 reviewer 增量复核 |
| TST-RWW-006 | tester | major | d6ac98d7f4f33149456472022d190d8543375227 | 未改动的 `app/src/main.rs:614-621` `Area::new()` 使用无原子计数器的 `SystemTime::now().as_nanos()` 在 Windows 并发跑 `npm run check` 时偶发临时目录重名，且只读 `FIXTURE-SHA256.md` 记录值早于父基线 `a69da69e`；本变更硬约束严禁修改 `app/` 与 `clients/web/tests/browser/`，已对齐 `tests.md` `RWW-009` 与 `plan.md` `PV4` 按父基线 Git blob 核验并在复测中验证 `npm run check` | pending-commit | fixed | 待独立 tester 复测确认（`app/src/main.rs` 偶发竞态列为范围外基线观察项） |
| RV-DEL-001 | reviewer | minor | d6ac98d7f4f33149456472022d190d8543375227 | `SessionQueueController`、`ApprovalController` 与 `CapacityReleaseController.nativeReleasePanel` 存在轻微 `Document` DI 一致性缺口（与 `TST-RWW-005` 同源） | pending-commit | fixed | 待独立 reviewer 增量复核 |
| RV-DEL-002 | reviewer | minor | d6ac98d7f4f33149456472022d190d8543375227 | `ApprovalController.reset()` 与 `CapacityReleaseController.resetAll()` 未接入 `ManagementWorkbench.resetAllSessionState()`（与 `TST-RWW-004` 同源） | pending-commit | fixed | 待独立 reviewer 增量复核 |
| RV-DEL-003 | reviewer | minor | d6ac98d7f4f33149456472022d190d8543375227 | `SourceSessionController.acceptViewFrame` 缺少与 `QueuedBodyController.acceptViewFrame` 对称的入口世代短路检查 | pending-commit | fixed | 待独立 reviewer 增量复核 |

## Verification Results

- 受测版本：pending（待第二轮增量复测确认）
- 执行者与环境：pending
- 验证命令及退出码：pending

| Case ID | Executor | Commit SHA | Status | Evidence / Limitations |
| --- | --- | --- | --- | --- |
| RWW-001 | tester | d6ac98d7f4f33149456472022d190d8543375227 | passed | 首轮独立验收通过，待修复提交增量复核 |
| RWW-002 | tester | d6ac98d7f4f33149456472022d190d8543375227 | passed | 首轮独立验收通过，待修复提交增量复核 |
| RWW-003 | tester | d6ac98d7f4f33149456472022d190d8543375227 | passed | 首轮独立验收通过，待修复提交增量复核 |
| RWW-004 | tester | d6ac98d7f4f33149456472022d190d8543375227 | passed | 首轮独立验收通过，待修复提交增量复核 |
| RWW-005 | tester | d6ac98d7f4f33149456472022d190d8543375227 | failed | 首轮因 `TST-RWW-001` 失败，已补齐断言待复测 |
| RWW-006 | tester | d6ac98d7f4f33149456472022d190d8543375227 | failed | 首轮因 `TST-RWW-002` 失败，已补齐断言待复测 |
| RWW-007 | tester | d6ac98d7f4f33149456472022d190d8543375227 | failed | 首轮因 `TST-RWW-004` 失败，已修复并补齐断言待复测 |
| RWW-008 | tester | d6ac98d7f4f33149456472022d190d8543375227 | failed | 首轮因 `TST-RWW-003` 失败，已补齐断言待复测 |
| RWW-009 | tester | d6ac98d7f4f33149456472022d190d8543375227 | failed | 首轮受范围外 `TST-RWW-006` 影响，待复测确认 |

- 未执行项与已知限制：正在修复快照上执行第二轮独立 `tester` 复测与独立 `reviewer` 增量检视。
- 问题关闭依据：`RV-PRE-001`–`RV-PRE-003` 已在编码前门禁闭环；`TST-RWW-001`–`TST-RWW-006` 与 `RV-DEL-001`–`RV-DEL-003` 待原提出者复核关闭后回填最终快照。
