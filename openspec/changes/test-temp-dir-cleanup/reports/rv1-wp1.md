## 交付物（写入 `openspec/changes/test-temp-dir-cleanup/reports/rv1-wp1.md`）

```yaml
task_id: "3.2"
role: reviewer
phase: review
agent_context: "fresh 只读检视子 Agent（未参与实现，不继承实现对话）；运行环境未回传实际 Agent ID。无法自证宿主未注入其他上下文。"
target_revision: "6c5e53b58ea170f015ede1b8c0622a8e4632bf3d"
scope: "WP1（DU1 唯一工作包）全部 diff 的独立检视：盘点无漏点、守卫满足 design D1–D4、未触碰产品代码与契约资产、未弱化断言；含 run_cli 目录改名依赖方、0700 口径、Drop 重试语义三项额外核查。"
changes: "无（本角色只读，不修改任何文件）"
checks:
  - "自跑盘点：`temp_dir()` 全仓 24 行逐行复核；另以 create_dir_all/create_dir/DirBuilder/acpr-* 字面量做「创建行为」补充扫描"
  - "逐个守卫读码：storage-sqlite / core / identity-keystore / server / app / agent-host 的 7 个守卫实现与全部写入面外引用"
  - "D2 危险写法扫描：101 个 temp_dir 调用点全部为 `let … = temp_dir(..)`；agent-host 两处「守卫当场丢弃」已修"
  - "改名前缀依赖方全仓 grep；0700 分支读码；Drop 重试语义推演"
  - "版本链核实（只读 .git/logs/HEAD reflog）：1359a13 → 6c5e53b → fe7b0e9，HEAD=fe7b0e9，工作树对 HEAD 干净"
issues:
  - "RV1-F1（MINOR）：identity-keystore 守卫工厂缺 #[must_use]，偏离 design D1，且与 inventory.md §2『都在 #[must_use] 的创建函数里』的陈述不符"
  - "RV1-F2（MINOR）：inventory.md §2 的 core/src/use_cases.rs:1138 行号陈旧（实际 1146）"
  - "RV1-F3/F4（SUGGESTION）：Drop 静默吞掉持久失败；本变更不引入机器防线（design 已自认风险）"
result: PASS
evidence_paths:
  - openspec/changes/test-temp-dir-cleanup/reports/rv1-wp1.md
  - openspec/changes/test-temp-dir-cleanup/reports/inventory.md
  - openspec/changes/test-temp-dir-cleanup/reports/wp1-handoff.md
  - openspec/changes/test-temp-dir-cleanup/reports/wp1-local-checks.log
resource_cleanup: "未创建/删除任何文件；未运行任何构建或测试；未占用临时目录或进程。"
handoff_index:
  - task_id: "3.2"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: "6c5e53b58ea170f015ede1b8c0622a8e4632bf3d"
    evidence_type: REVIEW
    evidence_id: RV1
    report_path: openspec/changes/test-temp-dir-cleanup/reports/rv1-wp1.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "本轮在 reviewer-launch HEAD=fe7b0e9（工作树干净、其父提交即目标 6c5e53b，提交主题为 openspec 簿记）上读取目标版本内容，独立重跑 ① 的 grep 核对表、逐个读码 ② 的全部守卫与调用点，并补充 ③/④ 的目标态核查。非提交范围的字面 diff 在本沙箱不可读（见 Review Context 限制）。"
    source_evidence: NOT_APPLICABLE
```

# RV1 独立检视报告（branch / work-package，WP1）

## Review Context

| 项 | 值 |
| --- | --- |
| Review ID | RV1 |
| Review Type | branch |
| Review Stage | 工作包交付前 branch validation（对应权威 tasks.md 3.2） |
| Work Package | WP1（变更 `test-temp-dir-cleanup`，DU1 唯一工作包） |
| Repository | `D:\Project\acp-remote` |
| Base Revision | `1359a1319648150e1d00a91dd8fe69e3870f1a08` |
| Target Revision | `6c5e53b58ea170f015ede1b8c0622a8e4632bf3d` |
| Reviewer-launch HEAD | `fe7b0e91d92eccf5aad5c8980545a9f817e8dfec`（工作树对 HEAD **零差异**；reflog 证明它是目标提交的**直接子提交**，主题为 `docs(repo): 登记 WP1 交接验收与任务勾选 [2.1-2.4]`，只动 `openspec/`） |
| 读取的规则/需求 | `proposal.md`、`design.md`（D1–D5）、`plan.md`（Code Review 节）、`tasks.md`（2.1–2.4/3.2）、`AGENTS.md`（§3/§4/§7/§9/§10）、`openspec/schemas/agentic/roles/{handoff,reviewer}.md` |
| 核对的自证证据 | `reports/inventory.md`、`reports/wp1-handoff.md`、`reports/wp1-local-checks.log`（只作**待核对的声称**，不作结论依据） |

