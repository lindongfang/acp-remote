# 候选审查报告 — `integration/sync-workspaces-and-create` @ `a6f6210`

- 审查对象：分支 `integration/sync-workspaces-and-create`，提交 `a6f6210bcb8c1aacd81a09bf5169cb5eb5e287c7`，工作树 `D:\Project\acp-remote-wt\wp4-storage`
- 合并目标：`refs/heads/main` @ `6c093f145aa3d69dcd573a6d94e31b692acb5d4b`（**未移动**，本审查全程只读，未执行任何合并）
- 分支头：WP1 `8d51e03`、WP2 `bcd2df6`、WP3 `347399f`（WP4 `1a60dc2` 的祖先）、WP4 `1a60dc2`
- 合并结构（实测 `git rev-list --parents`）：`1a60dc2 → e6f67f0(merge bcd2df6) → a6f6210(merge 8d51e03)`，即 WP2 → WP1 onto WP4 head，与 plan 的 Merge Order 一致；WP3 `347399f` 已是 WP4 祖先
- 构建目录隔离：`CARGO_TARGET_DIR=D:\Project\acp-remote-wt\target-candidate-review`

## 结论

**PASS** — 无阻断项，无 MAJOR 项。两条 MINOR/信息级观察见下。候选可进入合入流程（合入动作本身仍需 main 授权）。

理由要点：唯一的文本冲突按登记规则正确收口并经**独立重数**确认；两处静默自动合并经集合代数 / 三方逐行审计确认恰为并集；第一轮的两道 Rust 门禁失败经实证（非纸面）确认已修且修复是真实修复；`npm run verify` 全绿（1092 passed / 0 failed）；`crates/app/**` 与基线逐字节相同。

---

## 1. 冲突解决：`crates/sync-protocol/tests/envelope_fixtures.rs`

### 1.1 常量与各修订版对照

| 修订版 | `EXPECTED_VALID_MESSAGE_CASES` | `EXPECTED_ENVELOPE_REJECTED` | `EXPECTED_BODY_REJECTED` | `EXPECTED_SKIPPED_PAIRING` |
|---|---|---|---|---|
| `6c093f1`（基线） | 60 | 2 | 5 | 5 |
| `bcd2df6`（WP2） | 63 | 2 | **7** | 5 |
| `8d51e03`（WP1） | 64 | 2 | **7** | 5 |
| `e6f67f0`（WP2 合入 WP4 后） | 63 | 2 | **7**（静默自动合并） | 5 |
| **`a6f6210`（候选）** | **67** | **2** | **9** | **5** |

`EXPECTED_BODY_REJECTED` 确如记录所述被 git **静默自动合并为 7**（两侧各自独立做了 5→7，且 `e6f67f0` 处无冲突标记），并被 merger 手工改为 9。`EXPECTED_VALID_MESSAGE_CASES` 在 `e6f67f0` 时是 WP2 的 63，最终 67 是 WP1 合入时的手工收口值。

### 1.2 独立重数（不读常量，不跑 Rust 测试）

审查脚本 `D:\Project\acp-remote-wt\_review_logs\recount.mjs` 独立实现分类规则：

- 按 `crates/sync-protocol/tests/support/mod.rs` 的 `manifest_cases` 读取 manifest；
- 按 `envelope_fixtures.rs:105` 的 `is_wss_message`（`schema.ends_with("schemas/sync/v1/message.schema.json")`）判定 WSS 用例，其余按 `:130` 断言必须是 pairing；
- **信封层**不复用 Rust 代码，而是按 `crates/sync-protocol/src/envelope.rs` 重写了一遍 `Envelope::decode` 规则：`deny_unknown_fields`、`protocolVersion==1`、`type ∈ MessageType::ALL`(18)、`messageId` UUID、`connectionId`/`connectionSequence` 必须同现同缺、`PreAuth/PostAuth/Either` 的连接字段强制规则（`envelope.rs:240-253, 263-270`）；
- **body 层**用 ajv Draft 2020-12 对整篇文档跑 `schemas/sync/v1/message.schema.json`（其 `oneOf` 分支自身要求 `protocolVersion`，因此校验单位就是整篇消息，与 fixture 语义一致），并区分错误是否落在 `/body` 路径下。

结果：

```
total cases: 83
valid message cases      : 67
envelope-layer rejected  : 2
body-layer rejected      : 9
skipped (pairing)        : 5
sum check: 83 === total? true
```

11 条负例的逐条分类（与测试内的登记完全吻合）：

- 信封层 2：`invalid/sequence-is-number.json`（`connectionSequence=1` 非十进制字符串 → `Malformed`）、`invalid/ping-without-connection.json`（`control.ping` 缺连接字段 → `ConnectionFieldsRequired`）。二者正是 `envelope_fixtures.rs:167-178` 登记的两个信封层负例。
- body 层 9：`command-session-create-with-cwd`、`command-session-create-with-session-id`、`prompt-image-content`、`command-unknown-field`、`auth-unknown-field`、`remote-event-without-origin`、`snapshot-chunk-index-is-number`、`snapshot-chunk-workspace-item-has-path`、`snapshot-chunk-agent-item-missing-default`；全部在 `/body` 路径下产生 `additionalProperties` / `required` / `const` / `type` 失败。
- **0 处异常**：无「声明 valid 但被拒」，无「声明 invalid 但被接受」，无 body 之外的错误位置。

