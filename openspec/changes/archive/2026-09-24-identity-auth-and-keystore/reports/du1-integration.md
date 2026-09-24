# DU1 集成报告 — 候选阶段（6.1–6.3）

> 本文件由 DU1 集成执行者（subagent run `d8a7306a-bd96-4901-bf95-1f4a205d3303`）在候选阶段写入。
> **本轮不含合入**：`refs/heads/main` 未被改动，未 push、未建 tag；合入（6.6）与主分支回归（6.7）需主 Agent 另行授权。
> **候选 PASS ≠ 已合入 ≠ 最终验收 PASS。**

## 0. 固定字段

| 字段 | 内容 |
| --- | --- |
| `task_id` | `DU1 / 6.1–6.3（候选阶段）` |
| `role` | integrator（独立集成执行者；不由主 Agent 兼任，不复用实现者/测试 Agent/reviewer） |
| `phase` | **candidate**：6.1 基线复核 + 6.2 候选构成固定 + 6.3 候选 Project Verify（PV1–PV5）全部执行完毕；**未进入 merge 阶段** |
| `agent_context` | subagent run `d8a7306a-bd96-4901-bf95-1f4a205d3303`（agent = `worker`，`mode = single`）；**新建上下文，未继承任何产品实现/集成对话**（仅持有主 Agent 传入的角色模板全文与 Required Inputs）；cwd = `D:\Project\acp-remote`；本轮为**唯一集成执行者**（重复实例已被主 Agent 停止，见 §7） |
| `target_revision` | candidate = `ed98f6d9c390ca1d26423a8bfebebeff936736fc`（`feat/identity-auth-and-keystore` 分支 tip）；base = `refs/heads/main` = `30f0d78b803b94346ebd8972078febceec8af8d3` |
| `scope` | 6.1–6.3：基线/候选提交固定、DU1 构成与依赖包含关系核对、`cargo build`、Project Verify [PV1]–[PV5] 与证据重建。**不**编辑源码/文档/`Cargo.toml`/`Cargo.lock`/`plan.md`/`tasks.md`/`verification.md`；**不**合入；**不** push；**不**发布 |
| `changes` | **无代码改动**（`git diff --stat c1fd65d..HEAD -- crates/ Cargo.toml Cargo.lock` 为空，说明候选的源码内容与最近一轮已复核的代码 revision 一致；`git status --porcelain` 无源码/文档改动）。本 run 新建/重建的证据文件：`reports/du1-baseline.log`、`du1-composition.log`、`du1-candidate-build.log`、`du1-pv1.log`、`du1-pv2.log`、`du1-pv3.log`、`du1-pv4.log`、`du1-pv5.log`，以及本报告 `reports/du1-integration.md`（未 `git add`，未提交） |
| `checks` | §2（6.1）/ §3（6.2）/ §4（6.3，PV1–PV5 全绿） |
| `issues` | ① 编排层重复派发造成的**双写者并发污染**（已定性、已终止重复实例、已由本 run 单写者重建全部证据，见 §7）；② `%TEMP%` 下有 6 个 22:10–22:43 的历史残留 `acpr-keystore-atomic-*` 目录（**非本 run 产生**，未删除，见 §8）；③ 本报告文件当前未提交（下一步由主 Agent 决定何时随记录提交） |
| `result` | **PASS**（候选阶段；PV1–PV5 全部 exit 0 且证据自洽）——**不等于已合入，也不等于最终验收 PASS** |
| `evidence_paths` | `openspec/changes/identity-auth-and-keystore/reports/du1-baseline.log`（6.1）、`du1-composition.log`（6.2）、`du1-candidate-build.log`、`du1-pv1.log`、`du1-pv2.log`、`du1-pv3.log`、`du1-pv4.log`、`du1-pv5.log`、本报告 `du1-integration.md` |
| `resource_cleanup` | 未另开 worktree / 未设 `CARGO_TARGET_DIR` / 未启动后台进程；使用共享 `target/`（串行，无并发写）。未创建成功的临时 keystore 目录（PV5 与 PV4 自建自删，见 §8）。**无本 run 遗留资源** |