### 实际检查范围

- **目标态全量读取**：14 个改动文件中的每一个都逐点读码（守卫定义 + 每个受影响函数/用例），并对其调用方、调用点做全仓 grep 交叉核对。
- **盘点复核（①）**：自行重跑 `temp_dir()` 全仓检索得到 **24 行**，与 `inventory.md` §2 的 24 行逐条比对（23 条行号精确一致，1 条陈旧，见 RV1-F2）；并独立按「创建行为」口径补扫 `create_dir_all`/`create_dir(`/`DirBuilder`/`"acpr-` 字面量，逐一确认落点归属。
- **守卫语义复核（②）**：7 个守卫（storage-sqlite `TempDir`、core `TempDir`/`temp_dir()`、identity-keystore `temp_dirs::TempDir`、server `test_support::{TempDir,TempFile}`、app `cli::input::TempDir`、agent-host `TempFile`）全部读码；D2 危险写法（`foo(temp_dir(..))`、`temp_dir(..).join(..)`、`.0`/元组解构丢弃守卫）逐点核对；命名唯一性逐文件核对；Drop 不 panic 与资源释放顺序核对。
- **产品代码/契约边界（③）**：`crates/**` 内 `TempDir|TempFile|TempRoot|temp_dirs` 的全量命中逐一确认处于 `tests/**`、`#[cfg(test)] mod tests` 或 `#![cfg(test)]` 文件内；无产品模块引用守卫。
- **断言完整性（④）**：读遍所有改动区域（storage `admin_audit` 14/14 调用点、`session_version_rule` 3 个用例、`compose` 受影响用例、`run_cli`、agent-host `catalog` 16 个调用点与 `supervision` 3 个用例、server `audit`/`params` 用例头），未发现被删除或放松的断言；另全仓扫描 `// assert`、`#[ignore]`、`assert!(true`、`unwrap_or(true)`/`|| true` 一类弱化模式（命中的 2 处均在**未改动**的产品代码：`core/src/broker.rs:4274`、`app/src/config.rs:359`）。
- **额外三项**：`run_cli` 输出目录改名的依赖方（全仓 grep）；`storage-sqlite` 的 `0700` Unix 创建口径（读码）；Drop 重试是否掩盖真实泄漏（语义推演）。

### 证据限制（必须随结论阅读）

1. **本沙箱无 git 范围访问**：`watchdog_diff` 只覆盖「工作树相对 reviewer-launch HEAD」的增量（本轮为空），**不含已提交范围**。因此 `1359a13..6c5e53b` 的**字面 hunk**（含 base 侧内容、被删行、改动文件清单的完整性）我**未能独立读取**，只能以「目标版本内容（= 工作树）+ 独立 grep/读码」代替。我没有把「读过 commit」写进任何结论。
2. **基线侧 grep 不可复现**：`inventory.md` §1 的「基线 28 行」表我无法复算（需要 base 版内容）；我只独立验证了**改动后**的 24 行与分类。
3. 未执行任何构建/测试（隔离要求）；`npm run check` / PV1 / PV2 与 Unix 交叉编译结论均未由我验证。

## Findings

| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation | Recheck |
| --- | --- | --- | --- | --- | --- | --- |
| RV1-F1 | MINOR | `crates/identity-keystore/src/store.rs:497`（守卫模块 `#[cfg(test)] mod temp_dirs`，483–484 行起） | 该文件 `#[must_use]` **零命中**（全文件 grep 无匹配）；其余 6 个守卫工厂均有 `#[must_use]`（`storage-sqlite/tests/support/mod.rs:91`、`core/src/use_cases.rs:1144`、`app/src/cli/input.rs:178`、`agent-host/tests/support/mod.rs:34`、`server/src/local_admin/test_support.rs` 的 `TempDir::new`/`TempFile::new`）。触发条件：将来有人写语句级裸调用 `TempDir::new("…");` → 目录建好即在语句末被删，且**编译期不报警**。`inventory.md` §2 还声称这些构造函数「都在 `#[must_use]` 的创建函数里」，与代码不符。 | design D1 的「`#[must_use]` 让误用在编译期报警」在 identity-keystore 一处分母缺失；当前两个调用点都是 `let root = TempDir::new(…)`，**无实际故障**。 | 在 `pub(super) fn new` 上方补 `#[must_use]`（一行），并同步 `inventory.md` §2 的措辞。 | 待实现方修复后由新 reviewer 复核；不影响本轮 PASS |
| RV1-F2 | MINOR | `reports/inventory.md` §2 第 1 类首项：`core/src/use_cases.rs:1138` | 实际该行是 `std::env::temp_dir().join(name)` 于 **1146**；1138 落在 `impl Drop` 内。其余 23 条行号与我的 grep 输出**逐条精确一致**。 | 纯证据准确性；§2 的实质结论（24 行、无未分类点）成立，②① 的判定不受影响。 | 把 `1138` 改为 `1146`，或去掉行号只留函数名（行号随改动漂移）。 | 报告级修正；不阻断 |
| RV1-F3 | SUGGESTION | 7 个守卫的 `Drop`（如 `crates/storage-sqlite/tests/support/mod.rs` Drop、`crates/app/src/cli/input.rs` Drop） | `remove_dir_all`/`remove_file` 失败重试 `10×50ms` 后**静默返回**；唯一探测器是 PV2 的 `acpr-*` 计数。推演：瞬时占用重试成功→无残留（不掩盖）；持续泄漏重试后失败→残留落在计数窗口内→被 PV2 抓到（也不掩盖）。 | 不掩盖泄漏，但**残留的原因**在单次运行输出里不可见，需靠名字前缀人工归因。design D3 已接受该取舍。 | 可选：最终失败时 `eprintln!` 一行路径+错误（不 panic），便于定位。非必需。 | 不适用 |
| RV1-F4 | SUGGESTION | 变更整体（非文件缺陷） | 本变更不加机器防线（proposal non_goals 明确排除；design Risks 已登记）；AGENTS.md §9 的「先加回归测试」在此由 PV2 计数窗口替代，且用例数未变（`wp1-local-checks.log` 各目标全绿、无新增/删除用例）。 | 后续新增测试再写裸 `temp_dir` 时无编译/CI 拦截。 | 记录为后续变更候选（`scripts/` 前后计数检查进 CI）。 | 不适用 |

**未发现问题：** ① 无漏点（除上述行号笔误外，分类正确）；② 守卫满足 D1–D4；③ 目标态内无产品代码/契约资产引用守卫；④ 未发现断言被删除或弱化。无 CRITICAL / MAJOR。

## 已核对正确项（Correct，逐条给证据）

1. **① 盘点无漏点**
   - 我自跑得到的 24 行 `temp_dir()` 与 `inventory.md` §2 的 24 行集合**完全对应**，全部落在「守卫内部实现 / 已有守卫构造 / 只拼路径·守卫调用点」三类中，无未分类点。
   - 补充扫描（`create_dir_all|create_dir(|DirBuilder`、`"acpr-`）：`app/tests/{daemon_lifecycle,cli_commands,audit_export}.rs` 的创建点全部落在 `TempRoot` 派生子路径（`daemon.root().join(…)`、`temp.endpoint_dir()`），`storage-sqlite/tests/migration.rs:750` 是 `#[ignore]` 夹具生成器写仓库内 `fixtures/`（非临时目录），其余命中是产品代码（`storage-sqlite/src/migrate.rs`、`server/src/transport/local/platform/unix.rs`）——**无未守卫的临时创建点**。
   - 复核 9 个「无泄漏」声称：`agent-host/tests/supervision.rs:36` 与 `agent-host/tests/support/mod.rs:507` 只取既存目录的路径文本当 cwd；`core/src/use_cases.rs:1442/1801` 的 `acpr-missing-*` 只作为「解析必须失败」的输入、从不创建；`app/src/config.rs:922` 只在 TOML 文本里出现（`app/src/config.rs` 无任何 `create_dir*`，grep 未命中）；`server/src/local_admin/test_support.rs:1868` 只拼 `rootPath` 文本；`server/src/transport/local/platform/windows.rs:193`/`server/tests/local_endpoint_windows.rs:26` 的 `data_dir` 仅被 `unix_endpoint_directory` 使用（`endpoint.rs:147-158`），该函数仅被 `platform/unix.rs:56` 与 `tests/local_endpoint_naming.rs` 调用 → Windows 上不建条目；`storage-sqlite/tests/admin_store.rs:468` 的 `absolute_path()` 11 个调用点全部只作记录字段/文本断言。
   - `inventory.md` §3 的三处「非字面命中」我逐一验证：`session_version_rule.rs` 三个用例末尾均有 `store.close().await;`（111/158/211 起的用例，读到实体行）；`app/src/compose.rs:1030-1048` 确实在 `drop(extra)` 后 `Arc::try_unwrap(store).expect(…).close().await`，两个断言（`StoreStillShared`、`message` 含 `shared`）原样保留。