**83 = 67 + 2 + 9 + 5 成立，手工改回的 `EXPECTED_BODY_REJECTED = 9` 是正确值。** 另按 manifest 声明字段直接计数亦一致：67 条 `valid=true` 的 message 用例、11 条 `valid=false`、5 条 pairing。

### 1.3 手改是否只动了这一个常量、是否动了别的

- `git diff d28062e a6f6210 -- crates/sync-protocol/tests/envelope_fixtures.rs` **为空**。
- 逐字节 md5（去 CRLF 后）：`a6f6210` = `d28062e` = `1bdfc74f6efb1a31b1bfcf753e30fdad`。
- `PAIRING_SCHEMA_SUFFIX`（`:30`）在候选中存在、内容与基线/WP1/WP2 三方逐字节相同，`crates/sync-protocol/tests/pairing_fixtures.rs:17` 的同名常量亦未被波及。

**确认：最终文件与第一轮已验证的解决结果完全一致（modulo CRLF），merger 报告的「误删后恢复 PAIRING_SCHEMA_SUFFIX」没有留下任何残留。** 两次合并之间该 crate 测试目录唯一的差异是 `body_constraints.rs`（即第一轮门禁失败的 rustfmt 修复）。

---

## 2. 静默自动合并审计

### 2.1 改动面总账（先确认没有「计划外文件」）

以基线 `6c093f1` 为参照统计各分支触及的文件集：

- WP1 20 个文件、WP2 15 个、WP3+WP4 20 个、候选 52 个；
- **候选文件集合 ⊆ WP1 ∪ WP2 ∪ WP4 三个集合的并集，无一例外**（`comm -23` 结果为空）；
- 三方交集恰好只有 3 个文件：`crates/sync-protocol/tests/envelope_fixtures.rs`、`docs/SYNC_PROTOCOL.md`、`fixtures/sync/v1/manifest.json` —— 与 plan `## Shared File Ownership` 登记的共享面一致。

对**非共享**的 49 个文件逐个比对 blob：候选 blob 与其归属分支的 blob **完全相同**（无一处被合并改写）。唯一例外正是上面那 3 个共享文件。

### 2.2 `fixtures/sync/v1/manifest.json` — 集合代数

审计脚本 `manifest-audit.mjs` / `manifest-audit2.mjs`：

```
base cases: 72   wp1 added: 6   wp2 added: 5   cand added: 11
wp1/wp2 overlapping additions: 0
wp1 additions dropped in candidate: 0 []
wp2 additions dropped in candidate: 0 []
candidate additions not traceable to wp1/wp2: 0 []
duplicate fixture entries: []
base cases missing from candidate: 0 []
base cases appear in original relative order: true
schemaVersion: base 1 / wp1 1 / wp2 1 / cand 1
transcriptVectors: base 19 / cand 19 / missing 0 / extra 0
```

新增的 11 条及其来源（候选下标:fixture）：

```
 6: valid/sync-snapshot-chunk-sessions-workspace.json   ← WP2
 7: valid/sync-snapshot-chunk-workspaces.json           ← WP2
 8: valid/sync-snapshot-chunk-agents.json               ← WP2
11: valid/command-session-create.json                   ← WP1
12: valid/command-result-session-create-accepted.json   ← WP1
13: valid/command-result-session-create-completed.json  ← WP1
14: invalid/command-session-create-with-cwd.json        ← WP1
15: invalid/command-session-create-with-session-id.json ← WP1
16: valid/auth-client-hello-session-create.json         ← WP1
41: invalid/snapshot-chunk-workspace-item-has-path.json ← WP2
42: invalid/snapshot-chunk-agent-item-missing-default.json ← WP2
```

**结论：manifest 恰为 base ∪ WP1 ∪ WP2，无丢失、无重复、无交错、无覆盖基线条目；`transcriptVectors` 与 `schemaVersion` 未被合并波及。**

### 2.3 `docs/SYNC_PROTOCOL.md` — 逐行三方审计

审计脚本 `doc3way.mjs`（把四个修订版的行拆成集合后逐行归因）：

```
base lines: 1558   wp1: 1566   wp2: 1574   cand: 1582
candidate lines not in base: 28
candidate added lines NOT traceable to WP1 or WP2: 0
WP1 added lines missing from candidate: 0 []
WP2 added lines missing from candidate: 0 []
base lines absent from candidate (deletions): 11  —— 全部可归因到 WP1 或 WP2 的有意改写
lines added more often than WP1+WP2 combined (duplication): 0
```

11 处删除逐条归因（无一是合并产物，均为某一方的有意改写，且都被对方分支保留）：

- WP2 侧改写：`> 日期：2026-09-18`、`- 本表的五个 feature 构成 v1 baseline…`（WP1 把它改成「七个 feature」）、旧的两处 `chunkCount` 示例行、`session.create` 的 Node Link-only 行与 §11.5 首行说明（「13 条」表述被 WP1 改写为区分 Sync 12 / Node Link 13 的段落）、§16.2 的「增加 `session.create` 需要新 feature」条目。
- WP1 侧改写：§9.4 的 `chunkCount` 段落与 sessions item 表格行、示例里的 `updatedAt: timestamp` 行。

**结论：文档恰为 base ∪ WP1 ∪ WP2，无丢失、无重复、无交错。**

### 2.4 文档内部一致性（本次合并最易出错处）

