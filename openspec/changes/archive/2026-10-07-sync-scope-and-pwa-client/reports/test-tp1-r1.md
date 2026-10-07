# TP1 合同测试向量交付报告（test-tp1-r1）

## Shared Report

- **task_id**: 4.4（design）/ 4.5（design-author）
- **work_package**: TP1
- **role**: tester（`testing-1`）
- **phase**: design-author
- **agent_context**: `agent_id: TesterTP1`，`isolation: fork_turns=none`（未继承 WP1/WP2 或任何产品实现者的对话；实现只以只读方式查阅 `.worktrees/wp1` / `.worktrees/wp2` 的未提交工作区）
- **target_revision**: `353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f`（`.worktrees/tp1` 的 HEAD；本包无提交，全部产出为未暂存的工作区增量）
- **scope**: `crates/*/tests/` 与 `fixtures/sync/v1/` 的**新增**用例；不触碰产品代码、`docs/`、`schemas/`，不改写 WP1/WP2 的任何既有文件
- **result**: **PASS（编写交付）**；交付目标版本上的完整门禁**未执行**，按 main 指示留到 MU1a 合入后的固定提交上跑

### changes

交付物全部在 `.worktrees/tp1`（分支 `feat/tp1-contract-vectors`，基线 `353ba6e`），`git status --porcelain` 全量如下（首列均为空格＝**未暂存**，全程未执行 `git add`/`commit`/`push`）：

```
 M crates/sync-protocol/tests/schema_drift.rs
 M fixtures/sync/v1/manifest.json
?? crates/sync-protocol/tests/snapshot_scope_vectors.rs
?? fixtures/sync/v1/invalid/command-result-session-read-result-without-has-earlier.json
?? fixtures/sync/v1/invalid/command-session-read-before-missing-message-id.json
?? fixtures/sync/v1/invalid/snapshot-chunk-detail-resource-capabilities.json
?? fixtures/sync/v1/invalid/snapshot-chunk-detail-resource-pending-interactions.json
?? fixtures/sync/v1/invalid/snapshot-chunk-detail-resource-turns.json
?? fixtures/sync/v1/valid/command-result-session-read-earliest-page-completed.json
?? fixtures/sync/v1/valid/command-result-session-read-first-page-completed.json
?? fixtures/sync/v1/valid/command-session-read-default-page-include-only.json
?? fixtures/sync/v1/valid/command-session-read-second-page.json
```

#### 1. 关闭已知缺口：`schema_drift.rs` 的枚举覆盖

`crates/sync-protocol/tests/schema_drift.rs:323-337`（在既有 `view_enums_match_schema` 内，`TerminalStream` 断言之后）新增两条与既有四条同源的 `ALL`/`as_str` ↔ `VIEW_ENUMS` 登记 ↔ schema `enum` 三向断言：

| 断言 | 位置 | 登记条目 |
| --- | --- | --- |
| `views::AgentConnectedState::ALL.map(as_str)` | `schema_drift.rs:326-331` | `agent.connected` + `/properties/state` |
| `views::AgentDisconnectedState::ALL.map(as_str)` | `schema_drift.rs:332-337` | `agent.disconnected` + `/properties/state` |

补齐前，这四个封闭 enum 之外的 `state` 没有任何 `ALL`/`as_str` 镜像断言：只在一侧新增取值时全部门禁仍绿。现在两侧都动不了。

#### 2. 五类禁止入快照资源的负例（补 reviewer 标记的三条）

新增 fixture（每条 `items` 都是**非空且逐字段合法**的元素数组，因此唯一的失败原因就是 `resource` 不在封闭词表内）：

| fixture | `resource` | manifest 声明 |
| --- | --- | --- |
| `fixtures/sync/v1/invalid/snapshot-chunk-detail-resource-turns.json` | `turns` | `valid:false` + `expectedKeyword:"enum"` |
| `fixtures/sync/v1/invalid/snapshot-chunk-detail-resource-pending-interactions.json` | `pending_interactions` | 同上 |
| `fixtures/sync/v1/invalid/snapshot-chunk-detail-resource-capabilities.json` | `capabilities` | 同上 |

`messages` 与 `config_options` 的负例由 WP1 交付，本包在 `snapshot_scope_vectors.rs:171` 的判据表里把五条**并列**纳入同一断言，使 WP1 的两条与本包的三条受同一判据约束——否则那两条会随 WP1 的文件独立漂移。

#### 3. 分页形状的四条向量 + 两条负例

