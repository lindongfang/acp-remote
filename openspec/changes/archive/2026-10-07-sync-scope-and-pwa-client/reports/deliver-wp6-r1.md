# WP6 交付报告：同步客户端与连接/命令状态机

- 交付单元：MU3c（`plan.md` 的 WP6 行）
- worktree：`D:\Project\acp-remote\.worktrees\wp6`，分支 `feat/wp6`
- 基线：`263d3ba8b48ae5dae8ab87b16bc4c0ea86f62314`（MU3b 已合入的 main）
- 交付提交：`2302c4155faa54ecbfb1666013d78a5270bb61a1`
- 日期：2026-10-05
- PV3 日志：`openspec/changes/sync-scope-and-pwa-client/reports/PV3-wp6-r1.log`

---

## 1. 范围与逐条需求对照

### 1.1 `src/sync-client/`（交付范围 1–6、10、11）

| # | 交付范围条目 | 落点 | 关键不变量如何被强制 |
| --- | --- | --- | --- |
| 1 | 逐连接签名握手 | `connection.ts`（`SyncConnection`） | `clientNonce` 由 `RandomPort.bytes(32)` 在 `#onOpen` 内**每次连接重新生成**，类不持有跨连接凭据；`hostProof` 必须经 `HostIdentityPort.verifyHostChallenge` 校验，失败即 `#block("host_identity_changed")` 并**不发** `auth.client_proof` |
| 2 | `subscribe` + ACK + 增量重放 | `connection.ts`（`#onAuthenticated` / `#sendAck` / `#onCaughtUp`） | 订阅游标取客户端最后确认值（首次 `null`）；ACK 记在事件进入账本之后发出且**只能前进**，回退值被忽略并记诊断；`caught_up` 之前发 `replaying` 而非 `online` |
| 3 | 按 `eventId` 去重 | `dedupe.ts`（`EventLedger`） | 去重键是 `body.eventId`；`messageId` 与 `globalSequence` **都不参与**去重判定，只用后者做连续性检查；账本由调用方跨连接持有（`ledgerFor`），§8.5 切换窗口的重投仍被识别 |
| 4 | 快照暂存与原子替换 | `snapshot.ts`（`SnapshotStaging`）+ `client.ts`（`SnapshotRepository`） | chunk 连续、数量、`begin`/`end` cursor 一致、digest 两级哈希全通过才返回 `VerifiedSnapshot`；`reset_required` 与新 `snapshot_begin` 丢弃暂存区；仓库只接受已验证快照，因此「验证失败保留旧状态」是结构性的 |
| 5 | `session.read` 分页 | `paging.ts` | 游标是 `(createdAt, messageId)` 复合键，`cursorOf` 任一分量缺失即抛 `IncompleteReadCursorError`（不补全）；`earliestCursor` 只按 `createdAt` 取最小，**不**用 `messageId` 排序；`toLoadedPage` 在 `hasEarlier` 缺失或与空页矛盾时抛错 |
| 6 | 命令派发 | `client.ts`（`SyncClient.dispatch` / `buildStatusQuery`） | 同一 `requestId` 重派只增 `dispatchCount`，ID 不变；`command.status` 查询用新 ID 作自身标识、`targetRequestId` 指回原 ID |
| 10 | Agent 连接覆盖层（R19） | `agent-overlay.ts` | 只接受 `agent.connected`/`agent.disconnected`；其余事件（含 `session.updated`/`turn.*`）返回 `null` 且不改任何状态；覆盖层缺席时 `buildAgentCatalogView` 给出 `unknown`、不移除条目 |
| 11 | imported 落盘分流（R20） | `imported-content.ts` | `origin.kind === "remote"` 时**一切**类别（含 `summary`）都判 `memory_only`，无例外分支；`mayMarkInputAsSent` 要求在线且来源为本地 |

### 1.2 `src/state/`（交付范围 7–9）

