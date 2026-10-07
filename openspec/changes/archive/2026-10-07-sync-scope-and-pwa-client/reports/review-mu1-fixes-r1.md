<!-- MU1 前置修复的独立复核报告（Round 1）。Reviewer 只报告，不修改产品代码、任务状态或 verification.md。 -->

task_id: "MU1-FIXES"
role: reviewer
phase: branch
agent_context:
  agent_id: "review-mu1-fixes"
  isolation: "fork_turns=none（新建独立子 Agent；未参与 WP1/WP2 的实现或本轮修复对话，Round 1 的三位作者 review-1 / coding-1b / coding-2c 的上下文均未继承）"
target_revision: "WORKTREE-DIRTY:D:\\Project\\acp-remote\\.worktrees\\wp1@353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f（分支 feat/wp1-sync-snapshot-contract 的未提交工作区差异）与 WORKTREE-DIRTY:D:\\Project\\acp-remote\\.worktrees\\wp2@353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f（分支 feat/wp2-derived-event-fields 的未提交工作区差异）；两者均无提交可绑定"
scope: "MU1 前的四项 MINOR 修复复核（review-wp1-r1 的 M1/M2/M3 与 review-wp2-r2 的 F5）。逐处核对修复后的文件内容与其在 schema、Rust 镜像、夹具、文档四层上的一致性；确认 `crates/sync-protocol/tests/schema_drift.rs` 门禁缺口仍归属 TP1 未被触碰；确认无 `required` 集合、schema enum 成员或 Node Link schema 被作为副作用改动；并在各受检 worktree 内重跑 PV1 门禁。不判用例集合是否充分（交主 Agent Coverage Index 与 validator），不判 `server::sync` 服务端语义（属既定 Non-Goals）。"
changes: "只读检视。未修改任何被检视文件，未执行 git add/commit/push，未创建或切换 worktree；仅新增本报告文件。（检视期间一次 CARGO_TARGET_DIR 传参被 shell 吞掉反斜杠，在 .worktrees/wp1 下产生了一个名为 `Projectacp-remote.target-wtwp1/` 的空目录，已立即删除；`git status --porcelain --untracked-files=all` 现无任何非夹具的未跟踪文件，worktree 回到作者交付时的状态。）"
checks:
  - id: "PV1/npm-check（.worktrees/wp1，cwd=受检 worktree）"
    command: "npm run check（cwd=D:\\Project\\acp-remote\\.worktrees\\wp1）"
    result: "exit 0。check:schemas 133 valid / 35 invalid、check:commands 13、check:errors 58、check:features 13、check:assets 17 schemas / 181 fixture files、check:acp 71 rows、check:docs、check:boundaries、check:drift、check:agentic 十道门禁全部 PASS；openspec validate 21 passed / 0 failed"
  - id: "PV1/npm-check（.worktrees/wp2，cwd=受检 worktree）"
    command: "npm run check（cwd=D:\\Project\\acp-remote\\.worktrees\\wp2）"
    result: "exit 0。十道门禁全部 PASS；check:schemas 127 valid / 30 invalid、check:assets 170 fixture files、openspec validate 21 passed / 0 failed"
  - id: "PV1/cargo-test（WP1）"
    command: "CARGO_TARGET_DIR=D:/Project/acp-remote/.target-wt/wp1 cargo test --locked -p sync-protocol --all-features（cwd=.worktrees/wp1）"
    result: "exit 0；10 个 suite、56 个用例全绿"
  - id: "PV1/cargo-test（WP2）"
    command: "CARGO_TARGET_DIR=D:/Project/acp-remote/.target-wt/wp2 cargo test --locked -p sync-protocol --all-features（cwd=.worktrees/wp2）"
    result: "exit 0；10 个 suite、52 个用例全绿"
  - id: "补充（非计划内，只读佐证 M2 删除未留编译面问题）"
    command: "cargo check --locked --workspace --all-targets --all-features；cargo clippy --locked --workspace --all-targets --all-features -- -D warnings（cwd=.worktrees/wp1 与 .worktrees/wp2 各一次）；rustfmt --check --edition 2024 crates/sync-protocol/src/sync.rs"
    result: "两个 worktree 的 check 与 clippy 均 exit 0（12 个 crate 全部编译，无警告）；rustfmt 对 WP1 的 sync.rs exit 0"
issues: "0 CRITICAL / 0 MAJOR / 0 MINOR / 0 SUGGESTION。四项 MINOR（M1/M2/M3/F5）逐项确认已解决且未引入回归。"
result: PASS
evidence_paths:
  - "openspec/changes/sync-scope-and-pwa-client/reports/review-mu1-fixes-r1.md"
  - "openspec/changes/sync-scope-and-pwa-client/reports/review-wp1-r1.md"
  - "openspec/changes/sync-scope-and-pwa-client/reports/review-wp2-r2.md"
  - "D:\\Project\\acp-remote\\.target-wt\\wp1（CARGO_TARGET_DIR，WP1 的 cargo test / check / clippy 输出）"
  - "D:\\Project\\acp-remote\\.target-wt\\wp2（CARGO_TARGET_DIR，WP2 的 cargo test / check / clippy 输出）"

