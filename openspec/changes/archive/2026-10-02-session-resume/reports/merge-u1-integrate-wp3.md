# U1 集成基线报告（第二轮：并入 WP3）（role: merger / phase: integrate / stage: candidate）

> 本报告只覆盖本轮「集成（integrate）」动作：把已验收上游 **WP3** 并入 U1 的固定集成基线，供下游 **WP4 / WP5**（二者声明 `code:WP3`）从该基线开工。
> 本轮**没有**做候选 Project Verify 的完整判定（无独立 review、无 Coverage Index 核对、无 `workflow check --stage premerge`）、
> **没有**合入 `refs/heads/main`、**没有** push。候选 PASS 与已合入 PASS 都不在本报告的结论范围内。

## Shared Report

| 字段 | 值 |
| --- | --- |
| task_id | 1.4（WP4/WP5/… 「在相关集成验证前接入全部已验收上游提交」，本合同单元的落点是把 WP3 纳入集成基线）；关联 5.2 / 6.2 的「集成基线」部分 |
| role | merger（独立合入角色，非任何 WP 的实现者/测试者/reviewer） |
| phase | integrate |
| stage | candidate（`roles/_shared/role-report.md`：merger 的 `integrate` 与 `candidate` 绑同一候选提交） |
| agent_context | 独立 merger 子 Agent，**全新上下文**（未继承任何实现/review 对话），`PI_SESSION_ID=01a0f30e-638f-70d4-b6f2-657567d6b2db`，父会话 `01a0efdd-1c44-73b3-9d5d-19a36a79d6af`，模型 `deepseek-flash`；会话文件 `C:\Users\zhang\.pi\agent\sessions\--D--Project-acp-remote--\2026-09-30T01-10-40-452Z_01a0efdd-1c44-73b3-9d5d-19a36a79d6af\83228243-e3d4-41a7-806d-2cf24cef545c\run-0\session.jsonl` |
| target_revision | `8a08db8e77ac67efee317363ce241261af05c008`（**本轮新增的集成基线固定提交**，分支 `integration/session-resume-du1`） |
| scope | `D:/Project/acp-remote-wt/session-resume-du1`（**复用**本单元已有执行 worktree，未新建、未切换分支）；只做 WP3 的本地合入 + 计划内检查；不改产品代码、不改 plan/tasks/verification/AGENTS.md、不碰任何红窗口 crate |
| changes | 分支 `integration/session-resume-du1` 新增 1 个合并提交：`8a08db8e…`（parents `b0a387b1…` + `4f7a2355…`）。除 git 合并本身外，merger 未产生任何文件改动（`git diff 4f7a2355 HEAD` 为空） |
| checks | PV1（阶段 1 形式，crate 子集）PASS；PV2（`npm run check`）PASS；PV1 阶段 2（workspace 全量）前置条件「全部 WP 集成后」仍未满足 → NOT_APPLICABLE（诊断性执行只看到**已登记**红窗口：`E0046` ×10 + `E0004` ×1） |
| issues | 1 项**计划内、已登记、有主（WP4/WP5/WP6）、有界**的红窗口：并入 WP3 后红窗口从「`server` 一个 `E0004`」**扩大到** `storage-sqlite` / `agent-host` / `server` / `app` 四个 crate（详见下文「红窗口」）。除已登记原因外，本次执行**未发现第二个成因** |
| result | **PASS**（本轮 integrate 的全部适用条件满足：源提交合入无冲突、四源包含关系成立、`core` 与协议/身份 crate 全绿、PV2 全绿、红窗口形态与登记逐点一致） |
| evidence_paths | 本报告 + `reports/merge-u1-integrate-wp3-PV1.log`、`merge-u1-integrate-wp3-PV1-per-crate.log`、`merge-u1-integrate-wp3-PV1-green-set-test.log`、`merge-u1-integrate-wp3-green-set-clippy.log`、`merge-u1-integrate-wp3-workspace-clippy.log`、`merge-u1-integrate-wp3-workspace-clippy-keepegoing.log`、`merge-u1-integrate-wp3-per-crate-red.log`、`merge-u1-integrate-wp3-PV2.log`（均在 `openspec/changes/session-resume/reports/`） |
| resource_cleanup | worktree 保留（本单元候选阶段复用）；`CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-du1` 保留（本单元独占、可复用缓存）；未新建分支、未新建 worktree、未起监听器；`git status --porcelain` 空（merger 未留下未提交改动） |

## 环境（本轮实测）

| 项 | 值 |
| --- | --- |
| 工作目录 | `D:/Project/acp-remote-wt/session-resume-du1`（复用，未新建） |
| 分支 | `integration/session-resume-du1` |
| 合入前 HEAD（= 上一轮集成基线） | `b0a387b17f87a9d15d5b07d20f1e16539566c789` |
| rustc / cargo | `rustc 1.98.1 (48a229cea 2026-09-01)` / `cargo 1.98.1 (797e8a9bc 2026-08-05)`（由仓库 `rust-toolchain.toml` 固定，与 CI 同一编译器）；全部命令带 `--locked` |
| Node / npm | `v24.19.0` / `12.0.2`（`npm run check` 要求 Node ≥ 22.12） |
| CARGO_TARGET_DIR | `D:/Project/acp-remote-target/session-resume-du1`（**每条 cargo 命令显式导出**；该目录为本单元独占，复用上一轮缓存） |
| node_modules | worktree 内为目录联接（指向主工作区）。本轮**未**执行任何 `npm install/ci/update`，**未**对连接执行删除操作 |

