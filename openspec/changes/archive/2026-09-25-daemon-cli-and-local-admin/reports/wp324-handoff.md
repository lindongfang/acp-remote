# coder 报告 · WP3 任务 2.24（收口 RV1-WP3 的四项实现侧发现 F1/F2/F3/F6）

- task_id: `2.24`（WP3 增量；`tasks.md` 第 2.24 条）
- role: coder（worker）
- phase: implement
- stage: work-package
- agent_context: 独立子 Agent（worker / coder 角色）；同一分支 `feat/daemon-cli-and-local-admin` 的主 worktree；只继承任务单 2.24 的文本与 `reports/rv1-wp3.md` 的 F1/F2/F3/F6 条目，不继承 WP3 实现对话与 review 对话；同一时刻只有本写入者编辑 `crates/server/**`（另一工作包同时段在 `crates/storage-sqlite/**` 与 `docs/**` 上提交，见「并发与版本」）
- base_revision: **`1286eeb`**（开工前 `git log --oneline -3` 的 HEAD，与任务单声明一致）
- target_revision: **`cb983c2`**（`fix(server): 收口 RV1-WP3 的 F1/F2/F3/F6`，5 文件 / +181 −22）；其直接前置是 **`d5d1ec8`**（另一工作包的 `docs(storage)` 提交，不触碰 `crates/server/**`）——本任务的全部 cargo/npm 检查跑在 `d5d1ec8 + 本任务改动` 的同一内容上，该内容与 `cb983c2` 逐字相同
- scope: `crates/server/src/local_admin/{router,test_support,params,envelope}.rs`、`crates/server/src/transport/local/platform/windows.rs`、`openspec/changes/daemon-cli-and-local-admin/reports/`（本报告 + 两个 `.log`）。**未改** `docs/**`、`openspec/changes/**`（除 `reports/` 下本任务产物）、`schemas/**`、`fixtures/**`、`compatibility/**`、`crates/storage-sqlite/**`、其它 crate、根 `Cargo.toml`/`Cargo.lock`
- result: `PASS`（四项发现均已修；任务单列出的三条 cargo 检查与 `npm run check` 全部执行且全绿；F2、F3 各有实跑 RED 反例证据；`git status --porcelain` 只剩两个非本任务产物的未跟踪文件，见「工作树」）——**不代表**独立 review、集成或合并已完成

## 并发与版本

开工时 HEAD = `1286eeb`（工作树另有 `crates/storage-sqlite/tests/admin_audit.rs` 的在途改动，不在本任务范围）。本任务进行中，另一工作包在该路径上依次提交了 `fe093b3` / `9058ae4` / `64823a1` / `d5d1ec8`，**均不触碰 `crates/server/**`**，因此本任务的改动与检查不受影响；`base_revision` 仍按开工值登记，检查所依据的内容 = `d5d1ec8` 的工作树 + 本任务 5 个文件 = `cb983c2`。

提交时只 `git add` 了下列 5 个显式路径，未使用 `git add -A`/`.`：

```text
crates/server/src/local_admin/envelope.rs
crates/server/src/local_admin/params.rs
crates/server/src/local_admin/router.rs
crates/server/src/local_admin/test_support.rs
crates/server/src/transport/local/platform/windows.rs
```

## 四项发现的修复

### F1 · `import.remove` 未知 id 的常驻断言（`crates/server/src/local_admin/router.rs:1908-1918`）

既有用例 `import_add_is_always_unavailable_and_import_remove_maps_not_found`（`router.rs:1845` 起）在「先删成功再删 → `local.not_found`」之后，**新增一条独立输入路径**的断言（`router.rs:1908-1918`）：对**从未存在过的合法形状** id `"import-missing"` 调 `import.remove` → `local.not_found`。两条路径（存储里曾有过该行 / 从来不曾有过）刻意保留为两段独立断言，没有合并成一条。

关键代码路径：`Router::import_remove`（`router.rs:461`）→ `params::import_remove`（形状校验）→ `UseCases::remove_import`（`Actor::LocalCli`）→ `FakeExports::remove_import`（`test_support.rs:550`：`HashMap::remove` 命中 `Ok`、未命中 `PortError::NotFound(EntityRef::Import)`）→ `params::map_port_error` → `LocalErrorCode::NotFound`。