```agentic-handoff
version: 1
agent_context:
  agent_id: "review-mu1-fixes"
  isolation: "fork_turns=none"
handoff_index:
  - task_id: "MU1-FIXES"
    work_package: WP1
    role: reviewer
    phase: branch
    round: 1
    stage: work-package
    target_revision: "WORKTREE-DIRTY:D:\\Project\\acp-remote\\.worktrees\\wp1@353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f"
    evidence_type: REVIEW
    evidence_id: review-mu1-fixes-r1
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/review-mu1-fixes-r1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在 .worktrees/wp1（分支 feat/wp1-sync-snapshot-contract、HEAD=353ba6ef...，git rev-parse 已核实）上只读复核 review-wp1-r1 的 M1/M2/M3 三项 MINOR 的修复后状态：docs/SYNC_PROTOCOL.md:771 的 chunkCount 1..=3 表述、:103 的结构性常量清单新增 limit、crates/sync-protocol/src/sync.rs 中 SnapshotItemConfigOption 的删除与随之失效的 ConfigOptionView import；逐层比对 schemas/sync/v1/sync.schema.json:129 的 snapshotChunkCount（minimum 1 / maximum 3）、crates/sync-protocol/src/sync.rs:516 与 :604 的 BoundedU64<1,3>、docs/NODE_LINK_PROTOCOL.md:75/:612/:625 的反向引用；并在受检 worktree 内执行 npm run check（exit 0，133 valid/35 invalid）、cargo test --locked -p sync-protocol --all-features（exit 0，56 用例）、cargo check --workspace --all-targets --all-features（exit 0）、cargo clippy --workspace -D warnings（exit 0）、rustfmt --check（exit 0）。目标无提交，绑定的是 worktree 差异本身。"
    source_evidence: NOT_APPLICABLE
  - task_id: "MU1-FIXES"
    work_package: WP2
    role: reviewer
    phase: branch
    round: 1
    stage: work-package
    target_revision: "WORKTREE-DIRTY:D:\\Project\\acp-remote\\.worktrees\\wp2@353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f"
    evidence_type: REVIEW
    evidence_id: review-mu1-fixes-r1
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/review-mu1-fixes-r1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在 .worktrees/wp2（分支 feat/wp2-derived-event-fields、HEAD=353ba6ef...）上只读复核 review-wp2-r2 的 F5（MINOR）的修复后状态：fixtures/sync/v1/valid/view-agent-connected.json:9,:11 与 view-agent-disconnected.json:9,:11 的 sessionSequence/sessionId 置 null，与 view-device-revoked.json:9,:11 逐字段一致；比对 docs/SYNC_PROTOCOL.md:1030-1031/:1043 与 specs/core-derived-events/spec.md:68 的节点级 MUST、schemas/sync/v1/event.schema.json:17-26 的 uuid|null 与 decimalString|null 取值域、crates/sync-protocol/src/event.rs:165/:169 的 Nullable 镜像、fixtures/sync/v1/manifest.json:461-473 的条目未被扰动；确认 crates/sync-protocol/tests/schema_drift.rs 未被改动（F3 仍归 TP1）；并在受检 worktree 内执行 npm run check（exit 0，127 valid/30 invalid）、cargo test --locked -p sync-protocol --all-features（exit 0，52 用例）、cargo check（exit 0）、cargo clippy -D warnings（exit 0）。目标无提交，绑定的是 worktree 差异本身。"
    source_evidence: NOT_APPLICABLE
```

## Review Context

| 项 | 取值 |
| --- | --- |
| Review ID / Round | `review-mu1-fixes-r1` / 1 |
| Review Type | `branch`（对 WP1/WP2 工作包阶段候选的复核） |
| Review Stage | 工作包编写阶段之后、候选验证（MU1）之前的前置修复复核 |
| Work Package | WP1（M1/M2/M3）与 WP2（F5） |
| Repository | WP1 `D:\Project\acp-remote\.worktrees\wp1`（`feat/wp1-sync-snapshot-contract`）；WP2 `D:\Project\acp-remote\.worktrees\wp2`（`feat/wp2-derived-event-fields`） |
| Base Revision | `353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f`（两个 worktree 的 HEAD 即此提交，均已用 `git rev-parse` 核实） |
| Target Revision | 各 worktree 的未提交工作区差异。WP1：11 个已修改文件（+331/−212）+ 1 个删除 + 12 个未跟踪夹具；WP2：7 个已修改文件（+161/−13），无未跟踪文件 |
| 复核的问题 ID | `review-wp1-r1` 的 M1/F1、M2/F2、M3/F3；`review-wp2-r2` 的 F5 |
| 读取的规则 | `openspec/schemas/agentic/roles/reviewer.md`、`roles/_shared/role-report.md` |
| 读取的需求 | `specs/core-derived-events/spec.md:66-70`（R8 节点级 MUST）、`specs/sync-snapshot-scope/spec.md:11`（chunkCount 与实际下发资源数量一致）、`design.md:31`（chunkCount 全量 3、未协商目录 feature 时 1）；原检视报告 `reports/review-wp1-r1.md`、`reports/review-wp2-r2.md` 全文 |
| 实际检查范围 | 四处修复点的文件内容与其在 schema / Rust 镜像 / 夹具 / 文档四层上的一致性；`schema_drift.rs` 门禁缺口的归属；`required` 集合、schema enum 成员、Node Link schema 的副作用核查；写入范围；PV1 门禁复跑 |
| 使用的验证证据 | 本轮自行执行的 6 条命令（见上 `checks`），全部在对应受检 worktree 内以该 worktree 为 cwd 执行；未复用任何既有日志；未执行 E2E |
| 限制 | 按角色边界不判用例集合是否充分、风险盲区（交主 Agent Coverage Index 与 tester/validator）；不判 `server::sync` 服务端语义（design 的既定 Non-Goals）；不判 MU1/MU2/MU3 后续门禁 |

