# Web 工作台控制器与会话协议层模块化测试套件

本文由独立 `tester` 依据 [proposal.md](proposal.md)、[design.md](design.md) 与当前仓库真实代码树编写，定义活跃变更 `refactor-web-workbench`（`skip_specs: true` 纯前端内部重构）的可重复验收用例。验收聚焦 `clients/web/src/main.ts` 与 `clients/web/src/workbench/` 职责拆分及行数上限、单向无环依赖、`clients/web/src/management/` 协议与上传模块重导出兼容、单元测试去源码切片与 `new Function` 化、代际围栏与断线陈旧视图竞态不变量、独立浏览器夹具字节级不变性以及全仓工程构建门禁。正式验收运行日志保存在本地被 Git 忽略的 `testing/runs/refactor-web-workbench/` 目录，精简逐例验收结论回填至同目录 `plan.md`。

## 共用执行条件与真实边界

- **版本快照与角色独立性**：正式集成验收必须在与独立 `reviewer` 共享的同一受测本地分支 Git `commit SHA`（`git rev-parse HEAD` 且受跟踪工作区 `git status` 干净）上执行；`tester` 会话独立于 `coder` 开发会话，不以开发自评作为通过依据。
- **Node / Vitest / TypeScript / Vite 执行入口与零运行时依赖约束**：所有验证命令均在仓库根目录 PowerShell 中直接执行（通过 `--prefix clients/web` 或根目录脚本调用，严禁使用目录切换命令或多命令串联符），Node 版本满足根目录 `package.json` 的 `>=22.12.0` 要求。`clients/web/package.json` 必须保持零运行时第三方依赖（不含 `dependencies`、`peerDependencies` 或 `optionalDependencies` 字段，仅保留 `typescript`、`vite`、`vitest` 开发依赖），`clients/web/tsconfig.json` 保持 `strict: true`、`noUncheckedIndexedAccess: true`、`verbatimModuleSyntax: true`、`isolatedModules: true` 与 `noUncheckedSideEffectImports: true` 全量开启且不被注释或放宽。
- **只读独立浏览器夹具与范围边界**：`clients/web/tests/browser/` 下的独立浏览器 DOM 夹具（含 `clients/web/tests/browser/independent-dom/FIXTURE-SHA256.md` 及其所列 6 个文件）相对于父基线 `a69da69e5d1cc6af6a15b3aa393359429c45ba58` 保持只读零改动，且 `clients/web/tests/browser/session-view-main.ts` 通过 `await import("../../src/main.ts")` 驱动真实生产入口的挂载契约完全兼容；Rust 节点源码目录 `app/` 保持零改动，且不混入 `session.control.*` 会话控制权接管或 `transcript-cache` 本地快照缓存等未授权新协议与界面特性。
- **隔离边界、用户数据保护与日志归档**：静态图谱检查、Vitest 控制器单测/竞态集成测试及 Vite 生产打包均在进程内存替身或独立构建输出中运行，不读写本机真实节点 SQLite 数据库、浏览器真实 `IndexedDB` 身份私钥、原生 CLI 会话或用户私有文件；若临时脚本产生中间文件须置于系统临时目录并在退出前删除。详细验证输出落盘至 `testing/runs/refactor-web-workbench/`，不在受跟踪文档中使用超链接指向被忽略目录。本变更为纯内部重构，全部判据均可通过静态结构检查、单元测试、集成竞态测试与构建检查验证，不设 `LX` 用例。

## 需求关联

| 当前增量路径 | Requirement | Scenario | 用例 |
| --- | --- | --- | --- |
| proposal.md | Success Criteria | SC1 | RWW-001、RWW-002 |
| proposal.md | Success Criteria | SC2 | RWW-003、RWW-004 |
| proposal.md | Success Criteria | SC3 | RWW-005、RWW-006、RWW-007 |
| proposal.md | Success Criteria | SC4 | RWW-008、RWW-009 |

## Cases

### RWW-001 main.ts 薄启动入口与 workbench 模块行数上限及导入零副作用验证