| # | 交付范围条目 | 落点 | 关键不变量如何被强制 |
| --- | --- | --- | --- |
| 7 | 连接互斥状态机 | `connection-machine.ts` | 状态是单值 `ConnectionStateName`（12 态），机器状态只有 `{state, blocking, caughtUp}` 三个键，无可同时成立的布尔组合；迁移来源写进 `ALLOWED_SOURCES` 表，表外组合抛 `IllegalConnectionTransition`；四个阻断原因显式映射到四个阻断态并携带 `reason` + `nextStep`；`shouldAutoReconnect` 在阻断态为假；`blocking_cleared` 只回 `disconnected`，不直接回在线 |
| 8 | 命令状态机 | `command-machine.ts` | `uncertain` 只能由 `applyCommandResult`/`applyStatusRecord` 的服务端结论进入（`serverConfirmedUncertain` 为唯一凭据）；`markConnectionLost` **不接受**任何「因此不确定」的参数，它就是恒等函数；`mayDisplayAsAccepted` 要求在线且 `acceptedAt` 非空；终态不被迟到的结果改写 |
| 9 | 离线输入不得标记为已发送 | `connection-machine.ts#canMarkInputAsSent` + `imported-content.ts#mayMarkInputAsSent` | 前者只在 `online` 为真；后者额外要求来源为本地（imported 即使在线也不标记，因 Owner 可能不可达） |

---

## 2. 写入范围自查

`git diff --name-only 263d3ba..HEAD` 全部落在 `clients/app/src/sync-client/` 与 `clients/app/src/state/`：

```
clients/app/src/state/command-machine.test.ts
clients/app/src/state/command-machine.ts
clients/app/src/state/connection-machine.test.ts
clients/app/src/state/connection-machine.ts
clients/app/src/state/index.ts
clients/app/src/sync-client/agent-overlay.test.ts
clients/app/src/sync-client/agent-overlay.ts
clients/app/src/sync-client/base64url.ts
clients/app/src/sync-client/client.test.ts
clients/app/src/sync-client/client.ts
clients/app/src/sync-client/connection.test.ts
clients/app/src/sync-client/connection.ts
clients/app/src/sync-client/dedupe.test.ts
clients/app/src/sync-client/dedupe.ts
clients/app/src/sync-client/imported-content.test.ts
clients/app/src/sync-client/imported-content.ts
clients/app/src/sync-client/index.ts
clients/app/src/sync-client/paging.test.ts
clients/app/src/sync-client/paging.ts
clients/app/src/sync-client/ports.ts
clients/app/src/sync-client/snapshot.test.ts
clients/app/src/sync-client/snapshot.ts
clients/app/src/sync-client/socket.web.ts
clients/app/src/sync-client/testing/harness.ts
clients/app/src/sync-client/wire.test.ts
clients/app/src/sync-client/wire.ts
```

**越界自查**：`git status --porcelain | awk '{print $2}' | grep -v -E '^clients/app/src/(sync-client|state)/'` 输出为空。未新增 npm 依赖（`package.json` 未改动），未写 `.native.ts`，未改 `plan.md`。

**分层禁令**：`src/layers.test.ts`（仓库既有，9 个用例）全绿，确认无任何层 import `src/platform/`，且依赖方向单向。`src/sync-client/ports.ts` 自声明结构化端口，`DeviceIdentityLike` 的形状与 `PlatformPorts.deviceIdentity` 一致（`Promise<DeviceIdentity | null>` 协变赋给 `Promise<DeviceIdentityMaterial | null>`），组合根可直接传入无需包装。

**两个必须向 WP7 说明的接缝**（不是缺失，是设计上的注入点）：`TranscriptCodec`、`HostIdentityPort`、`DigestPort`、`RandomPort`、`ClockPort` 在 WP5b 的 `PlatformPorts` 中没有对应成员，需由组合根装配。其中 `HostIdentityPort` 是 §8.2 强制要求的验签能力，缺失时 `SyncConnection` 直接失败而非跳过校验。