## 1. 交接与工作区

- 单元/阶段：`DU1 / integrated`，唯一交付单元，顺序**先候选、后合入**；本轮止于候选。
- 分支/worktree：**未另开 worktree**——按 `plan.md` Delivery Units 行使用**主 worktree**：`D:\Project\acp-remote`（`git worktree list` 仅一条：`D:/Project/acp-remote ed98f6d [feat/identity-auth-and-keystore]`）。
- 提交固定值：source/base = `30f0d78`（`refs/heads/main`，主线未前进）；candidate = `ed98f6d`；main = `30f0d78`（本轮未动）。
- 依赖包含关系：WP1 `ce5b3fc` → WP2 `5c9d2c4` → WP3 `627bb8e` → WP4 `6861046`，其后 RV1–RV9 修复与记录提交，共 **21** 个提交，全部落在 `30f0d78..ed98f6d`。`git rev-list --count 30f0d78..HEAD` = 21，`git log --merges` = 0（线性，无合并提交）。
- crate/依赖方向：`identity-keystore -> identity-auth`；`identity-auth -> core / sync-protocol / node-link-protocol / acpr-transcript`（`cargo metadata --locked` 实测，见 `du1-composition.log` §3）；由 `check:boundaries` 与 `docs/MODULE_ARCHITECTURE.md` §5 一致（10 个 crate 全登记）。
- 冲突解决差异：**无**（候选自 base 线性构成，无 merge、无冲突解决动作）。
- 远端操作授权依据：**无**（未获远端授权；本轮未发生任何远端写入/推送/发布/回滚）。
- review / 关键 E2E 引用：候选轮**尚未**有独立 reviewer 报告（6.4 由主 Agent 另派**不继承实现/集成对话**的 reviewer）；关键 E2E = **NOT_APPLICABLE**（`plan.md` Main E2E `mode: not-applicable`、`x-agentic.e2e.command` 为空、已有用户降级批准），替代验证由主 Agent 在 7.1 执行；本 run 未改 mode、未执行 E2E。
- 基线复核与防竞态记录：检查前后各执行 `git rev-parse refs/heads/main` = `30f0d78`（复核于 23:15 与 23:19，两次一致，主线未前进）；`git status --porcelain --untracked-files=all` = 空；`tasklist //FI "IMAGENAME eq cargo.exe"` 在执行前为空，且全程**单写入者**（唯一并发执行者已于 23:18 前被主 Agent 停止并确认 `cargo.exe` 实例为 0）。
- 证据复用判断：**不复用** `c1fd65d` 的 `rv8-*` 结论作为候选轮证据——候选 revision 为 `ed98f6d`，与 `c1fd65d` 虽只差变更目录内的 md 记录提交（`git diff --name-only c1fd65d..HEAD` 仅 `plan.md`/`tasks.md`/`verification.md`/`reports/rv{7,8,9}-final.md`，`crates/` 与 `Cargo.*` 无差异），但按 plan 要求候选轮必须在候选提交上重跑，故 [PV1]–[PV5] **全部本轮重跑**。数值与 `rv8-*` 一致（`identity-auth` 79 用例、`identity-keystore` 36 用例、DPAPI 5 用例）可作为交叉印证，不作为替代。

## 2. 6.1 基线复核（`du1-baseline.log`，exit 0）

- `refs/heads/main` = `30f0d78b803b94346ebd8972078febceec8af8d3`（与计划确认时一致，主线未前进）。
- `HEAD` = `ed98f6d9c390ca1d26423a8bfebebeff936736fc`，分支 `feat/identity-auth-and-keystore`。
- `git rev-list --left-right --count refs/heads/main...HEAD` = `0  21`；`git merge-base --is-ancestor refs/heads/main HEAD` 成立（合入可 fast-forward）。
- `git status --porcelain` 与 `--untracked-files=all` 均为空；`git worktree list` 仅主 worktree。
- 工具链：`node v24.19.0`、`npm 12.0.2`、`cargo 1.98.1 (797e8a9bc 2026-08-05)`、`rustc 1.98.1 (48a229cea 2026-09-01)`（仓库 `rust-toolchain.toml` 固定）。

