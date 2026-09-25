# coder 报告 · WP3 任务 2.23（`*.revoke` 的重试语义对齐合同 §7）

- task_id: `2.23`（WP3 增量，补 2.11/2.12；`tasks.md` 第 2.23 条）
- role: coder
- phase: implement
- stage: work-package
- agent_context: 独立子 Agent（worker / coder 角色）；主 worktree 的分支 `feat/daemon-cli-and-local-admin`；只继承任务单 2.23 的文本与合同 `docs/LOCAL_ADMIN_PROTOCOL.md` §7（**未**改动它），不继承规划阶段对话；同一时刻只有本写入者编辑这两个源文件
- base_revision: **`d6b0182`**（`d6b0182d93e330e3405dfa0c16ce0a31c8c48b3c`，开工前 `git log --oneline -3` 与任务单一致）
- target_revision: **`0968c7aa4b5503bd39919fdafc7284974f577a2c`**（`0968c7a`，本任务的 `fix(server): 修正 *.revoke 的重试语义以符合 §7`；本报告随后以独立的 `docs(server)` 提交入库）
- scope: `crates/server/src/local_admin/router.rs`、`crates/server/src/local_admin/test_support.rs`（fake 保真修正）、`openspec/changes/daemon-cli-and-local-admin/reports/`（本报告 + 两个 `.log`，后者被 `.gitignore:27` 忽略，不入库）。**未改** `crates/storage-sqlite/**`、`docs/**`、`openspec/changes/**` 下除 `reports/` 外的任何文件（含 `plan.md`/`tasks.md`/`verification.md`/`specs/**`）、其它 crate、`compatibility/**`、`schemas/**`
- result: `PASS`（任务单列出的三项 cargo 检查与 `npm run check` 全部执行且全绿；**不代表**独立 review、集成或合并已完成）

## 问题与修复

合同 §7（`docs/LOCAL_ADMIN_PROTOCOL.md:590`）：「`export.create`、`import.add` 重试得到 `local.conflict`；`*.revoke`、`*.remove` 重试得到 `local.not_found`」。
任务单 2.23 的完成条件：三个 revoke 方法各自覆盖「首次成功 / 重复撤销 → `local.not_found` / 未知 id → `local.not_found`」。

已核实的存储侧事实（只读，未改）：三处撤销 SQL 都是 `UPDATE … SET … revoked_at = COALESCE(revoked_at, ?2) …`，因此**重复撤销会成功提交**，不会回 `NotFound`：

- `crates/storage-sqlite/src/admin/export.rs:400-404`
- `crates/storage-sqlite/src/admin/trust.rs:518-523`（device）
- `crates/storage-sqlite/src/admin/trust.rs:552-557`（node）

修复落在 `server` 侧（不改存储与合同）：三个入口先读目标记录，把「不存在」与「已是撤销终态」统一判为 `local.not_found`，并且**在调用用例之前**返回——不触发撤销提交、不触发 `close_device`/`close_node`。

### 三个方法的判定点（code 位置）

| 方法 | 前置读取 | 判定 | 提前返回位置 | 之后 |
| --- | --- | --- | --- | --- |
| `export.revoke` | `UseCases::export`（`core::use_cases.rs:640`） | `is_some_and(\|record\| !record.is_revoked())`（`ExportRecord::is_revoked` = `revoked_at.is_some()`，`core/src/model/export.rs:369`） | `router.rs:400-404` | `revoke_export` → 读回 `revokedAt` → `{exportId, revokedAt}`（形状未变） |
| `device.revoke` | `UseCases::device`（`core::use_cases.rs:414`） | `is_some_and(\|record\| record.revoked_at().is_none())`；`revoked_at` 与 `state = revoked` 由 `DeviceRecord::try_new` 绑定等价（`core/src/model/identity.rs:211`） | `router.rs:670-674` | `revoke_device` → `close_device` → 读回 → `{deviceId, revokedAt}`（形状未变） |
| `node.revoke` | `UseCases::nodes_for`（`core::use_cases.rs:478`） | `!rows.is_empty() && rows.iter().all(\|record\| record.revoked_at().is_none())`（§11.6：两种角色行同一事务一起进入 `revoked`） | `router.rs:907-914` | `revoke_node` → `close_node` → 读回全部角色行 → `{nodeId, revokedAt}`（形状未变） |