## 四项 MINOR 修复的逐项复核

### M1（`review-wp1-r1` F1，MINOR）— **已解决**

**复核依据**：直接读取修复后的 `docs/SYNC_PROTOCOL.md:771`。

> `chunkIndex` 是从 `0` 开始的十进制字符串，按 index 顺序连续递增，服务端必须按 index 顺序发送。`chunkCount` 在 begin/end 中必须一致，且保持 integer；**它 MUST 等于本次快照实际下发的 chunk 数，取值在 `1..=3` 之间——上界是 3（可下发资源的种类数，见下），下界是 1：`sessions` chunk 恒发，即使会话为空也照发一个空数组，因此 `chunkCount: 0` 是形状错误。**示例中的 `3` 是协商了 `core.local-catalog.v1` 时的一次完整快照（每种资源一个 chunk）；未协商该 feature 时目录资源 MUST NOT 发送，`chunkCount` 为 `1`。

1. **下界与理由均已写明**：`1..=3` 的区间、下界取 1 的**具体理由**（`sessions` chunk 恒发、空会话也照发空数组）、以及 `chunkCount: 0` 属形状错误的结论，三者都在同一句里，不再需要读者从 schema 反推。
2. **与 schema 一致**：`schemas/sync/v1/sync.schema.json:129-134` 新增的 `$defs/snapshotChunkCount` 是 `"type": "integer"` + `"minimum": 1` + `"maximum": 3`，被 `snapshotBegin`（`:121`）与 `snapshotEnd`（`:207`）共同 `$ref`。基线该处是内联的 `minimum: 0, maximum: 100000`（见 `git show 353ba6e:...`），因此「下界为 1」确如 Round 1 所记是本轮新增约束，prose 现已跟上。
3. **与 Rust 镜像一致**：`crates/sync-protocol/src/sync.rs:516`（`SnapshotBegin.chunk_count`）与 `:604`（`SnapshotEnd.chunk_count`）均为 `BoundedU64<1, 3>`；`:652-665` 的单元测试 `chunk_count_matches_the_resources_a_snapshot_can_carry` 断言 `1`/`3` 通过、`0`/`4`/`100_000` 被拒，与 prose 的 `1..=3` 逐值对应。两侧 schema 与 Rust 不可能各写一套口径。
4. **是改写而非外挂段落**：`git diff -- docs/SYNC_PROTOCOL.md` 显示该行是**原地替换**（`-` 一行 / `+` 一行，行号 771 未变），新约束以分号接在既有句子「`chunkCount` 在 begin/end 中必须一致，且保持 integer」之后、与紧随其后的「示例中的 `3` 是…」同一句群内。文中没有新增独立段落、没有重复陈述、没有留下与新句并存的旧句（全文 `chunkCount` 仅 4 处命中：`:739`、`:765` 两处示例值 3，`:771` 规则句，`:1301` §11.5 对 `limit` 的类比引用）。
5. **与同节其它约束不冲突**：`:802` 说未协商 `core.local-catalog.v1` 时 `workspaces`/`agents` 「连空数组占位也不得发送」——这与新句「`sessions` 恒发」是**互补**而非矛盾的两条：前者约束目录资源，后者约束会话清单资源，而 `sessions` 不受该 feature 门控。`:771` 自身末尾也仍写着「未协商该 feature 时目录资源 MUST NOT 发送，`chunkCount` 为 `1`」，与 `1..=3` 的下界衔接自洽（取 1 而非 0）。
6. **与 spec 的关系未被改写**：`specs/sync-snapshot-scope/spec.md:11` 只要求「`chunkCount` MUST 与实际下发的资源数量一致」，未规定下界；prose 补出的下界是 D1 的推论且与 `design.md:31`（「全量为 3，未协商目录 feature 时为 1」）一致，未越出 spec。

**结论**：F1 的具体后果（prose 落后于 schema 新增的下界约束）已消除，且改写形态符合建议。闭环。

### M2（`review-wp1-r1` F2，MINOR）— **已解决**

**复核依据**：直接读取修复后的 `crates/sync-protocol/src/sync.rs`，并与基线逐行比对。

