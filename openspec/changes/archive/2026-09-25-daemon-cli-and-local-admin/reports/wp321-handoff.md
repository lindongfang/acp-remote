# coder 报告 · WP3 任务 2.21（`node.rotate-key.begin` 进入 v1 方法词表 + 方法名正则放宽）

- task_id: `2.21`（WP3 增量；契约与 Rust 枚举同批原子交付）
- role: coder
- phase: implement
- stage: work-package
- agent_context: 独立子 Agent（worker / coder 角色），主 worktree 的分支 `feat/daemon-cli-and-local-admin`；只继承任务单与 `verification.md` 2026-09-25 裁定项给出的契约输入（用户裁决选项 a：把已登记的连字符方法名纳入 v1 词表，§5.7 保持「字段定义落地前回 `local.unsupported`」），不继承规划阶段对话；同一时刻只有本写入者编辑这批文件
- base_revision: `122994f`（`122994f02edfee01c8e3298f526f93cb7f887e0c`；开工前 `git log --oneline -3` 核对与任务单一致）
- target_revision: **`1cd7a3b`**（`1cd7a3b76db9c286a0f0c06671051f1b43fffa3a`，含本任务的**全部**契约与代码改动）。**注意**：该提交由主 Agent 并发创建，提交信息为 `docs(server): 同步 2.21 契约口径到 spec delta 与证据记录`——主 Agent 在同一时刻把 `specs/local-admin-methods/spec.md` 与 `verification.md` 的同步 `git add` 进了同一个索引，随后 `git commit` 把索引里**已由本 Agent `git add` 的 12 个文件一并带走**（见「提交与交付对应」）。内容完整且已验证，但提交信息不描述实现改动
- 补充提交：`dfb1c99`（`fix(server)`：`vendor/windows-local-ipc/Cargo.toml` 的 RV1-WP2 两条 MINOR）
- scope（写入范围）：`schemas/local-admin/v1/envelope.schema.json`、`docs/LOCAL_ADMIN_PROTOCOL.md`、`scripts/check-command-catalog.mjs`、`fixtures/local-admin/v1/**`、`crates/server/src/local_admin/{method,envelope,router}.rs`、`crates/server/tests/{local_admin_schema_drift,local_admin_channel}.rs`、`vendor/windows-local-ipc/Cargo.toml`、`openspec/changes/daemon-cli-and-local-admin/reports/`。**未改** `compatibility/commands/v1/commands.json`、`docs/CONFIG_REFERENCE.md`、其它 crate、`openspec/changes/**` 下除 `reports/` 外的任何文件（含 `spec.md`/`plan.md`/`tasks.md`/`verification.md`）
- result: `PASS`（本任务单列出的检查全部执行且全绿；**不代表**独立 review、集成或合并已完成）

## 提交与交付对应

| 提交 | 类型 | 内容 |
| --- | --- | --- |
| `1cd7a3b`（主 Agent 并发提交，含本任务的 12 个文件） | `docs(server)` | 本任务的全部契约与代码改动（14 个文件，+188/−49），另含主 Agent 的 `specs/local-admin-methods/spec.md`（method 正则同步到新取值）与 `verification.md`（裁定项记录） |
| `dfb1c99` | `fix(server)` | `vendor/windows-local-ipc/Cargo.toml`（license + 依赖来源注释） |
| 本报告提交 | `docs(server)` | `reports/wp321-handoff.md`（本文件） |

`dfb1c99` 与「本报告提交」都经 `.husky/pre-commit`（`cargo fmt --check` + `npm run check` + `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`，输出见日志）与 `commitlint` 通过；**未使用** `--no-verify`，未强推，未改写任何已有提交（`1cd7a3b` 的提交信息与内容不匹配这一事实按「不回滚、不改写共享历史」处理，如实登记）。

## 最终契约取值（三处 + Rust 侧，逐字）

