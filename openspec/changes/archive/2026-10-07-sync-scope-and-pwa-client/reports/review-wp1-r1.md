<!-- WP1 独立代码检视报告（review-wp1-r1，Round 1）。Reviewer 只报告，不修改代码、测试、规划文件、任务状态或 verification.md。 -->

task_id: "2.1"
role: reviewer
phase: branch
agent_context:
  agent_id: "review-1"
  isolation: "fork_turns=none（新建独立子 Agent；未参与 WP1 实现，也不继承任何实现或实现讨论）"
target_revision: "6779c36678376f02b95a4071b62dca809b221a8b"
scope: "WP1 交付提交 6779c36678376f02b95a4071b62dca809b221a8b 相对基线 353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f 的完整 diff（23 files / +585 / −209）。逐条核对 specs/sync-snapshot-scope/spec.md 的 R1–R4 与 design.md D1/D2/D3，检查 schema↔Rust 镜像同形性、夹具/manifest 完整性与排序、docs §9.4/§11.5 口径、写入范围、以及 R4 的 node-link 零改动。含 b964ae3（主分支带入的构建配置修复，非 WP1 范围，仅确认其不属本包）。不判分页的服务端语义实现（server::sync 未落地），不判用例集合是否齐备（交主 Agent Coverage Index / validator），不执行 E2E。"
changes: "只读检视，未修改任何文件；仅新增本报告。未切换分支、未提交、未合并。"
checks:
  - id: "PV1（WP1 工作包，主 Agent 复跑）"
    command: "CARGO_TARGET_DIR=.target-wt/wp1 npm run verify（cwd=.worktrees/wp1）"
    result: "exit 0。日志 .target-wt/wp1-verify.log：npm run check 十道脚本（check:schemas 133 valid / 35 invalid 等）+ check:rust（cargo fmt --check、clippy -D warnings、cargo test --locked --workspace --all-features）全 PASS。本报告只读取该日志作为证据，未自行复跑。"
  - id: "REVIEW/diff 绑定核对（本轮）"
    command: "git -C .worktrees/wp1 rev-parse 6779c36 / git diff --name-only/--numstat 353ba6e..6779c36"
    result: "路径与 SHA 核实：目标提交存在于 feat/wp1-sync-snapshot-contract；23 文件的 numstat 与主 Agent 交付事实一致（+585/−209）。"
  - id: "REVIEW/manifest 计数自洽（本轮，只读 python）"
    command: "python 解析 fixtures/sync/v1/manifest.json 与 git show 353ba6e:…/manifest.json"
    result: "基线 83 用例（72 valid / 11 invalid）；目标 94（78 valid / 16 invalid）。valid message-schema 用例 78−5 pairing=73，body 层拒绝负例 16−2 信封层=14，与 envelope_fixtures.rs 的 EXPECTED_VALID_MESSAGE_CASES=73 / EXPECTED_BODY_REJECTED=14 逐条吻合；增删夹具集合与 WP1 diff 完全一致。"
issues: "0 CRITICAL / 0 MAJOR；1 MINOR / 1 SUGGESTION。另 1 项基线既存的镜像不一致，按「不判既有缺陷」仅作范围外观察并回答作者问题，不计入本轮结论。"
result: PASS
evidence_paths:
  - "openspec/changes/sync-scope-and-pwa-client/reports/review-wp1-r1.md"
  - ".target-wt/wp1-verify.log"
resource_cleanup: NOT_APPLICABLE（只读检视，未创建资源）

```agentic-handoff
version: 1
agent_context:
  agent_id: "review-1"
  isolation: "fork_turns=none"
handoff_index:
  - task_id: "2.1"
    work_package: WP1
    role: reviewer
    phase: branch
    round: 1
    stage: work-package
    target_revision: "6779c36678376f02b95a4071b62dca809b221a8b"
    evidence_type: REVIEW
    evidence_id: review-wp1-r1
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/review-wp1-r1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "新建独立子 Agent（未参与 WP1 实现）对固定交付提交 6779c366（相对基线 353ba6ef）做只读检视；逐条核对 R1–R4 与 D1/D2/D3、schema↔Rust 镜像、夹具/manifest、docs §9.4/§11.5、写入范围与 node-link 零改动；PV1 证据读自主 Agent 复跑日志 .target-wt/wp1-verify.log（exit 0），未自行复跑。"
    source_evidence: NOT_APPLICABLE
```

## Review Context