| 向量 | fixture / 断言位置 | 钉住的合同点 |
| --- | --- | --- |
| 默认页请求（无 `before`/`limit`） | `valid/command-session-read-default-page-include-only.json`；断言 `snapshot_scope_vectors.rs:219` | 「同步面未提供参数时使用默认页」：两个分页键**不存在**（不是 `null`、不是缺省值） |
| 复合游标分页请求 | `valid/command-session-read-second-page.json`；断言 `:244` | `before` 同时携带 `createdAt`+`messageId`，`limit` 是请求上限 |
| `hasEarlier: true` 结果 | `valid/command-result-session-read-first-page-completed.json`；断言 `:299` | 还有更早内容 |
| `hasEarlier: false` 结果 | `valid/command-result-session-read-earliest-page-completed.json`；断言 `:299` | 到最早一条，且**不返回空占位**（`messages` 非空且属于被读会话） |
| 单字段游标负例（缺 `messageId`） | `invalid/command-session-read-before-missing-message-id.json`；断言 `:269` | 「游标不可由标识前缀构造」；`expectedKeyword:"required"` |
| 缺 `hasEarlier` 的结果负例 | `invalid/command-result-session-read-result-without-has-earlier.json`；断言 `:299` | 键缺失不得被读成「没有更早内容」；`expectedKeyword:"required"` |

另加跨向量断言 `adjacent_pages_join_without_gaps_or_overlap`（`:349`）：游标**就是**上一页最早一条的 `(createdAt, messageId)`；两页各自按 `createdAt` 升序、并集无重复、第二页整体早于游标时间、游标指向的消息不再出现；并显式断言同一 `createdAt` 的两条消息的次序**不等于** `messageId` 字典序（「不按标识排序」）。

WP1 已交付同场景的 `command-session-read-{default-page,paged}.json`、`command-result-session-read-{paged,last-page}-completed.json`、`command-session-read-single-field-before.json`、`command-result-session-read-missing-has-earlier.json`。本包**没有复制**它们：本包的默认页/分页/缺 `hasEarlier` 向量另取数据形态（两页共 6 条消息构成可拼接的一对），单字段游标取 WP1 未覆盖的**另一半**（只给 `createdAt`），缺 `hasEarlier` 的负例刻意让 `messages` **非空**（WP1 那条是空数组），因此被拒的原因只能是缺 `hasEarlier`。

#### 4. 扩展的既有门禁

| 文件 | 改动 | 性质 |
| --- | --- | --- |
| `crates/sync-protocol/tests/schema_drift.rs:323-337` | 追加两条 `ALL`/`as_str` 断言 | 扩展既有 `view_enums_match_schema` |
| `fixtures/sync/v1/manifest.json:502-551` | 在 `cases` 数组**末尾追加** 9 条 | 只追加，未删除或改写任何既有条目 |
| `crates/sync-protocol/tests/envelope_fixtures.rs` | **未改** | 计数常量由 main 在候选组装时统一设定，见下 |

#### 5. 附带钉住：`分页只作用于客户端同步面`

`node_link_session_read_payload_carries_no_paging_parameters`（`:444`）遍历 `schemas/node-link/v1/command.schema.json`，断言每个声明 `include` 的载荷对象键集恰好是 `{ include }` 且 `additionalProperties: false`。Node Link 调用方不带分页参数照常工作、一旦带上即被拒，两条同时被钉住。该用例带防空转守卫（找不到目标载荷即失败）。

**归属说明**：任务 4.5 的原文还列了 `file.changed` 新字段向量（`addedLines`/`deletedLines`/`outsideWorkspace`）。这一项**不在 main 下发给本包的范围内**，本包未做，留给 main 决定是否另派（WP2 已在 `valid/view-file-changed.json` 带上三个字段，但缺「键缺失即合法、`null` 被拒」这条负向断言）。

### checks

自检**不在交付 worktree 上执行**。原因：冻结合同只存在于 WP1/WP2 两个**未提交**工作区，基线 `353ba6e` 上 `AgentConnectedState`/`SessionReadBefore`/`hasEarlier`/收窄后的 `SnapshotResource` 全部不存在；在基线上跑必然是「符号不存在」的噪音红。main 已就此给出两个方案并选择「MU1a 先单独合入，TP1 从 MU1a 合入提交分支起」。

因此本包采用的做法是：**在一次性临时 worktree `.worktrees/tp1-verify`（基线 `353ba6e` 叠加 WP1+WP2 的工作区）上编写并运行，跑完即删**。交付 worktree `.worktrees/tp1` 全程只含本包增量，两者都不是提交。MU1a 候选组装后，main 在候选提交上重跑完整门禁，那一次才是权威证据。

临时 worktree 已删除（`git worktree list` 现为 main / mu1-merge / tp1 / wp1 / wp2，无 tp1-verify）。

