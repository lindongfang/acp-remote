# DU1 合入后最终版本独立复核（tasks.md `6.8`；Review ID `RvDu1Merge`）

## 固定字段

- **task_id**：`6.8` [DU1]（合入后独立检视合并新增差异；本轮 Review ID `RvDu1Merge`）
- **role**：独立代码检视子 Agent。未参与实现、未参与实现讨论、未参与 W1/W2 四份 RV1、未参与 `DU1-R1`/`DU1-R2`
- **agent_context**：新建子 Agent（`fork_turns=none` 的等效隔离），上下文只有调度者交办的本模板、`§Plan` 与仓库只读访问；只声明收到的输入与自身限制，不声称能证明宿主未注入其它上下文
- **只读边界**：唯一写入是本报告；未运行 formatter/linter/全量测试套件，未 `commit`/`merge`/`push`/`checkout`/`stash`/`worktree add`；唯一的执行类动作是两次「干净树解出 + 该树自己的 `check-doc-links.mjs`」探针与一条 `cargo test` 单用例（见 §1）
- **被检范围**：`git diff 601c8ae..86f282b`（交办指定；范围内带代码的提交只有 `62ef264`），另核实合入面 `37a398e..86f282b` 与工作区 `HEAD`
- **基线与合入**：`refs/heads/main` 由 `37a398e` 条件更新（快进）到 `86f282b`；`git rev-parse HEAD main` 均为 `86f282bf545ea7839e961e09481d0784005ce420`；`git merge-base --is-ancestor 37a398e 86f282b` 退出 0（确为快进，无额外合并提交）
- **结论**：**`correct`** —— 无 P1/P2；`DU1-R1-F1` 未回归；`62ef264` 未引入新的 P1/P2
- **issues**：2 条 P3（均为「合入后记录债」，非代码缺陷；见 §7）

## 1 被检提交范围与执行的命令

范围与组成（`git log --oneline --reverse 37a398e..86f282b`、`git diff --name-status 601c8ae..86f282b`）：

```text
86f282b docs: 修正最终验收块的提交后复跑结论            ← 仅记录
0b5fa80 docs: 记录 DU1 集成验证与第一轮最终验收(BLOCKED)  ← 仅记录
62ef264 fix(core): 按 DU1 复核补强前置校验的判别力与错误分层  ← 范围内唯一带代码的提交
601c8ae fix(repo): 修正干净检出下的文档门禁与 export/import 引用前置校验（DU1-R2 的被检版本）
```

`git diff --name-status 601c8ae..86f282b` 的全部条目：

```text
M crates/core/src/use_cases.rs
M openspec/changes/admin-state-persistence-v2/tasks.md
M openspec/changes/admin-state-persistence-v2/verification.md
A openspec/changes/admin-state-persistence-v2/reports/{verify-final-round1.log,wp6-admin-store-tests.log}
A reports/{clean-tree-check.log,du1-integration.md,parked-idle-identity-retention.patch,
          rv1-du1-r2.md,verify-du1-fixes-2.log,verify-final-round1.log}
```

即范围内 `docs/**`、`scripts/**`、`Cargo.toml`、`Cargo.lock`、`fixtures/**` **零改动**；`crates/**` 只动 `crates/core/src/use_cases.rs`。`git diff --name-only 62ef264..86f282b -- crates Cargo.toml Cargo.lock scripts docs` 输出为空 —— 合入 `main` 的两个记录提交**没有**任何代码差异。

实跑的命令与结果：

| 命令 | 结果 |
| --- | --- |
| `git diff 601c8ae..86f282b`（+ `-- crates docs scripts Cargo.toml Cargo.lock`、`--name-status`、`--name-only`） | 见上；`use_cases.rs` 为 +56/−3 |
| `git log --format='%h %p %s' 37a398e..86f282b`、`git merge-base --is-ancestor 37a398e 86f282b` | 13 个提交线性续接；退出 0（快进） |
| `git archive 86f282b \| tar -x -C <tmp>` 后于该树内 `node scripts/check-doc-links.mjs` | `md=89`、`.omp` 文件 0、`doc links OK: 366 relative links, 2287 section refs across 89 markdown files`、**EXIT=0** |
| 同法对 `62ef264` 重放（复核 `reports/clean-tree-check.log` 的自述） | `doc links OK: 366 relative links, 2275 section refs across 88 markdown files`、**EXIT=0**，与该日志逐字一致（本机工作区含宿主未跟踪 `.md` 时为 104） |
| `cargo test -p core --all-features export_and_import_preconditions_are_enforced` | `1 passed; 0 failed; 77 filtered out`（合入后的工作树，即 `86f282b` 的代码） |
| `git ls-tree -r --name-only <rev> \| grep -c '\.md$'`（`601c8ae`/`62ef264`/`86f282b`） | 87 / 88 / 89（与记录里的 88 一致） |

