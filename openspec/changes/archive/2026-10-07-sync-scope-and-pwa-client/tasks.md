<!-- 英文标题、中文正文，保留 - [ ] X.Y；每项写 WP/TP 或单元、负责人、显式依赖、动作、完成条件及 ID/计划引用。 -->

## 1. Dependency and Resource Setup

- [x] 1.1 用新的任务级最小上下文核实仓库、目标引用、工具版本、约定命令和资源，返回结构化事实、原始命令/输出及证据
      - 由 provisioner 实例 ProvisionerW1 完成：`git rev-parse refs/heads/main` → `353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f`，与 plan 的 `## Target Repository and Main Branch` 一致；仓库根 `node_modules/` 结构完整、十个 check 入口脚本齐全；`npm run check` exit 0。证据 `reports/provision-w1.json`、`reports/provision-w1-check.log`
- [x] 1.2 WP1 / WP2 确认实现所需契约、契约冻结版本、文件归属与合并负责人，记录编码起点；两者无 code 与 contract 依赖，W1 即可开工
- [x] 1.3 仅为依赖就绪且有池容量的包按 plan 分配/隔离资源或排独占队列：WP1/WP2 随 worktree 提供仓库根 `node_modules/` 只读访问；WP2 按 WP 分配独立 `CARGO_TARGET_DIR=.target-wt/wp2`；`.worktrees/wp1`、`.worktrees/wp2` 已建并各自在 worktree 内核实基线。按 (WP, Attempt) 返回 worktree、基线、认领执行者、开工前接收时间，main 校验后登记 verification 的 `## Worktree Handoff`；不预建后续包
      - 后续包（WP3 起）就绪后再逐包准备，不在本轮预建
- [x] 1.4 WP3 / WP4 / WP5a / WP5b / WP6 / WP7 / TP2 / TP3 / TP4 集成验证前接入全部已验收上游，核对实际基线/提交包含关系并引用 verification 交接证据（对应 plan 的 Dependency Handoffs 表）
- [x] 1.5 全变更：WP1/WP2 在各自交付时冻结 `schemas/sync/v1/**`、`fixtures/sync/v1/**`、`crates/sync-protocol/src/{views,command,sync}.rs` 的路径与版本并登记 plan 的 Shared File Ownership；下游 WP 引用该冻结版本作为 Contract Freeze

## 2. Implementation

- [x] 2.1 [wp:WP1] [PV1] 派发 WP1 coder，按 apply 协议登记开工/接收；交付后销毁作者、同窗口新建独立 reviewer 并确认接收，review PASS 后 main 勾选
      - WP1：实现 R1–R4。收窄 `sync.schema.json#/$defs/snapshotResource` 的实际下发范围为 `sessions`/`workspaces`/`agents` 并使 `chunkCount` 口径一致；给 `sessionRead` 增加可选 `before`（`(createdAt, messageId)` 复合游标）与 `limit`、给 `sessionReadResult` 增加「是否仍有更早内容」；按 D2 在 `docs/SYNC_PROTOCOL.md` §11.5 写明游标不得依赖 UUID、单字段游标被拒、默认页为最近 20 条用户输入；按 D3 在该表写明分页只作用于 Sync 面。依赖：无。完成条件：`npm run check` 全绿（`check:schemas` 与 `check:fixtures` 双向门禁），证据 `reports/PV1.log`
      - WP1 的写入范围含 `crates/sync-protocol/src/command.rs` 与 `crates/sync-protocol/src/sync.rs`：它们分别是 `command.schema.json` 与 `sync.schema.json` 的 Rust 类型化镜像，必须与 schema 同批更新，否则 `tests/schema_drift.rs` 门禁失败。`sync.rs` 的归属是在 apply 实施中补登记的——实现实例发现该文件无人认领，收窄 schema 后 Rust 侧会比合同更宽松且两条负例夹具失败；plan 的 Write Scope 与 Shared File Ownership 已同步更新并经 DDR Round 5 复核。
