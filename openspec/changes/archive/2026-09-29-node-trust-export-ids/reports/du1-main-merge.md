# DU1 合入报告（Phase B：符合门禁的本地合入与主分支 Project Verify）

## Shared Report

| 字段 | 值 |
| --- | --- |
| task_id | 6.6 / 6.7（符合门禁的本地合入、合入一致性核对与主分支 Project Verify；Phase B） |
| role | integrator |
| phase | merge |
| agent_context | **本 run Agent ID = `9f4abd45-c8eb-483d-832f-fbf7d94ea40b`**（subagent worker；session dir `4ee73356-b6bc-496a-9254-6882d75c2b25`，cwd `D:\Project\acp-remote`）。**Phase A（候选构建）Agent ID = `fc999a8d-c031-406e-acc3-5640fe1b4d8f`**（session dir `bd5f83df-dc67-4e29-94e1-5fee04554e60`，cwd `D:\Project\acp-remote-wt\export-ids-integration`，session name `subagent-worker-fc999a8d-…-1`）。与 Phase A 为**不同 run**：不继承其对话，上下文经 `reports/du1-integration.md`、`reports/du1-integration.log`、`reports/du1-pv1-merged.log`、`reports/pv5-windows-nodelink.log`、`reports/rv5-impl.md` 与集成分支状态只读继承，**未重跑**已完成的候选检查。工作目录为主检出 `D:/Project/acp-remote`。 |
| target_revision | 合入后本地 `refs/heads/main`（下称 `main'`）= `38c6b7233349fc7afee77de3ab0af73db7215fee`；合入前基线 = `3cadb12d79456750e40644a2ea53c96380b700e6` |
| scope | 只做：合入前防竞态与仓库规则复核 → premerge 门禁重跑确认 → 本地 `main` 串行合入 → 合入结果与候选一致性核对 → 主分支 Project Verify（合同门禁 + Rust 三条）。**未** `git push`、**未** 开 PR、**未** 跑 `openspec archive`、**未** 改代码/文档/`schemas`/`fixtures`、**未** 修改权威 `plan.md`/`tasks.md`/`verification.md`（只读）、**未** 触碰主检出未跟踪的 `openspec/changes/node-trust-export-ids/`（原样保留）。 |
| changes | 本地 `refs/heads/main` 新增 1 个合并提交（无新增文件内容，树与候选逐字节相同）；主检出工作区**无**被跟踪文件改动。 |
| checks | premerge 门禁重跑：PASS（exit 0）；`npm run check`：10/10 PASS（exit 0）；`cargo fmt --check`：PASS（0）；`cargo clippy -D warnings`：PASS（0）；`cargo test --workspace`：PASS（0，**994 passed / 0 failed / 2 ignored**）。 |
| issues | 无阻断。ISSUE-2 既有 flaky 本轮**首跑即通过**（§6.4）。任务输入引用的旧 `contractDigest`（`94c6d39a…`）已由主 Agent 在 steering 中说明并被 `8cc6ecf1…` 取代（§3）。 |
| result | PASS（合入与主分支检查阶段） |
| evidence_paths | `reports/du1-main-verify.log`（本次唯一新增原始日志）；本报告 `reports/du1-main-merge.md`；继承引用：`reports/du1-integration.md`、`reports/du1-integration.log`、`reports/du1-pv1-merged.log`、`reports/pv5-windows-nodelink.log`、`reports/rv5-impl.md` |
| resource_cleanup | **未创建任何临时联接**（合同门禁直接用主检出既有 `node_modules`；Rust 用主检出默认 `target/`，未设 `CARGO_TARGET_DIR`）；**未删除任何 worktree**；`D:/Project/acp-remote-wt/target-integration` 本轮未使用、保持 Phase A 留下的原状；loopback 均由测试自建自删（`127.0.0.1:0`）；主检出 `git status` 全程只有 `?? openspec/changes/node-trust-export-ids/`。 |

## 1. 固定提交（base / candidate / main′）

| 角色 | 引用 | 完整 SHA | 备注 |
| --- | --- | --- | --- |
| 合入前基线（base） | `refs/heads/main` = `origin/main` | `3cadb12d79456750e40644a2ea53c96380b700e6` | 合入前复核，与任务输入一致 |
| 候选（Phase A 固定） | 集成分支 `integration/node-trust-export-ids-du1` | `64e179c618e690efcc39f64854d59861e9ecbacf` | 合入时该分支 tip 仍为该值；未被移动 |
| **合入后主分支（main′）** | `refs/heads/main` | **`38c6b7233349fc7afee77de3ab0af73db7215fee`** | 父提交 `3cadb12d` + `64e179c6` |
| 远端 | `origin/main` | `3cadb12d79456750e40644a2ea53c96380b700e6` | **未变**（本轮不推送） |