共用错误构造 `not_revocable()`（`router.rs:1169-1180`）：`local.not_found` + `"{method}: {entity} \`{id}\` does not exist or is already revoked"`。`message` 只含方法名、实体名与用户自己传入的 id，不含 secret/路径/SQL（§6）。

任务单要求的代码注释已落在四处：`not_revocable` 的文档注释说明 §7 依据与「不先判就会把重试当成功」，三个入口各自的文档注释说明读-写竞态无害（并发重复撤销最坏落到存储的幂等撤销，结果仍是同一条已撤销记录）。

## 测试

新增 2 个用例（各覆盖 ① 首次成功 ② 重复 → `local.not_found` ③ 未知 id → `local.not_found`）：

| 用例 | 位置 | 覆盖 |
| --- | --- | --- |
| `device_revoke_retry_and_unknown_id_report_not_found` | `router.rs:2529-2577` | ① `revokedAt` = 时钟读回值 + `close_device` 恰好一次；② NotFound + `message` 含 id + `closed_devices()` 仍为 1（重试未再走撤销与关闭）+ 存储中的 `revoked_at` 未被改写；③ NotFound + 仍不触发关闭 |
| `node_revoke_retry_and_unknown_id_report_not_found` | `router.rs:2580-2641` | 同上（`close_node` 口径；② 另断言角色行的 `revoked_at` 未变） |

`export.revoke` 的 ①②③ 已由既有用例 `export_list_includes_revoked_records_and_export_revoke_returns_the_persisted_time`（`router.rs:1800-1839`）覆盖，本次只把它②的断言从 `let (code, _)` 扩为 `let (code, message)` 并新增 `assert!(message.contains("export-1"))`；**未删除或弱化任何既有断言**。

### fake 保真修正（是本任务的关键，不是顺带清理）

只加 router 判定还不够：`FakeExports::revoke_export`（`test_support.rs:509`）原先对「已撤销」的 Export **直接回 `PortError::NotFound`**，与真实存储的 `COALESCE` 幂等语义**不一致**，使 export 的重试用例退化成永真断言（无论 router 怎么写都会绿）。已改为与存储同款：读回既有 `revoked_at` 并用 `COALESCE` 语义保留首次时间，重复撤销回 `Ok(())`。`FakeTrust::revoke_device`/`revoke_node` 本就是照抄存储的 COALESCE 语义（`test_support.rs:1222-1280`，其 doc 注释已声明该意图），**未改**。

先红后绿的证据（两个方向都取自实跑，非推断）：

1. 只改 fake、不加 router 判定 → `export_list_includes_revoked_records_and_export_revoke_returns_the_persisted_time ... FAILED`，`panicked: 期望失败响应，实际 {"exportId": "export-1", "revokedAt": "2026-09-18T09:12:03.412Z"}`（证明该用例不再永真）。
2. 加完 router 判定后，把三处前置判定临时短路成 `if false && !revocable {`（`node -e` 就地替换、跑完立刻从 `/tmp/router.rs.bak` 还原）→ 三个用例**全红**，均是同款 panic「期望失败响应，实际 `{…, "revokedAt": "2026-09-18T09:12:03.412Z"}`」。还原后 `git diff --stat` 与还原前一致。

## 检查记录（命令 / 退出码 / 日志）

| 检查 ID | 命令（目录 `D:/Project/acp-remote`） | 退出码 | 日志 |
| --- | --- | --- | --- |
| CT1 | `cargo fmt --all -- --check` | 0 | `reports/wp3-server-methods.log`（本轮块 (1)） |
| CT2 | `cargo clippy --locked -p server --all-targets --all-features -- -D warnings` | 0 | 同上，块 (2) |
| CT3 | `cargo test --locked -p server --all-features` | 0 | 同上，块 (3)/(3b) |
| CT4 | `npm run check`（Node v24.19.0 / npm 12.0.2，10 道门禁） | 0 | `reports/du1-pv1.log`（本轮块，含显式 `EXIT(npm run check)=0`） |
| CT5 | RED 证据（上述两个方向的实跑） | 3 个用例 FAILED（预期） | `reports/wp3-server-methods.log` 块 (0)/(0b) |