- 用例级别：L1
- 操作步骤：1) 在仓库根目录执行 PowerShell 脚本核查 `clients/web/src/main.ts` 与 `clients/web/src/workbench/*.ts` 的文件清单及逐文件总行数：`(Get-Content clients/web/src/main.ts).Count` 与 `Get-ChildItem clients/web/src/workbench/*.ts | ForEach-Object { [PSCustomObject]@{ Name = $_.Name; Lines = (Get-Content $_.FullName).Count } }`，核对 `dom-primitives.ts`、`error-formatting.ts`、`pairing-access-controller.ts`、`device-trust-panel.ts`、`shared-view-controller.ts`、`queued-body-controller.ts`、`session-queue-controller.ts`、`approval-controller.ts`、`capacity-release-controller.ts`、`workspace-catalog-controller.ts`、`source-session-controller.ts`、`management-workbench.ts` 共 12 个工作台模块是否齐全；2) 审阅 `clients/web/src/main.ts` 源码，确认其仅负责查找 `#app`、挂载壳层与主题切换、注册 `pagehide`/`pageshow`/`online` 全局生命周期监听及实例化协调器调用 `boot()`，不再残留业务面板可变状态与协议编排实现；3) 执行 Vitest 测试加载全部 `clients/web/src/workbench/*.ts` 模块（在无真实 `#app` 节点的 Node 测试环境下直接 `import`），验证导入期零 DOM 挂载副作用。
- 预期结果：`clients/web/src/main.ts` 总行数 `<= 250` 行；`clients/web/src/workbench/` 下全部新建 TypeScript 模块单文件总行数均 `<= 650` 行且职责单一；在无 `#app` 容器的 Node / Vitest 环境下直接导入任一 `src/workbench/*.ts` 模块均以退出码 0 成功完成求值，不抛出缺失 `#app` 异常，也不向全局 `window` / `document` 触发未受控副作用。

### RWW-002 Web 源码单向分层依赖与零循环引用图谱验证

- 用例级别：L2
- 操作步骤：1) 在仓库根目录运行 `node --input-type=module` 静态依赖分析脚本（将检查结果同步记录至 `testing/runs/refactor-web-workbench/`），递归扫描 `clients/web/src/**/*.ts` 中全部静态 `import ... from`、`export ... from` 与动态 `import(...)` 相对路径，解析为模块有向图并执行深度优先环路检测（DFS Cycle Detection）；2) 在同一脚本中校验分层方向规则：`src/protocol/*`、`src/views/*`、`src/pairing/*` 不得导入 `src/management/*`、`src/workbench/*` 或 `src/main.ts`；`src/management/*`（含 `src/management/session-protocol/*`、`catalog-documents.ts`、`upload-transfer.ts`、`client.ts`）不得反向导入 `src/workbench/*` 或 `src/main.ts`；`src/workbench/*` 各模块不得导入 `src/main.ts`；`dom-primitives.ts` 不得导入 `src/pairing/*`、`src/management/*`、`src/workbench/*` 或 `src/main.ts`；`error-formatting.ts` 允许导入 `src/protocol/*`、`src/pairing/*` 与 `src/management/*`（如 `ManagementRequestError` 等错误类及 `managementRequestErrorMessage`），严禁导入 `src/workbench/*` 其他模块或 `src/main.ts`。
- 预期结果：静态依赖分析脚本退出码为 0，`clients/web/src/` 全量 TypeScript 模块有向图中强连通环与自环数量严格为 `0`（零循环依赖）；分层跨界违规计数严格为 `0`，严格符合 `protocol / views` -> `pairing / management` -> `workbench controllers` -> `workbench coordinator` -> `main.ts` 的单向依赖契约。

### RWW-003 session-protocol 子模块拆分、薄重导出入口行数与编解码契约验证

