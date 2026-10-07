# Dependency Declaration Review — reviewer-plan (Round 8)

- Review ID：`reviewer-plan` · Round：8 · Phase / Stage：`phase: plan` / `stage: plan`
- Reviewer：独立只读子 Agent（非任何工作包 Owner；本轮仅用 `read`/`grep`/`glob`/`git diff --name-only`/`git log`/`git show --stat`，未执行构建、测试、`npm run` 或 `workflow check`）
- 复核基线：Round 7 的 PASS 结论绑定 `sha256:08669be58a99bae6bdcae2c415fc4f4324580caef22f0611ad14314de627b5ba`（`verification.md:61`）；本轮不从该摘要继承任何结论，全部重新取证
- 交付分支：`feat/sync-workspaces-and-create`，`HEAD = fad57bb`；远端基线 `origin/main = 8bf11ca`，其相对规划基线的两个新增提交为 `8ac2e68`（#42）与 `8bf11ca`（#43），`git merge-base 6c093f1 origin/main = 6c093f1`
- 声明的待审变更：`plan.md:342`（`## Target Repository and Main Branch` 的 `Target Ref` 条目）与 `verification.md:126`、`verification.md:225`（Merged Commit / `target_commit` 改指交付 HEAD，附「上游基线推进事件」记录）

## Verdict

**PASS**。本轮 `plan.md` 的改动是**单行事实订正**（目标引用从「规划时提交 `6c093f1…`」改为「规划基线 `6c093f1…` + 交付期间远端被 #42/#43 推进、已并入最新 `origin/main` 并在新基线上重跑」），**范围受限、语义保持**：工作包、波次、写入归属表、依赖交接、运行资源、合并策略、覆盖索引与 Main E2E 决策逐字未动。上游两个提交与本变更的实现文件集**只交于 `README.md` 一个文件**，且两处编辑位于不相邻行、均完整存活于合并树；四个共享区域（`docs/SYNC_PROTOCOL.md`、`docs/CORE_PORTS_AND_STORAGE.md`、`fixtures/sync/v1/**`、`crates/sync-protocol/src/**`）被 #42/#43 **零触及**，已登记的非重叠结论继续成立。`code:WP3` 声明在合并树上仍属实。无 BLOCKER / MAJOR；新增 1 条 MINOR、2 条 INFO；Round 5–7 的 SUGGESTION-5 原样承接。

## 本轮变更核对

### 变更的物理位置：工作区未提交改动

`git status --porcelain` 仅两项：

```
 M openspec/changes/sync-workspaces-and-create/plan.md
 M openspec/changes/sync-workspaces-and-create/verification.md
```

因此本轮三处改动是**工作树相对 `HEAD=fad57bb` 的未提交差异**，不在任何已提交树中。`git log --oneline -- .../plan.md` 只返回 `bcc5e10`，说明 `plan.md` 自 `bcc5e10` 起只被创建过一次；上一轮读到的「规划时提交 `6c093f1…`」是 `bcc5e10` 里的内容，`fad57bb` 未再改 `plan.md`。

### 1) `plan.md`：单行、单条目

`git diff -- plan.md` = **1 insertion / 1 deletion**，无其它 hunk：

```
@@ -339,7 +339,7 @@ rows:
-- Target Ref: `refs/heads/main`（规划时提交 `6c093f145aa3d69dcd573a6d94e31b692acb5d4b`）
+- Target Ref: `refs/heads/main`。规划基线为 `6c093f1…`；交付期间远端被 #42（补记 PWA 原型）与 #43（`build(deps)` 升级 openspec-agentic 0.3.0 → 0.4.0）推进，本地已并入最新 `origin/main` 并在新基线上重跑 Project Verify 与 final/archive（见 `verification.md` 的「上游基线推进事件」）
```

- 位置 `plan.md:342`，位于 `## Target Repository and Main Branch`（`plan.md:339`–`344`）内。
- 该节其余三条（`Code Repository` `:341`、`Version Confirmation Owner` `:343`、`Confirmation Method / Evidence` `:344`）逐字未动。
- 语义判定：**事实订正，不是范围变更**。旧值断言「目标引用 = `6c093f1`」，在 #42/#43 合入后已不成立（`refs/heads/main` 的实际推进点为 `8bf11ca`，交付线并入点为 `00211d3`）；新值把「规划基线」与「交付时实际基线」分列，并指向 `verification.md` 的对应记录，未新增任何任务、工作包、依赖或范围。`Coverage Index` 的 `target_ref: refs/heads/main`（`plan.md:19`）仍成立，未随之变动。

