# Dependency Declaration Review — reviewer-plan (Round 5)

- Review ID：`reviewer-plan` · Round：5 · Phase / Stage：`phase: plan` / `stage: plan`
- Reviewer：独立只读 reviewer 子 Agent（非任何工作包 Owner；仅 `read`/`grep`/`glob` 及只读 git 查询，未执行构建或测试）
- 复核基线：Round 4 的 PASS 结论绑定 `sha256:4f87d0b0d429ca240158599354b4b4eeceebf702ce1771db0f758e18ec56b968`（`verification.md:41`）；本轮对象是**该摘要之后**的唯一契约变更。
- 角色路由说明：`openspec/agentic.yaml:7` 声明 reviewer → `deepseek/deepseek-flash`，宿主把 `reviewer`/`scout` 路由到配额受限模型（Round 2/3 前连续四次 `429`，见 `verification.md` 的 `## Failures and Retests`），本轮由当前会话模型承接 reviewer 职责；该偏离不改变判据与证据要求（与 Round 4 同一处置，见 `reports/dependency-declaration-review-round4.md:6`）。

## Verdict

**PASS**。无 BLOCKER、无 MAJOR。3 条 MINOR + 2 条 SUGGESTION 全部为**措辞/登记滞后**，不改变任何依赖声明、波次、所有权、覆盖行或红窗口结论；本轮可作为当前契约摘要的 PASS 结论使用。

## 本轮契约变更核对

### 变更内容（单点）

变更只落在一处：`specs/workspace-resolution/spec.md`。

1. `### Requirement: 创建时别名的持久化与投影来源` 正文新增一句「**别名与恢复所需的两列同门写入**：三列同属会话创建的那一次窄写提交，只有本次创建取得 Agent（ACP）侧会话标识时才一并落盘；未取得标识时三列同为 `NULL`……其目录归属同样不落盘、会话呈现为未分组」（`specs/workspace-resolution/spec.md:7`）。
2. 新增 `#### Scenario: 未取得 ACP 会话标识时三列同为空`（`:14-17`）。
3. `plan.md` 覆盖索引新增一行 `R53`（`plan.md:281-285`，`tasks: ["2.3","2.4"]`、`checks: [PV1]`）。

除此之外，`plan.md` 的 Work Packages（`:292-295`）、Execution Waves（`:303-306`）、Shared File Ownership（`:312-317`）、Dependency Handoffs（`:323-324`）、Runtime Resources（`:332-335`）、Merge Strategy Order（`:350`）、Verification Strategy（`:360-366`）与 `tasks.md`（`:14-21`）均无改动；`verification.md` 记录本轮之前的契约未变（`:50`「Check Plan Changes：无」）。

### 内部一致性：正文与新场景一致

- 正文：三列同属一次窄写提交，门限是「本次创建取得 ACP 侧会话标识」，未取得时三列同为 `NULL`，且「目录归属同样不落盘、会话呈现为未分组」（`specs/workspace-resolution/spec.md:7`）。
- 场景：`WHEN` 一次会话创建解析出了合法别名、但 Agent 侧没有回报 ACP 会话标识 → `THEN` 三列同为 `NULL`、目录归属未分组、`MUST NOT` 只写别名而留下另外两列为空（`:16-17`）。
- 场景为标准 `**WHEN**` / `**THEN**` 两行式，无第三种句式，无占位式断言（`:16-17`）；与正文的「同门写入」「三列同为 `NULL`」「归属不落盘」三点逐项对应，无相互削弱。**判定：一致。**

### 与已合并基线能力 `storage-schema-v2-migration` 的一致性

基线 `### Requirement: 恢复所需列的读写边界`（`openspec/specs/storage-schema-v2-migration/spec.md:118-120`）原文：「`agent_session_id` 只在会话创建取得 ACP 会话标识之后写入，`workspace_cwd` 只在创建时解析出规范化目录时写入；两者一旦写入 `MUST NOT` 被恢复流程改写」。