## 2 `62ef264` 实际改了什么（语义判定）

两处改动，都在 `crates/core/src/use_cases.rs`：

**(a) `UseCases::add_import` 的 owner 前置改为三分支错误分层**（`:719-749`）。原实现把「`TrustStore::node(id, NodeKind::Owner)` 返回 `None`」与「返回的行角色/状态不符」合成一个 `_` 臂，统一报 `InvalidRequest("import owner node must be a paired owner node on this node")`。新实现：

- `Some(node) if node.state() == NodeState::Paired` → 通过（不变）；
- `Some(_)` → `InvalidRequest("...paired owner node...")`（角色正确但状态非 `paired`）；
- `None` → 追加一次 `TrustStore::nodes_for(id)`：非空（该 node id 存在、但没有任何 `owner` 角色行）报 `InvalidRequest("...must hold the owner role...")`；为空（该 node id 在本机不存在）报 `NotFound(EntityRef::Node(id.clone()))`。

`match` 是穷尽的显式臂（`Some` 带守卫 + `Some(_)` + `None`），**没有**通配臂吞掉新取值。

**(b) 用例 `export_and_import_preconditions_are_enforced`**（`:1531`、`:1553`、`:1612-1651`）：① 在「别名未登记」用例前先把 `agent_ref` 推进 `catalog_agents`；② 在下一个用例前 `clear()` 目录；③ 把「owner 节点缺席」的断言从 `InvalidRequest` 收紧为 `NotFound`；④ 新增「`Owner` + `pending`」反例，并在接受用例前用 `retain` 清掉该 node id 的全部角色行。

未触及的东西（据此可判定回归面）：`put_export` 的别名/Agent 校验（属 `601c8ae`，本轮未动）、`settle_pairing` 的按族审计动作（属 `86ae8b4`）、`create_session` 的授权顺序（属 `86ae8b4`）、任何 trait 签名、任何 DDL、`CORE_PORTS_AND_STORAGE.md` §5.3/§7/§9 正文。

## 3 前置校验的判别力

「校验真的能拒绝目标输入」——逐条对实现求值（`crates/core/src/use_cases.rs:714-749`）：

| 输入 | 实现路径 | 结果 |
| --- | --- | --- |
| node id 在本机不存在 | `node(id, Owner)`→`None`；`nodes_for(id)`→`load_nodes_for` 的 `WHERE node_id = ?1` 返回空（`crates/storage-sqlite/src/admin/trust.rs:212-223`）→ `NotFound(EntityRef::Node)` | 拒绝（正确分层） |
| 该 id 只有 `access` 角色行 | `node(id, Owner)`→`None`（`load_node` 带 `AND kind = ?2`，`admin/trust.rs:202`）；`nodes_for` 非空 | 拒绝（`InvalidRequest`，角色错） |
| `owner` 角色但 `pending`/`revoked` | `Some(node)`，守卫不成立 | 拒绝（`InvalidRequest`，状态错） |
| `owner` + `paired` | 第一臂 | 放行 |
| Export 引用未登记 alias / 目录里没有的 Agent（`put_export`，本轮未动） | `self.config.workspace(alias)` / `catalog.agents()` | 拒绝 |

「会不会误拒合法输入」：三个拒绝分支都以存储层的真实语义为判据（`owned_node` 的 `(node_id, kind)` 唯一行 + `state` 列），没有依赖内存状态或时间；`NotFound` 只在 `nodes_for` 返回空时产生，而 `nodes_for` 与 `node()` 读的是同一张表、同一份 id 文本（`NodeId::as_str`），因此不存在「`node()` 说没有、`nodes_for` 也说没有、但库里其实有」的错杀面。额外的一次读只发生在**错误路径**上，且失败即向上返回（fail closed），不改变成功路径的写集与事务边界（仍是一次 `ExportStore::add_import` 调用）。

