# RV3-IMPL 独立代码检视报告（专项 delta 复核：服务端主动关闭的 close code / reason 语义 + 「重新配对收窄」路径）

```yaml
task_id: "2.3 / 2.4（专项 delta 复核；逐条见 handoff_index）"
role: reviewer
phase: review
stage: work-package
agent_context: "新建 reviewer 子 Agent（本报告作者）；不继承实现/修复/复审对话，未参与本变更任何实现、修复或裁决；只读检视（除本报告文件外未修改任何文件），按调度要求未执行 cargo/npm、未执行 E2E、未切分支、未提交"
target_revision: 043b359e60d546316013471ecb7ac31f914f7e30
scope: "git -C D:/Project/acp-remote-wt/export-ids diff 8ae3b613..043b359 的全部 7 文件（+193/−37，全在 crates/**）与 diff e4c980c..043b359（含 5d768d8/8ae3b613 引入的同一路径），以及该路径的完整调用链与 wire 表现：ConnectionCloser（trait/NoConnections/RecordingCloser/NodeLinkCloser）→ PairingSessions → LocalAdminRouter::{node_pair_confirm,node_revoke} → CommandRoute::{node_reauth,node_revoked} → ConnectionHandle::{request_close,close_request} → conn::session 的 Ending::Close/排空/close 帧；文档侧以 docs 分支 ee1ca277 的 §8.2/§12.1/§12.3/§12.6/§14.2/§15 为准逐字比对"
changes: "本次检视为只读：未修改 worktree 内代码、测试、规划、任务状态或 verification.md；仅新增本报告"
checks: "[PV1]（Rust 侧，043b359）**日志已返回但结论为 FAILED**（reports/du1-pv1-reauth.log：`-p app --test daemon_lifecycle` 11 passed / 1 failed，exit=101；失败目标不在本轮 delta 内，见 Assessment）；合同门禁 `npm run check`/[PV2]/[PV5] 待核对（PENDING，集成树）"
issues: "0 CRITICAL / 0 MAJOR；3 MINOR（2 条注释/文档引用、1 条设计规范同步）+ 4 SUGGESTION（2 条断言强度、1 条设备面登记、1 条冗余实现）"
result: PASS
evidence_paths:
  - "D:/Project/acp-remote/openspec/changes/node-trust-export-ids/reports/rv3-impl.md（本报告）"
  - "D:/Project/acp-remote/openspec/changes/node-trust-export-ids/reports/wp-fix-reauth.log（实现者 fix 轮次 3 自报日志；本轮只核对其「负向对照」是否如实记录，未复跑）"
  - "D:/Project/acp-remote/openspec/changes/node-trust-export-ids/reports/du1-pv1-reauth.log（主 Agent 在 043b359 上的 [PV1] 日志；当前为 FAILED）"
  - "D:/Project/acp-remote/openspec/changes/node-trust-export-ids/reports/rv1-impl.md、rv2-impl.md（Previous Findings：两轮 PASS 的角度）"
  - "D:/Project/acp-remote-wt/export-ids-docs/docs/NODE_LINK_PROTOCOL.md（docs 分支 ee1ca277，权威文档副本）"
resource_cleanup: "未创建任何资源（未跑 cargo/npm、未建临时目录、未起进程、未占端口）；本报告为唯一写入"
```

## 1. Review Context

- **Review ID / Type / Stage**：RV3-IMPL（本轮唯一）/ `branch`（**专项 delta 复核**，只覆盖 fix 轮次 3 的关闭语义与「重新配对收窄」路径，不是对全量 diff 的重审）/ work-package（候选准备）。
- **被检版本**：worktree `D:\Project\acp-remote-wt\export-ids`，分支 `agentic/node-trust-export-ids`，HEAD = `043b359e60d546316013471ecb7ac31f914f7e30`；`git status --porcelain` 为空。Base = `3cadb12d`；本轮重点 delta = `8ae3b613..043b359`（fix 轮次 3），并回溯核对 `e4c980c..043b359`（含 `5d768d8` 引入「确认后关连接」与 `8ae3b613` 引入 e2e 用例）。
- **文档权威版本**：docs worktree `D:\Project\acp-remote-wt\export-ids-docs`，HEAD = `ee1ca277`（`docs(node-link): 区分重新配对与撤销的关闭语义（1000 与 4410）`），工作区干净；主检出的 docs 未含该提交，因此**全部文档逐字比对以 `ee1ca277` 为准**。
- **读取的规则与需求**：`openspec/schemas/agentic/roles/reviewer.md`、`roles/handoff.md`；`AGENTS.md` §3（授权/会话不变量）、§7、§9；本变更 `design.md` D1–D10（含 D8 已知限制与 D9 勘误）、`specs/node-link-owner-server/spec.md`、`plan.md`、`tasks.md` 2.3/2.4、`verification.md` 的 Review Findings（含「主 Agent 复审-1（关闭码语义）」与 RV1/RV2-IMPL 各行）；`docs/NODE_LINK_PROTOCOL.md`（`ee1ca277`）§8.2/§12.1/§12.3/§12.6/§13/§14.2/§15。
- **实际检查范围**：自读两段 delta 的完整 diff（不依赖任何摘要）＋逐跳核对调用链（见 scope）；逐字比对 reason 字符串与文档文本（`grep -c -F`，两处各命中 1 次）；核对 §15/§14.2/§8.2 的节号引用；核对 `RecordingCloser`/`NoConnections` 是否把两类关闭**可区分记录**；核对两个用例的断言是否可能空转（含 support 客户端的帧读取实现与 `CLOSE_DRAIN_BUDGET`）；核对 `node.revoke` 既有断言是否仍成立；核对 delta 的文件边界、是否新增跳过/弱化断言/`unwrap`/detached task。
- **限制与不做的事**（必须随结论一起看）：① 本角色不执行命令，`[PV1]` 的实现者日志与主 Agent 日志只能核对不能复跑，因此**三条负向对照只能确认「是否如实记录」，不能确认「可复现」**；② 未执行 E2E；③ 未跑合同门禁；④ 本报告**不代替** Project Verify，也不代表任务可勾选或变更可归档。