### 2) 未动的部分（逐项确认）

以下各节与上轮一致，`git diff` 无任何 hunk 落入：

| 节 | 行 | 结论 |
| --- | --- | --- |
| `## Work Packages`（含 Dependencies / Contract Freeze / Owner / Role / Reviewer / Write Scope / Verification） | `plan.md:288`–`295` | 未动 |
| `## Execution Waves` | `plan.md:299`–`306` | 未动 |
| `## Shared File Ownership`（6 行） | `plan.md:308`–`317` | 未动 |
| `## Dependency Handoffs` | `plan.md:319`–`326` | 未动 |
| `## Runtime Resources`（`R1`/`R2`） | `plan.md:328`–`337` | 未动（但见发现 1） |
| `## Merge Strategy` | `plan.md:346`–`354` | 未动 |
| `## Coverage Index`（R1–R53） | `plan.md:15`–`286` | 未动 |
| `### Main E2E`（含 `downgrade_approval`） | `plan.md:379`–`390` | 未动 |
| `## Independent Validation` / `## Failure and Recovery` / `## Completion Criteria` | `plan.md:392`–`408` | 未动 |

`design.md`、`proposal.md`、`tasks.md`、`specs/**`（5 份增量规范）在工作树中均无改动（`git status` 只列出 `plan.md`、`verification.md`），故全部 Contract Freeze 指针仍指向同一内容：`design.md` 的 D1–D8 标题仍在 `design.md:36/54/64/79/91/97/110/116`（D9 在 `:134`）。

### 3) `verification.md`：两处版本指针 + 一段记录

`git diff -- verification.md` = **2 insertions / 2 deletions**（`00211d3…` → `fad57bb…`），另有一段「上游基线推进事件」散文与新证据行 `reports/main-verify-0.4.log` 已在 `HEAD`（`fad57bb`）中：

- `verification.md:126`：`## Merge History` 的 M1 行 `Merged Commit` 由 `00211d3b…` 改为 `fad57bb8905a…`（`Candidate Commit` 仍为 `a6f6210b…`）。
- `verification.md:225`：`agentic-assessment` 的 `target_commit` 同步改为 `fad57bb8905a…`。
- `verification.md:169` / `:226` 的 `contract_digest` 仍为 `sha256:08669be5…`，未被本轮改动触碰 —— 这是**正确**的：契约内容（`proposal.md`/`specs/**`/`design.md`）本轮未变，摘要不应变（见下文「重绑定」）。

## 上游推进对依赖声明的影响

### 文件集 (a)：本变更的实现文件集

`git diff --name-only 6c093f1..60af6e0`（候选并入主分支那一步，剥离 `openspec/changes/**`）= 52 个文件，覆盖 WP1–WP4 声明写范围的全部实际落点：`compatibility/**`、`schemas/sync/v1/**`、`crates/{sync-protocol,node-link-protocol,identity-auth,core,server,storage-sqlite}/**`、`docs/{SYNC_PROTOCOL,SECURITY_DESIGN,FRONTEND_DESIGN,DEVELOPMENT_PLAN,MODULE_ARCHITECTURE,CORE_PORTS_AND_STORAGE}.md`、`fixtures/sync/v1/**`、`README.md`。

### 文件集 (b)：#42/#43 触碰的文件

```
8ac2e68 (#42): README.md, prototypes/README.md
8bf11ca (#43): .github/dependabot.yml, AGENTS.md, package.json, package-lock.json,
               openspec/.agentic-install.json, openspec/schemas/agentic/**（14 个文件）
```

### 文件集 (c)：三者交集

**(c-1) 实现文件集 ∩ 上游文件集 = `{ README.md }`**（`comm -12` 结果唯一一项）。

