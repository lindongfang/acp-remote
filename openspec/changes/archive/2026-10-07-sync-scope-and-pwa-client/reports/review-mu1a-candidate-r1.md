```agentic-handoff
version: 1
task_id: "6.4"
role: reviewer
phase: merge
agent_context:
  agent_id: "ReviewerMu1aCandidateR1"
  isolation: "fork_turns=none（新建独立 reviewer，未参与 WP1/WP2 的实现、修复、组装或检视对话；仅接收调度方传入的角色契约、检视范围与版本 SHA）。本轮为只读检视。"
target_revision: "ad9ad3c6b532ce0cc9366c8cc34a217f9f466933"
scope: "MU1a 候选（mode=merge）自 base b964ae313abb8b83fc9acb02fbc062448c5c23bc 到候选 ad9ad3c6b532ce0cc9366c8cc34a217f9f466933 的合并结果。重点四轴：(1) 共享写点的冲突解决；(2) 溯源正确性（候选每文件逐字等于「最后写入方」版本）；(3) 候选相对 base 的增量完整性（== WP1 ∪ WP2，不多不少）；(4) b964ae3 构建修复的传递。两个源交付（WP1 6779c36/合入点 8265bf3、WP2 e5a31dc）各自已单独 PASS 独立 review，本轮不重复其内部正确性检视。"
changes: "只读检视，未修改任何文件；仅新增本报告。未切换分支、未提交、未合并、未删除分支或 worktree、未运行构建/测试，仅在临时索引上执行一次 git merge-tree。"
checks:
  - id: "E1-PROVENANCE"
    scope: "候选 29 路径逐条与 base / WP1 合入点 8265bf3 / WP2 e5a31dc 的 blob 三方比较"
    command: "git rev-parse <rev>:<path> 三方比对（29 路径全量）"
    result: "PASS——28 路径逐字节等于其唯一写入方版本；1 路径（docs/SYNC_PROTOCOL.md）为两写者并集，无第三语义。"
  - id: "E2-MERGE-TREE"
    scope: "查询式复算合并结果（临时索引，不写工作区、不改引用）"
    command: "git merge-tree --write-tree 8265bf3 e5a31dc"
    result: "PASS——exit 0、无冲突报告、树 6b206e6717449bcad64b00d1d674db44ba6768aa 与候选 ad9ad3c 的树逐字相同；merge-base(8265bf3,e5a31dc) = b964ae3。"
  - id: "E3-INCREMENT"
    scope: "候选相对 base 的路径集合 == WP1 ∪ WP2"
    command: "git diff --name-only b964ae3 <rev> 的集合比较"
    result: "PASS——WP1 23 + WP2 7 = 29 去重后 29，候选 29；missing 0 / extra 0。"
  - id: "E4-MANIFEST"
    scope: "manifest.json 条目不重不漏、排序保持（硬要求）"
    command: "node 解析四版本 manifest 做集合/顺序包含性运算"
    result: "PASS——base 83 / WP1 94 / WP2 83 / 候选 94；WP1 +12/−1；WP2 +0/−0；候选 == base∪WP1∪WP2 去重后的合法集合（唯一差即 WP1 的删除项）；重复 0；候选顺序与 WP1 顺序逐项相等（JSON 数组相等）；base 去删除项后为候选子序列。"
  - id: "E5-DOC-SPLIT"
    scope: "docs/SYNC_PROTOCOL.md 的 hunk 归属与接缝"
    command: "git diff <revA>:<path> <revB>:<path> 的 hunk 级 diff"
    result: "PASS——候选 vs WP1 恰 1 hunk（§10.3，位于第 1001 节之后）；候选 vs WP2 恰 6 hunk（§3.3 第 103 行、§9.4 第 736/762/783/792 行、§11.5 第 1283/1305 行），全部为 WP1 自有编辑。§3.3 的 `limit` 与 `chunkCount` 编辑逐字保留。"
  - id: "E6-FIXTURES"
    scope: "fixtures/sync/v1/valid/ 前缀划分"
    command: "git diff --name-only b964ae3 <wp> -- fixtures/sync/v1/valid/"
    result: "PASS——WP1 10 项全为 sync-*/command-*；WP2 3 项全为 view-*；交集为空；候选 valid/ 78 文件、invalid/ 16 文件。"
  - id: "E7-VENDOR"
    scope: "b964ae3 构建配置修复的传递"
    command: "git rev-parse <rev>:vendor/windows-local-ipc/Cargo.toml 与 git merge-base --is-ancestor"
    result: "PASS——候选 blob a7ff9b35384da06569b1c514bd23c9ba7a840782 == base == WP1 合入点 == WP2 交付提交；353ba6e 与 b964ae3 均为候选祖先；候选相对 base 无 vendor 路径 diff（未被回退或改写）。"
  - id: "E8-COUNT-GATE"
    scope: "envelope_fixtures 计数常量（WP1 × TP1 共享写点）在候选上的自洽性"
    command: "node 解析候选 manifest 分类计数"
    result: "PASS——候选 manifest 94 条；pairing 5；message 域 89（valid 73 / invalid 16）；16 条非法中 2 条在信封层被拒（EXPECTED_ENVELOPE_REJECTED=2），其余 14 条落在 body 层 → 与 73/14 两个常量逐一相等。常量本身未被候选改写（该文件候选 blob == WP1 blob）。"
  - id: "E9-CROSS-BOUNDARY"
    scope: "契约值跨边界的消费侧分派"
    command: "git grep 消费侧分派点（schema_drift / client / node-link schema）"
    result: "PASS——`SnapshotResource::ALL` 与 sync.schema.json 的 enum 由 tests/schema_drift.rs 的门禁双向约束（该门禁在 base 已存在且候选未改，故非空转）；sync-protocol crate 无生产侧构造器，消费方由 TS 客户端挂在 TP3；schemas/node-link 未引用 sync schema，分页键的隔离成立。"
issues: "0 CRITICAL / 1 MAJOR / 0 MINOR（对候选交付物本身）。另 1 项 MAJOR（MU2A-R1-F2）针对仓库状态而非候选内容，按角色指令登记但不阻断候选。"
result: "PASS（候选 ad9ad3c6b532ce0cc9366c8cc34a217f9f466933 无阻断缺陷）"
evidence_paths:
  - "openspec/changes/sync-scope-and-pwa-client/reports/review-mu1a-candidate-r1.md"
  - "openspec/changes/sync-scope-and-pwa-client/reports/PV1.log"
  - "openspec/changes/sync-scope-and-pwa-client/reports/merge-mu1a-r1.md"
```

