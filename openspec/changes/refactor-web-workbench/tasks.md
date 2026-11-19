# Web 工作台控制器与会话协议层模块化任务清单

## 1. 管理协议与客户端传输辅助拆分（WP1）

- [x] 1.1 WP1 将 `clients/web/src/management/session-protocol.ts` 按领域拆分为 `session-protocol/primitives.ts`、`queue-and-input.ts`、`catalog.ts`、`view-and-approval.ts`、`source-and-capacity.ts`、`wire-codec.ts` 6 个子模块，并保留 `session-protocol.ts` 作为薄 barrel 重导出入口（仅重导出既有公共符号，不外泄内部校验原语）；完成判据：`session-protocol.ts <= 120` 行、各子模块 `<= 650` 行、`verbatimModuleSyntax` 下 `npm --prefix clients/web run check` 与 `tests/session-protocol.test.ts` 全通过。
- [x] 1.2 WP1 从 `clients/web/src/management/client.ts` 提取 `catalog-documents.ts`（目录安装/绑定 JSON 文档构造与 `utf16leBase64` 环境变量编码）与 `upload-transfer.ts`（分块上传与单在途确认流控辅助），在 `client.ts` 保持既有全部公共符号（含 `utf16leBase64`、`CatalogLaunchEnvInput`、`CatalogAdditionalDirectoryInput`、`CatalogProfileRecipeInput` 等）100% 兼容重导出并补充子模块与重导出单测；完成判据：`catalog-documents.ts` 与 `upload-transfer.ts` 均 `<= 650` 行，`npm --prefix clients/web test -- tests/management.test.ts tests/catalog-workspace.test.ts` 退出码 0 通过。

## 2. 工作台基础原语与核心会话流/视图/队列控制器（WP2）

- [x] 2.1 WP2 创建 `clients/web/src/workbench/dom-primitives.ts`（DOM 构造与聚焦控件选区保存/恢复）与 `error-formatting.ts`（配对/管理/会话输入错误纯映射），将 `tests/management-error-messages.test.ts` 迁移为直接导入生产纯函数模块并新增焦点选区恢复单测；完成判据：`management-error-messages.test.ts` 彻底移除 `readFileSync("../src/main.ts")` 与 `new Function`，错误映射与 WV-15 选区恢复单测全部通过。
- [x] 2.2 WP2 创建 `clients/web/src/workbench/shared-view-controller.ts`、`queued-body-controller.ts` 与 `session-queue-controller.ts`，将 `tests/session-view-main.test.ts` 从 `main.ts` 字符串切片迁移为真实控制器实例化测试并补充 `sessionStorage` 来源视图恢复单测（`tests/workbench-dom-and-view.test.ts`）；完成判据：`session-view-main.test.ts` 零 `new Function`/`stripTypeScriptTypes` 残留，代际围栏、故障回滚、乱序响应与正文超时竞态用例全部退出码 0 通过。

## 3. 工作台其余领域控制器、顶层协调器与薄入口收口（WP3）

- [x] 3.1 WP3 创建 `clients/web/src/workbench/pairing-access-controller.ts`、`device-trust-panel.ts`、`approval-controller.ts`、`capacity-release-controller.ts`、`workspace-catalog-controller.ts`、`source-session-controller.ts` 与 `management-workbench.ts`，将 `clients/web/src/main.ts` 收敛为薄启动入口；完成判据：`src/main.ts <= 250` 行、12 个 `workbench/*.ts` 模块均 `<= 650` 行且无循环依赖、Node 环境导入 `workbench/*.ts` 零 DOM 副作用。
- [x] 3.2 WP3 将 `clients/web/tests/r3-source-session.test.ts` 中对 `main.ts` 的切片反射升级为真实 `SharedViewController`/`releaseSharedDisplay` 调用，删除不再需要的切片类型补丁 `clients/web/tests/session-view-node.d.ts`，新增 `tests/workbench-controllers.test.ts` 覆盖协调器帧路由与资源清理生命周期，并编写实现文档 `docs/implementation/web-workbench.md`；完成判据：`tests/*.ts` 零 `new Function`/`stripTypeScriptTypes` 残留、`FIXTURE-SHA256.md` 6 文件哈希不变、`npm --prefix clients/web test`、`npm --prefix clients/web run build`、`npm run check` 与 `npm run check:spec` 全部零错误通过。

## 4. 集成验收与交付检视门禁（WP4）

- [ ] 4.1 WP4 在完成 `WP1`–`WP3` 并锁定受跟踪文件干净的本地分支集成 `commit SHA` 后，并行执行独立 `tester` 正式验收（`RWW-001`–`RWW-009`）与独立 `reviewer` 交付检视；完成判据：`PV1`–`PV5` 全部通过、所有阻断问题闭环、`plan.md` 中 `Code Review` 集成交付门禁登记为 `passed` 且 `Verification Results` 完成精简证据回填。

## Workflow follow-up

- 归档在同一本地提交内按原子顺序执行：① 本变更声明 `skip_specs: true`，无需同步 `openspec/specs/` 规范 → ② 将变更目录移入 `openspec/changes/archive/YYYY-MM-DD-refactor-web-workbench/` 并补齐内部指向变更外相对链接的 `../` 层级，同步核查更新 `README.md`/`docs/` 引用 → ③ 本变更为纯前端内部重构，未新增或修改长期能力测试套件，无需写入 `testing/suites/` → ④ 运行 `npm run check:spec` 校验通过后提交。
