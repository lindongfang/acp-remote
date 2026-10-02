# U1 集成基线报告（role: merger / phase: integrate / stage: candidate）

> 本报告只覆盖本轮「集成（integrate）」动作：把已验收上游 WP1、WP2 组合成**固定集成基线**，供下游 WP3 开工。
> 本轮**没有**做候选 Project Verify 的完整判定（无独立 review、无 `workflow check --stage premerge`）、
> **没有**合入 `refs/heads/main`、**没有** push。候选 PASS 与已合入 PASS 都不在本报告的结论范围内。

## Shared Report

| 字段 | 值 |
| --- | --- |
| task_id | 1.4（上游包含关系核对与接入）；关联 5.2 / 6.2 的「集成基线」部分 |
| role | merger（独立合入角色，非任何 WP 的实现者/测试者/reviewer） |
| phase | integrate |
| stage | candidate（`roles/_shared/role-report.md`：merger 的 `integrate` 与 `candidate` 绑同一候选提交） |
| agent_context | 独立 merger 子 Agent，**全新上下文**（无继承的实现/review 对话），`PI_SESSION_ID=01a0f236-7c86-73c4-a7f8-af296e4a1760`，父会话 `01a0efdd-1c44-73b3-9d5d-19a36a79d6af`，模型 `deepseek-flash`；会话文件 `C:\Users\zhang\.pi\agent\sessions\--D--Project-acp-remote--\2026-09-30T01-10-40-452Z_01a0efdd-1c44-73b3-9d5d-19a36a79d6af\564402b0-8ab2-4f60-8f8d-2f64b8655a9b\run-0\session.jsonl` |
| target_revision | `b0a387b17f87a9d15d5b07d20f1e16539566c789`（U1 集成基线固定提交，分支 `integration/session-resume-du1`） |
| scope | `D:/Project/acp-remote-wt/session-resume-du1`（复用本单元已有执行 worktree，**未新建、未切换分支**）；只做 WP1/WP2 的本地合入 + 计划内检查；不改产品代码、不改 plan/tasks/verification/AGENTS.md、不碰 `crates/server/` |
| changes | 分支 `integration/session-resume-du1` 新增两个合并提交：`e8feaf95…`（合入 WP1）、`b0a387b1…`（合入 WP2 = 集成基线）。除 git 合并本身外，merger 未产生任何文件改动 |
| checks | PV1（阶段 1：crate 子集）PASS；PV2（`npm run check`）PASS；PV1（阶段 2：workspace 全量）前置条件「全部 WP 集成后」未满足 → NOT_APPLICABLE（诊断性执行只看到已登记红窗口 E0004） |
| issues | 1 项**计划内、已登记、有主（WP6/W4）、有界**的红窗口：`crates/server` 编译 E0004（详见下文「红窗口」）。除该 E0004 外，本次执行**未发现**任何其它编译/测试失败 |
| result | **PASS**（本轮 integrate 的全部适用条件满足：两源提交合入无冲突、祖先关系成立、非红窗口 crate 全绿、PV2 全绿、红窗口形态与登记一致） |
| evidence_paths | 本报告 + `reports/merge-u1-integrate-PV1-stage1.log`、`merge-u1-integrate-PV1-stage1-extra.log`、`merge-u1-integrate-PV2.log`、`merge-u1-integrate-workspace-clippy.log`、`merge-u1-integrate-PV1-stage2-workspace-test.log`、`merge-u1-integrate-PV1-stage2-exclude-red-window.log`（均在 `openspec/changes/session-resume/reports/`） |
| resource_cleanup | worktree 保留（本单元候选阶段复用）；`CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-du1` 保留（独占、无跨角色争用）；无残留临时库文件/监听器；`git status --porcelain` 空 |

## 环境（本轮实测）