1. **结构体确已删除**：`git show 353ba6e:crates/sync-protocol/src/sync.rs` 显示基线在 `:368` 有 `pub struct SnapshotItemConfigOption`（含 `session_id`/`config_options: Vec<ConfigOptionView>`/`version`）及 4 行文档注释；工作区版本该块整体消失，现 `:358` 直接是 `SnapshotItemCapability`。
2. **作者的「无消费者」判断成立**：基线中该类型的全部引用只有三处——`:411` 的 `SnapshotItems::ConfigOptions(Vec<SnapshotItemConfigOption>)` 变体、`:448` 的 `parse_items::<SnapshotItemConfigOption>` 分派、以及定义本身。这三处中的两处属于 `SnapshotItems` 的明细资源变体，已随 D1 的词表收窄在同一 diff 内删除（`SnapshotItems` 现只有 `Sessions`/`Workspaces`/`Agents` 三个变体，`:384-387`）。删除后 `grep -rn SnapshotItemConfigOption` 在 `crates/` 与 `docs/` 下命中数为 **0**。
3. **作者的「`SessionReadResources.config_options` 自带类型」判断成立，因此这不是「缺一个待接线的类型」**：`crates/sync-protocol/src/command.rs:919` 是 `pub config_options: Option<Vec<ConfigOptionView>>`，元素类型取自 `common.rs:269` 的 `ConfigOptionView`（扁平视图形状），与被删的 `SnapshotItemConfigOption`（多一层按会话归组的 `sessionId`/`version`）**是两个不同类型**。也就是说基线上这两处本来就各自成形、互不依赖；`SnapshotItemConfigOption` 从来不是 `SessionReadResources` 的候选实现，删除它不会让任何字段失去类型。**这同时印证了 Round 1「范围外观察」的判断**：`command.schema.json:337` 让该字段 `$ref` `sync.schema.json#/$defs/snapshotItem.config_options`（分组形状），与 Rust 的扁平 `ConfigOptionView` 不一致，而该不一致在基线提交上即已存在（`git show 353ba6e:.../command.rs:894` 同为 `Option<Vec<ConfigOptionView>>`），**不是本轮引入**，按角色边界不在本轮判定为缺陷。本轮修复没有触碰 `command.rs` 的该字段，也没有触碰 schema 的 `$defs/snapshotItem.config_options`（`sync.schema.json:291-302` 原样保留，仍被 `command.schema.json` 引用），即那条既存不一致的原样保留，不因本次删除而扩大也不被误当作已修。
4. **随之失效的 import 已一并删除，未留悬空引用**：`sync.rs:14-18` 的 `use crate::common::{...}` 列表中 `ConfigOptionView` 已移除（基线 `:12-16` 有）。删除后 `grep -c ConfigOptionView crates/sync-protocol/src/sync.rs` = **0**，即该文件内再无任何使用点，import 的删除是必需的而非可选；反向也不存在「import 留着但类型没了」或「类型删了但 import 留着」的悬空。
5. **未产生新的孤儿类型**：`sync.rs` 内其余 `pub` 类型均有消费者——`SnapshotItemMessage`/`SnapshotItemTurn`/`SnapshotItemPendingInteraction`/`SnapshotItemCapability` 四个仍被 `command.rs:36` 导入并用于 `SessionReadResources` 的四个字段（`:901`/`:907`/`:913`/`:925`）；`SnapshotItemWorkspace`/`SnapshotItemAgent` 是别名，被 `SnapshotItems` 的 `Workspaces`/`Agents` 变体消费。因此本轮**净减少**了一个无消费者类型，没有把孤儿从一处搬到另一处。
6. **编译面与 lint 干净**：`cargo check --locked --workspace --all-targets --all-features`（cwd=`.worktrees/wp1`）**exit 0**，12 个 crate 全部编译通过，证明删除 `pub` 类型没有打断 `crates/server` 等任何外部消费者；`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` **exit 0**，无警告。`rustfmt --check --edition 2024 crates/sync-protocol/src/sync.rs` **exit 0**，删除未留下格式偏差（如多余空行）。

**结论**：F2 建议的两个选项中作者选择了「删除」，且删除的正当性（无消费者、不是待接线类型）、完整性（import 同步、无新孤儿、无悬空引用）与工具面（check / clippy / fmt 三项 exit 0）均成立。闭环。

### M3（`review-wp1-r1` F3，MINOR）— **已解决**

**复核依据**：直接读取修复后的 `docs/SYNC_PROTOCOL.md:103` 与 §11.5 的 `:1301`，并检查反向引用方。

