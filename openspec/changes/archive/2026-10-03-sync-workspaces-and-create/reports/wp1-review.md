# WP1 独立检视报告 — `session.create` 进入 Sync 词表族

- 检视对象：`feat/sync-vocab`，工作树 `D:\Project\acp-remote-wt\wp1-vocab`
- HEAD：`ceedba8896c7d6a35df504a252e4c113d80c8308`（`ceedba8 feat(sync): session.create 进入 Sync 词表族并新增 pack 与 feature`）
- 基线：`6c093f145aa3d69dcd573a6d94e31b692acb5d4b`
- diff 规模：20 个文件，+469 / −38；工作树 `git status --porcelain` 为空（无未提交残留）
- 权威合同：`openspec/changes/sync-workspaces-and-create/design.md` D3/D4/D5/D7/D8、`proposal.md`、`specs/sync-session-create/spec.md`、`specs/scope-expansion/spec.md`、`plan.md:287`（WP1 写范围）
- 构建隔离：`CARGO_TARGET_DIR=D:\Project\acp-remote-wt\target-wp1-review`
- 检视方式：只读。全程未修改任何 source / schema / fixture / doc；本文件是本次唯一写入。

---

## 结论

**FAIL**

一条 MAJOR：`docs/SECURITY_DESIGN.md` §9.4 静默删除了一条与本工作包无关的规范性安全不变量（`Node identity 变化进入 identity_changed，不能自动接受。`），删除既不在 design D5 的要求内，也不在 WP1 的任何验收标准内，且在本文档内无任何替代表述。该项需一行回填即可修复。除此之外，9 条阻断性判据全部通过，8 条命令全部绿。

值得单独肯定的两点：`crates/core/src/broker.rs` 完全未被触碰（`required_grant` 零漂移），以及 `core.local-catalog.v1` 确实由 WP1 完成登记（此前 WP2 分支存在「先引用后登记」的悬空，本工作包已消除）。

---

## 发现

### F1 — MAJOR — `docs/SECURITY_DESIGN.md:241`（§9.4 配对和认证，删除发生在当前 239 与 241 两行之间）

**问题**：本工作包在 §9.4 删除了基线中的一条既有条目：

```
- Node identity 变化进入 `identity_changed`，不能自动接受。
```

基线 `6c093f1` 的 `docs/SECURITY_DESIGN.md:240` 有该行，当前版本没有。全文检索 `identity_changed` 只剩两处命中（`docs/SECURITY_DESIGN.md:457` 与 `:466`），均属审计动作取值表，与「不得自动接受」这条判定规则无关——即该规则在本文档内**没有任何替代表述**。`docs/FRONTEND_DESIGN.md:202` 保留了一句面向客户端的 `identity_changed` 必须阻止自动信任新主机密钥，但那是前端行为权威，不能替代安全设计 §9.4 的配对/认证侧规则。

**为什么是越权删除**：design D5 对 §9 的要求是「把『Sync 首版不暴露该入口』改写为 Sync 面经设备 scope 暴露并写出副作用」。被删的这句既不是该旧表述的一部分（那一句在 §10.2，本工作包已正确改写），也不在 proposal 的 `What Changes` / `Impact` 清单内，不在 `specs/sync-session-create/spec.md` 的任何 Requirement 内，也不在 `plan.md:287` 的 WP1 验收标准内。它是一条关于**节点身份变更不得被自动接受**的信任边界规则，与设备级 scope 语义无因果关系。

**影响**：违反本次变更自身的约束「不改变威胁模型与信任边界」，并在未记录的情况下削弱一条安全基线；后续变更若以 §9.4 为准实现配对流程，会失去「Node identity 变化不得自动接受」这条硬性依据。`check-doc-links.mjs` 与其它合同门禁都不覆盖规范性条目的存在性，因此该删除不会被任何门禁捕获——这正是它必须由人工检视拦下的原因。

**最小修复**：在 `docs/SECURITY_DESIGN.md` 第 241 行（`- 每个新 WSS 完整认证，不签发长期 bearer session/refresh token。`）之前原样回填该行：

```
- Node identity 变化进入 `identity_changed`，不能自动接受。
```

无需其它改动；回填后不触及任何门禁。

---