合入前状态核实（task 6.1 的「机械核实目标」部分，本轮只核实、未合入）：

```text
$ git status --porcelain         # 无输出（worktree 干净）
$ git rev-parse HEAD             # b0a387b17f87a9d15d5b07d20f1e16539566c789
$ git rev-parse --abbrev-ref HEAD# integration/session-resume-du1
$ git rev-parse refs/heads/main  # 81e350ff340014265eb7c9251237c799d4357fee（= 派发时给的目标引用，逐字一致）
```

## 固定版本与依赖包含关系（证据）

| 角色 | 引用 | 完整 SHA |
| --- | --- | --- |
| base（合入前集成分支 HEAD） | `integration/session-resume-du1` | `b0a387b17f87a9d15d5b07d20f1e16539566c789` |
| 源：WP3 | `agentic/session-resume-wp3` | `4f7a23554f76915cf5f29cc23e292ce270e014c9` |
| WP3 的 base | WP3 分支起点 | `b0a387b17f87a9d15d5b07d20f1e16539566c789`（= 合入前 HEAD，故无第三方前置合入） |
| **candidate / 集成基线（本轮）** | `integration/session-resume-du1` HEAD | **`8a08db8e77ac67efee317363ce241261af05c008`**（parents: `b0a387b1…`, `4f7a2355…`） |
| 目标主分支（本轮**未**合入，未移动） | `refs/heads/main` | `81e350ff340014265eb7c9251237c799d4357fee` |

**四个交付提交 + base 的包含关系实测**（`git merge-base --is-ancestor <rev> HEAD`，退出码 0 = 是祖先；均在新基线 `8a08db8e…` 上实测）：

| 交付提交 | 归属 | 命令退出码 | 结论 |
| --- | --- | --- | --- |
| `248d9b9fa69a1e6375a3b9708fdaa2a127dc7ccb` | WP1 | `0` | 基线包含 WP1 交付提交 |
| `32f71f5d5f08145cfd3e3bddd7d6f6a46a0a5992` | WP2 | `0` | 基线包含 WP2 交付提交 |
| `4f7a23554f76915cf5f29cc23e292ce270e014c9` | **WP3（本轮并入）** | `0` | 基线包含 WP3 交付提交 |
| `81e350ff340014265eb7c9251237c799d4357fee` | 原 base / `refs/heads/main` | `0` | 基线包含主分支起点 |

```text
$ git merge-base --is-ancestor 248d9b9fa69a1e6375a3b9708fdaa2a127dc7ccb HEAD   # exit=0（WP1）
$ git merge-base --is-ancestor 32f71f5d5f08145cfd3e3bddd7d6f6a46a0a5992 HEAD   # exit=0（WP2）
$ git merge-base --is-ancestor 4f7a23554f76915cf5f29cc23e292ce270e014c9 HEAD   # exit=0（WP3，本轮）
$ git merge-base --is-ancestor 81e350ff340014265eb7c9251237c799d4357fee HEAD   # exit=0（原 base）
$ git merge-base HEAD 4f7a23554f76915cf5f29cc23e292ce270e014c9                   # b0a387b1…（= 合入前 HEAD，快进型合并）
$ git diff --stat 4f7a23554f76915cf5f29cc23e292ce270e014c9 HEAD                  # 空 → 合并结果 = WP3 树内容，无第三方改动
```

## 合入过程与冲突解决

```text
$ git merge --no-ff --no-verify agentic/session-resume-wp3 \
    -m "chore(repo): 集成 WP3（core session.resume 恢复语义）到 U1 集成基线"
Merge made by the 'ort' strategy.   # exit=0
 crates/core/src/broker.rs        | 592 ++++++-
 crates/core/src/model/backend.rs |  57 ++-
 crates/core/src/model/config.rs  |   2 +-
 crates/core/src/model/error.rs   |  12 +-
 crates/core/src/model/ids.rs     |  20 ++
 crates/core/src/model/session.rs |   5 +-
 crates/core/src/model/tests.rs   |  81 ++-
 crates/core/src/ports.rs         |  53 ++-
 crates/core/src/use_cases.rs     | 423 ++++++++-
 docs/CORE_PORTS_AND_STORAGE.md   |  24 +-
 docs/MODULE_ARCHITECTURE.md      |   7 +-
 11 files changed, 1245 insertions(+), 31 deletions(-)
```