- [x] 2.2 [wp:WP2] [PV1] 派发 WP2 coder，按 apply 协议登记开工/接收；依赖已合入且资源就绪即开工，不等其他包
      - WP2：实现 R5–R9 的字段侧。给 `event-views.schema.json#/$defs/file.changed` 增加可选 `addedLines`/`deletedLines`（decimalString）与 `outsideWorkspace`（boolean），必填集合不变；把 `agent.connected`/`agent.disconnected` 的 `state` 收成封闭枚举并**同步登记 `crates/sync-protocol/src/views.rs` 的 `VIEW_ENUMS` 表**（否则 `tests/schema_drift.rs` 双向门禁失败），`views.rs` 的 `FileChanged` 与两个 Agent 视图同步加字段；更新对应 fixture 与 `fixtures/sync/v1/manifest.json`；在 `docs/SYNC_PROTOCOL.md` §10.3 写明三字段与「只统计 Agent 在工具调用中声明的编辑」的口径；按 D7 写明标题单向由 Agent 通知更新且无重命名命令。依赖：无。完成条件：`npm run check` 全绿，证据 `reports/PV1.log`
- [x] 2.3 [wp:WP3] [PV1] [PV2] 派发 WP3 coder，按 apply 协议登记开工/接收
      - WP3：实现 R5–R9 的实现侧。在 core 派生 `file.changed`：按 D4 对 ACP `Diff` 的 `oldText`/`newText` 跑行级 Myers diff 得 `addedLines`/`deletedLines`，判定不出时省略而非填零；按 D5 用规范化后的前缀关系做相对化，越界置 `outsideWorkspace` 且只下发文件名；按 D6 落库节点级 `agent.connected`/`agent.disconnected`（`session_id` 为空，不新增 migration）；按 D7 给 `SessionUpdate` 增加两层可选标题字段并把 `session_info_update` 投影为标题更新与 `session.info.changed`，`updatedAt` 取 Daemon 持久化时间而非 Agent 自报值。同步更新 `docs/CORE_PORTS_AND_STORAGE.md` §5 使 `check:drift` 两侧同时绿。依赖：`contract:WP1`、`contract:WP2`——**不依赖 WP1/WP2 的代码**：`crates/core` 的依赖闭包不含 `sync-protocol`（`check-crate-boundaries.mjs` 的 `CORE_FORBIDDEN`），事件载荷是 `crates/core/src/model/json.rs` 的 `ViewJson` 原始 JSON。完成条件：PV1 + PV2 全绿
- [x] 2.4 [wp:WP4] [PV1] [PV2] 派发 WP4 coder，按 apply 协议登记开工/接收
      - WP4：实现 R8/R21/R22。`agent-host` 在转发工具调用时把 `ToolCallContent::Diff` 元素连同逐字节原始文档一并交付给事件 sink，MUST NOT 从自由形状 `rawInput` 推断文件改动；按 profile 进程的生命周期上报连接与断开，复用既有进程不重复上报连接，超限退出时断开上报 MUST NOT 早于该运行时被标记为已退出。依赖：`contract:WP2`——**不依赖 WP2 的代码**：`crates/agent-host` 不 import `sync-protocol`，经 `EndpointEvent` + `EventSink` 向 core 上报，载荷同为 `ViewJson`。完成条件：PV1 + PV2 全绿
- [x] 2.5 [wp:WP5a] [PV3] 派发 WP5a coder，按 apply 协议登记开工/接收；依赖无上游，契约起点
      - WP5a：实现 R10/R15/R19 的基础层。建 `clients/app/` 工程骨架（`app/` 空壳路由、`package.json`、tsconfig 与构建配置）与七层目录中的 `src/domain/`、`src/protocol/`、`src/components/`；`src/protocol/` 严格 typed 地消费 `schemas/sync/v1/**` 与 `fixtures/sync/v1/manifest.json`，不做宽容解析、不复制测试样例；`src/domain/` 提供目录、会话摘要、Agent 目录条目的视图模型。写入范围**不含** `src/platform/`（归 WP5b）。依赖：`contract:WP1`、`contract:WP2`——前端把 schema 与 fixture 当**数据**读取，不链接 WP1/WP2 的编译产物。完成条件：PV3 全绿
- [x] 2.6 [wp:WP5b] [PV3] 派发 WP5b coder，按 apply 协议登记开工/接收；依赖其上游包的交付提交已合入（MU3a）
      - WP5b：实现 R11 与 R20 的平台侧，写入范围只有 `clients/app/src/platform/`。`secure-storage.web.ts` 用 WebCrypto 生成**不可导出**的 P-256 `CryptoKey` 并持久化到 IndexedDB，MUST NOT 以任何可导出形式落盘；另含 `local-cache.web.ts`（R20 的 imported 资源不落盘与本节点摘要缓存上限/TTL/LRU）与 `lifecycle.web.ts`。**不写 `.native.ts`**：v1 只交付 Web，原生端不在本次范围，该决定写进本任务而非靠「没人做」达成。跨语言字节对拍（`fixtures/sync/v1/transcripts/device-proof.json`）由 TP3 执行，本包只交实现。依赖：`code:WP5a`。完成条件：PV3 全绿