### F2 — MINOR — `crates/sync-protocol/tests/envelope_fixtures.rs:25,27`

**问题**：本工作包修改了 `EXPECTED_VALID_MESSAGE_CASES`（60→64）与 `EXPECTED_BODY_REJECTED`（5→7）两个常量。该文件**不在** WP1 的声明写范围内——`plan.md:287` 的 WP1 Write Scope 只列 `fixtures/sync/v1/`、`crates/sync-protocol/src/command.rs` 等；`crates/sync-protocol/tests/` 整目录属 WP2（`plan.md:288`）。

**判定：越界属实，但改动是强制且最小的，不构成阻塞。** 理由：这两个常量是 `fixtures/sync/v1/manifest.json` 的派生物，而 `manifest.json` 归 WP1 写；WP1 新增 6 条 manifest case 后若不同步该常量，WP1 自己的分支 `cargo test -p sync-protocol` 必红，`plan.md:288` 也只声明「`crates/sync-protocol/tests/` 默认不需改」，而此处并非「默认」情形。改动仅两行、仅常量、无逻辑变更，且 `Shared File Ownership`（`plan.md:307-312`）未把该文件登记为共享区——这是计划表的登记缺口，不是执行者的越权。

**影响**：集成时该文件成为 WP1/WP2 的真实冲突点（见下方独立重算）。合并取值必须由重算决定，不能沿用任一分支的分支内取值。

**最小修复**：无需修改本工作树。集成时按下方重算结果取 **67 / 9**；并建议在 `plan.md` 的 Shared File Ownership 表补一行，把 `crates/sync-protocol/tests/envelope_fixtures.rs` 登记为 WP1/WP2 共享、WP1 定稿。

---

### F3 — MINOR — `docs/DEVELOPMENT_PLAN.md:62`（§7 新增段落）

**问题**：新增段落称「命令目录、两份协议 schema、…都已就位」。实际本工作包只改了 `schemas/sync/v1/command.schema.json`；`schemas/node-link/v1/command.schema.json` 未被修改。这符合 design D8 的条件式要求（「仅当 `session.create` 的 pack 字段出现在该 schema 的约束中时才需同步（核查后决定）」）——我已核实 node-link schema 内不存在 `pack` 字段约束，故无需改动，结论正确。

**影响**：措辞「两份协议 schema…已就位」可能被读成「两份都被改过」，与实际 diff 不符；对后续读者构成轻微误导（node-link 侧是「本就一致、无需变更」，不是「已同步变更」）。

**最小修复**：将该句改为「命令目录与 `schemas/sync/v1/command.schema.json`（node-link schema 无 `pack` 约束、无需变更）…都已就位」。

---

### F4 — MINOR — `docs/FRONTEND_DESIGN.md:6`

**问题**：文件头三行改写时，基线中 `> 修订记录（2026-09-18）：…` 行末的两个尾随空格（Markdown 硬换行）丢失，当前 5/6/7 行成为同一段落，渲染后「修订记录（2026-09-18）…」与「日期：2026-10-02…」会挤在一行。

**影响**：纯渲染层 cosmetic，不影响任何合同语义与门禁（`check-doc-links.mjs` 已通过）。

**最小修复**：在 `docs/FRONTEND_DESIGN.md:6` 行末补回两个尾随空格。

---

### F5 — MINOR（判定为不违反，仅备案）— `fixtures/sync/v1/invalid/command-session-create-with-cwd.json:13`

**问题**：该负例向量含 `"cwd": "C:\\Users\\dev\\projects\\work-api"`，即一个绝对路径字面量。按判据 7 的字面读法（不得有 canonical path 进入 WP1 新增的 fixture），这需要点名。

**判定：实质合规，不需修改。** 理由：(a) 该向量是 `specs/sync-session-create/spec.md`「Scenario: 携带 cwd 被拒且不创建」所**强制要求**的载体——要证明 `cwd` 被拒，向量里就必须有一个路径取值；(b) 该值是合成的（`C:\Users\dev\...`），不是任何已登记 workspace 的规范化路径，不构成 `SECURITY_DESIGN.md` §12.3 意义上的路径泄漏（该节禁止的是已登记资源的规范化路径进入对端可见输出）；(c) 同类先例已存在于 `fixtures/acp/v1/valid/tool-call-location.json` 等；(d) 仓库无任何门禁扫描 fixture 中的路径形态。