- **冲突解决：无。** `merge-base(HEAD, WP3) == b0a387b1…`，且 `b0a387b1…` 包含 WP3 的 base，因此本次是**快进型合并**（用 `--no-ff` 保留来源与父提交）；`git status` 无冲突标记，merger 未手工编辑任何文件。
- 写范围核对（与 plan.md `## Shared File Ownership` 一致）：WP3 只改 `crates/core/**`（9 个文件）与 `docs/CORE_PORTS_AND_STORAGE.md`、`docs/MODULE_ARCHITECTURE.md`（2 个文件），共 11 个文件；与本单元已并入的 WP1（`crates/acp-protocol/`、`compatibility/acp/`、`docs/ACP_COMPATIBILITY_MATRIX.md`）、WP2（node-link/identity-auth/`compatibility/commands`/schemas/fixtures/四份文档/脚本）**不相交**；唯一「双写者」`crates/core/src/broker.rs` 在两包里改的是不相邻区域（WP2 = `required_grant` 一条臂；WP3 = 恢复用例与提交点）。
- **没有未提交改动残留**：合入后立刻跑 `git status --porcelain` → 空；全部检查跑完后再次核对 → 仍为空。
- 使用 `--no-verify` 的理由：`.husky/pre-commit` → `scripts/pre-commit.mjs` 会跑 **workspace 全量** `cargo clippy -D warnings`，在**已登记红窗口**内必然被拒（这正是本轮要准确记录、而不是修的东西）。plan.md 的 DR1-F23 明文允许此时用 `--no-verify`，且该钩子按 `AGENTS.md` §8 只是「更早发现失败」而非门禁本体。**判定责任没有因此下移**：下列所有计划内检查都在固定基线上重新手工执行并落盘日志。
- 基线提交 SHA 未做 amend / rebase；`refs/heads/main` 仍为 `81e350f…`；**未 push**。

## Check 记录（逐条命令 / 退出码 / 环境）

所有 cargo 命令：`cwd = D:/Project/acp-remote-wt/session-resume-du1`，`CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-du1`，`HEAD = 8a08db8e77ac67efee317363ce241261af05c008`，`rustc/cargo 1.98.1`，全部带 `--locked`。

### C1 — PV1 阶段 1 形式：`cargo fmt` + 不受红窗口影响的 crate 子集（**PASS**）

```text
$ cargo fmt --all -- --check
exit=0（无输出）
```

| crate | `cargo clippy --locked -p <c> --all-targets --all-features -- -D warnings` | `cargo test --locked -p <c> --all-features` | 用例数 |
| --- | --- | --- | --- |
| **core** | exit=0 | **exit=0** | **136 passed / 0 failed** |
| **acp-protocol** | exit=0 | **exit=0** | **40 passed / 0 failed** |
| **node-link-protocol** | exit=0 | **exit=0** | **40 passed / 0 failed** |
| **identity-auth** | exit=0 | **exit=0** | **83 passed / 0 failed**（含 2 doc-test） |
| sync-protocol | exit=0 | exit=0 | 46 passed / 0 failed |
| acpr-transcript | exit=0 | exit=0 | 24 passed / 0 failed |
| acpr-wire | exit=0 | exit=0 | 20 passed / 0 failed |
| identity-keystore | exit=0 | exit=0 | 36 passed / 0 failed |

上述 8 个 crate 即 `cargo metadata` 证明的**全部「不依赖红窗口 crate」的成员**（依赖图：`app → {agent-host, server, storage-sqlite}`；`server → {core, identity-auth, 三个协议}`；`agent-host → {core, acp-protocol}`；`storage-sqlite → {core, acpr-wire}`）。因此这不是抽样，而是「不受红窗口影响的全集」。

**加一遍更强形式的复验**（workspace 选择器下排除 4 个红窗口 crate）：

```text
$ cargo test --locked --workspace --all-features \
    --exclude server --exclude app --exclude agent-host --exclude storage-sqlite
exit=0   # 425 passed / 0 failed（无任何 "test result: FAILED"）
```

**新增用例数核对**：`core` 由基线 121 → **136**（+15，与 `wp3-coder.md` §3 的 15 个新增 `#[test]` 一致）；三个协议/身份 crate 的 40 / 40 / 83 与上一轮基线**逐字相同**（无回归、无被吞掉的用例）。

日志：`merge-u1-integrate-wp3-PV1.log`（fmt + core + 三 crate 一条命令）、`merge-u1-integrate-wp3-PV1-per-crate.log`（逐 crate 计数）、`merge-u1-integrate-wp3-green-set-clippy.log`（8 crate clippy）、`merge-u1-integrate-wp3-PV1-green-set-test.log`（workspace 排除式测试）。

### C2 — PV1 阶段 2：workspace 全量（**NOT_APPLICABLE**，诊断性执行）

```text
$ cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
exit=101

$ cargo clippy --locked --workspace --all-targets --all-features --keep-going -- -D warnings
exit=101   # 用 --keep-going 取「完整错误集」（默认行为在首个失败后跳过 dependents）
```

plan.md 的 PV1 行把阶段 2 定义为「**全部 WP 集成后的**集成基线、候选、主分支」的检查。本轮基线只含 WP1+WP2+WP3（W1+W2），前置条件未满足，因此**阶段 2 本轮不适用**；上面两次执行为诊断性执行，唯一目的是取出红窗口的**确切形态**并确认没有第二个成因。日志：`merge-u1-integrate-wp3-workspace-clippy.log`（默认）、`merge-u1-integrate-wp3-workspace-clippy-keepegoing.log`（完整集）。