- [x] 2.7 [wp:WP6] [PV3] 派发 WP6 coder，按 apply 协议登记开工/接收
      - WP6：实现 R2/R3/R12–R14/R19/R20。`src/sync-client/` 完成连接、逐连接签名握手、`subscribe`、ACK、增量重放、快照暂存与原子替换（按 D1 只有清单，digest 只需覆盖已到资源）；`src/state/` 以互斥状态机表达 `specs/pwa-web-client/spec.md`「连接状态是互斥状态机」枚举的全部状态（8 个常规态：未配对/配对中/已断开/连接中/认证中/重放追平中/在线/重连中，加 4 个阻断态：被撤销/版本不兼容/主机身份变化/被其它标签页顶替，合计 12 态）与命令 6 态，MUST NOT 用可同时成立的布尔组合；命令以稳定 `requestId` 重试或查状态，仅在服务端确认时才呈现结果不确定；事件按 `eventId` 去重；会话明细按 `(createdAt, messageId)` 游标翻页；Agent 连接事件作为**可选**覆盖层叠加在快照目录之上，缺席时呈现状态未知，MUST NOT 用会话活跃程度推断。依赖：`code:WP5a`、`code:WP5b`、`contract:WP1`。完成条件：PV3 全绿（Round 9 的 F37：原写「连接 9 态」，取自原型下拉项数，与本变更 spec 枚举的 12 态不符，会让 TP3 的覆盖完整性判定漏掉配对/连接中/认证中三态）
- [x] 2.8 [wp:WP7] [PV3] 派发 WP7 coder，按 apply 协议登记开工/接收
      - WP7：实现 R15–R19。目录页（以目录为主角、只显示展示名与别名、同名以别名区分、两种空态可区分、离线仍可渲染）、目录详情、对话页（含上下文条的「未上报」态、滚动加载更早的用户输入）、配对页、主机与连接面板（Agent 列表来自快照；连接状态在事件通道就绪前呈现「状态未知」）、降级卡片与诊断抽屉（原文未同步时不渲染占位、三类不支持可区分）。页面 MUST NOT 直接触碰 WebSocket/IndexedDB/CryptoKey，协议载荷 MUST NOT 进入视图模型。依赖：`code:WP6`、`code:WP5a`、`code:WP5b`、`contract:WP1`、`contract:WP2`（与 `plan.md` WP7 行的 Dependencies 列一致；Round 9 的 F36：原写 `code:WP5`，该 ID 已随拆分退役，无法解析到任何 worktree 或交付提交）。完成条件：PV3 全绿

## 3. Branch Validation