- 两条都是**必要条件**（「只在……才写入」），不是「一旦解析出就必须写入」的充分条件。因此新增的「别名与这两列同门」是**在同一必要条件集合上再加一条必要条件**，不与基线冲突：基线从未承诺「解析出目录就必须落 `workspace_cwd`」。
- 恢复只读义务（基线 `:120`、场景 `:122-125`）与本次新增的「恢复流程 `MUST NOT` 读写该别名」（`specs/workspace-resolution/spec.md:7`）同向，无矛盾。
- **判定：不矛盾（只做收紧，不做放宽）。** 可选的可读性增强（不要求）：在基线 `:120` 交叉引用同门口径，属文档增强而非冲突修复。

### 与本次变更内 `specs/storage-schema-v2-migration/spec.md` 新 requirement 的一致性

新 requirement「会话目录归属列的写入与解释」（`specs/storage-schema-v2-migration/spec.md:51-53`）正文同样是必要条件式表述：「只在解析出别名时写入」（`:53`）——**需求正文层面与同门口径不冲突**（同门只是再加一条必要条件）。

但其场景 `#### Scenario: 写入别名原文`（`:55-58`）的 `THEN` 是无条件的：「该会话行的 `workspace_alias` 与请求中的别名原文逐字节相同」。该场景的 `WHEN` 只要求「创建请求携带已登记且解析成功的 workspace 别名」，未要求取得 ACP 会话标识。因此对输入「别名已解析、Agent 侧未回报 ACP 标识」这一情形，本 delta 的 `THEN`（写别名）与 R53 的 `THEN`（三列同 `NULL`）给出互斥结论，且**该 delta 的需求正文不含门限、无法在单 delta 内消解**。记为 MINOR-1（见「发现的问题」第 1 条）。这不构成 BLOCKER：该情形的可执行判定由 R53 给出，且实际创建路径的正常用例（取得 ACP 标识）同时满足两者。

同 delta 内 `#### Scenario: 新列写入后可读回且不推导`（`:24-27`）的 `WHEN` 是「一个新建会话**在其创建流程中写入** `workspace_alias`」——以「已写入」为前提，不与门限冲突。`### Requirement: 版本常量与 migration 幂等` 中关于该列的两句（`:7`：`该列只在会话创建流程中写入` / `NULL` 解释为「没有目录归属」、不得反查）亦与 R53 同向。

### 未受影响的下游措辞

`sync-workspace-catalog` 的 `#### Scenario: 没有目录归属的会话`（`specs/sync-workspace-catalog/spec.md:18-21`）在括号里枚举了「创建时未解析出别名，或它早于本能力落地」两个原因。同门门限引入了第三个原因（别名已解析但无 ACP 标识 → 别名 `NULL` → `workspace` 为 `null`）。该场景的 `WHEN` 主句是**一般条件**「一个会话没有可用的持久化别名」，括号为非排他枚举，故 `THEN`（`workspace` 为 `null`、不猜测/补齐/反查）在新情形下自然成立。**判定：不需修改**，仅记为措辞增强建议。

### design.md 未受影响

`design.md@D6` 只写「`SessionUpdate` 写入字段（`workspace_alias`，与既有 `workspace_cwd` **同形的窄写入**）」（`design.md:105`）与「读取时 LEFT JOIN `owned_workspace` 取 `display_name`」（`:106`）；`D1` 只写「wire 上只传引用；会话行持久化创建时解析使用的别名」（`design.md:38`）。两者对「同门」保持沉默，而「同形窄写入」与「同一次提交同门落盘」不冲突。**design 无需改动**（design 属契约输入，改动会再次移动 `contractDigest`）。

### 与已实现侧的对应（只读核对，不作为判据）

本门限的措辞与 WP3 分支上的实现一致：`crates/core/src/broker.rs`（`feat/sync-core-projection`）在 `if let Some(agent_session_id) = endpoint.agent_session_id().cloned()`（`:1456`）这一个 `if` 内构造唯一的 `OwnedCommit`，其 `SessionUpdate` 同时带 `agent_session_id` / `workspace_cwd` / `workspace_alias`（`:1466-1470`）；`if` 之前先从 core 自己的解析结果取出 `workspace_alias`（`:1441-1443`），注释明写「`agent_session_id()` 为 `None`（未取得标识）时**三列都不写**」（`:1452-1455`）。即正文与场景描述的是既有实现行为，不是新引入的行为。