### F2 · `FakeExports::put_export` 与真实存储同形（`crates/server/src/local_admin/test_support.rs:493-514`）

fake 原先对**任何已存在 key** 返回 `Conflict(AlreadyExists)`，比真实存储更严；真实存储（`crates/storage-sqlite/src/admin/export.rs:321-380`）只有**已撤销行**回 `Conflict`，未撤销的既有 `export_id` 走 `ON CONFLICT DO UPDATE` 覆盖并 `Ok`。这会让 `Router::export_create` 的查重 guard（`router.rs:327-338`，§5.5「`exportId` 已存在 → `local.conflict`」在生产上的**唯一**保障）失去反例能力。

改为与存储同形：取既有记录，`revoked_at().is_some()` 才 `Conflict(AlreadyExists)`，否则 `seed_export`（`HashMap::insert`，等价覆盖）后 `Ok(())`。文档注释显式写明「这一点是测试强度的前提」。

**RED 证据（实跑，非推断）**：临时把 guard 短路成 `if false && self.deps.core.export(...)…is_some() {`，只跑 `export_create_requires_registered_aliases_and_a_free_id`：

```text
test local_admin::router::tests::export_create_requires_registered_aliases_and_a_free_id ... FAILED
thread ... panicked at crates\server\src\local_admin\router.rs:1267:49（行号取自临时短路版本；恢复后同一 panic 在 `router.rs:1265`，属测试助手 `error_of` 的「期望失败响应」分支）:
期望失败响应，实际 {"export": Object {… "exportId": String("export-1") …}}
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 87 filtered out
exit=101
```

失败点正是用例的「同一个 exportId 再次创建 → `local.conflict`」断言（`router.rs:1778-1779`）：guard 缺席时第二次 create **静默覆盖并返回成功 result**，与 review 预言的退化路径逐字一致。恢复 guard 后同一条用例 `exit=0`，随后重跑全量 `cargo test -p server --all-features` `exit=0`（`reports/wp3-server-methods.log` 块 `(2.24-c)` / `(2.24-c2)` / `(2.24-e)`）。

**未受影响说明**：本改动只放宽 fake 的一处判定，不删除任何断言；全量 88 项 lib 用例在该改动后全绿，说明没有其他用例依赖旧的「任何重复都 Conflict」语义。

### F3 · Windows `accept` 的错误路径不再让 endpoint 永久不可用（`crates/server/src/transport/local/platform/windows.rs:66-159`）

原实现先 `self.pending.take()`，随后 `connect().await` 与「补建下一实例的 `create(...)`」各自 `?`——任一返回 `Err` 都会让 `pending` 停在 `None`，此后每次 `accept()` 只回 `Err(Os{…})`，与模块头注释声明的不变量冲突。

修复把这一步拆成可重试的三段：

| 项 | 位置 | 作用 |
| --- | --- | --- |
| `take_pending` | `windows.rs:115-124` | 取出待连接实例；`pending` 为空时**现建一个**（上一次补建失败后的重试点），因此不再有「没有可用的 pipe 实例」这条永久失败路径 |
| `settle_accept` | `windows.rs:126-145` | 收尾一步：**先** `refill_pending()` 再匹配 `(connected, refilled)`；`connect` 失败时也先补建、再回 `Err(Os{connect 成因})`；补建自身失败时上报该错误，`pending` 保持为空由下一次 `take_pending` 重试 |
| `refill_pending` | `windows.rs:147-151` | 补建同名的下一个实例（单一来源，供 `settle_accept` 使用） |
| `create_pipe_instance` | `windows.rs:154-159` | 唯一的「创建同名后续实例」来源（`ServerOptions::new().create(pipe_name)`），`take_pending` 与 `refill_pending` 共用 |

凭据拒绝路径现状不变：补建发生在 `client_user_sid` 判定**之前**，`PeerRejected` 仍是非致命、`pending` 已填好。`accept` 的文档注释保留了「`PeerRejected` 非致命、其他错误表示 endpoint 不可用」的语义，模块头注释新增了该不变量的明文条目。