### C3 — 逐 crate 红窗口错误采集（预期失败，**记录不修**）

```text
$ cargo test --locked -p storage-sqlite --all-features   # exit=101
$ cargo test --locked -p agent-host     --all-features   # exit=101
$ cargo test --locked -p server         --all-features   # exit=101
$ cargo test --locked -p app            --all-features   # exit=101
```

日志：`merge-u1-integrate-wp3-per-crate-red.log`（每个 crate 的完整输出与退出码）。

### C4 — PV2：`npm run check`（**PASS，exit 0**）

```text
$ npm run check          # 仓库根（worktree 内），node v24.19.0 / npm 12.0.2
exit=0
```

十道合同门禁逐条 PASS（完整输出见 `merge-u1-integrate-wp3-PV2.log`）：

```text
schema fixtures OK: 120 valid, 26 invalid (ajv Draft 2020-12), 39 event views bound
command catalog OK: 13 commands
error registry OK: 58 codes across 2 protocols
feature registry OK: 11 feature ids across 2 protocols
contract assets OK: 17 schemas, 159 fixture files, 12 transcript vectors re-encoded from input,
                    20 negative vectors rejected as declared, 2 SAS values recomputed
ACP compatibility matrix OK: 25 methods, 11 updates, 5 content blocks, 3 tool content types,
                    19 capabilities, 8 invariants, 10 test families (71 rows, schema validated by ajv)
doc links OK: 399 relative links, 6766 section refs across 403 markdown files
crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致（12 个已在矩阵登记）
contract drift OK: §7 的 36 条 DDL 与 crates\storage-sqlite\src\migrate.rs 逐条一致；
                   §5 的 15 个 trait / 96 个方法签名与 crates\core\src\ports.rs 一致
agentic gate: toolchain / openspec / config / schema / verification skill / AGENTS.md 路由 / manifest 全 PASS，
              openspec validate 19/19，宿主入口检查 17 个文件 PASS
```

**PV2 为何不受红窗口影响**（与派发说明一致，且本轮由实测确认）：`check:boundaries` 只看 `cargo metadata` 的依赖边，`check:drift` 只读 §5/§7 的**文本**与 `ports.rs` 的**文本**，其余为资产校验，**没有任何一道门禁编译 workspace**。

**跨包一致性交叉核对**（防「两包各自绿、合并后漂移」）：

| 指标 | WP3 分支（`wp3-coder-PV2.log`） | 本基线 `8a08db8e…`（本轮） | 判定 |
| --- | --- | --- | --- |
| `§5 trait / method` | 15 / **96** | 15 / **96** | 一致（93 → 96 正是 WP3 的三个新必需方法） |
| `command catalog` | 13 commands | 13 commands | 一致（WP2 的变体数未因 WP3 变化） |
| `schema fixtures` | 120 valid / 26 invalid | 120 valid / 26 invalid | 一致 |
| `contract assets` | 17 schemas / 159 fixture files | 17 schemas / 159 fixture files | 一致 |
| `doc links` | 6766 section refs | 6766 section refs | 一致（相对上一轮基线 6751 的 +15 由 WP3 的文档行解释） |

证据状态为 **NEW**（在 `8a08db8e…` 上实跑，非复用 WP3 分支的 PV2）。

## 红窗口的确切错误清单（已知、有主、有界；merger **未修**）

### 成因（三类，全部预先登记）

1. **WP3 新增三个必需 trait 方法**（均无默认实现，刻意保留「每个后端都必须作答」的能力诚实边界，见 DR1-F14）：
   - `SessionBackendFactory::resume`
   - `SessionEndpoint::agent_session_id`
   - `SessionStore::load_recovery`
2. **WP2 的既有成因（未闭环）**：`CommandPayload` 新增第 13 个变体 `SessionResume`，而 `crates/server/src/node_link/command.rs` 的 `core_payload()` 是穷尽匹配、无通配臂。
3. **字面量涟漪**（WP3 报告 §8.3 已登记）：`SessionUpdate` 新增两个可空列。

### 完整错误清单（`cargo clippy --workspace --all-targets --all-features --keep-going -- -D warnings`，exit=101）

**汇总**：rustc 共报出 **11 条唯一诊断**（`E0046` ×10 + `E0004` ×1），**cargo 报 6 个失败编译单元**，全仓库**没有任何其它错误码**（`grep -oE "^error\[E[0-9]+\]" | sort | uniq -c` → `1 error[E0004]`、`10 error[E0046]`；`grep "^error" | grep -v` 其它形态 → 空）。失败编译单元的错误总数之和为 15（= 2+2+1+1+1+8），高于 11 是因为 rustc 对同一诊断在 `lib` 与 `lib test` 两个编译单元间**去重**（`lib test` 的摘要计入但不再重复打印）。