- [x] 3.1 WP1 完成 [PV1] 的交付前 Project Verify，逐项留完整命令、版本/配置、资源、退出码与 `reports/PV1.log`，确认 `CARGO_TARGET_DIR` 资源释放
- [x] 3.2 WP1 作者销毁后，同窗口新建不继承实现对话的只读 reviewer（review-1，Review Type: code）检视固定版本；修复/复核各用新实例，记录 Agent ID、版本、隔离与报告
- [x] 3.3 WP2 完成 [PV1] 的交付前 Project Verify，留 `reports/PV1.log` 并确认资源释放
- [x] 3.4 WP2 新建独立 reviewer（review-2）检视固定版本，重点核对 `VIEW_ENUMS` 登记完整性与「排序不得依赖 UUID」在文档中的落点；修复后独立复核留证
- [x] 3.5 WP3 完成 [PV1] 与 [PV2] 的交付前 Project Verify，留 `reports/PV1.log`、`reports/PV2.log`
- [x] 3.6 WP3 新建独立 reviewer（review-3）检视固定版本，重点核对行级 diff 的边界输入、路径规范化前缀判定、节点级事件的三条 DDL CHECK 自洽、ACP 原文三要素未变；修复后独立复核留证
- [x] 3.7 WP4 完成 [PV1] 与 [PV2] 的交付前 Project Verify，留 `reports/PV1.log`、`reports/PV2.log`
- [x] 3.8 WP4 新建独立 reviewer（review-4）检视固定版本，重点核对复用既有进程不重复上报连接、超限退出时序、`crates/core/src/ports.rs` 与 WP3 的合并不破坏 `check:drift`；修复后独立复核留证
- [x] 3.9 WP5a 完成 [PV3] 的交付前 Project Verify，留 `reports/PV3.log`
- [x] 3.10 WP5a 新建独立 reviewer（review-5）检视固定版本，重点核对七层目录与分层依赖方向、协议 DTO 未泄漏进视图模型、原生 adapter 未被写入；修复后独立复核留证
- [x] 3.11 WP5b 完成 [PV3] 的交付前 Project Verify，留 `reports/PV3.log`
- [x] 3.12 WP5b 新建独立 reviewer（review-5b）检视固定版本，重点核对密钥不可导出、IndexedDB 持久化、私钥不出现在任何字符串存储；修复后独立复核留证
- [x] 3.13 WP6 完成 [PV3] 的交付前 Project Verify，留 `reports/PV3.log`
- [x] 3.14 WP6 新建独立 reviewer（review-6）检视固定版本，重点核对互斥状态机无非法组合、`requestId` 复用、快照暂存原子替换、明细游标拼接不重不漏；修复后独立复核留证
- [x] 3.15 WP7 完成 [PV3] 的交付前 Project Verify，留 `reports/PV3.log`
- [x] 3.16 WP7 新建独立 reviewer（review-7）检视固定版本，重点核对目录项不含路径、同名以别名区分、无权限态说明、原文未同步不渲染占位；修复后独立复核留证
- [x] 3.17 TP1 完成 [PV1] 的交付前 Project Verify，留 `reports/PV1.log`
- [x] 3.18 TP1 新建非作者 reviewer（testing-1 之外）检视固定版本；修复/复核各用新实例并留证
- [x] 3.19 TP2 完成 [PV2] 的交付前 Project Verify，留 `reports/PV2.log`
- [x] 3.20 TP2 新建非作者 reviewer 检视固定版本；修复/复核各用新实例并留证
- [x] 3.21 TP3 完成 [PV3] 的交付前 Project Verify，留 `reports/PV3.log`
- [x] 3.22 TP3 新建非作者 reviewer 检视固定版本；修复/复核各用新实例并留证
- [x] 3.23 TP4 完成 [PV3] 的交付前 Project Verify，留 `reports/PV3.log` 并确认浏览器 profile 资源清理
- [x] 3.24 TP4 新建非作者 reviewer 检视固定版本；修复/复核各用新实例并留证

## 4. Test Design and Authoring

- [x] 4.1 [AC1] main 执行替代检查 AC1：扩展 `crates/app/tests/node_link_e2e.rs`（真实 Daemon 进程、真实 SQLite、fake ACP Agent），断言 `file.changed`（会话级）落库并按 origin 顺序回放、`agent.connected`（节点级）落库后会话标识为空且不被会话级投递路径误收、会话标题在 `session_info_update` 后更新。**不验证广播**：节点级事件在 `server::sync` 落地前没有投递通道。结果与完整日志写入 verification 的 `## Checks`，证据 `reports/AC1.log`
- [x] 4.2 [AC2] main 执行替代检查 AC2：运行 `npm run verify`（PV1 全量），确认 `check:drift` 对 `docs/CORE_PORTS_AND_STORAGE.md` §5 与 `crates/core/src/ports.rs` 的逐字比对、`check:schemas` 与 `check:fixtures` 的双向门禁全绿；证据 `reports/AC2.log`
- [x] 4.3 [AC3] main 执行替代检查 AC3：运行 `clients/app` 的契约测试，确认其读同一份 `fixtures/sync/v1/manifest.json` 与 `schemas/sync/v1/**`，并对 `fixtures/sync/v1/transcripts/device-proof.json` 完成跨语言字节级对拍；证据 `reports/AC3.log`
- [x] 4.4 [wp:TP1] [PV1] TP1、testing-1，设计需求映射、稳定用例 ID、场景/步骤/断言，覆盖 R1–R9 的机器可判定部分，以 `phase: design` / `evidence_type: DESIGN` 交付
      - 并行与冻结：契约明确时可与两个上游契约包的编码并行，仅在引用具体固定向量版本时等待冻结。**依赖声明放在本子条目而非派发行**：`planning-model.mjs:37-40` 的 `taskPackages` 只取每个 `- [ ] N.M` 行的同一行文本，包 ID 出现在派发行会把上游包的 Coverage 一并计入本任务（Round 11 的 F46 实测使 WP1 由语义正确的 17 行涨到 46 行、WP2 由 29 行涨到 46 行，sources 额外含 `core-derived-events` 与 `workspace-resolution` 两份 spec）
