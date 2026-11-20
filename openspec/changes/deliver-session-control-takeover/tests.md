# 单会话客户端控制权接管与只读隔离验收测试套件

本文由独立 `tester` 依据 [proposal.md](proposal.md)、[design.md](design.md) 以及 [session-control-takeover](specs/session-control-takeover/spec.md)、[session-execution](specs/session-execution/spec.md)、[permission-approvals](specs/permission-approvals/spec.md) 三份增量规格与仓库真实代码边界编写，定义活跃变更 `deliver-session-control-takeover` 的可重复、可证伪验收用例。正式验收运行日志与证据落盘至本地被 Git 忽略的 `testing/runs/deliver-session-control-takeover/` 目录，精简逐例结论回填至同目录 `plan.md` 的 `Verification Results` 章节。

## 共用执行条件与真实边界

- **固定提交快照与角色独立性**：正式集成验收须在与独立 `reviewer` 共享的同一受测本地分支 Git `commit SHA`（核对 `git rev-parse HEAD` 且受跟踪工作区 `git status` 干净）上执行；`tester` 会话独立于 `coder` 开发会话，不以开发自评作为通过依据。
- **隔离数据目录、随机端口与受管进程树清理**：所有节点与端到端测试均使用独占临时数据根目录、独立 SQLite 数据库实例、操作系统动态分配空闲本地端口及隔离浏览器上下文/Profile，严禁读写本机真实用户数据目录、默认 HOME 会话或私有凭据；用例完成、失败或取消退出时必须完整终止并清理本次拉起的 `acp-remote` 节点进程、Windows Job 受管子进程树、浏览器上下文及临时目录。
- **L2–L4 协议、协调器与 Web 工作台模块边界**：L2/L3 覆盖 Rust `SessionAction` 与 Web `clients/web/src/management/session-protocol/control.ts` 闭合协议编解码、`SessionControlCoordinator` 内存代际状态机与有界请求幂等收据、`clients/web/src/workbench/session-control-controller.ts` 及全部 `src/workbench/*.ts` / `src/management/session-protocol/*.ts` 单文件 `<= 650` 行（`session-protocol.ts` `<= 120` 行）与零循环依赖图谱、`#session-input-body` 只读可聚焦与未发草稿光标选区保留不变量，以及 `clients/web/tests/browser/independent-dom/management-peer.ts` 对 `WV-13`–`WV-16` 独立浏览器回归的兼容维护。
- **L5 Windows 节点真实 WSS + SQLite + 受控 ACP V2 对端边界**：L5 在真实 Windows `NodeRuntime`、TLS/WSS 管理通道（`/api/access/v1/control`）、真实 SQLite（含 `security_audit` 追加审计与 `node_management_audit` 冻结表精确 DDL 校验）及标准 ACP V2 受控协议对端/确定性竞态屏障上执行，验证控制权专用主树根解析（`resolve_control_root`）、顶号抢占、跨代际两步 `register -> consume/decide` 拦截、连接私有 `input.upload.*` 失败自动释放且不触发 `4400` 断连、队列交错、审批托管交付、WSS/CLI 命名管道双入口撤销即时归零、可控单调时钟 30 分钟空闲回收递增代际归零与进程重启归零。
- **L6 真实二进制与双浏览器端到端及真实宿主边界**：L6 使用内嵌 Vite 生产前端产物的真实 `acp-remote.exe` 二进制与两个独立浏览器上下文（Client A / Client B 各自持有独立 `IndexedDB` WebCrypto 身份并经真实配对入网）执行完整工作台交互闭环；涉及真实 Codex Harness 的宿主链路在具备本机已认证专属测试环境时实跑验证，若执行环境缺少真实 Codex CLI 或有效认证凭据，对应真实宿主子步骤必须如实记录为 `not-run` 并说明缺失前置，严禁以模拟对端冒充真实宿主通过，也严禁将工具可验证场景滥用或降级为 `LX`（本变更全部判据均可由自动化测试与真实浏览器/协议工具验证，不设 `LX` 用例）。
- **日志与受跟踪文档卫生**：详细命令输出、数据库审计查询与浏览器交互轨迹统一保存在本地被忽略目录 `testing/runs/deliver-session-control-takeover/`，受跟踪 Markdown 文档仅使用行内代码反引号提及该路径，严禁使用 Markdown 超链接指向任何 `.gitignore` 忽略路径。