## 3. 6.2 候选构成固定（`du1-composition.log`，exit 0）

| 核对项 | 结论 |
| --- | --- |
| DU1 提交数 | **21**（`30f0d78..ed98f6d`，线性、无 merge） |
| `[workspace] members` | 含 `crates/identity-auth`、`crates/identity-keystore`；`cargo metadata` 实测成员 **10** 个 |
| `Cargo.lock` 登记 | `identity-auth`、`identity-keystore` 与 `getrandom`、`windows-dpapi`、`hmac`(0.13) 均已登记；`cargo metadata --locked` 解析出的依赖清单与 crate 清单一致 |
| 未登记改动文件 | `git diff --name-status 30f0d78..HEAD` 只含预期文件（两个 crate 的 `src/`+`tests/`、`Cargo.toml`/`Cargo.lock`、`README.md`、`AGENTS.md`、`.gitleaks.toml`、`docs/{IDENTITY_AND_AUTH_CONTRACT,MODULE_ARCHITECTURE,DEVELOPMENT_PLAN}.md`、`docs/adr/0008-*.md`、变更目录内的 proposal/design/plan/tasks/verification/specs/reports）；`git status --porcelain --untracked-files=all` = 空 |
| 关键证据登记 | `verification.md` 登记最近一轮 RV8 证据（`rv8-pv1..pv5.log`、`rv8-linux-clippy.log`、`rv8-mutation.log`、`rv8-selfcheck-rg.log`）与 `rv9-final.md`，文件均存在；`plan.md`/`verification.md` 登记 DU1 证据名为 `reports/du1-pv1.log`、`reports/du1-main-verify.log`（合入阶段用）、`reports/du1-integration.md` |
| 候选构建 | `cargo build --locked --workspace --all-features` → `Finished dev profile … in 0.43s`，**EXIT=0**（`du1-candidate-build.log`） |

## 4. 6.3 候选 Project Verify（全部本轮重跑，单写入者）

| Check | 完整命令 | 退出码 | 日志（字节 / NUL / 末行） | 用例数 | 判定 |
| --- | --- | --- | --- | --- | --- |
| [PV1] | `npm run verify`（= `npm run check` + `cargo fmt --all -- --check` + `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` + `cargo test --locked --workspace --all-features`） | **0** | `du1-pv1.log`：48902 B / 0 NUL / `EXIT=0` | workspace 合计 **513 passed / 0 failed / 2 ignored**（68 个测试二进制全 `ok`）；`npm run check` 十道门禁：`Totals: 9 passed, 0 failed (9 items)`、`doc links OK: 369 relative links, 3826 section refs across 186 markdown files`、`crate boundaries OK: 10 个 crate…`、schema fixtures 117 valid/23 invalid、command catalog 12、error registry 58、feature registry 11、contract assets OK、ACP 矩阵 OK、drift OK、agentic OK | **PASS** |
| [PV2] | `node scripts/check-crate-boundaries.mjs` | **0** | `du1-pv2.log`：387 B / 0 NUL / `EXIT=0` | 无用例（脚本判定）：`crate boundaries OK: 10 个 crate 的依赖方向与 §5 矩阵一致（10 个已在矩阵登记）` | **PASS** |
| [PV3] | `cargo test --locked -p identity-auth --all-features` | **0** | `du1-pv3.log`：6220 B / 0 NUL / `EXIT=0` | **79 passed / 0 failed / 0 ignored**：authorization 15 + handshake 20 + pairing 26 + parity 2 + ports 9 + transcripts 5 + doc-tests 2（lib 单测二进制 0 用例，属该 crate 现状） | **PASS** |
| [PV4] | `cargo test --locked -p identity-keystore --all-features` | **0** | `du1-pv4.log`：3725 B / 0 NUL / `EXIT=0` | **36 passed / 0 failed / 0 ignored**：lib 3 + dpapi 5 + entry_format 6 + fail_closed 10 + keystore 12（Windows 上平台用例未被跳过） | **PASS** |
| [PV5] | `cargo test --locked -p identity-keystore --all-features dpapi -- --nocapture` | **0** | `du1-pv5.log`：1873 B / 0 NUL / `EXIT=0` | **5 passed / 0 failed**：`dpapi_wraps_and_unwraps_without_leaking_plaintext`、`dpapi_rejects_wrong_entropy_and_tampering`、`dpapi_provider_credentials_round_trip`、`dpapi_entry_round_trip_signs_in_process`、`dpapi_entropy_binds_entries_to_their_identity`（其余二进制按过滤器 `0 passed … N filtered out`）；日志为 `--nocapture` 原始输出 | **PASS** |