| 位置 | 取值 |
| --- | --- |
| `schemas/local-admin/v1/envelope.schema.json#/$defs/methodName.pattern` | `^[a-z][a-z0-9]*(\.[a-z0-9]+(-[a-z0-9]+)*)*$`（JSON 源里写作 `"^[a-z][a-z0-9]*(\\.[a-z0-9]+(-[a-z0-9]+)*)*$"`） |
| 同上 `enum` | 25 项；新增 `node.rotate-key.begin`，插在 `import.remove` 与 `audit.export` 之间（与 §5.1 能力表第 6 行 `local.node.rotate-key` 的行序一致；`Method::ALL` 的注释同步说明了这条排序依据） |
| `docs/LOCAL_ADMIN_PROTOCOL.md` §4 表格 `method` 行 | `^[a-z][a-z0-9]*(\.[a-z0-9]+(-[a-z0-9]+)*)*$`（附「段内允许连字符，段首/段尾不得为连字符」） |
| 同上 §6 的 `local.*` 错误码正则 | **保持旧值** `^[a-z][a-z0-9]*(\.[a-z0-9]+)*$`（错误码不含连字符，未放宽；任务单只要求 §4） |
| `scripts/check-command-catalog.mjs` 方法小节标题抽取 | `/^#### \`([a-z][a-z0-9]*(?:\.[a-z0-9]+(-[a-z0-9]+)*)*)\`/gm` |
| 同上 §5.1 表内方法名抽取 | `/\`([a-z][a-z0-9]*(?:\.[a-z0-9]+(-[a-z0-9]+)*)+)\`/g` |
| `crates/server/src/local_admin/method.rs::is_method_name` | 首段 `[a-z][a-z0-9]*`；后续段 `[a-z0-9]+(-[a-z0-9]+)*`（实现：按 `.` 切段，后续段再按 `-` 切分并要求每个子段非空且全为 `[a-z0-9]` → 段首/段尾连字符与连续连字符都非法） |
| `crates/server/tests/local_admin_schema_drift.rs` 的 pattern 字面量断言 | `Some("^[a-z][a-z0-9]*(\\.[a-z0-9]+(-[a-z0-9]+)*)*$")` |

判定边界（Rust 与 schema 同口径，测试逐条固定）：`node.rotate-key.begin` 合法；`node.-rotate-key.begin`、`node.rotate-key-.begin`、`node.rotate--key.begin`、`node.rotate-key.`、`node.rotate-key-`、`-node.rotate-key.begin`、`daemon-status`（首段不允许连字符）、`Daemon.status`、`daemon..status` 非法。

## §5.7 的文档结构（对任务单第 3 条的偏离，已由主 Agent 裁定批准）

任务单第 3 条的前提是「§5.7 标题已是 `#### \`method\``，只需放宽正则」。实测不成立：

```text
122994f 文档 + 放宽前正则：24 个方法小节
122994f 文档 + 放宽后正则：24 个方法小节（§5.7 是 `### 5.7 明确推迟：\`node.rotate-key.begin\``，h3 不被 `^#### ` 捕获）
122994f schema enum：24 项
HEAD 文档 + 放宽后正则：25 个方法小节          HEAD schema enum：25 项（两侧差集均为空）
```

因此本任务的落地包含一处文档结构改动（语义逐字未变）：`### 5.7 明确推迟（\`local.node.rotate-key\`）` + 新增 `#### \`node.rotate-key.begin\``，与既有 §5.6（`### 5.6 审计导出（\`local.audit.export\`）` + `#### \`audit.export\``）同形；门禁保持**一套**标题正则。三条 bullet 的正文与顺序未改（`git diff` 只显示标题行替换 + 空行 + 新 h4）。§5.1 能力表 `local.node.rotate-key` 行（`node.rotate-key.begin`（`post_mvp`，§5.7））与新 §5.7 一致：表内 25 个方法全部有对应小节（门禁的「表内方法必须有小节」断言通过）。

主 Agent 的裁定原话：批准保留该结构改动（理由：与 §5.6 的既有约定一致，方案 b 会让门禁长期维护两套解析规则），并要求保持三条 bullet 逐字不变；spec delta 的正则同步由主 Agent 负责（见 `1cd7a3b`）。

## 新增 fixtures 与 expectedKeyword

| fixture | manifest 声明 | Rust 侧期望（`local_admin_schema_drift.rs`） |
| --- | --- | --- |
| `valid/request-node-rotate-key-begin.json`（新增） | `valid: true`，无 `expectedKeyword` | `Expectation::RequestDecodes`：解码得 `Method::NodeRotateKeyBegin`、`params` 为 `{}` |
| `invalid/request-method-name-leading-hyphen.json`（新增，`method: node.-rotate-key.begin`） | `valid: false`，`expectedKeyword: "pattern"` | `Expectation::InvalidRequest`：`RequestDecodeError::InvalidRequest { MethodNameInvalid }` → `local.invalid_request` |
| `invalid/request-unknown-method.json`（既有，`method: device.rotate-key.begin`） | `valid: false`，`expectedKeyword: "enum"`（**未改**） | 期望由 `InvalidRequest` 改为新增的 `Expectation::UnsupportedMethod`（名字现在语法合法但不在集内 → `local.unsupported`，§4 规则 3）；断言读的是 `into_outcome()` 的响应码 `LocalErrorCode::Unsupported` |

`expectedKeyword` 的判定方式已先读 `scripts/check-schema-fixtures.mjs` 确认：它取该 fixture 的**全部** ajv 错误关键字集合，只要 `expectedKeyword` ∈ 集合即通过（不是「必须等于唯一关键字」）。新 invalid 用例同时命中 `pattern`（方法名段首连字符）与 `enum`（不在 25 项里），声明 `pattern` 是为了让失败原因指向正在验证的那条约束。