上游包含关系：`git merge-base --is-ancestor` 核实（合入前）`3cadb12d` 是 `64e179c6` 的祖先（exit 0）；合入后 `64e179c6` 是 `main'` 的祖先（exit 0）。两个已验收源提交 `cc6faefd`（代码）与 `659e5900`（文档）经 Phase A 报告核实为候选祖先，本次合入把整条历史带入 `main`，**未** 引入 base 之外的历史。

## 2. 合入前防竞态与仓库规则复核（原始输出）

工作目录 `D:/Project/acp-remote`，执行时刻：合入前。全部命令与输出：

```text
$ git status --porcelain
?? openspec/changes/node-trust-export-ids/
（唯一项 = 权威规划与证据目录，未被跟踪；**没有任何已跟踪文件改动** → 未触发停止条件）

$ git rev-parse refs/heads/main origin/main
3cadb12d79456750e40644a2ea53c96380b700e6
3cadb12d79456750e40644a2ea53c96380b700e6
（两者相等且 = 任务输入基线）

$ git merge-base --is-ancestor 3cadb12d79456750e40644a2ea53c96380b700e6 64e179c618e690efcc39f64854d59861e9ecbacf
（exit 0 → ANCESTOR=YES）

$ git rev-parse HEAD
3cadb12d79456750e40644a2ea53c96380b700e6      （主检出在 refs/heads/main 上）
```

仓库规则复核（`commitlint.config.mjs`）：`TYPES` 含 `chore`、`SCOPES` 含 `node-link` → 拟用提交信息 `chore(node-link): 合入 node-trust-export-ids 候选 64e179c6` 落在词表内。实际合入时 `.husky/commit-msg` 钩子被执行（输出 `npm notice run commitlint --edit .git/MERGE_MSG`，无报错），合并照常完成。

## 3. premerge 门禁重跑（本 run）

在**候选 worktree** 执行（HEAD 必须仍是候选、目标引用必须仍指向基线）：

```text
$ cd D:/Project/acp-remote-wt/export-ids-integration
$ git rev-parse HEAD
64e179c618e690efcc39f64854d59861e9ecbacf
$ git status --porcelain
（空）
$ node D:/Project/acp-remote/node_modules/@dongfanglin/openspec-agentic/bin/openspec-agentic.mjs \
    workflow check --change node-trust-export-ids --stage premerge --planning-root D:/Project/acp-remote --json
PREMERGE_EXIT=0
```

完整 JSON 摘要（本次执行的原始输出）：

```json
{
  "result": "PASS",
  "stage": "premerge",
  "contractDigest": "sha256:8cc6ecf1af02fcdaa5f2ca289dbdf3698f369feb73158dd3fec88e445c23871d",
  "targetCommit": "3cadb12d79456750e40644a2ea53c96380b700e6",
  "evidence": [
    { "path": "reports/du1-pv1-merged.log", "sha256": "sha256:be76f4b6462f4ddfbd79fc5c35ac19d8621c64826e85a0cedc78983e9bc6de86" },
    { "path": "reports/rv5-impl.md", "sha256": "sha256:0d37a94b39f6597cfd180811f64699258c86adb4798b6da4a489b05bfd9eac8e" },
    { "path": "reports/pv5-windows-nodelink.log", "sha256": "sha256:259e05fd23fe1b33fd005627cc1781c3dec891f2431db043cae4fbd7027cbc25" },
    { "path": "reports/du1-integration.log", "sha256": "sha256:8327ebed7fa80f7cb167ef908aec61aa4db7b1c2bc5b1dd8e1083849744d875e" },
    { "path": "reports/du1-integration.log", "sha256": "sha256:8327ebed7fa80f7cb167ef908aec61aa4db7b1c2bc5b1dd8e1083849744d875e" }
  ],
  "errors": [],
  "boundary": "仅检查结构、引用和版本；不证明用户批准、角色独立性或测试真实性。",
  "candidateCommit": "64e179c618e690efcc39f64854d59861e9ecbacf"
}
```