- 用例级别：L1
- 操作步骤：1) 在仓库根目录检查 `(Get-Content clients/web/src/management/session-protocol.ts).Count` 及 `clients/web/src/management/session-protocol/` 子目录下 `primitives.ts`、`queue-and-input.ts`、`catalog.ts`、`view-and-approval.ts`、`source-and-capacity.ts`、`wire-codec.ts` 6 个子模块的存在性与各自总行数，并核查 `session-protocol.ts` 是否仅作为纯聚合重导出（barrel）入口（严格使用 `export type` 且不外泄内部校验原语）；2) 执行 `npm --prefix clients/web run check` 校验 `verbatimModuleSyntax: true` 下的类型与值重导出合法性；3) 执行 `npm --prefix clients/web test -- tests/session-protocol.test.ts` 及依赖 `../src/management/session-protocol` 的全部现有协议测试套件。
- 预期结果：`clients/web/src/management/session-protocol.ts` 总行数 `<= 120` 行，6 个 `session-protocol/*.ts` 子模块齐全且单文件均 `<= 650` 行，仅依赖允许的内层模块与 `primitives.ts`；既有全部公共类型与解析/编码函数（含 `SessionProtocolError`、`requireSessionId`、`requireSessionU64`、`requireSessionI64`、36 个动作的 `parseSessionCommand`/`encodeSessionCommand`、`decodeSessionServerBinary` 及各子域 `parse*` 函数）从 `session-protocol.ts` 100% 兼容导出且不新增泄漏内部校验原语；`tsc --noEmit` 与 Vitest 协议测试全部退出码 0、零错误通过。

### RWW-004 management/client 目录文档与分块上传辅助提取及导出签名兼容验证

- 用例级别：L2
- 操作步骤：1) 检查 `clients/web/src/management/catalog-documents.ts` 与 `clients/web/src/management/upload-transfer.ts` 的存在性、行数（单文件 `<= 650` 行）及导入边界，核实 `clients/web/src/management/client.ts` 继续导出 `ManagementConnection`、`ManagementConnectionError`、`ManagementRequestError`、`SessionConsumeOutcomeError`、`CatalogActionError`、`InstallHarnessDocumentInput`、`BindTargetInitialDocumentInput`、`CatalogLaunchEnvInput`、`CatalogAdditionalDirectoryInput`、`CatalogProfileRecipeInput`、`utf16leBase64`、`buildInstallHarnessDocument`、`buildBindTargetInitialDocument` 等全部既有公共符号；2) 执行 `npm --prefix clients/web test` 中覆盖管理客户端握手、UTF-16LE 目录安装/绑定 JSON 文档构造、Base64 环境变量编码以及输入与目录分块上传单在途 `InputAcknowledgement` 流控与异常释放收敛的测试套件。
- 预期结果：`catalog-documents.ts` 与 `upload-transfer.ts` 职责清晰分离且不反向依赖 `src/workbench/*`；外部测试与工作台模块从 `src/management/client` 导入原有符号零破坏、零重命名回归；UTF-16LE Base64 目录文档生成、分块上传流控及握手状态机相关 Vitest 用例全部退出码 0 通过。

### RWW-005 测试套件去切片化静态核查与错误文案纯函数直连验证

- 用例级别：L1
- 操作步骤：1) 在仓库根目录对 `clients/web/tests/` 执行静态模式扫描（`Select-String -Path clients/web/tests/*.ts -Pattern "new Function","stripTypeScriptTypes"` 并核查所有针对 `../src/main.ts` 的 `readFileSync` 调用），确认 `session-view-main.test.ts`、`r3-source-session.test.ts`、`management-error-messages.test.ts` 及全套测试文件已彻底移除对 `main.ts` 函数体的字符串切片与动态求值，且专为切片测试设立的 `clients/web/tests/session-view-node.d.ts` 已被删除；确认 `r3-source-session.test.ts` 完整保留对 `./browser/session-view-main.ts` 浏览器夹具源码的只读契约断言；2) 扫描工作台层源码（`clients/web/src/main.ts` 与 `clients/web/src/workbench/*.ts`）确认不存在 `product.status` 调用；3) 执行 `npm --prefix clients/web test -- tests/management-error-messages.test.ts`，直接调用 `src/workbench/error-formatting.ts` 导出的 `managementErrorMessage`、`pairingErrorMessage` 与 `sessionActionErrorMessage` 验证错误码映射。
- 预期结果：`clients/web/tests/*.ts` 中 `new Function`、`stripTypeScriptTypes` 及对 `../src/main.ts` 截取函数体切片的匹配数严格为 `0`；`session-view-node.d.ts` 不复存在；工作台层源码中 `product.status` 匹配数严格为 `0`；`management-error-messages.test.ts` 直接导入生产纯函数模块并以退出码 0 通过，所有 `MANAGEMENT_ERROR_CASES` 下的 `ManagementRequestError` 均精确返回与 `managementRequestErrorMessage(code)` 一致的中性中文提示，连接关闭、协议异常与消费结果异常分支映射无遗漏。

