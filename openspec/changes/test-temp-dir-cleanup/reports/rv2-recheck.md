# RV2 独立复核报告（recheck）

```yaml
task_id: "3.2"
role: reviewer
phase: review
agent_context: "fresh 只读复核子 Agent（未参与实现、未参与 RV1，不继承其对话）；本会话产物目录 ID = a2dd5963-a2c2-4842-90db-c37b4e579c12。运行环境未回传实际 Agent ID，本人无法自证宿主未注入其他上下文。"
target_revision: "6f37979d9d6420cb9899257949e1f72f7146003e"
scope: "RV2 复核（不扩展）：① RV1-F1/F2 是否已解决（逐项读码 + 独立 grep 复算）；② 6f37979 是否夹带其它改动（在无 git 范围访问的沙箱内可核验的范围）；③ 复核中发现的新问题。本次 PASS 只覆盖该复核范围，不代表本变更的 PV1/PV2、Project Verify 或最终验收结论。"
changes: "无（本角色只读；唯一写入 = 本报告）"
checks:
  - "读码：`crates/identity-keystore/src/store.rs` 的守卫模块（482–541）、工厂（495–503）、两个调用点（566–569、621–624）"
  - "独立重跑 `temp_dir()` 检索（`crates/**/*.rs`）＝ 24 行，与 `reports/inventory.md` §2 的 24 条逐条比对"
  - "读码核实行号：`crates/core/src/use_cases.rs` 1128–1150（`impl Drop` 1128–1141、工厂 1143–1150）"
  - "交叉核对实现方交付副本（子 Agent 产物目录 3124ebbd 的 `inventory.md`/`wp1-handoff.md`）与仓库内当前副本"
  - "`watchdog_diff`（工作树 vs reviewer-launch HEAD）：仅 `verification.md` +13/−2"
  - "读 RV1 报告全文（任务书给的仓库路径不存在，实际读取其子 Agent 产物副本，见 RV2-F2）"
issues:
  - "RV1-F1（MINOR）：**已解决**（`#[must_use]` 已在工厂上方，两个调用点均绑定返回值，不新增警告）"
  - "RV1-F2（MINOR）：**已解决**（`inventory.md` §2 的 `1138` 已改为 `1146`，独立 grep 复算为真）"
  - "RV2-F1（P2，仅报告）：`inventory.md` §2 同一行的 `identity-keystore/src/store.rs:498` 因 F1 修复新增 2 行而陈旧（实际 500）"
  - "RV2-F2（P2，仅报告）：`reports/rv1-wp1.md` 不在仓库内，而 `tasks.md:21`（3.2 完成条件）与 `verification.md:76` 引用该路径"
  - "RV2-F3（P2，仅报告）：`wp1-handoff.md:109` 的 D1 表行未列 `#[must_use]`，修复后与代码不再一致"
result: PASS
evidence_paths:
  - openspec/changes/test-temp-dir-cleanup/reports/rv2-recheck.md
  - openspec/changes/test-temp-dir-cleanup/reports/inventory.md
  - openspec/changes/test-temp-dir-cleanup/reports/wp1-handoff.md
  - crates/identity-keystore/src/store.rs
  - crates/core/src/use_cases.rs
  - openspec/changes/test-temp-dir-cleanup/verification.md
handoff_index:
  - task_id: "3.2"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: "6f37979d9d6420cb9899257949e1f72f7146003e"
    evidence_type: REVIEW
    evidence_id: RV2
    report_path: openspec/changes/test-temp-dir-cleanup/reports/rv2-recheck.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "本轮在 reviewer-launch HEAD=6f37979（`watchdog_diff` 证明工作树对 HEAD 的唯一差异是未提交的 `verification.md` +13/−2，`store.rs` 与 `inventory.md` 相对 HEAD 干净）上读码 + 独立 grep 复算 RV1-F1/F2 的两处修复，并与实现方交付副本交叉核对。提交范围的字面 diff（`git show 6f37979`）在本沙箱不可读，见「证据限制」。"
    source_evidence: NOT_APPLICABLE