契约摘要说明（与任务输入的一处差异）：任务输入引用的 premerge 结果是 `contractDigest: sha256:94c6d39a…`，本 run 重跑得到 **`sha256:8cc6ecf1…`**。原因是主 Agent 在派发后按 RV5-IMPL-F3 修了 `tasks.md` 6.4 的报告路径（`reports/rv2-du1.md` → `reports/rv5-impl.md`），而**任务描述参与契约摘要**，摘要因此重算；主 Agent 已在 steering 中确认，并已把新值同步到 `verification.md` 的 `agentic-premerge` 块（本 run 实测该块记录的 `contract_digest` 与本次输出**逐字相同**）与 `agentic-assessment` 块。**候选提交、候选检查日志与其余证据未变**，故未重建候选、未重跑 [PV1]/[PV5]。

工具链取用说明（记录用，非缺陷）：候选 worktree 里 Phase A 的临时 `node_modules` 联接已按计划删除，因此在该 worktree 内直接 `npx openspec-agentic` 会落到**全局 0.2.0**（其 `--stage` 不接受 `premerge`，exit 1）；本轮改用**主检出的本地 0.2.4**（`package.json` pin 版本）bin 显式执行，`--version` = `0.2.4`。这与 `AGENTS.md` §12「日常 OpenSpec 命令一律走项目本地引擎（版本 pin 见 `package.json`）」一致。

## 4. 本地合入

```text
$ cd D:/Project/acp-remote
$ git merge --no-ff integration/node-trust-export-ids-du1 -m "chore(node-link): 合入 node-trust-export-ids 候选 64e179c6"
npm notice run acp-remote-contracts@0.0.0 npx
npm notice run commitlint --edit .git/MERGE_MSG
Merge made by the 'ort' strategy.
 crates/app/src/cli.rs                              |   6 +
 …（32 个文件，2373 insertions(+), 208 deletions(-)，其中 create mode 100644 crates/app/tests/node_pair_export_ids.rs）…
MERGE_EXIT=0

$ git rev-parse HEAD
38c6b7233349fc7afee77de3ab0af73db7215fee

$ git log -1 --format='%H %P%n  %s'
38c6b7233349fc7afee77de3ab0af73db7215fee 3cadb12d79456750e40644a2ea53c96380b700e6 64e179c618e690efcc39f64854d59861e9ecbacf
  chore(node-link): 合入 node-trust-export-ids 候选 64e179c6

$ git status --porcelain
?? openspec/changes/node-trust-export-ids/
```

**冲突解决：无冲突**（预期兑现）。`ort` 策略自动合并，退出码 0，无冲突标记、无手工解冲突、无内容取舍 → 本报告不含需交主 Agent 决策的需求/接口取舍项。合入后工作区仍只有那一个未跟踪目录（未被 `git add`、未被 `git clean`、内容原样）。

## 5. 合入结果与候选一致性核对（task 6.7 前半）

```text
$ git diff --stat 64e179c6 HEAD -- crates/ docs/ openspec/specs/
（空）

$ git diff --stat 64e179c6 HEAD            # 全树，不只三个目录
（空）                                      # → main′ 的树与候选逐字节相同，无夹带、无丢失

$ git diff --stat 3cadb12d HEAD | tail -3
 docs/NODE_LINK_PROTOCOL.md                         |  20 +-
 openspec/specs/storage-schema-v2-migration/spec.md |   2 +-
 32 files changed, 2373 insertions(+), 208 deletions(-)
（与 `git diff --stat 3cadb12d 64e179c6` 完全一致）

$ git merge-base --is-ancestor 64e179c618e690efcc39f64854d59861e9ecbacf HEAD
（exit 0 → CANDIDATE_IS_ANCESTOR=YES）

$ git rev-parse refs/heads/main origin/main
38c6b7233349fc7afee77de3ab0af73db7215fee
3cadb12d79456750e40644a2ea53c96380b700e6
```

结论：合入是**纯快进式合并提交**——`main′` 相对基线引入的差异 = 候选相对基线的差异（32 文件 / 2373+ / 208−），且 `main′` 的树与候选完全相同。**因此 task 6.8 的「合并是否引入新增差异」在文件层面为「无新增差异」**（该判定仍由主 Agent/独立 reviewer 按 6.8 流程记录，本报告只提供依据）。

## 6. 主分支 Project Verify（task 6.7 后半）