1. **采用了「补清单」而非「改措辞」的方案**：`SYNC_PROTOCOL.md:103` 现为「结构性常量保持 integer：`protocolVersion`、`minProtocolVersion`、`maxProtocolVersion`、`selectedProtocolVersion`、`schemaVersion`、`chunkCount`、`heartbeatIntervalMs`、**`limit`（Sync 面 `session.read` 的可选页大小）** 和 `limits.*` 中的字节、条数、连接数上限。」——`limit` 已入清单，且带括号限定其适用范围为 Sync 面 `session.read` 的可选页大小，措辞精确。
2. **§11.5 的声称现已事实成立**：`:1301` 写「`limit` 是可选的整数码数（`minimum: 1`，**与 `chunkCount` 同属第 3.3 节的结构性常量一类**，不走十进制字符串规则）」。Round 1 指出的正是这句话与 `:103` 清单脱节；现在 `limit` 确在 `:103` 的清单内，该声称不再是空头引用。
3. **§3.3 内部无自相矛盾**：`:101` 的十进制字符串规则逐个列举了 `globalSequence`/`sessionSequence`/`connectionSequence`/`cursor.globalSequence`/`originSequence`/`version`/`expectedVersion`/`byteLength`/`deltaIndex`/`chunkIndex` 共 10 个字段，**不含 `limit`**；`:103` 的 integer 清单含 `limit`。两条规则靠「逐个列举」划分而非「除…外皆…」的补集表述，因此 `limit` 同时只落在 integer 一侧不存在冲突。这也是 M3 的建议方案能成立的前提——若 `:101` 写的是开放式的补集规则，则把 `limit` 放进 `:103` 反而会制造矛盾；现写法不存在该问题。
4. **`docs/NODE_LINK_PROTOCOL.md` 不与此矛盾**（Round 1 特别要求核对的方向）：反向引用点有三处，逐处核对——
   - `NODE_LINK_PROTOCOL.md:75`：「framing、序号编码、未知字段和 schema 规则与 `SYNC_PROTOCOL.md` §3.3、§4 一致：序号是**无前导零十进制字符串**，结构性常量（`chunkCount`、`schemaVersion`、`heartbeatIntervalMs`、`limits.*`）保持 integer」。该处是对 §3.3 规则的**概括性转述并自带一份 Node Link 侧适用的举例**，不是「§3.3 清单的完整副本」；新增的 `limit` 是 Sync 面 `session.read` 的载荷字段，而 Node Link 面按 `:612` 与 `:625` 明确**没有** `limit` 这个键，因此 Node Link 侧的举例不列 `limit` 是正确的，不是遗漏。规则本身（「结构性常量保持 integer」）两侧完全一致，无矛盾。
   - `NODE_LINK_PROTOCOL.md:612`（命令表 `session.read` 行）：`before`/`limit` 不在本面出现，与 `:103` 新增条目的括号限定「Sync 面」一致。
   - `NODE_LINK_PROTOCOL.md:625`（新增段落）：明写「Sync 侧的 payload 另有可选的 `before`…与 `limit`，本面一律没有这两个键」，与 `:103` 的限定语方向一致。
   三处均不与 `:103` 的新条目冲突；本 diff 对 `NODE_LINK_PROTOCOL.md` 的改动只有 `:612` 与 `:625`（`git diff` 确认 3 insertions / 1 deletion），**未触碰 `:75`**，即反向引用点本身未被这次修复扰动。
5. **与 schema 侧一致**：`schemas/sync/v1/command.schema.json` 的 `$defs/sessionReadLimit` 是 `"type": "integer"` + `"minimum": 1`（无 `maximum`），description 亦写明「是结构性计数（integer，与 chunkCount 同类，§3.3）」；Rust 侧 `command.rs` 的 `limit: Option<UIntAtLeast<1>>` 为整型。文档、schema、Rust 三层在「integer 而非十进制字符串」这一点上同宽。

**结论**：F3 的两个建议方案中作者选择了「在 §3.3 清单补 `limit`」，且该改动使 §11.5 的交叉声称变为事实、未与 §3.3 自身的字符串规则冲突、未与反向引用方 `NODE_LINK_PROTOCOL.md` 的三处引用产生矛盾。闭环。

### F5（`review-wp2-r2`，MINOR）— **已解决**

**复核依据**：直接读取修复后的两条夹具，并与 `view-device-revoked.json`、`docs/SYNC_PROTOCOL.md` §10.3、`specs/core-derived-events/spec.md`、schema 与 Rust 镜像逐层比对。