### RWW-006 共享视图与队列正文控制器代际围栏、乱序竞态及断线陈旧保留验证

- 用例级别：L5
- 操作步骤：1) 执行 `npm --prefix clients/web test -- tests/session-view-main.test.ts tests/r3-source-session.test.ts`，核实测试直接实例化生产导出的 `SharedViewController`、`QueuedBodyController` 并调用 `releaseSharedDisplay`（不经任何源码字符串反射）；2) 逐项核对正常、异常、取消与并发竞态断言覆盖：首包 `0n` credit 授予；`constructor` / `append` / `initial-credit` 三类故障注入下的新流释放与原活动视图无损回滚；同一连接代际内乱序返回的两个 `open` 仅保留最新流并关闭旧流；切换目标或连接重连递增 `managementGeneration` 后用原连接安全释放迟到流且不误发 credit；远端 `close` 拒绝时本地立即释放展示以及旧 `close` 迟到不误清已重开的新视图；3) 核对来源只读视图与队列正文读取隔离契约：`openSharedSourceView` 在 `reconnect` 失败时保留既有 DOM 视图并标记 `data-stale="true"`、绝不污染 `inspectedQueueTargetId`；`rebindSharedView` 成功后平滑替换流且不向旧流重复发送 `detach`/`close`；`releaseSharedDisplay` 对 `detached: true` 流跳过重复 `detachSessionStream`；`QueuedBodyController` 在 30 秒超时、同目标逆序响应与目标切换重置时正确执行 `seal` 与 `detachSessionStream`。
- 预期结果：`tests/session-view-main.test.ts` 与 `tests/r3-source-session.test.ts` 全部用例以退出码 0 通过，实跑用例数非零且无跳过项；共享实时视图、来源只读视图与队列正文分块读取的全部代际围栏、故障回滚、断线陈旧标记（`data-stale="true"`）、目标状态隔离与流释放去重不变量 100% 成立。

### RWW-007 工作台新建领域控制器边界与顶层协调器生命周期清理验证

- 用例级别：L2
- 操作步骤：1) 执行 `npm --prefix clients/web test` 运行针对 `clients/web/src/workbench/` 新建控制器与协调器的模块级单元测试套件；2) 验证以下控制器边界行为：`PairingAccessController` 对 `IdentityVault` 操作的串行队列互斥、认证 `identity_changed` 自愈与身份重置触发全量会话资源销毁；`DeviceTrustController` 在并发刷新请求下的合并重跑（`refreshAfterCurrentRequest`）与 `stale_cursor` 自动回退首页；`ManagementWorkbench` 的 `onViewData` 严格按 `SharedViewController` -> `SourceSessionController` -> `QueuedBodyController` 顺序分发二进制视图帧（不把共享视图包错误独占路由至队列正文读取器），以及 `suspendForDisconnect()`、`disposeAllSessionResources()`、`resetAllSessionState()` 在断线、页面隐藏、身份替换时对共享视图、来源结果流与定时器、队列正文读取器、`sourceRawInspectors` 及 `SessionResumeCoordinator` 的确定性清理顺序。
- 预期结果：新建工作台控制器与协调器边界单元测试全部以退出码 0 通过；各领域控制器均仅通过注入的宿主端口交互而不互相篡改私有字段，断线挂起、连接切换与本地身份重置后零悬挂定时器、零泄漏视图流或未释放原始检查器。

