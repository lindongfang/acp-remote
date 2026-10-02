# WP2 交付报告（封闭词表原子点 + node-link wire）

```yaml
task_id: "2.2"
role: coder
phase: implement
agent_context: "coder-B：独立子 Agent，隔离上下文（不继承 main 的实现对话），只读主仓库的冻结契约（绝对路径），只写本 worktree 的 WP2 写入范围"
target_revision: "32f71f5d5f08145cfd3e3bddd7d6f6a46a0a5992"
base_revision: "81e350ff340014265eb7c9251237c799d4357fee（分支 agentic/session-resume-wp2 的基线）"
scope: "WP2：crates/node-link-protocol/、crates/identity-auth/、crates/core/src/broker.rs（仅 required_grant 一条臂）、compatibility/commands/v1/commands.json、schemas/node-link/v1/、fixtures/node-link/v1/、docs/NODE_LINK_PROTOCOL.md、docs/SYNC_PROTOCOL.md、docs/SECURITY_DESIGN.md、scripts/check-command-catalog.mjs、AGENTS.md、README.md、.github/workflows/ci.yml"
input_revisions:
  design: "openspec/changes/session-resume/design.md@D1, D5, D6（D6 = 本轮 DR1 新增的门禁 pack 规则决策）"
  specs: "openspec/changes/session-resume/specs/node-link-owner-server/spec.md@design-rev-4（R27/R34/R36/R37 中与 wire 契约有关的部分）"
  plan: "openspec/changes/session-resume/plan.md（Work Packages 的 WP2 行、Shared File Ownership、Dependency Handoffs 的红窗口段）"
result: PASS
evidence_paths:
  - "D:/Project/acp-remote/openspec/changes/session-resume/reports/wp2-coder-PV1.log"
  - "D:/Project/acp-remote/openspec/changes/session-resume/reports/wp2-coder-PV2.log"
  - "D:/Project/acp-remote/openspec/changes/session-resume/reports/wp2-coder-red-window.log"
resource_cleanup: "只使用 provisioner 指定的 CARGO_TARGET_DIR（D:/Project/acp-remote-target/session-resume-wp2），未在 worktree 内产生 target/ 或其他构建产物；未创建/占用 SQLite 文件；命令结束时无后台进程残留；worktree 工作区在提交后为 clean"
```

## 1. 改动文件清单（21 个，全部在 WP2 写入范围内）