- **执行顺序**：串行 `baseline → composition → build → PV1 → PV2 → PV3 → PV4 → PV5`。执行前 `tasklist //FI "IMAGENAME eq cargo.exe"` 为空；[PV5] 期间无任何第二个 `cargo`/测试进程，满足 `plan.md` §Runtime Resources 的 DPAPI **独占**要求。
- **零用例/全跳过复核**：无全跳过的检查；PV1 中若干 `Doc-tests` 二进制为 `running 0 tests` 属仓库既有形态（与 `rv8-pv1.log` 一致），不影响「检查整体非空」判据；[PV3]/[PV4]/[PV5] 均有用例被实际执行。
- **证据自检**：8 个日志逐个确认 `size > 0`、**NUL 字节 = 0**、末行 `EXIT=0`、头部 `### revision：ed98f6d（工作区干净）`，且头部含各自完整命令、baseline `refs/heads/main = 30f0d78…`、日期/时间戳。
- **新跑 vs 复用**：PV1–PV5 **全部为候选轮新跑**；未复用 `rv8-*`（revision 不同），仅在 §1 说明其数值一致关系。

## 5. 候选阶段工作区状态

- `git status --porcelain --untracked-files=all` = **空**；`git diff --cached --name-only` = 空（**无暂存文件**）。
- `reports/**/*.log` 按 `.gitignore` 第 20 行不入库，因此 8 个 `du1-*.log` 不出现在 `git status` 中；本报告 `du1-integration.md` 为 `.md`（该目录下 `.md` 属入库类型），写入后为**未跟踪**文件（未 `git add`，未提交），与「除你新增的日志与报告外必须干净」的口径一致。
- `refs/heads/main` 仍为 `30f0d78`（与检查前一致）；`git log -1 refs/heads/main` 未变化。

## 6. 失败/受阻与恢复记录

- 本轮无 FAIL：PV1–PV5 与 `cargo build` 全部 exit 0。
- 本轮**未**按 `plan.md` 生成 `reports/du1-main-verify.log`——该证据属于**合入后的主分支 [PV1]**（6.7），本轮不执行合入，故不生成；主 Agent 授权合入时一并补跑。
- 若后续基线前进（`refs/heads/main` ≠ `30f0d78`）：候选需重建并重跑 [PV1]–[PV5]；本轮证据的适用性以 `### baseline` 行为准。

## 7. 事故记录：双写者并发污染（如实登记）

**事实**：同一主 Agent 会话派出了**两个** DU1 集成 worker：

| run | 启动时间 | 结果 |
| --- | --- | --- |
| `41861676-bd06-4adb-9a54-fee2f0327518`（rootRunId `a3babdcf…`，pid 46192） | 2026-09-24 23:14:35 | 被用户/主 Agent **强制停止**（`stopped`，`cargo.exe` 实例 0） |
| `d8a7306a-bd96-4901-bf95-1f4a205d3303`（rootRunId `3ad2ad3a…`，pid 29944，**本 run**） | 2026-09-24 23:15:23 | 按主 Agent 裁决**独占** DU1 6.2/6.3 的 canonical 证据路径 |

两个实例被要求写入**同一组**仓库路径 `openspec/changes/identity-auth-and-keystore/reports/du1-*.log`，并以 `>`（截断）+ `>>`（追加）同时重定向 `npm run verify`：