| 断言 | 实测 |
|---|---|
| `docs/SYNC_PROTOCOL.md` §9.4 的资源清单 = 8 条 | 8 条：`sessions, messages, turns, pending_interactions, config_options, capabilities, workspaces, agents` |
| §9.4 清单 == `sync.schema.json#/$defs/snapshotResource.enum` | 逐条相等（同序） |
| §9.4 清单 == `crates/sync-protocol/src/sync.rs` 的 `SnapshotResource::ALL` | 8 == 8，同序相等 |
| §5.2 feature 表 = 7 行 | 7 行，且正文写「本表的**七个** feature」，与表内行数自洽 |
| §5.2 的 7 行 == `compatibility/features/v1/features.json` 的 `protocols.sync.features` | 集合相等 |
| §11.5 命令表 = 13 行 | 13 行（`session.list … session.create, session.resume`） |
| §11.5 说明段 | 明确写出「`commands.json` 13 条 / sync schema 枚举 12 条 / node-link schema 13 条」，与实测一致 |
| WP1 的 §5.2 / §11.3 / §11.5 / §16.2 | 四节全部存在且互相引用一致 |
| WP2 的 §9.4 / §9.6 / §10.3 | 三节全部存在；§9.4 的 `workspaces`/`agents` item 行（2 行）与 §10.3 的 `workspaceRef`/`agentCatalogEntry` 表述互相吻合 |

新章节互不矛盾：`§9.4` 说目录资源受 `core.local-catalog.v1` 门控、未协商时**连空数组也不得发送**；`§5.2` 的 `core.local-catalog.v1` 行与 `core.session-create.v1` 行分别说明「目录快照 + `SessionSummary.workspace` 可选字段」与「`session.create` 的 payload 与终态」，`§11.5` 的 `session.create` 行指向这两个 feature；`§11.5` 的 `session.create` 行与 `§11.3` 的首批命令列表一致。

---

## 3. 第一轮两道门禁失败：确认是真修，不是纸面

### 3.1 `cargo fmt --check` — `crates/sync-protocol/tests/body_constraints.rs`

`bcd2df6` 声称「whitespace-only」。独立验证（`git show <rev>:file | tr -d '[:space:]' | md5sum`）：

```
6c093f1 (基线)          strip-md5 = 219b09ca13939a94f3952e72fb920488
da9e390 (= bcd2df6^)    strip-md5 = 95e0316db5e431443be18465d66a57bb
bcd2df6                 strip-md5 = 95e0316db5e431443be18465d66a57bb
a6f6210 (候选)          strip-md5 = 95e0316db5e431443be18465d66a57bb
```

候选与 `bcd2df6` 的 blob 完全相同（`2c7b408a7e510d58bffac936e36524527e09803f`），且 `bcd2df6` 与其父提交在去空白后 md5 相同 —— **修复确为纯空白改动（rustfmt 的换行重排），零可执行语句变更**。diff 仅两处：一处把 `assert!(workspaces.is_ok(), "…{workspaces:?}")` 拆成三行，一处把 `let reserialized =\n serde_json::to_value(...)` 合并为一行。

### 3.2 `clippy::large_enum_variant` — `CommandResultPayload::SessionCreate`

`8d51e03` 把 `SessionCreate(SessionCreateResult)` 改为 `SessionCreate(Box<SessionCreateResult>)`，`from_raw` 的构造处加一层 `Box::new`，并补了一条内联断言（`crates/sync-protocol/src/command.rs:1334-1340`）：

```rust
assert_eq!(
    serde_json::to_string(result.result.as_ref().expect("completed 必须有 result"))
        .expect("CommandResultPayload 必须可序列化"),
    format!(r#"{{"sessionId":"{SESSION}","session":{SUMMARY}}}"#),
    "Box 对 serde 透明：装箱不得改变 wire"
);
```

**wire 中立性 —— 两组独立实验（都在工作树之外的 `git archive` 副本里做，用独立的 `_scratch_target`，已清理）：**

1. **去掉 `Box`（回到非装箱）**：`cargo test -p sync-protocol` 全绿（10 个测试二进制，0 failed），其中 `session_create_result_is_bound_to_completed` 通过。⇒ 装箱前后发出的 JSON 逐字节相同，装箱**确实**没有改变 wire；同一份断言在装箱与非装箱两种表示下都成立，这正是 serde 对 `Box<T>` 透明的可观测证据。
2. **杀掉这条断言（把待比较对象换成 `&payload.session_id`）**：该测试 **FAILED**，报 `left: "\"5d73…\""` vs `right: "{\"sessionId\":\"5d73…\",\"session\":{…}}"`。⇒ 断言是**承重的、非空转**，它真的在钉「重编码 == wire 原文」，不是恒真式。

补充：在去 `Box` 的副本上跑 `cargo clippy -p sync-protocol --all-targets --all-features` 会报 `error: large size difference between variants … clippy::large_enum_variant`，确认装箱确实是这条门禁的修复而非绕过。

另核实：`8d51e03` 只改 `crates/sync-protocol/src/command.rs` 一个文件（+23/-8），未触碰 schema、fixture、文档或任何其他 crate；`crates/node-link-protocol` 侧同名的 `CommandResultPayload::SessionCreate` 仍是**非装箱**的独立类型（`crates/node-link-protocol/src/command.rs:1038`），不在本次 clippy 失败面内，也未被顺带改动。

---

## 4. 跨工作包回归检查（机器核对，非目测）

脚本 `vocab-audit.mjs` / `packs-audit.mjs`：

