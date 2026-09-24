## Context

现状（已核实的代码事实，供实现者定位）：

- 事件 view 是 core 自己的**已验证 JSON 文本**：`crates/core/src/model/json.rs` 的 `ViewJson { text, depth }`，构造时校验「合法 UTF-8、良构 JSON、顶层 object、深度 ≤ 128」，之后**逐字节保留**（不重排键、不丢未知字段）。core 不允许出现 `serde_json::Value`（`docs/CORE_PORTS_AND_STORAGE.md` §2）。
- core 已有**手写文本级 JSON 读取工具**（`broker.rs` 的 `json_members` / `json_string` / `json_index`，用于 `delta_fragment`、`view_interaction_id`），证明「无 serde_json 也能按键读取顶层成员」这条路已经在生产代码里跑通。
- turn 归属今天在**推导阶段**解析：`commit_chunk`（`crates/core/src/broker.rs`）里 `let turn = event.turn.clone().or_else(|| running.clone())`，即「事件自带归属，否则回落到当前运行中的 turn」。
- `PendingEvent.turn: Option<TurnId>`（`crates/core/src/model/event.rs`）是落盘 `owned_event.turn_id` 的来源；owned 提交只有一个漏斗：`Broker::commit_owned`（`crates/core/src/broker.rs`）。
- 会话版本由存储层在提交事务内递增：`crates/storage-sqlite/src/session_store.rs` 的 `UPDATE owned_session SET version = version + 1 … RETURNING version`（含 `closed_at` 变体），无状态分支读回原值；`CommitOutcome.version` 是唯一权威返回值。命令路径已经在提交前读取当前版本（`command_session_version`，用于 `expected_version`）。
- imported 交付走另一条路径（`CommittedDelivery::Imported` 的分发点），它不做 owned 内容写入（no-content-cache），但同样要广播 view。

动机与范围见 `proposal.md` 的 Why / What Changes；行为要求见 `specs/core-event-view-identity/spec.md`（本文件只解释如何实现与为什么这样取舍）。

## Goals / Non-Goals

**Goals:**

- 让**持久化**与**广播**的 view 都满足 `docs/SYNC_PROTOCOL.md` §10.3 的 `turnId` / `version` 最低字段，且两者取值来自同一个权威事实。
- 把「谁提供身份、谁提供 ACP 派生字段」的职责分工固定下来：适配器提供 ACP 派生投影（`block`、`title`、`options` 等），core 提供 turn 归属与会话版本。
- 让「存储层版本规则」与「core 推导规则」的漂移变成**可观察的显式失败**，而不是静默不一致。
- 保持 ACP 原文保真、view 未知字段保真、core 依赖集与端口签名不变。

**Non-Goals:**

- 不改 `SessionEndpoint::prompt` 的签名（用户已裁定），不给适配器传入 `TurnId`。
- 不改 Sync/Node Link 的 wire schema、fixtures、`docs/SYNC_PROTOCOL.md` §10.3（它已是权威要求）。
- 不做 storage DDL/migration、不新增 `owned_event` 列。
- 不给 core 引入任何 JSON 解析依赖（`serde_json` 等）。
- 不改 agent-host 的产出行为；不顺手收口上一变更登记的其他「接线 app 前」项。

## Decisions

### D1 turn 归属在提交前一次性定稿，注入以该值为唯一依据

把今天分散在推导阶段的「事件自带 → 否则回落当前运行 turn」解析上移到**提交前**：在构建 owned 批（`commit_owned` 的入口）时，对批内每个事件解析出权威 turn，写回 `PendingEvent.turn`，并据此把 `turnId` 注入其 view。推导阶段不再二次解析（改为直接使用已定稿的 `turn`），这样「落盘的 `owned_event.turn_id`」「view 里的 `turnId`」「广播给客户端的事件 turn」三者必然一致。

- 理由：持久化发生在提交内，而 view 文本也是提交的输入；只有把归属定稿提到提交前，落盘 view 才可能合规，replay 也才合规（Sync 从存储读 view，不会二次拼装）。
- 备选（否）：只在广播路径注入（持久化 view 仍缺字段，replay 不合规）；让 Sync 适配器自己拼 `turnId`（把 core 的身份知识推到下游，且 `owned_event.turn_id` 与 view 可能不一致）。
- 无 turn 归属的事件（会话级、Agent 级）保持不注入——不得为「填满字段」而伪造归属。

### D2 版本注入与漂移断言都落在 owned 提交漏斗

在 `commit_owned` 内、调用存储层提交之前：

1. 推导预期版本 `predicted`：批内含 `state` 变更时 `expected_version + 1`，否则保持 `expected_version` 不变；`expected_version` 为 `None` 的批（如新建会话）不涉及要求 `version` 的 view，按「不注入 `version`」处理。
2. 对批内 §10.3 要求 `version` 的 view（当前为 `session.mode.changed`、`session.config.changed`）注入 `predicted` 的**十进制字符串**。
3. 提交后比对 `CommitOutcome.version == predicted`；不等即 fail-closed：返回显式错误、不发布该批、不把事件当作可重放（提交若已落盘，遵循既有的失败关闭语义，不得对外声称成功）。

