# WP7 交付报告 r1（MU3d）

- **分支 / 提交**：`feat/wp7` / `221e76e`（基线 `4ada2a24e23ff3f07d37eb0face70921a66ed987`，worktree 干净）
- **范围**：R15–R19 的全部页面与共享组件；并在已授权例外范围 `clients/app/src/sync-client/` 内修复 MU3c 带入的 P1-N1 / P1-N2
- **PV3 日志**：`reports/PV3-wp7-r1.log`

---

## 1. 门禁实测（退出码）

| 检查 | 命令 | 结果 |
| --- | --- | --- |
| 类型检查 | `clients/app: npx tsc --noEmit` | **exit 0**，无诊断 |
| 单元/契约测试 | `clients/app: npx vitest run` | **exit 0**，25 文件 / **338 passed（269 基线 + 69 新增）** |
| 浏览器用例入口 | `clients/app: node scripts/run-browser-check.mjs` | **exit 0**，**16/16 passed**（脚本未改动） |
| web 打包 | `clients/app: npx expo export --platform web` | **exit 0**；产物中 `node:` 内建命中数 **0**；`data-route="dirs|hosts|pair|session|dir-detail"` 均进入 bundle |
| 仓库根十道门禁 | `npm run check` | **exit 0**（schemas / commands / errors / features / assets / acp / docs / boundaries / drift / agentic 全绿，agentic `21 passed, 0 failed`） |

`cargo fmt --check` 本轮**未运行**：WP7 的改动全在 `clients/app/`，不涉及任何 Rust 文件；worktree 内该项的已知失败未复现、未触碰。

---

## 2. 改动文件清单

### 标准范围 `clients/app/app/`（6）

`_layout.tsx`（store 接线）、`index.tsx`（目录页）、`dir/[alias].tsx`（目录详情）、`session/[id].tsx`（对话页）、`pair.tsx`（配对页）、`hosts.tsx`（主机与连接页，新增）。

### 标准范围 `clients/app/src/components/`（18）

`index.ts`（barrel）、`DirectoryList`、`DirectoryDetail`、`ConversationStream`、`ContextUsageBar`、`DegradedEventCard`(既有，未改)、`CreateSessionSheet`、`DiagnosticsDrawer`、`HostConnectionPanel`、`PairingPanel`、`ConnectionBadge`、`RuntimeUnavailable`、`EmptyState`/`StatusPill`(既有，未改)，以及 5 个用例文件 `r15…r19` 与 `testing/react-dom-server.d.ts`。

### 标准范围 `clients/app/src/features/`（14）

`index.ts`、`ports.ts`、`client-store.ts`、`runtime.tsx`、`directory-model.ts`、`create-session-model.ts`、`conversation-model.ts`、`degradation-model.ts`、`host-panel-model.ts`、`pairing-model.ts`、`testing/fixtures.ts`，以及 2 个用例文件（`r15-directory-model`、`conversation-paging`）。

### 已授权例外 `clients/app/src/sync-client/`（3）

`connection.ts`（P1-N1：先判定后交付）、`client.ts`（P1-N1：门面的「只前进、同 epoch」守卫）、`snapshot-barrier.test.ts`（P1 判别力用例，新增）。**该目录内无其它改动。**

`git diff --name-only 4ada2a2..HEAD` 的 39 个路径全部落在上述三处之内（清单见 PV3 日志第 7 节）。

---

## 3. 两条 P1 的判别力证据

### P1-N1 门面采纳了被账本拒绝的快照屏障

- **根因**：`#onSnapshotEnd` 先 `emit snapshot_verified`、后 `ledger.advanceTo(barrier)`；账本拒绝（跨 epoch / 越界）时连接层不采纳，但门面已无条件执行 `#ackedCursor = event.snapshot.cursor`。
- **修复（结构层）**：判定前移到 `emit` 之前——未被账本采纳的快照**根本不交付**。
- **修复（幂等冗余）**：`SyncClient.#isAdoptableBarrier` 只接受「同 epoch 且不早于当前恢复点」的屏障，否则既不替换快照仓库也不移动恢复点（两处判定必须同结论）。
- **用例**：`snapshot-barrier.test.ts`，两类拒绝屏障（外来 epoch、越界序号）各一组。
  - 修复前 **红**：`expected true to be false`（仍在发 `snapshot_verified`）、`expected {…2} to deeply equal {…2}`（恢复点被换成外来游标）。
  - 修复后 **绿**；对照用例「合法屏障仍交付并推进游标」始终绿——排除「一律拒绝」这种平凡实现。