- 23:15:16 重复实例写 `du1-baseline.log`；23:15:28 写 `du1-composition.log`；23:15:39 写 `du1-candidate-build.log`。
- ~23:16:0x 两实例先后截断并追加 `du1-pv1.log`，各自运行 `npm run verify`。
- 23:16:27 该文件稳定在 **96573 字节，其中 14117 字节为 NUL**（约 14.6%），同时含两实例的内容与收尾标记 → **不可作为 [PV1] 证据**；本 run 那次 `npm run verify` 自身退出码虽为 0，但日志已被并发写坏，**未**被采信为证据。
- 23:16:54 重复实例也已自行发现该现象（其 bash 在做 NUL/md5 稳定性诊断）——竞态为双向可见。
- 定性：**编排层重复派发的环境事故，不是产品缺陷**（不派实现者修复）；虽未实际执行并发 DPAPI，但若不终止，[PV5] 的独占要求将被违反。

**处置（按主 Agent 裁决 A）**：本 run 保留为唯一写入者；**先删除** 4 个可能受污染的日志（`du1-baseline.log`、`du1-composition.log`、`du1-candidate-build.log`、含 14117 NUL 的 `du1-pv1.log`），再**由单一写入者重建全部 8 个日志**并在每个日志上自检（NUL = 0、末行 `EXIT=0`、头部 revision `ed98f6d`）。重建脚本未使用 `du1-pv1.log` 的旧内容；重建后新文件大小/NUL 见 §4。重建即本节的事实闭环：**受损证据已作废，当前证据全部为本 run 单写者产出**。

## 8. 资源与清理

- 未另开 worktree，未设置 `CARGO_TARGET_DIR`，未启动后台进程；`target/` 为共享构建目录且全程串行使用。
- 临时 keystore 目录（`TempRoot::new`，`%TEMP%\acpr-keystore-<tag>-<pid>-<n>`，析构即删）：PV4 与 PV5 执行前/后快照一致——**本 run 未新增任何残留**。现存 6 个 `/tmp/acpr-keystore-atomic-*`（pid 4384/9924/19868/19904/22904/47500，mtime **22:10–22:43**）为**早于本 run**（23:15 启动）的历史残留，非本 run 产生，按「只清理自身产生的临时目录」的口径**未删除**，在此如实登记。
- `reports/` 下无本 run 留下的坏条目目录（PV5 未失败、未留残留）。

## 9. 未解决项与下一步

1. **独立 review（6.4）**：候选 `ed98f6d` 尚无 reviewer 报告；需由**不继承实现/集成对话**的 reviewer 检视候选 diff / 契约 / 证据，重点见 `plan.md` 的 Code Review 段（阻断标准）。
2. **替代验证（7.1）**：E2E 为 `not-applicable`，替代验证由主 Agent 执行并留证。
3. **合入（6.6）与主分支回归（6.7）**：待 6.4/6.5 结论落定后由主 Agent 另行授权；合入时需再核对 `refs/heads/main` 基线（当前 `30f0d78`，若前进则重建候选并重跑 [PV1]–[PV5]），合入后补 `reports/du1-main-verify.log`。
4. **本报告文件的提交时机**：`reports/du1-integration.md` 目前未跟踪（未 `git add`）；是否随记录提交由主 Agent 决定。
5. **CI 专属判定未在本地执行**：`cargo-deny`（`deps`/`advisories`）与 `gitleaks`（`secrets`）本地无等价物，本变更新增的 `getrandom` 与 DPAPI wrapper 的许可证/来源/advisory 判定**只在 CI 完成**，本地 `npm run verify` 不是该判定的证据（见 `plan.md`「未执行项」）。

**本阶段结论：候选 PASS（PV1–PV5 全绿、证据单写者重建并自检通过）；未合入；不等于最终验收 PASS。**

---

# 第二轮：新候选 `4d988cd` 重跑（6.2/6.3）与合入阶段（6.6/6.7）

