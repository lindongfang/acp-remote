# 测试临时目录/文件盘点核对表（任务 2.1）

- 变更：`test-temp-dir-cleanup`（WP1，实现 Agent/coder）
- 基线：分支 `feat/test-temp-dir-cleanup` @ `1359a13`（工作树改动未提交，交付提交由主 Agent 固化）
- 盘点命令（基线）：`git grep -n 'temp_dir()' 1359a13 -- 'crates/*.rs' 'crates/**/*.rs'`（等价于任务书里的
  `grep -rn 'temp_dir()' crates/ --include='*.rs'`，同一批 28 行，逐行一致）
- 分类口径：**已有守卫**＝创建点已由 `Drop` 结构体托管；**无泄漏**＝该行只拼接路径/字符串，从不创建条目；
  **待修复**＝创建了目录或文件且只在用例末尾手动删除（panic 展开路径会留残留）。
- 任务书里「当前约 25 个创建点」与实际 grep 的 28 行差 3 行，差额是「无泄漏（仅拼路径）」的条目（#3/#7/#16 等），
  创建点只有 13 个（见下表统计）。

## 1. 基线核对表（`temp_dir()` 的 28 处命中，逐行分类）

| # | 位置 | 基线形态 | 分类 | 处置与依据 |
| --- | --- | --- | --- | --- |
| 1 | `agent-host/tests/catalog.rs:55` `temp_path()` | `acpr-agent-host-{tag}-{pid}.txt`，由 fake child 写出；用例末尾 `remove_file` | 待修复 | 已守卫：`support::TempFile`（Drop=remove_file，带重试） |
| 2 | `agent-host/tests/catalog.rs:406` | `acpr-agent-host-config-{pid}.json`，写出 + 末尾删 | 待修复 | 已守卫：`support::TempFile` |
| 3 | `agent-host/tests/supervision.rs:36` | `"cwd": temp_dir().to_string_lossy()`（`session/new` 参数） | 无泄漏 | 依据：只取既存系统临时目录的路径文本当 cwd，不新建任何条目 |
| 4 | `agent-host/tests/supervision.rs:315` | `acpr-agent-host-env.txt`，写出 + 末尾删 | 待修复 | 已守卫：`TempFile`；该固定名在本 binary 内只有这一个用例使用（D4 允许） |
| 5 | `agent-host/tests/supervision.rs:344` | `acpr-agent-host-heartbeat.txt`，同上 | 待修复 | 已守卫：`TempFile`（同上，单用例固定名） |
| 6 | `agent-host/tests/supervision.rs:432` | `acpr-agent-host-heartbeat-terminate.txt`，同上 | 待修复 | 已守卫：`TempFile`（同上，单用例固定名） |
| 7 | `agent-host/tests/support/mod.rs:460` | `ResolvedWorkspace` 的 cwd = `temp_dir()` 文本 | 无泄漏 | 依据：只是既存目录的路径字符串，测试不创建目录 |
| 8 | `app/src/cli/input.rs:228` | `acpr-wp4b-input-{pid}`：`create_dir_all` + 末尾删 | 待修复 | 已守卫：测试模块内 `TempDir`（名字加线程号，做到同 binary 唯一） |
| 9 | `app/src/compose.rs:800` | `TempDir::new`（测试模块自有守卫） | 已有守卫 | 保持（但该守卫在 `a_shared_store_handle_blocks_the_checkpoint` 上实测删不掉，另见 §3） |
| 10 | `app/src/config.rs:922` | `acpr-wp4a-config-consumed` 只出现在 TOML 文本里 | 无泄漏 | 依据：测试只做配置解析，不发生文件系统写入 |
| 11 | `app/src/lock.rs:251` | `TempDir::new`（测试模块自有守卫） | 已有守卫 | 保持 |
| 12 | `app/tests/support/mod.rs:69` | `TempRoot::new`（含重试的 Drop） | 已有守卫 | 保持 |
| 13 | `app/tests/support/mod.rs:639` | `acpr-wp4b-cli-{label}-{pid}-{counter}`：`create_dir_all` + 末尾删 | 待修复 | 已修复：改由 `TempRoot` 托管（并删除因此不再使用的 `next_cli_counter`） |
| 14 | `core/src/use_cases.rs:1334` | `acpr-ws-{uuid}`：`create_dir_all` + 末尾删 | 待修复 | 已守卫：测试模块内 `TempDir` |
| 15 | `core/src/use_cases.rs:1399` | `acpr-missing-{uuid}` 只作「目录不存在」的 workspace 路径 | 无泄漏 | 依据：断言「解析失败」，从不创建该目录 |
| 16 | `core/src/use_cases.rs:1758` | `acpr-missing-workspace-{uuid}`，同上 | 无泄漏 | 依据：同上（授权先于解析的回归用例） |
| 17 | `identity-keystore/src/store.rs:509` | `acpr-keystore-modes-{pid}-{seq}`（`unix_modes`，`cfg(all(test, unix))`）：创建 + 末尾删 | 待修复 | 已守卫：`temp_dirs::TempDir`（该守卫模块本身在**所有平台**编译，Unix 调用点已用 `--target x86_64-unknown-linux-gnu` 类型检查） |
| 18 | `identity-keystore/src/store.rs:562` | `acpr-keystore-atomic-{pid}-{seq}`：创建 + 末尾删 | 待修复 | 已守卫：`temp_dirs::TempDir` |
| 19 | `identity-keystore/tests/support/mod.rs:22` | `TempRoot::new`（Drop） | 已有守卫 | 保持 |
| 20 | `server/src/local_admin/audit.rs:221` | `acpr-wp3b1-audit-{pid}-{seq}-{name}`（**文件**）：写出 + 末尾 `remove_file` | 待修复 | 已守卫：`test_support::TempFile` |
| 21 | `server/src/local_admin/params.rs:991` | `acpr-wp3b1-params-{pid}-{seq}`：`create_dir_all` + 末尾删 | 待修复 | 已守卫：`test_support::TempDir` |
| 22 | `server/src/local_admin/test_support.rs:1756` | `TestWorld::temporary_directory()`：登记进 `TestWorld.temporary`，随 world 的 Drop 清理 | 已有守卫 | 保持（形态已是登记式守卫） |
| 23 | `server/src/local_admin/test_support.rs:1773` | `WorkspaceRecord` 的 rootPath = `temp_dir()` 文本 | 无泄漏 | 依据：只拼字符串，测试不创建目录 |
| 24 | `server/src/transport/local/platform/windows.rs:193` | `data_dir: std::env::temp_dir()`（`cfg(test)` 的 `LocalEndpointConfig`） | 无泄漏 | 依据：Windows 实现走 Named Pipe，`data_dir` 只被 Unix 的 `runtime_dir` 回落使用（`endpoint.rs` 的 `data_dir.join("run")` 是 `#[cfg(unix)]`），本平台不创建任何条目 |
| 25 | `server/tests/local_endpoint_unix.rs:31` | `TempDir::new`（该文件自有守卫） | 已有守卫 | 保持（Unix-only 文件，未改动） |
| 26 | `server/tests/local_endpoint_windows.rs:26` | `data_dir: std::env::temp_dir()` | 无泄漏 | 依据：同 #24 |
| 27 | `storage-sqlite/tests/admin_store.rs:468` | `absolute_path()`：只拼 `WorkspaceRecord` 的路径文本 | 无泄漏 | 依据：仅作为值对象的路径文本使用，无文件系统调用（`grep` 该 helper 的 11 个调用点全部只做记录字段） |
| 28 | `storage-sqlite/tests/support/mod.rs:45` | `temp_dir(name)` 返回裸 `PathBuf`（101 个调用点），只删同名旧目录，结束不清理 | 待修复 | 已修复：返回 `TempDir` 守卫（`Deref<Target = Path>` + `AsRef<Path>` + `AsRef<OsStr>` + `#[must_use]`），并保留 `0700` 创建口径 |