**影响**：无。记录此项是为了让判据 7 的逐字审查有明确结论，而非留给下游猜测。

**最小修复**：无。若集成方仍偏好更中性的形态，可把该值改为不含用户目录形态的占位串（如 `C:\\repo\\work-api`），但这纯属口味，不建议在集成窗口制造额外 diff。

---

## 独立重算：fixture 计数（本报告不采信任何一方的口述数字）

我从 `fixtures/sync/v1/manifest.json` 与 `crates/sync-protocol/tests/envelope_fixtures.rs` 的分类逻辑（`envelope_fixtures.rs:125-194`：`is_wss_message` 分流 → `valid` 计入 valid_message_cases；`valid:false` 且信封解码失败计入 envelope_rejected，且仅 `invalid/ping-without-connection.json` 与 `invalid/sequence-is-number.json` 走该臂；`valid:false` 且信封成功、body 层被拒计入 body_rejected；非 WSS（pairing）计入 skipped_pairing）自行枚举三份 manifest：

| 版本 | manifest 总 case | valid（WSS） | envelope_rejected | body_rejected | skipped_pairing | 常量声明 |
|---|---|---|---|---|---|---|
| 基线 `6c093f1` | 72 | **60** | 2 | **5** | 5 | 60 / 2 / 5 / 5 ✅ |
| WP1 `feat/sync-vocab` | 78 | **64** | 2 | **7** | 5 | 64 / 2 / 7 / 5 ✅ |
| WP2 `feat/sync-catalog-wire`（HEAD `da9e390`） | 77 | **63** | 2 | **7** | 5 | 63 / 2 / 7 / 5 ✅ |

分类求和自校验：64+2+7+5 = 78 = manifest 总数；63+2+7+5 = 77 = manifest 总数；60+2+5+5 = 72 = manifest 总数。

WP1 相对基线的增量逐条核对（6 条，全部为本工作包新增文件）：

| 新增 fixture | 归类 | 计入 |
|---|---|---|
| `valid/command-session-create.json` | valid | valid +1 |
| `valid/command-result-session-create-accepted.json` | valid | valid +1 |
| `valid/command-result-session-create-completed.json` | valid | valid +1 |
| `valid/auth-client-hello-session-create.json` | valid | valid +1 |
| `invalid/command-session-create-with-cwd.json` | valid:false，信封合法 → body 层拒 | body +1 |
| `invalid/command-session-create-with-session-id.json` | valid:false，信封合法 → body 层拒 | body +1 |

即 60+4 = **64**，5+2 = **7**，envelope 层与 pairing 层均未变动。**分支内取值 64/7 正确。**

WP2 相对基线的增量为 5 条（3 valid + 2 body），与 WP1 的 6 条**文件名零交集**（我逐名比对了两份新增列表，交集为空，并集 11）。因此合并后：

- valid_message_cases = 60 + 4 + 3 = **67**
- body_rejected = 5 + 2 + 2 = **9**
- envelope_rejected = 2（不变）
- skipped_pairing = 5（不变）
- 自校验：67 + 9 + 2 + 5 = **83** = 72 + 6 + 5 ✅

**结论：合并取值 67 / 9 正确。** 三方数字（WP1 的 64/7、WP2 的 63/7、合并的 67/9）全部经我独立复算确认。

---

## 阻断性判据逐条核验