## 2. 五项必查项的逐条结论

### 2.1 两条路径是否真的分流 —— ✅ 已分流，无残留交叉调用

端口与实现（trait → 组合根 → node_link）逐跳核对：

| 触发 | `ConnectionCloser` 方法 | 组合根实现（`crates/app/src/compose.rs`） | node_link 路由 | 返回 |
| --- | --- | --- | --- | --- |
| `node.revoke`（`router.rs:950`） | `close_node`（`pairing.rs:55/234`） | `NodeLinkCloser::close_node`（`compose.rs:782-790`）→ 日志事件 `daemon.revocation_connection_sweep` | `CommandRoute::node_revoked`（`command.rs:1568`）：**推** `node.trust.revoked` + `request_close(close::REVOKED=4410)` | 连接数 |
| `node.pair.confirm`（`router.rs:885`） | `close_node_after_reauth`（`pairing.rs:62/240`） | `NodeLinkCloser::close_node_after_reauth`（`compose.rs:793-802`）→ 日志事件 `daemon.reauth_connection_sweep` | `CommandRoute::node_reauth`（`command.rs:1634`）：**不推**消息 + `request_close(close::NORMAL=1000, reason)` | 连接数 |

- 反查结论：`CommandRoute::node_reauth` 的全仓库调用点只有 `compose.rs:794`（生产）与 `command/tests.rs:2255`（用例）；`node_revoked` 只有 `compose.rs:783` 与 `command/tests.rs:2228`。**两条路径没有任何交叉或残留复用**，`crates/` 内已无「confirm → close_node」或「revoke → node_reauth」的残留写法。
- 替身可区分：`RecordingCloser` 分列两表（`nodes` / `nodes_after_reauth`，`test_support.rs:1863-1900`），`closed_nodes()`（撤销）与 `closed_nodes_after_reauth()`（重新配对）各自可断言；`nodes_revoked_when_notified()` 只跟随撤销路径。`NoConnections` 补齐了新方法（`pairing.rs:78`，no-op），生产装配（`daemon.rs:1249-1262`）用的是 `NodeLinkCloser`（`NoConnections` 只出现在 `#[tokio::test]` 与该类型自身的单测装配里，已核对）。
- **两类关闭在日志与断言两层都可区分**，不存在「合成一个计数导致假绿」：本次 fix 还**加强**了断言（既断言 `closed_nodes_after_reauth() == [NODE_ID]`，又断言 `closed_nodes().is_empty()`，见 `router.rs:2697-2706`、`2920-2928`）。

### 2.2 关闭码与 reason —— ✅ 实现与文档逐字一致（reason 未在线上字节层被断言，见 F5）

- `node_reauth` 以 `close::NORMAL`（`conn/mod.rs:64` = `1000`）关闭，**不**调用 `handle.send(...)`，因此不推 `node.trust.revoked`；reason 字面量为
  `"the node trust was re-confirmed; reconnect to fetch the updated catalog"`（`command.rs:1640-1642`）。
- 文档文本（`docs/…/NODE_LINK_PROTOCOL.md` §8.2，docs 分支 `ee1ca277` 第 280 行）写作
  `形如 "the node trust was re-confirmed; reconnect to fetch the updated catalog"`。
  逐字比对：`grep -c -F` 在文档、`command.rs`、`command/tests.rs` **各命中 1 次且文本相同**（大小写、分号、`re-confirmed` 连字符一致）；实现与用例断言的是同一常量，不存在实现/用例/文档三处漂移。