# MU1a 候选独立检视（merge 类型，Round 1）

## 1. 检视输入与版本

| 项 | 值 |
| --- | --- |
| Base Revision | `b964ae313abb8b83fc9acb02fbc062448c5c23bc`（`refs/heads/main`） |
| Target Revision | `ad9ad3c6b532ce0cc9366c8cc34a217f9f466933`（候选 `merge/mu1a-candidate-r1`） |
| 候选父提交 | `906ec42`（第一段合入，父 `b964ae3` + `8265bf3`）、`e5a31dc`（WP2 交付） |
| WP1 交付 / 合入点 | `6779c36678376f02b95a4071b62dca809b221a8b` / `8265bf3` |
| WP2 交付 | `e5a31dc05ea55015446c19bfce9675f72f2ddc7b` |
| 增量 | 29 路径 / +746 / −222（`git diff --stat` 复核一致） |
| PV1 证据 | `reports/PV1.log`，sha256 `fa308ab101ca3b745cf3a085baf0d6ae173d659c9b200089cae25987adf0de01`，1815 行（独立复算 hash 与行数，均一致） |

PV1 为执行者证据，本报告不重复跑 Project Verify；仅核验其存在性、哈希与行数，并对其中与合并相关的读数（计数门禁）做独立复算。

## 2. merger 自报事实的逐条独立结论