## 需求关联

| 当前增量路径 | Requirement | Scenario | 用例 |
| --- | --- | --- | --- |
| specs/session-control-takeover/spec.md | 会话控制权接管与设备信任分离 | 未接管会话不影响节点级操作与操控另一会话 | SCT-003、SCT-016 |
| specs/session-control-takeover/spec.md | 会话控制权接管与设备信任分离 | 子会话继承主执行树控制权 | SCT-004 |
| specs/session-control-takeover/spec.md | 主执行树唯一主控客户端与顶号式显式接管 | 新建主会话自动取得控制权 | SCT-001、SCT-003、SCT-016 |
| specs/session-control-takeover/spec.md | 主执行树唯一主控客户端与顶号式显式接管 | 另一可信客户端顶号式显式接管 | SCT-001、SCT-002、SCT-005、SCT-016 |
| specs/session-control-takeover/spec.md | 主执行树唯一主控客户端与顶号式显式接管 | 相同接管请求重试幂等 | SCT-006 |
| specs/session-control-takeover/spec.md | 未接管客户端只读浏览与按需历史加载 | 只读客户端查看运行中会话与拦截写操作 | SCT-002、SCT-007、SCT-016 |
| specs/session-control-takeover/spec.md | 未接管客户端只读浏览与按需历史加载 | 只读客户端查看空闲历史会话 | SCT-008 |
| specs/session-control-takeover/spec.md | 接管状态仅驻留活跃期并随释放或重启归零 | 主控客户端主动释放控制权 | SCT-006、SCT-014、SCT-016 |
| specs/session-control-takeover/spec.md | 接管状态仅驻留活跃期并随释放或重启归零 | 主控客户端被撤销信任后归零控制权 | SCT-014 |
| specs/session-control-takeover/spec.md | 接管状态仅驻留活跃期并随释放或重启归零 | 会话空闲回收或节点重启后控制权归零 | SCT-015、SCT-016 |
| specs/session-execution/spec.md | 输入受理与防重复 | 断线后重复提交 | SCT-009 |
| specs/session-execution/spec.md | 输入受理与防重复 | 未持有会话控制权提交输入被拒 | SCT-007、SCT-009 |
| specs/session-execution/spec.md | 输入受理与防重复 | 请求身份冲突 | SCT-009 |
| specs/session-execution/spec.md | 输入受理与防重复 | 已清理身份再次提交 | SCT-009 |
| specs/session-execution/spec.md | 多客户端共享查看和操作 | B 操作 A 的输入 | SCT-005、SCT-010、SCT-016 |
| specs/session-execution/spec.md | 多客户端共享查看和操作 | 未接管客户端只读查看队列 | SCT-002、SCT-007、SCT-010 |
| specs/session-execution/spec.md | 排队输入可编辑 | 编辑与发送交错 | SCT-011 |
| specs/session-execution/spec.md | 排队输入可编辑 | 旧主控在被顶号后提交编辑 | SCT-005、SCT-011 |
| specs/permission-approvals/spec.md | 唯一裁决与交付分离 | 两客户端同时审批 | SCT-012 |
| specs/permission-approvals/spec.md | 唯一裁决与交付分离 | 未接管客户端尝试审批被拒而顶号后可裁决 | SCT-012、SCT-016 |
| specs/permission-approvals/spec.md | 唯一裁决与交付分离 | 决定交付失败 | SCT-012 |
| specs/permission-approvals/spec.md | 受理决定由节点接管 | 审批者撤销后交付 | SCT-013、SCT-014 |
| specs/permission-approvals/spec.md | 受理决定由节点接管 | 节点接管后原请求失效 | SCT-013 |

## Cases

### SCT-001 会话控制权协议闭合编解码、可选期望代际与错误码映射验证