- `node_revoked` 保持原语义：先 `send(MessageType::NodeTrustRevoked, …)` 再 `request_close(close::REVOKED, "the node trust was revoked")`（`command.rs:1596-1615`）；撤销的 reason 文本未在文档中规定（§12.3 只规定消息体字段），不构成不一致。
- 线上顺序可推导且已被负向对照印证：`session.rs:799-800` 在每轮循环顶部检查 `close_request()` → `break Ending::Close(code, reason)`，随后 `session.rs:934-937` **先 `self.drain(&mut receiver)`（排空已入队消息）再 `unregister` 再 `self.close(code, reason)`**（`close()` → `connection.close(code, reason)`，把 code+reason 写进 close 帧）。因此「推送先到、close 帧后到」不是假设而是实现顺序（`CLOSE_DRAIN_BUDGET = 2s`，`session.rs:99`）。
- **未覆盖**：e2e 用的 support 客户端 `close_code()` 只返回 code、**丢弃 reason**（`crates/app/tests/support/nodelink.rs:697`：`Some(Frame::Close(code, _)) => return code`），所以文档规定的说明性 reason 在端到端链路上没有断言（单测断言的是 `close_request()` 的内存对，已足以证明注册值与实现一致，且 `session.rs` 把该对的 reason 原样交给底层 close 帧）。记为 F5（SUGGESTION）。

### 2.3 协议一致性（§14.2 / §15 / §8.2，含节号引用核对） —— ⚠️ 语义自洽；发现一处**既有**错误节号引用（F2）

- **§14.2 表**（`ee1ca277` 第 893/901 行）：`1000 = 正常关闭`、`4410 = 节点已撤销` —— 与 `conn/mod.rs:64/76` 的常量及两条路径的实际用法一致。
- **§14.2 新增表注**（第 906 行）：「服务端因**授权变化**主动结束连接时用 `1000`（正常关闭）加 close reason，不用 `4410`……`4410` 只用于节点撤销（收到 `node.trust.revoked` 或本地 `node.revoke`）」。与实现一致：全仓库 `4410` 只有两处使用（`node_revoked` 的传播、握手期 `CredentialStatus::Revoked`，`session.rs:664`），都是「节点已撤销」；`1000` 只用于正常/作废类关闭。
- **§15「撤销即停」**（第 930 行）：触发条件收敛为「收到 `node.trust.revoked` 或本地 `node.revoke`」，并显式排除「服务端因其它原因主动关闭（如 `node.pair.confirm` 的 1000）」，要求对端仍按重连退避自动重连。与实现一致（`node_reauth` 既不推消息也不以 4410 关闭）。
- **§8.2**（第 280 行）与 §14.2 表注、§15 三条互不矛盾：都表达「收窄 → 落定后作废既有连接 → 以 1000 + 说明性 reason → 对端重连取新 catalog」。
- **§13 引用核对**：文档中全部 `§13` 引用（第 27、49、261、684、950 行）都指向「节点配对 HTTP」，**无一错引**；§15 内部引用为 §12.6、§8.2、§12.3、§14.1、§12.7 与 `LOCAL_ADMIN_PROTOCOL.md` §5.2，其中 **§8.2 正确**。
- **找到的错误引用**（F2）：§15 的 `收到 node.trust.revoked（§12.6）` —— `node.trust.revoked` 实际登记在 **§12.3 Catalog 消息**（第 521 行）与 §12.1 家族总览（第 481 行，归入 Catalog 族），§12.6「控制与错误消息」只有 `link.ping`/`link.pong`、`link.error`、`link.backpressure` 三行。该引用在 base `3cadb12` 就是错的，而 `ee1ca277` **恰好改动了这一行**（在其后追加限定语）却没有顺手纠正；`scripts/check-doc-links.mjs` 只校 markdown 链接、不校 `§N` 引用，因此门禁不会发现。同类既有问题：`crates/server/src/local_admin/pairing.rs:64` 的「`NODE_LINK_PROTOCOL.md` §12.6 的 `export.revoked`」也应为 §12.3（`export.revoked` 同样登记在 §12.3，第 520 行）。两者都不影响语义与实现，属非阻断。

### 2.4 用例是否真有判别力 —— ✅ e2e 具判别力且三条负向对照如实记录；⚠️ 单测第三条断言空转（F4）

- **负向对照是否如实记录**：`reports/wp-fix-reauth.log` 的「负向对照」三节都带命令、原始失败输出与退出码，逐条核对如下（**均为实现者自报，我未复跑**）：
  1. 注释掉 `node.pair.confirm` 的新调用 → `cargo test -p app --test node_link_e2e a_narrowing_repair_closes_the_live_attachment` **exit=101**，超时 10.69 s 后 panic 在用例第 838 行、消息为「重新配对确认后既有连接必须被关闭（超时 = 连接仍然活着…）」，随后 `git checkout --` 还原并复跑 3 passed。→ 对照与失败点自洽（`CLOSE_DRAIN_BUDGET=2s < 10s` 超时阈值，成立）。
  2. 把 confirm 的调用换回 `close_node`（撤销路径）→ **exit=101**，panic 在用例第 841 行、`assertion left == right failed: left: 4410 / right: 1000`。→ 正是关闭码断言的判别力证明。
  3. 临时让 `node_reauth` 在关前先推一条 `node.trust.revoked`（仍以 1000 关闭）→ **exit=101**，panic 在用例第 853 行（即「不得收到 `node.trust.revoked`」那条 `assert!`），并打印出整帧 JSON。→ **证明 `close_code()` 之前的文本帧确实进入待取队列**，即该断言不是「永远为真」的时序假设。