| # | merger 自报 | 独立结论 | 依据 |
| --- | --- | --- | --- |
| 1 | 两次 `git merge --no-ff`，共 29 paths / +746 / −222 | **成立** | `git diff --stat b964ae3 ad9ad3c` = 29 files / +746 / −222；`git log --format='%H %p'` 显示候选有两个父，906ec42 的父为 `b964ae3`+`8265bf3`（顺序与 Merge Order 一致）；候选的两个父的 merge-base 均为 `b964ae3` |
| 2 | manifest 候选集合 = base 83 −1 +WP1 新增 12；WP2 零新增；无重复；base 删除项后顺序为候选子序列 | **成立** | 实测 base 83 / WP1 94 / WP2 83 / 候选 94；WP1 adds 12、removes 1；WP2 adds 0、removes 0；重复条目 0；`JSON.stringify(cand) === JSON.stringify(wp1)` 为真（顺序逐项相等，强于子序列）；base 去删除项后为候选子序列为真 |
| 3 | 候选 vs WP1 仅差 1 hunk（§10.3）、vs WP2 差 6 hunk（§3.3/§9.4/§11.5）；§3.3 的 `limit` 与 `chunkCount` 完整保留 | **成立** | hunk 级 diff 实测：vs WP1 = 1 hunk（第 1001 节 `### 10.3` 之后，即 §10.3 表格与三段散文）；vs WP2 = 6 hunk（第 103 行 §3.3、第 736/762/783/792 行 §9.4、第 1283/1305 行 §11.5），均为 WP1 自有编辑；第 103 行同时含 `limit`（Sync 面 `session.read` 的可选页大小）与 `chunkCount` 两项原文 |
| 4 | `EXPECTED_VALID_MESSAGE_CASES=73` / `EXPECTED_BODY_REJECTED=14` 在候选上自洽 | **成立** | 独立复算：候选 manifest 94 条，pairing 5 条（schema 非 message）、message 域 89 条（valid 73 / invalid 16）；16 条非法中 `ping-without-connection` 与 `sequence-is-number` 在信封层被拒（=2），其余 14 条落在 body 层（=14）。常量文件候选 blob `0e65e8b` == WP1 blob，未被候选改写；PV1 第 914–928 行的 `every_manifest_case_behaves_as_declared ... ok` 与之吻合 |
| 5 | `views.rs` == WP2 blob、`sync.rs`/`command.rs` == WP1 blob，逐字节相同 | **成立** | `views.rs` 候选/ WP2 = `03a2eed766624a23c63f64827e15fc937938cddc`（WP1 侧为 base 值 `c230141…`）；`sync.rs` 候选/WP1 = `6cbfc535f7c68d9dfc64f133fb5a173ecb37fb6c`；`command.rs` 候选/WP1 = `a87aac085d6a3350f7d37018021ef7b69788edcb` |

五条自报事实**全部成立**，无一夸大。

## 3. 四个重点的结论

### 3.1 共享写点的冲突解决——**PASS**