| 项 | 取值 |
| --- | --- |
| Review ID / Round | `review-wp1-r1` / 1 |
| Review Type | `branch` |
| Review Stage | 工作包交付前检视（实现已交付、PV1 已 PASS、合入 MU1a 候选之前） |
| Work Package | WP1（MU1a 单元成员，R1–R4 的机器合同与文档） |
| Repository / 检视工作区 | `D:\Project\acp-remote` / `D:\Project\acp-remote\.worktrees\wp1`（只读） |
| Base Revision | `353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f` |
| Target Revision | `6779c36678376f02b95a4071b62dca809b221a8b`（分支 `feat/wp1-sync-snapshot-contract`） |
| 读取的规则 | `AGENTS.md`（§3 不变量、§4 依赖方向、§10 文档映射）、`openspec/schemas/agentic/roles/reviewer.md`、`roles/_shared/role-report.md` |
| 读取的需求 | `specs/sync-snapshot-scope/spec.md`（4 Requirement / 13 Scenario）、`proposal.md`、`design.md` D1/D2/D3（并对照 D4–D8 确认无越界）、`plan.md`（WP1 行、Shared File Ownership、Contract Changes） |
| 使用的验证证据 | `.target-wt/wp1-verify.log`（`npm run verify` exit 0）；作者 `reports/deliver-wp1-r1.md` 仅作待核实的自评读取，不作为检视证据 |
| 限制 | 服务端 `server::sync` 未落地，本轮只判 wire 合同与其 Rust 镜像，不判分页的服务端语义实现；不判用例集合齐备性；不执行 E2E；未逐条核对 `sync` 之外 18 个消息类型的全部字段约束（不在本 WP 范围） |
| 本轮报告路径 | `openspec/changes/sync-scope-and-pwa-client/reports/review-wp1-r1.md` |
| 核对的 Check ID | `PV1`（WP1 工作包复跑，`npm run verify`） |

### 逐证据核对

- `PV1`：目标差异 = 交付提交 `6779c366` 在工作树内 `npm run verify` 全量；适用依据 = 该提交 + `b964ae3` 的构建配置修复使 worktree 内 `cargo fmt` 可执行；结论 = exit 0，`check:schemas` 133 valid / 35 invalid，`check:rust` 三条全绿。本轮**未**自行复跑（交调度者 Project Verify），只读日志。
- `b964ae3`：主分支带入的空 `[workspace]` 表修复，仅使 worktree 内 `cargo fmt --all` 可执行，不属 WP1 实现范围——已在写入范围核对中排除，不计越界。

## Findings

| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation | Recheck |
| --- | --- | --- | --- | --- | --- | --- |
| `review-wp1-r1-F1` | MINOR | `crates/sync-protocol/src/sync.rs`（WP1 删除 `SnapshotItemConfigOption` 的 hunk，diff 中 `-pub struct SnapshotItemConfigOption { … }`）；相关 schema 仍活在 `schemas/sync/v1/sync.schema.json:$defs.snapshotItem.config_options` 与 `schemas/sync/v1/command.schema.json:$defs.sessionReadResult.resources.config_options`；文档断言在 `docs/SYNC_PROTOCOL.md:795` 与 §9.4 新增段落 | WP1 删除了 `SnapshotItemConfigOption`——它是 `sync.schema.json` 中**仍然存在**的 `snapshotItem.config_options`（`{sessionId, configOptions, version}` 归组形状）唯一的 Rust 镜像；删除后该 `$defs` 在 Rust 侧无任何同形类型，而 WP1 同批编辑的 §9.4 又新增「五类元素形状由 `session.read` 的结果直接复用（`sessionReadResult.resources` 引用同一批 `snapshotItem.*` 定义）」的表述。`SessionReadResources.config_options` 仍是 `Option<Vec<ConfigOptionView>>`（扁平元素形状，`crates/sync-protocol/src/command.rs`），与 schema 的归组形状不同形。 | 属本 WP 交付目标「Rust 类型化镜像与 schema 严格同形」的镜像缺口：`session.read` 结果一旦携带 `config_options`，符合 schema 的服务端输出会被 Rust 类型化层拒绝，Rust 序列化的扁平输出则不合 schema。注意：该 wire 不匹配在基线提交上即已存在（`git show 353ba6e:…/command.rs` 同行亦为 `Vec<ConfigOptionView>`），WP1 只改动了 `SessionReadResult`（新增 `has_earlier`）而未触及 `SessionReadResources` 字段，故运行行为的偏差非本轮引入；本轮引入的是「删掉唯一同形镜像 + 文档宣称已复用同一批定义」这两项。 | 二选一：(a) 恢复/新增一个 Rust 类型镜像 `snapshotItem.config_options` 的归组形状并用于 `SessionReadResources.config_options`；(b) 若认定 `config_options` 在 `session.read` 上也应为扁平形状，则同步 schema（把 `sessionReadResult.resources.config_options` 的 `items` 从 `snapshotItem.config_options` 改指 `common.configOptionView`）并调整 §9.4 的表述。任一方向都须让 `schema_drift` 之外再多一条针对该字段的形状断言，避免再次静默漂移。 | 不适用 |
| `review-wp1-r1-F2` | SUGGESTION | `fixtures/sync/v1/manifest.json`（新增负例条目） | D1 禁止进快照的是五类资源，WP1 只新增了 `invalid/snapshot-chunk-detail-resource-messages.json` 与 `-config-options.json` 两条负例；`turns`、`pending_interactions`、`capabilities` 三类在夹具层无机器断言（`crates/sync-protocol/src/sync.rs` 的 `snapshot_chunk_carries_only_catalog_resources` 单元测试已把五类都断言）。 | 词表是单一 enum，三类未覆盖不产生实际漏洞；但夹具门禁不会拦住「有人把某类临时加回 `if/then` 而忘了同步 enum」的回归。 | 视成本补 3 条负例；不阻塞交付。 | 不适用 |