- [x] 4.5 [PV1] TP1 交付可运行的 fixture 与 Rust 契约测试：清单快照向量、会话明细分页与复合游标向量、`file.changed` 新字段向量、`state` 枚举向量、以及 `node-link-protocol` DTO 未受影响的断言；以 `phase: design-author` 登记 DELIVERY/CHECK，附命令、零退出码与 `reports/PV1.log`
- [x] 4.6 TP1 新建非作者 reviewer 审查用例的需求映射、真实入口与断言，修正后独立复核留证；缺失用例与盲区交 main/validator
- [x] 4.7 [wp:TP2] [PV2] TP2、testing-2，设计覆盖 core 生产者行为的用例：Diff 派生唯一性、行级 diff 的「行数相等但内容不同」边界、新建/删除文件口径、判定不出时省略、同名前缀误判防护、节点级事件的 `session_id` 为空与复用不重复上报、标题的部分更新与显式清空、以及命令目录中不存在重命名命令；以 `phase: design` 交付，依赖 `code:WP3`
- [x] 4.8 [PV2] TP2 交付可运行的 core 与 storage 测试，基础检查通过后以 `phase: design-author` 登记 DELIVERY/CHECK，附命令、零退出码与 `reports/PV2.log`
- [x] 4.9 TP2 新建非作者 reviewer 审查用例覆盖，修正后独立复核留证
- [x] 4.10 [PV3] TP3、testing-3，设计覆盖前端契约与状态机的用例
      - 覆盖范围：与 `device-proof.json` 的固定向量对拍；连接 12 态（8 常规态 + 4 阻断态，逐项取自 `specs/pwa-web-client/spec.md` 的枚举）的全部合法迁移与阻断态停止重连；命令 6 态与「仅服务端确认才呈现结果不确定」；`eventId` 去重与快照暂存失败不覆盖已完成状态；上下文三态（已知/未上报/零窗口）；三类不支持可区分；imported 资源正文与 `acp.rawJson` 不进入任何持久浏览器存储。以 `phase: design` 交付
      - 依赖：`code:WP5a`、`code:WP5b`、`code:WP6`、`code:WP7`（测试与被测模块同处一个 TS 工程，必须 import 上游模块才可运行）与 `contract:WP1`（固定向量与 schema）。**依赖声明放在本子条目而非派发行**：`planning-model.mjs:37-40` 的 `taskPackages` 只取每个 `- [ ] N.M` 行的同一行文本，包 ID 出现在派发行会把上游包的 Coverage 一并计入本任务（Round 9 的 F35 与 Round 10 的 F41 同源；F41 实测使 WP5a 由 12 行膨胀到 42 行）
- [x] 4.11 [wp:TP3] [PV3] TP3 交付可运行的 `clients/app` 测试，基础检查通过后以 `phase: design-author` 登记 DELIVERY/CHECK，附命令、零退出码与 `reports/PV3.log`
- [x] 4.12 TP3 新建非作者 reviewer 审查用例覆盖，修正后独立复核留证
- [x] 4.13 [PV3] TP4、testing-4，设计覆盖浏览器可观察行为的用例
      - 覆盖范围：目录页在断连下仍渲染且目录项只含展示名与别名；两种空态可区分；创建会话提交载荷只含 `workspaceAlias` 与 `agentId`；无权限态入口不可用且有说明；原文未同步时降级卡片不渲染占位；三类不支持可区分。以 `phase: design` 交付
      - 依赖：`code:WP7`（与 4.10 同理，依赖声明放在子条目，避免上游包 ID 进入派发行而污染 Coverage 归属，见 Round 10 的 F41）
- [x] 4.14 [wp:TP4] [PV3] TP4 交付可运行的浏览器用例与启动脚本（`clients/app/e2e/`），基础检查通过后以 `phase: design-author` 登记 DELIVERY/CHECK，附命令、零退出码与 `reports/PV3.log`
- [x] 4.15 TP4 新建非作者 reviewer 审查用例的稳定选择器、需求映射与断言，修正后独立复核留证

## 5. Integration Readiness