- **`fixtures/sync/v1/manifest.json`（WP1, WP2, TP1）**：见 §2 第 2 条，条目不重不漏、排序保持（候选顺序与 WP1 逐项相等）。WP2 对该文件零写入（候选 blob `54f5418…` == WP1 blob），故不存在叠加语义。
- **`docs/SYNC_PROTOCOL.md`（WP1, WP2）**：见 §2 第 3 条。两写者落点分处不相交小节（WP1：§3.3/§9.4/§11.5；WP2：§10.3），候选为二者的**逐字并集**，未出现第三种语义、无冲突标记、无重复小节（标题唯一、无重复长行）。
- **`fixtures/sync/v1/valid/`（WP1, WP2）**：前缀划分实测成立（WP1 的 10 项全为 `sync-*`/`command-*`，WP2 的 3 项全为 `view-*`，交集为空）。
- **`crates/sync-protocol/src/views.rs` 与 `crates/*/tests/`（WP2, TP1）**：本单元无 TP1 参与，`crates/sync-protocol/tests/` 下唯一被改的 `envelope_fixtures.rs` 为 WP1 侧（WP2 未动，其 blob 在 WP2 侧 == base）。
- **`crates/sync-protocol/src/{command,sync}.rs`（WP1 独占）**：WP2 侧 blob 均 == base，未越界。
- **`schemas/sync/v1/event-views.schema.json`（WP2 独占）**：候选 blob `4a8898b…` == WP2 blob；WP1 侧 blob == base（`0864b6b…`）。**WP1 的目录级 Write Scope 未污染该文件**——区域收窄确实生效，只有 WP2 的 9 行改动（`file.changed` 三个可选字段、两个 `state` 收成封闭枚举）。
- 反向越界同样为零：WP2 侧对 `docs/NODE_LINK_PROTOCOL.md`、`crates/sync-protocol/tests/envelope_fixtures.rs`、`schemas/sync/v1/sync.schema.json` 的 blob 均 == base。

### 3.2 溯源正确性——**PASS**

候选 29 路径逐条三方比对（base / WP1 合入点 / WP2）结果：**28 路径逐字节等于其唯一写入方版本**，`docs/SYNC_PROTOCOL.md` 为两写者并集，无路径落到「既非 WP1 也非 WP2」的第三状态。写入方归属与 plan.md 的登记逐条一致（`views.rs`/`event-views.schema.json`/`ACP_COMPATIBILITY_MATRIX.md`/`view-*` → WP2；其余 → WP1）。

机械化复核更强：`git merge-tree --write-tree 8265bf3 e5a31dc`（查询式，临时索引，不写工作区、不改引用）exit 0、无冲突、树 `6b206e6717449bcad64b00d1d674db44ba6768aa` **与候选 ad9ad3c 的树逐字相同**。即候选内容 == 两个源提交的机械三路合并结果，未经任何人工改写。两段合入提交的树亦分别等于 `merge-tree(b964ae3, 8265bf3)` 与 `merge-tree(f64a196, b964ae3)`。

### 3.3 增量完整性——**PASS**

`{候选相对 base 的路径} == {WP1 相对 base 的路径} ∪ {WP2 相对 base 的路径}`：WP1 23、WP2 7、并集 29、候选 29，missing 0 / extra 0。候选相对 base 的 29 路径全部落在 WP1/WP2 的 Write Scope 与三个共享写点之内，无第四类来源。

### 3.4 b964ae3 构建配置修复的传递——**PASS**

`vendor/windows-local-ipc/Cargo.toml` 在候选上的 blob 为 `a7ff9b35384da06569b1c514bd23c9ba7a840782`，与 base、WP1 合入点 `8265bf3`、WP2 交付 `e5a31dc` **逐字节相同**；候选相对 base **无该路径的 diff**（既未被回退也未被改写）。`git merge-base --is-ancestor 353ba6e b964ae3` 与 `--is-ancestor b964ae3 ad9ad3c` 为真，两条交付分支各自并入该修复的路径均为 `--no-ff` 且机械复算通过。空 `[workspace]` 表在候选树中存在。

## 4. 发现

### MU2A-R1-F1（MAJOR，候选交付物）

**候选丢失 `crates/sync-protocol/tests/schema_drift.rs` 的 `view_enums_match_schema` 第三方向硬编码断言**

