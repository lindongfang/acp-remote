# Web 工作台控制器与会话协议层模块化设计

## Context

动机与范围边界见 [proposal.md](proposal.md)。当前 `clients/web/` 运行在零运行时第三方依赖的原生 TypeScript + DOM 架构之上，开启了 `strict: true`、`noUncheckedIndexedAccess: true` 与 `verbatimModuleSyntax: true`：
- `clients/web/src/main.ts`（3,695 行）在模块求值顶层直接查找 `#app`、挂载壳层并持有约 75 个 `let` 模块级可变状态，致使任何子逻辑都无法在无完整浏览器 DOM 环境时被 Vitest 独立导入测试；`session-view-main.test.ts`、`r3-source-session.test.ts` 与 `management-error-messages.test.ts`（连同 `node:module` `stripTypeScriptTypes` 类型补丁 `session-view-node.d.ts`）因此使用 `readFileSync("../src/main.ts")` 截取源码片段再传入 `new Function(...)` 执行。
- `clients/web/src/management/session-protocol.ts`（2,035 行）把基础闭合对象校验器、5 个业务子域的响应解析器、36 个管理命令 `SHAPES` 与二进制服务端帧解码堆叠在同一文件；`clients/web/src/management/client.ts`（1,693 行）同时承载连接握手状态机、分块上传流控与目录安装/绑定 JSON 文档（及 `utf16leBase64` 环境变量编码）构造。
- `clients/web/tests/browser/session-view-main.ts` 通过 `await import("../../src/main.ts")` 驱动真实生产入口执行 WV-13..WV-16 独立 DOM 用例，且 `clients/web/tests/browser/independent-dom/FIXTURE-SHA256.md` 锁定了浏览器夹具文件的字节哈希。

## Goals / Non-Goals

**Goals:**
- 建立清晰的单向分层依赖：`protocol / views` -> `pairing / management` -> `workbench controllers` -> `workbench coordinator` -> `main.ts` 薄启动入口，消除跨面板隐式全局变量共享与循环依赖。
- 将所有业务逻辑与状态控制器设计为**导入零副作用（import-pure）**的可实例化类或纯函数，只有 `src/main.ts` 执行顶层 `#app` 挂载与全局事件注册。
- 保持 `src/management/session-protocol.ts` 与 `src/management/client.ts` 既有公共导出符号集精确一致（100% 向后兼容且不外泄新增内部原语），避免无意义的跨文件重命名噪音。
- 为后续 [deliver-session-control-takeover](../2026-11-20-deliver-session-control-takeover/design.md) 与 [deliver-session-transcript-cache](../2026-11-20-deliver-session-transcript-cache/design.md) 预留单一职责扩展缝隙（会话级主控写门控集中在会话队列/输入与审批控制器，本地快照与后台校准集中在共享视图控制器）。

**Non-Goals:**
- 不改变任何 HTML 节点标签、CSS 类名、`id`、`aria-*`、`data-*`（如 `data-panel="source-session"`、`data-stale="true"`）、按钮中文文案、错误提示文案或 `sessionStorage` / `IndexedDB` 键名。
- 不引入虚拟 DOM、响应式信号库或第三方运行时包，不修改 `clients/web/tests/browser/**` 任何字节。

## Decisions

### 1. 分层架构与单向依赖规则

拆分后的 `clients/web/src/` 严格遵守由内向外的单向依赖，禁止反向引用或环形 `import`：

1. **基础协议与视图内核层（既有保持稳定）**：`src/protocol/*`、`src/views/*`、`src/pairing/*` 不得导入 `src/management/*`、`src/workbench/*` 或 `src/main.ts`。
2. **管理协议与传输层（`src/management/`）**：
   - `src/management/session-protocol/` 子模块仅依赖 `src/protocol/jws`、`src/management/protocol`（如 `MAX_MANAGEMENT_FRAME_BYTES`）、`src/management/input-transfer`、`src/views/binary`、`src/views/errors` 与同目录子模块（单向指向 `primitives.ts`），不得导入 `src/workbench/*` 或 `src/main.ts`。
   - `src/management/catalog-documents.ts` 与 `src/management/upload-transfer.ts` 仅依赖 `src/protocol/*`、`src/management/input-transfer`、`src/management/protocol` 与 `src/management/session-protocol`，不得导入 `src/workbench/*` 或 `src/main.ts`。
   - `src/management/client.ts` 聚合连接状态机并重导出既有全部文档构造与错误/配置类型，不反向依赖 `src/workbench/*` 或 `src/main.ts`。