- [x] 5.1 main 创建独立 merger，交接 `roles/merger.md` 全文、plan/契约、固定源提交与证据、单元复用 worktree、本地主分支与合入条件，记录实际 ID 与上下文；main 不兼任，缺能力则 BLOCKED
- [x] 5.2 MU1a、main 复核 independent 模式的 WP1/WP2 组成与有效交付检查及 review 证据（MU1a 不含 TP：测试向量归 MU1b）；变化先同步计划/依赖
- [x] 5.3 MU2、main 复核 independent 模式的 WP3/WP4/TP2 组成，确认上游 **MU1a/MU1b 已合入主分支**且契约冻结版本可读（与 `plan.md` MU2 行的 Start/Readiness「MU1b 已合入主分支且契约冻结可读」对齐；Round 8 的 F23：原措辞只写 WP1/WP2，会在 TP1 尚未合入时误判就绪）；变化先同步计划/依赖
- [x] 5.4 MU1b、main 复核 independent 模式的 TP1 组成与交付检查；TP1 须从 MU1a 的合入提交分支，其断言在该固定提交上编译并跑通；变化先同步计划/依赖
- [x] 5.5 MU3a、main 复核 independent 模式的 WP5a 组成，确认 MU1b 已合入（契约冻结可读）；变化先同步计划/依赖
- [x] 5.6 MU3b、main 复核 independent 模式的 WP5b 组成，确认 MU3a 已合入且 WP5a 的交付提交可读、`fixtures/sync/v1/transcripts/device-proof.json@WP1` 已冻结；变化先同步计划/依赖
- [x] 5.7 MU3c、main 复核 independent 模式的 WP6 组成，确认 MU3a/MU3b 已合入且 `schemas/sync/v1/command.schema.json@WP1` 已冻结；变化先同步计划/依赖
- [x] 5.8 MU3d、main 复核 independent 模式的 WP7 组成，确认 MU3a/MU3b/MU3c 已合入；变化先同步计划/依赖
- [x] 5.9 MU3e、main 复核 independent 模式的 TP3/TP4 组成，确认 MU3a–MU3d 已合入，且 TP3/TP4 的断言在固定提交上编译并跑通；变化先同步计划/依赖

## 6. Merge Unit

- [x] 6.1 MU1a、merger；候选构建前机械核实目标仓库、主分支准确引用与当前提交并留证（区别于 1.1 的 scout 调查）；6.6 合入瞬间再核实，目标不明则 BLOCKED
- [x] 6.2 MU1a、合入负责人；基于已核实基线（`refs/heads/main`）构造 MU1a 候选（WP1 + WP2；候选已在 `.worktrees/mu1-merge` 组装为 683dbbb8，组装后重新核实并固定基线与候选版本），记录组成与构建结果
- [x] 6.3 MU1a、检查执行者；按 [PV1] 完成候选 Project Verify，将版本、范围、结果关联到 verification 的 `## Checks`，证据 `reports/PV1.log`。**必须实测** `check:assets` 的用例计数类门禁能否在 MU1a 通过——`envelope_fixtures.rs` 的两个计数常量由 WP1 在其自身 worktree 对自身向量算出，组装后条目总数可能对不上
- [x] 6.4 MU1a、独立 reviewer；只读审查固定候选的新增交互与 `fixtures/sync/v1/manifest.json`、`docs/SYNC_PROTOCOL.md` 的冲突解决，修复后独立复核，留隔离/版本/报告
- [x] 6.5 MU1a、main；核对 Coverage Index 中归属本单元的 Scenario 的实现、候选 Project Verify/review 可追溯；缺口重开任务
- [x] 6.6 MU1a、merger；以 `agentic-premerge` 块运行 `workflow check --stage premerge`，PASS 后持久化版本化 receipt 并登记 verification 的 `## Premerge History`（MU1a、WP1/WP2、目标与候选提交、PASS、receipt 路径）；复核仓库规则与本地主分支基线后直接本地合入并记实际提交
- [x] 6.7 MU1a、检查执行者；核对主分支与候选一致性并完成 [PV1] 主分支回归，证据 `reports/PV1-main-mu1a.log`
- [x] 6.8 MU1a、独立 reviewer；审查合并新增差异，无差异时由 main 记依据与原 review ID
- [x] 6.9 MU1b、合入负责人；MU1a 合入后从其合入提交分支重建 TP1 的 worktree（丢弃当前基线副本——merger 证实它是修复前的过期镜像），在该固定提交上跑 [PV1] 并复算 `envelope_fixtures.rs` 的最终计数，固定 MU1b 候选
- [x] 6.10 MU1b、检查执行者；按 [PV1] 完成候选 Project Verify
- [x] 6.11 MU1b、独立 reviewer；只读审查 TP1 向量与门禁断言
- [x] 6.12 MU1b、main；核对 Coverage Index 中归属 TP1 的 Scenario
- [x] 6.13 MU1b、merger；`workflow check --stage premerge`、receipt、合入
- [x] 6.14 MU1b、检查执行者；核对主分支与候选一致性并完成 [PV1] 主分支回归，证据 `reports/PV1-main-mu1b.log`
- [x] 6.15 MU1b、独立 reviewer；审查合并新增差异，无差异时由 main 记依据与原 review ID（Round 8 的 F24：本组原缺「合入后主分支差异复检」与独立回归两项登记，与 MU1a/MU2 组的分工不一致）