- `README.md` 属 **WP1 声明写范围**（`plan.md:292` 末项 `README.md`）。上游确实落进了 WP1 的写范围，这是本轮必须显式交代的一点。
- 区域不重叠，逐行核实：`git diff -U0` 显示 WP1 侧为 `@@ -15 +15 @@`（改写第 15 行 `sync-protocol` crate 表行）与 `@@ -27,0 +28,2 @@`（在 `session.resume` 段后插入两行「切片 7 的 Sync 合同…」）；#42 侧为 `@@ -25,0 +26,2 @@`（插入 `prototypes/` 段落）。两个插入点在基线行号上相隔 2 行、非相邻、无共同上下文行，git 自动合并且**无需冲突解决**。
- 合并树取证：`README.md` 现同时含 `prototypes/` 段与「切片 7 的 Sync 合同」段，二者内容均与各自提交一致。**WP1 对 `README.md` 的写入既未丢失也未被上游覆盖，WP1 的写范围声明仍然属实。**
- WP1 对 `README.md` 的依赖方向不受影响：WP1 与 WP2/WP3 之间没有任何经由 `README.md` 的读写耦合（`plan.md:326`「WP1 与 WP2 只经冻结契约耦合，不互读代码」），故 #42 的插入不制造新的 WP1→WP2/WP3 依赖。

**(c-2) 四个共享区域 ∩ 上游文件集 = ∅**（严格核验）

`git diff --name-only 6c093f1..origin/main -- docs/SYNC_PROTOCOL.md docs/CORE_PORTS_AND_STORAGE.md fixtures/sync crates/sync-protocol/src crates/server crates/storage-sqlite crates/core compatibility schemas` 输出为空。为防遗漏，把范围放大到全部 `crates/**`、`schemas/**`、`compatibility/**` 仍为空。此外逐文件核对：

| 共享区域（`plan.md` 行） | 声明的 Writers | #42/#43 是否触及 | 非重叠结论 |
| --- | --- | --- | --- |
| `docs/SYNC_PROTOCOL.md`（`:312`） | WP1, WP2 | 否 | 仍成立（WP1 的 §5.2/§11.3/§11.5/§16.2 与 WP2 的 §9.4/§9.6/§10.3 划分未被外部插入打乱） |
| `fixtures/sync/v1/` 向量文件（`:313`） | WP1, WP2 | 否 | 仍成立 |
| `crates/server/src/node_link/`（`:314`） | WP2, WP3 | 否 | 仍成立 |
| `docs/CORE_PORTS_AND_STORAGE.md`（`:315`） | WP3, WP4 | 否 | 仍成立（含文件头版本链的共写区域） |
| `crates/sync-protocol/src/`（`:316`） | WP1, WP2 | 否 | 仍成立 |
| `fixtures/sync/v1/manifest.json`（`:317`） | WP1, WP2 | 否 | 仍成立 |

#43 的 `openspec/schemas/agentic/**` 是 **agentic 工具自身的受管 schema**（tooling 自描述），与本变更的 `openspec/changes/sync-workspaces-and-create/specs/**` 是不同路径层，未被触及；`openspec/changes/**` 不在上游文件集中（`git diff --name-only 6c093f1..origin/main` 无 `openspec/changes/` 条目）。因此**没有任何共享归属区域被外部改动侵入**。

**(c-3) 规划产物 ∩ 上游 = ∅**：`design.md`、`proposal.md`、`tasks.md`、`specs/**` 在上游 diff 中均不出现；`verification.md` 是本变更自有文件，不在上游侧。

### 依赖声明逐条复核（对合并后的树）

1. **`code:WP3`（`plan.md:295` WP4 依赖列；`plan.md:323` 交接行）—— 仍属实。** 合并树上 WP3 的端口形状确已落地：`WorkspaceRef` 定义于 `crates/core/src/model/ids.rs:430`；`Session::summary(workspace: Option<WorkspaceRef>)` 在 `crates/core/src/model/session.rs:293`（第 290–292 行注释明示聚合不持有别名、必须由调用方给出）；`SessionSummary.workspace` 在 `crates/core/src/model/session.rs:327/343/358/407-409`；broker 三个投影点均已传参（`crates/core/src/broker.rs:4893/5105/5171`，统一走 `self.world.workspace_ref(&state, …)`）。WP4 侧确已消费该形状：`crates/storage-sqlite/src/session_store.rs:58/63/68` 的投影 SQL 以 `s.workspace_alias` 为唯一来源并 `LEFT JOIN owned_workspace` 取展示名，`:470-481` 明确 `NULL` 即「未分组」不反查，`:1128-1168` 为恢复路径的窄写规则。**`plan.md:323` 记录的失效条件（WP3 端口形状或 `docs/CORE_PORTS_AND_STORAGE.md` §5 变化）在本轮均未触发。**