**测试方式（可测性结论 + 替代断言）**：真实 Named Pipe 上**无法**稳定注入 `connect` 失败——本机实测与依赖源码核对一致：`mio-1.2.3` 的 `connect_overlapped`（`src/sys/windows/named_pipe.rs:151-164`）把 `ERROR_PIPE_CONNECTED` 与 `ERROR_NO_DATA` 都当成功返回，只有真正的系统调用错误才落到 `Err`（mio 文档原话：「Normal I/O errors from the call to `ConnectNamedPipe` are returned immediately」）。实测探针：先 `ClientOptions::open()` 再立刻 drop 客户端，`accept()` 仍返回 `Ok`（`connect` 未失败），因此「客户端连上后立刻断开」不足以触发该路径。

据此按任务单允许的方式改用**可注入的构造点**：`settle_accept` 是 `accept` 里唯一以 `io::Result` 形式接收 connect 结果的一步，把它抽成独立方法后新增单元用例 `transport::local::platform::windows::tests::a_failed_connect_leaves_the_endpoint_usable`（`windows.rs:173-222`，`#[cfg(test)]`，用例函数 194-221，Windows 目标）：

1. 真实 `LocalEndpoint::bind` + `take_pending()` 取出首实例；
2. 以 `settle_accept(Err(io::Error::other("模拟 ConnectNamedPipe 立即失败")), server)` 注入失败；断言返回 `Err(Os{..})` **且 `endpoint.pending.is_some()`（补建被调用、pending 非空）**；
3. 再开一个真实 `ClientOptions` 客户端并断言 `endpoint.accept().await.is_ok()`——即失败一次之后 endpoint 仍能接受真实连接（这一条是本用例对「endpoint 永久不可用」的直接反例）。

**RED 证据（实跑）**：临时把 `settle_accept` 恢复成旧行为（`connect` 失败直接返回、不补建），只跑该用例：

```text
test transport::local::platform::windows::tests::a_failed_connect_leaves_the_endpoint_usable ... FAILED
thread ... panicked at crates\server\src\transport\local\platform\windows.rs:212:9（行号取自临时短路版本）:
connect 失败后必须已补建下一个 pipe 实例
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 87 filtered out
exit=101
```

恢复后全量 `cargo test -p server --all-features` `exit=0`（日志块 `(2.24-d)` / `(2.24-e)`）。**未**用注释、`#[ignore]` 或跳过代替断言。

### F6-① · `ProviderConfigure` 手写 `Debug`，不再可能打印明文凭据（`crates/server/src/local_admin/params.rs:315-348`）

`values: Vec<(String, String)>` 持有明文凭据，原 `#[derive(Debug, …)]` 会被未来任一句 `tracing::debug!(?configure)` 写进日志。改为 `#[derive(Clone, PartialEq, Eq)]` + 手写 `impl std::fmt::Debug`：只输出 `provider_id` / `kind` / `display_name` 与**字段名列表**（`credential_fields`），值一律不打印。既有的 trait 使用点（`assert_eq!(configure.values, …)`、字段访问）不变。

新增单元用例 `local_admin::params::tests::provider_configure_debug_prints_credential_field_names_but_never_values`（`params.rs:1256-1280`，用例函数 1260 起）：断言 `{:?}` 同时含 `api_key`、`org`、`openai.primary`，且**不含** `s3cret`、`acme`（两个凭据值）。

### F6-② · 清理两处不可达的枚举分支（`crates/server/src/local_admin/envelope.rs:249-266`）

`InvalidRequestReason::message()` 里 `MissingField{FIELD_ID}` 与 `MissingField{FIELD_PARAMS}` 两个具体分支不可达（`decode_request` 对 `id` 走 `IdNotCanonicalUuid`、对 `params` 非 object 走 `ParamsNotObject`）。按任务单允许的第二种方式处理：删除两个不可达分支，保留唯一可达的 `field: FIELD_METHOD` 分支，并把原先的 params 兜底文案改成中性的 `"request field is missing or is not a string"`，同时加注释说明可达性推理与兜底用意。`as_str()` 的 `missing_or_illtyped_field` 与 `RequestDecodeError` 形状未变；没有测试或 fixture 断言过被删除的两条文案（已在 `crates/server/**`、`tests/`、`fixtures/`、`schemas/`、`docs/` 全量检索确认）。