| # | crate / 编译单元 | 错误码 | 位置 | 缺失/未覆盖 |
| --- | --- | --- | --- | --- |
| 1 | `agent-host` (lib) | `E0046` | `crates\agent-host\src\host.rs:449` `impl SessionBackendFactory for AgentHost` | `resume` |
| 2 | `agent-host` (lib) | `E0046` | `crates\agent-host\src\session.rs:746` `impl SessionEndpoint for Endpoint` | `agent_session_id` |
| 3 | `storage-sqlite` (lib) | `E0046` | `crates\storage-sqlite\src\session_store.rs:1942` `impl SessionStore for SqliteStore` | `load_recovery` |
| 4 | `server` (lib) | **`E0004`** | `crates\server\src\node_link\command.rs:2191:25` `match &submit.payload` | `CommandPayload::SessionResume(_)`（定义于 `crates\node-link-protocol\src\command.rs:740`） |
| 5 | `server` (lib test) | `E0046` | `crates\server\src\local_admin\test_support.rs:817` `impl SessionStore for NotTouched` | `load_recovery` |
| 6 | `server` (lib test) | `E0046` | `crates\server\src\local_admin\test_support.rs:961` `impl SessionStore for FixedStore` | `load_recovery` |
| 7 | `server` (lib test) | `E0046` | `crates\server\src\local_admin\test_support.rs:1021` `impl SessionBackendFactory for NotTouched` | `resume` |
| 8 | `server` (lib test) | `E0046` | `crates\server\src\node_link\command\tests.rs:145` `impl SessionStore for CommandStore` | `load_recovery` |
| 9 | `server` (lib test) | `E0046` | `crates\server\src\node_link\command\tests.rs:437` `impl SessionBackendFactory for FakeBackends` | `resume` |
| 10 | `server` (lib test) | `E0046` | `crates\server\src\node_link\command\tests.rs:484` `impl SessionEndpoint for FakeEndpoint` | `agent_session_id` |
| 11 | `server` (lib test) | `E0046` | `crates\server\src\node_link\resource\tests.rs:265` `impl SessionStore for SliceStore` | `load_recovery` |

```text
error: could not compile `agent-host` (lib) due to 2 previous errors
error: could not compile `agent-host` (lib test) due to 2 previous errors
error: could not compile `storage-sqlite` (lib) due to 1 previous error
error: could not compile `storage-sqlite` (lib test) due to 1 previous error
error: could not compile `server` (lib) due to 1 previous error
error: could not compile `server` (lib test) due to 8 previous errors
Some errors have detailed explanations: E0004, E0046.
```

`E0004` 的完整文本与上一轮基线 / `reports/wp2-coder-red-window.log` **逐字节一致**（`command.rs:2191:25`、同一变体、同一 `help` 段）——即 WP2 的成因**未变化、未被 WP3 放大**。

### 逐 crate 的红窗口形态与「只有已登记原因」的断言

| crate | 实测 | 登记归属 | 是否恰好只有已登记原因 |
| --- | --- | --- | --- |
| `core` | **全绿**（clippy exit=0，136 passed） | — | ✅ 无红窗口 |
| `acp-protocol` / `node-link-protocol` / `identity-auth` | **全绿**（clippy exit=0，40 / 40 / 83 passed） | — | ✅ 无红窗口 |
| `sync-protocol` / `acpr-transcript` / `acpr-wire` / `identity-keystore` | **全绿**（clippy exit=0，46 / 24 / 20 / 36 passed） | — | ✅ 无红窗口 |
| `storage-sqlite` | exit=101；lib 与 lib test 各**恰好 1 个**错误：`E0046 missing: load_recovery`（`session_store.rs:1942`） | WP4（W3） | ✅ **只有**缺 `load_recovery`；无第二个错误码 |
| `agent-host` | exit=101；lib 与 lib test 各**恰好 2 个**错误：`E0046 missing: resume`（`host.rs:449`）、`E0046 missing: agent_session_id`（`session.rs:746`） | WP5（W3） | ✅ **只有**缺 `resume` / `agent_session_id`；无第二个错误码 |
| `server` | exit=101；lib **恰好 1 个**（`E0004`，`command.rs:2191`）；lib test **恰好 8 个**（同一 `E0004` + 7 个 `E0046` 于 4 个替身实现点） | WP6（W4） | ✅ **只有** `E0004`（WP2 遗留）+ 缺 `load_recovery`/`resume`/`agent_session_id`（WP3 涟漪）；无第二个错误码 |
| `app` | exit=101，但**全部错误来自其依赖**（`agent-host` / `storage-sqlite` / `server` 三个 crate 的编译失败）；`app` 自身**从未被编译**（日志无任何 `Checking app` / `could not compile \`app\`` 行） | WP6（W4） | ⚠️ **不可判定**：`app` 自身的 3 个替身实现点（`tests/support/owner.rs:416` `FlakySessionStore`、`:504` `ScriptedBackends`、`:619` `ScriptedEndpoint`）的错误**被上游失败遮蔽**，本轮**无法**断言它们「恰好」是已登记原因。已登记归属（WP3 报告 §8.1/§8.2 第 3 项 + plan 的 Shared File Ownership）指向 WP6，但**证据待 WP4/WP5 收口后重跑** |

