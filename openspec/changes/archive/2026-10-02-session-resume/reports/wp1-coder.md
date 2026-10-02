# WP1 Coder Report（session-resume / tasks 2.1）

## Shared Report

| 字段 | 值 |
| --- | --- |
| task_id | `2.1`（[wp:WP1] [PV1] [PV2]） |
| work_package | WP1 —— `acp-protocol` 的 `session/resume` 类型化 DTO 与兼容性矩阵提升（交付单元 U1，阶段 implement） |
| role | coder |
| phase | implement |
| agent_context | 任务级 coder 子 Agent `coder-A`（delegated，`fork_turns=none` 等效：只接收本任务模板 + 契约绝对路径 + 资源/检查/输出，不继承主 Agent 的规划讨论与实现推理） |
| target_revision | `248d9b9fa69a1e6375a3b9708fdaa2a127dc7ccb`（本 WP 交付提交，分支 `agentic/session-resume-wp1`） |
| start_revision | `81e350ff340014265eb7c9251237c799d4357fee`（provisioner 交接的基线，`refs/heads/main`） |
| scope | `crates/acp-protocol/`、`compatibility/acp/v1/matrix.json`、`docs/ACP_COMPATIBILITY_MATRIX.md`；不写 `fixtures/acp/v1/`、不写其它 crate、不改权威计划/任务/verification |
| changes | 4 个文件（见「改动清单」）；新增 2 个公开 DTO + 1 个解码入口 + 2 个私有辅助函数 + 5 条内联单元测试；改 `methods.rs` 一条登记项；改矩阵两行；改文档 §5 与修订记录 |
| checks | PV1（阶段 1，`-p acp-protocol`）PASS；PV2（`npm run check`）PASS；额外 `cargo clippy --locked --workspace` PASS（见「检查」） |
| issues | 无 FAIL / BLOCKED；1 条环境事实（worktree 内 `.husky/_` 未安装，pre-commit 钩子未执行，未使用 `--no-verify`；等价检查已手工跑，见「检查」的「钩子说明」）。1 条待主 Agent 决策的小项（矩阵 `revision` 字段未随本次改动推进，见「待澄清问题」） |
| result | PASS（WP1 范围内的实现与计划内检查全部满足；不代表独立 review、E2E 或合并已完成） |
| evidence_paths | `D:\Project\acp-remote\openspec\changes\session-resume\reports\wp1-coder.md`（本报告，含 PV1/PV2 原始命令与输出） |
| resource_cleanup | 仅使用分配的 `CARGO_TARGET_DIR=D:\Project\acp-remote-target\session-resume-wp1` 与进程内临时目录；未创建数据库/网络/独占资源；未清理 provisioner 的资源（worktree 与 target 目录由 provisioner 独占回收） |

## Target（已核实）

- Worktree：`D:\Project\acp-remote-wt\session-resume-wp1`（分支 `agentic/session-resume-wp1`）
- 起点提交：`81e350ff340014265eb7c9251237c799d4357fee`（`git rev-parse` 于开工前核对）
- 交付提交：`248d9b9fa69a1e6375a3b9708fdaa2a127dc7ccb`，标题 `feat(acp): 新增 session/resume 类型化 DTO 并提升矩阵交付节奏`
- 工具链：`cargo 1.98.1 (797e8a9bc 2026-08-05)` / `rustc 1.98.1 (48a229cea 2026-09-01)`（由 `rust-toolchain.toml` 固定）/ `node v24.19.0` / `npm 12.0.2`
- 依赖包含关系：无上游（plan.md 的 WP1 Dependencies = `none`），故无 `merge-base --is-ancestor` 待核对项；本提交可直接作为 WP5（`code:WP1`）与 WP6/TP2 的上游候选

## 改动清单（按 design.md 的 D1/D5，取值未自行改变）

