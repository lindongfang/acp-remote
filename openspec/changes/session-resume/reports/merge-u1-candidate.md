# U1 交付单元 — 集成 TP2、构造最终候选、候选级 Project Verify

- **task_id**: `6.2`（构造候选并固定基线/候选版本）
- **role**: `merger`
- **phase**: `candidate`
- **Agent ID**: `merger-A5`
- **上下文方式**: 继承本交付单元 `merger` 实例的集成上下文（`merge-u1-integrate-*` 系列报告为同一集成线程的前序产物），独立于 TP2 的 tester 与四位 reviewer
- **工作目录**: `D:/Project/acp-remote-wt/session-resume-du1`（**复用**，未新建、未切分支）
- **分支**: `integration/session-resume-du1`
- **权威 changeDir**: `D:/Project/acp-remote/openspec/changes/session-resume`
- **result**: **PASS**（本阶段：integrate + candidate + 候选级 PV1/PV2 + 机械核对）
- **result 限定**: 候选 PASS **不等于**已合入 `refs/heads/main`，**不等于**最终验收 PASS，**不等于**归档 PASS

---

## 1. target_revision 与本轮结论所绑定的提交

| 角色 | 完整 SHA | 短 SHA | 说明 |
| --- | --- | --- | --- |
| 本地主分支（**未动**） | `81e350ff340014265eb7c9251237c799d4357fee` | `81e350f` | 目标引用；本轮**未合入** |
| U1 集成基线（合并前 base） | `95051f93db15ff867c3207386e402094790434bb` | `95051f9` | 含 WP1–WP6 |
| TP2 来源分支 tip | `3484541977a1b708fbf9a3234b6ceba0c272845e` | `3484541` | `agentic/session-resume-tp1` |
| **U1 最终候选（本轮固定）** | `2ed142dedec2facf8f6e174d1aa165f549cbc47e` | **`2ed142d`** | merge commit |

**候选父提交对**：`2ed142d` 的父为 `(95051f93db15ff867c3207386e402094790434bb, 3484541977a1b708fbf9a3234b6ceba0c272845e)`，即 `(--no-ff)` 的第一父 = 集成基线、第二父 = TP2 tip。

**候选 tree**：`ff3a9cf4cd00252b9ce685a1f1fed224c33973dc`，与 `3484541^{tree}` **逐字节相同** —— 因 `95051f9` 本就是 TP2 的祖先，本次合并没有在 TP2 之上引入任何额外内容改动（`git diff --quiet 3484541 2ed142d` 退出 0）。

---

## 2. 开工前 workspace 核实

| 项 | 命令 | 实际输出 | 判定 |
| --- | --- | --- | --- |
| HEAD | `git rev-parse HEAD` | `95051f93db15ff867c3207386e402094790434bb` | ✅ 与交底一致 |
| 工作区 | `git status --porcelain` | （空） | ✅ 干净 |
| 分支 | `git rev-parse --abbrev-ref HEAD` | `integration/session-resume-du1` | ✅ |
| 主分支 | `git -C D:/Project/acp-remote rev-parse refs/heads/main` | `81e350ff340014265eb7c9251237c799d4357fee` | ✅ 与交底一致 |

**无不符项，未触发 BLOCKED。** 未执行任何 `reset` / 分支切换 / worktree 新建。

---

## 3. 祖先包含关系（逐条实测）

`git merge-base --is-ancestor <c> HEAD`（`HEAD = 95051f9`，合并前）：

| 提交 | 短 SHA | 结果 |
| --- | --- | --- |
| main | `81e350f` | ANCESTOR_OK |
| WP1 | `248d9b9` | ANCESTOR_OK |
| WP2 | `32f71f5` | ANCESTOR_OK |
| WP3 | `4f7a235` | ANCESTOR_OK |
| WP4 | `4a3882d` | ANCESTOR_OK |
| WP5 | `1376e1b` | ANCESTOR_OK |
| WP6 | `a7bc596` | ANCESTOR_OK |

合并后（`HEAD = 2ed142d`）对 `81e350f`/`248d9b9`/`32f71f5`/`4f7a235`/`4a3882d`/`1376e1b`/`a7bc596`/`3484541` 逐条复测，**8/8 全部 YES**。

`git merge-base HEAD agentic/session-resume-tp1` = `95051f93db15ff867c3207386e402094790434bb`（= 合并前 HEAD），且 `git merge-base --is-ancestor 95051f9 3484541` 为真。

### 3.1 与交底的一处事实性出入（已核实，不影响本轮结论）

交底描述 TP2 含**四个**提交（`f8133f2`、`a02e2fd`、`f44301e`、`3484541`）。实测 `git log 95051f9..3484541` 为**五个**提交：

```
f04a989 parents=95051f9  test(session-resume): 补协议/存储/后端三层恢复用例
f8133f2 parents=f04a989  test(session-resume): 补受控路径恢复用例与恢复观察点
a02e2fd parents=f8133f2  test(session-resume): 补 wire 层命令契约用例与格式修正
f44301e parents=a02e2fd  test(app): 补 R25 规范化结果变化的两族变体并修正 R31 观察点
3484541 parents=f44301e  docs(test): 订正 R25 覆盖分档与判别式注释
```

`f04a989` 是 TP2 的第一提交（Round 1 的基线提交），不在交底枚举内但**确属 TP2 分支**。`git merge-base --is-ancestor f04a989 95051f9` 为假，确认它不在 U1 基线里。**tip `3484541` 与交底一致**，合入内容与范围不受影响；此处如实记录，供主 Agent 校正交底措辞。

---

## 4. 合入动作与冲突解决

| 项 | 内容 |
| --- | --- |
| 命令 | `git merge --no-ff agentic/session-resume-tp1 -m "chore(repo): 集成 TP2（会话恢复测试补强与受控路径恢复用例）到 U1 集成基线"` |
| 退出码 | `0` |
| 策略 | `ort` |
| **冲突** | **零冲突**（输出为 `Merge made by the 'ort' strategy.`，未进入冲突解决流程） |
| 提交信息合规 | `chore(repo): …`，`chore` ∈ `TYPES`、`repo` ∈ `SCOPES`（`commitlint.config.mjs`）；主题中文 ✅ |
| 净内容差异 | `8 files changed, 3510 insertions(+), 11 deletions(-)`；新建 5 个测试文件 |