resource_cleanup: "未创建/删除/修改仓库内任何文件（本报告由 runtime 持久化）；未运行任何构建、测试或 git 写操作；未占用临时目录、进程、端口或数据库。"
```

# RV2 独立复核报告（branch / work-package，recheck）

## Review Context

| 项 | 值 |
| --- | --- |
| Review ID | RV2（recheck 轮） |
| Review Type | branch（recheck） |
| Review Stage | 工作包交付前 branch validation（权威 `tasks.md:21` = 3.2 的复核） |
| Work Package | WP1（变更 `test-temp-dir-cleanup`，DU1 唯一工作包） |
| Repository | `D:\Project\acp-remote` |
| Base Revision | `6c5e53b58ea170f015ede1b8c0622a8e4632bf3d`（RV1 的目标版本） |
| Target Revision | `6f37979d9d6420cb9899257949e1f72f7146003e`（RV1-F1/F2 修复提交，提交主题 `test(identity): 补上临时目录守卫的 must_use 标注 [RV1-F1/F2]`） |
| Reviewer-launch HEAD | `6f37979…`（`watchdog_diff`：工作树对 HEAD 唯一差异 = `verification.md` +13/−2，未提交） |
| 读取的输入 | RV1 报告全文、`reports/inventory.md`、`reports/wp1-handoff.md`、`design.md`（D1–D5）、`tasks.md`（2.x/3.2）、`verification.md`、`crates/identity-keystore/src/store.rs`、`crates/core/src/use_cases.rs` |
| 角色边界 | 只做 RV1-F1/F2 的复核与新问题的独立发现；不重跑 RV1 的①–④结论，不替主 Agent 判全局 PASS |

### 复核方法（逐项）

1. **RV1-F1**：读 `store.rs` 守卫模块与工厂全文；确认工厂上方有 `#[must_use]`；再核对该 crate 内**全部**调用点是否都绑定返回值（`#[must_use]` 唯一可能的副作用是新增 `unused_must_use` 警告 → 会让 `clippy -D warnings` 失败）；最后确认其余 6 个守卫工厂仍各自带 `#[must_use]`。
2. **RV1-F2**：读 `inventory.md` §2 当前文本；独立重跑 `temp_dir()` 检索得到 24 行，与 §2 的 24 条逐条比对；再读 `use_cases.rs` 确认 `1146` 与 `1138` 各自的真实内容。
3. **夹带改动**：RV1-F1/F2 的实现方交付副本（子 Agent 产物目录 `3124ebbd-…`）与仓库内当前副本逐行对照（§1–§5），看差异是否只落在 RV1 指名的两处；并用 `watchdog_diff` 确认工作树相对 HEAD 的增量。
4. **新问题**：由第 2/3 步的对照结果直接产生（见 Findings）。

## Findings