| 文件 | 改动 |
| --- | --- |
| `crates/acp-protocol/src/message.rs` | 新增 `SessionResumeRequest`（required `sessionId`、required `cwd`、可选 `_meta`）、`SessionResumeRequest::new`、`SessionResumeRequest::from_params`、方向校验入口 `session_resume_request(envelope)`、`SessionResumeResponse`（可选 `modes` / `configOptions` / `_meta`），以及私有 `required_string`、`value_kind`；文件尾部新增 `#[cfg(test)] mod tests`（5 条用例） |
| `crates/acp-protocol/src/methods.rs` | `session/resume` 登记项：`Delivery::PostMvp` → `Delivery::ConditionalMvp`，`implemented` `false` → `true`，并加 3 行注释说明门控在 `agent-host`（矩阵是权威，本表是与矩阵逐条比对的 Rust 镜像） |
| `compatibility/acp/v1/matrix.json` | `method.session_resume` 与 `cap.agent.session_resume`：`delivery` `post_mvp` → `conditional_mvp`，`layers.broker` `explicit_unsupported` → `project_and_preserve`；其余层保持 `acp=native`、`sync=explicit_unsupported`、`pwa=explicit_unsupported`、`facade=not_advertised`；`session/load` 未改（仍 `post_mvp`） |
| `docs/ACP_COMPATIBILITY_MATRIX.md` | 顶部修订记录新增 2026-09-30 一行；§5「第一阶段门槛」把 `session/resume` 从 `post_mvp` 枚举移出，新增三条说明（门控取 `agentCapabilities.sessionCapabilities.resume`、未宣告必须显式不支持且不得发送/降级、其余层目标行为与交付范围），并补一句 `session/load` 保持 `post_mvp`。措辞对未落地能力使用「本次变更的计划交付范围」，符合 `AGENTS.md` §10 的要求 |

未动（有意）：`fixtures/acp/v1/` 与 `manifest.json`（本 WP 走「只用内联字节」分支，DR1-F24 的另一种满足方式）；`crates/acp-protocol/tests/`（该目录归 TP2 独占）；其它 crate 与其它文档。

## 需求行映射（R1–R4）

| 需求行 | 覆盖方式 | 证据 |
| --- | --- | --- |
| R1 `session/resume` 的类型化解码与圆形保真 | 请求/响应各有类型化 DTO；解码边界 `from_params`/`decode_response`；保真仍走 `RawDocument` 原文承载（本 crate 的既有保真机制，`lib.rs` 文件头有说明） | 单元测试 `message::tests::resume_request_decodes_typed_fields_and_re_encodes_byte_exact`、`..._encode_is_limited_to_the_pinned_fields`、`resume_response_decodes_typed_view_and_re_encodes_byte_exact`；PV1 |
| R2 正常解码并回写未知字段 | 请求原文含 `futureFieldFromNewerAcp` 与 `_meta`：解码出类型化 `sessionId`/`cwd`/`meta` 后，再编码字节与输入逐字节相等；未知字段与 `_meta` 仍在原文中 | `..._decodes_typed_fields_and_re_encodes_byte_exact`（响应侧同断言在 `resume_response_decodes_typed_view_and_re_encodes_byte_exact`）；PV1 |
| R3 缺少 required 字段被拒绝 | `from_params` 对缺字段返回 `AcpError::MissingField{field}`、对类型不符返回 `AcpError::InvalidField{field}`（可区分），两种都不构造 DTO、不用默认值补齐 | 单元测试 `resume_request_rejects_missing_and_mistyped_required_fields`（6 组输入 + 两条精确错误断言）；PV1 |
| R4 `session/load` 仍为显式不支持 | `session/load` 的代码与矩阵行均未改；新用例断言其登记状态仍为 `NotImplemented`、`ensure_direction` 仍返回 `Unsupported`，且 `session/resume` 的登记状态为 `Implemented`（两者不得互相漂移） | 单元测试 `message::tests::session_load_stays_explicitly_unsupported`；既有集成用例 `tests/raw_fidelity.rs::unimplemented_and_unknown_methods_are_explicitly_unsupported`（含 `session/load`，本次未改且仍绿）；PV1/PV2 |