2. **② 守卫满足 D1–D4**
   - D1/D3：7 个守卫的 `Drop` 均为「`Ok`→返回、`NotFound`→立即返回、其它错误→睡 50ms」，循环 10 次，**无 `unwrap`/`expect`/`panic`**；创建函数 `#[must_use]`（除 RV1-F1 一处）。
   - `Deref<Target = Path>` + `AsRef<Path>`（+ storage 额外 `AsRef<OsStr>`）语义正确；storage 的 `StorageConfig::new(&dir)`（`impl Into<PathBuf>`）与 `set_permissions(&dir, ..)`/`inspect_path_permissions(&dir)` 由 `TempDir: AsRef<OsStr>` 与 std 的 `impl<T: AsRef<U>> AsRef<U> for &T` 兜住，故 101 个调用点零改写的说法成立（我按 trait 形状逐条核对了 `permissions.rs:80/109/129` 的用法）。
   - D2：storage-sqlite 的 101 个调用点**全部**是 `let dir = temp_dir(..)`（我的逐行检索输出可复核）；无 `foo(temp_dir(..))`/`temp_dir(..).join(..)`；agent-host 两处「守卫当场丢弃」（原 `let (profile, _) = …`、`vec![… .0]`）已改为 `_env_guard`/`broken_env` 并复用同一路径做「进程不得启动」断言（`catalog.rs:224-229`、`251-280`），**断言等价且更强**（不再重复建路径）。
   - 资源释放顺序：`admin_audit.rs:113-118` 返回 `(TempDir, SqliteStore)`，14/14 调用点写成 `let (_dir, store) = open(..)`（唯一例外 `:696` 用 `let (dir, store)` 因为要继续用路径，守卫仍声明在前→逆序析构时最后释放）；`server/src/local_admin/test_support.rs` 的 `TestWorld::temporary_directory`（1851 起）登记式清理保持不变。
   - D4：命名唯一性逐一核对——storage `acpr-storage-{name}-{pid}` 在同一 binary 内的 name 字面量**无重复**（我逐文件列名核对）；core 用 uuid；keystore/params/audit 用 `{pid}-{sequence}`；app `run_cli` 用 `TempRoot` 计数器、`input.rs` 用 `{pid}-{ThreadId}`；agent-host `catalog` 的 tag 无重复，`supervision` 三个固定名各只出现一次（全仓 grep 确认无第二个使用者，且与 `catalog` 的 `acpr-agent-host-env-{pid}.txt` 不同名）。
3. **③ 产品代码与契约资产**
   - 目标态：`crates/**` 内所有 `TempDir/TempFile/TempRoot/temp_dirs` 命中都在测试面（`tests/**`、`#[cfg(test)] mod`、或 `server/src/local_admin/test_support.rs` 文件级 `#![cfg(test)]`，第 13 行）+ `mod.rs:33-34` 的 `#[cfg(test)] pub(crate) mod test_support;`；`audit.rs:199`/`params.rs:978` 只在 `mod tests` 内 import 守卫。
   - 仓库规则符合：未新建 crate、未引入共享 utils（AGENTS.md §4）、未新增依赖（守卫只用 std）。