## 检查记录（命令 / 退出码 / 日志）

全部命令在 Windows 本机、工作树内容 = `cb983c2` 上执行；`cargo` 工具链由 `rust-toolchain.toml` 固定（1.98.1）。

| # | 命令 | 退出码 | 日志（块） |
| --- | --- | --- | --- |
| 1 | `cargo fmt --all -- --check` | `0` | `reports/wp3-server-methods.log` 块 `(2.24-a)`（显式 `EXIT(cargo fmt --all -- --check)=0`） |
| 2 | `cargo clippy --locked -p server --all-targets --all-features -- -D warnings` | `0` | 同上 块 `(2.24-b)`（显式 `EXIT(...)=0`） |
| 3 | `cargo test --locked -p server --all-features` | `0` | 同上 块 `(2.24-e)`（显式 `EXIT(...)=0`；lib 88 项 + 集成 14/6/4/4 项全过，7 个 `test result: ok`，无 FAILED） |
| 4 | `npm run check` | `0` | `reports/du1-pv1.log` 末段（显式 `EXIT(npm run check)=0`；Node v24.19.0，10 道合同门禁全绿，含 `check:docs`/`check:boundaries`/`check:drift`/`check:agentic`） |
| 5 | `rg -n "unsafe" crates/server/src` | `1`（无匹配） | `reports/wp3-server-methods.log` 块 `(2.24-f)`（显式 `EXIT(…)=1（1 = 无匹配）`） |

补充证据（非任务单要求，但在同一次运行中取得）：

- 提交时 `.husky/pre-commit`（3 步：`cargo fmt --check` → `npm run check` → `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`）在 `cb983c2` 上通过，输出含 `pre-commit: 通过（3 步…）`；本机未安装 gitleaks，本地密钥扫描被跳过，CI `secrets` job 仍会判定（本地无等价物）。
- 新增/受影响的用例在最终全量日志里的可见行：`transport::local::platform::windows::tests::a_failed_connect_leaves_the_endpoint_usable ... ok`（块 `(2.24-e)` 第 92 行）、`local_admin::params::tests::provider_configure_debug_prints_credential_field_names_but_never_values ... ok`、`local_admin::router::tests::import_add_is_always_unavailable_and_import_remove_maps_not_found ... ok`、`local_admin::router::tests::export_create_requires_registered_aliases_and_a_free_id ... ok`。
- Windows 真实 Named Pipe 集成用例（`crates/server/tests/local_endpoint_windows.rs`，4 项）在本次全量运行中仍全绿，说明 F3 的重构未改变「同用户被接受」「同名后续实例可创建」「同名第二个首实例 `AlreadyInUse`」这些既有行为。

## 未执行 / 未验证（明确声明）

1. **`cargo deny` 与 `gitleaks` 未在本地执行**：两者只在 CI 运行（`AGENTS.md` §8），本地无等价物；本报告不声称其通过。
2. **Linux/Unix 路径未在本机执行**：本次改动不触碰 `platform/unix.rs`，但 `cargo test -p server` 在 Windows 上只编译 Windows 目标（`crates/server/tests/local_endpoint_unix.rs` 本次 `running 0 tests`）；Unix 侧行为未复核。
3. **跨用户拒绝、Windows 后续实例 DACL 继承**仍不可执行（与 RV1-WP3 报告同一限制），本次未新增结论。
4. **未做端到端/组合根验证**：`crates/app` 尚不存在，本版本无端到端入口，`accept` 的调用方（Daemon 主循环）如何处理 `Err(Os{..})` 不在本切片范围内。
5. **未跑 workspace 全量测试**：任务单指定的是 `-p server`；`cargo clippy --workspace` 在提交钩子里跑过（通过），但 `cargo test --workspace` 未跑（另一工作包的 `storage-sqlite` 改动同时段进行，跨包全量不由本任务负责）。

## 残余风险 / 待澄清