## 4 错误分层是否与 §5.3 / `broker::port_error_public` 一致

- **无通配臂**：`broker::port_error_public`（`crates/core/src/broker.rs:2502-2540`）对 `NotFound(_)` 有显式臂（`command.not_found`、`retryable=false`），对 `ConflictKind`/`UnavailableKind` 逐值列出并注明「不用通配臂」。新增的 `NotFound(EntityRef::Node(..))` 落在既有显式臂上，不会静默落入 `internal.unavailable`。
- **与本地管理合同一致**：`docs/LOCAL_ADMIN_PROTOCOL.md:561` 把「目标 `deviceId`/`nodeId`/`pairingId`/`exportId`/`importId` 不存在」定为 `local.not_found`；`:461` 只要求「已配对且 `kind = owner`」，未规定这类拒绝的具体码。因此「缺席 → `NotFound`」「角色/状态不符 → 参数类错误」既不违背 §5.5/§6，也与 `DU1-R2-F3` 的建议方向一致。
- **与存储层同形**：`admin/trust.rs:505` 的 `revoke_node` 对缺席节点同样返回 `NotFound(EntityRef::Node(write.node.clone()))`，core 与适配器的实体引用形状一致（`EntityRef::Node` 存在、有 `kind() → "node"` 与 `target_id()`，`crates/core/src/model/ids.rs:498/513/540`），不会出现「同一实体两种引用形状」的映射分叉。
- **登记一致**：`verification.md` 第 36 条如实写明「缺席报 `NotFound(EntityRef::Node(..))`、存在但角色或状态不对报参数类错误」，并注明 alias 一侧 core 没有对应 `EntityRef` 变体、映射义务在适配器（残留，见 §8(3)）。

## 5 用例判别力：能否在旧实现下失败

该用例没有 `#[should_panic]`，也没有裸 `is_err()`：三条拒绝断言分别匹配**具体变体**（`InvalidRequest(_)` / `NotFound(_)`），且每个输入在用例里只可能被一个前置拒绝（依据列见下）。逐条假想回退的结果：

| 假想回退 | 用例行为 | 是否拦住 |
| --- | --- | --- |
| 删除 `put_export` 的别名循环 | 步 1（Agent 已在目录、别名未登记）不再报错 → `expect_err("an unregistered workspace alias must be refused")` panic | **是**（本轮修好 `DU1-R2-F1`：修前空目录会让 Agent 前置顶掉它） |
| 删除 `put_export` 的 Agent 存在性校验 | 步 2（别名已登记、目录已清空）不再报错 → `expect_err("an unknown agent must be refused")` panic | 是 |
| 别名判定反向（`is_none()`→`is_some()`） | 步 3（别名已登记且 Agent 在目录）报错 → `expect("a fully resolvable export must be accepted")` panic | 是 |
| 整体删除 `add_import` 的 owner 前置 | 步「absent owner node」不再报错 → panic | 是 |
| 把 kind 过滤换成只看状态（即只用 `nodes_for`） | 步「access-role node」不再报错 → panic | 是 |
| 删除 `nodes_for` 消歧、`None` 一律报角色错（= `62ef264` 之前的行为） | 步「absent owner node」得到 `InvalidRequest`，而断言要求 `NotFound` → panic | **是**（本提交新增的判别力：错误分层这一半被钉住） |
| 反过来把 `None` 一律当缺席 | 步「access-role node」得到 `NotFound`，断言要求 `InvalidRequest` → panic | 是 |
| 删掉 `state() == NodeState::Paired` 守卫（`Some(_) => {}`） | 步「`Owner` + `pending`」不再报错 → `expect_err("an unpaired owner node must be refused")` panic | **是**（本轮修好 `DU1-R2-F2`） |
| 把状态判定放宽成「非 `revoked` 即可」 | `pending` 仍被拒 → 三个断言全过 | **否**（残余：`revoked` 的 owner 行没有单独用例；无实现分支可据以区分，见 §8(4)） |