**统计**：28 处命中 = 已有守卫 6（#9/#11/#12/#19/#22/#25） + 无泄漏 9（#3/#7/#10/#15/#16/#23/#24/#26/#27）
+ 待修复 13（#1/#2/#4/#5/#6/#8/#13/#14/#17/#18/#20/#21/#28）。无未分类点。

## 2. 改动后复核（当前工作树的 `grep -rn 'temp_dir()' crates/ --include='*.rs'`，24 行）

改动后剩余的 24 行**全部**落在两类里，没有第三类：

| 当前命中 | 类别 |
| --- | --- |
| `agent-host/tests/support/mod.rs:39`（`TempFile::new` 内部）、`core/src/use_cases.rs:1146`、`identity-keystore/src/store.rs:500`、`server/src/local_admin/test_support.rs:1614`、`server/src/local_admin/test_support.rs:1660`、`app/src/cli/input.rs:180` | **守卫构造函数的内部实现**（都在 `#[must_use]` 的创建函数里，返回值必须绑定到局部变量） |
| `storage-sqlite/tests/support/mod.rs:93`、`app/tests/support/mod.rs:69`、`identity-keystore/tests/support/mod.rs:22`、`app/src/compose.rs:800`、`app/src/lock.rs:251`、`server/src/local_admin/test_support.rs:1851`、`server/tests/local_endpoint_unix.rs:31` | **已有守卫的构造函数**（`TempDir::new` / `TempRoot::new` / `TestWorld::temporary_directory`） |
| `agent-host/tests/supervision.rs:36`、`agent-host/tests/support/mod.rs:507`、`core/src/use_cases.rs:1442`、`core/src/use_cases.rs:1801`、`server/src/local_admin/test_support.rs:1868`、`server/src/transport/local/platform/windows.rs:193`、`server/tests/local_endpoint_windows.rs:26`、`storage-sqlite/tests/admin_store.rs:468`、`app/src/config.rs:922`、`app/src/cli/input.rs:179`（函数定义行）、`app/src/cli/input.rs:276`（调用点） | **无泄漏（只拼路径/字符串）** 或**守卫调用点**（`let dir = temp_dir();`） |