## 覆盖索引核对

对 `plan.md` 的 ```agentic-coverage``` 块（`plan.md:17-286`）与五份增量规范做了逐标题的机械比对（按 `source.path` + `source.heading` 原文配对）：

| 项 | 结果 | 证据 |
| --- | --- | --- |
| 行数 | **53 行**（`id: R1`–`R53`，ID 唯一无重复） | `plan.md:21`–`plan.md:285`；`grep -c "^  - id: R"` = 53 |
| 规范侧标题总数 | **53**（`### Requirement:` 13 条 + `#### Scenario:` 40 条） | 逐文件计数：`scope-expansion` 1/2、`storage-schema-v2-migration` 2/11、`sync-session-create` 5/10、`sync-workspace-catalog` 4/13、`workspace-resolution` 1/4 |
| 每个标题恰好一次 | missing = 0，dups = 0，extra = 0 | 配对结果：`workspace-resolution` 5 行（R33/R34/R35/R36/R53）、`storage-schema-v2-migration` 13 行（R40–R52）、`sync-session-create` 15 行（R18–R32）、`sync-workspace-catalog` 17 行（R1–R17）、`scope-expansion` 3 行（R37–R39） |
| 新场景已覆盖 | R53 精确指向 `#### Scenario: 未取得 ACP 会话标识时三列同为空` | `plan.md:281-285` |
| `target_ref` | `refs/heads/main`，真实存在且与计划记录一致（`6c093f145aa3…`） | `plan.md:19`、`plan.md:342`、`git rev-parse refs/heads/main` |
| `tasks` 引用 | 只引用 `2.1`/`2.2`/`2.3`/`2.4`，四者均存在于 `tasks.md` 且均以 `[PV1]` 声明 | `plan.md` 全块；`tasks.md:14`、`:16`、`:18`、`:20` |
| `evidence` | 全部为 `reports/PV1.log`；plan 阶段允许文件尚不存在 | `plan.md:25` 等；门禁口径见 `node_modules/@dongfanglin/openspec-agentic/src/workflow-check.mjs:1257`（`stage === 'plan'` 时 ENOENT 放行） |
| 标题歧义 | 五份规范内无同名 `### Requirement:`/`#### Scenario:` 标题，故无需 `source.requirement` 消歧 | 逐文件 `sort \| uniq -d` 全为空 |

**判定：覆盖索引块格式良好、行数一致、新场景已被覆盖且映射到正确的两个任务。**

需要主 Agent 在流程上完成的一件事（非计划缺陷）：`verification.md` 的 `## Dependency Declaration Review` 表（`:37-41`）当前最大 Round 是 4，绑定旧摘要。按门禁实现，`Plan Revision` 必须等于**当前** `contractDigest`（`workflow-check.mjs:566-568`），故本报告通过后需新增 Round 5 行并写入用当前计划重新计算出的摘要与本报告路径；摘要须由 `contractDigest` 计算得出，本轮只读约束下未执行该计算。

## 沿用结论复核

对 Round 4（`reports/dependency-declaration-review-round4.md:25-37`）逐条独立复核，结论全部维持：