- 用例级别：L2
- 操作步骤：在仓库根目录运行 Rust 协议单元测试与 `npm --prefix clients/web test` 协议测试，逐项校验 `session.control.inspect`、`session.control.takeover`、`session.control.release` 请求与 `SessionControlSnapshot` 响应闭合字段（`targetId`、`rootTargetId`、`sourceId`、`rootNativeSessionId`、`controllerClientId`、`controllerDisplayName`、`controlEpoch` 十进制 `u64` 字符串）、服务端推送帧 `session.control.changed` 闭合键集合、`input.register`/`input.consume`/`approval.register`/`approval.decide`/`session.control.release` 中可选 `expectedControlEpoch` 闭集解析规则（分别测试省略该键、合法 canonical 十进制 `u64` 字符串、显式传入 `"expectedControlEpoch": null`、非字符串类型、负值/前导零/溢出数字字符串及附加未知额外字段），以及 `{ "code": "session_not_controlled" }` 单键错误在 `protocol.ts`、`error-messages.ts` 与 `error-formatting.ts` 中的类型识别和中文文案映射。
- 预期结果：Rust 与 Web 闭集解析器对合法控制权命令、快照、推送帧以及“省略 `expectedControlEpoch` 键”或“携带合法 canonical 十进制 `u64` 字符串”的请求 100% 往返一致；显式传入 JSON `null`（`"expectedControlEpoch": null`）、非字符串、负值/非 canonical/溢出数字字符串、非 canonical `id128` 或任何未知额外字段均被确定性拒绝为协议/命令格式错误（内部 `invalid_command` / 线上 `invalid_request`）；`parseCommandResult` 严格接受单键 `{ "code": "session_not_controlled" }` 且 `knownManagementError("session_not_controlled")` 返回 `true`，工作台错误格式化函数输出明确一致的未持有会话控制权中文提示。

### SCT-002 Web 工作台控制权模块预算、零循环依赖、只读草稿保留与浏览器夹具兼容验证

- 用例级别：L2
- 操作步骤：执行静态行数与模块有向图检查脚本核查 `clients/web/src/management/session-protocol.ts`、`clients/web/src/management/session-protocol/*.ts`（含 `control.ts`）与 `clients/web/src/workbench/*.ts`（含 `session-control-controller.ts`）的行数上限与 DFS 环路计数；运行 Vitest 工作台测试验证本机主控态、他人主控态与无主态三类状态横幅渲染、会话写动作 `<button>` 禁用门禁及 `#session-input-body` 在收到 `session.control.changed` 顶号推送或 `devices.changed` 重绘时的草稿与光标选区恢复；同步核对 `clients/web/tests/browser/independent-dom/management-peer.ts`（及 `FIXTURE-SHA256.md`）对 `session.control.inspect` 的模拟响应并运行 `WV-13`–`WV-16` 独立浏览器回归与 `npm run check`。
- 预期结果：`session-protocol.ts` 总行数 `<= 120` 行，全部 `session-protocol/*.ts` 与 `workbench/*.ts` 单文件总行数严格 `<= 650` 行，Web 全量模块强连通环与跨层反向依赖计数为 `0`；非本机控制态下“发送输入”、“恢复暂停队列”、“优先执行”、“编辑”、“取消”、“保存编辑”、“提交审批决策”按钮全部处于 `disabled = true`，而“接管会话 / 顶号接管”入口保持可用，`#session-input-body` 保持可聚焦且重绘前后本地未发送草稿文本与 `[selectionStart, selectionEnd]` 零丢失；`WV-13`–`WV-16` 与全仓构建检查全部退出码 0 通过。

### SCT-003 新建主会话自动授予初始控制权与节点级操作跨会话正交性验证

- 用例级别：L5
- 操作步骤：启动隔离 Windows `NodeRuntime` 并配对两个可信客户端 A 与 B；令 A 调用 `source.session` `create` 创建并加载主会话 S1 后执行 `target.register` 与 `session.control.inspect`，同时监听 A、B 管理连接推送；在 B 未接管 S1 的状态下，令 B 打开 S1 查询控制权快照、尝试向 S1 发起 `input.register`，随后令 B 依次调用 `node.info`、`device.list`、`catalog.*`、`workspace.register`、`session.capacity.inspect`、`native.release.status`，并由 B 独立调用 `source.session` `create` 新建另一主会话 S2 并向 S2 提交输入。
- 预期结果：A 新建 S1 完成 `bind_created_capacity`（`root_verified = 1`）时自动获得 S1 初始控制权（`controllerClientId = A`、`controllerDisplayName = A 展示名`、`controlEpoch = "1"`），在线连接收到 `session.control.changed` 推送；B 查看 S1 显示为只读且由 A 接管，B 对 S1 的写请求返回 `session_not_controlled`；B 的全部节点级查询/配置命令均正常成功，且 B 新建 S2 后自动获得 S2 控制权（`controlEpoch = "1"`）并成功向 S2 入队派发，S1 与 S2 控制权互不干扰。

