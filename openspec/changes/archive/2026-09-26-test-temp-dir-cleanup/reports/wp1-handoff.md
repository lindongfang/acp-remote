```yaml
task_id: "2.1-2.4"            # 本报告覆盖的权威任务；逐任务索引见下方 handoff_index
role: coder                   # 实现 Agent（openspec/schemas/agentic/roles/coder.md）
phase: implement
agent_context: "子 Agent（实现 Agent/worker）：独立上下文，不继承主 Agent 会话历史，也不承担 review/验证/集成角色。运行环境未回传实际 Agent ID；本会话产物目录 ID = 3124ebbd-5938-4e93-ba2f-ecdff725607b。plan.md 把 WP1 的实现角色记为 deepseek/deepseek-flash，实际模型由 openspec/config.yaml 的 x-agentic 决定，本人无法自证该值。"
target_revision: "NOT_AVAILABLE —— 工作包不提交代码；改动停在基线 1359a13 之上的工作树（14 个文件已修改、未暂存），交付提交由主 Agent 固化。"
scope: "WP1（implement，DU1 的唯一工作包）= tasks 2.1/2.2/2.3/2.4；写入面严格限制在 crates/*/tests/** 与 crates/*/src/** 的 #[cfg(test)] 模块 + 本变更的 reports/。未触碰 docs/、schemas/、fixtures/、compatibility/、openspec/ 其它路径与根配置文件；未触碰任何非测试代码。"
changes:
  - crates/storage-sqlite/tests/support/mod.rs      # temp_dir() 改为返回 Drop 守卫 TempDir
  - crates/storage-sqlite/tests/admin_audit.rs      # open() 返回 (TempDir, SqliteStore)，14 个调用点改绑定顺序
  - crates/storage-sqlite/tests/session_version_rule.rs  # 3 个用例补 store.close().await
  - crates/core/src/use_cases.rs                    # 测试模块内 TempDir 守卫 + temp_dir(name)
  - crates/identity-keystore/src/store.rs           # #[cfg(test)] mod temp_dirs 守卫，unix_modes/atomic_tests 使用
  - crates/server/src/local_admin/test_support.rs   # TempDir + TempFile 两个守卫（测试支撑模块）
  - crates/server/src/local_admin/audit.rs          # temp_path() 返回 TempFile
  - crates/server/src/local_admin/params.rs         # temp_directory() 返回 TempDir
  - crates/app/src/cli/input.rs                     # 测试模块内 TempDir 守卫 + temp_dir()
  - crates/app/src/compose.rs                       # 共享句柄用例显式关池（否则临时目录删不掉）
  - crates/app/tests/support/mod.rs                 # run_cli 的输出目录改由既有 TempRoot 托管
  - crates/agent-host/tests/support/mod.rs          # TempFile 守卫
  - crates/agent-host/tests/catalog.rs              # 三个路径 helper 返回 TempFile + 两处守卫生命周期
  - crates/agent-host/tests/supervision.rs          # 三个固定名临时文件改经 TempFile
checks:
  - "cargo test -p core / -p identity-keystore / -p storage-sqlite / -p server / -p app / -p agent-host：全部 EXIT=0，acpr-* 残留 0 → reports/wp1-local-checks.log"
  - "cargo fmt --all -- --check：EXIT=0"
  - "cargo clippy --locked --workspace --all-targets --all-features -- -D warnings：EXIT=0"
  - "cargo test --locked --workspace --all-features：EXIT=0 且 acpr-* 前后差值 = 0（本地代理 [PV2]）；另独立连跑 3 次，均为差值 0"
  - "cargo check --target x86_64-unknown-linux-gnu -p identity-keystore --tests：EXIT=0（本机 Windows 上 cfg(unix) 不参与构建，用交叉 target 类型检查 Unix-only 调用点）"
issues:
  - "已修复（实现中发现）：storage-sqlite 的 3 个 session_version 用例没有关池，守卫 Drop 时 Windows 句柄仍在 → 残留 3 个目录；按 design D3 补 store.close().await。"
  - "已修复（实现中发现，非 temp_dir() 字面命中）：app::compose 的 a_shared_store_handle_blocks_the_checkpoint 故意让 close() 失败，连接池从未关闭 → 每次运行残留 1 个目录；用例内显式 try_unwrap + close().await。"
  - "已缓解：storage-sqlite 的 admin_store 用例在满载并行的完整运行里偶发残留 1 次（单跑 10/10 干净）；新守卫 Drop 改为「失败重试 10×50ms、NotFound 立即返回」，此后 3×storage + 3×full-workspace 全为 0 残留。"
  - "残余限制（如实登记）：storage-sqlite 的 Unix-only 测试行 permissions.rs:83/97（std::fs::set_permissions(&dir, ..) 与 inspect_path_permissions(&dir)）在本机不参与编译（libsqlite3-sys 交叉编译需要 x86_64-linux-gnu-gcc，本机没有），因此这两行只做了「等价最小程序」的 trait 形状验证（/tmp/guardprobe/probe4.rs，rustc EXIT=0），未在真实 Unix 构建里编译。"
result: PASS
evidence_paths:
  - openspec/changes/test-temp-dir-cleanup/reports/inventory.md        # 任务 2.1 的盘点核对表
  - openspec/changes/test-temp-dir-cleanup/reports/wp1-local-checks.log # 逐命令 + 退出码 + 残留计数原始输出
  - openspec/changes/test-temp-dir-cleanup/reports/wp1-handoff.md      # 本报告
resource_cleanup: "临时目录（本机 /tmp = C:\\Users\\zhang\\AppData\\Local\\Temp）在最后一次完整运行后 acpr-* 条目数 = 0（无本次残留；实现期间发现的 4 个历史残留目录已由本人删除）。自带资源：/tmp/wp1-checks.sh（证据生成脚本，保留以便复跑）、/tmp/guardprobe/probe4.rs（Unix-only trait 形状的最小复现，保留供 reviewer 复算）；probe 的 .exe 已删。未新增数据库、容器、端口或外部账号。"
handoff_index:
  - task_id: "2.1"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "NOT_AVAILABLE（工作包不提交；基线 1359a13，改动在工作树）"
    evidence_type: CHECK
    evidence_id: NOT_APPLICABLE
    report_path: openspec/changes/test-temp-dir-cleanup/reports/inventory.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "盘点以基线 grep（28 行）逐行分类，并在改动后复核当前 grep（24 行）无未分类点；口径为『是否创建条目』，含 3 处 grep 之外但实测留残留的点（session_version/compose/admin_store 偶发）。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.2"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "NOT_AVAILABLE（同上）"
    evidence_type: CHECK
    evidence_id: NOT_APPLICABLE
    report_path: openspec/changes/test-temp-dir-cleanup/reports/wp1-local-checks.log
    result: PASS
    evidence_status: NEW
    applicability_basis: "cargo test -p storage-sqlite EXIT=0（14 个测试目标全绿），acpr-* 残留 0；0700 创建口径保留（Unix 分支未改动）。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.3"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "NOT_AVAILABLE（同上）"
    evidence_type: CHECK
    evidence_id: NOT_APPLICABLE
    report_path: openspec/changes/test-temp-dir-cleanup/reports/inventory.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "core/identity-keystore/server/app/agent-host 各 crate 的 cargo test 均 EXIT=0 且残留 0；判定『无泄漏』的 9 个点逐条给出依据。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.4"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "NOT_AVAILABLE（同上）"
    evidence_type: CHECK
    evidence_id: NOT_APPLICABLE
    report_path: openspec/changes/test-temp-dir-cleanup/reports/wp1-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "cargo fmt --all -- --check 与 cargo clippy --locked --workspace --all-targets --all-features -- -D warnings 均 EXIT=0；交接报告即本文件。"
    source_evidence: NOT_APPLICABLE
```