执行树：主检出 `D:/Project/acp-remote`（`refs/heads/main` = `38c6b7233349fc7afee77de3ab0af73db7215fee`），**含完整变更目录**（本报告的权威判定树）。
环境：Windows / MINGW64_NT-10.0-26200；`cargo 1.98.1 (797e8a9bc 2026-08-05)` / `rustc 1.98.1 (48a229cea 2026-09-01)`（= `rust-toolchain.toml` channel）；Node `v24.19.0` / npm `12.0.2`；`CARGO_TARGET_DIR` 未设置（用主检出默认 `target/`）。
日志：`reports/du1-main-verify.log`（1621 行；sha256 = `sha256:9eb7c82af715c0e9ac07fda6a2a091250cd15c286bbffba90678305c3e160b84`）。逐条写入命令、退出码与 `test result` 汇总行。

| Check ID | 命令 | 退出码 | 结果 | 日志位置 |
| --- | --- | --- | --- | --- |
| [PV1]（合同门禁半）/ task 6.7 | `npm run check` | 0 | PASS：10/10 子门禁 | `du1-main-verify.log:6-81`（`CHECK_EXIT=0`） |
| [PV1]（rust 半）/ task 6.7 | `cargo fmt --all -- --check` | 0 | PASS | `du1-main-verify.log:82-84`（`FMT_EXIT=0`） |
| [PV1]（rust 半）/ task 6.7 | `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | 0 | PASS（`Finished dev profile … in 7.62s`，无 warning） | `du1-main-verify.log:85-95`（`CLIPPY_EXIT=0`） |
| [PV1]（rust 半）/ task 6.7 | `cargo test --locked --workspace --all-features` | 0 | PASS：86 个 `test result` 行**全为 `ok`**，**994 passed / 0 failed / 2 ignored** | `du1-main-verify.log:96-1621`（`TEST_EXIT=0`） |
| [PV2] / task 6.7 | `node scripts/check-crate-boundaries.mjs`（在 `npm run check` 内作为 `check:boundaries`） | 0 | PASS | `du1-main-verify.log:32-33` |
| `check:docs` / task 6.7 | `node scripts/check-doc-links.mjs`（在 `npm run check` 内） | 0 | PASS（**权威树全量**） | `du1-main-verify.log:28-30` |
| `check:drift` / task 6.7 | `node scripts/check-contract-drift.mjs`（在 `npm run check` 内） | 0 | PASS | `du1-main-verify.log:34-36` |
| `check:agentic` / task 6.7 | `node scripts/agentic-gate.mjs` + `sync-agentic-host-entrypoints.mjs --check` | 0 | PASS | `du1-main-verify.log:37-80` |
| `check:command-catalog` / task 6.7 | `node scripts/check-command-catalog.mjs` | 0 | PASS | `du1-main-verify.log:12-14` |
| [PV5]（替代验证）/ task 6.7（不重跑） | 计划规定在候选/主分支上执行 `cargo test --locked -p server -p app --all-features` 的受控路径全链路 | — | **不适用（不在本 run 重跑）** | 依据见 §6.3；替代验证主体证据 = `reports/pv5-windows-nodelink.log`（Phase A，候选 `64e179c6`），且 `main′` 的树与候选逐字节相同（§5） |

### 6.1 `npm run check` 全部子门禁（`du1-main-verify.log` 原文，逐字）

```text
npm notice run npm run check:schemas && … && npm run check:agentic
schema fixtures OK: 118 valid, 25 invalid (ajv Draft 2020-12), 39 event views bound
command catalog OK: 12 commands
error registry OK: 58 codes across 2 protocols
feature registry OK: 11 feature ids across 2 protocols
contract assets OK: 17 schemas, 156 fixture files, 12 transcript vectors re-encoded from input, 20 negative vectors rejected as declared, 2 SAS values recomputed
ACP compatibility matrix OK: 25 methods, 11 updates, 5 content blocks, 3 tool content types, 19 capabilities, 8 invariants, 10 test families (71 rows, schema validated by ajv)
doc links OK: 381 relative links, 7516 section refs across 356 markdown files
note: 3601 section refs 的归属文档由上下文决定（未在同一子句内指名文档），按设计未判定；设 DOC_LINKS_VERBOSE=1 可列出位置
crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致（12 个已在矩阵登记）
contract drift OK: §7 的 36 条 DDL 与 crates\storage-sqlite\src\migrate.rs 逐条一致；§5 的 15 个 trait / 93 个方法签名与 crates\core\src\ports.rs 一致
PASS  toolchain: Node v24.19.0 / npm 12.0.2
PASS  openspec: 项目本地 @fission-ai/openspec 1.13.0
PASS  config: openspec/config.yaml，7 个角色已配置；x-agentic.e2e.enabled=true
PASS  schema: openspec/schemas/agentic
PASS  schema validation: openspec schema validate agentic
PASS  verification skill: .agents/skills/agentic-verify
PASS  AGENTS.md: agentic 验收路由
…（Totals: 20 passed, 0 failed）
✓ change/node-trust-export-ids
agentic 宿主入口检查完成：17 个文件
CHECK_EXIT=0
```

`check:drift`（本变更关键项）在主分支树上**再次**通过，原始成功行与候选阶段逐字相同；`check:agentic` 的 `✓ change/node-trust-export-ids` 表明变更目录本身也通过校验。

### 6.2 候选树 vs 权威树的 `check:docs` 取位差异（重要，非缺陷）

Phase A 报告的 §4.3 已说明：候选树**不含**变更目录，`check:docs` 只扫到 324 个 markdown。本轮在主分支树（含完整 `openspec/changes/node-trust-export-ids/`，含本报告）执行，扫描范围为 **356 个 markdown / 381 相对链接 / 7516 章节引用**，exit 0。这正是 `verification.md` 的 Check Plan Changes「门禁一律在含完整变更目录的权威树跑」那条方法论纠正所要求的**权威判定**，它此前一直是 PENDING（候选树无变更目录），**本 run 闭合**。

### 6.3 Rust 三条原始要点

```text
$ cargo fmt --all -- --check
（无输出）FMT_EXIT=0

$ cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
    Checking app v0.0.0 (D:\Project\acp-remote\crates\app)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 7.62s
CLIPPY_EXIT=0

$ cargo test --locked --workspace --all-features
… 86 个 `test result: ok.` 行，汇总：
passed=994 failed=0 ignored=2
（`grep -cE "^failures:|FAILED"` = 0）
TEST_EXIT=0
```

`994 / 0 / 2` 与候选阶段 `reports/du1-pv1-merged.log` 的计数**完全一致**（§5 已证明两棵树逐字节相同，故该数值一致性符合预期，而非碰巧）。

### 6.4 已知 flaky 与 ignored 用例

| 项 | 本轮状态 |
| --- | --- |
| ISSUE-2 flaky：`crates/app/tests/daemon_lifecycle.rs::the_periodic_task_runs_at_least_once_and_is_cancelled_at_shutdown` | **首跑即通过**：`du1-main-verify.log:448` = `test the_periodic_task_runs_at_least_once_and_is_cancelled_at_shutdown ... ok`。无 panic、**未** 重跑、**未** `#[ignore]`、**未** 弱化断言。 |
| ignored 1 | `crates/storage-sqlite/tests/commit.rs`：`crash_child … ignored, 由 crash_recovery_leaves_an_consistent_database 拉起`（既有辅助子进程） |
| ignored 2 | `crates/storage-sqlite/tests/migration.rs`：`regenerate_v1_fixture … ignored, 夹具生成器：只在需要重建 fixtures/storage/v2/from-v1.sqlite3 时手动运行`（既有手动夹具生成器） |

两条 ignored 均为 base `3cadb12d` 上既有的辅助用例，非本变更新增跳过；与 Phase A 报告的清单一致。

## 7. 未修改项 / 合规确认

- 未改任何 `crates/**`、`docs/**`、`openspec/specs/**`、`compat*/**`、`schemas/**`、`fixtures/**`、`package.json`、`commitlint.config.mjs`：合入后 `git diff --stat HEAD` 为空，`git status --porcelain` 只有那个未跟踪目录。
- 未修改权威 `plan.md` / `tasks.md` / `verification.md`（只读引用）。`verification.md` 的 Merge History「本地合入（Phase B）」行与 `agentic-premerge` 之外的本轮记录由**主 Agent** 维护，本 Agent 不写。
- **未** `git push`、**未** 开 PR、**未** 跑 `openspec archive`、**未** 动 `.git/config`、分支保护或任何 worktree；**未** 对主检出未跟踪的 `openspec/changes/node-trust-export-ids/` 做 `git add` / `git clean` / 任何写入（除本报告与 §6 的日志两个新文件，二者本就落在该未被跟踪目录内且被 `.gitignore` 的 `reports/**/*.log` 规则覆盖日志文件）。
- 未触碰其它 worktree（`export-ids`、`export-ids-docs`、`export-ids-integration`、`nl-owner-integration`、`node-link-owner`）。