### SCT-004 控制权专用主树根解析、子会话控制权继承、独立登记顶层目标与未验证会话树拦截验证

- 用例级别：L5
- 操作步骤：保持 `NativeCoordinator` 容量加载 `require_root` 不变，在隔离节点上针对 `resolve_control_root` 构造三类目标并分别由客户端 A、B 调用 `session.control.inspect`、`session.control.takeover`、`input.register` 与 `approval.register`：(a) 存在 `node_session_parents` 父链的子会话 `C1`（顶层祖先 `S1` 具备 `node_session_capacity.root_verified = 1` 且由 A 持控，分别覆盖根目标已注册 `rootTargetId != null` 与仅注册子目标 `rootTargetId == null` 两种情形）；(b) 无父链的独立已登记顶层目标 `T_reg`（经 CLI `target.register` 或 Web `discover -> target.register` 写入 `product_targets`、尚未进入 `node_session_capacity` 的顶层目标）；(c) 未验证会话树目标 `U1`（含父链顶层祖先缺失 `root_verified = 1` 的伪造/断链子会话，以及 `node_session_capacity` 中仅有 `root_verified = 0` 的未验证顶层记录）与不存在的目标 `X1`。
- 预期结果：(a) 子会话 `C1` 强制校验顶层祖先 `S1` 具备 `root_verified = 1` 后收敛至 `S1` 的 `rootNativeSessionId` 并继承 A 的控制权（仅注册子目标时 `rootTargetId` 为 `null`，根目标已注册时返回根 `targetId`），A 可直接操作 `C1` 而未接管的 B 对 `C1` 的全部写操作均被归并拦截返回 `session_not_controlled`；(b) 独立已登记顶层目标 `T_reg` 以自身作为控制权根键而不误报 `session_tree_unverified`，初始处于无主只读态（`controllerClientId = null, controlEpoch = "0"`），未接管写请求返回 `session_not_controlled` 且显式 `session.control.takeover` 可正常接管；(c) 未验证会话树目标 `U1` 精确返回 `session_tree_unverified`，不存在的目标 `X1` 返回 `invalid_target`。

### SCT-005 顶号式显式接管原子代际切换、在途任务保持与跨代际两步请求及上传暂存验证

- 用例级别：L5
- 操作步骤：令主控客户端 A（`controlEpoch = "1"`）在 S1 启动一轮长耗时在途 `Run`、排入待发送输入并产生有效待裁决权限请求；同时构造三组并发/跨代际动作后由客户端 B 调用 `session.control.takeover`：(1) A 携带旧 `expectedControlEpoch: "1"` 或省略该字段发出迟到写请求；(2) A 在 `epoch = 1` 完成 `input.register` / `approval.register` 后暂停，经历 `B 顶号 (epoch = 2)` 乃至 `A 再次顶号回切 (epoch = 3)` 后再提交旧登记的 `input.consume` / `approval.decide`；(3) A 在被顶号前完成连接私有 `input.upload.begin` + `ACPI` 二进制分块 + `input.upload.finish`，在被顶号后走 `submitSessionText` / `editSessionText` 触发 `input.register` 失败分支。
- 预期结果：B 顶号成功后 S1 原子切换为 `controllerClientId = B`、`controlEpoch = "2"` 并实时向 A、B 下发 `session.control.changed`（无需额外 `subscribe`）；S1 在途 `Run` 持续流式输出不中断，既有队列项与待决审批完整保留并可由 B 合法操控和裁决；A 的迟到写请求以及跨代际（含 `1 -> 2 -> 3` 回切）未完成两步 `consume`/`decide` 请求均被拒绝并返回 `session_not_controlled`；A 的连接私有 `ACPI` 上传不触发 `4400` 断连，且 `input.register` 返回 `session_not_controlled` 后客户端自动调用 `input.upload.release` 干净回收连接暂存槽位。