| ID | Severity | Location | 触发/证据 | 影响 | 最小修复 | 复核状态 |
| --- | --- | --- | --- | --- | --- | --- |
| RV1-F1 | MINOR（P2，非阻断） | `crates/identity-keystore/src/store.rs:496-499`（守卫模块 `#[cfg(test)] mod temp_dirs` 起于 483-484） | **已解决**。① 工厂上方现为：`497 /// \`#[must_use]\`：语句级裸调用会在语句结束即删除目录（design D1）。` → `498 #[must_use]` → `499 pub(super) fn new(name: &str) -> Self {`（`std::env::temp_dir()` 在 `500`）。RV1 记录修复前 `pub(super) fn new` 在 497、该调用在 498，即本次恰好插入 2 行，与修复范围吻合。② 该 crate 内 `TempDir::new` 只有两个调用点——`566`（`private_directory_and_entry_file_modes_are_restrictive`）与 `621`（`write_atomic_replaces_content_without_leaving_temporaries_on_failure`）——**都是** `let root = TempDir::new(&format!(…));`，无语句级裸调用，因此 `#[must_use]` 不可能引入新的 `unused_must_use` 警告（`cargo clippy --all-targets -D warnings` 不受影响）。③ 全 crate `#\[must_use\]` 检索＝1 处（`store.rs:498`），且该属性位于 `#[cfg(test)]` 模块内，非测试构建不受影响。④ 其余 6 个守卫工厂仍各自带 `#[must_use]`：`storage-sqlite/tests/support/mod.rs:91`、`core/src/use_cases.rs:1144`、`server/src/local_admin/test_support.rs:1612`（`TempDir::new`）与 `:1657`（`TempFile::new`）、`app/src/cli/input.rs:178`、`agent-host/tests/support/mod.rs:36`。⑤ design D1 的「创建函数标 `#[must_use]`」现已在 7 个守卫上一致成立；`inventory.md` §2 断言这些构造函数「都在 `#[must_use]` 的创建函数里」也已重新为真。 | 无（原本就无实际故障，属编译期防线缺失） | 已由 `6f37979` 落地 | **已解决** |
| RV1-F2 | MINOR（P2，非阻断） | `reports/inventory.md:54`（§2 第 1 类首项） | **已解决**。当前该行写 `core/src/use_cases.rs:1146`；我独立读码确认 `use_cases.rs:1146` 正是 `let path = std::env::temp_dir().join(name);`（工厂 `1143-1150`，`#[must_use]` 在 `1144`），而 RV1 指出的错误值 `1138` 确实落在 `impl Drop for TempDir` 内（`1128-1141`，重试循环 `1133-1140`）——与 RV1 的描述一致。交叉证据：实现方交付副本（子 Agent 产物 `3124ebbd-…/inventory.md:54`）仍是 `1138`，当前副本为 `1146`，且 §1 全文、§3–§5 全文两份副本逐行一致 → 本次 `inventory.md` 的编辑就是这一个 token。 | 无（纯证据准确性） | 已由 `6f37979` 落地（顺带确认 §2 的「24 行」仍为真：我独立 grep 得到 24 行） | **已解决** |
| RV2-F1 | P2（仅报告，非阻断；同一行的另一处行号已被本轮修复改成陈旧） | `reports/inventory.md:54` 同一行：`identity-keystore/src/store.rs:498` | 同一行现在写 `identity-keystore/src/store.rs:498`，但**实际** `std::env::temp_dir()` 在 `store.rs:500`（读码；我的独立 grep 输出也是 `identity-keystore/src/store.rs:500`）。成因可由两份副本对照证明：交付副本该行 `1138` + `498`；当前行 `1146` + `498`；而 RV1 记录的修复前位置是 `fn new`@497、调用@498，修复在 `fn new` 上方插入了 2 行（文档行 497 + 属性 498）→ 调用从 498 变成 500，即 F1 修复使这一行的行号失效，而 F2 的订正只改了 `1138→1146`。§2 其余 23 条行号与我的独立 grep **逐条精确一致**（24 = 6+7+11 条）。 | §2 的实质结论（24 行、三类无未分类点）不受影响；仅「按行号找该 grep 命中」时需多算 2 行 | 把 `identity-keystore/src/store.rs:498` 改为 `:500`（或按 RV1-F2 的建议去掉行号只留函数名，避免随改动漂移） | **新发现，未修复** |
| RV2-F2 | P2（仅报告；不阻断 F1/F2 结论，但影响 3.2 的完成条件与证据可读性） | `openspec/changes/test-temp-dir-cleanup/reports/`（缺 `rv1-wp1.md`） | 该目录实际只有 5 项：`final-verify.log`、`inventory.md`、`temp-count-final.log`、`wp1-handoff.md`、`wp1-local-checks.log`（`.gitignore` 只忽略 `*.log`，`.md` 报告不受忽略 → 若存在必然可见）。RV1 报告现只存在于子 Agent 产物路径 `…/subagent-artifacts/outputs/281b1a00-…/openspec/changes/test-temp-dir-cleanup/reports/rv1-wp1.md`（我据该副本完成本轮复核）。而被引用处有三处：`tasks.md:21`（3.2 完成条件「报告 `reports/rv1-wp1.md`」）、`verification.md:76`（Review Findings 的 RV1 登记）、`plan.md:48`/`:126`。 | 仓库内无法按路径读到 RV1 报告；3.2 的完成条件与后续证据核对（5.2 / 8.1）会指向不存在的文件 | 由主 Agent 把 RV1（及本轮 RV2）报告副本复制进 `openspec/changes/test-temp-dir-cleanup/reports/`（`rv1-wp1.md` / `rv2-recheck.md`）；若本仓库的既定流程本就是「子 Agent 产物由主 Agent 转存」，则按该流程执行即可 | **新发现，未修复**（非 `6f37979` 引入） |
| RV2-F3 | P2（仅报告，非阻断） | `reports/wp1-handoff.md:109`（D1 守卫形态表 identity-keystore 行） | 该行仍写形态 = `` `Deref<Target = Path>` + `AsRef<Path>` ``，同表其它 5 行都显式列了 `#[must_use]`；代码现状已在 `store.rs:498` 补上 `#[must_use]`。两份副本（交付副本与当前副本）该行文字相同 → 属修复未同步的报告残留（同一行正是 RV1-F1 当初的证据）。同一文件表下的要点「创建函数一律 `#[must_use]`」在修复后重新为真，因此只有这一行表格待补。 | 报告的形态描述比代码少一个属性；不影响任何行为或门禁 | 在该行形态列补 `+ #[must_use]` | **新发现，未修复** |