**「除已登记原因外没有第二个成因」的断言依据（五条，可机械复现）**：

1. **错误码封闭**：整轮编译只有 `E0046` 与 `E0004` 两种错误码，且 `error[E…]` 行数为 11（10 + 1），不存在 `E0433`/`E0599`/`E0277`/`E0061` 之类的其它断裂；`grep "^error" | grep -v "^error\[E|^error: could not compile|^error: (aborting|Some errors)"` → **空**。
2. **与实现点清单一一对应**：全仓库 `impl SessionStore for` = 7 处、`impl SessionBackendFactory for` = 5 处、`impl SessionEndpoint for` = 4 处，共 16 处。其中 `core` 的 3 处**已实现**全部新方法（`broker.rs:4871` `load_recovery`、`:5679` `resume`、`:5712` `agent_session_id`），**零错误**；错误恰好落在其余 10 处（`storage-sqlite` 1、`agent-host` 2、`server` 7），**没有第 11 处**。
3. **`server` 自身完整**：`server` 的依赖（`core` / `identity-auth` / 三个协议 crate）全部在绿集合内，因此 `server` 的编译是**独立且完整**的（未被上游遮蔽）——它的错误集（1 + 8）就是它的全部错误集，且恰好覆盖 `core_payload` 的 `E0004` 与 `SessionStore`/`SessionBackendFactory`/`SessionEndpoint` 的 7 个替身实现点。
4. **绿集合独立证明**：8 个非红窗口 crate 的 per-crate `clippy -D warnings` 全 exit=0，且 workspace 排除式 `cargo test` exit=0（425 passed / 0 failed）——红窗口没有向绿集合泄漏任何「第二种成因」。
5. **无告警型失败**：`-D warnings` 已把 warning 提升为 error，而 8 个绿 crate 的 clippy 仍 exit=0，说明**不存在**被降级隐藏的 lint 型第二成因。

**本轮唯一「证据缺口」**：`app` 自身错误被遮蔽（上表最后一行的 ⚠️）。它**不改变**本轮对 `storage-sqlite`/`agent-host`/`server` 的精确断言，但意味着「`app` 的失败恰好是已登记原因」这句话本轮**不能**说。

## 对「红窗口会扩大」这一预期形式的核对

派发说明给出的预期表**逐点命中**：

| crate | 派发预期 | 本轮实测 | 判定 |
| --- | --- | --- | --- |
| `core`、`acpr-*`、`acp-protocol`、`node-link-protocol`、`identity-auth` | 应全绿 | clippy exit=0 + test 全绿（136 / 40 / 40 / 83，另有 sync-protocol 46 / acpr-transcript 24 / acpr-wire 20 / identity-keystore 36 亦全绿） | ✅ 一致 |
| `storage-sqlite` | ❌ 缺 `load_recovery`（`E0046`） | ❌ `E0046 missing: load_recovery`，`session_store.rs:1942`，**仅此一个** | ✅ 一致 |
| `agent-host` | ❌ 缺 `resume` / `agent_session_id` | ❌ 两条 `E0046`，`host.rs:449` / `session.rs:746`，**仅此两条** | ✅ 一致 |
| `server` | ❌ `E0004` + 四个替身缺 `load_recovery` | ❌ `E0004`（`command.rs:2191`）+ 7 个 `E0046`（`local_admin/test_support.rs` 3 处、`node_link/command/tests.rs` 3 处、`node_link/resource/tests.rs` 1 处）——即四个替身（`NotTouched`/`FixedStore`/`CommandStore`/`SliceStore`）缺 `load_recovery`，**外加**同批替身 `NotTouched`/`FakeBackends` 缺 `resume`、`FakeEndpoint` 缺 `agent_session_id`（WP3 报告 §8.2 已逐点登记） | ✅ 一致（预期是「四个替身缺 `load_recovery`」，实测为同一批替身 + 同批另两个 trait 方法） |
| `app` | ❌ 随 `server` 一起失败 | ❌ 随 `server`（及 `agent-host`、`storage-sqlite`）一起失败；自身错误被遮蔽 | ✅ 一致（形态一致，但见上文证据缺口） |
| PV2 | 预期全绿 | ✅ exit=0，十道门禁全 PASS | ✅ 一致 |

**结论：红窗口的扩大是本轮预期内的、已被 plan.md「Dependency Handoffs」/DR1-F13/F14 登记的、有明确归属（WP4/WP5/WP6）的形态；merger 未修、不越权修改。**

## 未解决项与下一步建议