# WP1 实现交接（test-temp-dir-cleanup / DU1）

## 结论（TL;DR）

WP1 的三件事都做完了：**盘点**（28 处 `temp_dir()` 命中逐行分类，另发现 3 处 grep 之外的实测残留）、
**修复**（13 个创建点 + 3 处非字面命中点）、**收尾检查**（`cargo test -p <crate>` ×6、`cargo fmt`、
`cargo clippy`、完整 `cargo test --locked --workspace --all-features` 全绿）。
本地代理 [PV2]：完整工作区测试运行前后系统临时目录 `acpr-*` 条目数差值 **0**（连跑 3 次均为 0）。
未触碰任何产品代码、契约资产或断言语义；`git diff --cached` 为空（无暂存）。

## 1. 实现内容（design D1–D5 对应）

### D1 守卫形态（每 crate 自己的测试支撑模块，无新依赖，只用 std）

| crate | 守卫 | 位置 | 形态 |
| --- | --- | --- | --- |
| storage-sqlite | `TempDir`（由 `temp_dir()` 返回） | `tests/support/mod.rs` | `Deref<Target = Path>` + `AsRef<Path>` + `AsRef<OsStr>` + `#[must_use]` |
| core | `TempDir`（测试模块私有） | `src/use_cases.rs` 的 `mod tests` | `Deref<Target = Path>` + `AsRef<Path>` + `#[must_use] fn temp_dir(name)` |
| identity-keystore | `temp_dirs::TempDir`（`#[cfg(test)]` 模块） | `src/store.rs` | `Deref<Target = Path>` + `AsRef<Path>` + `#[must_use]`（RV1-F1 修复补上） |
| server | `test_support::TempDir` / `test_support::TempFile` | `src/local_admin/test_support.rs`（文件级 `#![cfg(test)]`） | 同上；`TempFile` 不创建文件 |
| app | `TempDir`（测试模块私有） | `src/cli/input.rs` 的 `mod tests` | `Deref<Target = Path>` + `AsRef<Path>` + `#[must_use] fn temp_dir()` |
| agent-host | `TempFile` | `tests/support/mod.rs` | `Deref<Target = Path>` + `AsRef<Path>` + `#[must_use]` |