### 未发现问题（本轮复核范围内已核对为正确）

1. **修复没有引入代码层回归**：`#[must_use]` 位于 `#[cfg(test)]` 模块内、两个调用点都 `let` 绑定 → 不会新增 `unused_must_use`/clippy 失败；文档行只加注释，`cargo fmt --check` 不受影响（rustfmt 不重排注释）。
2. **本轮工作树的代码/报告内容 = 目标版本内容**：`watchdog_diff` 显示工作树对 HEAD 的唯一差异是 `openspec/changes/test-temp-dir-cleanup/verification.md`（+13/−2，PV 记录与 Review Findings 登记）；`crates/identity-keystore/src/store.rs` 与 `reports/inventory.md` 相对 HEAD 干净，故我读到的就是 `6f37979` 的内容。
3. **§2 的 24 行集合仍成立**：我独立重跑 `temp_dir()`（`crates/**/*.rs`）得到 24 行，隶属关系与 §2 的三类划分一致（除 RV2-F1 的单条行号漂移）。

## Assessment

**本轮复核结论：PASS（仅对应 Target Revision `6f37979` 的本次复核范围）。**

- **RV1-F1：已解决。** 工厂上方已有 `#[must_use]`（`store.rs:498`），两个调用点均绑定返回值，其余 6 个守卫工厂仍带该属性，design D1 已在 7 个守卫上一致成立。
- **RV1-F2：已解决。** `inventory.md:54` 的 `1138` 已改为 `1146`；我独立复算 `use_cases.rs:1146` 命中、`1138` 落在 `impl Drop` 内，与 RV1 描述吻合。
- **夹带改动**：在可核验范围内未发现夹带（`inventory.md` 相对交付副本的唯一差异就是该 token；`store.rs` 侧的两处调用点、其余 6 个守卫工厂与 `unused_must_use` 风险均已逐点核对）。**但提交范围的字面 diff 无法在本沙箱读取**（见下），因此「`6f37979` 只含这两个文件」仍未由我独立证明。
- **新发现 3 项**，均为 P2（报告/证据精度，非代码缺陷），不阻断本次复核结论。

### 证据限制（必须随结论阅读）

1. **无 git 范围访问**：`watchdog_diff` 只覆盖「工作树相对 reviewer-launch HEAD」的增量（本轮 = `verification.md` +13/−2），**不含已提交范围**。因此我**没有**读到 `git show 6f37979` / `git diff 6c5e53b..6f37979` 的字面 hunk 与文件清单，「无夹带」这一条由「目标态读码 + 两份副本对照 + 其余 6 个守卫逐点核对」间接支撑，不等于独立验证。
2. **未执行任何构建/测试**（隔离要求）：主 Agent 声称的「修复后 `cargo test -p identity-keystore` 10+12 passed、clippy、fmt 全绿」我未复跑，只能以静态分析说明其可信（`#[must_use]` 不可能新增警告）。
3. `verification.md` 的 PV1/PV2 记录（`fe7b0e9` / 工作树 `6c5e53b`）我按输入读取，未复核；其「证据时效说明」自认修复晚于 PV1/PV2、只重跑受影响 crate，这一点与我的观察一致（`inventory.md` 的一处行号确实在修复后失效，说明修复没有全量重放证据）。

### 需主 Agent/监督者执行或补的命令（我不执行）

| 目的 | 命令（期望结果） |
| --- | --- |
| 闭合「无夹带」残余风险 | `git show --stat 6f37979`（期望：仅 `crates/identity-keystore/src/store.rs` + `openspec/changes/test-temp-dir-cleanup/reports/inventory.md`）与 `git diff 6c5e53b..6f37979` |
| 复核修复后测试仍绿 | `cargo test -p identity-keystore`、`cargo fmt --all -- --check`、`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` |
| 闭合 RV2-F2 | `git status --porcelain openspec/changes/test-temp-dir-cleanup/reports/`（确认 `rv1-wp1.md`/`rv2-recheck.md` 已落到变更目录） |

### Merge verdict

**OK with notes**（对 `6f37979` 而言）：RV1-F1/F2 均已解决、无阻断项；建议在 3.2 收尾时顺手订正 RV2-F1/RV2-F3 两处行号/形态残留，并把 RV1（与本轮 RV2）报告转存进 `reports/`（RV2-F2）。本次 PASS **不**代表 PV1/PV2、Project Verify、候选轮或最终验收通过。