**Shared File Ownership 合入顺序处理：无适用** —— 零冲突，不需要任何所有权仲裁。TP2 与 WP 的共享落点 `crates/app/tests/support/owner.rs`（`FlakySessionStore`）与 `crates/storage-sqlite/tests/resume_columns.rs` 两侧均已合入且无冲突。

**合入未触及任何需求/接口取舍**，无需上报主 Agent协调。

---

## 5. 本轮关键交付：补做 reviewer 工具受限而无法完成的四项机械核对

CR8 Round 2（`reports/cr8-review-round2.md`「待补证据」表）把前两项交办给 merger，把 CR8-F6 留给 Linux CI。以下为**实际命令与实际输出**。

### 5.1 核对一：产品与测试之外零越界（CR8 待补项 1）

```
$ git diff --name-only 95051f9..3484541
Cargo.lock
crates/acp-protocol/tests/session_resume.rs
crates/agent-host/tests/resume.rs
crates/app/Cargo.toml
crates/app/tests/session_resume_e2e.rs
crates/app/tests/support/owner.rs
crates/node-link-protocol/tests/session_resume_command.rs
crates/storage-sqlite/tests/resume_columns.rs
```

| 过滤 | 命令 | 计数 |
| --- | --- | --- |
| 全部改动 | `git diff --name-only 95051f9..3484541 \| wc -l` | **8** |
| **`crates/*/src/**`（产品代码）** | `git diff --name-only 95051f9..3484541 -- 'crates/*/src/**' \| wc -l` | **0** ✅ |
| **`compatibility/` + `schemas/` + `fixtures/`** | `git diff --name-only 95051f9..3484541 -- compatibility/ schemas/ fixtures/ \| wc -l` | **0** ✅ |
| `crates/*/tests/**` | `git diff --name-only 95051f9..3484541 -- 'crates/*/tests/**'` | **6** ✅ |
| 其余（已授权清单） | `git diff --name-only 95051f9..3484541 -- . ':(exclude)crates/*/tests/**' ':(exclude)crates/*/src/**'` | **2**：`Cargo.lock`、`crates/app/Cargo.toml` |

**结论：✅ 证实**。TP2 全部提交对基线的改动**只**落在 6 个 `crates/*/tests/**` 文件 + 2 个已授权的清单文件。`crates/**/src/**`（产品代码）改动数 **0**；`compatibility/`、`schemas/`、`fixtures/` 改动数 **0**。**零越界。**

### 5.2 核对二：TP2 第 3 轮（`3484541`）的 diff 级证据 —— 断言未改动（CR8-F7 依赖）

```
$ git diff --stat f44301e..3484541
 crates/app/tests/session_resume_e2e.rs        | 22 +++++++++++++++++++---
 crates/app/tests/support/owner.rs             |  9 ++++++---
 crates/storage-sqlite/tests/resume_columns.rs | 19 +++++++++++++------
 3 files changed, 38 insertions(+), 12 deletions(-)
```

**恰好 3 个文件，全部落在 `crates/**/tests/**`**，无任何清单文件、无 `src/**`。（`crates/app/tests/support/owner.rs` 属 `tests/**` 下的辅助模块。）

只看去注释行的过滤：

```
$ git diff -U0 f44301e..3484541 -- <三个文件> \
    | grep -E '^[+-]' | grep -vE '^(\+\+\+|---)' | grep -vE '^[+-]\s*(//|///|//!)'
（无输出；管道末端 grep 退出码 = 1，即零命中）
```

为排除「过滤器把非注释行也吃掉了」这一误判风险，另做对照：**不**去注释的原始 `+/-` 行共 **50** 行，逐条打印后确认 **50/50 全部以 `//` 开头**（`//`、 `///`、`//!` 三种前缀，含 `//!` 模块级说明）。摘录（节选，实际 50 行全量已核对）：

