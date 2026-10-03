# WP2 独立检视报告 · Round 2 — `feat/sync-catalog-wire` @ `da9e390`

- 检视对象：worktree `D:\Project\acp-remote-wt\wp2-wire`，分支 `feat/sync-catalog-wire`，HEAD `da9e39040e02a6d5d75407d27acf3bbcbb479ca6`
- 被检视的修复提交：`da9e390`（`fix(sync): 修正快照目录文档交叉引用、示例计数与枚举漂移门禁`），其父为 Round 1 判定 PASS 的 `ba2d1d6`
- 变更基线：`6c093f145aa3d69dcd573a6d94e31b692acb5d4b`
- 检视方式：只读。未修改 worktree 内任何源码、schema、fixture、文档或配置（结束前 `git status --porcelain` 为空、`git diff HEAD` 为空，已核对）。所有变异实验都在 worktree 之外的 scratch 副本进行。
- 自建构建目录：`CARGO_TARGET_DIR=D:\Project\acp-remote-wt\target-wp2-review2b`

---

## 0. 结论

**PASS。** F1–F4 四条 MINOR 全部真实修完、方向正确、没有副作用；`da9e390` 未触及任何 wire 字节形状、schema 语义或声明的 Write Scope 之外的文件；新加的枚举漂移门禁经变异实验证明**非空转**（单侧新增、纯重排两种漂移都被抓住）；规范化路径禁令未被重新引入。

只有两条**不属于 WP2 缺陷**的集成期提示（I1、I2）与一条需要 main 知悉的门禁卫生问题（I3，见 §5）。

---

## 1. 逐条修复核验（Round 1 报告的 F1–F4）

### F1 — 路径禁令出处改指 `SECURITY_DESIGN.md` 第 12.3 节 → **已修，三处全改**

| 位置 | 现状 |
|---|---|
| `docs/SYNC_PROTOCOL.md:802` | `…MUST NOT 出现在快照、摘要或任何对端可见输出中（见 SECURITY_DESIGN.md 第 12.3 节）。` ✅ |
| `docs/SYNC_PROTOCOL.md:1090` | `…command.result 或审计记录的前像中的任何字段（见 SECURITY_DESIGN.md 第 12.3 节）。` ✅ |
| `crates/sync-protocol/src/common.rs:410` | ``/// （`SYNC_PROTOCOL.md` §9.4、`SECURITY_DESIGN.md` §12.3）。`` ✅ |

指针目标独立核实：`docs/SECURITY_DESIGN.md:386` 确为 `### 12.3 Workspace`，其下 `:388-389` 正是「原始 workspace 路径只能由 Owner Node 本地管理入口选择 / 远程主体只能选择 Export 发布的 workspace alias/template」。而 `docs/SYNC_PROTOCOL.md:1387` 仍是 `### 12.3 WebSocket Close Code`（一张 close code 表），即 Round 1 指出的错位确实存在且已被正确绕开。

链接形态与本文件既有写法一致（`:1229`、`:1285`、`:1536` 同为指向 `SECURITY_DESIGN.md` 的相对链接）。`check-doc-links.mjs` 对该链接的存在性判定通过（见 §5）。

### F2 — `snapshot_begin` / `snapshot_end` 示例的 `chunkCount` 与 8 项资源列表对齐 → **已修，且采用了更准确的「取决于协商」写法**

- `docs/SYNC_PROTOCOL.md:736` `sync.snapshot_begin` 示例：`"chunkCount": 8` ✅
- `docs/SYNC_PROTOCOL.md:762` `sync.snapshot_end` 示例：`"chunkCount": 8,` ✅
- `docs/SYNC_PROTOCOL.md:768` 新增散文：`示例中的 8 是协商了 core.local-catalog.v1 时的一次完整快照（每种资源一个 chunk）；未协商该 feature 时目录资源 MUST NOT 发送，chunkCount 为 6。` ✅

核验准确性：`schemas/sync/v1/sync.schema.json:121` 与 `:256` 的 `chunkCount` 为 `{"type":"integer","minimum":0,"maximum":100000}`，8 与 6 都是合法值，示例不与 schema 冲突；`:774-783` 的资源列表确实是 8 项且与 `$defs.snapshotResource.enum` 同序（见 §3 的漂移门禁）。「8/6 取决于是否协商」的说法与 §9.4 的 feature 门控条款（`:802`「未协商该 feature 时服务端 MUST NOT 发送这两个资源，连空数组占位也不得发送」）自洽——正因为 MUST NOT 发送，未协商时才是 6。Round 1 建议的两个方案里选了后者，是正确的那个。