| 检查 | 结果 |
|---|---|
| `commands.json` 命令总数 | 13 |
| 其中 `transport` 含 `sync` 的 | 12：`session.list, session.read, command.status, session.mode.list, session.config.list, session.prompt, session.cancel, elicitation.respond, session.mode.set, session.config.set, permission.resolve, session.create` |
| `schemas/sync/v1/command.schema.json` 的 `command` 取值集合 | 12，与上**相等** |
| `crates/sync-protocol/src/command.rs` `CommandName::ALL` | 声明长度 12、实枚举 12、`as_str` 字符串 12，与上**集合相等** |
| `schemas/node-link/v1/command.schema.json` | 13 == `commands.json` 全集 |
| `docs/SYNC_PROTOCOL.md` §11.5 行 | 13（含 `session.resume`），说明段与实测口径一致 |
| `docs/SECURITY_DESIGN.md` §10.2 命令名 | 13 |
| `broker::required_grant` 分支 | 命中集合全部在 catalog 内，无孤儿分支（按字面扫描只抓到 6 条 `name => grant` 字面量，其余为通配/共享分支，属既有写法，非本次问题） |
| `identity-auth::PACKS` vs `commands.json#packs` | 5/5 逐项**相等**（`pack.create-session = [session.create]`，且不进任何 preset） |
| `identity-auth::PRESETS` vs catalog | 2/2 **相等** |
| `identity-auth::GRANTS` vs catalog | 5/5 **相等**（`grant.remote-work = [session.create, session.resume]`） |
| 同一命令出现在多个 pack | 无 |
| 无 pack 覆盖的命令 | 仅 `session.resume`（既有设计：只经 Node Link，不进 Sync pack） |
| `sync.schema.json#/$defs/snapshotResource.enum` | 8，含 `workspaces`/`agents` |
| `SnapshotResource::ALL` | 8，与上同序相等 |
| `sync-protocol::SnapshotResource` vs schema 的漂移门禁 | **存在**：`crates/sync-protocol/tests/schema_drift.rs:332 snapshot_resources_match_schema_enum`（WP2-F4 的修复，逐条且同序断言） |

`common::SessionSummary.workspace`（`schemas/sync/v1/common.schema.json#/$defs/sessionSummary`）：

```
required: ["sessionId","title","agent","state","origin","currentMode","version","createdAt","updatedAt"]
workspace in required? false          ← 可选，保持缺席语义
workspace: {"oneOf":[{"$ref":"#/$defs/workspaceRef"},{"type":"null"}]}   ← 缺席 ≠ null，两者都合法
additionalProperties: false
workspaceRef: {type:object, additionalProperties:false, required:[alias,displayName],
               alias: string 1..64, displayName: string 1..128}   ← 无任何路径字段
```

Rust 侧对应 `crates/sync-protocol/src/common.rs:407-418, 450-466`（三态 `Option<Nullable<WorkspaceRef>>`，`None` 时整个键被跳过；`Nullable` = 显式 `null`）。Rust 侧有专门用例 `crates/sync-protocol/tests/body_constraints.rs:203 session_summary_workspace_keeps_absent_null_and_value_apart` 把「absent / null / value」三者分开钉住，本审查在 `npm run verify` 中确认该二进制通过。

core / storage 侧一致性：

| 检查 | 结果 |
|---|---|
| `core::model::SessionSummary::try_new` 形参 | 11 个，末位 `workspace: Option<WorkspaceRef>`（`crates/core/src/model/session.rs:333-344`） |
| `core::ports::SessionUpdate.workspace_alias` | 存在，`Option<String>`（`crates/core/src/ports.rs:195`），文档注明与 `workspace_cwd` 同一次提交写入、`None` = 不改该列 |
| `ports::SessionStore::load_recovery` | 文档与实现都注明**不读** `workspace_alias`（`ports.rs:423`、`docs/CORE_PORTS_AND_STORAGE.md:186`） |
| `docs/CORE_PORTS_AND_STORAGE.md` §5 与 `ports.rs` | 由 `check:drift` 机器核对：15 个 trait / 96 个方法签名一致 |
| `docs/CORE_PORTS_AND_STORAGE.md` §7 与 `migrate.rs` | 由 `check:drift` 机器核对：36 条 DDL 逐条一致 |
| v6 DDL | `migrate.rs:19-21` = 6/6/3（imported 仍 3）；`:653` 单条 `ALTER TABLE owned_session ADD COLUMN workspace_alias TEXT`；`:69` 与文档 `:929` 的 DDL 行逐字一致 |
| 文档版本链 | §头部 `0.16`（WP3）+ `0.17`（WP4）两行并存且顺序正确 |

---

## 5. Coverage Index 逐行核对（53 行）

核对方式：审查员逐行读 spec 的 Requirement/Scenario，再在候选树里定位**实现证据**（代码行号，不是「文档里提到了」）。为覆盖 53 行，拆成三个只读子任务并行核对（wire/vocab 侧 R1–R17+R37–R39、session.create 侧 R18–R32、core/storage 侧 R33–R36+R40–R53），每行的证据由子任务给出后由我复核汇总。**没有任何一行只靠「文档写了」就算通过。**

### R1–R17（sync-workspace-catalog，WP2）