- 为什么除了 `Deref` 还要 `AsRef<Path>`/`AsRef<OsStr>`：`Deref` 只覆盖「`&guard` 当 `&Path` 形参」的场景；
  `std::fs::remove_dir_all(&dir)`、`set_permissions(&dir, ..)` 的入参是泛型 `P: AsRef<Path>`，
  `StorageConfig::new(&dir)`（`impl Into<PathBuf>`）则需要 `TempDir: AsRef<OsStr>`（std 的
  `impl<T: ?Sized + AsRef<OsStr>> From<&T> for PathBuf`）。加了这两个 impl，`storage-sqlite` 的
  **101 个调用点零改动**（只有 `admin_audit::open()` 的元组顺序按 D3 调整）。
- 创建函数一律 `#[must_use]`：语句级裸调用（`temp_dir("x");`）会在语句结束即删目录，让编译器报警。
- **不新增依赖**：仅 std（`std::ops::Deref`/`AsRef`/`Drop`/`std::io::ErrorKind`）。

### D2 inline 临时值审计

- storage-sqlite 的 101 个调用点**全部**是 `let dir = temp_dir(..)`（复核命令见 inventory.md §4），
  没有 `foo(temp_dir("x"))` / `temp_dir("x").join(..)` 这类写法，因此没有需要改写的调用点。
- 新守卫的返回值全部绑定到命名局部变量。实现中发现 agent-host 有两处会把守卫**当场丢弃**
  （`let (profile, _) = dumping_profile(..)` 与 `vec![dumping_profile(..).0]`），已改成绑定到用例作用域
  （`agent-good` 的守卫改名为 `_env_guard` 留在同级作用域；`agent-broken` 的守卫改名为 `broken_env`，
  并复用它做「进程不得启动」的存在性断言，删掉了重复的 `env_path("broken")`）。

### D3 Drop 不 panic + 先释放资源

- 所有守卫的 Drop 都是「尽力而为」：`remove_dir_all`/`remove_file` 失败时**不 panic**（展开中 panic 会
  abort）；`NotFound` 立即返回（用例自带显式清理时不做无谓重试）；其他错误重试 10×50ms，口径与 `app` 测试
  既有的 `TempRoot` 一致（它的文档就写明「Windows 上句柄释放有延迟，因此带重试」）。