| 声明 | 结论 | 本轮独立证据 |
| --- | --- | --- |
| WP4 依赖 `code:WP3`；WP1–WP3 `none` | CONFIRMED | `plan.md:295`（WP4 Dependencies = `code:WP3`）、`plan.md:292-294`（WP1/WP2/WP3 Dependencies = `none`）；`tasks.md:20` 要求 WP4 必须等 WP3 进入已验收集成基线，`tasks.md:9`（1.4）把该交接单列为任务 |
| 波次 W1 = WP1/WP2/WP3、W2 = WP4，与 Dependencies 一致 | CONFIRMED | `plan.md:303-306`；W1 内三个包依赖皆 `none`、W2 依赖 W1 的 WP3，无逆序 |
| 四条 Serialization Reason 全为 `NOT_APPLICABLE` | CONFIRMED | `plan.md:303`、`:304`、`:305`、`:306` 四行取值均为 `NOT_APPLICABLE` |
| `resource:R1`（`CARGO_TARGET_DIR`）可隔离 | CONFIRMED | 仓库根无 `.cargo/` 目录（`glob .cargo/**` → Path not found）；`Cargo.toml` 无 `[build]`/`target-dir`（grep 无命中）；`package.json:25` 的 `check:rust` 不设 target-dir → 无强制共享覆盖；`plan.md:332` 逐 WP 独立目录 + `Exclusive Scheduling = NOT_APPLICABLE`；实际占用已记录于 `verification.md:56`（`D:\Project\acp-remote-wt\target-wp<N>`） |
| `resource:R2`（`node_modules`）只读共享 | CONFIRMED | `plan.md:333`（只读共享、`NOT_APPLICABLE`、只读使用不写入）；`verification.md:56` 记录三份 junction 只读指向主仓库同一份 |
| 无 WP 的 Reviewer 等于其 Owner | CONFIRMED | `plan.md:292` coder-vocab / reviewer-vocab；`:293` coder-wire / reviewer-wire；`:294` coder-core / reviewer-core；`:295` coder-storage / reviewer-storage；且 reviewer-plan 独立于全部 Owner（`plan.md:297`），满足门禁 `owners.has(reviewer)` 不得命中的判定（`workflow-check.mjs:565`） |
| 无阻断性依赖环 | CONFIRMED | 依赖图只有一条边 `WP4 → WP3`，单向、无回边 |
| Shared File Ownership 覆盖全部多写者路径 | CONFIRMED（声明层面） | 按门禁的 `scopeOverlap` 语义（`workflow-check.mjs:103-106`）重算 WP 写范围两两重叠，只有三对：`WP1×WP2` 命中 `docs/SYNC_PROTOCOL.md` 与 `fixtures/sync/v1`（`plan.md:312`、`:313`），`WP3×WP4` 命中 `docs/CORE_PORTS_AND_STORAGE.md`（`plan.md:315`）；三对均有登记行。表另**多登记**了门禁并不要求的三行（`crates/server/src/node_link/` `:314`、`crates/sync-protocol/src/` `:316`、`fixtures/sync/v1/manifest.json` `:317`），属更严而非更松 |
| 登记区域互不相交 | CONFIRMED | `docs/SYNC_PROTOCOL.md` 两方改 §5.2/§11.3/§11.5/§16.2 与 §9.4/§9.6/§10.3（`plan.md:312`）；`fixtures/sync/v1/` 行以「本行**不含** `manifest.json`」开头与下一行机械不相交（`plan.md:313-317`）；`docs/CORE_PORTS_AND_STORAGE.md` 两方改 §3.1/§3.6/§5 与 §7/§9/§11.3，唯一共写区域（文件头 `> 版本：` 链）已在 Region Note 中显式声明并给定定稿方（`plan.md:315`）；实测该文件确有 ≥10 行版本链（`docs/CORE_PORTS_AND_STORAGE.md:4-24`） |
| Merge Order 覆盖全部写入者 | CONFIRMED | 四行的 `WP2 → WP1` / `WP2 → WP3` / `WP4 → WP3` 分别覆盖对应行的全部 Writers；与 `Merge Strategy` 的 Order 列 `WP2 → WP1 → WP4 → WP3`（`plan.md:350`）一致，构成无环拓扑序（WP2<WP1、WP2<WP3、WP4<WP3） |
| Re-verify 单元格引用计划的 Check ID | CONFIRMED | 六行 `Re-verify After Merge` 均为 `WP1: PV1` 或 `WP2: PV1`（`plan.md:312-317`），`PV1` 是覆盖行使用的 Check ID（门禁要求见 `workflow-check.mjs:299-300`） |
| 唯一登记的红窗口（WP3→WP4，`crates/storage-sqlite`）仍真实、仍指明 WP4 收口 | CONFIRMED | `crates/storage-sqlite/src/session_store.rs:454` 的 `SessionSummary::try_new(` 为**位置参数**调用（基线 9 参：`crates/core/src/model/session.rs:322-331`；WP3 分支改为 10 参、末尾增 `workspace: Option<WorkspaceRef>`：`feat/sync-core-projection:crates/core/src/model/session.rs:333-343`）→ E0063；`crates/storage-sqlite/tests/` 内 `SessionUpdate {` **全字段字面量共 15 处**（`commit.rs:132/267/817/904/1173/1208/1237`、`contract_v03.rs:258`、`enum_coverage.rs:403`、`migration.rs:1223`、`retention.rs:600/733`、`session_version_rule.rs:164/223/267`）→ E0063。两类文件均在 WP4 写范围内（`plan.md:295`），收口方为 WP4（W2）已写明（`plan.md:324`） |
| 三条冻结接口变更的每个构造点都有属主 | CONFIRMED | `SessionSummary::try_new` 调用点：`crates/server/src/node_link/command/tests.rs:95/286/2469` 与 `crates/server/src/node_link/resource/tests.rs:205` → WP3 写范围（`plan.md:294`）；`crates/storage-sqlite/src/session_store.rs:454` → WP4（`plan.md:295`）。`SessionUpdate {` 字面量：`crates/core/src/broker.rs`（WP3 写范围）vs `crates/storage-sqlite/tests/`（WP4 写范围） |
| wire 摘要投影构造点属主 | CONFIRMED | `crates/server/src/node_link/command.rs:2452-2478` 的 `session_summary()` 是唯一把 core 摘要映射到 sync wire `SessionSummary` 的函数（WP2 写范围，`plan.md:293`）；`crates/server/src/node_link/resource.rs:1221` 组装的是 node-link 自有的 `SessionMeta`（`:1231`）并只经 `SnapshotItemSessionMeta`（`:486`）进入 node-link 快照，与 sync wire 形状无关 —— 与 `plan.md:314` 的 Region Note 一致 |