> 上文 §0–§9 描述的是**上一轮候选 `ed98f6d`**（含其双写者事故与重建）。本轮候选变更为 `4d988cd`，按计划**不得沿用**上一轮 `du1-pv*.log` 结论：上一轮日志已由主 Agent 删除，本轮由本 run 作为唯一写入者**全部重跑重建**。就候选 `4d988cd` 而言，**本文件的下述内容取代上文 §2–§6 的对应该轮结论**（§7 事故记录与 §8 资源记录仍然有效）。

## 10. 候选变更缘起（主 Agent 已完成的 6.4/6.5）

- **6.4 独立 review = PASS**：`reports/rv1-du1.md`（在 revision `27938ca` 时点检视候选 `ed98f6d`），登记 **5 条 P2、0 条 P0、0 条 P1**。
- **修复提交 `4f9c418`**（`fix(identity-auth): 去掉正常路径的 expect 并订正状态口径与合同重复块`）闭环 5 条 P2：合同 §2 类型归属（`KeyPurpose`/`SecretPurpose`/`KeyHandle`/`SecretBytes` 改为「由 `identity-auth` 定义、`identity-keystore` 实现」，并新增 `FeatureList`/`ClientKind`/`PairingProof` 同名指认）、`§5.1` 补登记 `Authority::new`、`§5.2` 删除重复的「挑战缓存有硬上限」、`MODULE_ARCHITECTURE.md §4.1` 与 `CORE_PORTS_AND_STORAGE.md`（文档头 + §11）三处状态句、`types.rs` 的 `PairingSecret::digest()` 改为 `Result<Digest, InvalidValue>` 并在 `pairing.rs` 用 `?` 传播（**本轮唯一代码改动**）、`port.rs` 悬空 doc 注释。
- **定向复核 = PASS**：`reports/rv2-du1.md` —— 5 条 P2 全部闭环；`digest()` 改动行为等价、无新增失败语义；`identity-auth/src` 的 `unwrap/expect` 命中由 17 → **16**（且 16 处全在 `identity-keystore` 测试模块与一处既有登记项）。
- `4d988cd` 订正 `openspec/config.yaml` 项目画像（8 → 10 个 workspace 成员，两个身份 crate 记为已落地）并归档上述两份 review 报告。

## 11. 6.1/6.2 重跑（新候选固定）

- 复核结果（`du1-baseline.log`，2346 B / 0 NUL / `EXIT=0`）：`refs/heads/main` = `30f0d78b803b94346ebd8972078febceec8af8d3`（**仍未前进**）；`HEAD` = `4d988cde3686376683685d91f5f7cf6f345a39c1`（分支 `feat/identity-auth-and-keystore`）；`git rev-list --left-right --count refs/heads/main...HEAD` = `0  24`；`git merge-base --is-ancestor refs/heads/main HEAD` 成立（可 fast-forward）；`git status --porcelain` 空；`git worktree list` 仅主 worktree；执行前 `tasklist //FI "IMAGENAME eq cargo.exe"` 无匹配进程。
- 构成核对（`du1-composition.log`，11999 B / 0 NUL / `EXIT=0`）：**24** 个提交（`30f0d78..4d988cd`，`git log --merges` = 0，线性）；workspace 成员 **10** 个、含 `identity-auth` 与 `identity-keystore`；`Cargo.lock` 登记两个新成员与 `getrandom`/`windows-dpapi`/`hmac`；`git diff --name-status 30f0d78..4d988cd` 只含预期文件、`git status --porcelain --untracked-files=all` 为空；`reports/` 内 `rv1-du1.md`、`rv2-du1.md`、`du1-selfcheck-rg.log`（主 Agent 自检证据，**本 run 未改动**）与 `du1-integration.md` 均在位并被 `verification.md` 引用。
- 候选构建（`du1-candidate-build.log`，344 B / 0 NUL / `EXIT=0`）：`cargo build --locked --workspace --all-features` → `Finished dev profile … in 0.38s`，**EXIT=0**。

## 12. 6.3 重跑（新候选 `4d988cd`，全部本轮新跑）