### P1-N2 被污染的游标使每次重连构造期抛错

- **修复前红（实测，含异常原文）**：`Error: resumeFrom 的 globalSequence 不可用：99999999999999999999999999（sequence_out_of_range）` 从 socket 的 `onMessage` 同步回调里抛出；`expected { cursor: null } to match { cursor: {…} }`（跨 epoch 情形恢复点被吞成 `null`）。
- **前提也由用例实测**（不是推断）：两条「前提实测」用例直接构造 `EventLedger` 与 `SyncClient`，证明越界 `resumeFrom` 在**构造期**抛错且异常逃出回调。
- **独立验证「污染无法再发生」**：取被污染门面的 `ackedCursor`（即组合根会持久化的值）重建一个**新**门面，走完握手，断言认证不抛错且 `sync.subscribe` 带的是上一次的有效游标 `{serverEpoch, "10"}`。修复前该断言红，修复后绿。
- 修复后：**10 passed**。

---

## 4. R15–R19 逐条覆盖对照

| 需求 | MUST / MUST NOT | 判别力用例（红→绿的反例） |
| --- | --- | --- |
| R15 | 目录项只显示展示名与别名 | `components/r15` 「带路径字段的目录项渲染后不含任何路径信息」：夹具给 `WorkspaceRef` 挂 `path`/`root`，断言 HTML 无 `D:\work`、`path=`；`features/r15` 断言模型键恰为 `["alias","counts","displayName"]` |
| R15 | 不显示/编辑/推导本机路径 | 目录详情、创建弹层、对话页的 props 里**没有**任何路径槽位；`DirectoryDetail` 断言无 `/`、`\`、`[A-Za-z]:[\\/]` |
| R15 | 同名目录不得用路径消歧 | `components/r15` 「同名目录各自带别名」：两个 `api` 各带 `work-api`/`personal-api`，且 HTML 无路径片段；`features/r15` 断言同名项数为 2（按名去重会变 1，红） |
| R15 | 三项汇总由会话摘要得出，断连仍可呈现 | `features/r15` 断连时 `counts={total:2,active:1,pending:1}`（不给出汇总会红）；`components/r15` 断连时目录行仍在并标注「内容来自上次同步」 |
| R15 | 数据来自快照，MUST NOT 依赖在线查询命令 | `features/r15` 注入记账网关：读目录页/详情/主机面板后 `dispatches` 为空；对照用例证明派发通道本身可用 |
| R15 | 两种空态可区分 | `components/r15`：两种 `data-empty-state` + 标题 + 指引互不相同，且互相不含对方的指引；`no snapshot → loading` 冒充即红 |
| R16 | 载荷只含两个引用 | `components/r16`：派发命令的 payload 键恰为 `["agentId","workspaceAlias"]`，序列化后无 `D:\`、`/home/`、`token|password|apiKey|mcp|cwd|exportId|templateParams`；6 组越界载荷（cwd/path/exportId/mcpServers/token/templateParams）全部抛错 |
| R16 | 候选只来自本机已配置的 Agent | `components/r16`：提交未登记的 Agent → `ok:false` 且 `dispatches` 为空 |
| R16 | 无权限时入口不可用且说明原因 | `components/r16`：`data-create-permission="denied"` + `disabled` + 说明含「电脑端」；对照用例证明按钮不是永远禁用；未认证的拒绝理由与「没有权限」可区分 |
| R16 | 创建中与终态分别呈现 | `components/r16`：提交中渲染 `data-create-submission="creating"` 且按钮 `disabled`（可重复提交会红）；`completed` → `created` + `sessionId`；`failed` → 码/可重试/消息分别可见 |
| R16 | 结果不确定显眼且不被自动消解 | `components/r16`：`uncertain` 渲染 `role="alert"` 区块；再来无关事件后仍在 `uncertainRequestIds`；只有 `dismissUncertain` 才移除 |
| R17 | 未上报不得以零冒充 | `components/r17`：`unreported` 的 HTML 无 `data-context-used`/`data-context-size`/`0%`/`>0<` |
| R17 | 窗口为零同样视为未上报且不算占比 | `components/r17`：`zero_window` 分支存在且无 `NaN`/`Infinity`/`0%` |
| R17 | 三种状态可区分 | `components/r17`：三种 `data-context-state` 去重后为 3；对照用例证明已知态确实出数字 |
| R17 | 未上报不得阻断其它交互 | `components/r17`：未上报/零窗口时 `canSend/canCancel/canViewHistory` 全为真；仓库层「在线且未收到用量事件」的会话 `canSend=true`；对照：imported 来源离线时 `canSend=false` |
| R18 | 未识别事件保留原文可查 | `components/r18`：未登记 `eventType` 原样显示 + 降级卡片 + `data-action="open-diagnostics"`；对照：原文可用时抽屉给出 `rawJson` |
| R18 | 三类不支持可区分 | `components/r18`：client/broker/agent 三段 HTML 去重后为 3，标签表本身去重为 3；合并成一句即红 |
| R18 | 原文未下发时不渲染占位 | `components/r18`：`rawUnavailable` 场景无 `data-raw-json`、无 `{}`/`undefined` 占位，仍给出原因与字节长度/摘要 |
| R18 | 二进制未下发的说明 | `components/r18`：`binary_not_delivered` 分支给出「结构化事件已收到」，且无 `<img` |
| R18 | 未知事件不得让整个会话渲染失败 | `components/r18`：同一次渲染里消息与降级条目同时存在 |
| R19 | 列表来自快照，断连仍可呈现 | `components/r19`：重连中 + 无覆盖层 → 2 行 Agent；断连标注「来自上次成功同步的快照」 |
| R19 | 覆盖层缺席 = 状态未知 ≠ 已断开 | `components/r19`：每行 `unknown`、文案「状态未知」、无 `>已断开<`；对照：收到 `agent.connected` 后该 Agent 才是 `connected` |
| R19 | 不得以会话活跃度推断 | `components/r19` 仓库层：两条 `running` 会话 + 零覆盖层 → 仍为 `["unknown","unknown"]` |
| R19 | 同名 Agent 以标识区分 | `components/r19`：两个 `codex` 为 2 行，两个 id 都渲染；覆盖层按 id 索引，互不覆盖 |
| D8 | 明细现取 + `(createdAt, messageId)` 复合游标翻页 | `features/conversation-paging`：首屏无 `before`；翻页游标取**最早**一条（用最新一条会红）；游标两键齐备；相邻两页重叠抛错；无连接时不派发；读取失败给结构化原因 |

---

## 5. 页面层的其它保证

- **页面不碰平台能力**：`src/layers.test.ts` 的「页面层不得直接 import 平台 API」与「没有任何跨层反向 import」两条仍绿；feature 层对同步客户端只依赖 `features/ports.ts` 的三个窄接口（快照只读 / 命令派发 / 认证结果）+ `ConnectionIdentity`。
- **协议载荷不进视图模型**：目录、会话、Agent、上下文、降级四类模型都是 `src/domain/` 类型之上的纯投影，`MessageView` 只带纯文本与 `hasStructuredContent`，结构化原文只出现在诊断抽屉里。
- **稳定选择器**：每页带 `data-route`，关键状态带 `data-content-origin` / `data-empty-state` / `data-context-state` / `data-create-permission` / `data-create-submission` / `data-degraded-event` / `data-raw-unavailable` / `data-agent-row` / `data-connection-state`，供 TP4 按选择器编写。

---

## 6. 缺口登记（需要 main 裁决，不在本包 Write Scope 内）

### 6.1 组合根没有归属：客户端运行时尚未装配

- **事实**：`src/layers.test.ts` 的「没有任何跨层反向 import」把 `PLATFORM_LAYER` 对**七层中的每一层**判为违规（含 `app/`）。因此任何落在 `app/`、`src/features/`、`src/components/` 的文件都**不能** import `src/platform/`，组合根在 WP7 的 Write Scope 内无处安放。
- **本包的处理**：新增 `features/ports.ts` 声明 feature→客户端的窄端口面；页面在 store 为 `null` 时渲染 `RuntimeUnavailable`，**明确说明尚未装配**，而不是渲染空目录/空 Agent 列表（那会被读成 R15 的「本机没有任何目录」）。所有 R15–R19 判定与交互逻辑都已在纯组件与 store 上实现并被用例覆盖，只差「谁来创建 store」。
- **仍缺**：`openPlatform()` 的调用、五个端口实现（`TranscriptCodec` 需复用 `platform/transcript.ts` 的编码器、`HostIdentityPort` 需要已配对主机记录与 P1363 验签、`DigestPort`/`RandomPort`/`ClockPort` 可由 WebCrypto/系统时钟满足）、`PairingTransport` 的 HTTPS 实现，以及 `ConnectionIdentity` 的取值来源（`SyncClient` 未暴露 `connectionId`；feature 层因此在无连接时拒绝派发命令，见 `features/conversation-paging` 的负向用例）。
- **建议**：新增一个层级中立文件（如 `clients/app/src/composition.ts`，`layerOfSourcePath` 返回 `null` 即不受方向判定），或把组合根登记为独立工作包；两者都需要扩 Write Scope，故本轮**未自行创建**。

### 6.2 测试文件归属与 plan 的区域收窄

`plan.md` Shared File Ownership 记有「`clients/app/**/*.test.ts`：WP7 只写实现文件，测试文件由 TP3 独占」。本轮按派发要求为每条 MUST/MUST NOT 写了负向用例，因此**新增的用例文件都放在 `src/features/` 与 `src/components/` 目录内**（仍在 WP7 的 Write Scope 内），未触碰 `src/protocol/`、`src/domain/`、`src/state/`、`src/platform/`、`src/sync-client/` 的既有测试。若 main 认为与该登记冲突，最小处置是把 `src/features/r15-*`、`src/features/conversation-paging.test.ts`、`src/components/r1*.test.ts` 与 `features/testing/fixtures.ts` 移交 TP3——它们不依赖任何其它测试文件。

### 6.3 渲染测试用 `react-dom/server`

`vitest.config.ts` 的 `include` 只收 `src/**/*.test.ts`（不在本包可改范围），且不允许新增依赖（`package.json` 不在 Write Scope）。因此组件渲染断言写在 `.ts` 里用 `createElement` 渲染，并在 `src/components/testing/react-dom-server.d.ts` 声明 `react-dom/server` 的实际形状（一个纯函数）。真实浏览器里的可观察行为仍需 TP4 用 `clients/app/e2e/` 覆盖。

---

## 7. 未验证 / 未声称

| 项 | 状态 |
| --- | --- |
| 浏览器级可观察行为（离线渲染、两种空态、提交载荷、无权限态、降级卡片不渲染占位、三类不支持可区分） | **缺浏览器级证据**——`scripts/run-browser-check.mjs` 与 `src/platform/testing/browser-checks.ts` 不在本包 Write Scope，本轮未改；浏览器用例归 TP4（`tasks.md` 4.13/4.14）。Node 级证据是 `renderToStaticMarkup` 的 HTML 断言（见 §4） |
| 组合根接线后的真实连接 | **未实测**：见 §6.1 |
| 配对 HTTPS 流程 | **未实测**：`PairingTransport` 尚无实现，只交付状态机与视图模型 |
| 事件流式增量（`agent.message.delta` 等）在正文里的呈现 | **未实现**：R15–R19 未要求，且 D8 规定明细一律现取；事件侧只归并摘要/用量/覆盖层/降级四类 |
| `deps` / `advisories` / `secrets` 三个 CI-only job | **未执行，不声称通过** |
| `cargo fmt --check` | **未运行**（本包零 Rust 改动） |
| worktree 内根 `node_modules/@dongfanglin` 与 `@fission-ai` 缺失 | 环境修复：从主检出复制这两个包到 worktree 的根 `node_modules`（`node_modules` 被 gitignore，不构成代码改动），之后 `npm run check` 十道门禁全绿 |

---

## 8. 刻意保留的取舍（供复核攻击）

1. **门面守卫与连接层判定重复**：两处各判一次「只前进、同 epoch」。连接层已判过一次，看似冗余；但门面是唯一写恢复点的地方，一旦任何未来路径绕过连接层判定，重复判定是唯一能挡住永久卡死的结构。
2. **未装配时不渲染空数据**：`RuntimeUnavailable` 让页面在 store 缺失时「什么都不显示」。另一种选择是渲染空目录页，但那会把工程缺口伪装成用户事实，违反 R15 第一种空态的语义。
3. **`applyConnectionEvent` 吞掉非法迁移**：关闭后迟到的 `online`/`closed` 属正常竞态，静默改状态才是错的（会让追平期被误标成在线）；抛错则会把竞态升级成崩溃。迁移表仍是唯一判定源。
4. **R19 面板里连接态与 Agent 态用不同文案**：`CONNECTION_STATE_LABELS` 有「已断开」，Agent 行的 `disconnected` 也叫「已断开」，但二者语义不同（前者是本设备到主机的连接，后者是主机上 Agent 进程）。负向用例因此用「重连中/同步中」作为连接态，避免与 Agent 行的断言混淆。