## 契约符合性逐条核对

### R1 快照只承载清单类资源 — 符合

- `schemas/sync/v1/sync.schema.json`：`$defs.snapshotResource` 收为 `["sessions","workspaces","agents"]`；`snapshotChunk` 中 messages/turns/pending_interactions/config_options/capabilities 五条 `if/then` 整段删除，只留 sessions/workspaces/agents 三条，无悬挂 `$ref`。
- 新增 `$defs.snapshotChunkCount`（`integer`、`minimum: 1`、`maximum: 3`），被 `snapshotBegin` 与 `snapshotEnd` 共同 `$ref`——begin/end 不可能各写一套口径，满足「chunkCount MUST 与实际下发的资源数量一致」。
- Rust 镜像：`SnapshotResource` 去五变体、`ALL` 缩为 3 且**同序**；`SnapshotItems` 同步去五变体，`parse`/`resource`/`len`/`Serialize` 四处分派同步收窄且 match 仍穷尽；`SnapshotBegin/SnapshotEnd.chunk_count: BoundedU64<1,3>`。`tests/schema_drift.rs::snapshot_resources_match_schema_enum` 对 `$defs.snapshotResource.enum` 逐条同序双向门禁。
- 桌面夹具：`valid/sync-snapshot-begin.json`、`valid/sync-snapshot-end.json` 的 `chunkCount` 6→3；新增 `valid/sync-snapshot-sessions-only-{begin,chunk,end}.json`（chunkCount=1，仅 sessions）覆盖「未协商目录 feature」；新增两条 `invalid/snapshot-chunk-detail-resource-*.json` 覆盖明细资源被 `enum` 拒。
- 「明细缺席不被表述为完整」：`docs/SYNC_PROTOCOL.md` §9.4 新增「明细缺席不是『为空』」段落；§9.4 资源表把五类明细标注为「只经 `session.read` 返回，不进快照」。
- local/remote 一视同仁：schema 的 `resource` 是全局封闭词表，不带 `origin` 条件；§9.4 把基线「仅 imported 会话豁免」改写为「无论 `origin.kind` 是 `local` 还是 `remote`」。

### R2/R3 分页与复合游标 — 符合

- `command.schema.json`：`$defs.sessionReadBefore` = object、`additionalProperties: false`、`required: ["createdAt","messageId"]`（两键必需，单字段游标在 schema 层不成立）；`$defs.sessionReadLimit` = `integer`、`minimum: 1`、**故意无 `maximum`**（超上限由服务端收敛而非报错，与 R2 一致）；`before`/`limit` 挂到 `sessionRead.payload`，`payload` 仍 `additionalProperties: false` + `required: ["include"]`（省略两参数即默认页）；`sessionReadResult.required` 加入 `hasEarlier`，定义为 `boolean`。
- Rust 镜像：`SessionReadBefore { created_at: Timestamp, message_id: Uuid }` + `deny_unknown_fields`、无 `default`；`SessionRead { include, before: Option<SessionReadBefore>, limit: Option<UIntAtLeast<1>> }`；`SessionReadResult.has_earlier: bool`（非 `Option`、无 `default`，缺席即拒）。`UIntAtLeast<1> = BoundedU64<1,u64::MAX>`，经 `u64::deserialize` 拒绝小数/负数——不比 schema 宽松。
- 复合游标防「排序依赖 UUID」：schema 与 Rust 均要求 `(createdAt, messageId)` 成对；`before` 只接受这两键（`offset` 之类被 `additionalProperties: false` 拒）。排序语义（createdAt 升序、messageId 仅作同刻 tie-breaker、MUST NOT 依赖 messageId 数值/字典序）在 §11.5 逐条写明并回指 §3.3「排序不得依赖 UUID」。
- 保留窗口错误：§11.5 指定 `sync.cursor_invalid` + `details.reason = "cursor_expired"`；该 code 与 reason 取值域在 `compatibility/errors/v1/errors.json` 与 `schemas/sync/v1/common.schema.json` 均已登记，一致。
- 夹具：`valid/command-session-read-default-page.json`（仅 include）、`valid/command-session-read-paged.json`（复合游标 + limit）、`valid/command-result-session-read-paged-completed.json`（`hasEarlier: true`，含两条 `createdAt` 相同的消息以坐实同刻 tie-break 场景）、`valid/command-result-session-read-last-page-completed.json`（`hasEarlier: false`）；负向 `invalid/command-session-read-single-field-before.json`、`invalid/command-result-session-read-missing-has-earlier.json`。

