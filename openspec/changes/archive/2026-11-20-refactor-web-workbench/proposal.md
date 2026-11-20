# 模块化拆分 Web 工作台入口与会话协议编解码层

## Why

当前 `clients/web/src/main.ts`（3,695 行）在单一顶层副作用脚本内混杂了配对引导、管理连接代际围栏、设备信任列表、会话队列与输入编排、共享视图与正文分块流、严格审批、容量与原生释放、工作区/Harness 目录以及来源会话发现等全部状态与 DOM 装配；同时 `clients/web/src/management/session-protocol.ts`（2,035 行）与 `clients/web/src/management/client.ts`（1,693 行）也集中了过多跨域 DTO 校验与文档构造职责，导致部分单元测试被迫使用 `readFileSync("../src/main.ts")` 搭配字符串切片与 `new Function` 反射测试私有逻辑。在启动 [deliver-session-control-takeover](../2026-11-20-deliver-session-control-takeover/proposal.md) 与 [deliver-session-transcript-cache](../2026-11-20-deliver-session-transcript-cache/proposal.md) 之前完成纯内部模块化重构，可建立清晰的单向状态与控制器边界，消除大文件耦合与脆弱源码切片测试。

## Success Criteria

### 用户原始要求

> refactor-web-workbench需要现在实现吗
> 同意，开始吧

### 硬约束与非目标

- 本变更属于不改变任何外部可观察产品行为、WebSocket 管理协议帧格式、HTTP 配对契约、DOM 结构选择器/无障碍属性或会话存储键名的纯前端内部重构，在 `.openspec.yaml` 声明 `skip_specs: true`，严格遵守 [总体设计](../../../../docs/architecture/overview.md)、[能力地图](../../../../docs/architecture/capability-map.md)、[AGENTS.md](../../../../AGENTS.md) 与 [硬规则清单](../../../../docs/hard-rules.md)。
- 保持 `clients/web/package.json` 零运行时第三方依赖，不引入前端 UI 框架、状态管理库或新构建工具，保持 TypeScript `strict: true`、`noUncheckedIndexedAccess: true` 与 `verbatimModuleSyntax: true` 全量通过。
- 严禁修改 `clients/web/tests/browser/` 下的独立浏览器 DOM 夹具文件，确保 `clients/web/tests/browser/independent-dom/FIXTURE-SHA256.md` 记录的全部夹具字节与 SHA-256 摘要零变动，且 `await import("../../src/main.ts")` 生产入口契约完全兼容。
- 不在本变更中提前实现会话控制权接管（`session.control.*`）或持久化展示快照缓存（`transcript-cache`）的任何新协议或新界面功能，不修改 Rust 节点代码（`app/`）。

### 本 change 的成功判据

- SC1：将 `clients/web/src/main.ts`（原 3,695 行）收敛为仅负责挂载壳层、主题切换、全局页面生命周期监听与控制器装配的薄启动入口（`<= 250` 行），将配对与本地身份生命周期、管理连接与面板协调、设备信任分页、会话队列与输入编排、队列正文分块读取、共享会话/来源视图生命周期、严格权限审批、会话容量与原生释放、工作区与 Harness 目录、来源会话发现与结果流拆分为 `clients/web/src/workbench/` 下职责单一、无循环依赖的独立模块（新建模块单文件均 `<= 650` 行）。
- SC2：将 `clients/web/src/management/session-protocol.ts`（原 2,035 行）按校验原语、队列与输入、目录与工作区、共享视图与严格审批、来源会话与容量释放、顶层命令与二进制帧编解码拆分为 `clients/web/src/management/session-protocol/` 子模块，并保留 `session-protocol.ts` 作为薄聚合重导出入口（`<= 120` 行，且不外泄内部校验原语）；同时从 `clients/web/src/management/client.ts`（原 1,693 行）提取目录安装/绑定 JSON 文档构造（含 `utf16leBase64`）与通用分块上传辅助模块并保持 `client.ts` 原有全部公共导出签名 100% 兼容。
- SC3：消除 `clients/web/tests/` 中对 `../src/main.ts` 实施函数体字符串切片与 `new Function(...)` 动态求值的脆弱测试写法（涉及 `session-view-main.test.ts`、`r3-source-session.test.ts`、`management-error-messages.test.ts` 及配套类型补丁 `session-view-node.d.ts`），改为直接导入并调用拆分后的真实生产 TypeScript 控制器与纯函数模块，完整保留所有代际围栏、乱序响应、断线保留旧视图（`data-stale="true"`）、流释放去重及错误码中性文案映射断言，且新增模块级单元测试覆盖新建控制器边界。
- SC4：`clients/web/tests/browser/independent-dom/FIXTURE-SHA256.md` 所列浏览器夹具文件校验和 100% 保持不变；`npm --prefix clients/web test`、`npm --prefix clients/web run build`、全仓 `npm run check` 与 `npm run check:spec` 全部零告警零错误通过，且工作台各面板渲染顺序、表单焦点与选区恢复（WV-15）及 `sessionStorage` 来源只读视图恢复语义零回归。