### F3 — §9.4 写明目录资源的授权面归属 → **已修**

`docs/SYNC_PROTOCOL.md:802` 段末新增：`workspaces/agents 的读取归在既有 session.list scope（pack.observe）之下，与会话列表同一屏；本次不新增 scope。` ✅

引用准确性独立核实（`compatibility/commands/v1/commands.json` 的 `session.list` 条目）：

```json
{ "name": "session.list", "kind": "query", "scope": "session.list",
  "pack": "pack.observe", "grant": "grant.observe",
  "transport": ["sync", "node_link"], "delivery": "mvp" }
```

`scope` 确为 `session.list`、`pack` 确为 `pack.observe`，且该命令本就在 `sync` 传输面上——文档所述与登记表逐字相符，不是编的。与 design.md D2 冻结的「不新增 scope」一致。

### F4 — 枚举漂移门禁 → **已修，且经变异实验证明非空转**

新增用例：`crates/sync-protocol/tests/schema_drift.rs:325-348`，`fn snapshot_resources_match_schema_enum`，导入在 `:18`（`use sync_protocol::sync::SnapshotResource;`）。

断言形式（`:343-347`）：

```rust
assert_eq!(
    SnapshotResource::ALL.map(|value| value.as_str()).to_vec(),
    declared,
    "$defs.snapshotResource.enum 与 SnapshotResource::ALL 不一致"
);
```

`declared` 由 `:336-341` 从 `sync.schema.json` 的 `$defs.snapshotResource.enum` 逐项取出。两侧都是 `Vec<&str>`，`assert_eq!` 走 `Vec` 的元素级按序比较，因此同时钉住**长度**与**顺序**。

实测运行：`cargo test -p sync-protocol --test schema_drift` 中 `snapshot_resources_match_schema_enum ... ok`。

**变异实验（worktree 外）**：把 worktree 的 `crates/{acpr-transcript,acpr-wire,sync-protocol}`、`schemas/`、`compatibility/`、`fixtures/` 复制到 `D:\Project\acp-remote-wt\scratch-wp2r2`，构成独立 workspace，然后只改 scratch 里的 `schemas/sync/v1/sync.schema.json`：

- 基线（未变异）：`test snapshot_resources_match_schema_enum ... ok`
- 变异 A（schema 单侧新增 `"projects"`）：**FAILED**，报错 `left: […8 项…]` / `right: […8 项…, "projects"]`
- 变异 B（纯重排，把 `capabilities` 与 `workspaces` 对调，集合完全相同）：**FAILED**，报错逐项列出两侧顺序差异

结论：该断言能抓住单侧新增，也能抓住纯重排；`server::sync` 消费 Rust 枚举、客户端校验 schema 的漂移风险已被机器判据覆盖。实验后 scratch 的 schema 已复原，worktree 全程 `git status --porcelain` 为空。

---

## 2. 回归核验（`da9e390` 不得改变任何 wire / schema / 判定）

`git diff --name-only ba2d1d6..da9e390` = 三个文件：

1. `crates/sync-protocol/src/common.rs` —— **仅一行文档注释**（`§12.3` → `SECURITY_DESIGN.md §12.3`），`git show` 的 hunk 上下文是 `#[derive(...)] #[serde(deny_unknown_fields)] pub struct WorkspaceRef` 之上的 `///`，无一行可执行代码变动。
2. `crates/sync-protocol/tests/schema_drift.rs` —— 只增一个测试函数与一条 `use`，纯增量，不改任何既有断言。
3. `docs/SYNC_PROTOCOL.md` —— 10 行改动，全部落在 §9.4（`:736`、`:762`、`:768`、`:802`）与 §10.3（`:1090`）。

因此：**无 wire 字节形状变化**（`common.rs`/`sync.rs` 的 serde 属性、枚举、方法体一字未动）、**无 schema 语义变化**（`schemas/` 两个文件在修复提交里根本没被触碰）、**无 fixture/manifest 变化**、**无依赖或 crate 增删**（`git diff 6c093f1..HEAD -- '*Cargo.toml' Cargo.lock` 为空）。

Round 1 的 PASS 判据逐条仍然成立，本轮独立复核了其中会被修复提交影响到的几条：