结论：`62ef264` 的三条修复各自都有了「删掉修复即变红」的回归网；未拦住的只剩「`revoked` 状态」这一条边缘（`pending` 已覆盖「必须 `paired`」的语义，`revoked` 与 `pending` 走同一 `Some(_)` 臂，不构成独立行为分支）。

## 6 回归判断：前置 review 的阻断项与发现

### 6.1 `DU1-R1-F1`（唯一 MAJOR 阻断项：干净检出上 `check:docs` 必红）—— **未回归**

用与 `DU1-R1` 完全相同的方法（`git archive <rev>` 解出**只含被跟踪文件**的树 → 在该树内跑该树自己的 `scripts/check-doc-links.mjs`）实测：

- `aed9fb5`（`DU1-R1` 的被检版本）：`AGENTS.md:224` 缺 `.omp/commands/commit.md` → `1 problem(s)`、`EXIT=1`；`601c8ae`（`DU1-R2`）：`EXIT=0`；
- **`62ef264`（本范围内版本）**：88 个 `.md`、`366 relative links / 2275 section refs`、`EXIT=0`（`reports/clean-tree-check.log` 自述逐字可复现，其 revision/取树命令/计数三项自洽 ⇒ `DU1-R2-F4` 的「证据标签」问题也已真正修好）；
- **`86f282b`（合入后的最终版本）**：89 个 `.md`（多出 `reports/du1-integration.md`）、`366 relative links / 2287 section refs`、`EXIT=0`。

即合入新增的两份 `.md`（`reports/rv1-du1-r2.md`、`reports/du1-integration.md`）没有引入断链、坏锚点或悬空 `§` 引用，`npm run check` 的第七道门禁在合入后的树上仍然成立。

### 6.2 `DU1-R2` 的 5 条非阻断发现 —— 逐条闭合核对

| ID | 本轮核对 | 结论 |
| --- | --- | --- |
| `DU1-R2-F1`（别名断言的判别力被 Agent 前置顶掉） | 用例步 1 之前已把 `agent_ref` 放进目录（`:1531-1545`），并在步 2 前 `clear()`；见 §5 第 1 行 | 已闭合 |
| `DU1-R2-F2`（`paired` 那一半无用例） | 新增 `Owner` + `pending` 反例并在接受用例前 `retain` 清理（`:1628-1651`）；见 §5 第 8 行 | 已闭合 |
| `DU1-R2-F3`（错误码分层 / 未登记） | owner 节点缺席改为 `NotFound(EntityRef::Node(..))`（`:746-748`），并被用例断言钉住；残余的 alias 一侧已登记进 `verification.md` 第 36 条 | 已处置（残余属适配器义务，见 §8(3)） |
| `DU1-R2-F4`（`clean-tree-check.log` 标签与内容不符） | 日志重写后含 revision、取树命令、`.omp/` 缺失与 `.md` 计数；本轮实测与自述一致（§1 表第 3 行） | 已闭合 |
| `DU1-R2-F5`（`verification.md` 第 16 条保留与第 25 条相反的从句） | 该从句已划线并加「已被第 25 条取代」指针（`verification.md:63`），所指第 25 条存在且内容相符 | 已闭合 |

### 6.3 更早的阻断项（`WP23-2`、`WP6-*`、`WP6B-*`）—— 不可能回归

范围里唯一被改的实现文件是 `crates/core/src/use_cases.rs`，且改动只落在 `add_import` 与一个 `#[cfg(test)]` 函数内；`settle_pairing`（`WP23-2` 的落点）、`create_session`、`put_export`、`crates/storage-sqlite/**`、`Cargo.toml`/allow-list、`fixtures/**`、合同 §5.3/§7/§9 正文在本范围内**逐字未变**（§1 的 `--name-status` 与 `git diff --name-only 62ef264..86f282b -- crates …` 为空为证）。

## 7 发现

### `RvDu1Merge-F1`（P3）合入已发生，但记录里的合入提交与验收目标未回填