| 文件 | 改动 |
| --- | --- |
| `crates/node-link-protocol/src/command.rs` | 新增 `CommandName::SessionResume`（12→13，`ALL`/`as_str`/`FromStr`/需会话范围且 `expectedVersion` 为 null）、payload 类型 `SessionResume {}`、`CommandPayload::SessionResume` 变体与 `command_name`/`from_raw` 分支、`CommandResultPayload::SessionResume(SessionResumeResult)` 与 `from_terminal` 的 `completed` 类型化解析；同步「12 条」等计数文档注释；新增内联 `#[cfg(test)]` 单测 3 个 |
| `crates/node-link-protocol/src/common.rs` | 新增 `SessionResumeResult`（`remoteSessionRef` + `sessionMeta`，与 `SessionCreateResult` 同形） |
| `crates/node-link-protocol/tests/envelope_fixtures.rs` | **既有测试文件的计数常量**随新增 fixture 更新：`EXPECTED_VALID_MESSAGE_CASES` 38→40、`EXPECTED_BODY_REJECTED` 9→10（见 §6 的跨界说明） |
| `crates/identity-auth/src/authorization.rs` | `GRANTS` 的 `grant.remote-work` 会员加 `session.resume`；`command_names()` 的「12 条」注释改 13 |
| `crates/identity-auth/tests/authorization.rs` | 既有硬编码计数断言同步：`request.scopes.len()` 6→7，并新增 `contains("session.resume")` 断言；「12 条」注释改 13 |
| `crates/core/src/broker.rs` | **只**在 `required_grant` 的 `_ => return None` 之前加一条臂 `"session.resume" => "grant.remote-work"` |
| `compatibility/commands/v1/commands.json` | 追加 `session.resume`（mutation / `grant.remote-work` / `pack: null` / `transport: ["node_link"]` / `conditional_mvp`），并把 `session.resume` 加入 `grants["grant.remote-work"]` |
| `schemas/node-link/v1/command.schema.json` | `commandName.enum` 加 `session.resume`；`commandSubmit.body.oneOf` 加 `submitSessionResume`；新增 `submitSessionResume` def（空对象 payload + 会话三字段非 null + `expectedVersion: null`）；`commandTerminal` 的 `allOf` 加 `session.resume` 的 `completed → sessionResumeResult` 分支 |
| `schemas/node-link/v1/common.schema.json` | 新增 `sessionResumeResult` def |
| `fixtures/node-link/v1/valid/command-submit-session-resume.json` | 新增（空对象 payload 的正例） |
| `fixtures/node-link/v1/valid/command-terminal-session-resume-completed.json` | 新增（`completed` 结果 = `sessionResumeResult` 的正例） |
| `fixtures/node-link/v1/invalid/session-resume-with-cwd.json` | 新增（payload 出现 `cwd` 的负例，`expectedKeyword: additionalProperties`） |
| `fixtures/node-link/v1/manifest.json` | 三条新 fixture 登记（`check-schema-fixtures` 的「fixture 必须在 manifest 登记」门禁） |
| `fixtures/node-link/v1/README.md` | 正反样例表与 `invalid/` 意图说明补 `session.resume` 三行 |
| `scripts/check-command-catalog.mjs` | **D6**：`pack: null` 豁免判据从「按命令名（`session.create`）」改为「按 transport：含 `sync` 才必须有 pack」，并写明该判据的理由 |
| `docs/NODE_LINK_PROTOCOL.md` | §10 grant→命令表（`grant.remote-work` 行）、§12.5 计数（12→13）、§12.7 payload 表 + `session.resume` 硬约束与结果契约、§15 uncertain 语义 |
| `docs/SYNC_PROTOCOL.md` | §11.5 命令表加第 13 行、计数文本 12→13 |
| `docs/SECURITY_DESIGN.md` | §10.2 表格加第 13 行与 `session.resume` 授权说明 |
| `AGENTS.md` | §10 封闭词表门禁句子补 `pack` 的 transport 判据 |
| `README.md` | 「合同检查」②补同一规则 |
| `.github/workflows/ci.yml` | 文件头 `npm run check` 判据注释补同一规则 |

`package.json` 的 `check` 脚本**未改**（未新增/删除门禁，D6 只改已有门禁内的判据）。

## 2. 七处原子点逐项对应（同一次提交 32f71f5）

| # | 原子点 | 落点 | 证据 |
| --- | --- | --- | --- |
| 1 | `compatibility/commands/v1/commands.json` | `commands[12] = session.resume`；`grants["grant.remote-work"] = ["session.create","session.resume"]`（packs ∪ grants 成员集合 = 13 个命令名） | PV2 `command catalog OK: 13 commands` |
| 2 | `schemas/node-link/v1/command.schema.json` | `commandName.enum` 13 项（顺序与 registry 的 node_link 子集一致）；`submitSessionResume` 分支 | PV2（同一门禁的 `collectCommandNames` 双向集合比对）+ PV1（fixture 正反例） |
| 3 | `docs/SYNC_PROTOCOL.md` §11.5 | 表格第 13 行 + 计数段落 | PV2（`tableCommands` 集合相等） |
| 4 | `docs/SECURITY_DESIGN.md` §10.2 | 表格第 13 行 | PV2（同上） |
| 5 | `crates/core/src/broker.rs::required_grant` | 新增一条臂 | PV2（门禁按源码解析 match 臂，比对命令名集合与 grant 取值） |
| 6 | `crates/node-link-protocol::CommandName` | `ALL`/`as_str`/`FromStr` 13 项 | PV1（`crates/node-link-protocol` 单测 + `schema_drift`）；PV2 的 schema 侧双向集合比对 |
| 7 | `crates/identity-auth`（DR1-F9 的第七个强制写目标） | `GRANTS` 会员 + 既有验收测试的计数断言；该测试用 `include_str!` 直读 `commands.json` | PV1 `identity_auth tests/authorization.rs: 15 passed`（`grants_match_machine_catalog`、`command_names_cover_the_whole_catalog`、`node_request_expands_grants_to_command_scopes`） |

