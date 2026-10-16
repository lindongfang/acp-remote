<!-- 执行安排：把 specs 的行为要求与 design 的技术方案转成可派发的并行工作包。
     plan 只写“谁在哪个隔离工作区做什么、按什么顺序合入、怎么验”，不重复 specs/design 的内容。 -->

## Execution Model

- 派发：主 Agent 用 `task` 工具的 batch 形状一次性扇出多个工作包；每个工作包是**一次 spawn**。
- 隔离：`task.isolation`（`patch` 模式、`apply: false`）。工作包之间不共享目录。
- 交付：`<id>.patch` + `agent://<id>` + `history://<id>`；工作包不产生 commit、不产生分支。
- 合入：主 Agent 按 `## Merge Order` 串行 `git apply`；每次合入后按该行的「Re-verify」跑主检出检查。
- 验证：隔离工作区内**没有** `target/`、`node_modules/` 与 `clients/app/node_modules/`，因此仓库级与前端级检查只在主检出执行。
- 检视：每个工作包由一个**独立** spawn（不与作者同实例）读 patch 与 `agent://<id>`、`history://<id>` 后给出结构化结论。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 解码边界受理面修正：`pwa-web-client` 的「事件来源类型的五值词表与 node 配对规则」「错误帧的两种信封」（design D1/D2/D3） | none | wp-coder | `clients/app/src/sync-client/wire.ts` | 运行期词表补第五值并绑到 `EventOriginKind`；`kind = "node"` 的配对规则生效且只对 node 生效；`error` 改成「连接标识与连接序号同时出现或同时缺席」（`control.ping`/`control.pong` 不变） | 主检出：`cd clients/app && npm run typecheck` + `npm run test`；判定证据要看既有 `wire.test.ts` 全绿、且两个 node 负例的理由与既有的「词表外取值被拒」负例都仍然可区分 |
| WP2 | 固定样例门禁与连接级回归（specs 的「解码边界受理每一条协议合法帧」四条场景；design D4/D5） | code:WP1 | wp-tester | `clients/app/src/sync-client/wire-fixtures.test.ts`, `clients/app/src/sync-client/frame-acceptance.test.ts` | 新增：manifest 里 `valid` 且 schema 为 `message.schema.json` 的每条样例必须被受理（含 node 来源与认证前错误两条定点样例的理由断言）；连接级用例断言 node 来源事件被提交进账本并触发 ACK | 主检出：`cd clients/app && npm run check`（vitest 全绿；报告须给出新用例的实测条数与「把词表改回四值即变红」的判别力核对） |
| WP3 | 浏览器级用例：node 来源事件在真实页面里呈现且游标前进（plan 的 `## Main E2E`） | code:WP1 | wp-tester | `clients/app/e2e/page/checks/frame-acceptance.ts`, `clients/app/e2e/page/fixtures.tsx`, `clients/app/e2e/page/index.tsx` | 新增一个检查集：假主机在真实连接上投递 `origin.kind = "node"` 的事件，页面呈现它、且假主机收到覆盖该序号的 `sync.ack`；`wireEvent` 增加可选的 `origin` 入参（缺省保持今天的取值，既有检查集不受影响）；检查集在 `ALL_CHECKS` 中注册 | 主检出：`cd clients/app && npm run check`（含 `typecheck:e2e`）+ `node e2e/run-e2e.mjs`（全部检查通过，输出须含新检查集与本轮条数） |
| WP4 | 过期文档回正（proposal 六；design D6/D7） | none | wp-coder | `crates/server/src/sync/mod.rs`, `docs/FRONTEND_DESIGN.md` | `mod.rs` 模块文档与 `crates/server/src/sync/` 实际落地一致（补 `event`、删「尚未落地」段、过渡形态段对齐 `subscribe/mod.rs` 今天的自述）；`FRONTEND_DESIGN.md` 的 §4.1①、§9 `[实现现状]`、§9 上方修订记录行统一到归档真机验收记录；`README.md` 不改 | 主检出：`npm run check`（`check:docs`）+ `cargo fmt --all -- --check`；报告须逐句给出「改前表述 → 证据来源 → 改后表述」三列 |