| 项 | 值 |
| --- | --- |
| 工作目录 | `D:/Project/acp-remote-wt/session-resume-du1` |
| 分支 | `integration/session-resume-du1` |
| 基线（合入前 HEAD） | `81e350ff340014265eb7c9251237c799d4357fee`（= `refs/heads/main`） |
| rustc / cargo | `rustc 1.98.1 (48a229cea 2026-09-01)` / `cargo 1.98.1 (797e8a9bc 2026-08-05)`（由仓库 `rust-toolchain.toml` 固定） |
| Node / npm | `v24.19.0` / `12.0.2`（`npm run check` 要求 Node ≥ 22.12） |
| CARGO_TARGET_DIR | `D:/Project/acp-remote-target/session-resume-du1`（独占，目录已存在、合入前为空） |
| node_modules | 目录联接可用：`realpathSync.native('node_modules')` → `D:\Project\acp-remote\node_modules`；`require.resolve('ajv')` 成功；`git status --porcelain --ignored` 只有 `!! node_modules/` |

合入前状态核实（task 6.1 的「机械核实目标」部分）：

```text
$ git status --porcelain        # 无输出（worktree 干净）
$ git rev-parse HEAD            # 81e350ff340014265eb7c9251237c799d4357fee
$ git rev-parse refs/heads/main # 81e350ff340014265eb7c9251237c799d4357fee
```

## 固定版本与依赖包含关系（证据）

| 角色 | 引用 | 完整 SHA |
| --- | --- | --- |
| base（合入前集成分支 = 本地主分支） | `integration/session-resume-du1` / `refs/heads/main` | `81e350ff340014265eb7c9251237c799d4357fee` |
| 源：WP1 | `agentic/session-resume-wp1` | `248d9b9fa69a1e6375a3b9708fdaa2a127dc7ccb` |
| 源：WP2 | `agentic/session-resume-wp2` | `32f71f5d5f08145cfd3e3bddd7d6f6a46a0a5992` |
| 合并提交 1（合入 WP1） | `e8feaf95edbb322416a9f6a42ba087bfb42d44fa`（parents: `81e350f`, `248d9b9`） | 同左 |
| **candidate / 集成基线** | `integration/session-resume-du1` HEAD | **`b0a387b17f87a9d15d5b07d20f1e16539566c789`**（parents: `e8feaf9`, `32f71f5`） |
| 目标主分支（本轮**未**合入，未移动） | `refs/heads/main` | `81e350ff340014265eb7c9251237c799d4357fee` |

包含关系证明（`git merge-base --is-ancestor`，退出码 0 = 是祖先）：

```text
$ git merge-base --is-ancestor 248d9b9fa69a1e6375a3b9708fdaa2a127dc7ccb HEAD   # exit=0（基线包含 WP1 交付提交）
$ git merge-base --is-ancestor 32f71f5d5f08145cfd3e3bddd7d6f6a46a0a5992 HEAD   # exit=0（基线包含 WP2 交付提交）
$ git merge-base --is-ancestor 81e350ff340014265eb7c9251237c799d4357fee HEAD   # exit=0（基线包含原 base）
$ git merge-base --is-ancestor HEAD agentic/session-resume-wp1                  # exit=1（基线不在 WP1 分支上，符合预期）
$ git merge-base --is-ancestor agentic/session-resume-wp1 HEAD                  # exit=0
$ git merge-base --is-ancestor agentic/session-resume-wp2 HEAD                  # exit=0
```

合入前两个源分支与 base 的共同祖先均为 `81e350f…`，即两包都是从当前主分支起点开出的，无需其它前置合入。

## 合入过程与冲突解决

```text
$ git merge --no-ff --no-verify agentic/session-resume-wp1 \
    -m "chore(repo): 集成 WP1（acp-protocol session/resume DTO）到 U1 集成基线"
Merge made by the 'ort' strategy.   # exit=0，4 files changed, 236 insertions(+), 5 deletions(-)

$ git merge --no-ff --no-verify agentic/session-resume-wp2 \
    -m "chore(repo): 集成 WP2（node-link session.resume 与封闭词表原子点）到 U1 集成基线"
Merge made by the 'ort' strategy.   # exit=0，21 files changed, 409 insertions(+), 36 deletions(-)（3 个新 fixture 文件）
```