- **e2e 断言不空转**的理由（独立于对照 3，我按代码复述）：`close_code()` 把 close 帧之前的文本帧收进 `pending`（`nodelink.rs:690-700`），而撤销路径必然先 `send` 后 `request_close`、会话又先排空再发 close 帧（§2.2），因此若 confirm 误走撤销路径，那条消息**只能**出现在 `pending` 里被本断言捕获；对照 2 已另证 4410 会打在 code 断言上。
- **单测 `node_reauth_closes_with_1000_and_no_revocation_notification`**：前两条断言（`drain()` 里无 `node.trust.revoked`、`close_request() == Some((1000, 逐字 reason))`）具判别力（`drain()` 抓的是该 handle 的出站队列，既有撤销用例用同一机制正向断言「恰有 1 条」）。**第三条 `route.pending_of(&fixture.handle).is_empty()` 在本用例里恒真**：该用例从未提交任何命令（`states` 里本就没有该连接的条目），而 `forget_connection`（`command.rs:1528`）只清 `states`、**不**注销注册表 —— 无论 `forget_connection` 有无被调用，断言都成立。这是**既有**写法（撤销用例 `tests.rs:2239` 同款），本 fix 未使其变差；但作为「作废后观察表随连接消失」的证据它是无效的（对照 `tests.rs:2074/2094` 的「先断言 1 条、再断言 0 条」写法才是可失败的）。见 F4。
- **单测没有负向对照**：对照 1/2/3 都只跑 `-p app --test node_link_e2e`；对照 3 的临时补丁虽然同样会让单测红，但日志里没有该单测的失败记录。属登记项（不单独列 finding）。
- **未做的一条**：e2e 只断言「旧连接被关 + 新连接 catalog 为空」，**没有**断言「新连接在收窄后的清单下还能正常 attach 仍可见的 Export」。空清单场景下这不可能（可见集为空），而「非空但更窄」的清单路径由单元/适配器用例覆盖（`catalog.rs`/`resource/tests.rs`）；作为组合断言可更完整，但不构成缺口（决定不列 finding）。

### 2.5 回归与边界 —— ✅ 未发现回归；设备面缺口与本次语义不冲突（F6）

- **`device.pair.confirm` 未加 closer**：不冲突。① 本切片没有设备连接（`server::sync` 未落地），`NodeLinkCloser::close_device` 与 `NoConnections` 都是显式记录型 no-op（`compose.rs:771-780`）；② 设备侧从来就没有「授权变化 → 关连接」的实现，本次改的只是节点面，未新增不一致；③ 但**同类缺口确实存在**（`device.pair.confirm` 改 scopes 后不作废任何连接，且 trait 也无 `close_device_after_reauth`），将来 `server::sync` 落地时它就会复现 RV2-IMPL-F1 那一类问题。建议登记为后续切片要求，**现在不要**加空实现（那会是推测性 scaffolding）。见 F6。
- **`node.revoke` 的既有断言仍成立且更强**：`router.rs:2657-2765` 的撤销用例现在断言「确认只进 `closed_nodes_after_reauth`」「撤销恰 1 次且进 `closed_nodes`」「重试不再关闭（len 仍 1）」「未知 id 不关闭」；`nodes_revoked_when_notified()` 由 `[false, true]` 收紧为 `[true]`（因为确认不再走撤销路径，表中的 `false` 本就消失，且 `is_empty`/`len==1` 的新断言把「确认不得走撤销路径」显式钉住）。e2e 的 ⑪ 仍断言撤销收到 `node.trust.revoked` + `4410`（`node_link_e2e.rs:697-706`）。
- **文件边界**：`git diff --name-only 8ae3b613..043b359` = 7 文件，全部在 `crates/**`；`-- docs schemas fixtures compatibility openspec Cargo.toml Cargo.lock` = **0 个文件**（`git diff 3cadb12..043b359` 的 28 文件同样全在 `crates/**`）⇒ 无合同资产越界、无新增依赖。
- **无跳过/弱化**：delta 未新增 `#[ignore]`/`allow(...)`；删除行只有旧注释与旧断言（都成对被新断言替换），无「只删断言不补」的净弱化。
- **无新增 `unwrap`/panic 进入正常路径**：新增生产代码（`compose.rs` 的 `close_node_after_reauth`、`command.rs` 的 `node_reauth`）只有 `u64::try_from(closed).unwrap_or(u64::MAX)`（沿用既有写法，不 panic）；`test_support.rs`/测试里的 `expect(...)` 属测试面。
- **无新增 detached task、无跨 await 持锁**：`node_reauth` 全程无 `await`，`handles_for_node` 取完即释放内部 Mutex。
- **窗口（登记项，非缺陷）**：`request_close` 是**请求**，旧连接在「会话下一次循环检查」之前（≤ 一个 `SESSION_TICK`，且排空预算 ≤2 s）仍可能收到一条已收窄 Export 的 `resource.event`——实时扇出不逐条复查清单是 RV1-IMPL-F3/RV2-IMPL-F1 已登记的既有缺口，本次关闭路径是它的**缓解**而非替代。该窗口未被用例量化（见 §4）。