## What Changes

- 新增 `clients/web/src/workbench/` 模块化工作台层：
  - 提取纯 DOM 构造与焦点选区保存/恢复原语（`dom-primitives.ts`）以及配对/管理/会话输入错误文案纯映射模块（`error-formatting.ts`）。
  - 提取配对认领、轮询恢复与本地 `IdentityVault` 串行重置控制器（`pairing-access-controller.ts`）及可信设备列表分页与撤销面板（`device-trust-panel.ts`）。
  - 提取共享实时/来源只读视图控制器（`shared-view-controller.ts`）、队列输入正文分块读取控制器（`queued-body-controller.ts`）、会话队列与输入/编辑/恢复编排控制器（`session-queue-controller.ts`）。
  - 提取严格权限审批控制器（`approval-controller.ts`）、会话容量与原生会话释放控制器（`capacity-release-controller.ts`）、工作区与 Harness 目录控制器（`workspace-catalog-controller.ts`）以及来源会话发现/创建与结果流控制器（`source-session-controller.ts`）。
  - 提取已认证管理连接与仪表盘协调器（`management-workbench.ts`），并将 `clients/web/src/main.ts` 精简为薄启动入口。
- 拆分 `clients/web/src/management/` 协议与文档辅助层：
  - 新建 `clients/web/src/management/session-protocol/`（`primitives.ts`、`queue-and-input.ts`、`catalog.ts`、`view-and-approval.ts`、`source-and-capacity.ts`、`wire-codec.ts`），由 `session-protocol.ts` 统一重导出既有全部公共类型与解析/编解码函数。
  - 新建 `clients/web/src/management/catalog-documents.ts` 与 `clients/web/src/management/upload-transfer.ts`，收敛目录 JSON 文档构造（含 `utf16leBase64` 编码）与分块上传状态传输逻辑，`client.ts` 保持全部既有公共导出不变。
- 升级单元测试为真实模块直连验证：
  - 将 `clients/web/tests/session-view-main.test.ts`、`clients/web/tests/r3-source-session.test.ts` 与 `clients/web/tests/management-error-messages.test.ts` 中针对 `../src/main.ts` 的源码文本切片 + `new Function` 反射测试迁移为直接调用 `src/workbench/` 生产导出接口，移除专为切片测试设立的 `clients/web/tests/session-view-node.d.ts`，保留并强化全部既有竞态与静态契约断言。

## Capabilities

### New Capabilities
- 无

### Modified Capabilities
- 无

## Impact

- Web 前端源码：`clients/web/src/main.ts`、`clients/web/src/workbench/*`、`clients/web/src/management/session-protocol.ts`、`clients/web/src/management/session-protocol/*`、`clients/web/src/management/client.ts`、`clients/web/src/management/catalog-documents.ts`、`clients/web/src/management/upload-transfer.ts`。
- Web 单元测试：`clients/web/tests/session-view-main.test.ts`、`clients/web/tests/r3-source-session.test.ts`、`clients/web/tests/management-error-messages.test.ts`、删除废弃补丁 `clients/web/tests/session-view-node.d.ts` 及新增控制器边界单元测试；`clients/web/tests/browser/` 保持只读零改动。
- 前置与后续衔接：承接已归档 [fix-build-and-legacy-baseline](../2026-11-19-fix-build-and-legacy-baseline/proposal.md)，为后续 [deliver-session-control-takeover](../2026-11-20-deliver-session-control-takeover/proposal.md)（接入会话级主控/只读状态门控）与 [deliver-session-transcript-cache](../2026-11-20-deliver-session-transcript-cache/proposal.md)（接入共享视图本地快照秒开与后台校准）提供清晰的控制器挂载点。

## Exploration

- **实施时机确认（用户确认）**：经对比前置预重构与随业务特性混合拆分的风险，确认在启动 `deliver-session-control-takeover` 与 `deliver-session-transcript-cache` 之前先行独立交付 `refactor-web-workbench`，使结构重构与新协议/UI 行为演进彻底解耦，验收基准完全锚定现有测试套件与构建门禁。
- **公共导入兼容与独立夹具保护**：经扫描 `clients/web/tests/` 全量测试与 `clients/web/tests/browser/independent-dom/FIXTURE-SHA256.md`，确认保留 `src/main.ts`、`src/management/session-protocol.ts` 与 `src/management/client.ts` 的全部既有外部导入签名，仅将 Vitest 中依赖 `main.ts` 函数体字符串切片的 3 个测试文件升级为真实生产模块导入（并清理配套 `session-view-node.d.ts`），确保独立浏览器夹具文件 SHA-256 零变更。
- **术语与 ADR 复核**：复用 [领域术语表](../../../GLOSSARY.md) 既有定义，不引入新产品概念或外部依赖，无需新增独立 ADR。