### R4 分页只作用于客户端同步面 — 符合

- `git diff --name-only 353ba6e..6779c36 -- crates/node-link-protocol schemas/node-link` 为空：Node Link 的 Rust DTO 与 schema **零改动**。
- `schemas/node-link/v1/command.schema.json#/$defs/submitSessionRead.payload` 仍只有 `include`（`additionalProperties: false`），Node Link 调用方按既有形状发起不受影响。
- 文档两侧都写明该边界：`docs/NODE_LINK_PROTOCOL.md` 命令表标注分页参数只存在于 Sync 面、并新增段落说明这是有意边界（回指 §12.4 `no-content-cache`）；`docs/SYNC_PROTOCOL.md` §11.5 写明 node-link schema 与 DTO 不含这两个键、行为与变更前一致。
- `docs/NODE_LINK_PROTOCOL.md` 属 WP1 声明写入范围，其改动是「写明非改动」，与 R4「node-link 侧零改动」不矛盾。

### 镜像宽严逐字段（Rust 不比 schema 宽松）

| 字段 | schema | Rust 镜像 | 结论 |
| --- | --- | --- | --- |
| `snapshot_chunk.resource` | `enum[3]` | `SnapshotResource` 3 变体 + `ALL` | 一致，且 drift 门禁覆盖 |
| `snapshot_begin/end.chunkCount` | `integer 1..3` | `BoundedU64<1,3>` | 一致 |
| `sessionRead.payload.before` | object、两键 required、`additionalProperties:false` | `SessionReadBefore` + `deny_unknown_fields`、无 default | 一致 |
| `sessionRead.payload.limit` | `integer minimum 1`、无 maximum | `Option<UIntAtLeast<1>>` | 一致（Rust 额外拒小数） |
| `sessionRead.payload` 其余 | `additionalProperties:false`、`required:["include"]` | `SessionRead` + `deny_unknown_fields` | 一致 |
| `sessionReadResult.hasEarlier` | `boolean`、required | `bool` 非 Option | 一致 |
| `sessionReadResult.resources.config_options` | `$ref snapshotItem.config_options`（归组） | `Option<Vec<ConfigOptionView>>`（扁平） | **不一致（基线既存，见 F1 / 范围外观察）** |

### 夹具与 manifest 完整性

- manifest 不重不漏、文件齐备、排序保持：94 条 case 全部有对应文件，磁盘上无未登记 case，无重复登记。
- 计数自洽：valid message-schema 用例 = 78−5 pairing = **73**（`EXPECTED_VALID_MESSAGE_CASES`）；body 层拒绝负例 = 16−2 信封层 = **14**（`EXPECTED_BODY_REJECTED`）。与基线（67 / 9）的差值恰为 WP1 夹具的 +6 valid（+7 新增 −1 删除）与 +5 invalid。WP1 给出的中间值与当前 manifest 实测条目数自洽；最终值由 TP1 在 MU1a 合入提交上定，符合 Shared File Ownership 的登记。
- 删除 `valid/sync-snapshot-chunk-config-options.json` 正当：其 `resource: config_options` 已被 D1 移出词表，保留必然与 `enum` 冲突；不存在以删除掩盖失败的情形。

### 写入范围

diff 的 23 个文件全部落在 WP1 Write Scope（`schemas/sync/v1/` 的 command/sync 两个文件、`fixtures/sync/v1/**`、`crates/sync-protocol/src/{command,sync}.rs`、`docs/SYNC_PROTOCOL.md`、`docs/NODE_LINK_PROTOCOL.md`）或已登记的共享写点 `crates/sync-protocol/tests/envelope_fixtures.rs`。`views.rs`、`node-link-protocol/`、`core/`、`storage-sqlite/`、`agent-host/`、`app/` 均未被触碰。`schemas/sync/v1/common.schema.json` 本轮无需改动（无新 `$defs` 引用），未改动不构成缺口。**无越界。**