| 行 | 判定 | 实现证据 |
|---|---|---|
| R1 会话摘要携带目录归属引用 | VERIFIED | `schemas/sync/v1/common.schema.json#/$defs/sessionSummary.workspace`（非 required）+ `crates/sync-protocol/src/common.rs:407-418,450-466`；v6 列 `migrate.rs:69` |
| R2 已在已登记目录中创建的会话 | VERIFIED | `fixtures/sync/v1/valid/sync-snapshot-chunk-sessions-workspace.json` items[0]；投影 `crates/storage-sqlite/src/session_store.rs:61-68` LEFT JOIN `owned_workspace` |
| R3 没有目录归属的会话 | VERIFIED | 同 fixture items[1] `workspace: null`；`session_store.rs:470-479`（NULL = 未分组，无路径反查） |
| R4 目录重指向后归属不漂移 | VERIFIED | `local_config.rs:284-288` 重指向只改 path；`session_store.rs:1128-1141` 恢复不碰 alias 列 |
| R5 别名已从本机删除时归属仍稳定 | VERIFIED | `session_store.rs:475-485` JOIN 落空时回退为别名本身（非 null、不泄漏路径） |
| R6 imported 会话的归属 | VERIFIED | `common.rs:458-459` imported 固定 `Some(null)`，不猜测；fixture items[2] `origin.kind=remote` + `workspace:null` |
| R7 目录与 Agent 目录随快照下发 | VERIFIED | `sync.schema.json` `snapshotResource` 增 `workspaces`/`agents`；`sync.rs:113-116,391-398,456-463`；`common.schema.json#/$defs.agentCatalogEntry` |
| R8 首次同步获得完整目录 | VERIFIED | `valid/sync-snapshot-chunk-workspaces.json` + `…-agents.json`；`SYNC_PROTOCOL.md §9.4` item 表 |
| R9 空目录仍出现在快照中 | PARTIAL | schema 允许空 `items`、文档明确要求「已登记但无会话的 workspace 仍出现在 `workspaces` 里」；**本工作树内无服务端快照构造代码**（见下方说明） |
| R10 离线渲染不依赖在线查询 | PARTIAL | 文档 §9.4 描述客户端缓存替换语义、`features.json` 的 `core.local-catalog.v1` 供数说明；**无客户端渲染代码** |
| R11 目录资源的协商与授权过滤 | PARTIAL（已证部分 VERIFIED） | feature 已登记（`features.json`）、门控在文档与 schema 描述；**按设备的授权过滤无代码** |
| R12 未协商时不发送该资源 | PARTIAL | 文档 §9.4「未协商时 MUST NOT 发送，连空数组占位也不得发送」；**无服务端省略逻辑代码** |
| R13 未协商时摘要不含目录字段 | VERIFIED | `common.schema.json` 的 `required` 不含 `workspace` 且 `additionalProperties:false`；`common.rs:450-464` `None` = 键缺席 |
| R14 缺少目录读取授权的设备不下发 | PARTIAL | 文档 §9.4「设备无权读取的字段和资源不得进入 snapshot」；**无按设备过滤代码** |
| R15 目录相关输出不泄漏规范化路径 | VERIFIED | `workspaceRef`/`agentCatalogEntry` 均 `additionalProperties:false` 且无路径字段；负例 fixture `invalid/snapshot-chunk-workspace-item-has-path.json`（`expectedKeyword: additionalProperties`）；`common.rs:413,435` `deny_unknown_fields`；文档 §10.3 |
| R16 快照与会话摘要只含引用 | VERIFIED | 两个新快照 fixture 只含引用字段；sessions fixture 的 `workspace` 只有 `alias`+`displayName` |
| R17 错误响应不回显路径 | PARTIAL | 文档 §12.2 只允许登记字段；node-link 侧已实现（`crates/server/src/node_link/command.rs:1410` 的 `details={"parameter":"workspaceAlias"}`）；**sync 面无服务端代码** |

### R18–R32（sync-session-create，WP1）