> `app/src/cli/input.rs:276` 是 `let dir = temp_dir();`（守卫绑定到局部变量，符合 D2）；`app/src/cli/input.rs:179`
> 是 `fn temp_dir() -> TempDir` 的定义行。两者都不是裸路径泄漏点。

## 3. 非 `temp_dir()` 命中、但实测会留残留的点（盘点扩展到「创建行为」而非「字面命中」）

盘点以 grep 为**起点**，因此另外核对了所有 `create_dir_all`/`create_dir` 与既有守卫的**实际清理结果**
（口径见 design 的 R1/验收判据：完整测试运行前后 `acpr-*` 条目数之差为 0）。发现两处 grep 之外的残留：

1. `storage-sqlite/tests/session_version_rule.rs` 的 3 个用例（`acpr-storage-session-version-*`）：
   用例没有显式 `store.close().await`，`SqliteStore` 的连接池释放是**异步**的，守卫 Drop 时文件句柄仍在
   → Windows 上 `remove_dir_all` 失败。处置（design D3「持有打开资源的用例必须先显式释放资源」）：
   三个用例各补一行 `store.close().await;`。
2. `app/src/compose.rs` 的 `a_shared_store_handle_blocks_the_checkpoint`（`acpr-wp4a-compose-shared-*`）：
   该用例**故意**让 `Composition::close()` 以 `StoreStillShared` 失败，因此连接池从未被关闭，句柄留到最后
   → 守卫删不掉。处置：用例先留一份 `Arc<SqliteStore>`，`drop(extra)` 后 `Arc::try_unwrap(..).close().await`
   显式关池并等待（三次连跑残留均为 0；断言未变）。
3. `storage-sqlite/tests/admin_store.rs` 的 `claim_with_a_foreign_host_binding_is_rejected` 在**满载并行**的
   完整运行里偶发残留一次（单跑 10/10 干净）。处置：所有新守卫的 Drop 在 `remove_dir_all`/`remove_file`
   失败时重试（10×50ms，`NotFound` 立即返回），与 `app` 测试 `TempRoot` 的既有口径一致；此后
   `cargo test -p storage-sqlite` ×3 与 `cargo test --locked --workspace --all-features` ×3 的残留均为 0。

## 4. D2（inline 临时值）审计结论

- 基线 101 个 `temp_dir(...)` 调用点**全部**是 `let dir = temp_dir(..)` 形式（用
  `grep -rn 'temp_dir(' crates/storage-sqlite --include='*.rs' | grep -vE ':\s*let '` 复核，唯一命中是
  `absolute_path()` 的 `std::env::temp_dir()`，属无泄漏），因此不存在 `foo(temp_dir("x"))` 或
  `temp_dir("x").join(..)` 这类「语句结束即删目录」的写法，无需改写调用点。
- 新守卫的返回值一律绑定到命名局部变量；两处原本会把守卫当场丢弃的写法在本次一并改成绑定到用例作用域：
  `catalog.rs` 的 `dumping_profile("agent-good","good")`（原 `let (profile, _)`）与
  `dumping_profile("agent-broken","broken").0`（原直接放进 `vec![...]`）。
- **删除顺序**（D3）：`storage-sqlite` 的 `admin_audit.rs::open()` 改为返回 `(TempDir, SqliteStore)`，调用点
  统一写成 `let (_dir, store) = open(..)`，靠「局部变量逆序析构」保证连接池先于守卫释放；其余用例沿用
  `let dir = temp_dir(..)` 在前的既有顺序，并在需要时显式 `store.close().await`（见 §3.1）。

## 5. 未纳入本次改动、但已被登记的点

- `agent-host/tests/supervision.rs` 的三个固定文件名（`acpr-agent-host-env.txt` /
  `-heartbeat.txt` / `-heartbeat-terminate.txt`）保留原名：同一 binary 内各只有**一个**用例使用，
  不构成「并行用例共用固定名」（design D4 的豁免条件），改名会扩大 diff 而对清理没有收益。
- `app` 测试 `TempRoot` 的 Drop 未加 `NotFound` 提前返回（既有代码，未在本变更范围内）；
  它的持有者（含改用它的 `run_cli`）都不在守卫之前删除同一目录，因此不会触发 500ms 重试空转。