- **位置**：`crates/sync-protocol/tests/schema_drift.rs`（候选 blob `fed60ea86fb8a5d723b9ed85bab6ed8188337da5`，与 base **逐字节相同**）；受影响契约文件 `schemas/sync/v1/event-views.schema.json`（候选第 328–344 行两个 `state` 封闭枚举），blob `4a8898b2557af8fa17ccfb8209115a54b524925b`。
- **触发条件**：以 `crates/sync-protocol/src/views.rs::VIEW_ENUMS` 为基准、对照 `event-views.schema.json` 逐条核验第三方向断言（`ALL`/`as_str` 等值断言）时成立。
- **预期**：WP2 新增了 `event-views.schema.json` 中两个新的封闭 `enum`（`agent.connected.state` 与 `agent.disconnected.state`），Rust 侧新增对应镜像 `AgentConnectedState`/`AgentDisconnectedState` 并已在 `views::VIEW_ENUMS` 登记，则 `schema_drift.rs` 的等式列表应同步纳入这两个类型，使「schema 有枚举 ⇔ Rust 有镜像」双向门禁对新枚举同样生效。
- **实际**：`schema_drift.rs` 未被本单元任何一方改动（候选 blob == base blob）。其第三方向硬编码断言仍只覆盖改动前的类型集合；新增的两个状态枚举只由 schema↔`VIEW_ENUMS` 的登记一致性约束覆盖，未进入等式断言。
- **影响**：未来若 `AgentConnectedState`/`AgentDisconnectedState` 的 `ALL`/`as_str` 与 schema 漂移，本门禁不会报错——这正是该测试自身注释所针对的失效模式（「schema 里的封闭枚举与 Rust 镜像之间没有判据时，两侧可以静默漂移」）。候选本身语义正确（枚举取值与 schema 逐条相等，见 `views.rs:61` 与 §3.1），故缺陷是**门禁覆盖漏洞**，不是当前值错误。
- **独立依据**：这与前序 WP2 检视的 F3 记录一致（`reports/review-wp2-r1.md`），并被处置为「归 TP1，由 MU1b 落地」（见 `verification.md` 的 `## Review Findings` 与 plan.md 的 `crates/sync-protocol/tests/envelope_fixtures.rs` 共享行）。因此该缺失由合并引入候选状态，**修复责任在后续单元 MU1b/TP1**，候选不因此在合并语义上失败。
- **建议**：在 MU1b（TP1）中将两个新状态类型补入 `schema_drift.rs` 的等式断言集合，与 plan.md 已登记的 `envelope_fixtures.rs` 计数常量同批落地；本条不阻断候选合入。

### MU2A-R1-F2（MAJOR，针对仓库状态；按角色指令登记，不以此判候选 FAIL）

**主检出 `D:\Project\acp-remote` 处于未完成的合并状态，含 23 个 `docs/**` 路径的巨额删除，且三项产品文件暂存**