1. **红窗口未闭环（预期如此）**：`app` 自身的错误被上游遮蔽，是一条**待补证据**。建议在 WP4（`storage-sqlite`）与 WP5（`agent-host`）各自合入后，重跑一次 `cargo clippy --workspace --all-targets --all-features --keep-going -- -D warnings`，把 `app` 的错误集也取全；或在 WP6 合入前由 WP6 自己在其分支上取。
2. **`storage-sqlite` 的集成测试字面量涟漪被遮蔽**：`crates/storage-sqlite/tests/*.rs`（`commit.rs` 7 处、`session_version_rule.rs` 3 处、`retention.rs` 2 处、`contract_v03.rs` 1 处、`enum_coverage.rs` 1 处，WP3 报告 §8.3）在 lib 编译失败时**无法被编译**，故本轮看不到它们各自是否还缺 `SessionUpdate` 字段。这属于 WP4 的已登记范围，但**不是**本轮可断言的「只有 1 个错误」的例外——本报告的「恰好 1 个」只针对 `lib` / `lib test` 两个编译单元。
3. **本轮没有覆盖的部分**：候选阶段的完整判定（独立 review 对集成新增交互/冲突差异的检视、Coverage Index 37 行覆盖核对、`workflow check --stage premerge`）**未执行**，也不在本轮授权内。若主 Agent 要把 `8a08db8e…` 当作「U1 候选」，必须先补齐这三项，且候选的 `target_revision` 应与本报告一致；但按 plan.md，候选应基于**最新 `refs/heads/main`** 构造，而当前 `refs/heads/main` 仍是 `81e350f…`（未移动），所以本轮基线**只是集成基线**，不是候选。
4. **资源**：`CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-du1` **刻意保留**（同一 worktree 的后续集成/候选验证会复用这份缓存，且为本单元独占）；worktree 保留且干净（`git status --porcelain` 空）；分支 `integration/session-resume-du1` 保留在 `8a08db8e…`；`refs/heads/main` 未移动；未新建 worktree/分支；未 push；未执行 `npm install/ci`，未删除 `node_modules` 联接。
5. **下游开工判据（WP4 / WP5）**：二者声明 `code:WP3`，其起点应是**本报告的固定集成基线 `8a08db8e…`**。WP4 的写范围（`storage-sqlite` + `docs/CORE_PORTS_AND_STORAGE.md` 文件头/§7）与 WP5 的写范围（`agent-host`）都不依赖 `server`/`app`，因此**可以从本基线开工**（红窗口不阻塞它们各自的 crate 子集检查）。建议主 Agent 用一条命令机械复核：

```text
git -C D:/Project/acp-remote-wt/session-resume-du1 merge-base --is-ancestor \
  4f7a23554f76915cf5f29cc23e292ce270e014c9 8a08db8e77ac67efee317363ce241261af05c008
# → exit 0
```

## handoff_index

