# Web 工作台控制器与会话协议分层实现

本说明记录 `clients/web/` 在零运行时第三方依赖约束下的前端内部模块化分层、控制器生命周期契约以及无源码切片单元测试边界。

## 单向分层架构

`clients/web/src/` 采用严格由内向外的单向依赖关系，禁止反向导入或循环依赖：

1. **基础协议与视图内核层（`src/protocol/*`、`src/views/*`、`src/pairing/*`）**：承载 JWS/WebCrypto 原语、流式时间线与原始载荷检查器、IndexedDB 身份保险库与配对会话状态机，不依赖 `src/management/*`、`src/workbench/*` 或 `src/main.ts`。
2. **管理协议与客户端传输层（`src/management/`）**：
   - `session-protocol/` 将会话管理协议按职责拆分为 6 个子模块：`primitives.ts`（闭合对象与基础标量校验原语）、`queue-and-input.ts`（输入队列与正文上传解析）、`catalog.ts`（工作区、Harness、Profile 与 Target 清单解析）、`view-and-approval.ts`（共享视图与严格权限审批解析）、`source-and-capacity.ts`（来源会话发现/创建、原生释放与容量快照解析）、`wire-codec.ts`（36 个线上命令 `SHAPES` 编解码与二进制帧分发）。`session-protocol.ts` 保留为纯 barrel 重导出入口，维持既有公共导出集不变。
   - `catalog-documents.ts` 负责安装 Harness 与初始绑定 Target 的 JSON 文档构造及 `utf16leBase64` 启动环境变量编码；`upload-transfer.ts` 负责输入正文与目录文档共用的分块发送与单在途确认流控；`client.ts` 聚合 `ManagementConnection` 状态机并保持既有公共导出兼容。
3. **工作台领域控制器层（`src/workbench/`）**：
   - 无状态基础模块：`dom-primitives.ts`（纯 DOM 构造辅助及 WV-15 聚焦输入控件选区快照保存/恢复）与 `error-formatting.ts`（配对、管理请求/连接与会话输入动作错误中性文案映射）。
   - 领域控制器：`pairing-access-controller.ts`、`device-trust-panel.ts`、`shared-view-controller.ts`、`queued-body-controller.ts`、`session-queue-controller.ts`、`approval-controller.ts`、`capacity-release-controller.ts`、`workspace-catalog-controller.ts`、`source-session-controller.ts`。所有控制器导入零 DOM 副作用，仅通过显式宿主回调端口（Host Port）读取活动连接、连接代际（`managementGeneration`）并触发重绘。
   - 顶层协调器：`management-workbench.ts` 持有连接代际与各领域控制器实例，统一编排认证握手、固定顺序仪表盘渲染、二进制流帧路由与断线/卸载清理钩子。
4. **浏览器薄启动入口（`src/main.ts`）**：仅负责查找 `#app`、挂载壳层 DOM、绑定明暗主题切换与 `pagehide` / `pageshow` / `online` 浏览器事件并调用 `workbench.boot()`。

## 关键状态隔离与生命周期不变式

- **二进制流帧路由优先级**：`ManagementWorkbench.handleSessionViewData` 先核对 `generation === managementGeneration`，随后按固定顺序将 `ViewDataFrame` 路由至 `SharedViewController.acceptViewFrame` → `SourceSessionController.acceptViewFrame` → `QueuedBodyController.acceptViewFrame`，确保共享视图块、来源发现/创建结果流与队列正文流互不串扰。
- **目标会话视图与来源只读视图隔离**：`SharedViewController` 通过 `SharedViewSpec` 区分 `target` 与 `source` 视图；来源只读视图断线重连失败时保留已有视图节点并标记 `data-stale="true"`，同时保持 `SessionQueueController.inspectedQueueTargetId` 不被污染；`rebindSharedView` 成功后在新快照就绪时清理旧视图，避免对已 `detached` 流重复发送释放请求。
- **重绘焦点与选区保持（WV-15）**：每次 `showManagementDashboard` 替换 `#access-panel` 子节点前后，均通过 `captureActiveControlSelection` 与 `restoreActiveControlSelection` 无滚动恢复当前聚焦 `input` / `textarea` 的值与选区方向。
- **统一资源回收**：页面隐藏（`handlePageHide`）、身份变更自愈与撤销重置统一调用 `disposeAllSessionResources` / `resetAllSessionState`，确保共享视图实例、30 秒正文/来源结果读取定时器、流密封与 `RawInspector` 同步释放。

## 测试边界

- Node 单元测试（`clients/web/tests/*.test.ts`）直接导入 `src/management/*` 与 `src/workbench/*` 实例化控制器并注入轻量替身，不使用 `readFileSync("../src/main.ts")` 源码切片或 `new Function` 动态执行。
- 真实浏览器生产入口与独立 DOM 验证继续由 `clients/web/tests/browser/session-view-main.ts` 通过 `await import("../../src/main.ts")` 覆盖 WV-13..WV-16 契约，`clients/web/tests/browser/` 全目录保持只读零改动。