### RWW-008 仪表盘渲染顺序、WV-15 焦点选区恢复与 sessionStorage 来源视图恢复验证

- 用例级别：L2
- 操作步骤：1) 通过工作台协调器与 `dom-primitives.ts` 单元测试验证 `ManagementWorkbench` 组装管理仪表盘时各业务面板的挂载顺序、DOM 选择器、`data-*` 属性与无障碍属性；2) 测试 `captureActiveControlSelection(doc, container)` 与 `restoreActiveControlSelection(doc, snapshot)`：在重绘仪表盘容器前后分别让具有相同 `id` 且 `value` 未变的 `HTMLInputElement`（如选区 `[2, 8]`）与 `HTMLTextAreaElement`（如选区 `[3, 9]`）持有焦点，以及构造 `value` 已变更或控件移除的边界情况，验证重绘后焦点与光标选区恢复行为（对应 WV-15 契约）；3) 测试 `SharedViewController.restorePersistedSourceViewIfIdle` 与 `persistSourceViewSpec` / `clearPersistedSourceViewSpec` 协作：验证空闲连接建立时按 `sessionStorage` 持久化的来源视图规格自动发起只读恢复，而在已有活动共享视图或用户主动关闭来源视图时正确跳过或清除持久化记录。
- 预期结果：仪表盘严格按“身份详情 -> 节点信息 -> 会话输入队列 -> 严格权限审批 -> 会话容量 -> 原生会话释放 -> 工作区与目录 -> 来源会话发现 -> 已登记客户端 -> 底部操作栏”顺序渲染，`[data-panel="source-session"]` 等既有选择器与中文文案零漂移；重绘前后受控输入框与多行文本框的焦点、`selectionStart`、`selectionEnd`、`selectionDirection` 精确无滚动恢复；`sessionStorage` 来源只读视图恢复与清理语义零回归。

### RWW-009 独立浏览器夹具基线零改动与全仓构建门禁验证

- 用例级别：L1
- 操作步骤：1) 在仓库根目录对比 `clients/web/tests/browser/independent-dom/FIXTURE-SHA256.md` 所列 6 个浏览器夹具文件（`session-view-main.html`、`session-view-main.ts`、`independent-dom/management-peer.ts`、`independent-dom/view-wire.ts`、`independent-dom/vite.https.config.mjs`、`independent-dom/README.md`）在父基线 `a69da69e5d1cc6af6a15b3aa393359429c45ba58` 与受测提交之间的 Git blob 字节数与小写 SHA-256 摘要（注：`FIXTURE-SHA256.md` 内文本早于基线 `3204b4fe` R3 增补且受 `clients/web/tests/browser/` 只读约束保护不予改写），同时执行 `git diff --exit-code a69da69e5d1cc6af6a15b3aa393359429c45ba58 -- clients/web/tests/browser app clients/web/package.json`；2) 依次执行 `npm --prefix clients/web test`、`npm --prefix clients/web run build`、`npm run check`、`npm run check:spec` 与 `git diff --check`，将命令退出码、Vitest 实跑测试文件数与用例总数、Vite 生产包构建结果记录至 `testing/runs/refactor-web-workbench/`。
- 预期结果：`FIXTURE-SHA256.md` 所列全部 6 个浏览器夹具文件相对于父基线 `a69da69e5d1cc6af6a15b3aa393359429c45ba58` 的字节数与 SHA-256 摘要 100% 完全一致，`clients/web/tests/browser/`、`app/` 与 `clients/web/package.json` 运行时依赖保持零改动；`npm --prefix clients/web test`、`npm --prefix clients/web run build`（含 `tsc --noEmit` 与 `vite build`）、全仓 `npm run check`、`npm run check:spec` 及 `git diff --check` 全部以退出码 0、零告警、零错误通过，Vitest 实跑套件与用例数量大于重构前基线且 `src/main.ts` 生产入口正常完成打包。