- [x] 6.16 MU2、merger；候选构建前机械核实目标仓库与主分支当前提交并留证
- [x] 6.17 MU2、合入负责人；构造 MU2 候选（WP3 + WP4），固定基线与候选版本
- [x] 6.18 MU2、检查执行者；按 [PV1] 与 [PV2] 完成候选 Project Verify，证据 `reports/PV1.log`、`reports/PV2.log`
- [x] 6.19 MU2、独立 reviewer；只读审查固定候选的新增交互，修复后独立复核留证
- [x] 6.20 MU2、main；核对 Coverage Index 中归属本单元的 Scenario 的实现与候选检查/review 可追溯，确认 ACP 原文三要素与 `check:drift` 证据可追溯
- [x] 6.21 MU2、merger；以 `agentic-premerge` 块运行 `workflow check --stage premerge`，PASS 后持久化 receipt 并登记 `## Premerge History`，复核基线后直接本地合入并记实际提交
- [x] 6.22 MU2、检查执行者；核对一致性并完成 [PV1] 与 [PV2] 主分支回归，证据 `reports/PV1-main-mu2.log`、`reports/PV2-main-mu2.log`
- [x] 6.23 MU2、独立 reviewer；审查合并新增差异，无差异时记依据与原 review ID
- [x] 6.24 MU3a、merger；候选构建前机械核实目标仓库与主分支当前提交并留证
- [x] 6.25 MU3a、合入负责人；构造 MU3a 候选（WP5a），固定基线与候选版本
- [x] 6.26 MU3a、检查执行者；按 [PV1] 与 [PV3] 完成候选 Project Verify，证据 `reports/PV1.log`、`reports/PV3.log`
- [x] 6.27 MU3a、独立 reviewer；只读审查固定候选，修复后独立复核留证
- [x] 6.28 MU3a、main；核对 Coverage Index 中归属本单元的 Scenario 的闭环
- [x] 6.29 MU3a、merger；以 `agentic-premerge` 块运行 `workflow check --stage premerge`，PASS 后持久化 receipt 并登记 `## Premerge History`，复核基线后直接本地合入并记实际提交
- [x] 6.30 MU3a、检查执行者；核对一致性并完成 [PV1] 与 [PV3] 主分支回归，证据 `reports/PV1-main-mu3a.log`、`reports/PV3-main-mu3a.log`
- [x] 6.31 MU3a、独立 reviewer；审查合并新增差异，无差异时记依据与原 review ID
- [x] 6.32 MU3b、merger；候选构建前机械核实目标仓库与主分支当前提交并留证
- [x] 6.33 MU3b、合入负责人；构造 MU3b 候选（WP5b），固定基线与候选版本
- [x] 6.34 MU3b、检查执行者；按 [PV1] 与 [PV3] 完成候选 Project Verify，证据 `reports/PV1.log`、`reports/PV3.log`
- [x] 6.35 MU3b、独立 reviewer；只读审查固定候选，重点核对密钥不可导出与 IndexedDB 持久化；修复后独立复核留证
- [x] 6.36 MU3b、main；核对 Coverage Index 中归属本单元的 Scenario 的闭环
- [x] 6.37 MU3b、merger；`workflow check --stage premerge`、receipt、合入与主分支回归
- [x] 6.38 MU3b、检查执行者；核对一致性并完成主分支回归，证据 `reports/PV1-main-mu3b.log`、`reports/PV3-main-mu3b.log`
- [x] 6.39 MU3b、独立 reviewer；审查合并新增差异，无差异时记依据与原 review ID
- [x] 6.40 MU3c、merger；候选构建前机械核实目标仓库与主分支当前提交并留证
- [x] 6.41 MU3c、合入负责人；构造 MU3c 候选（WP6），固定基线与候选版本
- [x] 6.42 MU3c、检查执行者；按 [PV1] 与 [PV3] 完成候选 Project Verify，证据 `reports/PV1.log`、`reports/PV3.log`
- [x] 6.43 MU3c、独立 reviewer；只读审查固定候选，修复后独立复核留证
- [x] 6.44 MU3c、main；核对 Coverage Index 中归属本单元的 Scenario 的闭环
- [x] 6.45 MU3c、merger；`workflow check --stage premerge`、receipt、合入与主分支回归
- [x] 6.46 MU3c、检查执行者；核对一致性并完成主分支回归，证据 `reports/PV1-main-mu3c.log`、`reports/PV3-main-mu3c.log`
- [x] 6.47 MU3c、独立 reviewer；审查合并新增差异，无差异时记依据与原 review ID
- [x] 6.48 MU3d、merger；候选构建前机械核实目标仓库与主分支当前提交并留证
- [x] 6.49 MU3d、合入负责人；构造 MU3d 候选（WP7），固定基线与候选版本
- [x] 6.50 MU3d、检查执行者；按 [PV1] 与 [PV3] 完成候选 Project Verify，证据 `reports/PV1.log`、`reports/PV3.log`
- [x] 6.51 MU3d、独立 reviewer；只读审查固定候选，修复后独立复核留证
- [x] 6.52 MU3d、main；核对 Coverage Index 中归属本单元的 Scenario 的闭环
- [x] 6.53 MU3d、merger；`workflow check --stage premerge`、receipt、合入与主分支回归
- [x] 6.54 MU3d、检查执行者；核对一致性并完成主分支回归，证据 `reports/PV1-main-mu3d.log`、`reports/PV3-main-mu3d.log`
- [x] 6.55 MU3d、独立 reviewer；审查合并新增差异，无差异时记依据与原 review ID
- [x] 6.56 MU3e、merger；候选构建前机械核实目标仓库与主分支当前提交并留证
- [x] 6.57 MU3e、合入负责人；从 MU3d 的合入提交构造 MU3e 候选（TP3 + TP4），固定基线与候选版本
- [x] 6.58 MU3e、检查执行者；按 [PV1] 与 [PV3] 完成候选 Project Verify，证据 `reports/PV1.log`、`reports/PV3.log`
- [x] 6.59 MU3e、独立 reviewer；只读审查固定候选的用例与断言，修复后独立复核留证
- [x] 6.60 MU3e、main；核对 Coverage Index 中归属本单元的 Scenario 的闭环，确认跨单元贡献与前端消费可追溯
- [x] 6.61 MU3e、merger；`workflow check --stage premerge`、receipt、合入与主分支回归
- [x] 6.62 MU3e、检查执行者；核对一致性并完成主分支回归，证据 `reports/PV1-main-mu3e.log`、`reports/PV3-main-mu3e.log`
- [x] 6.63 MU3e、独立 reviewer；审查合并新增差异，无差异时记依据与原 review ID