#### BT1 — 本包向量（临时 worktree，含 WP1+WP2 叠加）

```
$ cargo test --locked -p sync-protocol --all-features --test snapshot_scope_vectors
running 8 tests
test snapshot_resource_enum_excludes_every_session_detail_resource ... ok
test single_field_cursor_is_rejected ... ok
test paged_request_carries_the_composite_cursor ... ok
test default_page_request_omits_both_paging_parameters ... ok
test session_read_result_declares_whether_earlier_content_exists ... ok
test adjacent_pages_join_without_gaps_or_overlap ... ok
test node_link_session_read_payload_carries_no_paging_parameters ... ok
test every_forbidden_detail_resource_has_a_registered_negative_chunk_fixture ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

#### BT2 — schema 层（ajv，`scripts/check-schema-fixtures.mjs`）

```
$ node scripts/check-schema-fixtures.mjs
schema fixtures OK: 137 valid, 40 invalid (ajv Draft 2020-12), 39 event views bound
ajv_exit=0
```

#### BT3 — 整包（临时 worktree，计数常量临时置为 77/19）

```
$ cargo test --locked -p sync-protocol --all-features --no-fail-fast
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests\body_constraints.rs
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests\envelope_fixtures.rs
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
     Running tests\field_constraints.rs
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests\pairing_fixtures.rs
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests\schema_drift.rs
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
     Running tests\snapshot_scope_vectors.rs
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests\tables_match_registry.rs
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests\transcript_vectors.rs
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests\view_projections.rs
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
exit=0
```

计数常量的**实测值**（供 main 在候选组装时设定；本包未写入 `envelope_fixtures.rs`）：

```
assertion `left == right` failed: 合法消息用例数变化
  left: 77
 right: 73
...
  left: 19
 right: 14
```

即 WP1 的 `73`/`14` 之后应为 **`EXPECTED_VALID_MESSAGE_CASES = 77`**（+4 本包合法向量）与 **`EXPECTED_BODY_REJECTED = 19`**（+5 本包 body 层负例：3 条 chunk + 1 条游标 + 1 条缺 `hasEarlier`）。

#### BT4 — 变异检验（每条向量都因它声明的原因而红）

| 变异 | 结果 |
| --- | --- |
| `sync::SnapshotResource::from_str` 对未知取值放行 | `snapshot_resource_enum_excludes_every_session_detail_resource ... FAILED`：`messages（invalid/snapshot-chunk-detail-resource-messages.json）：会话明细资源不得是合法快照资源，却解析成 sessions`；`every_forbidden_detail_resource_has_a_registered_negative_chunk_fixture ... FAILED`：`invalid/snapshot-chunk-detail-resource-messages.json：五类会话明细资源的 chunk 必须在类型化层被拒`；`test result: FAILED. 5 passed; 2 failed` |
| `sync.schema.json` 的 `$defs.snapshotResource.enum` 加回 `turns` | `invalid fixture accepted: fixtures\sync\v1\invalid\snapshot-chunk-detail-resource-turns.json`，`ajv_exit=1` |
| `node-link/v1/command.schema.json` 的 `session.read` 载荷加 `before` | `node_link_session_read_payload_carries_no_paging_parameters ... FAILED`：`assertion left == right failed: Node Link 面的 session.read 载荷只允许 include，不得出现分页参数`；`test result: FAILED. 7 passed; 1 failed` |

第二行是本包对 reviewer 那条 MINOR 的正面回答：`turns` 的负例**确实**因枚举而非法，把枚举改宽它立刻被 ajv 判为「invalid fixture accepted」，枚举收窄被真正钉住。

### issues

- **test-tp1-r1-I1（BLOCKED，外部依赖）**：`npm run check` 与 `cargo test --locked -p sync-protocol --all-features` **未在交付 worktree `.worktrees/tp1` 上执行**。基线 `353ba6e` 不含 WP1/WP2 的实现，本包的 `schema_drift.rs` 断言与 `snapshot_scope_vectors.rs` 在其上无法编译；三条 chunk 负例在旧的 `sync.schema.json` 下反而是**合法**样例。解除条件：MU1a 合入后由 main 在固定提交上重跑两个门禁，并把 `EXPECTED_VALID_MESSAGE_CASES=77` / `EXPECTED_BODY_REJECTED=19` 写入候选。本包不因此声称任何门禁 PASS。
- **test-tp1-r1-I2（交接事项，非本包范围）**：任务 4.5 列出的 `file.changed` 新字段向量未做（见「归属说明」），需 main 决定是否另派。
- 无 FAIL：BT1–BT3 在临时 worktree 上全部零退出码通过，未改写任何既有向量使其通过。

### evidence_paths

- 交付 worktree：`.worktrees/tp1`（分支 `feat/tp1-contract-vectors`，HEAD `353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f`）
- 本报告：`openspec/changes/sync-scope-and-pwa-client/reports/test-tp1-r1.md`
- 临时验证 worktree `.worktrees/tp1-verify`：**已删除**；其运行输出即本报告 BT1–BT4 的 verbatim 段落
- `CARGO_TARGET_DIR`：`D:\Project\acp-remote\.target-wt\tp1`（与主 checkout 隔离）

### resource_cleanup

- `.worktrees/tp1-verify`：已 `rmdir` 解除 `node_modules` junction 后删除并 `git worktree prune`，无残留。
- `.worktrees/tp1` 内为本包建立的 `node_modules` junction（指向主 checkout 的 `node_modules`，gitignored，未暂存）保留，便于后续在本 worktree 跑 `npm run check`。
- 未启动任何长驻进程，未占用共享资源，未改动主 checkout 的任何受版本控制文件。

### test_delivery

```yaml
test_delivery:
  TP1:
    kind: automated
    artifacts:
      - crates/sync-protocol/tests/snapshot_scope_vectors.rs
      - crates/sync-protocol/tests/schema_drift.rs
      - fixtures/sync/v1/manifest.json
      - fixtures/sync/v1/valid/command-session-read-default-page-include-only.json
      - fixtures/sync/v1/valid/command-session-read-second-page.json
      - fixtures/sync/v1/valid/command-result-session-read-first-page-completed.json
      - fixtures/sync/v1/valid/command-result-session-read-earliest-page-completed.json
      - fixtures/sync/v1/invalid/command-session-read-before-missing-message-id.json
      - fixtures/sync/v1/invalid/command-result-session-read-result-without-has-earlier.json
      - fixtures/sync/v1/invalid/snapshot-chunk-detail-resource-turns.json
      - fixtures/sync/v1/invalid/snapshot-chunk-detail-resource-pending-interactions.json
      - fixtures/sync/v1/invalid/snapshot-chunk-detail-resource-capabilities.json
    basic_checks: [BT1, BT2, BT3, BT4]
