<!-- 说明实现方案与决策理由；行为以既有契约为准（本变更 skip_specs：产品行为不变），协作安排写 plan.md。 -->

## Context

动机见 `proposal.md` 的 Why。影响方案的现状事实（2026-09-25 盘点，均已读码核实）：

- **主泄漏点**：`crates/storage-sqlite/tests/support/mod.rs` 的 `temp_dir(name)` 返回裸 `PathBuf`，
  只在**创建前**删除同名旧目录，测试结束不清理；调用点约 **100 处**。
- **次要泄漏点**：`crates/identity-keystore/src/store.rs` 的 `#[cfg(test)]` 测试（`acpr-keystore-modes-*`、
  `acpr-keystore-atomic-*`，后者在本次清理的存量中被实证）；`crates/app/src/cli/input.rs` 测试
  （`acpr-wp4b-input-{pid}`）；`crates/agent-host/tests/{catalog,supervision}.rs` 的
  `acpr-agent-host-*.txt` 文件。
- **待核实点**：`crates/core/src/use_cases.rs` 测试的 `acpr-ws-*`（第 1334 行附近，需确认是否真实创建）、
  `crates/server/src/local_admin/{audit,params}.rs` 的散落创建点。
- **已有正确形态**：`app` 测试的 `TempRoot`/`TempDir`（`tests/support/mod.rs`、`src/compose.rs`）、
  `app::lock` 测试 `TempDir`、`identity-keystore/tests/support` 的 `TempRoot`、
  `server` 的 `TestWorld::temporary_directory`（登记进列表、随 world 清理）。这些证明「Drop 守卫」
  形态在本仓库已成立，本次是把同一形态补齐到所有创建点。
- 约束（proposal 的 Intent）：只改测试代码；每 crate 的测试助手各自落地守卫（AGENTS.md §4 禁止
  共享 utils crate）；不新增依赖；产品行为与 16 个既有能力规范不变。

## Goals / Non-Goals

**Goals:**

- 测试自建临时目录/文件在**正常结束与 panic 展开**两条路径上都被删除（Drop 守卫）。
- 存量约 100+ 调用点的迁移成本最小化（守卫 `Deref<Target = Path>`，多数调用点零改动）。
- 验收可重复执行：完整测试运行前后 `acpr-*` 条目数之差为 0。

**Non-Goals:**

- 不统一各 crate 助手的其它形态差异（命名、0700 处理等保持各 crate 现状）。
- 不引入 CI 门禁/lint 防回归（记录为后续候选，见 Risks）。
- 不处理已被删除的历史存量与无代码来源的一次性探针目录（`acpr-job-probe` 等）。

## Decisions

### D1：守卫形态——`Drop` + `Deref<Target = Path>`，不加新依赖

每个泄漏点所在 crate 的测试支持模块新增（或复用已有）一个守卫：

```text
struct TempDir { path: PathBuf }            // 名字各 crate 自定（storage 可叫 TempDir）
impl Deref<Target = Path>                    // 让 &guard、guard.join()、guard.display() 照常工作
impl Drop { let _ = remove_dir_all(path) }   // 忽略错误、绝不 panic（见 D3）
```

- 选它而不是 `tempfile` crate：std 足够（现有 `TempRoot` 就是这么写的），不新增依赖符合
  AGENTS.md §7 的依赖纪律。
- 选 `Deref` 而不是改签名返回 `PathBuf` + 手动清理：100 处调用点里绝大多数是
  `let dir = temp_dir("x"); …&dir / dir.join()…`，`Deref` 下零改动。
- 创建函数标 `#[must_use]`：语句级裸调用（`temp_dir("x");`）会在语句结束立即删除目录，
  `#[must_use]` 让这种误用在编译期报警。

### D2：调用点审计准则（防止「临时变量提前 Drop」）

迁移时逐点检查两种危险写法并改写为「先绑定再使用」：

1. `foo(temp_dir("x"))`（按值/按引用传临时值）→ 语句结束即删除目录；
2. `temp_dir("x").join("f")` 链式调用 → 同上。

编译器不会报这两种（借用临时值合法），因此审计必须**人工逐点**；判据是「目录的生命周期覆盖
整个测试体」。`server`/`identity-keystore` 的既有登记式形态（`TestWorld::temporary_directory`）
不在此列。

### D3：Drop 的语义是「尽力而为」，验收靠端到端计数兜底

- `Drop` 内 `let _ = std::fs::remove_dir_all(...)`：目录可能已被测试自己删除/移动，Windows 上也可能
  因句柄释放时序暂时删不掉；守卫不因清理失败 panic（展开中 panic 会 abort）。
- 测试若持有打开的资源（如 `SqliteStore` 连接池），必须先显式 drop 再让守卫出作用域；审计时核对
  此类测试的 drop 顺序。
- 因 D3 的「尽力而为」，单点正确性不由单元自证，而由验收的**前后计数差为 0** 兜底（见 tasks 的
  验证任务）；系统性失败（某个 helper 忘记删）会在计数上立刻显形。

### D4：命名唯一性保持现状

沿用各创建点现有的 `{pid}`/`{sequence}`/`线程号` 唯一化（并行测试互不误删），守卫只删除自己创建的
路径。审计时确认没有两个 helper 使用同一固定名（如 `acpr-agent-host-env.txt` 这类固定名在
并行 binary 之间是不同进程、pid 不同，安全；同 binary 内串行/并行用例共用一个固定文件名的，
改为带 pid/sequence 或落在各自目录内）。

### D5：文件类临时产物同原则处理

`agent-host` 的 `acpr-agent-host-*.txt` 是文件不是目录：同一守卫思路（`Drop` 里 `remove_file`），
或把文件放进该用例的守卫目录内。逐点核实后取改动小的一种。

## Risks / Trade-offs

- [迁移漏点：某个调用点仍用裸路径，或某 crate 还有未被本次盘点发现的创建点] → tasks 第一步用
  `grep -rn 'temp_dir()' crates/ --include=*.rs` 的全量清单建核对表，逐点标记「已守卫/无泄漏/已修复」；
  验收以前后计数差为 0 收口，漏点会在计数上显形。
- [守卫改变测试进程失败后的现场保留能力：以前失败后目录还在可查，现在被清掉] → 失败时
  cargo 已输出断言信息；确需现场时可临时注释 Drop。接受此取舍（不清理的代价已被 1.4 万个
  残留目录证明）。
- [Windows 句柄释放时序导致偶发删不掉] → Drop 尽力而为 + 验收计数允许逐个解释残余
  （残余必须能归因到具体用例并修复，不得用「尽力而为」豁免系统性泄漏）。
- [后续回归：新增测试再写裸 `temp_dir`] → 本变更不加门禁；把「测试临时产物必须经守卫」写进
  各 helper 的文档注释。若要机器防线，另立变更评估在 `scripts/` 加前后计数检查进 CI（不在本次范围）。