1. **修复内容与 Round 1 的建议逐字对应**：`fixtures/sync/v1/valid/view-agent-connected.json:9` `"sessionSequence": null`、`:11` `"sessionId": null`；`view-agent-disconnected.json:9`/`:11` 同样两处置 `null`。`git diff` 显示每条文件恰好 2 行改动（`"225"`→`null`、`"226"`→`null`；`"5d73cd10-a465-43cd-b3f1-704e2d49e99e"`→`null`），**无其它字段被动过**。
2. **与既有节点级事件夹具一致**：`fixtures/sync/v1/valid/view-device-revoked.json:9` `"sessionSequence": null`、`:11` `"sessionId": null`。两条 agent 夹具现在与它**逐字段同形**（`sessionId`/`sessionSequence` 均显式为 `null`，而非省略键——这点重要，因为 `event.schema.json:17` 的 `required` 含 `sessionSequence`/`sessionId`，省略键会被 `required` 拒，显式 `null` 才是正确写法）。
3. **与 §10.3 的 MUST 一致**：`docs/SYNC_PROTOCOL.md:1030`（`agent.connected` 行：「节点级事件（`sessionId` 为空）」）、`:1031`（`agent.disconnected` 行：「同为节点级事件」）、`:1043`（「它们是**节点级**事件（事件信封的 `sessionId` 为空）」）。Round 1 指出的正是「文档声明节点级、夹具却带非空 `sessionId`」的自不自洽；现在夹具与文档口径统一。
4. **与 spec 的 MUST 一致**：`specs/core-derived-events/spec.md:68`「Agent 连接与断开事件 SHALL 是节点级事件：**其会话标识 MUST 为空**」。夹具的 `sessionId: null` 是该 MUST 的机器可读体现。
5. **`schemas/sync/v1/event.schema.json` 仍然接受**（Round 1 特别要求核对的方向）：`:17-19` 的 `sessionSequence` 是 `oneOf [common.schema.json#/$defs/decimalString, {"type":"null"}]`，`:20-23` 的 `sessionId` 是 `oneOf [common.schema.json#/$defs/uuid, {"type":"null"}]`。`null` 在两个取值域内，因此**不存在 schema 层的矛盾或被迫放宽**；Round 1 已指出该 schema 无按 `eventType` 的条件约束，本次修复也不需要、也没有新增这类条件约束。
6. **Rust 镜像同宽**：`crates/sync-protocol/src/event.rs:165` `pub session_sequence: Nullable<DecimalString>`、`:169` `pub session_id: Nullable<Uuid>`（反序列化侧 `:189`/`:193` 同型）。`Nullable<T>` 要求键存在且值为 `T | null`，与 schema 的 `required` + `oneOf[…, null]` 严格一致——即 Rust 侧同样接受 `null`，且不会因键缺席而误判。
7. **夹具 `required` 集合与 manifest 条目均未被扰动**：
   - `fixtures/sync/v1/manifest.json:460-466`（`view-agent-connected.json`）与 `:467-473`（`view-agent-disconnected.json`）的四个字段——`fixture`/`schema`/`viewSchema`/`viewDef`——与 `:495-501`（`view-device-revoked.json`）结构完全相同，`viewDef` 指针仍分别为 `#/$defs/agent.connected` / `#/$defs/agent.disconnected`，`valid: true` 未变。`git diff --stat` 显示 WP2 的改动文件列表为 7 个，**`fixtures/sync/v1/manifest.json` 不在其中**，即 manifest 零改动。
   - `schemas/sync/v1/event.schema.json` 的 `required` 数组（信封层 `:8`、body 层 `:17`）**未被本次修复改动**：WP2 对该文件的 diff 全部落在 `$defs/file.changed` 的三个可选属性与两个 `state` 枚举上（见下节），与 F5 无关；F5 只改了两个夹具文件的内容值。
   - `crates/sync-protocol/tests/view_projections.rs:23` 的 `EXPECTED_VIEW_CASES: usize = 36` 未变——夹具**数量**未变（只是内容变），计数门禁无需也未被上调；实测 52 个 Rust 用例全绿即包含该计数断言。

**结论**：F5 的具体后果（合同语料内部不自洽、下游会照抄出「`agent.connected` 携带会话标识」的样例）已消除，修复与 §10.3 的 MUST、spec R8、schema 取值域、Rust 镜像、manifest 登记五处全部对齐。闭环。

## 副作用与边界核查

### `crates/sync-protocol/tests/schema_drift.rs` 的门禁缺口仍归属 TP1，未被触碰 — 符合预期

- WP2：`git diff --stat -- crates/sync-protocol/tests/` 输出**为空**，确认 `schema_drift.rs` 未被本轮改动。文件 `:295-322` 的第三个方向仍是四条硬编码 `assert_eq!`（`PlanPriority`/`PlanStatus`/`ElicitationAction`/`TerminalStream`），`AgentConnectedState`/`AgentDisconnectedState` 仍不在其中——即 `review-wp2-r1-F3` 指出的缺口原样保留，与 Round 2 的判断（属 TP1，WP2 不得自行修改 `crates/*/tests/`）一致。
- WP1：`git diff --stat -- crates/sync-protocol/tests/schema_drift.rs` 输出**为空**，同样未被触碰。`:331-348` 的 `snapshot_resources_match_schema_enum` 仍是原有的一条双向 `assert_eq!`，未被削弱或删除。
- 两个 worktree 中 `crates/sync-protocol/tests/` 下唯一被修改的文件是 WP1 的 `envelope_fixtures.rs`（`EXPECTED_VALID_MESSAGE_CASES` 67→73、`EXPECTED_BODY_REJECTED` 9→14），这属于 WP1 原始交付的夹具计数同步，**不是本轮四项修复的一部分**，也不涉及 schema_drift 的枚举门禁。

### 无 `required` 集合、schema enum 成员或 Node Link schema 被作为副作用改动 — 确认

| 受保护面 | WP1 | WP2 | 证据 |
| --- | --- | --- | --- |
| Node Link schema（`schemas/node-link/**`） | 未改 | 未改 | 两侧 `git diff --stat -- schemas/node-link` 均为空 |
| Node Link 夹具（`fixtures/node-link/**`） | 未改 | 未改 | 两侧 `git diff --stat -- fixtures/node-link` 均为空 |
| Node Link Rust（`crates/node-link-protocol`） | 未改 | 未改 | `git diff --stat -- crates/node-link-protocol` 为空 |
| `sync.schema.json` 的 `$defs/snapshotResource.enum` | `["sessions","workspaces","agents"]` | 未涉及 | 这是 D1 的原始改动（Round 1 已判符合），**本轮四项修复未再触碰**：`git diff` 中该 enum 的改动只出现在 D1 的那次收窄，四项修复涉及的 schema 行是 `$defs/snapshotChunkCount`（M1 的对照面，只读未改） |
| `event.schema.json` 的 `required`（`:8` 信封、`:17` body） | 未涉及 | 未改 | WP2 对该文件零改动（WP2 改的是 `event-views.schema.json`） |
| `event-views.schema.json` 的 `required` 集合 | 未涉及 | 未改 | `:319` `file.changed` 的 `required` 仍恰为 `["changeId","kind","displayPath","summary"]`；`:333`/`:342` 两个 agent 视图的 `required` 仍为 `["agentId","state"]`。均为 WP2 原始交付的内容，本轮 F5 只改夹具、不改 schema |
| `command.schema.json` 的 `required` | `sessionReadResult` 加 `hasEarlier`（WP1 原始交付） | 未涉及 | Round 1 已判符合 R2/R3；本轮 M3 只改文档，未再动 schema |
| 夹具 manifest | WP1 改（新增 12 条夹具登记，属原始交付） | **未改** | WP2 的 `git diff --stat` 7 个文件中不含 `fixtures/sync/v1/manifest.json` |