- 判据 4（8 项枚举 + 穷尽匹配）：新门禁 `snapshot_resources_match_schema_enum` 直接把 `SnapshotResource::ALL` 与 schema 枚举钉死，`body_constraints::catalog_snapshot_resources_enforce_their_item_shape` 仍 `ok`。
- 判据 8（夹具登记与计数）：`envelope_fixtures` 7 个用例全 `ok`，含 `every_manifest_case_behaves_as_declared` 与 `every_message_type_is_covered_by_a_valid_fixture`，计数不是死常量。
- 判据 7（server 侧唯一字面量）：`cargo clippy -p sync-protocol -p server --all-targets --all-features -- -D warnings` 退出码 0（见 §5），`server` crate 编译通过本身就是「`SessionSummary` 新增字段的每个构造点都已补齐」的机器证据。

---

## 3. Write Scope 纪律（`6c093f1..HEAD` 全量枚举，共 15 个文件）

| 文件 | 状态 | Scope 内 |
|---|---|---|
| `crates/server/src/node_link/command.rs` | M | ✅ |
| `crates/sync-protocol/src/common.rs` | M | ✅ |
| `crates/sync-protocol/src/sync.rs` | M | ✅ |
| `crates/sync-protocol/tests/body_constraints.rs` | M | ✅ |
| `crates/sync-protocol/tests/envelope_fixtures.rs` | M | ✅ |
| `crates/sync-protocol/tests/schema_drift.rs` | M | ✅ |
| `docs/SYNC_PROTOCOL.md` | M | ✅ |
| `fixtures/sync/v1/manifest.json` | M | ✅ |
| `fixtures/sync/v1/valid/sync-snapshot-chunk-agents.json` | A | ✅ |
| `fixtures/sync/v1/valid/sync-snapshot-chunk-sessions-workspace.json` | A | ✅ |
| `fixtures/sync/v1/valid/sync-snapshot-chunk-workspaces.json` | A | ✅ |
| `fixtures/sync/v1/invalid/snapshot-chunk-agent-item-missing-default.json` | A | ✅ |
| `fixtures/sync/v1/invalid/snapshot-chunk-workspace-item-has-path.json` | A | ✅ |
| `schemas/sync/v1/common.schema.json` | M | ✅ |
| `schemas/sync/v1/sync.schema.json` | M | ✅ |

全部落在声明的 Write Scope 内。**未触碰**：`crates/server/src/node_link/resource.rs`（Round 1 已证其无需改）、`Cargo.toml`、`Cargo.lock`、`openspec/`、`compatibility/`（`features.json`、`commands.json`）、任何其它 `crates/**`、任何其它 `docs/**`。

---

## 4. 规范化路径禁令（F4 之外的无回归项）

按 Round 1 的口径重跑全量扫描（`schemas/sync/v1/`、`fixtures/sync/v1/`、`crates/sync-protocol/`、`docs/SYNC_PROTOCOL.md`），命中仍是**同样的 4 处**，与 Round 1 完全一致，无新增：

| 位置 | 性质 |
|---|---|
| `fixtures/sync/v1/invalid/snapshot-chunk-workspace-item-has-path.json:15` | 负向夹具，`"canonicalPath": "C:/Users/dev/src/project"`——该夹具的存在目的就是证明这条被拒 |
| `crates/sync-protocol/tests/body_constraints.rs:168` | Rust 负向用例 |
| `crates/sync-protocol/tests/body_constraints.rs:126` | 测试文档注释，列举被禁字段名 |
| `docs/SYNC_PROTOCOL.md:802` | 散文把 `canonicalPath` 当**被禁止的字段名**引用 |

`schemas/sync/v1/**` 下 `canonicalPath|canonical_path|"X:[\/]` **零命中**；`crates/sync-protocol/src/**` 零命中。修复提交新增的文字只提到字段名本身，没有任何路径字面量进入正向 fixture 或 schema。

---

## 5. 五条命令的真实输出

**关于构建目录的一点必须说明**：首次执行时 `cwd` 未生效，命令落到了主仓库 `D:\Project\acp-remote`（输出里明确出现 `Compiling sync-protocol v0.0.0 (D:\Project\acp-remote\crates\sync-protocol)`），且 cargo 的 test 二进制 hash 在不同 target 目录间**完全相同**（`sync_protocol-5681b18904b6cd6e` 等），导致后续复跑被判为 `Fresh` 而继续吐出主仓库的产物（`body_constraints` 只有 9 个用例、`schema_drift` 只有 5 个）。为避免把主仓库的结果误当作本分支的结果，下面全部改用 `--manifest-path D:\Project\acp-remote-wt\wp2-wire\Cargo.toml` + **全新的** target 目录 `target-wp2-review2b` 重跑；输出中的 `Finished`/`Running` 行可自证 crate 来自 `wp2-wire`。