## 7. Independent Validation

- [x] 7.1 [validation] validator；依赖 6.58（MU3e 的候选 Project Verify，即前端最后一个交付单元 MU3e 的检查；拆分前写 6.24 指向单一 MU3，§6 重编号后为 6.58）。按 plan 的 `## Independent Validation` 表验证最终主分支固定版本，重点核对路径越界判定是否覆盖规范化前缀边界、会话明细分页是否覆盖不重不漏、imported 资源是否有落盘断言、设备身份固定向量对拍是否为真实字节比对；交付覆盖充分性、假设判定、隔离方式与 `reports/IV1.log`

## 8. Final E2E

- [x] 8.1 全变更、main；依赖主分支检查。核对 Main E2E 为 `not-applicable` 的判据仍然成立（`server::sync` 与 `/ui` 仍未落地、无浏览器 E2E 驱动入口），并核对 4.1–4.3 三条替代检查 AC1/AC2/AC3 均已 PASS 且证据可读；不成立则 BLOCKED 并重开受影响任务
- [x] 8.2 全变更、main；依赖 8.1。汇总替代验证覆盖：逐条确认 AC1 覆盖派生事件在真实 Daemon 上的可观察性、AC2 覆盖合同门禁与存储契约、AC3 覆盖前端协议与 Rust 的同源一致性，确认资源已清理，全部通过才完成
- [x] 8.3 [e2e-owned] 全变更、扩展；依赖 8.2。运行 `openspec-agentic e2e check --change sync-scope-and-pwa-client`，仅 PASS 自动勾选；此行只检查门禁，不执行测试或汇总

## 9. Final Verification

- [x] 9.1 [final-verification] 用 agentic-verify（亦为 `/opsx:verify` 入口）核对意图、需求/设计/计划/任务、全变更 Coverage Index（跨单元闭环与最终运行证据）、最终主分支证据及 User Deliverables 标准；记录当前 agentic-assessment，`workflow check --stage final` 全通过才完成，逐项交接产物与实际状态