## 3. Findings（本轮新发现）

| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation | Recheck |
| --- | --- | --- | --- | --- | --- | --- |
| RV3-IMPL-F1 | MINOR | `crates/server/src/local_admin/router.rs:816`（`node_pair_confirm` 的 `///` 文档首段） | 该行仍写「信任行提交成功后关闭该节点的现有活动连接（**与 `node.revoke` 同一机制**）」，而同函数体内 `router.rs:883-884`（本轮新增）明确写「**不复用**它的撤销语义」；`pairing.rs:46-49` 的 trait 文档与 `compose.rs:756-757` 也写「两条不同路径、不能互换」 | 注释自相矛盾，且矛盾点正是本次 fix 的核心（关闭码语义）；读者按首段理解为「同一条路径」就会误判 4410/1000 的可互换性 | 改为「与 `node.revoke` 同为『提交后关连接』的时机，但不复用其撤销语义（不推 `node.trust.revoked`、以 1000 关闭）」；本变更内自查同类措辞即可（代码注释由实现方改） | 新增（非阻断） |
| RV3-IMPL-F2 | MINOR | `D:\Project\acp-remote-wt\export-ids-docs\docs\NODE_LINK_PROTOCOL.md:930`（§15「撤销即停」）＋同类：`crates/server/src/local_admin/pairing.rs:64` | §15 写「收到 `node.trust.revoked`（**§12.6**）」，但该消息登记在 **§12.3**（第 521 行）与 §12.1（第 481 行，Catalog 族）；§12.6 只有 ping/pong、link.error、link.backpressure。`pairing.rs:64` 的「§12.6 的 `export.revoked`」同理应为 §12.3 | 不影响任何实现或判定（语义靠 §15/§14.2 的正文表达），但会让按 §号查证的读者找不到消息定义；`check-doc-links.mjs` 只校 markdown 链接，门禁不会捕获 | 文档收口轮把 §15 的 `（§12.6）` 改为 `（§12.3）`，并顺手改 `pairing.rs:64` 的同一处（该行在 base 即错，`ee1ca277` 改到 §15 这一行时也未纠正；本 reviewer 不改文件） | 新增（非阻断；属既有错误引用） |
| RV3-IMPL-F3 | MINOR | 变更规划面：`openspec/changes/node-trust-export-ids/specs/node-link-owner-server/spec.md`（MODIFIED Requirements 与 5 个 Scenario）＋`design.md` D8 第 4 条 | 本轮的关闭语义（**不得**推 `node.trust.revoked`、以 **1000** + 说明性 reason 关闭、对端应重连）只写进了权威文档（§8.2/§14.2 表注/§15），delta spec 里没有任何 Requirement/Scenario 承载它；`design.md` D8 仍写「两种做法都在落定后立即关闭该节点的活动连接（**与 `node.revoke` 同一把机制**）」，在 fix 轮次 3 之后已不准确 | 归档后 `openspec/specs/**` 的能力规范不会含「重新配对必须以 1000 关连接」这条义务；文档仍是权威（AGENTS.md §1/§12），故非阻断，但契约在两侧不同步 | 在 delta spec 补一条 Requirement + Scenario（如「重新配对落定后 MUST 以 1000 + 说明性 reason 作废既有连接、MUST NOT 推送 `node.trust.revoked`、对端 MUST 仍按 §15 重连」），并把 D8 措辞改为「同一**时机**、不同**语义与关闭码**」。属主 Agent 维护的规划文本 | 新增（非阻断） |
| RV3-IMPL-F4 | SUGGESTION | `crates/server/src/node_link/command/tests.rs:2268-2271` | `assert!(route.pending_of(&fixture.handle).is_empty(), "作废后该连接的观察表随连接消失")` 在本用例**恒真**：用例未提交任何命令 ⇒ `states` 里无该连接条目；`forget_connection`（`command.rs:1528`）只清 `states`、不注销注册表，去掉该调用断言同样通过 | 该条断言不提供任何判别力，会让覆盖表对「作废后状态被回收」的判断略微高估（既有撤销用例同款，非本 fix 引入） | 删除该断言，或先提交一条命令再断言（参照 `tests.rs:2074/2094` 的「先 1 条、后 0 条」写法） | 新增（非阻断） |
| RV3-IMPL-F5 | SUGGESTION | `crates/app/tests/node_link_e2e.rs:836-843` × `crates/app/tests/support/nodelink.rs:690-700` | e2e 只断言 close **code**（`assert_eq!(code, 1000)`），support 客户端的 `close_code()` 丢弃 reason（`Frame::Close(code, _)`），因此文档规定的说明性 reason 文本在端到端链路上没有断言 | 无功能影响；但 §8.2 特意规定了「给出说明…的 close reason」，而唯一的 wire 层证据是「底层库会带上 reason」的代码推理（§2.2）。单测已断言注册值，故风险低 | 让 `close_code()` 返回 `(code, reason)`（或加 `close_code_and_reason()`）并在步骤 ④ 断言 reason 含 `re-confirmed`/`updated catalog`；注意 §8.2 写的是「形如」，不必逐字 | 新增（非阻断） |
| RV3-IMPL-F6 | SUGGESTION | `crates/server/src/local_admin/router.rs:575-631`（`device_pair_confirm`）＋`crates/server/src/local_admin/pairing.rs:44-66`（trait 无设备侧对等方法） | 设备面配对确认会改写该设备的 scopes（授权变化），但既不作废任何连接、也没有 `close_device_after_reauth`；今天没有设备连接（`server::sync` 未落地，`close_device` 是记录型 no-op），因此**与本次节点语义不冲突** | 现在无可观测漏洞；但这是与 RV2-IMPL-F1 同类的缺口，切片 7（设备/sync）落地时会复现——届时应以同一套「落定后作废连接 + 明确关闭码语义」处理 | 登记为后续切片要求（写入 design/verification 的后续项）；**现在不要**新增空实现方法（推测性 scaffolding） | 新增（非阻断） |
| RV3-IMPL-F7 | SUGGESTION | `crates/server/src/node_link/conn/registry.rs:358`（`ConnectionRegistry::close_node(node, code, reason)`）vs `crates/server/src/node_link/command.rs:1568-1652` | 该注册表方法在工作区内**无调用者**（base `3cadb12` 亦然，非本 fix 引入），而 `node_revoked`/`node_reauth` 各自手写同一段「`handles_for_node` → `request_close`」循环（后者还需额外的 `forget_connection`，故不便直接复用） | 无功能影响；仅是一处冗余的公开方法 + 两段重复循环 | 由后续清理轮决定：要么让不需要 `forget_connection` 的路径复用它，要么删除该方法。不建议为清理而改动本变更 | 新增（非阻断） |