- **位置**：`D:\Project\acp-remote`（`git rev-parse HEAD` = `b964ae313abb8b83fc9acb02fbc062448c5c23bc`，即未移动）；`.git/MERGE_HEAD` 存在且内容为 `8265bf3ecae82785480c1eff0ffe350b0cea144e`（即 WP1 合入点）。
- **触发条件**：`git status --porcelain` / `git diff --cached` 读主检出状态时可见。
- **预期**：plan 的 Shared File Ownership 规定三处共享写点的 Merge Owner 为 merger，Merge Order 为 WP1 → WP2；候选已按该顺序组装并固定在分支 `merge/mu1a-candidate-r1`，主检出 `refs/heads/main` 应停在 `b964ae3` 且工作区干净（merger 自报「主检出产品跟踪文件零改动」）。
- **实际**：主检出工作区有 23 个 `docs/**` 路径被删除（其中 `docs/diagrams/` 三个文件共 −15508 行，`docs/` 目录在文件系统上已不存在）；已暂存（`git diff --cached --stat`）的恰好是 **WP1 相对 base 的 23 个路径 / +585 / −209**，其中 `crates/sync-protocol/src/{command,sync}.rs`、`schemas/sync/v1/{command,sync}.schema.json` 与全部新增 fixture 已进索引。唯一解释是主检出处于**中断的 `git merge`**：`MERGE_HEAD == 8265bf3` 表明这是把 WP1 交付合入主检出的操作，而 `docs/**` 的删除是索引/工作区路径解析或外部干扰造成的派生损坏。该状态的最终提交会产出一棵既非候选、又把仓库全部 `docs/` 从跟踪中删除的树，并会把 WP1 的中间态（而非候选）写进 `main`。
- **影响**：若不清理而继续，`main` 会把 WP1 的中间态（不含 WP2 的派生事件字段）而非候选 `ad9ad3c` 落盘，同时丢失整个 `docs/` 树（`check:docs` 依赖的 518 个 markdown 文件）。这是对交付物的破坏性风险，且与 MU1a 的唯一实质风险点（冲突解决/合入顺序）直接相关。
- **依据边界**：本 reporter 为只读检视，不执行 `git merge --abort`/`reset`/`checkout` 等状态修改命令。因此**只报告，不处置**。
- **给主 Agent 的必要动作**：在固定候选后、启动 premerge 门禁与静默合入之前：(1) 记录现场（`git status --porcelain`、`git diff --cached --stat`、`cat .git/MERGE_HEAD`）；(2) 将主检出恢复到 `b964ae3` 且工作区仅含既有未跟踪项（`.target-wt/`、`.worktrees/`、变更目录）的状态；(3) 复核 `docs/` 全部文件在位且 `git diff --stat` 无删除。**在完成上述清理之前，本候选不应进入合入步骤。**

### 非阻断观察（不计入 finding，供调度者参考）

1. `openspec/changes/sync-scope-and-pwa-client/verification.md` 的 `## Merge History` 与 dispatcher 的本轮说明冲突：前者记候选分支 `merge/mu1a-candidate-r1` 且 `Merge Commit = NOT_APPLICABLE（未合入）`，merger 6.6 小节称已以条件更新把候选分支合入主分支。实测 `refs/heads/main` 仍为 `b964ae3`（未移动），故未合入成立；`## Merge History` 需按实际候选分支名与最终结果订正。
2. `reports/PV1-main-mu1a.log` 内容为占位文本「主分支回归日志占位：合入后写入（6.7）。」，而 merger 报告的 `MAIN-REGRESSION-6.7` 行将其记为 `BLOCKED / 0`。归为「未执行」（与「结论」小节一致），但该 `exit_code` 字段填 `0` 易被误读为已执行，建议后续改为 `NOT_APPLICABLE`。
3. merger 自报候选 worktree `.worktrees/mu1a-merge` 与分支已回收。实测 `git worktree list` 仍有 `.worktrees/mu1-merge`（`683dbbb`）、`.worktrees/tp1`、`.worktrees/wp1`、`.worktrees/wp2`；`mu1a-merge` 已不在其中，与自报一致。候选提交 `ad9ad3c` 现由分支 `merge/mu1a-candidate-r1` 固定，可从悬空对象恢复——该分支在 premerge PASS 前不应删除（与 `verification.md` 已登记的「候选保全」教训一致）。
4. `verification.md` 的 `## Checks` 表中 `CANDIDATE-ASSEMBLE-6.2` 行以 `merge/mu1a-candidate`（无 `-r1`）记录候选分支名，而实际固定分支为 `merge/mu1a-candidate-r1`；属台账文本偏差，不影响候选内容。

## 5. 结论

**PASS** —— Target Revision `ad9ad3c6b532ce0cc9366c8cc34a217f9f466933`。