## 阻断性依赖环

**无。** 依赖图为单边 `WP4 → WP3`（`plan.md:295`），WP1/WP2/WP3 无出边、无回边；Dependency Handoffs 两行（`plan.md:323`、`:324`）中只有 WP4←WP3 是真实下游交接，第二行是集成窗口的登记而非依赖边。

## 发现的问题

1. **MINOR｜跨 delta 场景互斥（契约保真）**：`specs/storage-schema-v2-migration/spec.md:55-58` 的 `#### Scenario: 写入别名原文` 的 `THEN` 无条件断言「`workspace_alias` 与请求中的别名原文逐字节相同」，其 `WHEN` 未含「取得 ACP 会话标识」；对「别名已解析但 Agent 侧未回报标识」这一输入，它与 R53（`specs/workspace-resolution/spec.md:14-17`）互斥，且该 delta 的需求正文（`:53`）不含门限、单 delta 内无法消解。
   *最小修复*：在 `specs/storage-schema-v2-migration/spec.md:53` 的需求正文补一句「该列与 `agent_session_id`/`workspace_cwd` 同门写入：只有本次创建取得 ACP 会话标识时才落盘」，或把 `:57` 的 `WHEN` 改为「……且本次创建取得了 ACP 会话标识」。二者择一即可，不需改 R53。

2. **MINOR｜同 delta 内场景措辞未带门限（可由正文消解，仅措辞）**：`specs/workspace-resolution/spec.md:9-12` 的 `#### Scenario: 创建写入别名与路径` 的 `WHEN` 同样只要求「携带来已登记且解析成功的 workspace 别名」。该情形可由**同需求正文**（`:7` 的「同门写入」）消解，故不构成矛盾，但同 delta 内两个场景对同一输入读起来会给出不同预期。
   *最小修复*：`:11` 的 `WHEN` 补「，且本次创建取得了 ACP 会话标识」。可与问题 1 一并处理。