### 5.1 `cargo test --locked -p sync-protocol`

```
running 1 test
test common::tests::cursor_requires_both_keys_and_rejects_unknown_and_ill_typed ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 11 tests
test required_nullable_keys_are_enforced_on_nested_common_types ... ok
test catalog_snapshot_resources_enforce_their_item_shape ... ok
test session_summary_origin_block_is_discriminated_by_kind ... ok
test session_summary_workspace_keeps_absent_null_and_value_apart ... ok
test prompt_content_only_accepts_text_blocks ... ok
test snapshot_session_state_enum_is_enforced ... ok
test projection_rejects_view_that_violates_its_registered_shape ... ok
test snapshot_chunk_index_is_a_decimal_string_and_items_follow_the_resource ... ok
test command_name_and_payload_must_agree ... ok
test subscribe_cursor_key_is_required_but_may_be_null ... ok
test event_origin_and_remote_origin_follow_the_event_schema ... ok
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 7 tests
test connection_fields_must_appear_together ... ok
test envelope_preserves_body_bytes_verbatim ... ok
test envelope_rules_are_enforced_on_construction ... ok
test unknown_message_type_and_unknown_envelope_field_are_rejected ... ok
test phase_rules_match_the_connection_state_machine ... ok
test every_message_type_is_covered_by_a_valid_fixture ... ok
test every_manifest_case_behaves_as_declared ... ok
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

running 10 tests
test error_code_default_retryable_matches_documented_semantics ... ok
test control_ping_and_pong_bodies_round_trip ... ok
test base64url_fields_require_canonical_unpadded_encoding_of_the_declared_width ... ok
test error_body_requires_correlation_id_key_and_object_details ... ok
test uuid_and_decimal_string_patterns_are_enforced ... ok
test protocol_version_minimums_are_enforced ... ok
test limits_minimums_are_enforced ... ok
test feature_lists_reject_duplicates_overlong_and_bad_ids ... ok
test scopes_reject_duplicates_empty_and_overlong_entries ... ok
test heartbeat_interval_bounds_are_enforced ... ok
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 6 tests
test canonical_origin_requires_a_bare_https_authority ... ok
test claim_response_status_is_pinned_to_pending_confirmation ... ok
test qr_payload_constants_and_widths_are_enforced ... ok
test http_error_reuses_the_registered_error_codes ... ok
test status_response_device_and_host_follow_the_status ... ok
test every_pairing_fixture_parses_as_its_declared_shape ... ok
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 6 tests
test unknown_message_type_is_rejected ... ok
test error_codes_and_retryable_match_registry ... ok
test snapshot_resources_match_schema_enum ... ok          <-- 本轮新增
test view_enums_match_schema ... ok
test message_types_match_schema_union ... ok
test view_types_match_schema_defs ... ok
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

running 3 tests
test domain_lookup_covers_every_registered_domain ... ok
test tags_are_strictly_ascending ... ok
test domains_match_registry ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 2 tests
test transcript_negatives_are_rejected_with_declared_error ... ok
test positive_vectors_re_encode_byte_exactly ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 3 tests
test unregistered_event_type_is_rejected_by_projection ... ok
test view_keeps_unknown_fields_verbatim ... ok
test every_view_fixture_projects_onto_its_declared_event_type ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests sync_protocol
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

合计 **49 passed / 0 failed**（11 个 suite）。对比 Round 1 的 48，新增 1 个用例（`schema_drift` 由 5 → 6，正是 F4 的新门禁），其余不变；`body_constraints` 仍是 11。

### 5.2 `cargo clippy --locked -p sync-protocol -p server --all-targets --all-features -- -D warnings`

```
    Checking acpr-wire v0.0.0 (D:\Project\acp-remote-wt\wp2-wire\crates\acpr-wire)
    Checking sync-protocol v0.0.0 (D:\Project\acp-remote-wt\wp2-wire\crates\sync-protocol)
    Checking node-link-protocol v0.0.0 (D:\Project\acp-remote-wt\wp2-wire\crates\node-link-protocol)
    Checking core v0.0.0 (D:\Project\acp-remote-wt\wp2-wire\crates\core)
    Checking windows-local-ipc v0.0.0 (D:\Project\acp-remote-wt\wp2-wire\vendor\windows-local-ipc)
    Checking identity-auth v0.0.0 (D:\Project\acp-remote-wt\wp2-wire\crates\identity-auth)
    Checking server v0.0.0 (D:\Project\acp-remote-wt\wp2-wire\crates\server)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 14.32s