---

## 3. PV3 与根门禁的实际输出摘要

全部命令的完整输出见 `reports/PV3-wp6-r1.log`。退出码：

| 门禁 | 命令 | 退出码 | 结果 |
| --- | --- | --- | --- |
| 类型检查 | `npx tsc --noEmit` | **0** | 无诊断 |
| 单元/契约测试 | `npx vitest run` | **0** | 17 个文件、**234 个用例全绿**（WP6 新增 8 个文件、136 个用例） |
| 浏览器用例 | `node scripts/run-browser-check.mjs` | **0** | **16/16 passed**（脚本未改动） |
| Web 导出 | `npx expo export --platform web` | **0** | 产物 `_expo/static/js/web/entry-*.js` 1.03 MB + `index.html` + `metadata.json` |
| 仓库根十道门禁 | `npm run check` | **0** | schemas / commands / errors / features / assets / acp / docs（415 链接 + 9058 节引用）/ boundaries（12 crate）/ drift（36 DDL + 96 方法签名）/ agentic（21 items, 0 failed）全绿 |
| Rust 格式 | `cargo fmt --all -- --check` | **0** | 无差异 |
| 新增模块可 web 打包 | `npx esbuild src/sync-client/index.ts --bundle --platform=browser` | **0** | 61.0 kb 产物，`node:` 引用数 **0** |

**关于「产物里没有 node 内建」的核对口径**：`expo export` 的产物中确实出现一处 `node:crypto`，来自 `node_modules` 内 expo 自带的 `uuid` 库（`uuid.v4` 的浏览器回退分支），**与本包代码无关**。因此另跑了一次 esbuild 直接把 `src/sync-client/index.ts` 打成浏览器 bundle，`node:` 引用数为 0——这才是「新增模块能被 web 打包」的证据。

---

## 4. 判别力证据（变异测试）

每条关键不变量都先写用例，再**改坏实现**确认用例变红，最后还原。下表的「变异」列是实际执行过的改动：

| 变异 | 预期红点 | 实际结果 |
| --- | --- | --- |
| A：去重键换成 `messageId` | 已见过 `eventId` 但换了序号的重投被当新事件 | ✅ 1 failed |
| B：去重键换成 `globalSequence` | 同上 | ✅ 1 failed |
| C：跳过 digest 校验 | digest 被篡改时仍完成替换 | ✅ 2 failed |
| D：digest 按重新序列化的 JSON 计算 | 带缩进的原始文本不再匹配 | ✅ 1 failed |
| F：`markConnectionLost` 断线即置 `uncertain` | 「结果不确定不被自行断定」 | ✅ 2 failed |
| G：`retrySameRequest` 生成新 `requestId` | 「重试复用同一标识」 | ✅ 5 failed |
| H：`subscribed` 直接置 `online` | 「追平期不谎称已在线」 | ✅ 10 failed |
| I：`canMarkInputAsSent` 放宽到非 `unpaired` | 「离线输入不被标记为已发送」 | ✅ 2 failed |
| J：给 imported 的 `summary` 开落盘例外 | R20 全类别只留内存 | ✅ 2 failed |
| K：游标补全缺失的 `createdAt` 分量 | 「不猜测或补全缺失分量」 | ✅ 2 failed |
| L：从 `session.agent` 推断 Agent 已连接 | 「不得以会话活跃度推断」 | ✅ 2 failed |
| M：`reset_required` 不丢弃暂存快照 | 「未完成内容不得覆盖已完成状态」 | ✅ 1 failed |
| N：跨连接复用 `clientNonce` | 「每次连接重新握手」 | ✅ 13 failed |

变异 A/B/L/M 在首轮**未变红**，暴露了原用例的判别力不足，据此补写了三个用例才达到上表状态：