| # | 判据 | 结论 | 核验方式与证据 |
|---|---|---|---|
| 1 | 命令目录一致性（5 处 + `broker.rs` 未改） | **通过** | `commands.json:16` → `session.create` 现为 `transport: ["sync","node_link"]`、`pack: "pack.create-session"`、`grant: "grant.remote-work"`、`delivery: "mvp"`；`schemas/sync/v1/command.schema.json` 的 `commandName` 枚举含 `session.create`（12 项）；`docs/SYNC_PROTOCOL.md:1273` §11.5 首列含之；`docs/SECURITY_DESIGN.md:287` §10.2 表含之。`node scripts/check-command-catalog.mjs` 输出 `command catalog OK: 13 commands`（exit 0），该脚本按 `scripts/check-command-catalog.mjs:245,250,253-262` 逐集合比对上述五处并解析 `crates/core/src/broker.rs::required_grant`。`git diff --stat 6c093f1..HEAD -- crates/core/src/broker.rs` 输出为空 → `broker.rs` **零改动**，`session.create => grant.remote-work` 的既有映射未被编辑。 |
| 2 | `pack.create-session` 存在、成员恰为 `session.create`、不进任何 preset | **通过** | `crates/identity-auth/src/authorization.rs:38` 新增 `("pack.create-session", &["session.create"])`；同文件 `PRESETS`（`:42-53`）仍只有 `preset.remote-control`（observe/interact/configure-session/approve）与 `preset.read-only`（observe），未含新包。`compatibility/commands/v1/commands.json` 的 `packs` 新增 `"pack.create-session": ["session.create"]`，`presets` 两项未变。测试 `create_session_pack_expands_to_session_create_and_stays_out_of_presets` 逐 preset 断言 `!members.contains("pack.create-session")` 且展开后不含 `session.create`，并断言包展开 ≡ 显式 scope 请求 —— 实跑 `ok`。 |
| 3 | feature 三方一致（registry / §5.2 / fixture），且 `core.local-catalog.v1` 确已登记 | **通过** | `compatibility/features/v1/features.json` 的 sync 段新增 `core.local-catalog.v1` 与 `core.session-create.v1`（均 `delivery: mvp`、`required: false`）；`docs/SYNC_PROTOCOL.md:187-188` §5.2 表新增对应两行，并把基线计数句从「五个 feature」改为「七个 feature」；`fixtures/sync/v1/valid/auth-client-hello-session-create.json` 的 `supportedFeatures` 同时列出两个新 ID。`node scripts/check-features.mjs` 输出 `feature registry OK: 13 feature ids across 2 protocols`（exit 0）——该脚本（`scripts/check-features.mjs:21-28`）按 `### 5.2 Feature ID` 标题抽取文档表并扫描 fixture 目录做三方相等断言。**WP2 分支此前「引用了 `core.local-catalog.v1` 却未登记」的悬空，在本工作包已消除。** |
| 4 | 未新增错误码 | **通过** | `git diff --stat 6c093f1..HEAD -- compatibility/errors/ schemas/*/v1/error.schema.json` 输出为空 → `compatibility/errors/v1/errors.json` 与两份协议 schema 的错误枚举零改动。`node scripts/check-error-registry.mjs` 输出 `error registry OK: 58 codes across 2 protocols`（exit 0）。文档新引用的 `authorization.scope_denied`、`protocol.schema_invalid`、`internal.unavailable`、`command.uncertain`、`protocol.feature_required` 均为既有码，已逐个在 `errors.json:9,12,21,26,29,41` 命中。 |
| 5 | Sync 命令形状：payload 只两键、未知键被拒不忽略；Rust 侧穷尽；completed 携新会话 id + SessionSummary | **通过** | Schema：`sessionCreate`（`command.schema.json:66-83`）的 payload 为 `additionalProperties: false` + `required: ["workspaceAlias","agentId"]`，两字段 `minLength:1/maxLength:128`；body 层同样 `additionalProperties: false` 且不含 `sessionId`。Rust：`SessionCreate`（`command.rs:266-274`）用 `#[serde(deny_unknown_fields)]` + `NonEmptyText<128>`，与 schema 同口径；`CommandName::SessionCreate` 在 `ALL`（12 项）、`as_str`、`from_str`（走 `ALL` 线性查找，非硬编码 switch）、`is_query`（`false`，mutation）、`requires_session_id`（`false`，禁止顶层 `sessionId`）、`requires_expected_version`（`false`）、`CommandPayload::from_raw` 臂、`CommandResultPayload::from_raw` 臂中**全部**显式出现。全文件 `unreachable!` 检索零命中；`from_raw` 用 `_ => {}` 收尾于「已完成但非查询型结果」的既有分支，`session.create` 在其之前已 `return`，不存在静默穿透。终态：`sessionCreateResult`（schema `:341-350`）`required: ["sessionId","session"]`，`session` 指向 `common.schema.json#/$defs/sessionSummary`；Rust `SessionCreateResult` 同形。实测证据：`session_create_accepts_only_the_two_registered_references` 断言 `cwd`/`exportId`/`templateParams`/`mcpServers`/缺键/空串/顶层 `sessionId` **全部 `is_err()`**（被拒不忽略）；`session_create_result_is_bound_to_completed` 断言 completed 必须落在 `SessionCreateResult`、缺 `session` 被拒、`terminalEventId: null` 被拒、accepted 的 `result` 为 null。 |
| 6 | 文档承载语义而非仅标题 | **通过** | `SECURITY_DESIGN.md:240`（§9.4）同时给出设备侧规则与副作用原句：「判定以『该 workspace 当前是否已登记、该 Agent 当前是否已配置』为准，因此**一次授予覆盖该节点当时及此后新增的全部已登记 workspace 与已配置 Agent，新登记的资源自动进入已授权范围，无需重新授权**」，并要求配对确认页原样展示、撤销即失效；§10.2（`:288-289`）重复该副作用并声明 Sync 面已暴露。`FRONTEND_DESIGN.md:286`（§9 第 1 条）已不再禁止创建，改为受限入口，并记录原型来源 `prototypes/acp-remote-pwa.html` 的 `#/dirs`、`#/dir/work-api`、「在此目录新建会话」、副作用提示与无权限文案——我逐条回查原型：`prototypes/acp-remote-pwa.html:561,562,1457,1458,1460,1923,1925` 全部命中，引用属实。`SYNC_PROTOCOL.md:1199` 已把 `session.create` 移出「尚未定义/仅 Node 本地」清单（原清单首项 `session.create` 已删，`session.delete` 升为首位）；`:187-188` §5.2 增两行；`:1270` §11.5 增命令行，写明 payload 两键、`accepted → completed { sessionId, session: SessionSummary }` 与唯一 `terminalEventId`、三类失败码、幂等语义。**（F1 的越权删除发生在 §9.4 的另一条不变量上，不影响本判据关于 session.create 语义的判定。）** |
| 7 | 无凭据 / canonical path 泄漏进 WP1 新增的 schema、doc、fixture | **通过** | 6 个新增 fixture 全文检索 `token\|secret\|password\|api[-_]?key\|bearer\|credential` 零命中。新增 schema 文本只出现字段名与取值域，无任何真实路径或密钥。文档新增段落一律以「规范化路径不得出网」的**禁令**形式出现（`SECURITY_DESIGN.md:312,339`、`SYNC_PROTOCOL.md:1279`），未附带任何真实路径实例。唯一含路径字面量的是负例向量 `invalid/command-session-create-with-cwd.json:13`，判定与理由见 F5（合成值、spec 强制要求、实质不泄漏）。 |
| 8 | node-link 侧形状不变，仅注释可能变 | **通过** | `git diff --stat 6c093f1..HEAD -- schemas/node-link/` 输出为空 → node-link schema 零改动；`crates/node-link-protocol/src/command.rs` 的 diff 为 `4 insertions(+), 2 deletions(-)`，全部落在文件顶部 `//!` 模块注释（把「Sync 的 11 条」更正为「Sync 面接受的 12 条」，并点明两面 payload 不同）。已核实 node-link schema 内无 `pack` 字段约束，故 design D8 的条件式要求不触发（与 F3 同源）。`node scripts/check-schema-fixtures.mjs` exit 0。 |
| 9 | 八条命令实跑 | **通过** | 见下节，全部 exit 0。 |