3. **工作台领域控制器层（`src/workbench/`）**：
   - 基础无状态原语：
     - `dom-primitives.ts` 仅依赖浏览器/DOM 标准类型，**禁止导入 `src/pairing/*`、`src/management/*`、`src/workbench/*` 或 `src/main.ts`**。
     - `error-formatting.ts` 为零状态纯函数模块，允许从 `src/protocol/*`、`src/pairing/*` 与 `src/management/*` 导入错误类（如 `ManagementRequestError`、`ManagementConnectionError`、`SessionConsumeOutcomeError`、`ManagementProtocolError`、`SessionInputActionError`、`PairingApiError`、`PairingProtocolError`、`IdentityStoreError`、`PairingSessionError`）及 `managementRequestErrorMessage`，**禁止导入 `src/workbench/*` 其他模块或 `src/main.ts`**。
   - 领域控制器：`pairing-access-controller.ts`、`device-trust-panel.ts`、`shared-view-controller.ts`、`queued-body-controller.ts`、`session-queue-controller.ts`、`approval-controller.ts`、`capacity-release-controller.ts`、`workspace-catalog-controller.ts`、`source-session-controller.ts`。各领域控制器仅通过显式宿主回调接口（Context / Host Port）读取当前活动 `ManagementConnection`、当前 `managementGeneration` 以及触发仪表盘重绘（`renderDashboard`），彼此不直接抓取私有状态，且不得导入 `src/main.ts`。
   - 顶层协调器：`management-workbench.ts` 持有连接代际与各领域控制器实例，按固定顺序组装仪表盘片段并分发 `onDevicesChanged`、`onViewData`、`onClose` 回调，不得导入 `src/main.ts`。
4. **浏览器启动入口（`src/main.ts`）**：仅负责查找 `#app`、初始化壳层 DOM 与主题切换按钮、实例化协调器、绑定 `pagehide`/`pageshow`/`online` 并调用 `boot()`。

### 2. `management/session-protocol` 与 `management/client` 拆分契约

#### 2.1 `src/management/session-protocol/` 模块划分