### SCT-006 显式接管与主动释放请求幂等缓存、参数冲突拒绝及同主控重接管稳定验证

- 用例级别：L3
- 操作步骤：针对 `SessionControlCoordinator` 执行确定性状态机与协议层测试：(1) 客户端 B 使用相同 `(actor_id, requestId)` 与相同 `targetId` 连续重试 `session.control.takeover`；(2) B 复用同一 `requestId` 但改传另一 `targetId` 或改调 `session.control.release`；(3) 已是当前主控的 B 更新展示名后使用新 `requestId` 再次对同一主会话调用 `session.control.takeover`；(4) 当前主控 B 使用相同 `(actor_id, requestId)` 重试 `session.control.release`，随后非主控客户端 A 或携带过期 `expectedControlEpoch` 的请求调用 `session.control.release`。
- 预期结果：(1) 同身份同参数接管重试直接命中内存幂等收据返回首次 `SessionControlSnapshot`，`controlEpoch` 不重复递增且不重复触发广播；(2) 同 `requestId` 异目标或异动作精确返回 `request_conflict`；(3) 已持控客户端新请求接管刷新 `controllerDisplayName` 但保持 `controlEpoch` 不变；(4) 主控释放同请求重试幂等返回同一无主快照，而非主控或代际失配的 `release` 精确返回 `session_not_controlled` 且不改变当前控制权与代际。

### SCT-007 未接管客户端实时只读旁观、排队项正文查阅与写操作零副作用拦截验证

- 用例级别：L5
- 操作步骤：令客户端 A 持有 S1 控制权并运行前台任务、排入含多段正文的队列项且挂起一个待处理权限请求；令未接管的可信客户端 B 调用 `view.open` 附着 S1 实时共享视图，并依次调用 `input.queue`、`input.body.open`（按需流式读取排队项正文）、`approval.list`、`approval.read` 与 `approval.status`；随后令 B 逐一尝试 `QueueMutation` 全部 5 类写动作（`submit`、`edit`、`cancel`、`prioritize` 立即发送/中断、`resume` 显式继续）以及 `approval.register` / `approval.decide`。
- 预期结果：B 的全部只读查看请求均成功返回，实时接收到与 A 一致的输出流、工具进展、停止原因、共享队列快照、完整排队项正文及待审批详情；B 发起的全部 5 类队列/执行变更与审批裁决请求均被节点拒绝为 `session_not_controlled`，SQLite 队列记录、请求消费状态、审批状态及底层 Harness 传输帧计数均保持零写副作用，A 的正常执行不受任何干扰。

### SCT-008 只读客户端打开空闲无主会话读取本地快照或触发标准历史重放且不自动授权验证

- 用例级别：L5
- 操作步骤：准备一个已完成对话并释放底层会话资源、当前处于空闲无主态（`controllerClientId = null`）的历史主会话 S1；在客户端 B 不调用 `session.control.takeover` 的前提下，分别在命中本地展示快照分支与需按 `native-session-recovery` / `session-resource-policy` 预留容量执行标准只读历史重放加载分支下调用 `view.open` / `view.open_source` 查看 S1 历史，并在加载前后调用 `session.control.inspect` 及尝试 `input.register`。
- 预期结果：两种分支下节点均允许 B 正常只读读取 S1 历史内容（标准重放分支正常纳入 50 主会话容量计数管理）；查看与重放全过程中 S1 始终保持无主只读态（`controllerClientId = null` 且 `controlEpoch` 不因查看而变化），绝不为历史查看或恢复加载自动授予控制权，B 在显式调用 `session.control.takeover` 前提交输入仍返回 `session_not_controlled`。

### SCT-009 输入受理控制权核验、主控断线重连幂等重试与登记身份冲突及清理边界验证