### 写入范围与 worktree 卫生

- WP1：`git status --porcelain` = 11 个 ` M`（`crates/sync-protocol/src/{command,sync}.rs`、`crates/sync-protocol/tests/envelope_fixtures.rs`、`docs/{SYNC_PROTOCOL,NODE_LINK_PROTOCOL}.md`、`fixtures/sync/v1/manifest.json`、`fixtures/sync/v1/valid/sync-snapshot-{begin,end}.json`、`schemas/sync/v1/{command,sync}.schema.json`）+ 1 个 ` D` + 12 个 `??`（全部在 `fixtures/sync/v1/` 下）。`--untracked-files=all` 过滤掉 `fixtures/sync/v1/` 后**无任何未跟踪文件**。
- WP2：`git status --porcelain` = 7 个 ` M`，**无未跟踪文件、无删除**。
- 两侧受保护路径（`crates/{node-link-protocol,core,storage-sqlite,agent-host,app}`、`crates/sync-protocol/src/views.rs`（对 WP1）、`crates/sync-protocol/tests/`（对 WP2））均未被本轮修复触碰。

## Findings

| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation | Recheck |
| --- | --- | --- | --- | --- | --- | --- |
| review-wp1-r1-F1 | — | `docs/SYNC_PROTOCOL.md:771`（schema 侧 `schemas/sync/v1/sync.schema.json:129`；Rust 侧 `crates/sync-protocol/src/sync.rs:516`/`:604`） | Round 1：schema 把 `chunkCount` 下界从 0 抬到 1，§9.4 prose 未跟上 | — | — | **已解决**：`:771` 原地改写原句，写明 `1..=3`、下界取 1 的理由（`sessions` chunk 恒发、空会话照发空数组）与 `chunkCount: 0` 是形状错误；与 `snapshotChunkCount`（`minimum: 1`/`maximum: 3`）及 `BoundedU64<1,3>` 三方一致；非外挂段落（行号未变、`-`/`+` 一对一替换、与 `:802` 的目录资源门控互补不冲突） |
| review-wp1-r1-F2 | — | `crates/sync-protocol/src/sync.rs`（基线 `:361-373` 的 `SnapshotItemConfigOption` 已删除；`:14-18` 的 import 已同步） | Round 1：变体删除后该 `pub struct` 成为无消费者孤儿，门禁不报 | — | — | **已解决**：结构体与随之失效的 `ConfigOptionView` import 一并删除，文件内该标识符计数为 0，全仓 `grep` 命中 0；`SessionReadResources.config_options`（`command.rs:919`）本就自带 `ConfigOptionView` 类型、并非待接线，删除未让任何字段失去类型；其余 `pub` 类型均有消费者，未产生新孤儿；`cargo check` / `clippy -D warnings` / `rustfmt --check` 三项 exit 0 |
| review-wp1-r1-F3 | — | `docs/SYNC_PROTOCOL.md:103`（§11.5 的声称在 `:1301`） | Round 1：§11.5 自称 `limit` 已属 §3.3 结构性常量清单，但清单未含它 | — | — | **已解决**：`:103` 的清单已加入 `limit`（带「Sync 面 `session.read` 的可选页大小」限定），`:1301` 的交叉声称变为事实；§3.3 自身的字符串规则（`:101`）为逐个列举式、不含 `limit`，两条规则不冲突；反向引用方 `docs/NODE_LINK_PROTOCOL.md:75`/`:612`/`:625` 三处均不矛盾，且 `:75` 未被本轮触碰 |
| review-wp2-r2-F5 | — | `fixtures/sync/v1/valid/view-agent-connected.json:9`/`:11`、`view-agent-disconnected.json:9`/`:11` | Round 2：文档声明 agent 事件为节点级（`sessionId` 为空），两条 valid 夹具却带非空 `sessionId`/`sessionSequence` | — | — | **已解决**：两条夹具的 `sessionSequence` 与 `sessionId` 均置 `null`（显式 null 而非省略键），与 `view-device-revoked.json:9`/`:11` 逐字段同形，每文件恰好 2 行改动、无其它字段被动；与 §10.3（`:1030`/`:1031`/`:1043`）及 `specs/core-derived-events/spec.md:68` 的 MUST 一致；`event.schema.json:17-23` 的 `uuid\|null` 与 `decimalString\|null` 取值域无需改动即接受；`event.rs:165`/`:169` 的 `Nullable` 镜像同宽；manifest 条目（`:460-473`）与 `event.schema.json` 的 `required` 均未被扰动，`view_projections.rs:23` 的夹具计数 36 无需变更 |

> 本轮**未发现新问题**（新编号 `review-mu1-fixes-r1-F<n>` 无适用项）。