```yaml
handoff_index:
  - task_id: "1.4"
    work_package: "WP3"
    role: merger
    phase: integrate
    round: NOT_APPLICABLE
    stage: candidate
    target_revision: "8a08db8e77ac67efee317363ce241261af05c008"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "openspec/changes/session-resume/reports/merge-u1-integrate-wp3.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: >-
      集成基线 8a08db8e（合入 WP3 后）上实跑 PV1 阶段 1 形式：cargo fmt --all -- --check exit 0；
      对不受红窗口影响的 8 个 crate（core / acp-protocol / node-link-protocol / identity-auth /
      sync-protocol / acpr-transcript / acpr-wire / identity-keystore）逐个
      cargo clippy --locked -p <c> --all-targets --all-features -- -D warnings 全部 exit 0，
      cargo test --locked -p <c> --all-features 全部 exit 0（136 / 40 / 40 / 83 / 46 / 24 / 20 / 36 passed，
      0 failed）；另以 cargo test --locked --workspace --all-features --exclude server --exclude app
      --exclude agent-host --exclude storage-sqlite 复验 exit 0（425 passed / 0 failed）。
      crate 集合由 cargo metadata 依赖图证明为「不依赖红窗口 crate 的全部成员」。core 用例数 121 → 136（+15），
      与 wp3-coder.md §3 一致；三个协议/身份 crate 计数与上一轮基线逐字相同（无回归）。
      日志：merge-u1-integrate-wp3-PV1.log、-PV1-per-crate.log、-green-set-clippy.log、-PV1-green-set-test.log。
    source_evidence: NOT_APPLICABLE
  - task_id: "1.4"
    work_package: "WP3"
    role: merger
    phase: integrate
    round: NOT_APPLICABLE
    stage: candidate
    target_revision: "8a08db8e77ac67efee317363ce241261af05c008"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "openspec/changes/session-resume/reports/merge-u1-integrate-wp3.md"
    result: NOT_APPLICABLE
    evidence_status: NEW
    applicability_basis: >-
      PV1 阶段 2（workspace 全量）在 plan.md 中的适用前提是「全部 WP 集成后的集成基线、候选、主分支」；
      本轮基线只含 W1（WP1/WP2）+ W2（WP3），前提未满足，故本轮不适用。诊断性执行
      （cargo clippy --locked --workspace --all-targets --all-features [--keep-going] -- -D warnings，exit 101）
      只看到已登记红窗口：E0046 ×10（storage-sqlite/session_store.rs:1942 缺 load_recovery；
      agent-host/host.rs:449 缺 resume、session.rs:746 缺 agent_session_id；server 的 7 个替身实现点）
      + E0004 ×1（server/node_link/command.rs:2191 未覆盖 CommandPayload::SessionResume，WP2 遗留，文本与
      wp2-coder-red-window.log 逐字节一致）。除已登记原因外无第二个成因（错误码封闭、实现点 16 处一一对应、
      server 依赖全绿故其错误集完整、8 个绿 crate 独立全绿）。
      日志：merge-u1-integrate-wp3-workspace-clippy.log、-keepegoing.log、-per-crate-red.log。不得据此判定阶段 2 通过。
    source_evidence: NOT_APPLICABLE
  - task_id: "1.4"
    work_package: "WP3"
    role: merger
    phase: integrate
    round: NOT_APPLICABLE
    stage: candidate
    target_revision: "8a08db8e77ac67efee317363ce241261af05c008"
    evidence_type: CHECK
    evidence_id: PV2
    report_path: "openspec/changes/session-resume/reports/merge-u1-integrate-wp3.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: >-
      集成基线 8a08db8e 上实跑 npm run check，exit=0，十道合同门禁全 PASS。任何门禁都不编译 workspace
      （check:boundaries 用 cargo metadata / check:drift 读 §5↔ports.rs 文本 / 其余为资产校验），
      因此 PV2 不受红窗口影响，本轮 PASS 是「真绿」而非「被跳过」。跨包一致性核对：
      §5 的 15 traits / 96 methods 与 WP3 分支一致，13 commands / 120+26 fixtures / 159 fixture files
      / 17 schemas 与 WP2 基线一致（无合并漂移），doc links 6766 section refs（相对上一轮 6751 的 +15
      由 WP3 文档行解释）。Node v24.19.0 / npm 12.0.2。日志：merge-u1-integrate-wp3-PV2.log。
    source_evidence: NOT_APPLICABLE
  - task_id: "1.4"
    work_package: "WP3"
    role: merger
    phase: integrate
    round: NOT_APPLICABLE
    stage: candidate
    target_revision: "8a08db8e77ac67efee317363ce241261af05c008"
    evidence_type: DELIVERY
    evidence_id: NOT_APPLICABLE
    report_path: "openspec/changes/session-resume/reports/merge-u1-integrate-wp3.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: >-
      交付物 = 新集成基线提交 8a08db8e77ac67efee317363ce241261af05c008（分支 integration/session-resume-du1），
      单个 --no-ff 合并提交（parents b0a387b17f87a9d15d5b07d20f1e16539566c789 + 4f7a23554f76915cf5f29cc23e292ce270e014c9），
      快进型、无冲突、无手工改动（git diff --stat 4f7a2355 HEAD 为空）、git status --porcelain 空。
      四个交付提交的包含关系实测：git merge-base --is-ancestor 248d9b9f… HEAD = 0（WP1）、32f71f5d… HEAD = 0（WP2）、
      4f7a2355… HEAD = 0（WP3）、81e350ff… HEAD = 0（原 base / refs/heads/main）。
      本轮未合入 refs/heads/main（仍为 81e350ff…）、未 push、未 amend。
    source_evidence: NOT_APPLICABLE
```

## 结论

- **result: PASS**（本轮 integrate 范围）。
- **新集成基线 SHA：`8a08db8e77ac67efee317363ce241261af05c008`**（分支 `integration/session-resume-du1`；base/父 `b0a387b1…`；WP3 `4f7a2355…` 已并入；WP1 `248d9b9f…`、WP2 `32f71f5d…`、`refs/heads/main` = `81e350ff…` 均为其祖先）。
- **PV2：绿**（`npm run check` exit 0，十道门禁全 PASS）。
- **红窗口确切错误清单**（11 条唯一诊断，全部为已登记原因，merger 未修）：
  - `E0046 missing: load_recovery` — `storage-sqlite/src/session_store.rs:1942`；`server/src/local_admin/test_support.rs:817`（NotTouched）、`:961`（FixedStore）；`server/src/node_link/command/tests.rs:145`（CommandStore）；`server/src/node_link/resource/tests.rs:265`（SliceStore）
  - `E0046 missing: resume` — `agent-host/src/host.rs:449`（AgentHost）；`server/src/local_admin/test_support.rs:1021`（NotTouched）；`server/src/node_link/command/tests.rs:437`（FakeBackends）
  - `E0046 missing: agent_session_id` — `agent-host/src/session.rs:746`（Endpoint）；`server/src/node_link/command/tests.rs:484`（FakeEndpoint）
  - `E0004 non-exhaustive patterns: CommandPayload::SessionResume(_) not covered` — `server/src/node_link/command.rs:2191:25`（WP2 遗留，文本逐字节一致）
  - 归属：`storage-sqlite` → WP4；`agent-host` → WP5；`server`/`app` → WP6。**`app` 自身错误被上游遮蔽，本轮不可断言**（唯一证据缺口）。
- **WP4 / WP5 能否从该基线开工：能。** 二者声明的 `code:WP3` 已由 `git merge-base --is-ancestor 4f7a2355… 8a08db8e…` = 0 机械证实；它们的写范围（`storage-sqlite`、`agent-host`）都不依赖红窗口 crate，可各自在其 crate 子集上产出 PV1 阶段 1 证据；workspace 阶段 2 全绿须等 WP4/WP5/WP6 全部收口。