2. **WP1–WP3 依赖列为 `none` —— 仍独立。** 三者均只经 `design.md@D1–D8` 的冻结契约耦合；上游对 `crates/**`、`schemas/**`、`compatibility/**`、`docs/SYNC_PROTOCOL.md` 的零改动意味着冻结契约的**实现面**未漂移，唯一交点 `README.md` 不在任何耦合路径上（见 c-1）。

3. **`contract` 冻结指针 —— 仍解析。** `design.md` 工作树无改动，D1–D8 标题位置见上；13 条 Requirement / 39 条 Scenario 与 Coverage Index R1–R53 的 `source.heading` 引用的规范标题未被上游触碰（`specs/**` 不在上游文件集内）。

4. **`resource:R1`（构建目录 `CARGO_TARGET_DIR`，`plan.md:332`）—— 仍属实。** 上游未改 `rust-toolchain.toml`、未改任何 `Cargo.toml`/`Cargo.lock`、未引入容器/端口/数据库；每 WP 一个 `target-wp<N>` 的隔离前提未变。

5. **`resource:R2`（`node_modules` 只读共享，`plan.md:333`）—— 前提已变，见发现 1。** #43 改写了 `package.json`（`"@dongfanglin/openspec-agentic": "0.3.0"` → `"0.4.0"`）与 `package-lock.json`（4 行增删），因此「`npm ci` 已完成；存在性已由 1.1 核实」这一在规划基线（0.3.0 lock）下核实的事实不再覆盖当前 lock。主 Agent 已在合并后重跑 `npm ci`，故资源本身可满足；只是计划文本未记录该重置。

6. **已登记的 storage-sqlite 预期红窗口（`plan.md:324`）—— 未被扰动。** 该行的 15 处 `SessionUpdate` 全字段字面量是 **WP3 落地后、WP4 落地前**这一中间态的量化断言；#42/#43 对 `crates/storage-sqlite/**` 零改动，不会改变中间态的内容。（精确复核 15 这个数字需要检出 WP3 分支跑 grep，属构建级操作，本轮按只读约束未做；此处只判定「上游未扰动其成立前提」。）

### 重绑定判断（是否需要 Round 8 重新绑定）

**需要，且本轮 PASS 正是重绑定的那一次。** 依据 `openspec/schemas/agentic/procedures/workflow-check.md:42`（plan 门禁要求「`## Dependency Declaration Review` 逐条 PASS……Plan Revision 对新报告绑定当前 `planningDigest`，旧报告继续按原 `contractDigest` 校验」）与 `workflow-check.md:44`（final 阶段同款要求）：审查结论绑定的是 **`planningDigest`（plan-v2 语义规划摘要，覆盖「需求、设计、依赖/波次/写入职责与资源、合并安排」，见 `procedures/scheduling.md:19`）**，而 `plan.md:342` 属于其覆盖范围。

- 因此 `plan.md` 这次单行改动使当前 `planningDigest` 变化，Round 7 记录的 `Plan Revision = sha256:08669be5…`（`verification.md:61`）不再绑定当前 `plan.md`，必须由 Round 8 以新摘要重新绑定。
- 反之，`contract_digest`（`verification.md:169`、`:226`）绑定的是契约内容，本轮 `specs/**`/`design.md`/`proposal.md` 零改动，故其保持 `sha256:08669be5…` 是**正确**的，不应被本轮改动；`verification.md:49` 已确立的规则（「改计划会使 `contractDigest` 失效」）针对的是契约摘要，二者在此处不冲突。
- `verification.md:51`–`61` 的 `## Dependency Declaration Review` 表当前最新行为 Round 7，**尚无 Round 8 行**。主 Agent 需追加一行：`reviewer-plan | 8 | reviewer-plan | <CLI 当前 planningDigest> | PASS | reports/dependency-declaration-review-round8.md`。
- **证据缺失声明**：本轮为只读审查，未运行 `openspec`/`workflow check --stage plan`，因此**无法给出新的 `planningDigest` 实际取值**；上表该格必须由主 Agent 用 CLI 输出填入，不能沿用 Round 7 的 `sha256:08669be5…`，也不应臆造。

## 发现的问题