**未发现问题**：分流的完整性、`1000`/`4410` 的取值与 `NODE_LINK_PROTOCOL.md` §14.2/§15/§8.2 的一致性、reason 字符串三处逐字一致、`RecordingCloser` 的两表可区分、`node.revoke` 既有断言的完整性、delta 的文件边界与依赖边界、无新增跳过/弱化/`unwrap`/detached task。**本轮无 CRITICAL、无 MAJOR。**

## 4. 我认为仍未被任何一轮覆盖的检查面

（按「本轮范围之外、但与本变更交付相关」列出，供调度者决定是否另开轮次）

1. **[PV1] 在 `043b359` 上当前不是绿的**（见 Assessment 与 handoff_index 的 PV1 行）：`reports/du1-pv1-reauth.log` 的 `cargo test --locked --workspace --all-features` 在 `-p app --test daemon_lifecycle` 停住（11 passed / 1 failed，`exit=101`，cargo 未加 `--no-fail-fast` 即在该 target 后中止）。日志**没有保留失败用例名与 panic 文本**，我无法归因；实现者在同一修订的自报日志里该 target 为 `12 passed`（`wp-fix-reauth-full.log:349-366`，其中 `the_periodic_task_runs_again_after_one_full_cycle` 单条跑了 >60 s）。该 target 不在本轮 delta 的改动面内，因此**不影响我的静态结论**，但它是交付门禁的硬证据，属**待核对且必须处置**项。
2. 合同门禁 `npm run check`（含 `check:drift`/`check:docs`/`check:agentic`）与 `[PV2]`/`[PV5]`：PENDING（集成树）。本 delta 零改动合同资产/脚本/依赖，故不可能因这 7 个文件变化，但按规则不得据此记 PASS。
3. **Access 侧对 §15 重连策略的实现**：文档现在要求「收到 4410 或 `node.trust.revoked` → 停止重连；1000（含重新配对）→ 按退避重连」，而 `node-link-client` 尚未落地，本变更没有任何可执行的客户端侧证据。属**已文档化的后续义务**，不是本变更的缺陷，但应在后续切片开工时列为必测项。
4. **收窄后的实时扇出窗口**：旧连接在 `request_close` 生效前仍可能收到一条已收窄 Export 的 `resource.event`（扇出不逐条复查 `exportIds`，RV1-IMPL-F3/RV2-IMPL-F1 的既有缺口）。本轮只补了「更快断链」，**没有任何用例或文档量化该窗口**（例如「确认返回后旧连接不再收到新事件」这种断言不存在）。
5. **多连接并发**：`handles_for_node` 一次作废同一节点的**全部**连接，但 `node_reauth`/`node_revoked` 的多连接行为（2 条连接都被关、计数为 2）没有用例；两条连接不同 `attachmentGeneration` 时的清理顺序同样无断言。
6. **关闭 reason 的线上字节**（F5）与**设备面缺口**（F6）已在 Findings 登记，此处不重复。
7. **`close_code()` 判别力的可复用性**：support 客户端丢弃 reason 这一事实只在本轮被发现；若后续要断言「关闭原因」，需要先改 support（见 F5）。

## 5. Assessment