- 用例级别：L5
- 操作步骤：在真实 Windows 节点与 SQLite 请求登记表上依次执行四组输入受理验证：(1) 主控客户端 A 提交输入在节点落库受理后、响应到达前断开 WSS 连接（设备未撤销且 S1 仍加载），重连后先核查控制权归属再用原 `(actor_id, requestId)` 与相同内容重试 `input.register` / `input.consume`；(2) S1 分别处于无主态或由 A 接管时，未接管客户端 B 尝试提交新输入；(3) 主控 A 使用已登记的 `requestId` 改换 `targetId` 或改换输入正文再次提交，对照使用独立新 `requestId` 发起队列项编辑；(4) 在旧登记请求按生命周期完成安全清理退役后，A 再次携带该旧登记身份提交输入。
- 预期结果：(1) A 断线期间控制权保留在 A，重连重试直接返回原队列项受理结果，不重复入队或二次派发；(2) B 在无主态和他人持控态下的输入提交均返回 `session_not_controlled`，不占用 3 个共享队列名额也不触发派发；(3) 同身份异目标或异内容精确拒绝为冲突（`request_conflict`），独立新身份编辑按队列编辑规则正常受理；(4) 已安全清理的旧登记身份被明确拒绝（`request_retired`），不会因记录清理而恢复新提交资格。

### SCT-010 共享队列跨设备顶号移交操作与未接管客户端队列只读隔离验证

- 用例级别：L5
- 操作步骤：令主控客户端 A 向运行中（或因非成功结束暂停）的主会话 S1 排入 3 条待发送输入（`Q1`、`Q2`、`Q3`）；先令未接管的客户端 B 调用 `input.queue` 查看队列并尝试编辑 `Q1`、取消 `Q2` 或点击继续恢复队列；随后令 B 调用 `session.control.takeover` 显式顶号接管 S1，由 B 编辑 `Q1` 正文、取消 `Q2` 并触发优先发送或继续恢复，同时在退回只读态的 A 与第三观察客户端 C 上监听队列快照，并令 A 再次尝试编辑或取消剩余队列项。
- 预期结果：接管前 B 可完整读取 `Q1`–`Q3` 队列状态与停止原因，但 B 的编辑、取消与恢复请求全部返回 `session_not_controlled`，原队列内容与顺序分毫无损；B 顶号接管后可合法编辑 `Q1`、取消 `Q2` 及恢复/优先派发，节点不因换设备顶号而清空既有队列，A、B、C 三端实时同步看到完全一致的更新后节点队列事实，且退回只读态的 A 再次发起的队列变更请求被 `session_not_controlled` 拒绝。

### SCT-011 排队输入编辑与节点派发串行交错及被顶号后旧主控迟到编辑拒绝验证

- 用例级别：L5
- 操作步骤：在受控对端与节点派发器间设置确定性同步屏障测试两类交错场景：(1) 当前主控 A 编辑待发送项 `X` 与节点从队列取出 `X` 向 Harness 派发相邻发生，分别跑“编辑先进入互斥门”与“派发先进入互斥门”两个分支，核对队列名额、位置、操作者记录与实际派发载荷；(2) A 构造针对 `X` 的编辑请求并在网络/屏障延迟期间由客户端 B 完成 `session.control.takeover` 顶号接管，随后放行 A 延迟到达的 `input.register` 或 `input.consume`（编辑动作）。
- 预期结果：(1) 编辑仅在 `X` 未开始发送时保留原队列 ID 与位置（不额外占用名额并记录实际编辑者与新正文），编辑先处理则 Harness 精确收到更新后正文，派发先开始则编辑明确失败提示该项已发送/不可编辑且绝不改写已发内容；(2) B 顶号生效后到达的 A 迟到编辑请求无论停留在 `register` 还是 `consume` 阶段均被节点按当前主控身份与 `controlEpoch` 拒绝为 `session_not_controlled`，`X` 原正文保持不变。

### SCT-012 双端并发权限审批与顶号切换交错唯一裁决及交付未知状态隔离验证

- 用例级别：L5
- 操作步骤：令客户端 A 持有 S1 控制权并由受控对端发出有效真实权限请求 `P1`，分三组执行：(1) 未接管的客户端 B 直接对 `P1` 提交批准，被拒后 B 调用 `session.control.takeover` 顶号接管 S1 并再次提交批准，随后令原主控 A 再对 `P1` 提交拒绝；(2) 在新权限请求 `P2` 上通过并发屏障令“A 提交批准”与“B 顶号接管并提交拒绝”（以及未接管 B 直接并发提交）同时竞争；(3) 在新权限请求 `P3` 上令当前主控提交决定并可靠落库后，在底层 writer 交付确认前注入 Harness 连接断开故障并查询审批状态。
- 预期结果：(1) B 接管前的批准因 `session_not_controlled` 被拒，顶号后的批准被唯一接受并交付，退回只读的 A 后续裁决被拒；(2) 并发竞争下未持有有效当前控制代际的请求被确定性拒绝，`P2` 至多生效一个最终主控裁决，另一端收到明确冲突或 `session_not_controlled`，A 与 B 查询 `approval.read/status` 显示完全一致的唯一最终决定；(3) `P3` 保留已落库的唯一用户裁决并将交付结果明确标记为交付未知/未确认，绝不宣称 Harness 已执行，也不向重连后的新连接自动重发允许决定。