---

## 八条命令的真实输出

工作目录 `D:\Project\acp-remote-wt\wp1-vocab`，`CARGO_TARGET_DIR=D:\Project\acp-remote-wt\target-wp1-review`。

### 1) `node scripts/check-command-catalog.mjs`
```
command catalog OK: 13 commands
exit=0
```

### 2) `node scripts/check-features.mjs`
```
feature registry OK: 13 feature ids across 2 protocols
exit=0
```

### 3) `node scripts/check-schema-fixtures.mjs`
```
schema fixtures OK: 124 valid, 28 invalid (ajv Draft 2020-12), 39 event views bound
exit=0
```

### 4) `node scripts/check-contract-assets.mjs`
```
contract assets OK: 17 schemas, 165 fixture files, 12 transcript vectors re-encoded from input, 20 negative vectors rejected as declared, 2 SAS values recomputed
exit=0
```

### 5) `node scripts/check-error-registry.mjs`
```
error registry OK: 58 codes across 2 protocols
exit=0
```

### 6) `node scripts/check-doc-links.mjs`
```
doc links OK: 407 relative links, 8678 section refs across 485 markdown files
note: 4162 section refs 的归属文档由上下文决定（未在同一子句内指名文档），按设计未判定；设 DOC_LINKS_VERBOSE=1 可列出位置
exit=0
```