| Check | 完整命令 | 退出码 | 日志（字节 / NUL / 末行） | 用例数 | 判定 |
| --- | --- | --- | --- | --- | --- |
| [PV1] | `npm run verify` | **0** | `du1-pv1.log`：48902 B / 0 NUL / `EXIT=0` | workspace **513 passed / 0 failed / 2 ignored**（68 个测试二进制全 `ok`）；`Totals: 9 passed, 0 failed (9 items)`；`doc links OK: 370 relative links, 3987 section refs across 189 markdown files`；`crate boundaries OK: 10 个 crate…` | **PASS** |
| [PV2] | `node scripts/check-crate-boundaries.mjs` | **0** | `du1-pv2.log`：387 B / 0 NUL / `EXIT=0` | 脚本判定：10 个 crate 与 §5 一致 | **PASS** |
| [PV3] | `cargo test --locked -p identity-auth --all-features` | **0** | `du1-pv3.log`：6141 B / 0 NUL / `EXIT=0` | **79 passed / 0 failed**（authorization 15 + handshake 20 + pairing 26 + parity 2 + ports 9 + transcripts 5 + doc-tests 2） | **PASS** |
| [PV4] | `cargo test --locked -p identity-keystore --all-features` | **0** | `du1-pv4.log`：3638 B / 0 NUL / `EXIT=0` | **36 passed / 0 failed**（lib 3 + dpapi 5 + entry_format 6 + fail_closed 10 + keystore 12） | **PASS** |
| [PV5] | `cargo test --locked -p identity-keystore --all-features dpapi -- --nocapture` | **0** | `du1-pv5.log`：1880 B / 0 NUL / `EXIT=0` | **5 passed / 0 failed**；执行前/后 `%TEMP%` 临时 keystore 目录数不变（6 个 22:10–22:43 的历史残留，非本 run 产生） | **PASS** |

- 8 份日志（`du1-baseline/composition/candidate-build/pv1..pv5.log`）逐个自检：`size > 0`、**NUL = 0**、末行 `EXIT=0`、头部 `### revision：4d988cd（工作区干净）`，且头部含完整命令、`refs/heads/main` 基线与时间戳。
- 执行顺序串行（baseline → composition → build → PV1 → PV2 → PV3 → PV4 → PV5），[PV5] 期间无第二个 `cargo`/测试进程（DPAPI 独占满足）。
- **适用性判断**：本轮**全部新跑**；与上一轮（`ed98f6d`）及 `rv8-*`（`c1fd65d`）的数值一致（513/79/36/5）仅作交叉印证，**不**作为替代证据。
- **候选阶段结论：PASS**（候选 PASS ≠ 已合入 ≠ 最终验收 PASS —— 合入结果见 §13）。

## 13. 合入阶段（6.6 merge，本地 FF）

### 13.1 合入前防竞态核对

- `git rev-parse refs/heads/main` = `30f0d78b803b94346ebd8972078febceec8af8d3` → **与计划基线一致（未前进），判据满足，继续合入**（若已前进则按计划停止并重建候选）。
- `git status --porcelain --untracked-files=all` = 空；`git worktree list` = 仅 `D:/Project/acp-remote [feat/identity-auth-and-keystore]`（`main` 未被其它 worktree 占用）。
- 合入时点：2026-09-24 23:34（本地）。

### 13.2 执行的命令与实际结果

```text
git switch main              # Switched to branch 'main'（此前 main 未被任何 worktree 占用）
git merge --ff-only 4d988cd  # Fast-forward（无 merge 提交、无冲突、无手工介入）
```

- 合入前 `refs/heads/main` = `30f0d78b803b94346ebd8972078febceec8af8d3`
- 合入后 `refs/heads/main` = **`4d988cde3686376683685d91f5f7cf6f345a39c1`**
- `git log --oneline -1` = `4d988cd docs(openspec): 归档 DU1 候选复核报告并订正项目画像成员数`
- 未使用 `--no-ff`、未手工 merge、未 rebase、未 amend、未回滚。

### 13.3 tree 一致性核对