```

### handoff_index

```agentic-handoff
version: 1
agent_context:
  agent_id: "TesterTP1"
  isolation: "fork_turns=none"
handoff_index:
  - task_id: "4.5"
    work_package: TP1
    role: tester
    phase: design-author
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f"
    evidence_type: DELIVERY
    evidence_id: test-tp1-delivery-r1
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/test-tp1-r1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "本包产出为 .worktrees/tp1 上的未暂存增量（10 个新增 fixture/测试文件 + 2 个文件的追加式改动），基线与该 worktree 的 HEAD 一致；写入范围限于 crates/*/tests/ 与 fixtures/sync/v1/ 的新增用例。DELIVERY 只声明「可运行用例/脚本齐备」，不代表任何门禁在该提交上通过。"
    source_evidence: NOT_APPLICABLE
  - task_id: "4.5"
    work_package: TP1
    role: tester
    phase: design-author
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f"
    evidence_type: CHECK
    evidence_id: BT1
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/test-tp1-r1.md"
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "BT1/BT2/BT3/BT4 已在一次性临时 worktree（基线 353ba6e 叠加 WP1+WP2 未提交实现）上执行且零退出码，但该环境既不是交付 worktree 也不是任何提交，不能顶替交付目标的门禁。交付目标上的同两条命令尚未执行；待 MU1a 合入后由 main 在固定提交上重跑。缺失门禁：npm run check、cargo test --locked -p sync-protocol --all-features（须同时把 EXPECTED_VALID_MESSAGE_CASES 设为 77、EXPECTED_BODY_REJECTED 设为 19）。"
    source_evidence: NOT_APPLICABLE
```

---

## 交接要点（给 main）

1. 候选组装时本包取这 12 个路径：`.worktrees/tp1/crates/sync-protocol/tests/snapshot_scope_vectors.rs`、`.worktrees/tp1/crates/sync-protocol/tests/schema_drift.rs`（**只取 323-337 行的新增块**，该文件其余部分与基线逐字节相同）、`.worktrees/tp1/fixtures/sync/v1/manifest.json`（**只取 502-551 行的 9 条追加**，与 WP1 对同一文件的改动位置不重叠：WP1 改在 30 行附近，本包追加在 `cases` 末尾）、以及 9 个新增 fixture。
2. `envelope_fixtures.rs` 的两个计数常量本包**未改**，实测目标值 **77 / 19**。
3. 本包没有触碰 `docs/SYNC_PROTOCOL.md`，也没有发现其中任何描述与本包向量矛盾——无需归属裁决。