### 7) `cargo test --locked -p sync-protocol -p identity-auth`
全部测试目标通过，零失败。逐目标摘要：
```
identity_auth  (unittests)          ok. 0 passed
tests\authorization.rs              ok. 16 passed; 0 failed
tests\handshake.rs                 ok. 20 passed; 0 failed
tests\pairing.rs                   ok. 30 passed; 0 failed
tests\parity.rs                    ok.  2 passed; 0 failed
tests\ports.rs                     ok.  9 passed; 0 failed
tests\transcripts.rs               ok.  5 passed; 0 failed
sync_protocol (unittests)          ok.  4 passed; 0 failed
tests\body_constraints.rs          ok.  9 passed; 0 failed
tests\envelope_fixtures.rs         ok.  7 passed; 0 failed
tests\field_constraints.rs         ok. 10 passed; 0 failed
tests\pairing_fixtures.rs          ok.  6 passed; 0 failed
tests\schema_drift.rs              ok.  5 passed; 0 failed
tests\tables_match_registry.rs     ok.  3 passed; 0 failed
tests\transcript_vectors.rs        ok.  2 passed; 0 failed
tests\view_projections.rs          ok.  3 passed; 0 failed
doc-tests identity_auth            ok.  2 passed (compile-fail)
doc-tests sync_protocol            ok.  0 passed
finished in 16.85s
```
与本工作包直接相关的四个用例单独过滤实跑：
```
test create_session_pack_expands_to_session_create_and_stays_out_of_presets ... ok
test command::tests::session_create_result_is_bound_to_completed ... ok
test command::tests::session_create_accepts_only_the_two_registered_references ... ok
test command_name_covers_the_sync_subset_and_round_trips ... ok   (并入上两项所在的 lib 测试组，4 passed)
```

### 8) `cargo clippy --locked -p sync-protocol -p node-link-protocol -p identity-auth --all-targets --all-features -- -D warnings`
```
Checking acpr-wire v0.0.0
Checking sync-protocol v0.0.0
Checking node-link-protocol v0.0.0
Checking core v0.0.0
Checking identity-auth v0.0.0
Finished `dev` profile [unoptimized + debuginfo] target(s) in 5.70s
```
零 warning（`-D warnings` 下无任何诊断输出）。

---

## 补充观察（非发现，供参考）

- `compatibility/commands/v1/commands.json` 与 `compatibility/features/v1/features.json` 的 `revision` 字段仍为 `2026-09-18`，本工作包未上调。仓库先例一致：上一次改动 `commands.json` 的提交 `db129c2`（session-resume）同样未上调 revision，且无门禁要求。故不作为发现。
- 本工作包新增的 6 个 fixture 均以无尾随换行结尾，而 `fixtures/sync/v1/` 下既有 72 个 fixture 全部以 `\n` 结尾。这是与既有约定的不一致，但任何门禁都不检查该属性，`check-schema-fixtures.mjs` 与 `check-contract-assets.mjs` 均通过。之所以不单列为发现：它纯属文本风格，且在新分支上批量补换行会与 WP2 的 fixture 改动在相邻 manifest 区域制造额外 diff，得不偿失。若集成方希望统一，建议在集成提交里一次性处理全部 11 个新 fixture。
- `crates/sync-protocol/src/command.rs` 末尾新增的 `mod tests` 内嵌于 `src/`（而非 `tests/`），符合该 crate 既有惯例（`src/` 内已有 `CommandState`、`ElicitationAction` 等的内联测试模块），未越出 WP1 写范围（`crates/sync-protocol/src/command.rs` 在 `plan.md:287` 列表内）。