设计取值核对：命令/grant/payload/结果与错误码属 WP2/WP3/WP6，本 WP 不涉及；本 WP 只落实 D1 的「`session/resume` 请求 required `sessionId`+`cwd`」与 D5 的矩阵/文档两行（`broker` 取 schema 枚举内的 `project_and_preserve`，未写 `native`）。

## 检查

### PV1 阶段 1（分支、本 WP crate 子集）

工作目录：`D:\Project\acp-remote-wt\session-resume-wp1`；环境：`CARGO_TARGET_DIR=D:\Project\acp-remote-target\session-resume-wp1`（provisioner 分配）；目标提交 `248d9b9fa69a1e6375a3b9708fdaa2a127dc7ccb`。

```text
$ cargo fmt --all -- --check
exit=0（无输出）

$ cargo clippy --locked -p acp-protocol --all-targets --all-features -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.08s
exit=0

$ cargo test --locked -p acp-protocol --all-features
     Running unittests src\lib.rs (...\acp_protocol-9fbb21c33feb5045.exe)
running 5 tests
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
     Running tests\capabilities.rs
running 5 tests
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
     Running tests\envelope.rs
running 6 tests
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
     Running tests\fixtures.rs
running 2 tests
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
     Running tests\matrix_tables.rs
running 7 tests
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
     Running tests\raw_fidelity.rs
running 7 tests
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
     Running tests\updates.rs
running 8 tests
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   Doc-tests acp_protocol
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
exit=0
```

新增的 5 条用例（全绿）：

```text
test message::tests::resume_request_encode_is_limited_to_the_pinned_fields ... ok
test message::tests::resume_request_rejects_missing_and_mistyped_required_fields ... ok
test message::tests::resume_response_decodes_typed_view_and_re_encodes_byte_exact ... ok
test message::tests::resume_request_decodes_typed_fields_and_re_encodes_byte_exact ... ok
test message::tests::session_load_stays_explicitly_unsupported ... ok
```

说明：`tests/matrix_tables.rs::methods_table_matches_matrix` 与 `post_mvp_methods_are_not_claimed_as_implemented` 都跑过且为绿——矩阵 `delivery` 改动与 Rust 镜像同步由这两条既有契约测试判定，不是我自述。

### PV2（合同门禁）

工作目录同上，命令 `npm run check`，exit=0：

```text
schema fixtures OK: 118 valid, 25 invalid (ajv Draft 2020-12), 39 event views bound
command catalog OK: 12 commands
error registry OK: 58 codes across 2 protocols
feature registry OK: 11 feature ids across 2 protocols
contract assets OK: 17 schemas, 156 fixture files, 12 transcript vectors re-encoded from input, 20 negative vectors rejected as declared, 2 SAS values recomputed
ACP compatibility matrix OK: 25 methods, 11 updates, 5 content blocks, 3 tool content types, 19 capabilities, 8 invariants, 10 test families (71 rows, schema validated by ajv)
doc links OK: 398 relative links, 6749 section refs across 403 markdown files
crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致（12 个已在矩阵登记）
contract drift OK: §7 的 36 条 DDL 与 crates\storage-sqlite\src\migrate.rs 逐条一致；§5 的 15 个 trait / 93 个方法签名与 crates\core\src\ports.rs 一致
PASS  toolchain / openspec / config / schema / schema validation / verification skill / AGENTS.md / manifest
Installation: PASS
Totals: 19 passed, 0 failed (19 items)
agentic 宿主入口检查完成：17 个文件
exit=0
```

其中 `check:acp` 用 ajv 强校验矩阵（`additionalProperties: false` 的分层枚举），因此 `broker=project_and_preserve`、`delivery=conditional_mvp` 的合法性由该门禁直接判定；`check:schema-fixtures` 未新增 fixture，仍为 118/25。

### 额外（非计划内，仅补强「无跨 crate 涟漪」的判定）

```text
$ cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 18.86s
exit=0
```