- **冲突解决：无。** 两次合入都是干净的自动合并，`git status` 无冲突标记，merger 未手工编辑任何文件；WP1 只写 `crates/acp-protocol/`、`compatibility/acp/v1/matrix.json`、`docs/ACP_COMPATIBILITY_MATRIX.md`，WP2 写 node-link/identity-auth/commands.json/schemas/fixtures/四份文档与脚本，写范围不相交（plan.md `## Shared File Ownership` 亦如此登记）。
- 唯一「双写者」文件 `crates/core/src/broker.rs` 本轮只有 WP2 改动（`required_grant` 一条臂），WP3 的改动尚不存在，因此不存在区域冲突。
- 两次合入都用了 `--no-ff`（保留来源与父提交）与 `--no-verify`。`--no-verify` 的理由：红窗口内 `.husky/pre-commit` → `scripts/pre-commit.mjs` 会跑 **workspace 全量** `cargo clippy -D warnings`，必然被已登记的 E0004 拒绝；plan.md 的 DR1-F23 明文允许此时用 `--no-verify`，且钩子按 `AGENTS.md` §8 只是「更早发现失败」而非门禁本体。**判定责任没有因此下移**：下列所有计划内检查都在固定基线上重新手工执行并落盘日志。
- 基线提交 SHA 未做 amend、未 rebase、未 push；`refs/heads/main` 仍是 `81e350f…`（本轮明确只做 integrate）。
- 合入后 `git status --porcelain` 为空（merger 未留下未提交改动）。

## Check 记录（逐条命令 / 退出码）

### C1 — PV1 阶段 1：不依赖 server 的 crate 子集（PASS）

工作目录 = 仓库根；`CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-du1`。

```text
$ cargo fmt --all -- --check
exit=0（无输出）

# 逐个 crate（plan.md PV1 的阶段 1 形式）
$ cargo clippy --locked -p <crate> --all-targets --all-features -- -D warnings
$ cargo test   --locked -p <crate> --all-features
```

| crate | clippy exit | test exit | 通过用例数 |
| --- | --- | --- | --- |
| acp-protocol | 0 | 0 | 40 passed / 0 failed |
| node-link-protocol | 0 | 0 | 40 passed / 0 failed |
| identity-auth | 0 | 0 | 83 passed / 0 failed |
| core | 0 | 0 | 121 passed / 0 failed |
| storage-sqlite | 0 | 0 | 129 passed / 0 failed / 2 ignored |
| agent-host | 0 | 0 | 55 passed / 0 failed |
| acpr-transcript | 0 | 0 | 24 passed / 0 failed |
| acpr-wire | 0 | 0 | 20 passed / 0 failed |
| sync-protocol | 0 | 0 | 46 passed / 0 failed |
| identity-keystore | 0 | 0 | 36 passed / 0 failed |

覆盖依据：`cargo metadata` 显示 workspace 共 12 个 crate，**只有 `app` 依赖 `server`**；上表 10 个 crate 即「不依赖 server」的全部 crate，`server` 自身是红窗口本体。因此这条子集不是抽样，而是「不受红窗口影响的全集」。

日志：`merge-u1-integrate-PV1-stage1.log`（前 6 个）、`merge-u1-integrate-PV1-stage1-extra.log`（后 4 个）。
补充：`cargo test --locked --workspace --all-features --exclude server --exclude app` → **exit=0**（全绿，日志 `merge-u1-integrate-PV1-stage2-exclude-red-window.log`），把子集结果在 workspace 选择器下再确认了一次。

### C2 — PV1 阶段 2：workspace 全量（NOT_APPLICABLE / 诊断性执行）

```text
$ cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
exit=101（唯一错误 = 已登记红窗口 E0004，见下节）

$ cargo test --locked --workspace --all-features --no-fail-fast
exit=101（同一 E0004；cargo 在 server 单元失败后中止，未产生任何 "test result:" 行）
```

plan.md 的 PV1 行把阶段 2 定义为「**全部 WP 集成后的**集成基线、候选、主分支」的检查。本轮集成基线只含 WP1+WP2（W1 上游），前置条件未满足，因此阶段 2 **本轮不适用**，上面两次执行为诊断性执行：它们的唯一目的就是取出红窗口的**确切形态**并确认没有第二个成因。

日志：`merge-u1-integrate-workspace-clippy.log`、`merge-u1-integrate-PV1-stage2-workspace-test.log`。

### C3 — PV2：`npm run check`（PASS，这是本次集成最关键的一项）

```text
$ npm run check            # 仓库根，Node v24.19.0 / npm 12.0.2
exit=0
```