保留 `src/management/session-protocol.ts`（`<= 120` 行）作为纯 barrel 入口，**严格仅重导出原有 `session-protocol.ts` 已公开的全部公共类型与函数**（内部校验辅助函数仅在 `session-protocol/` 子模块间共享，不从 `session-protocol.ts` 新增导出）：
- `session-protocol/primitives.ts`：对外公共导出 `SessionProtocolError`、`requireSessionId`、`requireSessionU64`、`requireSessionI64`；同时向同目录子模块提供内部共享原语（`SessionProtocolErrorCode`、`MAX_SOURCE_TEXT_BYTES`、`Validator`、`Shape`、`ShapeValue`、`object`、`closed`、`oneOf`、`literal`、`responseResult`、`requireNullable`、`requireNull`、`requireBoolean`、`requireArray`、`approvalQuantity`、`requireText`、`requireTrue`、`requireBoundedSourceText`）。
- `session-protocol/queue-and-input.ts`：队列与输入上传公共类型（`QueueMutationDto`、`SessionStopReason`、`SessionPriorityFailure`、`SessionQueueEntry`、`SessionQueueProcessingPhase`、`SessionQueueProcessing`、`SessionQueueSnapshot`、`SessionQueueAction`、`SessionRegistration`、`SessionResumeResult`、`InputUploadBeginResult`、`InputUploadStatus`、`InputUploadFinishResult`、`InputUploadReleaseResult`、`SessionInputOutcome`、`SessionConsumeResult`、`SessionInputFact`、`SessionQueuedBodyDescriptor`）、内部 `mutation` 校验器及公共解析函数（`parseSessionQueue`、`parseSessionRegistration`、`parseSessionResumeResult`、`parseInputUploadBeginResult`、`parseInputUploadStatus`、`parseInputUploadFinishResult`、`parseInputUploadReleaseResult`、`parseSessionInputFact`、`parseSessionConsumeResult`、`parseSessionQueuedBodyDescriptor`）。
- `session-protocol/catalog.ts`：目录与工作区/目标公共类型（`CatalogUploadBeginResult`、`CatalogUploadStatus`、`CatalogUploadFinishResult`、`CatalogUploadReleaseResult`、`CatalogRegistration`、`CatalogFactResult`、`CatalogFactMetadata`、`CatalogSqlSlotObservation`、`CatalogSqlObservationSet`、`CatalogReceiptMetadata`、`WorkspaceRecord`、`ProductSourceSummary`、`HarnessSummary`、`ProfileSummary`、`TargetBindingSummary`、`ProductTargetRecord`、`CatalogInventory`）及公共解析函数（`parseCatalogUploadBeginResult`、`parseCatalogUploadStatus`、`parseCatalogUploadFinishResult`、`parseCatalogUploadReleaseResult`、`parseCatalogRegistration`、`parseCatalogFactMetadata`、`parseCatalogReceiptMetadata`、`parseWorkspaceRecord`、`parseProductTargetRecord`、`parseCatalogInventory`）。
- `session-protocol/view-and-approval.ts`：共享视图与严格审批公共类型（`SessionViewMode`、`SessionViewFreshness`、`SessionViewProjection`、`SessionViewJoin`、`SessionViewOpened`、`SessionViewClosed`、`ApprovalValidity`、`ApprovalAssociation`、`ApprovalDecision`、`ApprovalStatus`、`ApprovalView`、`ApprovalListResult`、`ApprovalRegistration`、`ApprovalDecisionResult`）及公共解析函数（`parseSessionViewOpened`、`parseSessionViewClosed`、`parseApprovalStatusResult`、`parseApprovalListResult`、`parseApprovalResult`、`parseApprovalRegistration`、`parseApprovalDecisionResult`）。
- `session-protocol/source-and-capacity.ts`：来源会话、原生释放与会话容量公共类型（`NativeReleaseRegistered`、`SessionCapacity`、`SourceLaunchSelectionInput`、`SourceOperationInput`、`SourceSessionRegistrationMetadata`、`SourceSessionRegistration`、`SourceSessionRpcId`、`SourceSessionTransportReason`、`SourceSessionTransportFact`、`SourceSessionResponseFact`、`SourceSessionFailureClass`、`SourceSessionFact`、`SourceSessionResultDescriptor`、`SourceSessionResultReleaseResult`、`NativeReleaseStatus`）、内部 `sourceSelection` / `sourceOperation` 校验器及公共解析函数（`parseNativeReleaseRegisterResult`、`parseSessionCapacity`、`parseNativeReleaseStatusResult`、`parseSourceSessionRegistration`、`parseSourceSessionFact`、`parseSourceSessionResultDescriptor`、`parseSourceSessionResultReleaseResult`）。
- `session-protocol/wire-codec.ts`：汇总 36 个动作（含线上协议保留动作 `"product.status"`）的 `SHAPES`、`SessionAction`、`SessionCommand`、`parseSessionCommand`、`encodeSessionCommand`、`SessionServerBinary` 与 `decodeSessionServerBinary`。

#### 2.2 `src/management/client.ts` 辅助提取契约

- `src/management/catalog-documents.ts`：承载并公开导出 `CatalogActionError`、`CatalogLaunchEnvInput`、`CatalogAdditionalDirectoryInput`、`CatalogProfileRecipeInput`、`InstallHarnessDocumentInput`、`BindTargetInitialDocumentInput`、`utf16leBase64`、`buildInstallHarnessDocument`、`buildBindTargetInitialDocument`（构造安装 Harness 与绑定初始目标的 JSON 文档及 UTF-16LE Base64 启动环境变量值）。
- `src/management/upload-transfer.ts`：承载输入正文与目录文档共用的二进制分块发送、单在途 `InputAcknowledgement` 等待与异常释放收敛辅助逻辑，供 `ManagementConnection` 内部调用。
- `src/management/client.ts` 继续重导出 `CatalogActionError`、`CatalogLaunchEnvInput`、`CatalogAdditionalDirectoryInput`、`CatalogProfileRecipeInput`、`InstallHarnessDocumentInput`、`BindTargetInitialDocumentInput`、`utf16leBase64`、`buildInstallHarnessDocument`、`buildBindTargetInitialDocument` 并导出原有的 `ManagementConnectionErrorCode`、`ManagementConnectionError`、`ManagementRequestError`、`SessionConsumeOutcomeError`、`ManagementClose`、`ManagementConnectionCallbacks`、`ManagementConnectionOptions`、`ManagementConnection`，确保 `workspace-catalog-view.ts` 与全部现有测试零导入破坏。

### 3. `workbench/` 控制器职责与跨模块状态契约

为保证单文件内聚（每个新建模块 `<= 650` 行、`src/main.ts <= 250` 行）且在 Vitest Node 环境下可直接实例化测试，定义如下模块边界：