- **本轮检视结论：PASS（对应 Target Revision `043b359e60d546316013471ecb7ac31f914f7e30`）**。在只读、未继承实现/修复对话、未执行 cargo/npm/E2E 的隔离条件下，我自读了两段 delta 的完整 diff 与整条调用链，逐字比对了文档文本与实现字符串，核对了三类替身/断言的可区分性与判别力，并检查了回归边界：**未发现 CRITICAL 或 MAJOR**；3 条 MINOR 与 4 条 SUGGESTION 均为非阻断（其中 F2 是既有错误节号引用、F1/F3 是措辞与规划文本同步，均不影响实现行为），按收口约定不阻塞收口。两条路径**确实分流**（端口方法、组合根实现、node_link 路由三层都可区分，无残留交叉调用），关闭码与 reason 与文档**逐字一致**，`node.revoke` 的既有语义与断言**未被弱化**（反而被显式钉住）。
- **不可据本报告宣称的事**：本轮**只覆盖** `8ae3b613..043b359` 这一 delta 的关闭语义与重新配对路径（不是对全量 diff 的重审，也不覆盖 RV1/RV2 已覆盖的其他面）；**不代替** Project Verify、合同门禁、[PV5] 或用例执行；不代表任务状态可勾选，也不代表变更可归档。
- **检查证据逐 ID 核对**：
  - **[PV1]（Rust 侧，修订 `043b359`）＝ 已返回但结论为 FAILED → 记 FAIL / NEW，且属待归因项**。`reports/du1-pv1-reauth.log`（2026-09-27 09:49:51 起，文件 09:50 定型，含 `exit=101`）：`cargo fmt --all -- --check` `exit=0`；`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` `exit=0`；`cargo test --locked --workspace --all-features` 在 `-p app --test daemon_lifecycle` 失败（`test result: FAILED. 11 passed; 1 failed; 0 ignored`，随后 `error: test failed, to rerun pass '-p app --test daemon_lifecycle'`、`exit=101`）。该日志**未保留失败用例名与 panic 文本**，且 `daemon_lifecycle` 不在本 delta 的改动面内、实现者同修订日志为 `12 passed`（同提交）⇒ 更像既有 flaky/环境依赖，但**证据不足以断定**。**不影响本轮静态判断**（本轮判断只依赖代码与文档文本，不依赖该 target 的执行），但它决定交付门禁：需要主 Agent 复跑（建议 `--no-fail-fast` 并保留失败块），必要时在 base `3cadb12` 对照同 target 以区分 flaky 与回归。若最终证明与 delta 有关，本报告的结论必须重开复核。
  - **[PV2]（`check-crate-boundaries.mjs`）、**`npm run check`（含 `check:drift`/`check:docs`/`check:agentic`）**、[PV5]：待核对（PENDING）**。本 delta 未新增任何依赖、未动 `docs/**`/`schemas/**`/`fixtures/**`/`compatibility/**`/`openspec/**`（已用 `git diff --name-only` 逐目录证明），因此这些门禁不改变本轮代码判断；但按规则不得据此记 PASS，须在 3.1/6.3/7.1 补齐。
  - **实现者自报日志（`wp-fix-reauth.log`）**：仅用于核对「三条负向对照是否如实记录」，核对结论为「是」（含命令、原始失败输出、退出码、失败行号与还原后的复跑结果）。**未复跑，不作为代码正确性的独立证据。**

### handoff_index