## 8. 资源清理

| 资源 | 处理 | 核实 |
| --- | --- | --- |
| 主检出 `node_modules` | 直接复用（未建联接） | `node_modules/ajv/package.json`、`node_modules/.bin/commitlint*` 均在；`npm run check` 全绿 |
| 主检出 `target/` | 使用默认 `CARGO_TARGET_DIR`（未覆盖） | `cargo clippy`/`test` 退出 0；未产生额外 target 目录 |
| `D:/Project/acp-remote-wt/target-integration` | **未使用、未删除**（Phase A 有意保留供 Phase B 复用；本轮因改用主检出默认 target 而未用到） | 仍存在于原位置；主 Agent 可在最终验收前决定是否删除 |
| worktree | 未删除任何 worktree | `git worktree list` 与 Phase A 相同（6 项） |
| 临时数据目录 / 证书 / loopback | 由测试用例自建自删；loopback 一律 `127.0.0.1:0` | 主检出 `git status --porcelain` 无反残留；未新增未跟踪数据目录 |

## 9. 结论与交接

- **已本地合入**：`refs/heads/main` `3cadb12d` → **`38c6b7233349fc7afee77de3ab0af73db7215fee`**（`--no-ff`，无冲突）；`main′` 的树与候选 `64e179c6` **逐字节相同**（全树 `git diff` 为空），相对基线的差异 = 候选差异（32 文件 / 2373+ / 208−）。
- **premerge 门禁在合入前重跑 = PASS**（`contractDigest` = `sha256:8cc6ecf1…`，`targetCommit` = `3cadb12d…`，`errors: []`，exit 0）；目标引用在合入前后均未移动。
- **主分支 Project Verify 全绿**：`npm run check` 10/10（含权威树 `check:docs` 356 文件、`check:drift`、`check:agentic` 且 `✓ change/node-trust-export-ids`）、`cargo fmt --check` 0、`clippy -D warnings` 0、`cargo test --workspace` 0（994 / 0 / 2）。
- **未推送**：`origin/main` 仍为 `3cadb12d79456750e40644a2ea53c96380b700e6`（**未变**）。交付 `origin/main` 仍须走 `AGENTS.md` §8 的 PR + 必需检查路径，由主 Agent 决定。
- **候选 PASS ≠ 已合入 ≠ 最终验收 PASS**；本报告的 PASS 只覆盖「合入 + 主分支检查」两个阶段。

### 未解决项 / 需主 Agent 处理

1. `verification.md` 的 Merge History 表需由主 Agent 填入 Phase B 行（`main′` = `38c6b72…`、本报告路径、premerge 重跑摘要、主分支 Project Verify 结果）；`agentic-premerge` 块的 `target_commit` 是**合入前**基线，保持 `3cadb12d` 不改（它是 premerge 记录）。
2. task 6.8（合入新增差异的独立检视）：文件层面**无新增差异**（§5），按 `tasks.md` 6.8 的口径可由主 Agent 记录依据并沿用 RV5-IMPL，无需同范围重复审查——由主 Agent 裁定。
3. task 7.1 的最终替代验证（在**最终主分支版本**上重跑 [PV1]/[PV3]/[PV4]/[PV5]）尚未执行；本轮只做了 task 6.7 口径的主分支回归（合同门禁 + rust 三条）。注意 [PV5] 受控路径全链路本轮**未**在主分支树上重跑（§6 表末行）；若 7.1 要求在该版本重跑 [PV5]，需另开一轮。
4. ISSUE-2（既有 flaky）仍未解决，仍是 CI `checks` job 的间歇红灯风险，建议单独变更修（本轮首跑即过，不构成阻断）。
5. `D:/Project/acp-remote-wt/target-integration`（8.1 GB）保留决策：本轮未使用；是否清理由主 Agent 定。

## handoff_index