十道门禁逐条 PASS（完整输出见 `merge-u1-integrate-PV2.log`）：

```text
schema fixtures OK: 120 valid, 26 invalid (ajv Draft 2020-12), 39 event views bound
command catalog OK: 13 commands
error registry OK: 58 codes across 2 protocols
feature registry OK: 11 feature ids across 2 protocols
contract assets OK: 17 schemas, 159 fixture files, 12 transcript vectors re-encoded from input,
                    20 negative vectors rejected as declared, 2 SAS values recomputed
ACP compatibility matrix OK: 25 methods, 11 updates, 5 content blocks, 3 tool content types,
                    19 capabilities, 8 invariants, 10 test families (71 rows, schema validated by ajv)
doc links OK: 399 relative links, 6751 section refs across 403 markdown files
crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致
contract drift OK: §7 的 36 条 DDL 与 crates\storage-sqlite\src\migrate.rs 逐条一致；
                   §5 的 15 个 trait / 93 个方法签名与 crates\core\src\ports.rs 一致
agentic gate: toolchain/openspec/config/schema/verification skill/AGENTS.md 路由/manifest 全 PASS，
              宿主入口检查 17 个文件 PASS
```

- 「同一基线合并后封闭词表各处仍彼此一致」得到证实：`check:commands` 报 **13 commands**，`check:schemas` 报 **120 valid / 26 invalid**，`check:assets` 报 **159 fixture files / 17 schemas**——与 WP2 分支上的 PV2 数字（13 commands、120 valid / 26 invalid）一致，说明**没有**出现「两包各自绿、合并后漂移」的经典集成失败。
- `check:drift`、`check:boundaries`、`check:acp`、`check:docs` 也都在合并后的树上通过（WP1 改的 ACP 矩阵与 WP2 改的 node-link 合同没有互相踩到）。
- 证据状态为 **NEW**（在 `b0a387b…` 上实跑，不是复用两个 WP 分支的 PV2）。

## 红窗口的确切错误清单（已知、有主、有界；merger **未修**）

- **成因（唯一一个）**：WP2 在 `crates/node-link-protocol/src/command.rs` 的 `CommandPayload` 里新增了第 13 个变体 `SessionResume(SessionResume)`（第 740 行），而 `crates/server/src/node_link/command.rs` 的 `core_payload()` 对 wire payload 是**穷尽匹配、无通配臂**，未覆盖该变体。
- **影响 crate**：`crates/server`（lib 与 lib test 两个编译单元）；`crates/app` 因依赖 `server` 而不能被编译（cargo 在 server 失败后中止，因此 `app` 本轮**未被**检查——它是红窗口的下游，不是第二个成因）。
- **完整错误文本**（`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`，exit=101）：

```text
error[E0004]: non-exhaustive patterns: `&node_link_protocol::command::CommandPayload::SessionResume(_)` not covered
    --> crates\server\src\node_link\command.rs:2191:25
     |
2191 |     let payload = match &submit.payload {
     |                         ^^^^^^^^^^^^^^^ pattern `&node_link_protocol::command::CommandPayload::SessionResume(_)` not covered
     |
note: `node_link_protocol::command::CommandPayload` defined here
    --> crates\node-link-protocol\src\command.rs:714:1
     |
 714 | pub enum CommandPayload {
     | ^^^^^^^^^^^^^^^^^^^^^^^
...
 740 |     SessionResume(SessionResume),
     |     ------------- not covered
     = note: the matched value is of type `&node_link_protocol::command::CommandPayload`
help: ensure that all possible cases are being handled by adding a match arm with a wildcard pattern or an explicit pattern as shown
     |
2264 ~         },
2265 +         &node_link_protocol::command::CommandPayload::SessionResume(_) => todo!()
     |

For more information about this error, try `rustc --explain E0004`.
error: could not compile `server` (lib) due to 1 previous error
warning: build failed, waiting for other jobs to finish...
error: could not compile `server` (lib test) due to 1 previous error
```