1. **MINOR — `Runtime Resources` 的 `R2` 行未反映 #43 造成的依赖锁定变更。**
   位置：`plan.md:333`（`Node 依赖（node_modules）| 只读共享（npm ci 已完成）；存在性已由 1.1 核实 | … | provisioner 保证 npm ci 已执行且版本与 lock 一致`）。
   事实：#43 改动 `package.json` 与 `package-lock.json`（agentic 0.3.0 → 0.4.0，lock 4 增 4 删），规划期任务 1.1 的资源核实绑定的是旧 lock。`plan.md:342` 已声明「本地已并入最新 `origin/main` 并在新基线上重跑」，但未把该事件延伸到 `R2` 行的资源前提上。
   影响：仅是资源声明的记录滞后，不改变任何依赖判定（`R2` 为只读共享资源，`Exclusive Scheduling` 本就是 `NOT_APPLICABLE`，且合并后 `npm ci` 已重跑、资源实际可用）。
   最小修复：在 `plan.md:333` 的 `Isolation / Configuration` 单元格追加一句「#43 推进 lock（agentic 0.3.0 → 0.4.0）后已在合并基线上重跑 `npm ci`；任务 1.1 的存在性核实对应旧 lock」，或在 `## Failure and Recovery` 补同义登记。

2. **INFO — `verification.md` 的两个版本指针指向尚未包含本轮改动的提交。**
   位置：`verification.md:126`（`Merged Commit = fad57bb…`）与 `verification.md:225`（`target_commit = fad57bb…`）。`fad57bb` 的树里仍是上一版 `verification.md`（`00211d3…` 取值）与上一版 `plan.md`，本轮两文件的改动尚未提交。
   说明：这是自指记账的固有结果（把目标提交写死为「当前 HEAD」必然落后一步），final/archive 会再次改写。`Candidate = a6f6210b…` 仍是 `fad57bb` 的严格祖先，`Contract Freeze`/`specs/**` 未变，故不影响依赖判定。**无需修复**，仅要求提交顺序为「先提交 `plan.md`/`verification.md` 与本报告，再跑 final/archive 并按新 HEAD 重钉 `target_commit`」。

3. **INFO — 两处声明写范围被登记但实现未改动，且与上游无交叠。**
   `plan.md:292`（WP1）声明 `schemas/node-link/v1/command.schema.json`，`plan.md:293`（WP2）声明 `crates/server/src/node_link/resource.rs`；两者在实现文件集（`6c093f1..60af6e0`）中**均未出现**，上游也未触及。写范围是许可集而非必改集，故不构成声明失实。此处仅作登记，说明上游推进没有使这两条声明变成新的耦合点。

4. **SUGGESTION-5（承接，非本轮新增）— 未登记的双写文件与越界文件仍在计划外。**
   `crates/sync-protocol/tests/envelope_fixtures.rs`（WP1 与 WP2 双写）未登记进 `## Shared File Ownership`，`crates/core/src/model/tests.rs`（WP3 写）未补进 WP3 的 Write Scope。两者均**不被 #42/#43 触及**，故其状态与 Round 5/6/7 记录一致。收口规则已在 `verification.md:49` 给出（`EXPECTED_VALID_MESSAGE_CASES = 67`、`EXPECTED_BODY_REJECTED = 9`、`EXPECTED_ENVELOPE_REJECTED = 2`、`EXPECTED_SKIPPED_PAIRING = 5`，合计 83），属「下一版计划」的工作。原样承接，不因此判 FAIL，也不建议在本轮夹带修改 `plan.md`（那会再次变动 `planningDigest` 并连带作废本轮结论）。
5. **INFO — `## Premerge History` 的 receipt 仍绑定推进前的目标提交，与 `refs/heads/main` 的当前指向不一致（域外观察，仅备案）。**
   位置：`verification.md:161`（M1 行 `Target Commit = 6c093f145aa…`）与 `verification.md:167`（`agentic-premerge` 块 `target_commit: 6c093f145aa…`），而 `Target Ref = refs/heads/main` 现实际指向 `8bf11ca`。
   说明：`openspec/schemas/agentic/procedures/workflow-check.md:44` 的 final 阶段要求 receipt 的 `target` 与 Merge History 行「及目标版本一致」。上游推进改变了 `refs/heads/main` 的指向，因此这条 premerge 记录的 target 已不再是目标版本。这是基线推进造成的台账一致性问题，不属于依赖声明，且只能由 merger / 主 Agent 在 final/archive 的重验中处置，reviewer-plan 无权也不应代改。
   对依赖判定的影响：无。它不触及任何 `code` / `contract` / `resource` 声明，也不改变本轮 PASS 的范围。