```yaml
handoff_index:
  - task_id: "6.6"
    role: integrator
    phase: merge
    stage: main
    target_revision: "38c6b7233349fc7afee77de3ab0af73db7215fee"
    evidence_type: DELIVERY
    evidence_id: NOT_APPLICABLE
    report_path: "reports/du1-main-merge.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "本地 refs/heads/main 由 3cadb12d 经 git merge --no-ff integration/node-trust-export-ids-du1 移到 38c6b72（父 3cadb12d + 64e179c6），MERGE_EXIT=0、无冲突、无手工取舍；合入前 git status --porcelain 仅 '?? openspec/changes/node-trust-export-ids/'（无已跟踪改动）、refs/heads/main=origin/main=3cadb12d、merge-base --is-ancestor 3cadb12d 64e179c6 = exit 0；提交信息 chore(node-link): … 的两词均在 commitlint.config.mjs 词表内且 .husky/commit-msg 已实际执行通过。未 push、未开 PR、未 archive。"
    source_evidence: NOT_APPLICABLE

  - task_id: "6.6"
    role: integrator
    phase: merge
    stage: main
    target_revision: "38c6b7233349fc7afee77de3ab0af73db7215fee"
    evidence_type: VALIDATION
    evidence_id: premerge-gate-rerun
    report_path: "reports/du1-main-merge.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "合入前在候选 worktree（HEAD=64e179c6、工作区干净）用主检出本地 openspec-agentic 0.2.4（package.json pin）执行 workflow check --change node-trust-export-ids --stage premerge --planning-root D:/Project/acp-remote --json → result=PASS、targetCommit=3cadb12d79456750e40644a2ea53c96380b700e6、candidateCommit=64e179c618e690efcc39f64854d59861e9ecbacf、contractDigest=sha256:8cc6ecf1af02fcdaa5f2ca289dbdf3698f369feb73158dd3fec88e445c23871d、errors=[]、exit 0；该摘要与 verification.md 的 agentic-premerge 块逐字一致。"
    source_evidence: NOT_APPLICABLE

  - task_id: "6.7"
    role: integrator
    phase: merge
    stage: main
    target_revision: "38c6b7233349fc7afee77de3ab0af73db7215fee"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "reports/du1-main-verify.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "主检出（含完整变更目录的权威树，356 markdown）在 main' 上：npm run check exit 0（10/10 子门禁）、cargo fmt --all -- --check exit 0、cargo clippy --locked --workspace --all-targets --all-features -- -D warnings exit 0、cargo test --locked --workspace --all-features exit 0（86 个 test result 行全 ok，994 passed / 0 failed / 2 ignored）。工具链 cargo/rustc 1.98.1（rust-toolchain.toml）、Node v24.19.0 / npm 12.0.2。exit 码与汇总行逐条写在该日志（CHECK_EXIT/FMT_EXIT/CLIPPY_EXIT/TEST_EXIT）。"
    source_evidence: NOT_APPLICABLE

  - task_id: "6.7"
    role: integrator
    phase: merge
    stage: main
    target_revision: "38c6b7233349fc7afee77de3ab0af73db7215fee"
    evidence_type: CHECK
    evidence_id: "check:docs"
    report_path: "reports/du1-main-verify.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "权威树全量判定（本变更此前一直 PENDING 的那一项）：'doc links OK: 381 relative links, 7516 section refs across 356 markdown files'，exit 0；候选树只有 324 markdown（不含变更目录），故本轮结果不能由 Phase A 证据替代。"
    source_evidence: NOT_APPLICABLE

  - task_id: "6.7"
    role: integrator
    phase: merge
    stage: main
    target_revision: "38c6b7233349fc7afee77de3ab0af73db7215fee"
    evidence_type: CHECK
    evidence_id: "check:drift"
    report_path: "reports/du1-main-verify.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "主分支树原文：'contract drift OK: §7 的 36 条 DDL 与 crates\\storage-sqlite\\src\\migrate.rs 逐条一致；§5 的 15 个 trait / 93 个方法签名与 crates\\core\\src\\ports.rs 一致'，exit 0、无差异行；与候选阶段同一条成功行一致。"
    source_evidence: NOT_APPLICABLE

  - task_id: "6.7"
    role: integrator
    phase: merge
    stage: main
    target_revision: "38c6b7233349fc7afee77de3ab0af73db7215fee"
    evidence_type: CHECK
    evidence_id: "check:agentic"
    report_path: "reports/du1-main-verify.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "主分支树：agentic-gate.mjs（toolchain/openspec/config/schema/schema validation/verification skill/AGENTS.md 全 PASS）＋ sync-agentic-host-entrypoints.mjs --check（17 个文件）＋ 'Totals: 20 passed, 0 failed (20 items)'，含 '✓ change/node-trust-export-ids'，exit 0。"
    source_evidence: NOT_APPLICABLE

  - task_id: "6.7"
    role: integrator
    phase: merge
    stage: main
    target_revision: "38c6b7233349fc7afee77de3ab0af73db7215fee"
    evidence_type: CHECK
    evidence_id: PV2
    report_path: "reports/du1-main-verify.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "主分支树 node scripts/check-crate-boundaries.mjs（在 npm run check 内作为 check:boundaries）exit 0：'crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致（12 个已在矩阵登记）'。"
    source_evidence: NOT_APPLICABLE

  - task_id: "6.7"
    role: integrator
    phase: merge
    stage: main
    target_revision: "38c6b7233349fc7afee77de3ab0af73db7215fee"
    evidence_type: CHECK
    evidence_id: merge-consistency
    report_path: "reports/du1-main-merge.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "合入结果一致性：git diff --stat 64e179c6 HEAD -- crates/ docs/ openspec/specs/ = 空；git diff --stat 64e179c6 HEAD（全树）= 空；git diff --stat 3cadb12d HEAD = 32 files changed, 2373 insertions(+), 208 deletions(-)（与 base→候选逐项一致）；git merge-base --is-ancestor 64e179c6 HEAD = exit 0。故 task 6.8 的合并新增差异在文件层面为「无新增差异」。"
    source_evidence: NOT_APPLICABLE

  - task_id: "6.7"
    role: integrator
    phase: merge
    stage: main
    target_revision: "38c6b7233349fc7afee77de3ab0af73db7215fee"
    evidence_type: CHECK
    evidence_id: PV5
    report_path: "reports/pv5-windows-nodelink.log"
    result: PASS
    evidence_status: REUSED
    applicability_basis: "计划规定 [PV5] 在候选与主分支（替代验证主体）执行受控路径全链路 cargo test --locked -p server -p app --all-features；该证据在候选 64e179c6 上取得（PASS / exit 0，含 a_narrowing_repair_closes_the_live_attachment 与 node_pair_export_ids 2/2）。主分支版本的树与候选 64e179c6 逐字节相同（git diff --stat 64e179c6 38c6b72 全树为空），因此同一提交内容上的结果为适用；**本轮未在主分支树上重跑该命令**，若 task 7.1 要求最终版本重跑，需另开一轮。"
    source_evidence: {id: PV5, report_path: "reports/pv5-windows-nodelink.log", target_revision: "64e179c618e690efcc39f64854d59861e9ecbacf"}

  - task_id: "6.7"
    role: integrator
    phase: merge
    stage: main
    target_revision: "38c6b7233349fc7afee77de3ab0af73db7215fee"
    evidence_type: RESOURCE
    evidence_id: NOT_APPLICABLE
    report_path: "reports/du1-main-merge.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "未创建任何临时联接；使用主检出既有 node_modules 与默认 target/；未删除任何 worktree；target-integration 未使用也未删除；git worktree list 与 Phase A 相同（6 项）；loopback 一律 127.0.0.1:0 且由测试自建自删；主检出 git status 全程仅 '?? openspec/changes/node-trust-export-ids/'；未 push（origin/main 仍为 3cadb12d）。"
    source_evidence: NOT_APPLICABLE
```