| 行 | 判定 | 实现证据 |
|---|---|---|
| R18 命令登记与双层授权 | VERIFIED | `commands.json:16`（scope=`session.create`、pack=`pack.create-session`、grant=`grant.remote-work`、transport=[sync,node_link]）；两份协议 schema；`broker.rs:120-133` `required_grant("session.create")="grant.remote-work"`；`authorization.rs:38,76` |
| R19 未持 scope 被拒且无副作用 | VERIFIED | `broker.rs:622-633` 授权判定；`:65-87` `Denied::scope_denied()` → `authorization.scope_denied`；core 测试 `broker.rs:6595,7295-7298` 断言 `commit_count()==0`；`use_cases.rs:3159-3193` 断言授权先于任何 workspace/文件系统读取 |
| R20 持 scope 的设备成功创建 | VERIFIED | 两个 valid fixture + `command.schema.json:66-83,341-350,415`；`crates/sync-protocol/src/command.rs:853-863`；accepted→completed 的运行时路径在 Node Link 面 `crates/server/src/node_link/command.rs:600-851` |
| R21 载荷形状与拒绝 | VERIFIED | `command.schema.json:66-83` payload 只有 `workspaceAlias`/`agentId` 且 `additionalProperties:false`；`command.rs:254-269` `deny_unknown_fields` |
| R22 携带 cwd 被拒且不创建 | VERIFIED | `invalid/command-session-create-with-cwd.json`（`expectedKeyword: additionalProperties`）与 `…-with-session-id.json`，二者均在 manifest 登记为 `valid:false` |
| R23 只接受已登记引用 | VERIFIED | `command.schema.json:79-80` 描述；`SYNC_PROTOCOL.md §11.3:1233`；运行时 `use_cases.rs:248-270` 对 `LocalConfigStore` 解析 |
| R24 设备级授权语义 | VERIFIED | `authorization.rs:15-17` 模块文档（展开覆盖「当时及此后新增的全部已登记 workspace 与已配置 Agent」）；`SECURITY_DESIGN.md:240,:293` 明文；`broker.rs:622-633` 只读 scope 集合 |
| R25 新登记目录自动进入已授权范围 | VERIFIED | 授权按名判定（`broker.rs:629`、`authorization.rs:38,76`），代码中不存在 per-device 的 workspace 快照；`use_cases.rs:249-258` 在提交时读当时的 workspace 表 |
| R26 撤销后立即失效 | VERIFIED | `broker.rs:622-638` 每次提交重判且 fail-closed（`unwrap_or(false)`）；撤销路径 `use_cases.rs:618-624,689-697` |
| R27 引用失败与解析失败的分类 | VERIFIED | `use_cases.rs:238-240` 契约注释 + `:256-271` 实现：未登记别名 → 参数类 `InvalidRequest`；已登记但解析失败 → `Unavailable(IoError)`；`resolve_workspace:1647-1680` |
| R28 未登记别名是参数类错误 | VERIFIED | `use_cases.rs:2639-2700` 测试 `session_create_reports_local_resolution_failure_as_unavailable`；`:3158` 测试授权先于解析（避免成为注册状态预言机） |
| R29 已登记但解析失败是服务端错误 | VERIFIED | 同上 `Unavailable` 分类；node-link 侧 `command.rs:871-884,2046-2050` |
| R30 终态与幂等 | VERIFIED | `command.schema.json` 的 `accepted`/`completed` 分支 + `if-then` 约束（`:415`）；文档 §11.5 `session.create` 行；core `broker.rs` 的 `create_session` 幂等记录 |
| R31 重复 requestId 不产生第二个会话 | VERIFIED | `crates/core/src/broker.rs:1369-1425` `create_session` 构造 `IdempotencyRecord`，重放即早退；`crates/storage-sqlite` 的 `owned_command` 落盘；测试覆盖 |
| R32 崩溃窗口进 uncertain | VERIFIED | `recover_unsettled`/`recover_command` 对 `accepted` 记录写 `command.uncertain` + `terminalEventId`；文档 §6 第 20 条；storage 侧恢复测试覆盖 |

R31/R32 的**归属澄清**（避免误读）：运行时幂等与崩溃窗口逻辑确实存在于候选中，但位于 **Node Link 面 + core + storage**；Sync 面本身在本仓库尚无服务端 WSS 流水线（`crates/server/src` 只有 `local_admin/`、`node_link/`、`transport/`，无 `sync` 模块）。这与 plan 的 Main E2E `not-applicable` 声明（仓库当前不存在可端到端运行的产品路径）一致，不是缺口，但也不应被表述为「Sync 面已有服务端实现」。

### R33–R36 + R53（workspace-resolution，WP3）

| 行 | 判定 | 实现证据 |
|---|---|---|
| R33 创建时别名的持久化与投影来源 | VERIFIED | `broker.rs:1437-1476` 取 `ResolvedWorkspace.alias()` 原文，与 `workspace_cwd`/`agent_session_id` 同一次窄写提交；`ports.rs:189-195` |
| R34 创建写入别名与路径 | VERIFIED | `use_cases.rs:2139-2165`；`crates/storage-sqlite/tests/workspace_alias.rs:323-435`（别名原文往返 + 未写行仍 NULL） |
| R35 不按路径反查归属 | VERIFIED | `session_store.rs:476-478` NULL 直接 `None`；`workspace_alias.rs:566-619`（路径可匹配但归属仍 None） |
| R36 别名重指向不改变既有会话归属 | VERIFIED | `workspace_alias.rs:621-750`；`use_cases.rs:2192` |
| R53 未取得 ACP 会话标识时三列同为空 | VERIFIED | `broker.rs:1453-1457` 仅在 `agent_session_id` 为 `Some` 时提交三列；`use_cases.rs:2267-2317` 测试 `create_session_writes_no_recovery_columns_without_an_agent_session_id` |

### R37–R39（scope-expansion，WP1）

| 行 | 判定 | 实现证据 |
|---|---|---|
| R37 设备授权包 `pack.create-session` | VERIFIED | `commands.json` 的 `packs` 与 `authorization.rs:38` 逐项相等（见第 4 节机器核对） |
| R38 展开得到命令级 scope | VERIFIED | `authorization.rs:94-104` `expand_device_request`；`crates/identity-auth/tests/authorization.rs:259-288` 断言展开结果 = `[session.create]` 且 pack 名不进 wire |
| R39 包成员漂移被门禁发现 | VERIFIED | `identity-auth/tests/authorization.rs:49-53` `packs_match_machine_catalog`（数量 + 成员双向）、`:333+` `pack_members_are_registered_commands`；`scripts/check-command-catalog.mjs:207-216` 交叉核对 |

### R40–R52（storage-schema-v2-migration，WP4）