### SCT-013 已受理审批决定由节点托管交付及原请求失效安全阻断验证

- 用例级别：L5
- 操作步骤：在主控客户端 B 对有效权限请求提交决定并通过 `product_approvals` 事务持久化（`state = 'decided'`）处设置交付前屏障，分两组验证：(1) 决定落库后、节点向 Harness 写出响应前，分别令 B 断开管理连接、主动调用 `session.control.release`、被客户端 A 调用 `session.control.takeover` 顶号，或被管理员撤销 B 的设备信任，随后放行节点交付任务并令失去权限的 B 尝试发起新审批/新输入；(2) 决定落库后、交付前，分别令上游原权限请求取消、已答复或底层原生连接/广告选项失效，观察节点交付行为与存储记录。
- 预期结果：(1) 四种子场景下，只要原原生权限请求与连接仍然有效，节点均凭已固定的授权事实照常向 Harness 完成唯一交付，不因 B 断线、释放控制权、被顶号或被撤销而中断交付或要求重新授权，同时 B 随后发起的任何新写操作均按最新控制权/信任状态被拒绝；(2) 原请求取消、已答复或连接/选项失效时，节点严格阻断交付允许决定，完整保留原用户裁决记录并分别如实记录取消或未交付结果。

### SCT-014 主动释放控制权、设备信任双入口撤销即时归零及安全审计表无损约束验证

- 用例级别：L5
- 操作步骤：分三步验证控制权释放、信任撤销与 SQLite 审计约束：(1) 令持有 S1 控制权且有在途 `Run` 与排队输入的客户端 A 调用 `session.control.release`（并测试非主控 B 发起 `release`）；(2) 令 A 重新接管 S1（另建由 A 接管的 S2），先由可信客户端 B 通过 WSS `device.revoke` 撤销 A，再在另一对已接管会话上通过本机 CLI 命名管道 `acp-remote device revoke` 撤销当前主控客户端 C；(3) 检查 SQLite 数据库表清单、`node_management_audit` 表结构校验（`validate_management_audit`）及 `security_audit` 表中控制权事件行。
- 预期结果：(1) A 主动释放后 S1 立即归零为无主只读态（`controllerClientId = null`、`controlEpoch` 递增 `+1`）并广播 `session.control.changed`，底层会话资源、在途 `Run` 与已入队输入继续保留，非主控 B 的 `release` 返回 `session_not_controlled`；(2) WSS 与本机 CLI 命名管道两类撤销成功提交后均无死锁地立即清空被撤销设备持有的全部主会话控制权（置 `null` 并递增 `controlEpoch`、广播变更），被撤销端管理连接失效，撤销前已受理输入与已落库审批继续由节点托管完成，其余可信端可立即显式接管；(3) `security_audit` 完整记录 `'session.control.initial'`、`'session.control.takeover'`、`'session.control.release'`、`'session.control.revoke_clear'` 的根会话键与前后 `controlEpoch` 摘要，`node_management_audit` 零改动且不存在封禁的 `session_commands` 表。

### SCT-015 会话资源释放、三十分钟空闲回收同进程代际递增归零及节点重启归零验证