## 附：复现命令

```text
# 合入前复核
cd D:/Project/acp-remote
git status --porcelain                       # -> ?? openspec/changes/node-trust-export-ids/
git rev-parse refs/heads/main origin/main    # -> 3cadb12d… ×2
git merge-base --is-ancestor 3cadb12d79456750e40644a2ea53c96380b700e6 64e179c618e690efcc39f64854d59861e9ecbacf

# premerge 门禁（候选 worktree；用主检出的本地 0.2.4 bin）
cd D:/Project/acp-remote-wt/export-ids-integration
node D:/Project/acp-remote/node_modules/@dongfanglin/openspec-agentic/bin/openspec-agentic.mjs \
  workflow check --change node-trust-export-ids --stage premerge --planning-root D:/Project/acp-remote --json

# 本地合入（主检出）
cd D:/Project/acp-remote
git merge --no-ff integration/node-trust-export-ids-du1 -m "chore(node-link): 合入 node-trust-export-ids 候选 64e179c6"   # -> 38c6b723

# 合入一致性核对
git diff --stat 64e179c618e690efcc39f64854d59861e9ecbacf HEAD                     # 需为空
git diff --stat 3cadb12d79456750e40644a2ea53c96380b700e6 HEAD                     # = 32 files, 2373(+), 208(-)

# 主分支 Project Verify（全部追加到 reports/du1-main-verify.log）
npm run check
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
```