| 行 | 判定 | 实现证据 |
|---|---|---|
| R40 版本常量与 migration 幂等 | VERIFIED | `migrate.rs:19-21`（6/6/3）、`:652-654` 单条 ALTER ADD COLUMN、`:958-1059` 单事务 + 版本守卫 |
| R41 连续两次打开 schema 文本不变 | VERIFIED | `crates/storage-sqlite/tests/migration.rs:185-253`、`:1482-1526`、`resume_columns.rs:555-595` |
| R42 升级中途失败整体回滚 | VERIFIED | `migration.rs:1391-1479`（注入中途 DDL 冲突 → 失败回滚、版本/表结构未变、移除冲突后可重试） |
| R43 v5→v6 保留既有行且新列为空 | VERIFIED | `workspace_alias.rs:181-315`（新列为 SQL NULL、摘要 `None`、DDL 仅追加不重建） |
| R44 新列写入后可读回且不推导 | VERIFIED | `workspace_alias.rs:323-435`（写别名原文 → 关闭重开按字节读回；未写行仍 NULL）；`:438-520` 覆盖 `ModeChange` 同批写入 |
| R45 v4→v5 保留既有行且新列为空 | VERIFIED | `resume_columns.rs:294-363`；`migration.rs:1032-1178` |
| R46 升级库与新建库 owned 列清单相等 | VERIFIED | `migration.rs:611-633, 930-950, 1157-1178` 逐项比较；`:987-991` 断言追加列 nullable / 无默认值 |
| R47 v3→v4 给既有节点行写空清单 | VERIFIED | `migration.rs:815-960`（既有行 `export_ids_json` 为 `[]`、列顺序一致）；`migrate.rs:623-625` 为 `ADD COLUMN … DEFAULT '[]'` |
| R48 v2→v3 保留审计并扩展词表 | VERIFIED | `migration.rs:659-789`（审计行/`audit_id`/序列保留、新词可写、旧词拒绝、三值 CHECK）；`migrate.rs:552-611` |
| R49 会话目录归属列的写入与解释 | VERIFIED | `ports.rs:189-195`（仅创建窄写）、`:421-423`（`load_recovery` 不读）；`session_store.rs:1127-1168`（`None` 不改） |
| R50 写入别名原文 | VERIFIED | `use_cases.rs:2139-2154`；`workspace_alias.rs:438-520`（`ModeChange` 分支也原文写入并读回）；绑定点 `session_store.rs:1141,1168` |
| R51 恢复不改写目录归属 | VERIFIED | `workspace_alias.rs:752-859`（恢复前后 `quote(alias)` 相同）；`use_cases.rs:2236-2258` |
| R52 NULL 不被路径反查补齐 | VERIFIED | `workspace_alias.rs:566-619`；`session_store.rs:470-478`（只从持久化 alias 构造 `WorkspaceRef`） |

**汇总：53 行中 47 行 VERIFIED，6 行 PARTIAL（R9、R10、R11 的过滤面、R12、R14、R17），0 行 NOT-VERIFIED，0 行完全无证据。**

6 行 PARTIAL 共用一个根因，且这个根因是**变更范围本身的属性，不是缺陷**：Sync 快照的服务端构造、feature 门控的实际省略、按设备的授权过滤、客户端离线渲染，以及 Sync 面的错误响应构造，在本仓库都还没有代码落点 —— `crates/server/src` 只有 `local_admin/`、`node_link/`、`transport/`，无 `sync` 模块。这六行的 wire 合同（schema / fixture / 文档 / 枚举门禁）是完整且已钉住的，但服务端与客户端行为无法在本候选内证明。这与 plan `### Main E2E` 的 `mode: not-applicable`（理由：仓库当前不存在可端到端运行的产品路径）一致，属**已如实记录的已知限制**，不构成合入阻断。

---

## 6. 范围纪律

### 6.1 候选改动面完全落在授权范围内

52 个改动文件对四个工作包的写范围逐一归类，全部可归因（详见第 2.1 节：候选文件集合 ⊆ 三分支并集，交集恰为 3 个已登记共享文件）。逐 crate 检查未发现计划外目录（无 `crates/app/**`、无 `crates/agent-host/**`、无新增 crate、无 `openspec/**` 改动：`git diff --name-only 6c093f1 a6f6210 | grep -c openspec/` = **0**）。

### 6.2 两个既有预存问题未被偷偷修补

| 既有问题 | 核实 |
|---|---|
| `crates/app/src/daemon.rs` 的锁 vs 迁移顺序（文档 §11.3 称迁移在单实例锁之后，实现中 `Composition::assemble` 先于 `DaemonLock::acquire`） | **未修补。** `git rev-parse 6c093f1:crates/app/src/daemon.rs` = `a6f6210:crates/app/src/daemon.rs` = `c2b913e0586a34cb765fca18ff09978944e2c098` —— 与基线**逐字节相同** |
| `crates/app/tests/daemon_lifecycle.rs:648-653` 的偶发 maintenance-log 断言 | **未修补。** blob = `9d4356825b74596ae0870958d38c1654afe14621`，与基线逐字节相同 |

`git diff --name-only 6c093f1 a6f6210 -- crates/app` 输出为空。两条既有问题保持原状，交由独立变更处理 —— 这与 WP4 审查记录 WP4-F-02 / WP4-F-03 的处置一致，也保住了范围证据。

另核实第一轮 WP4-F-01（`workspace_alias.rs` 的 U+FFFD 编码污染）确实已修：`grep -rlP "\x{FFFD}"` 在 `crates/`、`docs/`、`schemas/`、`fixtures/`、`compatibility/` 下**无命中**（基线同样无命中，即该污染是本轮新引入后被修掉的）。

---

## 7. `npm run verify` 实跑输出