- **是否只有这一个成因：是。** 判据四条：
  1. 整轮编译只产出 **1 个 rustc 错误**（`grep -c "^error" ` 的另外 2 行是同一次失败的 `could not compile` 摘要）；
  2. `server` lib 与 lib test 两个编译单元各自只有 1 个错误，说明同一文件内也不存在第二个未覆盖的 match；
  3. 全仓库对 wire `CommandPayload`（`WirePayload`）的引用只在 `crates/server/src/node_link/command.rs`（grep 证据：`crates/server/src/node_link/command.rs:76` 的 import 是唯一外部引用点；`crates/app` 只引用 `node_link_protocol::common`/`pairing`），因此不存在第二个误匹配点；
  4. 10 个不受影响的 crate 的 clippy + test 与 `--exclude server --exclude app` 的 workspace 测试都全绿。
- **与登记内容一致**：本基线上的错误位置（`command.rs:2191:25`）、变体名与完整文本与 `plan.md` `## Dependency Handoffs`/DR1-F13 的登记、以及 WP2 自己保存的 `reports/wp2-coder-red-window.log` **逐字节一致**。
- **归属与收口**：WP6（W4，写范围含整个 `crates/server/` 与 `crates/app/`），由它补齐 `core_payload` 的 `SessionResume` 臂与路由。按派发指令，merger **不修**它，也未触碰 `crates/server/`（git diff 证明本基线相对两个源提交没有第三方改动）。
- **对下游的含义**：WP3 的写范围是 `crates/core/`（+ `docs/CORE_PORTS_AND_STORAGE.md`），不依赖 `server`，因此可以从本基线开工；阶段 2 的 workspace 全绿要到 WP6 合入后才可达（plan.md `## Completion Criteria` 也如此要求）。

## 未解决项与下一步建议

1. **红窗口未闭环**（预期如此）：下一步 WP3 从本基线开工；WP3 落地两个必需 trait 方法后，`agent-host`、`storage-sqlite` 既有测试字面量、`server`、`app` 会再出现**新的**已登记涟漪（DR1-F14），这些应由 WP4/WP5/WP6 各自收口，仍**不需要** merger 介入。
2. **本轮没有覆盖的部分**：候选阶段的完整判定（独立 review 对集成新增交互/冲突差异的检视、Coverage Index 核对、`workflow check --stage premerge`）都**未执行**，也不在本轮授权内。若主 Agent 希望把 `b0a387b…` 当作「U1 候选」，必须先补齐这三项，且候选的 `target_revision` 应与本报告一致。本报告的 PASS **只**代表 integrate 阶段条件满足。
3. **`crates/app` 未经编译**：受红窗口连带影响，`app` 的编译/测试证据在 WP6 收口前不可得；本轮也未发现任何指向 `app` 的独立缺陷（WP1/WP2 均未改 `crates/app/`）。
4. **资源**：`CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-du1`（12 GB）**刻意保留**——同一 worktree 的后续候选验证会复用这份缓存，删除会重新付出全量构建成本；该目录为本单元独占，不会被其它 WP 的 target 目录争用。worktree 保留且干净（`git status --porcelain` 空），分支 `integration/session-resume-du1` 保留在 `b0a387b…`，`refs/heads/main` 未移动。
5. **建议**：WP3 开工前，主 Agent 可用一条命令机械复核基线：

```text
git -C D:/Project/acp-remote-wt/session-resume-du1 merge-base --is-ancestor \
  32f71f5d5f08145cfd3e3bddd7d6f6a46a0a5992 b0a387b17f87a9d15d5b07d20f1e16539566c789
```

## handoff_index