`docs/IDENTITY_AND_AUTH_CONTRACT.md` 的 no-op 判定见 §5。

## 3. 门禁说明四处同步的落点

| 处 | 落点 |
| --- | --- |
| `package.json` 的 `check` 脚本 | **无需改**（D6 不新增/删除门禁，只改 `check:commands` 内部判据） |
| `AGENTS.md` §10 本段说明 | 封闭词表门禁句末补「命令目录侧还按 `transport` 判定 `pack`……」 |
| `README.md` 「合同检查」② | 同一条规则写进②的括号 |
| `.github/workflows/ci.yml` 注释 | 文件头 `npm run check` 判据注释补同一规则 |

## 4. Checks（原样命令与结果）

| Check | 命令（worktree 根 `D:/Project/acp-remote-wt/session-resume-wp2`） | 退出码 | 结果 | 日志 |
| --- | --- | --- | --- | --- |
| PV1 阶段 1 | `cargo fmt --all -- --check` | 0 | PASS | `reports/wp2-coder-PV1.log` |
| PV1 阶段 1 | `CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-wp2 cargo clippy --locked -p node-link-protocol -p core -p identity-auth --all-targets --all-features -- -D warnings` | 0 | PASS（0 warning） | 同上 |
| PV1 阶段 1 | `CARGO_TARGET_DIR=… cargo test --locked -p node-link-protocol -p core -p identity-auth --all-features` | 0 | PASS（`acp_core` 121、`identity_auth` 集成 15/20/30/2/9/5、`node_link_protocol` lib 9 + 集成 6/7/5/7/3/3、doc-test 2；无 0 测试的约定套件） | 同上 |
| PV2 | `npm run check` | 0 | PASS（十道门禁全绿：`command catalog OK: 13 commands`、`schema fixtures OK: 120 valid, 26 invalid`、`contract assets OK: 17 schemas, 159 fixture files`、`doc links OK`、`crate boundaries OK`、`contract drift OK`、`check:agentic`） | `reports/wp2-coder-PV2.log` |
| 提交信息 | `npx --no-install commitlint --from 81e350ff… --to HEAD` | 0 | PASS | 见本报告 §7 |

**预期红窗口（不作为 PV1 结果，只作说明性证据）**：`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` 因第 13 个 wire payload 变体在 `crates/server/src/node_link/command.rs:2191` 的 `core_payload()` 穷尽匹配失配而失败（唯一一条 `error[E0004]`，`could not compile server`），该收口按 plan.md 属 WP6。原始输出见 `reports/wp2-coder-red-window.log`。

## 5. `docs/IDENTITY_AND_AUTH_CONTRACT.md` 为 no-op 的依据

`grep -n "grant.remote-work|session.create|GRANTS|command_names|12 条|13 条" docs/IDENTITY_AND_AUTH_CONTRACT.md` 只命中一行（§6.1）：

```text
docs/IDENTITY_AND_AUTH_CONTRACT.md:51: 只存在于 identity-auth 的**授权词表镜像**：`authorization::{PACKS, PRESETS, GRANTS, LOCAL_CAPABILITIES}`；唯一机器来源仍是 `compatibility/commands/v1/commands.json`（§6.1），镜像由常驻测试逐项断言。
```

该文档只声明「镜像存在 + 唯一机器来源是 `commands.json`」，**不列举任何 grant 的会员**，也不含命令数量文本（同文件的 §6.1 只写展开规则，不写命令名清单）。因此本次会员从 1 项变 2 项不需要改它——**未改动该文件**。

## 6. 边界与需要 main 知悉的两处跨界事实

1. **未触任何 `crates/server/` 文件**（第 13 个 payload 变体造成的 `core_payload()` 失配是已知红窗口，收口属 WP6）。
2. **改了一个未在 `plan.md` 的 Shared File Ownership 里登记的既有测试文件**：`crates/node-link-protocol/tests/envelope_fixtures.rs`。原因是本 WP 按 design D5 必须新增 node-link fixture（并在 `manifest.json` 登记），而该测试把 manifest 的用例数写成硬编码常量（`EXPECTED_VALID_MESSAGE_CASES` / `EXPECTED_BODY_REJECTED`），不改它就必然让 WP2 自己分支上的 PV1 变红。改的是两个常量（38→40、9→10），**没有新增/删除用例、没有弱化断言**（多出来的两条正例与一条负例由 manifest + ajv 与 Rust 解码双重校验）。性质与 DR1-F14 的「既有测试字面量随实现变化」一致；该文件属 `crates/node-link-protocol/`（WP2 的整 crate 写范围），与「TP2 独占**新增**测试文件」不冲突。请 reviewer-B 与 main 确认这一登记是否需要补进 Shared File Ownership。