- **位置**：`openspec/changes/admin-state-persistence-v2/verification.md:294`（「交付候选…**尚未合入 main**（`6.6` 未执行）」）、同文件 `:222`（`target_commit: "62ef264…"`）、`:12`（「**未合入 main**（缺授权）」）；`tasks.md:58-60`（`6.6`/`6.7`/`6.8` 仍未勾选）；`reports/du1-integration.md:23`（「合入后 `HEAD` 预期等于候选 SHA」）。
- **为什么是问题**：`refs/heads/main` 现在已是 `86f282b`（= 候选 `62ef264` + 两个记录提交），工作区 `HEAD` 与 `main` 同为 `86f282b`。于是：`6.6` 的完成条件（「实际合入提交记录在 `verification.md`」）未满足；机器可读的 `agentic-assessment.target_commit` 与 HEAD 不一致，正是 `workflow check --stage final` 那 10 条错误里「`验收结论的 target_commit` 已失效」「当前代码 `HEAD` 与计划目标引用不一致」两条的来源；`reports/du1-integration.md:23` 的「预期等于候选 SHA」与事实相反（真实合入是快进到 `86f282b`，候选不是 `main` 的 tip）。这会让 `6.7`/`8.1` 的所有者按错误的引用去做主分支复验，或把「候选未合入」当成仍然成立的阻塞理由。
- **最小修复方向**：在 `Merge History` 追加一段「`refs/heads/main` 由 `37a398e` 条件更新（快进）到 `86f282b`；`62ef264..86f282b` 无代码差异」；把 `agentic-assessment.target_commit` 改为最终主分支提交（若验收要求被检目标保留为候选，则在紧邻正文写明候选是 `main` 的祖先且两者代码等价）；勾选/回填 `6.6`，并把 `reports/du1-integration.md:23` 的预期改为实际结果。
- **备注**：这是「记录滞后于合入」的流程债，不是代码缺陷；`62ef264` 提交时该陈述为真。

### `RvDu1Merge-F2`（P3）Final Assessment 的散文行与同段机器块不同步

- **位置**：`openspec/changes/admin-state-persistence-v2/verification.md:287`（`- Audit / Evidence: …仍未做：5.1–6.5（集成就绪与候选构造）…`）与 `:290`（`- Assessment ID / Time: admin-state-persistence-v2-w1w2-review-closure`），对照 `:221`（机器块 `assessment_id: "admin-state-persistence-v2-du1-closure"`）。
- **为什么是问题**：同一段里两套验收 ID，且 `5.1`/`5.2`/`6.1`–`6.5` 在 `tasks.md:48-56` 已勾选并附执行记录（只有 `6.6`–`6.8` 未完成），因此散文行给出的「未完成集合」比实际多 7 项；`8.1` 最终验收若引用散文行，会得出错误的验收 ID 与验收范围。此前的 `DU1-R1` F6 与 `DU1-R2` F5 处理的就是同类「同一文件自相矛盾」。
- **最小修复方向**：把 `:284-288` 的 ID 与未完成清单改成与机器块、`tasks.md` 一致，或加「本行已被下方 DU1 段取代」的指针。

## 8 未覆盖范围与限制