1. **`workbench/dom-primitives.ts`**：
   - 提供纯 DOM 构造辅助（默认以传入的 `Document` 或 `globalThis.document` 创建节点）：`element`、`button`、`eyebrow`、`title`、`paragraph`、`statusLine`、`detailBlock`、`requiredElement`、`formatHex`、`formatTimestamp`、`delay`。
   - 提供 `captureActiveControlSelection(doc, container)` 与 `restoreActiveControlSelection(doc, snapshot)`，精确复刻原 `showManagementDashboard` 对获得焦点的 `HTMLInputElement` / `HTMLTextAreaElement` 的 `id`、`value`、`selectionStart`、`selectionEnd`、`selectionDirection` 保存与无滚动恢复逻辑，确保 WV-15 零回归。
2. **`workbench/error-formatting.ts`**：
   - 纯函数模块（零 DOM 依赖）：导出 `pairingErrorMessage(error)`、`managementErrorMessage(error)`、`sessionActionErrorMessage(error)`。
   - `managementErrorMessage` 对 `ManagementRequestError` 严格委托给 `managementRequestErrorMessage(error.code)`，消除 `management-error-messages.test.ts` 对 `main.ts` 源码切片的依赖。
3. **`workbench/shared-view-controller.ts`**：
   - 导出 `releaseSharedDisplay` 纯辅助函数、`SharedViewSpec` 类型与 `SharedViewController` 类。
   - 独立持有 `openedSessionView`、`sharedSessionDisplay`、`sharedSessionConnection`、`sharedViewTargetId`、`sharedViewSpec`、`sharedViewAttempt`、`sharedViewStaleNotice`、`staleSharedDisplays`，通过构造注入宿主端口（获取当前 `connection` / `generation` / `inspectedQueueTargetId`、`sharedViewHost`、`document`、可选自定义 `DisplayFactory`、`persistSourceViewSpec` / `clearPersistedSourceViewSpec`、`renderDashboard`、`formatError`）。
   - 提供 `openSharedSessionView`、`openTargetSharedView`、`closeTargetSharedView`、`openSharedSourceView`、`closeSharedSourceView`、`closeSharedSessionView`、`rebindSharedView`、`acceptViewFrame`、`suspendSharedView`、`disposeSharedViews`、`clearSharedSourceViewSpec`、`restorePersistedSourceViewIfIdle`。
   - 保持目标会话共享视图与来源只读视图的隔离不变式：`openSharedSourceView` 断线重连失败保留已有视图并标记 `data-stale="true"`，绝不污染 `inspectedQueueTargetId`；成功 `rebind` 在新快照就绪后清理旧实例，不向旧流重复发送 `detach`/`close`。
4. **`workbench/queued-body-controller.ts`**：
   - 导出 `QueuedBodyController` 类，独立持有 `activeBodyRead`、`bodyReadTimer`、`bodyReadAttempt`、`bodyPreview`。
   - 提供 `openQueueBody`、`acceptViewFrame`、`cancelActiveBodyRead`、`failQueueBodyRead`、`reset`，完整保留 30 秒超时围栏、同目标逆序响应自动 `detach` 旧流、切换目标或重置 UI 时密封并释放旧流等竞态保护。
5. **`workbench/session-queue-controller.ts`**：
   - 导出 `SessionQueueController` 类，持有 `inspectedQueueTargetId`、`inspectedQueue`、`editingQueueEntry`、`sessionResume`（`SessionResumeCoordinator`），协调队列查询表单、输入提交/编辑草稿组件、取消/优先发送与显式恢复队列，并与 `SharedViewController` 和 `QueuedBodyController` 通过清晰接口协作（后续 `deliver-session-control-takeover` 只需在此处注入主控只读态）。
6. **`workbench/approval-controller.ts` 与 `workbench/capacity-release-controller.ts`**：
   - `ApprovalController` 封装严格权限审批列表、详情读取、选项登记、提交裁决与状态刷新及 `sessionApprovalPanel` 渲染。
   - `CapacityReleaseController` 封装节点会话容量读取面板委托（调用既有 `sessionCapacityPanel`）及原生会话释放两阶段登记/执行/状态查询面板 `nativeReleasePanel`。