## Assessment

### 本轮结论：PASS

四项 MINOR（M1/M2/M3/F5）逐项复核确认全部解决，改动形态与 Round 1/2 的建议一致且各自最小；未发现任何 CRITICAL / MAJOR / MINOR / SUGGESTION；未发现四项修复引入的回归。PV1 门禁在**各自受检 worktree 内**复跑全部 exit 0。

### 证据状态

| 检查 | 状态 | 结论 | 是否影响本轮判断 |
| --- | --- | --- | --- |
| `npm run check`（cwd=`.worktrees/wp1`） | 已执行，exit 0 | 十道门禁全绿；`check:schemas` 133 valid / 35 invalid，`check:assets` 181 fixture files；`openspec validate` 21/21 | 否 |
| `npm run check`（cwd=`.worktrees/wp2`） | 已执行，exit 0 | 十道门禁全绿；`check:schemas` 127 valid / 30 invalid，`check:assets` 170 fixture files；`openspec validate` 21/21 | 否 |
| `cargo test --locked -p sync-protocol --all-features`（`.target-wt/wp1`，cwd=`.worktrees/wp1`） | 已执行，exit 0 | 10 suite / 56 用例全绿，含 `schema_drift::snapshot_resources_match_schema_enum`、`envelope_fixtures`、`view_projections` | 否 |
| `cargo test --locked -p sync-protocol --all-features`（`.target-wt/wp2`，cwd=`.worktrees/wp2`） | 已执行，exit 0 | 10 suite / 52 用例全绿，含 `schema_drift::view_enums_match_schema`、`view_types_match_schema_defs`、`view_projections` | 否 |
| `cargo check --workspace --all-targets --all-features`（两侧各一次，补充） | 已执行，exit 0 | 12 crate 全编译，证明 M2 删除 `pub` 类型未打断任何外部消费者 | 否 |
| `cargo clippy --workspace --all-targets --all-features -D warnings`（两侧各一次，补充） | 已执行，exit 0 | 无警告 | 否 |
| `rustfmt --check --edition 2024 crates/sync-protocol/src/sync.rs`（WP1，补充） | 已执行，exit 0 | 删除未留格式偏差 | 否 |
| MU1 / MU2 / IV1 / AC1–AC3 / E2E | 待补（后续门禁） | 本轮不产生 | 否 |

### 说明（不计为问题）

- **`cargo fmt --all` 在 worktree 内报 workspace 归属错误**（`vendor/windows-local-ipc` 自称属于主 worktree 的 workspace）。这是 worktree 隔离的环境事实，与本 diff 无关；沿用 Round 2 的处理方式，改用 `rustfmt --check` 直检目标文件，exit 0。
- **`SessionReadResources.config_options` 与 schema 的形状不一致仍然存在**：Rust 是扁平的 `Option<Vec<ConfigOptionView>>`，schema `$defs/sessionItem.config_options` 是按会话归组的形状。该不一致在基线提交 `353ba6e` 上即已存在（`git show` 已核实），本轮修复既未引入也未声称修复它，M2 的删除也不构成对它的修复。按角色边界与「未在本轮判定」的范围约定，此处仅记录并保留 Round 1 的移交建议（建议主 Agent 单独派一轮修复），不计入本轮结论。
- **`schema_drift.rs` 第三个方向的枚举门禁缺口仍在**（新登记的 `AgentConnectedState`/`AgentDisconnectedState` 不在四条硬编码断言中）。这与 `review-wp2-r1-F3` 的判断一致：属 TP1 归属（`plan.md:867` 的 `crates/*/tests/`），WP2 不得自行修改，本轮亦未触碰该文件。按设计该漂移在当前版本无可达路径（Rust 侧无 crate 构造这两个类型），维持非阻断。
- **`valid/sync-snapshot-chunk-config-options.json` 的删除仍然正当**：其 `body.resource` 为 `config_options`，而该值已被 D1 移出 `sync.schema.json:232` 的封闭词表，保留必然与新契约冲突。这是 WP1 原始交付的一部分，本轮复核确认其正当性未变。

### 未在本轮判定的内容（角色边界）

- 夹具/用例集合是否齐备、风险盲区 → 主 Agent Coverage Index 与 validator。例：F5 修好后是否有负向夹具断言「`agent.connected` 带非空 `sessionId` 会被拒」——schema 层无此条件约束，故这不是缺陷，但也没有机器断言。
- `server::sync` 的服务端语义实现（默认页 20 条、重复 `before` 幂等、超上限收敛、`cursor_expired`）→ design 的既定 Non-Goals，属其它工作包。
- MU1/MU2/MU3 及 IV1/AC1–AC3/E2E 门禁 → 后续独立门禁，本报告不代替。

### 移交建议

- 四项 MINOR 无需再移交：M1/M2/M3/F5 均已闭环，可进入 MU1 候选验证。
- 保留（不阻断本轮）：`SessionReadResources.config_options` 的 schema↔Rust 形状不一致（基线既存）建议主 Agent 单独派一轮；`schema_drift.rs` 第三个方向的枚举断言交 TP1。
- 本报告不代替主 Agent 更新 `tasks.md` 的 2.1 / 2.2，也不宣称整个变更可归档。

PASS