关键输出：`cargo test -p server` = lib **86**（原 84，+2）+ `local_admin_channel` 14 + `local_admin_schema_drift` 6 + 4 + `local_endpoint_unix` 0（Windows 上按设计为空）+ `local_endpoint_windows` 4 + doc-tests 0 = **114 项全过，0 failed**。`npm run check` 各道的判定文本与上一轮一致（`check:boundaries` 11 个 crate、`check:drift` §7 36 条 DDL / §5 15 个 trait 87 个方法签名、`check:agentic` `Totals: 13 passed, 0 failed`）。

## 未执行项

- 未执行 workspace 级 `cargo test`/`cargo clippy`：任务单只要求 `-p server`。`git commit` 时 `.husky/pre-commit` 会跑 `cargo fmt --check` + `npm run check` + workspace 级 `cargo clippy`（涉及 Rust 时），其结论以该钩子输出为准。
- 未执行 `cargo-deny`（`deps`/`advisories`）与 `gitleaks`（`secrets`）：只在 CI 运行，本机无对应二进制/网络；本任务未新增依赖、未引入凭据。
- 未执行 `openspec` 任务复选框勾选与 `specs/**` 同步：`tasks.md` 与 spec delta 属规划/候选资产，按任务单不在本 Agent 的写入范围内（spec delta `local-admin-methods/spec.md:118/122` 已经写着「`*.revoke` 重试得到 `local.not_found`」与「对已撤销的 Export 再次调用 `export.revoke` → `local.not_found`」，本实现正是让它成立，**无 spec 漂移**）。
- 未做跨 crate / 集成层验证：`server::local_admin` 的路由单测是任务单指定的验证面；`app::daemon` 组合根尚未装配该路由（`server` 侧 WP3 仍在增量中），因此没有可跑端到端入口。

## 待澄清 / 残余风险

1. **`*.remove`（`import.remove`）未在本次范围内**：§7 把 `*.remove` 与 `*.revoke` 并列要求重试得到 `local.not_found`。`import_remove` 走 `UseCases::remove_import` → 存储的 `remove_import` 用 `DELETE` + 行数判定（`test_support.rs:545-555` 同形），**天然**满足该语义，因此本次未加前置读取、也未新增用例。若主 Agent 认为仍需要一条显式的 `import.remove` 重试用例作为合同证据，请单独下任务（本 Agent 无该授权，未擅自扩范围）。
2. **`export.revoke` 的「已撤销」判定依赖 `ExportRecord::is_revoked()`（`revoked_at.is_some()`）**：ExportView 没有 `state` 字段（§5.5 只有 `revokedAt`），因此用的是任务单给出的两个判据中的前者。device/node 同理用 `revoked_at`（由 `try_new` 与 `state = revoked` 绑定等价）。
3. **读-写竞态**：两个并发的同 id `*.revoke` 可能都通过前置判定，第二个落到存储的幂等撤销并返回成功。§7 的语义针对**串行重试**（CLI 不自动重试、用户显式发起），任务单亦明确「读-写之间的竞态无害」。未加锁/未在事务内判定，以避免引入新的写入序列化点。
4. **`not_revocable` 的 `message` 对「已撤销」也写 `does not exist or is already revoked`**：CLI 侧只按 `code` 分支（§5.8），文案不参与判定。若主 Agent 希望「已撤销」与「未知」在文案上可区分（例如便于用户理解重试），需要新的合同口径——本次按 §7 的统一 `local.not_found` 处理，未自造细分码。

## 提交与交付对应

| 提交 | 类型 | 内容 |
| --- | --- | --- |
| `0968c7aa4b5503bd39919fdafc7284974f577a2c` | `fix(server)` | `crates/server/src/local_admin/router.rs`、`crates/server/src/local_admin/test_support.rs`、本报告（`.log` 被 `.gitignore:27` 忽略，不入库） |

提交用显式路径 `git add`（未 `git add -A`/`.`），未使用 `--no-verify`，未强推，未改写任何既有提交；提交后 `git status --porcelain` 为空（主 Agent 的并发改动若在提交后出现，不在本 Agent 的控制范围内，以本 Agent 的提交内容为准）。

## evidence_paths

- `openspec/changes/daemon-cli-and-local-admin/reports/wp3-server-methods.log`（CT1/CT2/CT3/CT5，本轮块含 `guards found: 3` 与三次 FAILED 的 RED 输出）
- `openspec/changes/daemon-cli-and-local-admin/reports/du1-pv1.log`（CT4，含 `EXIT(npm run check)=0`）
- `openspec/changes/daemon-cli-and-local-admin/reports/wp323-handoff.md`（本文件）