`fixtures/local-admin/v1/README.md` 的两条 bullet 同步登记了新覆盖（valid 新增连字符方法名请求；invalid 新增「方法名段首连字符」）。

## Rust 侧实现

- `Method::NodeRotateKeyBegin` 加入枚举、`ALL`（`[Self; 25]`）与 `as_str()`（`"node.rotate-key.begin"`）；`from_name` 由 `ALL` 线性查找，自动生效。
- `LocalAdminRouter::dispatch` 显式接上该方法：按 §5.7 直接 `AdminError::unsupported_method(Method::NodeRotateKeyBegin)`（`method node.rotate-key.begin is not implemented yet`），**不发明** `params`/`result`；设备/节点配对族的通配分支注释相应收窄（`§5.7` 从该分支移出）。
- 测试口径三处固定：`envelope.rs`（新增 `hyphenated_names_inside_the_set_decode`、`hyphen_at_a_segment_boundary_is_a_naming_error`，并把 `syntactically_valid_names_outside_the_set_are_unsupported` 扩到 `daemon.doctor` + `device.rotate-key.begin`）、`router.rs`（`unimplemented_methods_answer_local_unsupported` 增加 `NodeRotateKeyBegin`）、`local_admin_channel.rs`（`rejected_requests_keep_the_connection_usable`：`node.rotate-key.begin` 期望从 `local.invalid_request` 改为 `local.unsupported`，并新增 `node.-rotate-key.begin` → `local.invalid_request`）。
- `method.rs` 的单测：`ALL.len()` 24 → 25；合法集合新增 `node.rotate-key.begin`/`a.b-c`/`x.y-z.w-0`；非法集合删去 `daemon.sta-tus`（现在合法），新增 7 个连字符边界用例。

## 检查记录（命令 / 退出码 / 日志）

| 检查 ID | 命令（目录 `D:/Project/acp-remote`） | 退出码 | 日志 |
| --- | --- | --- | --- |
| CT1 | `npm run check`（Node v24.19.0：`check:schemas`/`commands`/`errors`/`features`/`assets`/`acp`/`docs`/`boundaries`/`drift`/`agentic` 共 10 道） | 0 | `reports/wp321-contract.log`（含 `NPM_CHECK_EXIT=0`） |
| CT2 | `cargo test --locked -p server --all-features` | 0 | `reports/wp3-server-methods.log`（本轮块，`exit(...)=0`） |
| CT3 | `cargo fmt --all -- --check` | 0 | `reports/wp3-server-methods.log`（同轮块） |
| CT4 | `cargo clippy --locked -p server --all-targets --all-features -- -D warnings` | 0 | `reports/wp3-server-methods.log`（同轮块） |
| CT5 | 交付提交后复核：`git diff --stat HEAD -- schemas docs scripts fixtures crates`（空）+ `npm run check` | 0 | `reports/wp321-contract.log` 末尾「交付提交后复核」 |

关键输出：

- `check:schemas`：`schema fixtures OK: 118 valid, 24 invalid (ajv Draft 2020-12), 39 event views bound`（改动前为 `117 valid, 23 invalid`，即本次净增 1 valid + 1 invalid，见同目录 `baseline-npm-check.log`）。
- `check:commands`：`command catalog OK: 12 commands`（方法小节集合 = 25、§6 错误码集合、`localCapabilities` 三处比对全过）。
- `check:docs`：`doc links OK: 374 relative links, 4302 section refs`。
- `check:drift`/`check:boundaries`/`check:agentic`：均 OK；`check:agentic` 的 `Installation: PASS`、`Totals: 13 passed, 0 failed`。
- `cargo test -p server`：`65`（lib 单测）+ `14`（`local_admin_channel`）+ `6`（`local_admin_schema_drift`）+ `4` + `0`（`local_endpoint_unix`，Windows 上按设计为空）+ `4` + doc-tests `0` = **93 项全过，0 failed**。
- drift 项数：`local_admin_schema_drift.rs` 仍为 **6 个测试函数**；其中 `FIXTURES` 表 **11 → 13 条**（与 `manifest.json` 逐条比对并通过），`method_set_matches_the_schema_enum` 逐项比对 **25 项**，pattern 字面量断言同步到新取值。

## 未执行项