1. **未跑项目级门禁**：未执行 `cargo fmt --check`、`clippy`、全量 `cargo test`、`check:drift`、`check:boundaries`、`npm run check`、`workflow check`（交办明确限定；结构一致性由门禁绑定，且本范围未触碰 §5.3 rust 块/§7 sql 块/allow-list）。本报告只对「单用例行为」与「合入树上的文档门禁」给出了实测证据，其余按 `reports/verify-du1-fixes*.log` 采信。
2. **合入面的真实盲区**：快进把 `37a398e` 之后的 13 个提交全部带进了 `main`，其中 `5d77f25`（+535 行合同、`docs/IDENTITY_AND_AUTH_CONTRACT.md`、`.github/workflows/ci.yml`、`fixtures/local-admin/**`）、`7cdff57`、`28f8cb9` 三个**文档提交不属于本单元组成**，且此前各轮的 diff 范围要么以 `5d77f25` 为基（`DU1-R1` 的 `git diff 5d77f25 aed9fb5`）、要么以 `5404610` 为被检（W1/W2），因此「它们自身相对 `37a398e` 的改动」没有任何一轮 review 覆盖。我对合入后的整树跑了文档门禁（退出 0），并确认这三个提交不碰 `crates/**` 与 `Cargo*`（本单元提交才是 `crates/**` 的改动来源）；但它们确实改了 `scripts/check-command-catalog.mjs`、`scripts/check-schema-fixtures.mjs`、`.github/workflows/ci.yml` 与 `openspec/config.yaml`（`git diff --name-only 37a398e 28f8cb9` 中的非文档条目），这几个文件不在任何一轮 review 的 diff 范围内，本报告也未读其内容 —— 它们的门禁效果仍被后续各次全绿日志覆盖（日志跑在其后代提交上、含同一份脚本），但**其合同正文与脚本语义未由本报告复核**。
3. **登记在案的残留（非缺陷）**：`put_export` 的「alias 缺失 / Agent 缺失」仍报 `InvalidRequest`，而 `LOCAL_ADMIN_PROTOCOL.md:440/561` 要求适配器映射为 `local.not_found`；core 侧没有 workspace 的 `EntityRef` 变体（§3.1 的枚举受漂移门禁约束），适配器只能按消息文本区分两类失败。该义务已写进 `verification.md` 第 36 条，且是 `DU1-R2-F3` 之后的既定处置，不计为缺陷。
4. **`revoked` owner 状态**：无用例、无独立实现分支（与 `pending` 同臂），见 §5 最后一行；不影响本次判定。
5. **未复核**：夹具两次重生成哈希（需跑 `#[ignore]` 生成器）、`reports/parked-idle-identity-retention.patch`（930 行，本轮起随合入进入版本库；其内容语义不在本单元代码内）的内容、`identity-auth`/`identity-keystore`/`server`/`app`/`node-link-client` 等未实现 crate、E2E（mode `not-applicable`）、以及 `workflow check --stage final` 的当场状态（该命令会写记录缓存，超出本报告只读边界）。
6. **本轮不代替 Project Verify**：结论只覆盖上述被检范围与实测项；`6.7`（主分支复验）、`7.x`（替代验证）、`8.1`（最终验收）仍待其所有者执行。

## 9 结论

- **`correct`**：`git diff 601c8ae..86f282b` 中唯一带代码的提交 `62ef264` 只做了两类改动 —— 把 `add_import` 的 owner 前置从「单臂 `InvalidRequest`」改为「缺席 `NotFound(EntityRef::Node(..))` / 角色错 `InvalidRequest` / 状态错 `InvalidRequest`」的显式三分支，以及给用例补上能各自钉住这三条前置的输入。合入的两个提交没有任何代码差异（快进，`62ef264..86f282b` 对 `crates`/`Cargo*`/`scripts`/`docs` 零改动）。
- **无新的 P1/P2**：错误分层与 `broker::port_error_public`（`NotFound` 显式臂，无通配）、`LOCAL_ADMIN_PROTOCOL.md` §6（`local.not_found` 覆盖「目标 `nodeId` 不存在」）、存储层 `revoke_node` 的实体引用形状三处一致；判别力方向的两个错误实现（一律 `NotFound` 或一律 `InvalidRequest`）都会被用例拦下；新增的 `nodes_for` 读只在错误路径、失败即返回，不改变成功路径的写集。
- **`DU1-R1-F1`（唯一 MAJOR 阻断项）未回归**：干净树上 `check-doc-links.mjs` 在 `62ef264`（88 个 `.md`）与合入后的 `86f282b`（89 个 `.md`）上都退出 0，与 `aed9fb5` 上的 `1 problem(s)`/`EXIT=1` 形成正反对照；`DU1-R2` 的 5 条非阻断发现全部闭合（§6.2）。
- **两条 P3 记录债**（`RvDu1Merge-F1`/`F2`）：合入已发生但 `verification.md` 仍写「未合入 main」、`agentic-assessment.target_commit` 仍是候选、`tasks.md` 的 `6.6`–`6.8` 未勾选，且 Final Assessment 的散文行与机器块 ID/未完成清单互相矛盾。两者都不影响代码正确性，但会让 `6.7`/`8.1` 依错误的引用执行、并继续触发最终阶段的「`target_commit` 已失效」类错误，建议随合入记录一并回填。