CLIPPY_EXIT=0
```

零 warning、零 error，退出码 0。`server` 通过 `--all-targets` 编译即证明 `command.rs:2478` 仍是 `SessionSummary` 的唯一构造点且已正确补 `workspace: None`。

### 5.3 `node scripts/check-schema-fixtures.mjs`

```
schema fixtures OK: 123 valid, 28 invalid (ajv Draft 2020-12), 39 event views bound
```

（与 Round 1 逐字一致——修复提交未改 schema 与夹具，符合预期。）

### 5.4 `node scripts/check-contract-assets.mjs`

```
contract assets OK: 17 schemas, 164 fixture files, 12 transcript vectors re-encoded from input, 20 negative vectors rejected as declared, 2 SAS values recomputed
```

（与 Round 1 逐字一致。）

### 5.5 `node scripts/check-doc-links.mjs`

```
error: reports/wp2-review.md:158: 链接目标不存在 -> ./SECURITY_DESIGN.md
doc link check failed: 1 problem(s)
```

**这不是 WP2 的缺陷，也不是修复提交引入的回归。** 定位过程：

1. 唯一报错指向 `reports/wp2-review.md:158` —— 那是 **Round 1 检视报告自身**的文件。Round 1 在「最小修复」里把指向 `SECURITY_DESIGN.md` 的写法写成了一个 Markdown 链接，被链接检查器当成真链接解析，而该相对路径从 `reports/` 出发不存在。
2. `reports/` 被 `.gitignore:32` 的 `/reports/` 忽略（`git check-ignore -v` 已确认），不在提交内容里。检查器的 `SKIP_DIRS`（`scripts/check-doc-links.mjs:33`）只跳 `.git/.husky/node_modules/target/dist`，不读 git，所以它会扫到这个本地残留文件。
3. 实证：把 HEAD 的**已跟踪内容**用 `git archive HEAD` 导出到 `D:\Project\acp-remote-wt\scratch-head`（其中没有 `reports/`），在那里跑同一个脚本：

```
doc links OK: 405 relative links, 8642 section refs across 485 markdown files
note: 4160 section refs 的归属文档由上下文决定（未在同一子句内指名文档），按设计未判定
```

退出码 0。**即 WP2 提交内容本身的文档链接是全绿的**，失败 100% 来自 gitignore 的本地报告残留。

**给 main 的处理建议（I3）**：合入前清掉 worktree 的 `reports/` 目录（或让 `check-doc-links.mjs` 的 `SKIP_DIRS` 加上 `reports`，但那是脚本改动，超出 WP2 scope，应由 main 决定）。本报告刻意不使用 Markdown 链接语法，以免再触发同一条判据。

---

## 6. F5 的当前状态（`core.local-catalog.v1`）与 WP2 分支的自洽性

**当前状态（在本 worktree 内）**：`core.local-catalog.v1` 被 `docs/SYNC_PROTOCOL.md` 的 `:768`、`:791`、`:797`、`:798`、`:802`、`:1088` 共 6 处引用，而 `compatibility/features/v1/features.json` 在本分支上 `grep` 命中数为 **0**，`SYNC_PROTOCOL.md` §5.2 的 feature 表（`:185` 附近）也只有 `resource.remote-origin.v1` 一条目录类 feature。**F5 的观察在 WP2 分支上原样成立**，仍是「文档引用了一个机器可读登记表里没有的 ID」。

**WP1 侧的落地情况**（只读核对 `git show ceedba8`，未合入本 worktree）：`compatibility/features/v1/features.json` 新增了 `"id": "core.local-catalog.v1"`；`docs/SYNC_PROTOCOL.md` §5.2 同步新增了 `| core.local-catalog.v1 | 本机 workspace 目录与 Agent profile 随快照下发（§9.4 的 workspaces/agents 资源、§10.3 的 workspaceRef/agentCatalogEntry）… | mvp | 否 |`。design.md D7 要求的三原子点里，**登记表与 §5.2 两处已由 WP1 补齐**。

**合并态实测**（在 worktree 之外的 `scratch-merge` 克隆里把 WP2 与 WP1 试合并）：

- `check-features.mjs` → `feature registry OK: 13 feature ids across 2 protocols`（合并前 11）
- `check-schema-fixtures.mjs` → `schema fixtures OK: 127 valid, 30 invalid (ajv Draft 2020-12), 39 event views bound`
- `check-contract-assets.mjs` → `contract assets OK: 17 schemas, 170 fixture files, …`
- `check-doc-links.mjs` → `doc links OK: 409 relative links, 8678 section refs across 485 markdown files`

即：**F5 在集成后消失，且合并后的三条 registry 门禁全绿**。

**WP2 是否在自己分支上自洽**：**除 F5 外自洽**。schema、fixture、Rust DTO、文档四者在本分支内部完全对齐，`snapshot_resources_match_schema_enum` 现在把这份对齐变成了机器判据。F5 是纯粹的前向依赖（WP2 引用 WP1 拥有的词表条目），design.md D7 也把它列为跨包契约原子点，因此**不构成本分支的缺陷**，但它意味着**在 WP1 合入之前，WP2 分支的文档引用无法被任何门禁验证**（`check-features.mjs` 在本分支输出 11 条，仍是绿的）。

---

## 7. 集成期提示（非 WP2 缺陷，交给 main）

### I1 — WP1 与 WP2 在 `crates/sync-protocol/tests/envelope_fixtures.rs` 的计数常量上冲突

试合并时 git 报 `CONFLICT (content)`，唯一冲突块是 `EXPECTED_VALID_MESSAGE_CASES`：WP2 写 63、WP1 写 64。合并值应为 **67**（基线 60 + WP2 的 3 + WP1 的 4）。

### I2 — 更隐蔽的一处：`EXPECTED_BODY_REJECTED` 两边都写成 7，git 自动合并没有报冲突，但值是错的

WP1 把 `EXPECTED_BODY_REJECTED` 从 5 改成 7，WP2 也从 5 改成 7 —— 两边**结果相同**，git 视为无冲突自动合并，于是合并后仍是 7，而实际应是 **9**（基线 5 + WP2 的 2 + WP1 的 2）。实测证据：合并态（valid 计数已改为 67）下

```
---- every_manifest_case_behaves_as_declared stdout ----
assertion `left == right` failed: body 层拒绝的负例数变化
  left: 9
 right: 7