- 未执行 workspace 级 `cargo test`/`cargo clippy`（任务单只要求 `-p server`；`dfb1c99` 的 pre-commit 钩子跑了 workspace 级 `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`，通过，输出在该提交的钩子日志里）。
- 未执行 `cargo-deny`（`deps`/`advisories`）与 `gitleaks`（`secrets`）：只在 CI 运行，本机无对应二进制/网络；`license = "Apache-2.0"` 的改动**不会**因此被本地验证，判定依据是 `deny.toml` 的 allow 列表同时含 `Apache-2.0` 与 `MIT`（读文件确认），且该 crate `publish = false`。
- 未改动、未验证 `openspec/changes/**/specs/**`：按裁定由主 Agent 同步（已在 `1cd7a3b` 内）；`openspec validate` 在 `npm run check` 的 agentic 门里已重新跑过并通过。

## 已知限制 / 残余风险

1. **提交历史与内容不匹配**（非本任务缺陷，但影响验收证据链）：`1cd7a3b` 的信息写的是 `docs(server)` 且只描述 spec delta 同步，实际含本任务的 12 个实现/契约文件。若要纠正，需要主 Agent 决定是否在候选版本里补一条说明（本 Agent 无改写共享历史的授权，未做 force/rebase）。
2. **首段仍不允许连字符**：新正则沿用裁定记录里逐个字符给出的取值（首段 `[a-z][a-z0-9]*`），因此 `rotate-key.begin` 这类名字仍非法；这是与旧正则一致的既有不对称（旧正则的首段同样比后续段更严）。已写进 §4 表格说明、`method.rs` 的文档注释与测试。
3. **本地管理通道的 `local.unsupported` 会回显方法名**（`method … is not implemented yet`）：这是 WP3a 既有口径（§6 允许方法名出现在 `message`，不含 secret），本次未改，只是 `node.rotate-key.begin` 现在真的走到这条路径。
4. `schemas/local-admin/v1/README.md` 末尾仍写着 `check-local-admin`（`scripts/check-local-admin-contract.mjs`）——该脚本在本仓库**不存在**（实际断言在 `scripts/check-command-catalog.mjs`，见 `wp3a-handoff.md` 同类记录）。属既有文档不准确，**不在本任务范围**，未改，留给主 Agent 决定。
5. 新 fixture 的 `expectedKeyword: "pattern"` 依赖 ajv 同时上报 `pattern` 与 `enum` 两个关键字（当前行为）。若将来有人把 `enum` 从 `methodName` 移除或改成 `not` 形式，该声明会失效——`check:schemas` 会立刻报错，不会静默漂移。

## handoff_index

```yaml
handoff_index:
  - task_id: "2.21"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "1cd7a3b"
    evidence_type: DELIVERY
    evidence_id: NOT_APPLICABLE
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp321-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "交付物 = 1cd7a3b 里本任务的 12 个文件（schema pattern/enum 25 项、docs §4/§5.7/§8、门禁脚本两处正则、2 条新增 fixture + manifest + README、Method::NodeRotateKeyBegin 与路由/测试口径）+ dfb1c99（vendor manifest 两条 MINOR）；criterion 见本报告「最终契约取值」与「检查记录」。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.21"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "1cd7a3b"
    evidence_type: CHECK
    evidence_id: CT1
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp321-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "npm run check 退出码 0（10 道门禁）；check:schemas 118 valid/24 invalid、check:commands 方法小节↔enum↔§5.1 表三处比对通过；日志 reports/wp321-contract.log。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.21"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "1cd7a3b"
    evidence_type: CHECK
    evidence_id: CT2
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp321-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "cargo test --locked -p server --all-features 退出码 0，93 项全过（含 local_admin_schema_drift 6 项、FIXTURES 13 条与 25 项方法集比对）；日志 reports/wp3-server-methods.log 本轮块。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.21"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "1cd7a3b"
    evidence_type: CHECK
    evidence_id: CT3
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp321-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "cargo fmt --all -- --check 退出码 0（首次运行报出 2 处需要格式化，已 `cargo fmt --all` 后复跑为 0）；日志同上。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.21"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "1cd7a3b"
    evidence_type: CHECK
    evidence_id: CT4
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp321-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "cargo clippy --locked -p server --all-targets --all-features -- -D warnings 退出码 0，零告警；日志同上。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.21"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "1cd7a3b"
    evidence_type: CHECK
    evidence_id: CT5
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp321-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "交付提交后复核：`git diff --stat HEAD -- schemas docs scripts fixtures crates` 空输出（交付内容确实在 HEAD 里）+ `npm run check` 退出码 0；日志 reports/wp321-contract.log 末尾块。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.21"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: "1cd7a3b"
    evidence_type: REVIEW
    evidence_id: NOT_APPLICABLE
    report_path: NOT_AVAILABLE
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "独立 review 尚未返回：须由不继承本实现对话的执行者检视 §5.7 的文档结构改动（h4 新增、bullet 逐字未改）、新正则的首段不对称、fixture 的 expectedKeyword 选择与「枚举/路由/测试三处口径一致」。本报告不把自检当作其结论。"
    source_evidence: NOT_APPLICABLE
```