## 范围外观察（不计入本轮结论）

1. **作者自报的 `SessionReadResources.config_options` 不同形，逐项回答（不作为 WP1 的 MAJOR）：**
   - (a) **属实**：Rust 侧为 `Option<Vec<ConfigOptionView>>`（扁平 id/name/type/currentValue…），schema 侧 `sessionReadResult.resources.config_options` 的 `items` 指向 `snapshotItem.config_options`（`{sessionId, configOptions, version}` 归组形状）。
   - (b) **基线既存**：`git show 353ba6ef:…/crates/sync-protocol/src/command.rs` 同行即 `Vec<ConfigOptionView>`，与目标提交同形；schema 侧同一 `$defs` 引用在基线与目标均存在。
   - (c) **责任归属**：按「不判既有缺陷」原则不在本轮判为 WP1 缺陷；但 WP1 本次删除了该形状唯一同形 Rust 镜像并新增了「复用同一批定义」的文档表述，故在 F1 中记 1 条 MINOR，建议本轮一并收口（改动小、方向单一）。真正的 wire 不匹配由主 Agent 决定是否单独派修。
   - (d) **不构成 WP1 的 MAJOR**：它不阻断 WP1 的四条交付要求（R1–R4 的 schema/夹具/文档层均已满足），且运行行为在基线即如此，非本轮回归。
2. **`daemon_lifecycle.rs::the_periodic_task_runs_at_least_once_and_is_cancelled_at_shutdown`**：`git diff --name-only 353ba6e..6779c36` 中无 `crates/app/**`，WP1 未触碰该 crate；PV1 日志（`.target-wt/wp1-verify.log`）中该用例本次实测 **ok**，同文件另一例 `the_periodic_task_runs_again_after_one_full_cycle` 亦 ok。该用例确与 WP1 无关，PV1 复跑全绿（exit 0）。
3. **分页服务端语义未实现**：「默认页=最近 20 条用户输入」「同一 `before` 重复读同结果」「超上限收敛」「保留窗口报 `cursor_expired`」目前只有 §11.5 规范文本，无生产者（`server::sync` 未落地）。这是变更既定边界，非 WP1 缺口。

## Assessment

### 本轮检视结论

- **PASS**（对应 Target Revision `6779c36678376f02b95a4071b62dca809b221a8b`）。0 CRITICAL / 0 MAJOR。
- R1–R4 四条 Requirement 与 D1/D2/D3 的决定在 schema、Rust 镜像、夹具、文档四层互相一致；`chunkCount` 口径（1..3，begin/end 同源 `$ref`）自洽；`session.read` payload/result 形状、`hasEarlier` 必填与错误码（`sync.cursor_invalid` + `cursor_expired`）一致；R4 的 node-link 零改动经 `git diff` 证实；写入范围无越界；夹具/manifest 不重不漏且与 `envelope_fixtures` 中间值自洽；PV1 复跑 exit 0。
- 非阻断项：`review-wp1-r1-F1`（MINOR，镜像缺口 / 文档表述与 Rust 现状的落差）与 `review-wp1-r1-F2`（SUGGESTION，夹具负例覆盖）。二者不阻断本 WP 合入 MU1a 候选，但 F1 建议在同一 WP 收口。
- 相对上一份针对**未提交工作区**的检视（Attempt 1）：该报告的 F1（§9.4 未写明 `chunkCount:0` 非法）、F2（`SnapshotItemConfigOption` 孤立）、F3（§3.3 未列 `limit`）在**已提交**的 `6779c366` 上均已修复——已独立核实（§9.4 现写明下界 1 与 `chunkCount: 0` 是形状错误；`SnapshotItemConfigOption` 已删除；§3.3 已列 `limit`）。这正说明对固定提交的独立复核必要。

### 未验证内容（须主 Agent 注意）

- 服务端 `server::sync` 的分页语义实现与端到端行为 → 交本变更其他工作包 / 最终验收；本轮不判。
- 夹具/用例集合是否齐备与风险盲区 → 交主 Agent Coverage Index 与 validator。
- PV1 未由本 reviewer 复跑；本报告引用的是主 Agent 的 `.target-wt/wp1-verify.log`（exit 0）。
- `sync` 家族之外其他消息类型与 schema 的逐字段约束未逐条核对（不在 WP1 范围）。

PASS