理由：`.husky/pre-commit` 的 clippy 步是 workspace 全量；本 WP 未跑 `cargo test --workspace`（属 PV1 阶段 2，留给集成基线/候选，见 plan.md 的红窗口说明）。

### 钩子说明（环境事实，必须记录）

worktree 内 `git config core.hooksPath` = `.husky/_`，但该目录在本 worktree 不存在（husky 只在主仓库执行过 `prepare`，`node_modules` 由 provisioner 以目录联接提供、未在本 worktree 装配钩子），因此本次提交**没有**触发 `.husky/pre-commit` 与 `.husky/commit-msg`。这**不是**用 `git commit --no-verify` 绕过：钩子等价步骤已手工执行（`cargo fmt --check` + `npm run check` + `cargo clippy --workspace -D warnings` 全绿），提交信息符合 Conventional Commits（`feat(acp): …`，scope `acp` 在 `commitlint.config.mjs` 词表内，由人工核对，未跑 commitlint CLI）。

## 未执行项

- `cargo test --locked --workspace --all-features`（PV1 阶段 2）：按 plan.md 必须在「全部 WP 集成后的集成基线/候选/主分支」运行，本 WP 分支不跑。
- 独立 review（CR1）、TP1/TP2 的用例设计/编写、E2E：不属本角色，未执行也未替代。
- 目标 Agent 是否真的宣告并实现 `sessionCapabilities.resume`：本 WP 无法验证（属 7.1 validation 的待验证假设）。

## 待澄清问题

1. `compatibility/acp/v1/matrix.json` 的 `revision` 字段（值 `2026-09-18`）是否应随本次矩阵内容改动推进到当天日期？design.md 的 D5 资产表未列该项，脚本与门禁也不读取它（`grep` 全部 `scripts/*.mjs` 无引用），因此我按「不越界」保持原值；若计划要求矩阵修订日期与内容一致，请指定新值，我在修复轮补上（属一行改动）。
2. `methods.rs` 的 `implemented: true` 是本 WP 的自主判断（design/plan 未逐字规定）：依据是 `MethodStatus::Implemented` 的定义「本 crate 能编码/解码并按语义处理」，与同层的 `session/set_mode`、`session/set_config_option`（`conditional_mvp` + `implemented: true`）同形；门控仍在 `agent-host`。若计划认为 `implemented` 只应在整条链路落地后才置真，请指出，我改回 `false` 并同步那条注释（此时 `session_resume_request(envelope)` 入口会恒返回 `Unsupported`，需一并决定该入口去留）。

## handoff_index

```yaml
handoff_index:
  - task_id: "2.1"
    work_package: WP1
    role: coder
    phase: implement
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "248d9b9fa69a1e6375a3b9708fdaa2a127dc7ccb"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "openspec/changes/session-resume/reports/wp1-coder.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在分支 agentic/session-resume-wp1 的固定提交 248d9b9 上、以 CARGO_TARGET_DIR=D:\\Project\\acp-remote-target\\session-resume-wp1 执行 PV1 阶段 1（cargo fmt --all --check；cargo clippy --locked -p acp-protocol --all-targets --all-features -- -D warnings；cargo test --locked -p acp-protocol --all-features），工具链由 rust-toolchain.toml 固定为 cargo/rustc 1.98.1；crate 子集取 plan.md 的 WP1 Verification 列；无上游依赖，故无包含关系差异"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.1"
    work_package: WP1
    role: coder
    phase: implement
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "248d9b9fa69a1e6375a3b9708fdaa2a127dc7ccb"
    evidence_type: CHECK
    evidence_id: PV2
    report_path: "openspec/changes/session-resume/reports/wp1-coder.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "同一提交、同一 worktree 根执行 npm run check（Node v24.19.0 / npm 12.0.2）；十道门禁串行全绿，含 check:acp（ajv 强校验矩阵 schema，覆盖本次 delivery/layers 取值）、check:docs、check:boundaries、check:drift、check:agentic；本次改动与它的镜像/文档同包同提交，未依赖任何上游 WP 的资产"
    source_evidence: NOT_APPLICABLE
```