```
+        // 「不存在」侧的额外防泄露断言：`details` 两条路径**实测同为 `{}`（空对象，非 `null`）**——
+        // 两条都走 `command.rs::deny` 的 `CommandFault::NotGranted(None)` 分支（`RawObject::empty()`）。
+        // 所以这是**结构性防泄露守卫**（若将来某条路径回 `NotGranted(Some(parameter))`，本断言会失败），
+        // 而不是当前的判别点；真正的判别力在上一条 `code` 断言与「目录已被删除」这个前提上。
-/// R24 + R25 + CR7-F2：目录复校验的两个失败变体，以及「`NULL` 与能力不支持同路径」的归类。
+/// R24 + R25 + CR7-F2：目录复校验的两个失败变体，以及「能力不支持」的归类。
+/// **本用例不造「两列为 `NULL` 的会话」**（第 ③ 段只把**受控后端**切到
+///   `ResumeBehavior::BackendUnsupported`，会话自身的两列始终有值）。两列 `NULL` 侧的真实证据在别处：
+///   `server/src/node_link/command/tests.rs::session_resume_without_persisted_recovery_data_fails_as_unsupported`
-//! - **两处刻意的测试替身**：① Agent 后端用脚本化端点（真实 ACP 子进程不在本测试范围，且会引入
+//! - **三处刻意的测试替身/测试介入**：① Agent 后端用脚本化端点（…）；③ `overwrite_persisted_workspace_cwd`
+//!   （TP2 第 2 轮新增）经 `sqlx` **原始 SQL** 直接改写 `owned_session.workspace_cwd`，**绕过全部端口**
+//!   与本地管理方法**——这是三者中介入最深的一处：产品写路径永远写入 `canonicalize` 的结果…
-/// 1. `owned_session` 升级后的 DDL **含** `IF NOT EXISTS`（新建/追加文本的写法），**不含**重建用的
+/// 1. `owned_session` 升级后的存储文本以**不带引号**的 `CREATE TABLE owned_session (` 开头（即追加
+///    路径），且**不含**重建用的临时表名（`owned_session_v2` / `_v3` / `_v5`）。
+///    **注意**：`IF NOT EXISTS` **不能**用作判别式——实测 `ALTER TABLE ADD COLUMN` 会把它从存储文本里
+///    去掉，而重建文本本来就没有它，**两条路径都不含**，按它断言会恒真…判别式只认「表名是否带双引号」。
-/// 4. 反证：v1 库升级里**确实会重建**的 `owned_audit`，其文本**不含** `IF NOT EXISTS`——证明本用例
+/// 4. 反证：…其文本以**带双引号**的 `CREATE TABLE "owned_audit" (` 开头（RENAME 的签名）——证明本用例
+///    的判别式不是恒真。若把 ① 的判别式方向写反或改成恒真表达式，本断言会立刻失败。
```

**结论：✅ 证实，CR8-F7 可机械闭环**。`3484541` 对 `f44301e` 的改动**只增改注释与 doc 注释**：38 增 12 删全部落在 `//` / `///` / `//!` 行上，**零行可执行代码/断言改动、零阈值改动、零用例增删**。

> 附带的 CR8-Round1-F1 复核（`3484541` 不得引入 `#[ignore]`）：`git grep -n '#\[ignore' 95051f9` 与 `git grep -n '#\[ignore' 2ed142d` 输出**完全相同且仅 2 处**——`crates/storage-sqlite/tests/commit.rs:1100`（`crash_child`，由 `crash_recovery_leaves_an_consistent_database` 拉起）与 `crates/storage-sqlite/tests/migration.rs:1618`（`regenerate_v1_fixture`，夹具生成器）。**均非 TP2 文件，TP2 新增 `#[ignore]` 数 = 0。** workspace 全量跑出的 2 个 `ignored` 正是这两条既有项。

### 5.3 核对三：`4146611` 的归属（CR8-F2 待补项）

```
$ git cat-file -t 4146611
commit

$ git rev-parse 4146611^{tree} f44301e^{tree}
97bbf785cbafb1c2df22878be9b92d263bcfd7ab
97bbf785cbafb1c2df22878be9b92d263bcfd7ab

$ git log --all --oneline | grep 4146611
（无输出，grep 退出码 = 1）

$ git diff --name-only 4146611 f44301e
（无输出 —— 两提交内容完全一致）
```

进一步并排实测两者的元数据：

| 字段 | `4146611` | `f44301e` |
| --- | --- | --- |
| full SHA | `4146611a2e7002b89534227293d2ad547f5e11de` | `f44301ecfe65aa5c2e860af405cf57b476006000` |
| parent | `a02e2fd` | `a02e2fd`（**同一父**） |
| author | `lindongfang` | `lindongfang` |
| author-date | `2026-10-01T17:50:36+08:00` | `2026-10-01T17:50:36+08:00`（**同一刻**） |
| **committer-date** | `2026-10-01T17:50:36+08:00` | `2026-10-01T17:51:35+08:00`（**相差 59 秒**） |
| subject | `test(app): 补 R25 …` | `test(app): 补 R25 …`（**逐字相同**） |
| **tree** | `97bbf785cbafb1c2df22878be9b92d263bcfd7ab` | `97bbf785cbafb1c2df22878be9b92d263bcfd7ab`（**完全相同**） |

**结论：✅ 说法成立，且证据比要求的更强**。`4146611` 真实存在（类型 `commit`，非悬空对象类型），**tree 与 `f44301e` 逐位相同**（`git diff --name-only` 两者为空）；`git log --all` 全历史无任何 ref 可达它，故它是**悬空（dangling / unreachable）提交**，尚未被 gc 回收。「同一交付 amend 前的 SHA、tree 相同、权威值是 `f44301e`」——**三点全部证实**。同父 + 同作者时刻 + 同主题 + 同 tree、**仅 committer 时刻晚 59 秒**，正是 `git commit --amend` 的标准指纹。

> 资源提示：`4146611` 仍以悬空对象存在于本仓库 object DB。**本轮未运行任何 gc**（`git gc` / `git prune` / `git reflog expire` 均未执行），它随时可供复核。请勿在下游执行 gc 后再要求复核此项。

### 5.4 核对四：清单两行的最终形态

```
$ git diff --numstat 95051f9..3484541 -- Cargo.lock crates/app/Cargo.toml
1	0	Cargo.lock
1	0	crates/app/Cargo.toml
```

**全文 diff**：

```diff
diff --git a/Cargo.lock b/Cargo.lock
index b6017ea..3fd727f 100644
--- a/Cargo.lock
+++ b/Cargo.lock
@@ -132,6 +132,7 @@ dependencies = [
  "serde_json",
  "server",
  "sha2 0.11.0",
+ "sqlx",
  "storage-sqlite",
  "thiserror 2.0.20",
  "tokio",
diff --git a/crates/app/Cargo.toml b/crates/app/Cargo.toml
index d0690ca..20a6ed9 100644
--- a/crates/app/Cargo.toml
+++ b/crates/app/Cargo.toml
@@ -77,6 +77,7 @@ rcgen.workspace = true
 rustls.workspace = true
 tokio-rustls.workspace = true
 rustls-pki-types.workspace = true
+sqlx = { workspace = true }
 
 [lints]
 workspace = true
```

逐项确认：

| 断言 | 实测 | 判定 |
| --- | --- | --- |
| 恰好两行（1 增 + 1 增，0 删） | numstat `1 0` / `1 0` | ✅ |
| **未新增任何 `[[package]]` 条目** | `[[package]]` 计数：baseline `95051f9` = **333**，candidate `3484541` = **333**（无变化） | ✅ |
| **未升级版本** | `name = "sqlx"` 块两侧逐字相同：`version = "0.8.6"`、`source = registry+…crates.io-index`、`checksum = 1fefb893899429669dcdd979aff487bd78f4064e5e7907e4269081e0ef7d97dc`。`sqlx` 的 `[[package]]` 块**在 baseline 已存在**（`95051f9:Cargo.lock:2107`），本次只是把它**挂到 `app` 包的 `dependencies` 列表** | ✅ |
| **未加 feature** | `sqlx = { workspace = true }` —— 单一 `workspace = true`，**无 `features = {…}` 键**；workspace 根 `Cargo.toml` 未出现在 diff 中，故无工作区级 feature 变更 | ✅ |

**结论：✅ 证实**。两处改动都是「把既有 workspace 依赖 `sqlx` 声明到 `app` 的 dev/测试用途所需」的单行登记，`Cargo.lock` 无新包、无版本漂移、无 feature 面扩张。用途见 `crates/app/tests/support/owner.rs` 的 `overwrite_persisted_workspace_cwd`（经 `sqlx` 原始 SQL 改写 `owned_session.workspace_cwd`，见 5.2 摘录）。

### 5.5 核对五（交底第 8 节要求）：`#[cfg(unix)]` 两条 PENDING 的「未执行」机器证据

**如实转达：状态为 PENDING，不记 PASS。**

`crates/app/tests/session_resume_e2e.rs` 有 **2 条 `#[cfg(unix)]` 用例**：

| 行 | 用例名 | 场景 |
| --- | --- | --- |
| `:1406` `#[cfg(unix)]` / `:1408` | `a_persisted_directory_replaced_by_a_symlink_is_refused_before_any_backend_call` | 符号链接改指 |
| `:1512` `#[cfg(unix)]` / `:1514` | `an_inaccessible_persisted_directory_is_refused_before_any_backend_call` | `chmod 000` 打在父目录 |

它们在 Windows 上**从未执行、也没有编译证据**（交叉编译缺 `x86_64-linux-gnu-gcc`）。**PENDING，待 Linux CI `checks` job（`ubuntu-latest`）首次编译 + 执行。** 失败则开新问题编号回 TP2，不改判本轮结论。

**机器证据 A — workspace 全量跑的 app e2e 目标只有 8 条，且不含这两条**（`merge-u1-candidate-PV1-stage2-workspace-test.log` 第 437–450 行）：

```
Running tests\session_resume_e2e.rs (…\session_resume_e2e-e98c2d9733b1faa6.exe)

running 8 tests
test any_resume_payload_field_is_rejected_before_side_effects ... ok
test a_repeated_resume_request_id_replays_the_first_result_once ... ok
test workspace_revalidation_and_null_recovery_data_take_distinct_paths ... ok
test an_unsupported_agent_fails_the_resume_terminal_without_creating_a_session ... ok
test a_persisted_cwd_whose_canonical_form_differs_is_refused_before_any_backend_call ... ok
test an_unauthorized_resume_is_rejected_before_any_local_read ... ok
test a_successful_resume_returns_the_session_reference_and_leaves_the_columns_untouched ... ok
test a_crash_window_leaves_uncertain_persisted_across_a_reopened_store ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.83s
```

**机器证据 B — 显式 `--list` 枚举，同样 8 条、两个名字零出现**：

```
$ cargo test -p app --test session_resume_e2e --all-features -- --list
（退出码 0）
a_crash_window_leaves_uncertain_persisted_across_a_reopened_store: test
a_persisted_cwd_whose_canonical_form_differs_is_refused_before_any_backend_call: test
a_repeated_resume_request_id_replays_the_first_result_once: test
a_successful_resume_returns_the_session_reference_and_leaves_the_columns_untouched: test
an_unauthorized_resume_is_rejected_before_any_local_read: test
an_unsupported_agent_fails_the_resume_terminal_without_creating_a_session: test
any_resume_payload_field_is_rejected_before_side_effects: test
workspace_revalidation_and_null_recovery_data_take_distinct_paths: test

$ … | grep -cE 'a_persisted_directory_replaced_by_a_symlink|an_inaccessible_persisted_directory'
0        # grep 退出码 1 = 零命中
```

`--list` 输出**同时**证明这两条既未编译进本机目标（不存在条目），也未被 `#[ignore]` 掩盖（`--list` 会列出 ignored 用例，本目标 `0 ignored`）。**「未执行」已由机器证据确认。** 日志：`reports/merge-u1-candidate-app-e2e-list.log`。

---

## 6. 候选级 Project Verify

环境：Node `v24.19.0`、npm `12.0.2`（`engines` 要求 `>=22.12.0` ✅）、cargo `1.98.1 (797e8a9bc 2026-08-05)`、rustc `1.98.1 (48a229cea 2026-09-01)`，与 `rust-toolchain.toml` 的 `channel = "1.98.1"` 逐位一致。`CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-du1`（每条 cargo 命令显式导出）。

| Check ID | 命令 | 目录 | 退出码 | 结果 | 日志 |
| --- | --- | --- | --- | --- | --- |
| **PV1-STAGE1-FMT** | `cargo fmt --all -- --check` | `D:/Project/acp-remote-wt/session-resume-du1` | **0** | **PASS**（无输出） | `reports/merge-u1-candidate-PV1-stage1-fmt.log` |
| **PV1-STAGE1-CLIPPY** | `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | 同上 | **0** | **PASS**，`^warning` / `^error` 诊断计数 = **0** | `reports/merge-u1-candidate-PV1-stage1-clippy.log` |
| **PV1-STAGE2-TEST** | `cargo test --locked --workspace --all-features` | 同上 | **0** | **PASS**，`1074 passed / 0 failed / 2 ignored` | `reports/merge-u1-candidate-PV1-stage2-workspace-test.log` |
| **PV2** | `npm run check` | 同上 | **0** | **PASS**，10 道门禁全绿 | `reports/merge-u1-candidate-PV2.log` |

### 6.1 PV1 阶段 2 用例总数核对

| 口径 | 数值 | 来源 |
| --- | --- | --- |
| 合并前（`95051f9`）workspace 全量 | **1041 passed / 0 failed / 2 ignored** | 本轮**重新聚合**既有日志 `reports/merge-u1-integrate-wp6-PV1-stage2-workspace-test.log`（对全部 `^test result` 行 `awk` 求和） |
| TP2 五个新测试目标本机可执行用例 | **33**（acp-protocol `session_resume` 7 + storage-sqlite `resume_columns` 6 + agent-host `resume` 7 + node-link-protocol `session_resume_command` 5 + app `session_resume_e2e` 8） | 本候选 PV1-STAGE2-TEST 日志逐目标 `running N tests` 行 |
| 期望 | 1041 + 33 = **1074 passed**，failed/ignored 不变 | — |
| **本候选实测** | **1074 passed / 0 failed / 2 ignored** | 本轮 `merge-u1-candidate-PV1-stage2-workspace-test.log` |
| `error` / `failures:` / `test … FAILED` 命中 | **无** ✅ | 同上 |

**精确吻合**：passed 差值恰为 33、failed 仍为 0、**ignored 仍为 2 且是同名同因的两条**（`crash_child`、`regenerate_v1_fixture`）。TP2 未新增、未删除、未改写任何 `#[ignore]`——已由 `git grep -n '#\[ignore' 95051f9` 与 `… 2ed142d` 输出**完全相同且仅 2 处**独立确认（5.2 附注）。

**33 的分布 7/6/7/5/8 与 TP2 `tp2-test-design.md §7` / `tp2-tester.md` 声明的 33 条本机可执行用例逐项吻合。**

> **对前序报告的一处计数订正（如实记录）**：`reports/merge-u1-integrate-wp6.md`（`:21`、`:153`、`:310`）的三处叙述把 `95051f9` 的结果写为「1041 passed / 0 failed / **0 ignored**」。其**同一份日志** `merge-u1-integrate-wp6-PV1-stage2-workspace-test.log` 实际含 2 条 `... ignored` 行，求和为 `passed=1041 failed=0 ignored=2`，与本候选的 2 条 ignored **逐名相同**。即「`0 ignored`」是该报告叙述的漏记，**日志数据无误**。此订正**加强**而非削弱本轮结论：基线与候选在 ignored 面上完全一致。本 merger 未改动该报告（禁止改他人报告），仅在此登记。

### 6.2 PV2 十道门禁逐道输出

```
schema fixtures OK: 120 valid, 26 invalid (ajv Draft 2020-12), 39 event views bound
command catalog OK: 13 commands
error registry OK: 58 codes across 2 protocols
feature registry OK: 11 feature ids across 2 protocols
contract assets OK: 17 schemas, 159 fixture files, 12 transcript vectors re-encoded from input,
                    20 negative vectors rejected as declared, 2 SAS values recomputed
ACP compatibility matrix OK: 25 methods, 11 updates, 5 content blocks, 3 tool content types,
                    19 capabilities, 8 invariants, 10 test families (71 rows, schema validated by ajv)
doc links OK: 402 relative links, 6789 section refs across 403 markdown files
crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致（12 个已在矩阵登记）
contract drift OK: §7 的 36 条 DDL 与 crates\storage-sqlite\src\migrate.rs 逐条一致；
                    §5 的 15 个 trait / 96 个方法签名与 crates\core\src\ports.rs 一致
Installation: PASS …（agentic 0.3.0 / openspec 1.13.0，8 个角色已配置，dispatch.pool={"coding":3,"testing":2}）
Totals: 19 passed, 0 failed (19 items)
agentic 宿主入口检查完成：17 个文件
```

其中 `crate boundaries`（新增 `app → sqlx` 依赖方向）与 `contract drift`（§7 的 36 条 DDL / §5 的 96 个端口签名）在本候选上**重跑通过**，即 TP2 引入的 `app` 依赖登记未越界破坏 `AGENTS.md` §4 的依赖方向、未改动 DDL 与端口合同。

### 6.3 CI 专属判定：**本轮未执行，不等于通过**

| 判定 | 状态 | 原因 |
| --- | --- | --- |
| `cargo-deny`（依赖许可证 / 来源 / advisory） | **NOT_EXECUTED** | 需网络与额外二进制，按 `AGENTS.md` §8 与 `docs/adr/0008-ci-supply-chain-tooling.md` 只在 CI（`deps` / `advisories` job）运行 |
| `gitleaks`（密钥扫描） | **NOT_EXECUTED** | 同上（CI `secrets` job） |
| `commits` job（Conventional Commits 范围校验） | **NOT_EXECUTED**（本地等价物为 `npm run lint:commits`） | 本轮未单独执行；候选与 TP2 的提交标题均已按 `commitlint.config.mjs` 词表人工核对（`chore`/`repo` ∈ 词表、主题中文） |

**本机全绿不等于这三类判定通过。** 本报告不对其作任何通过结论。

---

## 7. 引用（不重跑 review）

按角色契约只引用 ID、路径与版本，**未重跑任何 review**：

| 证据 | 类型 | 目标版本 | 结果 | 报告 |
| --- | --- | --- | --- | --- |
| CR8 Round 1 | REVIEW（`test-case`） | `f44301e` | PASS | `reports/cr8-review.md` |
| CR8 Round 2 | REVIEW（`test-case`，round 2） | `3484541` | PASS | `reports/cr8-review-round2.md` |
| TP2 tester | CHECK / DELIVERY | `3484541` | 见报告 | `reports/tp2-tester.md` |
| TP2 test-design | DELIVERY | `3484541` | 见报告 | `reports/tp2-test-design.md` |
| U1 前序集成（WP1–WP6） | CHECK / DELIVERY | `95051f9` | 见各报告 | `reports/merge-u1-integrate-*.md` |
| WP1–WP6 coder/review | CHECK / REVIEW | 各自交付提交 | 见各报告 | `reports/cr1..cr7-*.md` 等 |

**Round 2 的适用范围说明**：CR8 Round 2 的 PASS 绑定 `3484541`，静态检视对象即本轮合入的第二父。Round 1 的 PASS 绑定 `f44301e`，**不被本报告用于替代 Round 2**。Round 2 列出的两项 diff 级待补证据（第 5.1、5.2、5.3 节）已由本轮机械补齐；CR8-F6 保持 PENDING（第 5.5 节）。

---

## 8. 证据失效 / 复用依据

| 原证据 | 判定 | 依据 |
| --- | --- | --- |
| WP1–WP6 在 `95051f9` 上的 PV 记录 | **REUSED**（仅用于「已验收上游」身份） | 本候选的祖先包含关系已由第 3 节在 `2ed142d` 上逐条重测（8/8 YES），上游内容未被改写；TP2 相对 `95051f9` 的改动经第 5.1 节证明为**零产品代码** |
| TP2 在 `f44301e` 上的 a3 执行日志 | **PENDING（`CR8-F7` 已被本轮机械补强，但原始日志头仍记录 `f44301e`）** | CR8-F7 指出的「日志头 `target_revision: f44301e` 与交接行 `3484541` 绑定不一致」属**报告侧标注**问题；本轮第 5.2 节以 `git diff` 直接证明 `3484541` 相对 `f44301e` **零断言改动**，即被测行为内容与 `f44301e` 一致。该标注问题仍留在 TP2 报告侧，**未由本 merger 改动**（禁止改他人报告） |
| CR8-F6（2 条 `#[cfg(unix)]`） | **PENDING，不记 PASS** | 第 5.5 节机器证据；待 Linux CI |
| `cargo-deny` / `gitleaks` | **PENDING**（NOT_EXECUTED） | 第 6.3 节 |

**无 INVALID 记录。**

---

## 9. 资源释放

| 资源 | 处置 |
| --- | --- |
| `CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-du1` | **保留，未删除**（本单元独占，缓存复用） |
| 其余 7 个工作区 | **未触碰**；`D:/Project/acp-remote-wt/session-resume-tp1` 全程**只读** |
| `npm install` / `npm ci` / `npm update` | **未执行** |
| `node_modules` | **未改动、未删除联接** |
| `git gc` / `git prune` / `git reflog expire` | **未执行**（保留悬空对象 `4146611` 供复核，5.3） |
| `plan.md` / `tasks.md` / `verification.md` | **未修改**（主 Agent 职责） |
| `crates/**` 任何文件（产品与测试） | **未修改**（本轮只创建 1 个 merge commit，其内容 = TP2 内容，逐位等于 `3484541^{tree}`） |
| `refs/heads/main` | **未移动**（仍 `81e350f`） |
| 暂存区 | 空（`git diff --cached --name-only` 无输出）；工作区干净（`git status --porcelain` 无输出） |

---

## 10. 明确标注：以下步骤**本轮未执行**（属后续步骤，由主 Agent 另行发起）

| 步骤 | 状态 | 归属 |
| --- | --- | --- |
| `openspec-agentic workflow check --change session-resume --stage premerge --planning-root <权威规划根> --json` | **NOT_EXECUTED** | 主 Agent 在 `verification.md` 固化 `agentic-premerge` 候选证据块（记 `delivery_unit`）后，于本单元复用的候选工作树运行；须取得 PASS |
| `agentic-premerge` 证据块 + **版本化 receipt** | **NOT_EXECUTED** | 同上；receipt 须在 premerge PASS **之后**才持久化 |
| **合入 `refs/heads/main`** | **NOT_EXECUTED** | 主 Agent；本轮 `main` 仍为 `81e350f`，与开工前一致 |
| push / PR | **NOT_EXECUTED** | 不在本流程授权内 |
| 最终验收 / 归档 | **NOT_EXECUTED** | validator + 主 Agent |
| tasks.md `6.3` 候选 Project Verify（U1 检查执行者） | **NOT_EXECUTED** | 见第 12 节「待澄清问题」 |

**候选 PASS 不等于已合入或最终验收 PASS。**

---

## 11. issues

**无 FAIL。无 BLOCKED。** 本轮未修改任何代码，未发现需要上报的产品/接口取舍。

**如实转达的 PENDING（不可当作已验证）：**

1. **CR8-F6 / 2 条 `#[cfg(unix)]`** — `a_persisted_directory_replaced_by_a_symlink_is_refused_before_any_backend_call`（符号链接改指）与 `an_inaccessible_persisted_directory_is_refused_before_any_backend_call`（`chmod 000` 打在父目录）。Windows 上**从未执行、也无编译证据**（交叉编译缺 `x86_64-linux-gnu-gcc`）。**PENDING，待 Linux CI `checks` job（`ubuntu-latest`）首次编译 + 执行。** 机器证据见第 5.5 节。失败则开新问题编号回 TP2。
2. **CR8-F7 报告侧标注** — TP2 的 a3 执行日志头记录 `target_revision: f44301e`，而交接行绑定 `3484541`。第 5.2 节已机械证明二者零断言差异，行为风险为零；但**标注本身仍在 TP2 报告侧，本轮未改动**。
3. **`cargo-deny` / `gitleaks` / `commits` job** — NOT_EXECUTED，见第 6.3 节。
4. **`4146611` 为悬空对象** — 尚未 gc，但**随时可能被 gc 回收**；如需长期留证建议主 Agent先决定是否留档。

---

## 12. 待澄清问题（交主 Agent）

1. **`tasks.md` `6.3` 与本轮职责重叠**：`6.3` 是「U1 **检查执行者**按 PV1/PV2 完成候选 Project Verify」，而 merger 角色契约第 2 步要求 merger 在候选上「执行计划内 Project Verify」，本轮据此执行了 PV1 阶段 2 + PV2（task `6.2`）。请主 Agent 明确：`6.3` 是复用本轮 PV 证据（记 `REUSED`）还是仍需检查执行者独立复跑一次。这不影响本轮 PASS。
2. **TP2 提交数与交底不一致**：实测 `95051f9..3484541` 为 **5** 个提交（含 `f04a989`），交底写 4 个。tip 一致、合入范围一致，仅措辞需校正（第 3.1 节）。
3. **候选的独立 reviewer 检视尚未发起**：`6.4`（U1 独立 reviewer 只读检视固定候选的新增交互和冲突解决）可与 `6.3` 并行。本候选的新增交互面为：TP2 引入的 5 个新测试目标、`crates/app` 的 `sqlx` 依赖登记、以及 `crates/app/tests/support/owner.rs` / `crates/storage-sqlite/tests/resume_columns.rs` 两个共享落点。**因本轮零冲突，无冲突解决差异需检视。** 请主 Agent 按需派发。

---

## 13. 交接摘要

- **源（第二父）**：`3484541`；**base（第一父）**：`95051f9`；**候选**：`2ed142d`
- **冲突解决差异**：**无**（零冲突）
- **候选固定提交**：`2ed142dedec2facf8f6e174d1aa165f549cbc47e`（`integration/session-resume-du1`）
- **候选内容等价性**：候选 tree `ff3a9cf…` ≡ `3484541^{tree}`，合入未引入任何额外内容改动
- **PV1 阶段 2**：**全绿**，`1074 passed / 0 failed / 2 ignored`（基线 1041 + 33 精确吻合，failed/ignored 与基线一致）；fmt、clippy（0 诊断）退出码均 0
- **PV2**：**全绿**，10 道门禁退出码 0
- **第 5 节四项机械核对**：**全部证实**（零越界 / 断言未改动 / `4146611` 归属确认且为悬空 / 清单恰好两行、无新包无版本无 feature）
- **premerge 门禁、`agentic-premerge` receipt、合入 `main`、push、最终验收、归档**：**均未执行**

---

## 14. handoff_index

```yaml
handoff_index:
  - task_id: "6.2"
    work_package: DELIVERY
    role: merger
    phase: integrate
    round: NOT_APPLICABLE
    stage: candidate
    target_revision: "2ed142dedec2facf8f6e174d1aa165f549cbc47e"
    evidence_type: DELIVERY
    evidence_id: INTEGRATE-TP2
    report_path: "openspec/changes/session-resume/reports/merge-u1-candidate.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "git merge --no-ff agentic/session-resume-tp1 退出 0，ort 策略，零冲突；父提交对 (95051f9, 3484541)；候选 tree ff3a9cf 与 3484541^{tree} 逐位相同，故合入未引入额外内容改动。合并前 7 条祖先实测（81e350f/248d9b9/32f71f5/4f7a2355/4a3882d/1376e1b/a7bc596）全部 ANCESTOR_OK，合并后 8 条（含 3484541）全部 YES。"
    source_evidence: NOT_APPLICABLE

  - task_id: "6.2"
    work_package: DELIVERY
    role: merger
    phase: candidate
    round: NOT_APPLICABLE
    stage: candidate
    target_revision: "2ed142dedec2facf8f6e174d1aa165f549cbc47e"
    evidence_type: DELIVERY
    evidence_id: CANDIDATE
    report_path: "openspec/changes/session-resume/reports/merge-u1-candidate.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "候选固定在 integration/session-resume-du1 @ 2ed142dedec2facf8f6e174d1aa165f549cbc47e，含 WP1–WP6、TP1、TP2 全部 8 个工作包；工作区干净、无暂存项；main 仍为 81e350f（未合入）。候选级 PV1 阶段 2 与 PV2 见下三行。"
    source_evidence: NOT_APPLICABLE

  - task_id: "6.2"
    work_package: DELIVERY
    role: merger
    phase: candidate
    round: NOT_APPLICABLE
    stage: candidate
    target_revision: "2ed142dedec2facf8f6e174d1aa165f549cbc47e"
    evidence_type: CHECK
    evidence_id: PV1-STAGE1-FMT
    report_path: "openspec/changes/session-resume/reports/merge-u1-candidate-PV1-stage1-fmt.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "命令 cargo fmt --all -- --check；目录 D:/Project/acp-remote-wt/session-resume-du1；rustfmt 1.98.1（rust-toolchain.toml channel=1.98.1）；CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-du1；退出码 0，无输出。PV1 的格式子项。"
    source_evidence: NOT_APPLICABLE

  - task_id: "6.2"
    work_package: DELIVERY
    role: merger
    phase: candidate
    round: NOT_APPLICABLE
    stage: candidate
    target_revision: "2ed142dedec2facf8f6e174d1aa165f549cbc47e"
    evidence_type: CHECK
    evidence_id: PV1-STAGE1-CLIPPY
    report_path: "openspec/changes/session-resume/reports/merge-u1-candidate-PV1-stage1-clippy.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "命令 cargo clippy --locked --workspace --all-targets --all-features -- -D warnings；目录同上；cargo 1.98.1 / rustc 1.98.1；同一 CARGO_TARGET_DIR；退出码 0，^warning 与 ^error 诊断计数 = 0。--all-targets 覆盖 TP2 新增的 5 个测试目标（日志含 Checking app v0.0.0）。PV1 的 lint 子项。"
    source_evidence: NOT_APPLICABLE

  - task_id: "6.2"
    work_package: DELIVERY
    role: merger
    phase: candidate
    round: NOT_APPLICABLE
    stage: candidate
    target_revision: "2ed142dedec2facf8f6e174d1aa165f549cbc47e"
    evidence_type: CHECK
    evidence_id: PV1-STAGE2-TEST
    report_path: "openspec/changes/session-resume/reports/merge-u1-candidate-PV1-stage2-workspace-test.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "命令 cargo test --locked --workspace --all-features；目录同上；同一 CARGO_TARGET_DIR；退出码 0；1074 passed / 0 failed / 2 ignored，无 error/failures/FAILED 命中。基线取既有日志 merge-u1-integrate-wp6-PV1-stage2-workspace-test.log 重新聚合的 95051f9 结果 1041/0/2，加 TP2 新目标 7/6/7/5/8 共 33 条，得 1074/0/2 精确吻合，failed 与 ignored 均与基线一致。2 ignored 为 95051f9 已存在的 storage-sqlite 两条 #[ignore]（git grep 两侧输出完全相同），TP2 新增 #[ignore] 数 = 0。PV1 阶段 2（workspace 全量）。"
    source_evidence: NOT_APPLICABLE

  - task_id: "6.2"
    work_package: DELIVERY
    role: merger
    phase: candidate
    round: NOT_APPLICABLE
    stage: candidate
    target_revision: "2ed142dedec2facf8f6e174d1aa165f549cbc47e"
    evidence_type: CHECK
    evidence_id: PV2
    report_path: "openspec/changes/session-resume/reports/merge-u1-candidate-PV2.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "命令 npm run check；目录同上；Node v24.19.0 / npm 12.0.2（engines >=22.12.0）；退出码 0；10 道门禁全绿（schema fixtures 120/26、command catalog 13、error registry 58、feature registry 11、contract assets 17 schemas+159 fixtures、ACP matrix 71 rows、doc links 402/6789、crate boundaries 12 crate、contract drift 36 DDL+96 方法签名、agentic 19 passed 0 failed）。其中 crate boundaries 与 contract drift 在本候选上重跑通过。"
    source_evidence: NOT_APPLICABLE

  - task_id: "6.2"
    work_package: DELIVERY
    role: merger
    phase: candidate
    round: NOT_APPLICABLE
    stage: candidate
    target_revision: "2ed142dedec2facf8f6e174d1aa165f549cbc47e"
    evidence_type: DELIVERY
    evidence_id: MECH-NO-BOUNDARY-VIOLATION
    report_path: "openspec/changes/session-resume/reports/merge-u1-candidate.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "补做 CR8 Round 2『待补证据』第 1 项（reviewer 无 shell/git 工具无法自证）。命令 git diff --name-only 95051f9..3484541 实测 8 个文件；crates/*/src/** 计数 = 0；compatibility/ schemas/ fixtures/ 计数 = 0；crates/*/tests/** = 6；其余 2 = Cargo.lock 与 crates/app/Cargo.toml（已授权）。零越界。"
    source_evidence: NOT_APPLICABLE

  - task_id: "6.2"
    work_package: DELIVERY
    role: merger
    phase: candidate
    round: NOT_APPLICABLE
    stage: candidate
    target_revision: "2ed142dedec2facf8f6e174d1aa165f549cbc47e"
    evidence_type: DELIVERY
    evidence_id: MECH-TP2-R3-NO-ASSERTION-CHANGE
    report_path: "openspec/changes/session-resume/reports/merge-u1-candidate.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "补做 CR8 Round 2『待补证据』第 1 项的 diff 级部分，并机械闭环 CR8-F7。命令 git diff --stat f44301e..3484541 = 恰好 3 个文件且全在 crates/**/tests/**；去注释过滤管道输出为空（末端 grep 退出码 1）；对照原始 +/- 行共 50 行且 50/50 全部以 // /// //! 开头，确认零断言/零阈值/零用例增删。另 git grep '#[ignore' 在 95051f9 与 2ed142d 输出相同仅 2 处，闭环 CR8-F1 的 ignore 面。"
    source_evidence: NOT_APPLICABLE

  - task_id: "6.2"
    work_package: DELIVERY
    role: merger
    phase: candidate
    round: NOT_APPLICABLE
    stage: candidate
    target_revision: "2ed142dedec2facf8f6e174d1aa165f549cbc47e"
    evidence_type: DELIVERY
    evidence_id: MECH-4146611-ATTRIBUTION
    report_path: "openspec/changes/session-resume/reports/merge-u1-candidate.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "补做 CR8-F2 待补项。git cat-file -t 4146611 = commit；4146611^{tree} 与 f44301e^{tree} 同为 97bbf785cbafb1c2df22878be9b92d263bcfd7ab；git diff --name-only 4146611 f44301e 为空；git log --all --oneline | grep 4146611 无命中（grep 退出码 1）故为悬空提交。两者同父 a02e2fd、同作者 lindongfang、同 author-date 17:50:36、同主题、仅 committer-date 晚 59 秒，确证 amend 前后孪生。说法『amend 前 SHA、tree 相同、权威值为 f44301e』三点全部成立。本轮未执行任何 gc。"
    source_evidence: NOT_APPLICABLE

  - task_id: "6.2"
    work_package: DELIVERY
    role: merger
    phase: candidate
    round: NOT_APPLICABLE
    stage: candidate
    target_revision: "2ed142dedec2facf8f6e174d1aa165f549cbc47e"
    evidence_type: DELIVERY
    evidence_id: MECH-MANIFEST-TWO-LINES
    report_path: "openspec/changes/session-resume/reports/merge-u1-candidate.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "命令 git diff 95051f9..3484541 -- Cargo.lock crates/app/Cargo.toml 全文 diff 已贴入报告；numstat 为 1 0 / 1 0，恰好两行且无删除。[[package]] 计数两侧同为 333（无新增条目）；sqlx 的 [[package]] 块在 baseline 已存在且 version 0.8.6 / source / checksum 逐字未变（无版本升级）；新增行 sqlx = { workspace = true } 无 features 键，工作区根 Cargo.toml 未出现在 diff 中（无 feature 面扩张）。"
    source_evidence: NOT_APPLICABLE

  - task_id: "2.8"
    work_package: TP2
    role: merger
    phase: candidate
    round: NOT_APPLICABLE
    stage: candidate
    target_revision: "2ed142dedec2facf8f6e174d1aa165f549cbc47e"
    evidence_type: CHECK
    evidence_id: CR8-F6-UNIX-VARIANTS
    report_path: "openspec/changes/session-resume/reports/merge-u1-candidate-app-e2e-list.log"
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "crates/app/tests/session_resume_e2e.rs 的 2 条 #[cfg(unix)] 用例（:1408 符号链接改指、:1514 chmod 000 打在父目录）在 Windows 上从未执行、也无编译证据（交叉编译缺 x86_64-linux-gnu-gcc）。机器证据：workspace 全量跑的 app e2e 目标只有 8 条且不含这两条；cargo test -p app --test session_resume_e2e --all-features -- --list 退出码 0、枚举同样 8 条、grep 两个名字命中数 0（--list 会列出 ignored 用例而本目标 0 ignored，故排除被 ignore 掩盖）。待 Linux CI checks job（ubuntu-latest）首次编译+执行；失败则开新问题编号回 TP2，不改判本轮结论。"
    source_evidence: NOT_APPLICABLE

  - task_id: "6.2"
    work_package: NOT_APPLICABLE
    role: merger
    phase: candidate
    round: NOT_APPLICABLE
    stage: candidate
    target_revision: "2ed142dedec2facf8f6e174d1aa165f549cbc47e"
    evidence_type: CHECK
    evidence_id: CI-ONLY-JUDGMENTS
    report_path: "openspec/changes/session-resume/reports/merge-u1-candidate.md"
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "cargo-deny（deps/advisories job）与 gitleaks（secrets job）需网络或额外二进制，按 AGENTS.md §8 与 docs/adr/0008-ci-supply-chain-tooling.md 只在 CI 运行；commits job 的本地等价物 npm run lint:commits 本轮未单独执行（提交标题已按 commitlint.config.mjs 词表人工核对）。本机全绿不等于这三类判定通过，本报告不对其作任何通过结论。待 CI。"
    source_evidence: NOT_APPLICABLE
```

### 关于 `PENDING` 行的 `result: BLOCKED`

第 5.5 节（CR8-F6 / 2 条 `#[cfg(unix)]`）与第 6.3 节（CI 专属判定）两行按 `roles/_shared/role-report.md` 记 `evidence_status: PENDING` + `result: BLOCKED`。这两项**不属于本轮（integrate + candidate + 候选级 PV1/PV2 + 机械核对）的适用判据**，故**不改变本轮 `result: PASS`**；它们是随候选移交主 Agent 的待补门禁。CR8-F8 指出的 `result: NOT_EXECUTED` 取值不合规问题在 TP2 报告侧，本轮按契约在 `PENDING` 行写 `BLOCKED`。