## 复核（交付提交后）

```text
$ git log --oneline -2
0968c7a fix(server): 修正 *.revoke 的重试语义以符合 §7
$ git show --stat --oneline 0968c7a
 crates/server/src/local_admin/router.rs       | 184 +++++++++++++++++++++++++-
 crates/server/src/local_admin/test_support.rs |  12 +-
 2 files changed, 188 insertions(+), 8 deletions(-)
$ git diff --stat HEAD -- crates            # 空输出：交付内容确实在 HEAD 里
$ git status --porcelain                     # 提交后为空（两个 `.log` 被 .gitignore:27 忽略）
```

`0968c7a` 经 `.husky/pre-commit`（`cargo fmt --check` + `npm run check` + workspace 级 `cargo clippy -- -D warnings`，输出 `pre-commit: 通过（3 步…）`）与 `commitlint` 通过；**未使用** `--no-verify`。

## resource_cleanup

- 无新增外部进程、无临时守护进程、无监听端口；未启动 Daemon 或 Agent。
- 临时文件 `/tmp/router.rs.bak`（RED 实验用）在还原后已无用，留在系统临时目录，不在仓库内；未在其中写入任何凭据。
- 未创建新分支、未改动 worktree 结构；工作树仅含本任务的两个源文件（提交后为空）。

## handoff_index

```yaml
handoff_index:
  - task_id: "2.23"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "0968c7aa4b5503bd39919fdafc7284974f577a2c"
    evidence_type: DELIVERY
    evidence_id: NOT_APPLICABLE
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp323-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "交付物 = router.rs 三处 §7 前置判定（export 400-404 / device 670-674 / node 907-914）+ 共用 not_revocable(1169-1180) + fake 保真修正（test_support.rs:509-533）+ 2 个新增用例（2529-2577、2580-2641）与 export 既有用例的 message 断言；未改 storage/docs/合同与规划文件。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.23"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "0968c7aa4b5503bd39919fdafc7284974f577a2c"
    evidence_type: CHECK
    evidence_id: CT1
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp323-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "cargo fmt --all -- --check 退出码 0（改动经 `cargo fmt --all` 后复跑）；日志 reports/wp3-server-methods.log 块 (1)。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.23"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "0968c7aa4b5503bd39919fdafc7284974f577a2c"
    evidence_type: CHECK
    evidence_id: CT2
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp323-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "cargo clippy --locked -p server --all-targets --all-features -- -D warnings 退出码 0，零告警（`if !revocable` 三处、`is_some_and`、block 表达式均无 lint）；日志同上块 (2)。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.23"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "0968c7aa4b5503bd39919fdafc7284974f577a2c"
    evidence_type: CHECK
    evidence_id: CT3
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp323-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "cargo test --locked -p server --all-features 退出码 0，114 项全过（lib 84 → 86）；新增 device/node 两个 revoke 重试用例均在 --list 清单里；日志同上块 (3)/(3b)。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.23"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "0968c7aa4b5503bd39919fdafc7284974f577a2c"
    evidence_type: CHECK
    evidence_id: CT4
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp323-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "npm run check 退出码 0（10 道合同门禁，Node v24.19.0）；日志 reports/du1-pv1.log 含显式 `EXIT(npm run check)=0` 行。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.23"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "0968c7aa4b5503bd39919fdafc7284974f577a2c"
    evidence_type: CHECK
    evidence_id: CT5
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp323-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "RED 双向证据：① 只改 fake 不加判定 → export 重试用例 FAILED；② 临时短路三处判定 → 三个 revoke 用例全 FAILED（`guards found: 3`），随后已还原。日志 reports/wp3-server-methods.log 块 (0)/(0b)。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.23"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: "0968c7aa4b5503bd39919fdafc7284974f577a2c"
    evidence_type: REVIEW
    evidence_id: NOT_APPLICABLE
    report_path: NOT_AVAILABLE
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "独立 review 尚未返回：须由不继承本实现对话的执行者检视「§7 判定的位置（用例之前、不返回成功、不触发关闭）」「fake 保真修正是否确实让 export 用例非永真（可复跑 (0b) 的 RED）」「未改 storage 与合同」「`*.remove` 被排除是否有据」。本报告不把自检当作其结论。"
    source_evidence: NOT_APPLICABLE
```