1. **fake 与存储在「传入已撤销记录」上的剩余差异**：真实 `put_export` 对「传入记录带 `revoked_at`」回 `PortError::InvalidRequest`，fake 仍回 `Ok`。该输入在当前 `server` 里不可达（`params::export_create` 只构造 `revoked_at = None` 的记录，且 `revokedAt` 是未知字段），因此未在本次一并建模；若未来 `server` 侧新增「按记录写入」的入口，需要同步补这一条。
2. **`connect()` 的真实失败仍无端到端覆盖**：本机无法构造（依据见 F3 段），新用例覆盖的是「失败之后不变量仍成立」这一决策点，不是 `ConnectNamedPipe` 的真实错误码路径。若后续要在 Linux/Windows CI 上补真实注入，需要 mio/tokio 层面的可注入点，属新决策。
3. **`take_pending` 的自愈会掩盖「补建长期失败」**：连续补建失败时每次 `accept` 只报错而不退出，属刻意选择（失败关闭、不透传连接），但若组合根把 `Err(Os{..})` 当致命错误就会退出——与 RV1-WP3 报告 F3 的处置一致，未新增强制策略。

## 工作树

提交后 `git status --porcelain` 只剩两个**非本任务产物**的未跟踪文件：`openspec/changes/daemon-cli-and-local-admin/reports/rv1-wp3.md`（review 报告，由主 Agent 产出）与本报告 `…/wp324-handoff.md`（按既有惯例由独立 `docs(server): 登记 … 交接报告` 提交入库；`.log` 被 `.gitignore:27` 忽略）。本任务改动的 5 个源文件在 `cb983c2` 中无残留未提交改动，`git diff cb983c2 --stat` 为空。

```yaml
handoff_index:
  - task_id: "2.24"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "cb983c2"
    evidence_type: CHECK
    evidence_id: CT1
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp324-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "cargo fmt --all -- --check 退出码 0（工作树内容 = cb983c2）；日志 reports/wp3-server-methods.log 块 (2.24-a) 含显式 `EXIT(cargo fmt --all -- --check)=0`。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.24"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "cb983c2"
    evidence_type: CHECK
    evidence_id: CT2
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp324-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "cargo clippy --locked -p server --all-targets --all-features -- -D warnings 退出码 0（含 F3 新增的 #[cfg(test)] 单元模块与 F6① 的手写 Debug）；日志 block (2.24-b) 含显式 EXIT=0；提交钩子的 workspace 版 clippy 同样通过。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.24"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "cb983c2"
    evidence_type: CHECK
    evidence_id: CT3
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp324-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "cargo test --locked -p server --all-features 退出码 0；7 个 test result 全 ok（lib 88 项、集成 14/6/4/0/4/0），无 ignored、无 should_panic；新用例 a_failed_connect_leaves_the_endpoint_usable、provider_configure_debug_prints_credential_field_names_but_never_values 与 F1/F2 触及的 import.remove、export.create 用例全部 ok（日志块 (2.24-e)）。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.24"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "cb983c2"
    evidence_type: CHECK
    evidence_id: CT4
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp324-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "npm run check 退出码 0（10 道合同门禁，Node v24.19.0/npm 12.0.2）；日志 reports/du1-pv1.log 末段含显式 `EXIT(npm run check)=0`，运行内容与 cb983c2 逐字相同。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.24"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "cb983c2"
    evidence_type: CHECK
    evidence_id: CT5
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp324-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "F2 反例能力实跑双向：短路 export.create 查重 guard → export_create_requires_registered_aliases_and_a_free_id FAILED（panic「期望失败响应，实际 {export…}」，exit=101）；恢复 guard → 同用例 ok（exit=0）。F3 反例能力实跑：settle_accept 恢复旧行为 → a_failed_connect_leaves_the_endpoint_usable FAILED（panic「connect 失败后必须已补建下一个 pipe 实例」，exit=101）；恢复后全量 exit=0。日志 reports/wp3-server-methods.log 块 (2.24-c)/(2.24-c2)/(2.24-d)/(2.24-e)。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.24"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "cb983c2"
    evidence_type: CHECK
    evidence_id: CT6
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp324-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "工程不变量自检：`rg -n unsafe crates/server/src` 零命中（rg 退出码 1，日志块 (2.24-f)）；新增代码无 unwrap/expect/panic/unreachable/dbg!/println!/todo!/#[ignore]（F3 的不可达「没有可用的 pipe 实例」分支已由 take_pending 的自愈语义删除，未引入新的不可达分支）。"
    source_evidence: NOT_APPLICABLE
```