- 候选相对 base 的合并结果**逐字等于**两个源交付的机械三路合并（树 `6b206e67…` 相同），无人工改写、无伪冲突、无第三语义，无冲突标记。
- 四个重点（共享写点冲突解决、溯源正确性、增量完整性、b964ae3 传递）全部 PASS。
- 候选交付物本身的唯一 MAJOR（`MU2A-R1-F1`）是**门禁覆盖漏洞而非值错误**，且已按既有登记归 TP1/MU1b 落地，**不阻断候选合入**。
- **不列入候选判定**的 `MU2A-R1-F2`（主检出的中断合并 + `docs/**` 删除 + WP1 暂存）针对**仓库状态**而非候选内容，按角色指令「只报告、不处置」登记；但它是合入步骤的**前置清理条件**，主 Agent 必须先恢复主检出，再启动 premerge 与合入。若在清理前推进，会以 WP1 中间态覆盖 `main` 并删除 `docs/` 树。

按 `roles/reviewer.md` 的判定规则：候选交付物无未解决的 CRITICAL/MAJOR，判 **PASS**；`MU2A-R1-F1` 与 `MU2A-R1-F2` 作为非阻断项/仓库状态项列出。

## 6. 实际检查范围

已检查（只读）：

- base→候选完整 diff（29 路径）与 diff stat；候选两个父提交与 merge-base。
- 29 路径逐条三方 blob 溯源（base / 8265bf3 / e5a31dc / ad9ad3c）。
- `git merge-tree --write-tree 8265bf3 e5a31dc`（临时索引，未写工作区、未改引用）与候选树比对；两段合入提交的树与各自源-基线机械合并比对。
- `fixtures/sync/v1/manifest.json` 四版本集合/顺序运算（条目不重不漏、排序保持、WP2 零写入）。
- `docs/SYNC_PROTOCOL.md` hunk 级三向 diff（vs WP1 1 hunk、vs WP2 6 hunk）与接缝逐字核验；候选文档的重复标题/重复长行/冲突标记扫描（0 命中）。
- `fixtures/sync/v1/valid/` 前缀划分与候选夹具目录计数（valid 78 / invalid 16）。
- `vendor/windows-local-ipc/Cargo.toml` 四版本 blob 与祖先关系。
- `envelope_fixtures.rs` 四个计数常量与候选 manifest 的独立分类复算（73/14/2/5 全部吻合）。
- 跨边界消费侧分派：`sync::SnapshotResource::ALL` ↔ `sync.schema.json#/$defs/snapshotResource.enum` 的 `schema_drift.rs` 双向门禁（base 已有、候选未改）；`SessionReadResult` 消费点（`command.rs` 内解析 + 测试）；`schemas/node-link` 对 sync schema 的零引用（分页键隔离成立）。
- `AGENTS.md` §3/§4 相关不变量与依赖方向的定向核对（未越 crate 边界）。
- 主检出当前状态（`git status --porcelain`、`git diff --cached`、`.git/MERGE_HEAD`）。
- `reports/PV1.log` 的哈希、行数及计数门禁用例读数；`reports/merge-mu1a-r1.md` 的自报事实逐条比对。

## 7. 未验证内容

- 未执行任何构建、测试、lint 或 Project Verify（按角色指令交调度者）。`npm run verify` 在候选上的通过性仅作为执行者证据引用，未复现。
- 未执行 E2E。
- 未验证 `reports/PV1.log` 是否确由候选 `ad9ad3c` 的工作区产出（仅核验文件哈希、行数与其中计数门禁读数的自洽性）。
- 未审查 WP1/WP2 交付内部的正确性（已由 `review-wp1-r1.md`、`review-wp2-r1.md`、`review-wp2-r2.md` 独立覆盖）；本报告仅复核其二者的合并组合语义与溯源。
- 未评估 `docs/**` 删除事件对 `check:docs`/`check:agentic` 在**主检出**上的具体影响（该影响取决于主 Agent 清理时的恢复方式），亦未尝试清理（只读边界）。
- 未核验 `merge/mu1a-candidate-r1` 分支的创建时间与来源对象是否即为 merger 组装的候选（只核验其树内容 == 机械合并树）。
- 未验证 `reports/merge-mu1a-r1.md` 中关于 6.6 premerge 门禁、6.7 主分支回归的**结果性陈述**（后者证据文件为占位符，已登记为观察）。