7. **`workbench/workspace-catalog-controller.ts` 与 `workbench/source-session-controller.ts`**：
   - `WorkspaceCatalogController` 封装目录清单缓存、工作区登记草稿、Harness 安装草稿、会话目标登记/一步登记并绑定/既有绑定动作及 `workspaceCatalogPanel` 渲染。
   - `SourceSessionController` 封装来源选择草稿、发现游标与手动原生 ID 草稿、来源操作登记/执行/对账/结果分块读取/释放、`sourceRawInspectors` 生命周期及 `sourceSessionPanel` 渲染。
8. **`workbench/device-trust-panel.ts`、`workbench/pairing-access-controller.ts` 与 `workbench/management-workbench.ts`**：
   - `DeviceTrustController` 管理已登记客户端分页、并发刷新合并（`devicesRefreshInFlight` / `refreshAfterCurrentRequest`）、`stale_cursor` 自动重置以及设备撤销确认视图。
   - `PairingAccessController` 管理浏览器 `IdentityVault` 串行队列（`runIdentityOperation`）、启动引导 `boot`、配对码认领、待批准轮询、`identity_changed` 自愈与被撤销身份重置。
   - `ManagementWorkbench` 统一协调 `openManagement` 认证握手、订阅设备变更、按既定顺序（身份详情 -> 节点信息 -> 会话输入队列 -> 严格权限审批 -> 会话容量 -> 原生会话释放 -> 工作区与目录 -> 来源会话发现 -> 已登记客户端 -> 底部操作栏）渲染仪表盘，并在 `onViewData` 中依次路由给 `SharedViewController` -> `SourceSessionController` -> `QueuedBodyController`。

### 4. 单元测试去切片化迁移策略

1. **`clients/web/tests/management-error-messages.test.ts`**：移除 `readFileSync("../src/main.ts")` 与 `new Function`，直接导入 `src/workbench/error-formatting.ts` 的 `managementErrorMessage`，验证所有 `MANAGEMENT_ERROR_CASES` 下的 `ManagementRequestError` 精确返回与 `managementRequestErrorMessage` 一致的中性文案，并验证连接关闭/协议错误/会话消费错误分支。
2. **`clients/web/tests/session-view-main.test.ts`**：移除 `readFileSync("../src/main.ts")`、`stripTypeScriptTypes` 与 `new Function`，直接实例化 `SharedViewController` 与 `QueuedBodyController`（注入替身 `connection`、可控 `generation` 与测试 `DisplayFactory`），逐条保留原有的全部共享视图与队列正文竞态用例（连接切换、目标切换、乱序 open/close、首包 credit 失败回滚、正文超时/重选/逆序响应）。
3. **`clients/web/tests/r3-source-session.test.ts` 与 `clients/web/tests/session-view-node.d.ts`**：移除 `r3-source-session.test.ts` 第 1161..1305 行对 `main.ts` 的 `new Function` 切片，改为直接调用 `releaseSharedDisplay` 与 `SharedViewController` 验证 `detached` 流去重、`openSharedSourceView` 断线保留 `data-stale="true"` 与 `inspectedQueueTargetId` 隔离、`rebindSharedView` 无重复释放，同时扫描工作台层源码（`clients/web/src/main.ts` 与 `clients/web/src/workbench/*.ts`）断言其不调用 `"product.status"`（`session-protocol` `SHAPES` 与 `source-session-view.ts` 说明文案中的合法引用保持不变）；第 1307..1316 行对 `./browser/session-view-main.ts` 夹具源码的只读契约检查原样保留；在最后一处切片测试迁移完成后一并删除不再需要的 `clients/web/tests/session-view-node.d.ts` 类型声明补丁。

## Risks / Trade-offs

- [拆分后面板重绘丢失输入框焦点或光标选区] → 在 `dom-primitives.ts` 中提取统一的焦点与选区快照保存/恢复辅助函数并在每次 `showManagementDashboard` 替换 `panel` 子节点前后调用，同时补充单元测试模拟焦点控件选区恢复，保持 WV-15 契约不变。
- [重置身份、页面隐藏或切换目标时遗漏清理某一子控制器的流/定时器] → 在 `ManagementWorkbench` 上集中定义 `suspendForDisconnect()`、`disposeAllSessionResources()` 与 `resetAllSessionState()` 生命周期钩子，统一编排共享视图、来源结果流、正文分块读取、原始结构检查器与恢复协调器的清理顺序，并用单元测试验证。
- [TypeScript `verbatimModuleSyntax: true` 下重导出类型触发编译错误] → 所有 `session-protocol.ts` 与 `client.ts` 的类型重导出严格使用 `export type { ... }`，并通过 `tsc --noEmit` 与 `vite build` 双重门禁校验。