- 「先释放资源再让守卫出作用域」的三处落地：
  1. `storage-sqlite/tests/admin_audit.rs::open()` 改为返回 `(TempDir, SqliteStore)`，14 个调用点写成
     `let (_dir, store) = open(..)`，靠局部变量逆序析构保证连接池先释放（注释写明了这条约束）；
  2. `storage-sqlite/tests/session_version_rule.rs` 的 3 个用例补 `store.close().await;`
     （它们的 `SqliteStore` 连接池是异步释放的，守卫 Drop 时句柄仍在，实测残留 3 个目录）；
  3. `app/src/compose.rs::a_shared_store_handle_blocks_the_checkpoint` 在 `drop(extra)` 之后
     `Arc::try_unwrap(store).expect(..).close().await`（该用例故意让 `close()` 返回 `StoreStillShared`，
     因此必须自己关池；实测此前每次运行残留 1 个目录）。

### D4 命名唯一性

沿用各创建点现有唯一化：`{pid}`（多数）+ 序号/线程号。改动点：
- `app/src/cli/input.rs` 的测试目录从 `acpr-wp4b-input-{pid}` 改为 `acpr-wp4b-input-{pid}-ThreadId(..)`（同 binary 唯一）；
- `app/tests/support/mod.rs` 的 `run_cli` 输出目录改由 `TempRoot::new("cli-{label}")` 提供，唯一性来自
  `TempRoot` 自己的计数器（同时删掉了因此不再使用的 `next_cli_counter`）；
- agent-host 三个固定名（`acpr-agent-host-env.txt` / `-heartbeat.txt` / `-heartbeat-terminate.txt`）**保持不变**：
  同一 binary 内各只有一个用例使用（D4 的豁免条件），改名只会扩大 diff。
- 守卫只删自己持有的路径；同一路径出现两个守卫（`_env_guard` 与 `broken_env` 那类）也只删同一文件，无跨用例影响。

### D5 文件类产物

`agent-host` 的 `acpr-agent-host-*.txt` 与 `server::local_admin::audit` 的 `acpr-wp3b1-audit-*` 都是文件：
采用「同原则的文件守卫」（`TempFile`，Drop 里 `remove_file`），而不是把文件塞进目录（后者会改路径形状与
断言文本，改动更大）。

## 2. 变更文件清单（14 个，均在允许写入面内）

```
crates/agent-host/tests/support/mod.rs                 | 47 ++
crates/storage-sqlite/tests/support/mod.rs             | 51 ++++++   3 -
crates/server/src/local_admin/test_support.rs          | 96 ++++++   1 -
crates/identity-keystore/src/store.rs                  | 59 ++++++   2 -
crates/app/src/cli/input.rs                            | 49 ++++++   2 -
crates/core/src/use_cases.rs                           | 45 ++++++   2 -
crates/storage-sqlite/tests/admin_audit.rs             | 20 ++++ 19 -
crates/agent-host/tests/catalog.rs                     | 16 ++++ 17 -
crates/app/tests/support/mod.rs                        |  6 ++++ 16 -
crates/storage-sqlite/tests/session_version_rule.rs    |  6 ++++
crates/app/src/compose.rs                              |  8 ++++
crates/server/src/local_admin/params.rs                |  4 ++++  5 -
crates/agent-host/tests/supervision.rs                 |  4 ++++  4 -
crates/server/src/local_admin/audit.rs                 |  3 ++++  2 -
14 files changed, 414 insertions(+), 73 deletions(-)      # git diff --numstat 的真实值
```

非测试代码零改动：每个 hunk 都落在 `tests/**`、`#[cfg(test)]` 模块或文件级 `#![cfg(test)]` 内
（`server/src/local_admin/test_support.rs` 文件首行即 `#![cfg(test)]`）。

## 3. 检查证据（原始输出见 `reports/wp1-local-checks.log`）