4. **额外三项**
   - `run_cli` 目录改名：全仓 grep `wp4b-cli|wp4a-cli` 只命中本变更的 reports 与历史归档文档（都是指 `wp4b-cli.log` **日志文件名**，与临时目录前缀无关）；`app/tests/cli_commands.rs:63-68` 的目录断言用的是 `daemon.data_dir()`（`TempRoot` 派生），不依赖 `run_cli` 的输出目录名；PV2 只按 `acpr-*` 过滤 → 改名无依赖方。
   - `0700` 口径保留：`storage-sqlite/tests/support/mod.rs` 的 `temp_dir()` 仍保留 `#[cfg(unix)]` `DirBuilder::new().recursive(true).mode(0o700)` 分支与 `#[cfg(not(unix))] create_dir_all` 分支（98–107 行），且创建前仍先 `remove_dir_all` 同名旧目录。

## Assessment

**本轮检视结论：PASS（对应 Target Revision `6c5e53b`）。** 已完成约定范围的检视：① 盘点无漏点（含 3 处补充口径）、② 守卫满足 design D1–D4、③ 目标态内无产品代码/契约资产受守卫影响、④ 未发现断言被删除或弱化；**无未解决 CRITICAL/MAJOR**。存在 2 项 MINOR 与 2 项 SUGGESTION（RV1-F1 建议在本单元内顺手修掉那一个 `#[must_use]`；RV1-F2 是报告行号笔误），均不构成阻断。

按 reviewer.md 的判定顺序，我未选 BLOCKED：目标版本内容是可读且准确的（reflog 证明 HEAD 的直接子提交关系、工作树对 HEAD 零差异），我完成了代码层的实质检视；缺的只是**字面 hunk 产物**（见下）。

### 待补证据（逐项注明是否影响本轮判断）

| 待补项 | 门禁 | 是否影响本轮判断 |
| --- | --- | --- |
| `1359a13..6c5e53b` 的字面 diff / 改动文件清单完整性（我无法读取已提交范围；`watchdog_diff` 不含 commit） | 本轮 branch review 的资料完整性 | **不改变 PASS**，但留下一项残余风险：「除这 14 个文件外无其它文件被改动（尤其 `docs/`/`schemas/`/`fixtures/`/`compatibility/`/产品代码零改动）」是**实现方 `git diff --numstat` 的声称 + 目标态读码的间接支持**，非我独立验证。主 Agent 侧用 `git diff --stat 1359a13..6c5e53b` 即可一秒闭合（期望 `14 files changed, +414/−73`）。 |
| PV1 `npm run verify`（主 Agent 并行执行中） | 3.1 / 候选轮 | 不影响静态判断；但**不能**由本次 PASS 推论 PV1 已通过。 |
| PV2 正式计数（`acpr-*` 差值为 0，串行窗口） | 3.1 / 交付前 | 不影响静态判断。注意 `wp1-local-checks.log` 的 0 残留与连跑 3 次均为**实现方本地代理**结果，我未复核；D3 的「单点正确性由计数兜底」这一设计前提，其真实性恰由 PV2 承担。 |
| `cargo check --target x86_64-unknown-linux-gnu -p identity-keystore --tests`（handoff 声称 EXIT=0） | CI Linux `checks` job | 不影响判断。我只从代码形状侧确认 `unix_modes` 对守卫的用法（`TempDir::new`/`root.join(..)`/`remove_dir_all(&root)`）与 Windows 上已编译的 `atomic_tests` 同形；真实 Linux 编译在 CI。 |
| `storage-sqlite` Unix-only 行（`permissions.rs` 的 `set_permissions(&dir, ..)`/`inspect_path_permissions(&dir)`）未在本机编译 | CI Linux `checks` job | 不影响判断（trait 形状侧我已核对）；残余风险与 handoff 登记一致。 |
| `inventory.md` §1 的**基线** 28 行表 | ① 的「盘点无漏点」上游 | 不影响判断：结论由**改动后 24 行的独立复核 + 创建行为补扫**支撑，不依赖基线表复算。 |

### Merge verdict

**OK with notes**（对 `6c5e53b` 而言：无阻断项；RV1-F1 建议在本单元内一并修（一行），RV1-F2 为报告修正）。本次 PASS **不**代表 PV1/PV2、Project Verify 或最终验收通过。