写范围互不重叠：WP1 只碰 `wire.ts`；WP2 只新建两个用例文件；WP3 只碰 `clients/app/e2e/page/` 下三个文件；WP4 只碰两个文档文件。WP2 与 WP3 的用例在 WP1 合入前必然红（这正是它们要断言的缺陷），因此两者排在 WP1 之后合入。

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 与其它工作包无文件重叠；它是 WP2/WP3 断言的先决条件 | `cd clients/app && npm run check` |
| 2 | WP2 | 与 WP1 同目录但不同文件；其断言依赖 WP1 的修正才会变绿 | `cd clients/app && npm run check` |
| 3 | WP3 | 与 WP1/WP2 不同目录；其浏览器用例依赖 WP1 的修正 | `cd clients/app && npm run check` + `cd clients/app && node e2e/run-e2e.mjs` |
| 4 | WP4 | 与前三者无文件重叠；合入后仓库级门禁才覆盖全部改动 | `npm run check` + `cargo fmt --all -- --check` |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | `cd clients/app && npm run check` | typecheck + `typecheck:e2e` + vitest（含新门禁与连接级回归）+ `expo export` 全绿；新门禁的样例条数与覆盖的消息类型数写进验收记录 |
| PV2 | `cd clients/app && node e2e/run-e2e.mjs` | 全部浏览器检查通过，含新检查集；断言的是可观察行为（页面呈现 + 收到覆盖该序号的 ACK），不是「启动成功」 |
| PV3 | `npm run check`（仓库根） | 十项合同门禁全绿：证明 schema/fixture/feature/命令/ACP 资产与 `clients/app` 未被波及，且变更资产本身结构合法（含 `check:openspec`） |
| PV4 | `cargo test --locked --workspace --all-features`（仓库根） | 全绿：`crates/server/src/sync/mod.rs` 只改注释，不得影响任何行为 |
| PV5 | `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings && cargo fmt --all -- --check`（仓库根） | 无告警、格式合规 |
| PV6 | `npm run verify`（仓库根） | 以上两类的合并入口全绿 |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer | 是否**只**放宽、未收紧：node 配对规则是否只对 node 生效、`kind: "device"` 且身份字段为空是否仍被受理；`error` 的信封判定是否与 schema 的 `oneOf` 等价（含「只出现一半必须被拒」）；`control.ping`/`control.pong` 是否未被顺手放宽；词表类型绑定是否真的构成编译期关系 | 无 CRITICAL/MAJOR 未闭环项 |
| WP2 | reviewer | 门禁是否由固定样例驱动（不是手写样例）、过滤条件是否只含 WSS 消息样例、是否**没有**写成「所有 invalid 都必须被拒」；负例断言的理由是否与解码器的实际分类一致；连接级用例是否断言了 ACK/游标前进而不是只断言解码 ok；是否复用既有 manifest 解析而不是新建第二份 | 无 CRITICAL/MAJOR 未闭环项 |
| WP3 | reviewer | 用例是否走真实解码与真实账本（不是直接构造模型）、断言是否可证伪（把修正回退后必须变红）、`wireEvent` 的新入参是否**加性**（既有检查集的取值不变）、检查集注册是否落在既有顺序约定内 | 无 CRITICAL/MAJOR 未闭环项 |
| WP4 | reviewer | 每条改动是否都能在代码或归档记录里找到对应证据；`mod.rs` 是否还有与实现相反的句子；`FRONTEND_DESIGN.md` 三处是否口径一致且没有把「未在 CI 覆盖」说成「已覆盖」；是否**没有**顺带改动 §9 第 11 条（proposal 已把它列为非目标） | 无 CRITICAL/MAJOR 未闭环项 |

## Main E2E

```yaml
mode: required
reason: 本变更修的是客户端可观察行为——`origin.kind = "node"` 的协议合法事件在浏览器里被拒收，且因此永不被 ACK、游标停在该事件之前（后续事件都进不来）。这条后果只能由真实页面在真实连接上断言；只启动成功、退出码为零、截图都不算通过。
basis: docs/WORKFLOW.md §7 要求按被测面选 prelude；本变更的被测面是 Web/PWA（真实浏览器、DOM），因此用 browser prelude（默认开），入口是既有的 `clients/app/e2e/run-e2e.mjs`（真实 Chromium 内核 + CDP，挂真实组合根与真实路由组件，事件经真实 `SyncConnection` 解码与真实账本归并）。该入口不在 CI 中运行（CI 无 Chromium），因此由主 Agent 在合入后本地执行并留存实测输出。
alternative_checks:
  - `clients/app` 的 vitest 层：固定样例门禁（每条 `valid` 的 WSS 消息样例必须被受理）+ 连接级用例（node 来源事件被提交并触发 ACK）——它随 CI 的必需检查 `前端检查（clients/app）` 执行，是长期回归网
  - 仓库根 `npm run check` 与 `cargo test --locked --workspace`：证明未波及机器合同与服务端行为
```

本次必须断言的可观察行为（WP3 给出实测输出）：

1. 假主机在真实连接上投递一条 `origin.kind = "node"`（含 `nodeId`）的合法事件后，页面**呈现**它（而不是它消失或页面进入阻断态）。
2. 假主机**收到**覆盖该事件 `globalSequence` 的 `sync.ack`——即游标推进到该事件之后；这是「拒收 ⇒ 永不被 ACK ⇒ 游标停滞」这条缺陷的直接反面。
3. 该事件之后再有事件时，后续事件同样被呈现（游标没有卡住）。
4. 既有检查集全绿，条数在验收记录里写明。

## Risks

- **WP2/WP3 在合入前必然红**：它们断言的正是未修的缺陷。缓解：把这两条排在 WP1 之后合入，并在派发时明确「隔离区跑不了 `npm run check`（没有 `node_modules`），交付是 patch + 静态核对 + 判别力说明」，真实运行由主 Agent 在主检出按 PV1/PV2 执行。
- **浏览器用例的判别力容易假绿**：页面「什么都没渲染」也能让某些选择器断言通过。缓解：WP3 的检查集必须同时断言「事件类条目存在」与「ACK 已发出」，并在报告里给出把 WP1 的修正临时回退后该检查变红的实测（回退只在隔离区或主检出临时进行，不留痕）。
- **固定样例门禁可能暴露第三处漂移**：处理规则见 design 的 Risks（按同一原则修解码边界；若问题出在协议资产自身，停止并回到变更层讨论）。
- **WP3 与 WP2 覆盖同一行为的两层**：不视为重复——WP2 进 CI 的必需检查，WP3 是浏览器级 Main E2E 但只在本地运行；两层缺一都会让「同类漂移下次无人发现」或「用户可见后果无实证」其中之一失守。
- **WP4 与门禁的耦合**：`docs/FRONTEND_DESIGN.md` 被 `check:docs` 引用检查覆盖，`crates/server/src/sync/mod.rs` 走 `cargo fmt --check`；两者都在 PV 里显式列出，避免「只改了注释没验」。
- **前端工程独立门禁**：`clients/app` 不在根 `npm run check` 链上（根命令对它一无所知）。缓解：PV1/PV2 显式带 `cd clients/app`，且 CI 的 `前端检查（clients/app）` 是必需检查。