```yaml
handoff_index:
  - task_id: "1.4"
    work_package: "WP1, WP2"
    role: merger
    phase: integrate
    round: NOT_APPLICABLE
    stage: candidate
    target_revision: "b0a387b17f87a9d15d5b07d20f1e16539566c789"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "openspec/changes/session-resume/reports/merge-u1-integrate.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: >-
      集成基线 b0a387b 上实跑 PV1 阶段 1 形式（cargo fmt --all -- --check exit 0；对不依赖 server 的
      10 个 crate 逐个 cargo clippy --locked -p <c> --all-targets --all-features -- -D warnings 与
      cargo test --locked -p <c> --all-features，全部 exit 0，见 merge-u1-integrate-PV1-stage1.log 与
      -PV1-stage1-extra.log）；另以 cargo test --locked --workspace --all-features --exclude server --exclude app
      复验 exit 0。crate 集合由 cargo metadata 证明为「不依赖 server 的全部 crate」。
    source_evidence: NOT_APPLICABLE
  - task_id: "1.4"
    work_package: "WP1, WP2"
    role: merger
    phase: integrate
    round: NOT_APPLICABLE
    stage: candidate
    target_revision: "b0a387b17f87a9d15d5b07d20f1e16539566c789"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "openspec/changes/session-resume/reports/merge-u1-integrate.md"
    result: NOT_APPLICABLE
    evidence_status: NEW
    applicability_basis: >-
      PV1 阶段 2（workspace 全量）在 plan.md 中的适用前提是「全部 WP 集成后的集成基线、候选、主分支」；
      本轮基线只含 W1 的 WP1+WP2，前提未满足，故本轮不适用。诊断性执行（workspace clippy exit 101、
      workspace test --no-fail-fast exit 101）只看到已登记红窗口 E0004（crates/server/src/node_link/command.rs:2191
      未覆盖 CommandPayload::SessionResume，归属 WP6/W4），与 reports/wp2-coder-red-window.log 逐字节一致；
      除该错误外无第二个编译/测试成因（日志 merge-u1-integrate-workspace-clippy.log、
      merge-u1-integrate-PV1-stage2-workspace-test.log）。不得据此判定阶段 2 通过。
    source_evidence: NOT_APPLICABLE
  - task_id: "1.4"
    work_package: "WP1, WP2"
    role: merger
    phase: integrate
    round: NOT_APPLICABLE
    stage: candidate
    target_revision: "b0a387b17f87a9d15d5b07d20f1e16539566c789"
    evidence_type: CHECK
    evidence_id: PV2
    report_path: "openspec/changes/session-resume/reports/merge-u1-integrate.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: >-
      集成基线 b0a387b 上实跑 npm run check，exit=0，十道门禁全 PASS；封闭词表在合并后仍自洽
      （13 commands / 120 valid + 26 invalid fixtures / 159 fixture files / 17 schemas），
      与 WP1、WP2 各自分支的 PV2 数字一致，说明两包合同改动同基线合并未产生漂移；
      日志 merge-u1-integrate-PV2.log。Node v24.19.0 / npm 12.0.2。证据为本轮新执行，不是复用分支证据。
    source_evidence: NOT_APPLICABLE
  - task_id: "1.4"
    work_package: "WP1, WP2"
    role: merger
    phase: integrate
    round: NOT_APPLICABLE
    stage: candidate
    target_revision: "b0a387b17f87a9d15d5b07d20f1e16539566c789"
    evidence_type: DELIVERY
    evidence_id: NOT_APPLICABLE
    report_path: "openspec/changes/session-resume/reports/merge-u1-integrate.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: >-
      交付物 = 集成基线提交 b0a387b17f87a9d15d5b07d20f1e16539566c789（分支 integration/session-resume-du1），
      由两个 --no-ff 合并提交组成：e8feaf95…（parents 81e350f + 248d9b9f，WP1）与 b0a387b1…
      （parents e8feaf9 + 32f71f5，WP2）。包含关系已核：git merge-base --is-ancestor 248d9b9f… HEAD = 0、
      32f71f5d… HEAD = 0、81e350f… HEAD = 0（无冲突、无手工改动、git status 干净）。
      本轮未合入 refs/heads/main（仍为 81e350f…）、未 push、未 amend。
    source_evidence: NOT_APPLICABLE
```

## 结论

- **result: PASS**（本轮 integrate 范围）。
- **集成基线 SHA：`b0a387b17f87a9d15d5b07d20f1e16539566c789`**（分支 `integration/session-resume-du1`；base `81e350f…`；WP1 `248d9b9…`、WP2 `32f71f5…` 均为其祖先）。
- **PV2：绿**（exit 0，十道门禁全 PASS）。
- **红窗口**：恰好一个 E0004，`crates/server/src/node_link/command.rs:2191` 未覆盖 `CommandPayload::SessionResume`，归属 WP6(W4)，merger 未修；与登记及 WP2 分支日志逐字节一致。
- **下游 WP3**：可以从本基线开工（其写范围 `crates/core/` 不依赖 `server`）；候选/主分支的 workspace 阶段 2 全绿须等 WP6 收口。