## 7. 提交

```text
32f71f5d5f08145cfd3e3bddd7d6f6a46a0a5992
feat(node-link): 加入 session.resume 命令与封闭词表原子点
```

- 分支：`agentic/session-resume-wp2`；父提交：`81e350ff340014265eb7c9251237c799d4357fee`。
- `git status --porcelain` 提交后为空（无未提交、无暂存残留）。
- **未使用 `--no-verify`**：本 worktree 里 `core.hooksPath = .husky/_`，而 `.husky/_` 目录不存在（husky 的钩子 shim 由主仓库的 `npm install` 生成，worktree 内没有），因此 `git commit` **没有执行 pre-commit/commit-msg 钩子**（提交输出中没有任何 `pre-commit:` 行）。等价判定由本 WP 手工执行并在 §4 记录：`cargo fmt --check`、`npm run check`、三条 crate 级 clippy/test、以及单独一条 commitlint 校验。worktree 内没有安装钩子属 provisioner/环境事实，不阻塞本 WP，但会影响 plan.md 里「红窗口内提交会被钩子拒绝」的预期（本 WP 未触发该路径）。

## handoff_index

```yaml
handoff_index:
  - task_id: "2.2"
    work_package: WP2
    role: coder
    phase: implement
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "32f71f5d5f08145cfd3e3bddd7d6f6a46a0a5992"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "reports/wp2-coder.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "WP2 分支阶段 1：worktree D:/Project/acp-remote-wt/session-resume-wp2、CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-wp2、目标提交 32f71f5d5f08145cfd3e3bddd7d6f6a46a0a5992；命令 = cargo fmt --all -- --check && cargo clippy -p node-link-protocol -p core -p identity-auth --all-targets --all-features -D warnings && cargo test 同三 crate --all-features；workspace 全量属阶段 2，本阶段不适用（红窗口，见报告 §4）"
    source_evidence: NOT_APPLICABLE
    log_path: "reports/wp2-coder-PV1.log"
  - task_id: "2.2"
    work_package: WP2
    role: coder
    phase: implement
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "32f71f5d5f08145cfd3e3bddd7d6f6a46a0a5992"
    evidence_type: CHECK
    evidence_id: PV2
    report_path: "reports/wp2-coder.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "同一 worktree 与目标提交上运行 npm run check（Node v22+，仓库内 devDependencies）；十道门禁全绿，含 check:command-catalog（13 commands）、check:schema-fixtures（新增 3 条 node-link fixture 的 ajv 正反例）、check:contract-assets、check:docs"
    source_evidence: NOT_APPLICABLE
    log_path: "reports/wp2-coder-PV2.log"
```

## 待澄清 / 交给 main 的问题

1. `crates/node-link-protocol/tests/envelope_fixtures.rs` 的计数常量属既有测试文件，是否需要在 `plan.md` 的 Shared File Ownership 补登一行（WP2 单一写者）？（本报告 §6 已说明理由与改动范围。）
2. design D1 只冻结了 `terminal.result（completed）= SessionResumeResult`，未要求 `command.accepted.result` 对 `session.resume` 恒为 `null`；因此 wire 层只按 `command.terminal` 的 `if/then` 收紧了结果形状，accepted 的 `result` 仍是 schema 的通用 `object | null`（Owner 侧由 WP6 按规格发 `null`）。若 main 希望 wire 层也钉死该点，需要在 design D1 补一行再改 schema——**本 WP 未自行扩大冻结面**。
3. 未新增 feature ID（`node-link.session-resume.v1` 之类）：proposal 的 Impact 明确「不新增 feature ID」，因此 §11.3 的 feature 表保持不动；若后续需要 Access 侧按 feature 协商，属另一变更。