- 用例级别：L5
- 操作步骤：在隔离节点上令客户端 A 接管主会话 S1（`controlEpoch = "1"`），依次覆盖四条生命周期归零与跨释放周期防重放路径：(1) A 在 `epoch = 1` 发起 `input.register` / `approval.register` 后暂不提交第二阶段，在 A 离线情况下由未接管的客户端 B 发起全局容量手动释放 `native.release`（会话已安全空闲）以及容量满额淘汰 `CapacityPressure` 触发 `NativeCoordinator::capacity_closed`（含空闲记录淘汰水位下界测试），随后令 A 重新调用 `session.control.takeover` 接管 S1 并尝试提交释放前滞留的旧 `input.consume`/`approval.decide` 或携带旧 `expectedControlEpoch: "1"` 的请求；(2) 使用可控单调时钟推进已接管会话安全空闲满 30 分钟触发 `IdleTimeout` 自动回收，并对一个尚未加载 `CapacityResource` 即提前执行 `takeover`（记录 `unloaded_claimed_at`）的冷会话推进满 30 分钟未加载超时；(3) 在 S2 仍由 A 持有控制权时正常停止并重启同一数据目录下的 `NodeRuntime` 进程，重启后令 A 与 B 重新连接并打开 S2 查询 `session.control.inspect` 及尝试直接提交输入。
- 预期结果：(1) 主控 A 离线不阻塞其他可信客户端对安全空闲会话执行 `native.release`；在同一 `NodeRuntime` 进程运行期内，手动释放与满额淘汰完成 `capacity_closed` 清空主控（置 `controllerClientId = null, controllerDisplayName = null`）时严格单调递增 `controlEpoch`（`+1`，绝不在同进程内倒退重置为 `"0"`，即使淘汰空闲条目亦保留代际水位下界）并以递增后的新 `controlEpoch` 广播 `session.control.changed`，A 再次接管后 `controlEpoch` 继续递增，其在释放前滞留的旧两步 `consume`/`decide` 与旧 `expectedControlEpoch` 请求均被拒绝为 `session_not_controlled`（彻底阻断跨释放周期 ABA 绕过）；(2) 30 分钟空闲自动回收与冷会话预接管 30 分钟未加载超时在清空主控时同样单调递增 `controlEpoch`（`+1`）并广播归零为无主只读态；(3) 节点进程重启后全新内存协调器为空且不从持久化表重放重启前收据，A 与 B 重新连接打开历史会话 S2 均表现为初始无主只读态（`controllerClientId = null, controlEpoch = "0"`），继续提交输入前须重新调用 `session.control.takeover`。

### SCT-016 Windows 真实节点与双独立浏览器工作台控制权接管及顶号移交端到端闭环验证

- 用例级别：L6
- 操作步骤：编译内嵌最新前端静态产物的真实 `acp-remote.exe`，在隔离临时数据根与随机端口启动 Windows 节点，拉起两个独立无痕浏览器上下文（Client A 与 Client B 各自经真实配对取得独立可信设备身份），接入真实 Codex Harness（若受测机器缺少真实 Codex CLI 或有效认证凭据，在受控标准 ACP V2 对端上跑通完整浏览器工作台双端闭环，并将真实 Codex 宿主子项如实记为 `not-run`）：(1) 在 Client A 页面新建主会话 S1 并提交输入，在 Client B 页面打开 S1 旁观流式输出、队列快照与排队项正文，同时在 B 的 `#session-input-body` 输入未发草稿；(2) 当 S1 存在排队项与待决审批且 Client A 的 `#session-input-body` 含有未发送草稿及选中光标区间时，在 Client B 页面点击“顶号接管会话”并由 B 编辑队列项、提交审批裁决；(3) 在 Client B 点击“释放控制权”，随后重启 `acp-remote.exe` 并令双浏览器重连打开 S1，最后清理节点进程树、Windows Jobs、浏览器实例与临时目录。
- 预期结果：(1) A 新建 S1 后工作台横幅显示“当前由本设备接管 · 代际 1”与“释放控制权”按钮，B 打开 S1 实时显示“只读观察 · 当前由 `<A 展示名>` 接管”与“顶号接管会话”按钮且全部写操作按钮禁用，B 可实时旁观输出流、队列及待审批详情；(2) B 点击“顶号接管会话”后实时升为“当前由本设备接管 · 代际 2”并成功编辑 A 遗留队列项及完成审批裁决，A 无需刷新即刻切回只读态并禁用写按钮，且 A 输入框 `#session-input-body` 内的未发送草稿文本与光标选区完整保留；(3) B 释放控制权后双端同步显示“只读观察 · 当前无人接管”（代际 3），节点重启重连后双端打开 S1 均处于无主只读态（代际 0）直至显式点击接管，退出后零残留子进程与临时锁文件。