- `git rev-parse refs/heads/main^{tree}` = `125da8538e740487db3715bd3dfff658ebe99d18`
- `git rev-parse 4d988cd^{tree}` = `125da8538e740487db3715bd3dfff658ebe99d18`（**相同**）
- `git diff --stat 4d988cd refs/heads/main` = **空**（合入后 main 的内容与候选逐字节一致）
- `feat/identity-auth-and-keystore` 分支引用保持指向 `4d988cd` **未变**（`git branch -v` 两行均 `4d988cd`）。

### 13.4 未执行远端操作声明

- **未** `git push`、**未**更新远端跟踪引用（`refs/remotes/origin/main` 仍停在 `30f0d78`，`main` 显示 `[ahead 24]`）、**未**建 tag、**未**发布、**未**回滚、**未**改动 `main` 的历史。
- 授权依据：用户决定「仅本地合入 main」；远端写入不在授权内。

## 14. 6.7 主分支回归（合入后 `main` 实际提交 `4d988cd`）

单一日志 `reports/du1-main-verify.log`（61546 B / **0 NUL** / 末行 `ALLDONE=1`），头部列出五条命令与 revision `4d988cde3686376683685d91f5f7cf6f345a39c1`。

| Check | 命令（在合入后 main 上执行） | 退出码 | 结果 | 与候选轮的关系（适用性） |
| --- | --- | --- | --- | --- |
| [PV1] | `npm run verify` | **0** | 513 passed / 0 failed / 2 ignored；`Totals: 9 passed, 0 failed (9 items)`；`doc links OK: 370 relative links, 3987 section refs across 189 files`；边界 10 crate | **本轮新跑**；与候选轮逐项一致（合入为 FF，内容相同 → 预期一致，实测印证） |
| [PV2] | `node scripts/check-crate-boundaries.mjs` | **0** | `crate boundaries OK: 10 个 crate…` | **本轮新跑**；与候选轮一致 |
| [PV3] | `cargo test --locked -p identity-auth --all-features` | **0** | **79 passed / 0 failed**（与候选轮同分布） | **本轮新跑**（合入后重跑，不沿用候选轮结论） |
| [PV4] | `cargo test --locked -p identity-keystore --all-features` | **0** | **36 passed / 0 failed** | **本轮新跑** |
| [PV5] | `cargo test --locked -p identity-keystore --all-features dpapi -- --nocapture` | **0** | **5 passed / 0 failed**（DPAPI 串行独占） | **本轮新跑**；执行前后 `%TEMP%` 临时 keystore 目录数不变（6 个历史残留，非本 run 产生） |

- 主分支回归**全部 PASS**：`du1-main-verify.log` 内 5 处 `EXIT=0`，无 `FAILED`/`panicked`。
- 与候选轮结论的关系：合入为 **fast-forward**，`main^{tree}` 与候选 tree 相同，因此预期结果应逐项一致；本节仍为**在合入后 `main` 上实际执行**的结果，不属复用。

## 15. 第二轮资源与状态

- 工作区：`git status --porcelain --untracked-files=all` = 空（`reports/**/*.log` 按 `.gitignore` 不入库）；暂存区为空；**无未登记改动**。
- 分支/worktree：当前 HEAD 检出分支 = `main`（按 6.6 指令 `git switch main` 后未再切回）；`git worktree list` = 仅主 worktree `D:/Project/acp-remote`；`git branch -v`：`feat/identity-auth-and-keystore 4d988cd`（未变）与 `main 4d988cd [ahead 24]`。
- 资源：未另开 worktree、未设 `CARGO_TARGET_DIR`、无后台进程；DPAPI 用例自建自删，本 run **零残留**；`reports/du1-selfcheck-rg.log`（主 Agent 证据）未被本 run 触碰。
- CI 专属判定（`cargo-deny` 的 `deps`/`advisories`、`gitleaks` 的 `secrets`）本地无等价物，仍未在本地执行，结论只能来自 CI。

**第二轮结论：候选重跑 PASS → 本地 FF 合入 `refs/heads/main` 成功（`30f0d78` → `4d988cd`）→ 主分支回归 PASS；未执行任何远端操作；本轮不等于最终验收 PASS（6.8 归档与最终验收由主 Agent 依据其全局证据判定）。**