- `去重键是 eventId 而非 messageId/序号：已见过的 ID 即使换了序号也不得重新呈现`（A、B 因此变红）
- `会话处于 running 也不得推断 Agent 已连接`（L 因此变红）
- `reset 之后到达的 snapshot_end 不得完成替换`（构造 digest 与 chunkCount **完全吻合**的迟到 end，唯一能挡住它的就是「reset 丢弃了暂存区」这条不变量；M 因此变红）

变异 E（注入一个无用字段）作为对照组全绿，确认失败不是由「改了文件就会红」造成的假阳性。

无 `#[ignore]` 式跳过、无恒真断言、无只断言「非空」的关键不变量用例。

---

## 5. 未验证项与如实声明

1. **没有真实 Daemon 可连。** 握手、`subscribe`、重放、ACK、快照、命令派发全部只对**注入的 fake socket 与 fake 端口**验证。`auth.server_challenge` / `auth.authenticated` 的真实字节形状、以及 `sync.*` 在服务端实际投递下的时序，都未与 `server::sync`（尚未落地）对拍。
2. **固定向量字节对拍未做。** `fixtures/sync/v1/transcripts/device-proof.json` 的跨语言对拍按 `tasks.md` 2.6 归 TP3。本包的 transcript 端口用假 codec 验证的是**字段选择与顺序**（tag 集合 `[1,2,3,6,9,10,11,12]`、domain、两个 nonce），字节级正确性依赖 WP5b 的 `src/platform/transcript.ts` 与 TP3。`base64url.ts` 是本层自带的纯字母表变换（解码方向平台层没有），其 32 字节长度约束与往返一致性有单测覆盖，但未与 Rust 侧对拍。
3. **`HostIdentityPort` 无真实实现可测。** WP5b 的 `PlatformPorts` 未提供已配对主机记录与验签能力，因此 `identity_changed` 的判定路径只用假端口验证了「校验失败即阻断、不发 `client_proof`、不发 `loadPairedHost` 后的自动接受」。真实 Host key 存储与验签由组合根（WP7）或后续变更提供。
4. **`DigestPort` 用确定性替身而非 SHA-256。** 快照 digest 的不变量是「两级哈希、按 index 连接、覆盖**已到达** chunk 的原始字节」，测试通过**独立复算**（`expectedSnapshotDigest`）来判定，因此用 FNV 派生的确定性替身不影响判别力；但与 Rust 侧 `sha256` 的一致性未验证。
5. **`deps` / `advisories` / `secrets` 三个 CI-only job 本地无等价物，未执行、未声称通过。**
6. **提交钩子未在本次提交中观察到执行**（worktree 的 `.husky/_` 为空，`core.hooksPath` 指向它但目录不存在）。为补齐，另行单独运行了 `cargo fmt --all -- --check`（退出码 0）与 `npm run check`（退出码 0）作为等价证据；`cargo clippy` 未运行（本包无 Rust 改动，`crates/**` 未被触碰）。
7. **重连退避定时未在本包实现。** `plan.md` 把退避策略与阻断判定放在状态层；本包的 `SyncClient.connect()` 在阻断态直接抛错，由调用方决定何时重试。`ClockPort` 已声明但当前无消费方——WP7 接线时若要做退避定时可用，或按实际需要再收窄该端口。
8. **`retrying`/`disconnect_requested` 等迁移的组合覆盖是枚举式的**：迁移表已用 `allowedSourceStates()` 导出并被测试遍历断言「每个事件都有非空来源集合」，但未对「全部 12×10 组合」逐一断言合法/非法。非法迁移的抛错行为已用具体用例覆盖。

---

## 6. 交付物

- 提交：`2302c4155faa54ecbfb1666013d78a5270bb61a1`，主题 `feat(frontend): WP6 同步客户端与连接/命令状态机`
- 工作区：`git status --porcelain` 为空
- 本报告：`openspec/changes/sync-scope-and-pwa-client/reports/deliver-wp6-r1.md`
- PV3 日志：`openspec/changes/sync-scope-and-pwa-client/reports/PV3-wp6-r1.log`