```yaml
handoff_index:
  - task_id: "2.3"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: 043b359e60d546316013471ecb7ac31f914f7e30
    evidence_type: REVIEW
    evidence_id: RV3-IMPL
    report_path: openspec/changes/node-trust-export-ids/reports/rv3-impl.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "只读专项检视 server 面：ConnectionCloser（trait/NoConnections/RecordingCloser）→ PairingSessions → router 的 node.pair.confirm 与 node.revoke → CommandRoute::{node_reauth,node_revoked} 的分流与关闭码，含 conn::session 的排空/close 帧顺序与 §14.2/§15/§8.2 的逐字比对"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.4"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: 043b359e60d546316013471ecb7ac31f914f7e30
    evidence_type: REVIEW
    evidence_id: RV3-IMPL
    report_path: openspec/changes/node-trust-export-ids/reports/rv3-impl.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "同上；组合根面（NodeLinkCloser 的 close_node 与 close_node_after_reauth、两条不同的日志事件）与 app e2e 用例 a_narrowing_repair_closes_the_live_attachment 的断言判别力（含 support 客户端 close_code 的帧读取行为与三条负向对照的日志核对）"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.3 / 2.4"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: 043b359e60d546316013471ecb7ac31f914f7e30
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "openspec/changes/node-trust-export-ids/reports/du1-pv1-reauth.log"
    result: FAIL
    evidence_status: NEW
    applicability_basis: "主 Agent 在修订 043b359 上亲跑（非实现者自述）：fmt/clippy exit=0，workspace test 在 `-p app --test daemon_lifecycle` 失败（11 passed / 1 failed，exit=101，cargo 未 --no-fail-fast 即中止）。该 target 不在本 delta 改动面内、实现者同修订日志为 12 passed ⇒ 更可能是既有 flaky/环境，但日志缺失败用例名与 panic 文本，证据不足；需主 Agent 复跑/对照 base 后归因。不影响本轮 REVIEW 的静态结论，但门禁不得记 PASS"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.3 / 2.4"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: 043b359e60d546316013471ecb7ac31f914f7e30
    evidence_type: CHECK
    evidence_id: "PV2 / npm run check（check:drift、check:docs、check:agentic）"
    report_path: NOT_AVAILABLE
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "本角色不执行 npm/cargo（调度要求）；门禁须在含完整变更目录、代码与文档两支并入的集成树跑（3.1/6.3）。本 delta 零改动合同资产/文档/脚本/依赖，故不影响本轮判断，但不得据此记 PASS"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.3 / 2.4"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: 043b359e60d546316013471ecb7ac31f914f7e30
    evidence_type: CHECK
    evidence_id: PV5
    report_path: NOT_AVAILABLE
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "reports/pv5-windows-nodelink.log 尚未产生；本轮的关闭语义用例（node_link_e2e::a_narrowing_repair_closes_the_live_attachment）应在 [PV5] 轮次留原始日志"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.3 / 2.4"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: 043b359e60d546316013471ecb7ac31f914f7e30
    evidence_type: DOCS
    evidence_id: "docs/NODE_LINK_PROTOCOL.md §8.2/§14.2/§15（docs 分支 ee1ca277）"
    report_path: "D:/Project/acp-remote-wt/export-ids-docs/docs/NODE_LINK_PROTOCOL.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "逐字比对 reason 字符串、close code 表与表注、§15 的『撤销即停』限定；发现一处既有错误节号引用（§12.6 应为 §12.3，见 RV3-IMPL-F2），不影响语义与实现"
    source_evidence: NOT_APPLICABLE
```

### 返回给主 Agent 的摘要

- **结论：PASS**（Target `043b359e60d546316013471ecb7ac31f914f7e30`；0 CRITICAL / 0 MAJOR）。
- **必查五项**：① 两条路径**确实分流**（`close_node_after_reauth` → `node_reauth`＝不推消息+1000；`close_node` → `node_revoked`＝推消息+4410；端口/组合根/node_link 三层都无交叉，`RecordingCloser` 两表可区分）；② 关闭码 `close::NORMAL`(1000) + reason 与 §8.2 文档文本**逐字一致**（三处各命中 1 次），撤销仍 4410+推送；③ 语义自洽，§13 引用全部正确，**但 §15 的 `node.trust.revoked（§12.6）` 是既有错误引用（应为 §12.3）**，`pairing.rs:64` 同病；④ e2e 断言**具判别力**，三条负向对照（删调用→超时红、换回 close_node→4410≠1000 红、临时加推→第 853 行断言红）在 `reports/wp-fix-reauth.log` 中**如实记录**（原始输出+退出码，我未复跑）；单测第三条 `pending_of` 断言**恒真**（无判别力，既有同款写法）；⑤ 无回归：`node.revoke` 断言仍成立且更严、diff 只含 `crates/**`、无新增跳过/弱化/`unwrap`/detached task；`device.pair.confirm` 无 closer 与本次语义**不冲突**（本切片无设备连接），但属同类后续缺口，建议登记而非现在实现。
- **新发现**：F1（MINOR，`router.rs:816` 仍写「与 node.revoke 同一机制」，与同函数内新注释/trait 文档矛盾）；F2（MINOR，§15 与 `pairing.rs:64` 的 `§12.6` 应为 `§12.3`，既有错误、`check-doc-links` 不覆盖）；F3（MINOR，关闭语义只落在权威文档，delta spec 无 Requirement/Scenario，`design.md` D8 措辞已过时）；F4~F7（SUGGESTION：空转断言、e2e 未断言 close reason、设备面对称缺口、`ConnectionRegistry::close_node` 无调用者 + 循环重复）。
- **仍未被覆盖的检查面（7 条，详见 §4）**：最重要的是 —— **[PV1] 在 `043b359` 上当前是红的**：`reports/du1-pv1-reauth.log` 显示 `-p app --test daemon_lifecycle` 11 passed / 1 failed、`exit=101`，且日志未保留失败用例名与 panic 文本。该 target 不在本 delta 改动面内、实现者同修订为 12 passed，故更像是既有 flaky/环境问题，但**必须由你复跑/对照 base 归因**（建议 `--no-fail-fast`）；若最终与 delta 有关，本报告结论须重开。其余：合同门禁/[PV2]/[PV5] PENDING；Access 侧 §15 重连策略（`node-link-client` 未落地）无实现证据；收窄后实时扇出窗口未被量化；多连接并发无用例；close reason 无线上字节断言；设备面缺口。
- **报告路径**：`openspec/changes/node-trust-export-ids/reports/rv3-impl.md`。
- **我未做**：修改任何代码/测试/规划/任务状态/`verification.md`，跑 cargo/npm，执行 E2E，切分支或提交。