- 理由：注入必须发生在提交前（view 是提交输入），而权威版本在提交后才产生；因此把「规则」显式化并用一次断言把它钉死，是唯一既不猜值又不改端口/DDL 的路径。
- 备选（否）：① 新增 `owned_event.session_version` 列（跨 crate + DDL + §7 + drift 门禁 + 迁移，且把协议字段知识下沉到存储层）；② 把版本权威从存储层移进 core（改 `OwnedCommit`/`CommitOutcome` 契约）；③ 只在广播路径注入 `version`（replay 缺字段）。三者都比本方案面大，若 D2 的断言在实现期证明规则不成立，回来在 design 与 plan 里改选 ①（并在变更内登记 Check Plan Change）。
- imported 路径**不做**版本注入：`version` 由拥有该会话的节点注入并随事件到达，本端原样保留（`specs/core-event-view-identity/spec.md` 的对应场景）。imported 若缺 `turnId` 则按 D1 补，已有则不覆盖。

### D3 文本级注入工具：只做前置插入，不重排、不改写既有成员

在 `core::model::json` 增设一个**只在上位插入一个顶层成员**的能力（`ViewJson` 上的方法或同模块自由函数），实现复用现有文本工具（`json_members`/`json_string`），做法是把新成员放在原对象体的最前面，其余字节原样搬移，再交 `ViewJson::new` 重新校验（深度可能 +1，仍受 `MAX_JSON_DEPTH` 约束）。

- 冲突处理：注入前先用既有的顶层成员读取确认目标键是否存在；存在时比较取值——一致则**不重写**（保留原字节），不一致则返回显式错误（fail-closed），不得覆盖也不得生成第二个同名键。
- 值编码：`turnId` 与 `version` 都写成 JSON 字符串（转义按现有 `json_string` 的对应编码规则处理，只允许受控字符集：UUID/RequestId/Digest 一类的 ASCII 标识与十进制数字串）。
- 备选（否）：引入 `serde_json`（违反 core 依赖 allow-list 与 §2）；用字符串替换 `}` 做插入（在存在嵌套对象时会插错层级）。

### D4 `turn` 定稿与推导复用的边界

推导阶段今天依赖 `running`（来自 `Slot` 的当前 turn）。D1 之后，推导必须改用事件上已定稿的 `turn`，避免出现「落盘 view 说 turn A、状态推导按 turn B」的双源。若某条既有路径确实无法在提交前知道归属（例如事件由适配器在 turn 结束后异步到达），则该事件按「无归属」处理并保持不注入——宁可字段缺失也不制造与落盘 `turn_id` 不一致的值；这类路径必须在 apply 中逐个确认并在 `verification.md` 记录（如果没有，明确写「无」）。

### D5 `TurnAccepted.turn` 的语义写实（RV-DU1-F6）

保留字段与端口签名不动，但把语义写死并用测试固定：

- `prompt` 的接受结果**不参与** turn 归属决策；core 一律使用自己分配的 `TurnId`（今天的实现已是 `Ok(_) => Ok(true)`，本变更把它变成**有断言、有注释、有文档**的约定，而不是「碰巧没读」）。
- 适配器返回值与 core 分配值不同时，行为必须与「返回相同/不返回」完全一致：不产生第二个 turn 行、不影响事件归属与 view 的 `turnId`。
- 端口契约文档（`docs/CORE_PORTS_AND_STORAGE.md` §5.1）与模型注释同步写明该字段是适配器侧占位/审计值。

### D6 接口与数据约定（供下游拆包与测试设计）

- **接口**：`Broker::commit_owned` 仍是唯一 owned 提交入口；本变更**不新增端口方法、不改任何既有端口签名**。新增的内部能力只是 `core::model::json` 的文本插入与 `broker` 内部的归属定稿步骤。
- **数据**：不新增/不修改任何表与列；`owned_event.turn_id` 的语义不变（本变更只是保证 view 与它一致）。view 的字段形状：`turnId` 为字符串，`version` 为十进制字符串。
- **环境**：无新增依赖、无新进程/端口/文件；测试沿用 core 既有的 fake 存储与 broker harness（内存实现与 SQLite 实现都要跑到）。

## Risks / Trade-offs

- [存储层的版本规则不是「带状态变更 +1、否则不变」] → 提交前后做 fail-closed 比对，用 fake 存储造出「返回不同版本」的反例用例；一旦实现期证明规则不成立，立即改选 D2 的备选 ①（新增列）并在变更内登记 Check Plan Change 与受影响任务。
- [view 已带 `turnId`/`version` 且取值不同] → 一致性校验 + 显式失败 + 冲突用例（断言「不写行、不发布、状态不变」）。
- [文本注入改变 view 字节，导致既有快照/契约断言失败] → 只做前置插入、绝不改写既有成员；受影响断言按「多了一个字段」的预期显式更新，并在 `verification.md` 的 Failures and Retests 里记录哪些断言因预期变化而更新（不得为了绿灯删断言）。
- [归属双源（D4 的边界）] → 归属只在提交前定稿一次；推导改用同一值；无法定稿的路径按「无归属」处理并逐个确认、记录。
- [误改 core 依赖或端口] → 依赖 allow-list 与端口漂移由 `check:boundaries`/`check:drift` 门禁守住；本变更不得修改 `§9 判据 13` 的 allow-list 或 `crates/core/src/ports.rs` 的签名。
- [imported 重复注入] → imported 只补缺失的 `turnId`，既有的 `turnId`/`version` 原样保留（有用例覆盖）。

## Open Questions

无（会改变规范、方案或任务拆分的问题已在 `proposal.md` 的 `decision_bounds` 内解决或列为需用户决策项；D2 的版本规则有明确的 fail-closed 兜底与备选路径）。