| 检查 | 命令 | 退出码 | 结果 |
| --- | --- | --- | --- |
| 局部（core） | `cargo test -p core` | 0 | 94 passed / 0 failed；残留 0 |
| 局部（identity-keystore） | `cargo test -p identity-keystore` | 0 | 6 个目标全绿；残留 0 |
| 局部（storage-sqlite） | `cargo test -p storage-sqlite` | 0 | 14 个目标全绿（1 ignored=夹具生成器）；残留 0 |
| 局部（server） | `cargo test -p server` | 0 | 7 个目标全绿；残留 0 |
| 局部（app） | `cargo test -p app` | 0 | 6 个目标全绿；残留 0 |
| 局部（agent-host） | `cargo test -p agent-host` | 0 | 7 个目标全绿；残留 0 |
| 收尾 | `cargo fmt --all -- --check` | 0 | 无 diff |
| 收尾 | `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | 0 | 无告警 |
| 全量（[PV2] 本地代理） | `cargo test --locked --workspace --all-features` | 0 | 82 个测试目标全绿；`acpr-*` 前后 0 → 0，**差值 0** |
| 复算 | 上条独立连跑 3 次 + `cargo test -p storage-sqlite` 连跑 3 次 | 0 | 每次残留均为 0 |
| Unix-only 类型检查 | `cargo check --target x86_64-unknown-linux-gnu -p identity-keystore --tests` | 0 | `unix_modes` 的守卫调用点可编译（本机 Windows 不编译 `cfg(unix)`） |

> [PV1] `npm run verify`（含十道合同门禁）与 [PV2] 的正式计数验收由主 Agent 在 3.1 执行；本报告只声明
> 实现期的局部与收尾检查，不冒充 Project Verify 结论。

## 4. 未执行项与残余风险

- **未执行**：`npm run verify`（[PV1]）、正式 [PV2] 计数窗口、独立 review（[RV1]）、集成/合入（6.x/7.x）——
  这些不是 coder 角色的授权范围。
- **未执行的替代**：`storage-sqlite` 的 Unix-only 测试行（`permissions.rs` 的 `set_permissions(&dir, ..)`、
  `inspect_path_permissions(&dir)`）无法本机交叉编译（`libsqlite3-sys` 需要 `x86_64-linux-gnu-gcc`），
  改用等价最小程序验证 trait 形状（`rustc` 退出码 0）。CI 的 Linux `checks` job 会真正编译它们。
  风险等级：低（改动只是 `dir` 的类型从 `PathBuf` 换成 `TempDir`，且该守卫在所有平台都编译并以
  `set_permissions(&dir, ..)` 的等价形状验证过）。
- **残余风险（如实登记）**：Windows 上要删得掉必须句柄已释放；守卫只能「尽力而为 + 重试」。
  若将来有新用例忘记关池，残留会由 [PV2] 的计数立刻显形（按名字前缀可归因到 crate/用例），
  不能用「尽力而为」豁免。另：进程被强杀（CI 取消）时无法清理，属平台事实。
- **一处行为差异（有意）**：`run_cli` 的输出目录名从 `acpr-wp4b-cli-*` 变成 `acpr-wp4a-cli-*`
  （复用既有 `TempRoot`）。`grep` 确认没有任何测试或文档依赖旧名字前缀；[PV2] 只按 `acpr-*` 过滤。

## 5. 结构化验收报告（acceptance-report）

```acceptance-report
{
  "criteriaSatisfied": [
    {
      "id": "criterion-1",
      "status": "satisfied",
      "evidence": "14 个改动文件全部落在允许写入面（crates/*/tests/**、crates/*/src/** 的 #[cfg(test)] 模块/文件级 #![cfg(test)]、本变更 reports/）；git diff --numstat 显示没有产品代码、docs/、schemas/、fixtures/、compatibility/ 改动；未改动任何断言语义，只调整临时资源生命周期；未新增依赖（守卫只用 std）。"
    },
    {
      "id": "criterion-2",
      "status": "satisfied",
      "evidence": "reports/inventory.md（基线 grep 28 行逐行分类 + 改动后 grep 24 行复核 + 3 处非字面命中残留点）、reports/wp1-local-checks.log（逐命令退出码与 acpr-* 计数）、本报告的变更清单与检查表；完整工作区测试连跑 3 次 acpr-* 差值均为 0。"
    }
  ],
  "changedFiles": [
    "crates/storage-sqlite/tests/support/mod.rs",
    "crates/storage-sqlite/tests/admin_audit.rs",
    "crates/storage-sqlite/tests/session_version_rule.rs",
    "crates/core/src/use_cases.rs",
    "crates/identity-keystore/src/store.rs",
    "crates/server/src/local_admin/test_support.rs",
    "crates/server/src/local_admin/audit.rs",
    "crates/server/src/local_admin/params.rs",
    "crates/app/src/cli/input.rs",
    "crates/app/src/compose.rs",
    "crates/app/tests/support/mod.rs",
    "crates/agent-host/tests/support/mod.rs",
    "crates/agent-host/tests/catalog.rs",
    "crates/agent-host/tests/supervision.rs"
  ],
  "testsAddedOrUpdated": [
    "crates/storage-sqlite/tests/support/mod.rs",
    "crates/storage-sqlite/tests/admin_audit.rs",
    "crates/storage-sqlite/tests/session_version_rule.rs",
    "crates/core/src/use_cases.rs",
    "crates/identity-keystore/src/store.rs",
    "crates/server/src/local_admin/test_support.rs",
    "crates/server/src/local_admin/audit.rs",
    "crates/server/src/local_admin/params.rs",
    "crates/app/src/cli/input.rs",
    "crates/app/src/compose.rs",
    "crates/app/tests/support/mod.rs",
    "crates/agent-host/tests/support/mod.rs",
    "crates/agent-host/tests/catalog.rs",
    "crates/agent-host/tests/supervision.rs"
  ],
  "commandsRun": [
    {
      "command": "cargo test -p core",
      "result": "passed",
      "summary": "94 passed / 0 failed；acpr-* 残留 0"
    },
    {
      "command": "cargo test -p identity-keystore",
      "result": "passed",
      "summary": "6 个测试目标全绿；acpr-* 残留 0"
    },
    {
      "command": "cargo test -p storage-sqlite",
      "result": "passed",
      "summary": "14 个测试目标全绿（1 ignored = 夹具生成器）；acpr-* 残留 0；连跑 3 次均 0"
    },
    {
      "command": "cargo test -p server",
      "result": "passed",
      "summary": "7 个测试目标全绿；acpr-* 残留 0"
    },
    {
      "command": "cargo test -p app",
      "result": "passed",
      "summary": "6 个测试目标全绿；acpr-* 残留 0"
    },
    {
      "command": "cargo test -p agent-host",
      "result": "passed",
      "summary": "7 个测试目标全绿；acpr-* 残留 0"
    },
    {
      "command": "cargo fmt --all -- --check",
      "result": "passed",
      "summary": "EXIT=0，无 diff"
    },
    {
      "command": "cargo clippy --locked --workspace --all-targets --all-features -- -D warnings",
      "result": "passed",
      "summary": "EXIT=0，无告警"
    },
    {
      "command": "cargo test --locked --workspace --all-features",
      "result": "passed",
      "summary": "EXIT=0；82 个测试目标全绿；acpr-* 前后 0 -> 0，差值 0；独立连跑 3 次均为 0"
    },
    {
      "command": "cargo check --target x86_64-unknown-linux-gnu -p identity-keystore --tests",
      "result": "passed",
      "summary": "EXIT=0；用交叉 target 类型检查 Windows 本机不编译的 cfg(unix) 守卫调用点"
    },
    {
      "command": "rustc --edition 2021 /tmp/guardprobe/probe4.rs",
      "result": "passed",
      "summary": "EXIT=0；复刻 storage-sqlite 守卫的 trait 形状（Deref/AsRef<Path>/AsRef<OsStr> + set_permissions(&dir,..) + impl Into<PathBuf>）"
    },
    {
      "command": "npm run verify",
      "result": "not-run",
      "summary": "属 [PV1]，由主 Agent 在 3.1 执行；coder 不冒充 Project Verify 结论"
    },
    {
      "command": "cargo test --locked --workspace --all-features（正式 [PV2] 计数窗口）",
      "result": "not-run",
      "summary": "正式 [PV2] 由主 Agent 独占窗口执行；本报告只提供实现期的本地代理结果（差值 0）"
    }
  ],
  "validationOutput": [
    "cargo test -p storage-sqlite => EXIT=0；14 个目标 ok（含 admin_audit 14 passed、admin_store 32 passed、commit 15 passed 等）",
    "cargo test --locked --workspace --all-features => EXIT=0；acpr-* before=0 after=0；残余条目：（无）",
    "cargo fmt --all -- --check => EXIT=0",
    "cargo clippy --locked --workspace --all-targets --all-features -- -D warnings => EXIT=0",
    "实现中发现并修复的残留：acpr-storage-session-version-*（3 个，用例未关池）、acpr-wp4a-compose-shared-*（1 个，故意共享句柄）",
    "满载并行时的偶发残留 1 次（acpr-storage-admin-pairing-binding-*）：单跑 10/10 干净 -> 守卫 Drop 加 10x50ms 重试后 3xstorage + 3xworkspace 全为 0"
  ],
  "residualRisks": [
    "storage-sqlite 的 Unix-only 测试行（permissions.rs 的 set_permissions(&dir, ..) / inspect_path_permissions(&dir)）本机不参与编译（libsqlite3-sys 交叉编译缺 x86_64-linux-gnu-gcc），只做了等价最小程序的 trait 形状验证；CI 的 Linux checks job 会真正编译它们。",
    "守卫是 design D3 的『尽力而为 + 重试』：句柄未释放时删不掉。新用例若忘记关池，残留由 [PV2] 的 acpr-* 计数显形（可按前缀归因），不能用『尽力而为』豁免。",
    "进程被强杀（CI 取消等）时无法清理，属平台事实，无法由 Drop 守卫覆盖。",
    "run_cli 的输出目录前缀从 acpr-wp4b-cli-* 变为 acpr-wp4a-cli-*（复用既有 TempRoot）；grep 确认无测试/文档依赖该名字。"
  ],
  "noStagedFiles": true,
  "diffSummary": "14 个文件（+414/-73），全部在测试面内：为 storage-sqlite/core/identity-keystore/server/app/agent-host 逐个落地 Drop 守卫（Deref + AsRef + #[must_use]，Drop 不 panic 且带 10x50ms 重试），修复 13 个创建点与 3 处非字面命中的残留（补显式关池 / 显式关池并等待），保留 0700 创建口径与全部既有断言。",
  "reviewFindings": [
    "no blockers",
    "note: app/src/compose.rs::a_shared_store_handle_blocks_the_checkpoint 的修改只是新增显式关池，两个既有断言（StoreStillShared、message 含 shared）未改动，属 design D3 的『先释放资源』要求。",
    "note: agent-host/tests/catalog.rs 中两处守卫生命周期修正（_env_guard / broken_env）改变了守卫的作用域，未改变任何断言（broken_env 复用了原本重复创建的同一路径）。"
  ],
  "manualNotes": "1) 本机 git bash 的 /tmp 映射到 C:\Users\zhang\AppData\Local\Temp，且 Rust 测试进程读到的 TEMP 也是该目录，因此 [PV2] 计数请以该目录为准。2) 实现开始前该目录已有 4 个历史 acpr-* 残留（3 个 session-version + 1 个 compose-shared，均由本人本次实验首次产生），已用 node fs.rmSync 删除；当前为 0。3) 按运行环境的产物路径覆盖要求，同一份 WP1 交接报告同时写在 openspec/changes/test-temp-dir-cleanup/reports/wp1-handoff.md（handoff_index 指向的路径）与子 Agent 产物目录下同名文件。4) 未新增测试用例：本次只调整既有用例的临时资源生命周期，断言与覆盖不变；新增的守卫自身没有单测（design D3 已说明单点正确性由 [PV2] 的计数兜底）。5) 未提交、未暂存、未推送；交付提交由主 Agent 固化。"
}
```