3. **MINOR｜计划内 Scenario 计数滞后**：`plan.md:6` 写「共 13 条 Requirement / **39** 条 Scenario」，而五份增量规范现为 13 条 Requirement + **40** 条 Scenario（新增的 R53 场景未同步回该行）。13 条 Requirement 的计数仍正确。
   *最小修复*：`plan.md:6` 的 `39` 改为 `40`。注：该行属契约输入，改动会移动 `contractDigest` 并使本轮 PASS 过期 —— 见下方处置建议。

4. **SUGGESTION｜Work Packages 的 Goal 列未列新行**：`plan.md:294`（WP3 Goal = `R33–R36 的 core 侧`）与 `plan.md:295`（WP4 Goal = `R33–R36、R40–R52 的存储侧`）都未提及 `R53`，尽管覆盖索引已把 R53 正确映射到 `tasks: ["2.3","2.4"]`（即 WP3 + WP4）。这是描述性列与机器可读索引之间的措辞落差，不影响门禁（门禁只读 `tasks`/`checks`/`source`，`workflow-check.mjs:1222-1238`）。
   *最小修复*：WP3 Goal 改为 `R33–R36、R53 的 core 侧`，WP4 Goal 改为 `R33–R36、R40–R53 的存储侧`。

5. **SUGGESTION｜两处实现期写范围越界是否需要改计划（本轮显式判定）**：
   - `crates/sync-protocol/tests/envelope_fixtures.rs`（硬编码 manifest 用例数，`:25-29`）现为 **WP1 与 WP2 双写**，但该文件只登记在 WP2 的写范围（`plan.md:293` 的 `crates/sync-protocol/tests`），**Shared File Ownership 表中没有它**。`verification.md:23` 与 `:30` 已实证两分支分别取 `EXPECTED_VALID_MESSAGE_CASES = 64/63`、`EXPECTED_BODY_REJECTED = 7/7`（后者会被 git **静默自动合并**），并在 `verification.md:33` 给出了确定性合入规则（`67` / `9` / `envelope 2` / `pairing 5`，总数 83 = 67+11+5）且经 reviewer-wire-2 实测合并态全绿。
   - `crates/core/src/model/tests.rs:543` 是 WP3 因 `Session::summary` 签名变化（WP3 分支 `:543` 改为 `.summary(None)`）而必须改的调用点，同样不在 WP3 声明的写范围（`plan.md:294`）。
   - **判定：不需要在本轮改 `plan.md`，`verification.md` 的记录规则足够。** 理由：(a) 两者都不是依赖声明、不是多工作包所有权重叠（该文件不在任何 WP 的**声明**写范围内，故门禁的 `scopeOverlap` 重算也不会要求新增登记行——重算结果只有三对，见上表）；(b) `envelope_fixtures.rs` 的合入结果由确定性数字规则唯一确定，且该规则已被独立 reviewer 实证（52 passed / 0 failed），不留歧义；(c) `model/tests.rs` 只有 WP3 一个写者，不存在合并协调需求；(d) 关键差别在于——本轮触发复审的是**规范语义变更**（`specs/**` 是 `contractDigest` 的输入，`workflow-contract.mjs:41-45`），而这两处是实现期文件归属事实，契约语义未变；现在改 `plan.md` 会**再次**移动 `contractDigest`，使刚取得的本轮 PASS 立即过期并需要 Round 6。
   - **必须做的事（下一版计划，不在本轮）**：按 `verification.md:33` 已作出的承诺，把 `crates/sync-protocol/tests/envelope_fixtures.rs` 登记进 Shared File Ownership（Writers = WP1, WP2；Merge Order `WP2 → WP1`；Re-verify `WP1: PV1`；Region Note 写明共用常量区与 `verification.md:33` 的 67/9 收口规则），并把 `crates/core/src/model/tests.rs` 补进 WP3 的 Write Scope。

问题 1、2 属于契约文本的一致性收口，与问题 3、4 一样都会移动 `contractDigest`。若主 Agent 选择一次性修完这四条再复审，应把它们合并成**一次**计划/规范修订并重跑一轮依赖声明审查（Round 6），而不是分多次改；本轮 PASS 只对**当前**摘要有效。