准备：按 WP2 集成期提示 I3，先把工作树里未被 git 跟踪的 `reports/` 移到工作树外（`check-doc-links.mjs` 会扫到它并对报告里的相对链接报错），跑完后原样移回（未编辑其中任何文件）。全量日志：`D:\Project\acp-remote-wt\_review_logs\candidate-review-verify.log`（2205 行）。

```
> acp-remote-contracts@0.0.0 verify
> npm run check && npm run check:rust
EXIT=0
```

### 7.1 `npm run check` — 十项合同门禁，逐项输出

| # | 子门禁 | 输出 |
|---|---|---|
| 1 | `check:schemas` | `schema fixtures OK: 127 valid, 30 invalid (ajv Draft 2020-12), 39 event views bound` |
| 2 | `check:commands` | `command catalog OK: 13 commands` |
| 3 | `check:errors` | `error registry OK: 58 codes across 2 protocols` |
| 4 | `check:features` | `feature registry OK: 13 feature ids across 2 protocols` |
| 5 | `check:assets` | `contract assets OK: 17 schemas, 170 fixture files, 12 transcript vectors re-encoded from input, 20 negative vectors rejected as declared, 2 SAS values recomputed` |
| 6 | `check:acp` | `ACP compatibility matrix OK: 25 methods, 11 updates, 5 content blocks, 3 tool content types, 19 capabilities, 8 invariants, 10 test families (71 rows, schema validated by ajv)` |
| 7 | `check:docs` | `doc links OK: 409 relative links, 8699 section refs across 485 markdown files`（附设计内说明：4162 条 section refs 的归属文档按设计未判定） |
| 8 | `check:boundaries` | `crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致（12 个已在矩阵登记）` |
| 9 | `check:drift` | `contract drift OK: §7 的 36 条 DDL 与 crates\storage-sqlite\src\migrate.rs 逐条一致；§5 的 15 个 trait / 96 个方法签名与 crates\core\src\ports.rs 一致` |
| 10 | `check:agentic` | `Installation: PASS`；`Totals: 19 passed, 0 failed (19 items)`；`agentic 宿主入口检查完成：17 个文件`；另有 `PASS toolchain: Node v22.22.0 / npm 10.9.4` 等 7 行 PASS |

### 7.2 `npm run check:rust` — 三个阶段

```
> cargo fmt --all -- --check && cargo clippy --locked --workspace --all-targets --all-features -- -D warnings && cargo test --locked --workspace --all-features
   ... clippy: Finished `dev` profile [unoptimized + debuginfo] target(s) in 19.63s
   ... clippy: Finished `test` profile [unoptimized + debuginfo] target(s) in 44.43s
```

- `cargo fmt --all -- --check`：无输出即通过（第一轮的 `body_constraints.rs` 两处失败已消失）
- `cargo clippy --workspace --all-targets --all-features -D warnings`：通过，日志中**无 `warning:`、无 `error:`、无 `large_enum_variant`**（`grep "clippy\|large_enum_variant"` 只命中脚本回显行本身）
- `cargo test --workspace --all-features`：**92 个测试二进制，1092 passed / 0 failed / 0 ignored 项失败**（`grep -c "test result: ok"` = 92；`grep "test result:" | grep -v " 0 failed"` 无输出）。含关键用例 `command::tests::session_create_result_is_bound_to_completed ... ok`

**`npm run verify` 整体退出码 0。**

---

## 8. Findings

| ID | 严重度 | 位置 | 问题 | 影响 | 最小修复 |
|---|---|---|---|---|---|
| CR-1 | MINOR | `openspec/changes/sync-workspaces-and-create/verification.md:106`（契约目录，本审查只读） | `## Merge History` 只登记了第一轮候选 `d28062edfc0e…`，第二轮候选 `a6f6210…` 及其两道门禁的重新修复处置（`bcd2df6` / `8d51e03` 的重新合入、PV1 通过）尚未写回 | 合入证据链在文档上停在失败状态，与实际已通过的第二轮候选不一致 | 由 main 在合入前把第二轮候选提交号、PV1 退出码 0 与本报告路径补进 `## Merge History` |
| CR-2 | 信息 | 6 行 PARTIAL（R9/R10/R11 过滤面/R12/R14/R17） | 这些行的服务端/客户端行为在候选内无代码落点（`crates/server/src` 无 `sync` 模块），只有 wire 合同层证据 | 若被表述为「已实现」会高估交付；作为 wire+类型层变更这是范围内的正确终点 | 无需改代码。建议在 `verification.md` 的覆盖核对处沿用本报告第 5 节的措辞，明确「wire 合同已钉、服务端行为不在本变更范围」，与 plan 的 Main E2E `not-applicable` 保持一致 |

无 BLOCKER、无 MAJOR。

---

## 9. 复核者声明

- 全程**只读**：未修改任何源码、schema、fixture、文档或 `openspec/**`；唯一写入是本报告，以及工作树外的临时脚本与构建目录（`_review_logs/`、`target-candidate-review/`、两个 `_scratch_*`，后两者已删除）。
- `reports/` 目录在跑门禁前移出工作树、跑完后原样移回，**未编辑其中任何文件**；移回后 `git status --porcelain` 为空，`HEAD` 仍为 `a6f6210`。
- 所有结论均来自本报告内列出的实跑输出或脚本输出；未跑 `npm run verify` 之外的任何构建/测试（除为验证 Box 中立性而在工作树外的 `git archive` 副本上做的两次定点实验，已清理）。