test result: FAILED. 6 passed; 1 failed
```

把该常量改为 9 后，合并态 `cargo test -p sync-protocol` 全绿（合并后各 suite 合计 52 passed / 0 failed，其中 sync-protocol 单测 4、body_constraints 11、envelope_fixtures 7、field_constraints 10、pairing 6、schema_drift 6、tables 3、transcript 2、view_projections 3），`every_manifest_case_behaves_as_declared` 恢复 `ok`。`check-schema-fixtures.mjs` 在合并态已报 `30 invalid`（两分支各 28），可作为该计数的独立佐证。

**建议**：main 合入时同时核对 `EXPECTED_VALID_MESSAGE_CASES = 67` 与 `EXPECTED_BODY_REJECTED = 9`，不要只解 git 报出来的那个冲突。

### I3 — 见 §5.5，合入前清理 worktree 的 `reports/` 残留，否则 `check-doc-links.mjs` 会红

---

## 8. 本轮实际做了什么（可复核清单）

| 检查项 | 手段 |
|---|---|
| F1 三处引用 | 逐处 `read` + 目标章节定位（`SECURITY_DESIGN.md:386`、`SYNC_PROTOCOL.md:1387` 对照） |
| F2 示例与散文 | `git diff` 读全文 + `sync.schema.json:121/:256` 的 `chunkCount` 约束对照 |
| F3 授权面 | `SYNC_PROTOCOL.md:802` 逐字 + `commands.json` 的 `session.list` 条目程序化核对 |
| F4 门禁有效性 | 读断言形式 + **worktree 外 scratch 副本的两次变异实验**（单侧新增 / 纯重排，均 FAILED） |
| 无回归 | `git show da9e390`（3 文件）、`git diff --name-status 6c093f1..HEAD`（15 文件）、Cargo.toml/lock 零变动 |
| 路径禁令无回归 | 对四个 scope 目录重跑 Round 1 同口径扫描，命中集完全一致（4 处） |
| 五条门禁 | 全程 `--manifest-path` 指向 worktree + 全新 target 目录，避免缓存串味 |
| F5 | 本分支 features.json 计数 + `git show ceedba8` 核对 WP1 + worktree 外克隆的合并态实测 |

worktree 终态：`git rev-parse HEAD` = `da9e39040e02a6d5d75407d27acf3bbcbb479ca6`，`git status --porcelain` 空，`git diff HEAD` 空。**未修改任何源码、schema、fixture、文档或配置**，唯一写入是本报告文件。