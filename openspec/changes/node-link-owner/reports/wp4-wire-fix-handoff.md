# WP4 wire 修订（2.29）+ §2.5 JSON 结构上限执行点（2.30）Handoff

## Shared Report

- **task_id**：2.29（`node.challenge` 增 `catalogRevision`，用户裁决 A / design D13）、2.30（§2.5 JSON 结构上限执行点，design D13 附带项，WP5 预备）。
- **role / phase**：coder / implement。
- **agent_context**：worker 子 Agent（本机 worktree 独占；未继承其它 WP 的实现对话）。工作目录 `D:\Project\acp-remote-wt\node-link-owner`（分支 `agentic/node-link-owner`）。
- **target_revision**：2.29 = `a85a65d`；2.30 = `319c77b`。起点 = `038349b`（WP4 handoff 的交付提交，核实为干净工作区）。
- **scope**（写入范围）：
  - 2.29：`docs/NODE_LINK_PROTOCOL.md`（§12.2 表 + 文件头修订记录）、`schemas/node-link/v1/handshake.schema.json`、`fixtures/node-link/v1/{manifest.json,valid/node-challenge.json,invalid/node-challenge-missing-catalog-revision.json}`、`crates/node-link-protocol/src/handshake.rs`、`crates/node-link-protocol/tests/{envelope_fixtures.rs,transcript_vectors.rs}`、`crates/server/src/node_link/conn/{session.rs,tests.rs}`；
  - 2.30：`crates/node-link-protocol/src/{structure.rs（新增）,envelope.rs,lib.rs}`、`crates/node-link-protocol/tests/structure_limits.rs`（新增）、`crates/server/src/node_link/conn/{wire.rs,tests.rs}`；
  - **未改**任何权威规划工件（`plan.md`/`tasks.md`/`verification.md`）、`compatibility/**`、`crates/core`/`storage-sqlite`/`identity-auth`/`app`、`crates/server/src/transport/**`（WP2 冻结形状）。
- **changes**：见「改动与需求映射」。
- **checks**：`npm run check`（exit 0）、`cargo fmt --all -- --check`（exit 0）、`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`（exit 0）、`cargo test --locked -p node-link-protocol -p server --all-features`（exit 0，无失败/忽略）、额外 `cargo test --locked --workspace --all-features`（exit 0）。原始输出：`reports/wp4-wire-fix.log`。
- **issues**：无阻断项。两条需要在 review 中确认的设计判断（结构上限的判定顺序、与 §2.4 错误码的映射）见「设计判断与理由」。WP4 handoff 登记的两条缺口（`catalogRevision` 不在 wire、§2.5 三上限没有执行点）在本批改动后均已闭合。
- **result**：**PASS**（两个任务的工作包检查全部满足）。**不代表**独立 review、`[PV5]`/E2E 或合并已完成。
- **evidence_paths**：`reports/wp4-wire-fix.log`、本文件。
- **resource_cleanup**：用例只绑定 `127.0.0.1:0`（`Harness::stop` 触发 listener 关闭与排空）；未起后台进程、未用数据库服务/容器/固定端口/外部账号；`target/` 作为缓存保留（被 git 忽略）。

## handoff_index

```yaml
handoff_index:
  - task_id: "2.29"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "a85a65d"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: "reports/wp4-wire-fix.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "工作区 = a85a65d（提交后 git status 为空）上的本机轮次：npm run check 退 0（schema fixtures 118 valid/25 invalid 含新增缺字段反例）；cargo fmt --all -- --check 退 0；cargo clippy --locked -p node-link-protocol -p server --all-targets --all-features -- -D warnings 退 0；cargo test --locked -p node-link-protocol -p server --all-features 全绿（conn 的 the_challenge_catalog_revision_is_the_proof_transcript_source、handshake_completes_and_enters_the_business_phase、fixtures_are_consumed_by_the_handshake_and_error_layers 见日志第 5 节）。[PV5] 的本机端到端轮次属 3.7/3.9，本行只声称 [PV3]。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.30"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "319c77b"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: "reports/wp4-wire-fix.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "工作区 = 319c77b（提交后 git status 为空）上的本机轮次：npm run check 退 0；cargo fmt --all -- --check 退 0；cargo clippy --locked --workspace --all-targets --all-features -- -D warnings 退 0；cargo test --locked -p node-link-protocol -p server --all-features 全绿（structure_limits 7 个用例、conn 的 json_structure_limits_are_enforced_on_the_wire、protocol_constants_are_not_configurable 见日志第 5 节）；另跑 cargo test --locked --workspace --all-features 全绿（日志第 6 节，确认协议 crate 改动不影响其它 crate）。"
    source_evidence: NOT_APPLICABLE
```

## 改动与需求映射

### 2.29（提交 `a85a65d`）—— `node.challenge` 增必需 `catalogRevision`

| 文件 | 内容 |
|---|---|
| `docs/NODE_LINK_PROTOCOL.md` | §12.2 的 `node.challenge` 行增 `catalogRevision`(decimal string)（全部必需）并写明语义：Owner 当前目录修订号、与 `node.ready.catalogRevision` 同源、两个连接 domain 的 tag 6 来源；文件头修订记录加一行（2026-09-26，v1 内合同修订，未实现未发布）。**§9.3/§9.4 的 transcript 表未改**（表本来就含 tag 6） |
| `schemas/node-link/v1/handshake.schema.json` | `nodeChallenge.body.required` 增 `catalogRevision`；`properties` 增 `{"$ref": "common.schema.json#/$defs/decimalString"}` |
| `fixtures/node-link/v1/valid/node-challenge.json` | 加 `"catalogRevision": "42"`（与该消息同源的固定向量 `transcripts/challenge.json`/`node-proof.json` 的 `input.catalogRevision` 一致） |
| `fixtures/node-link/v1/invalid/node-challenge-missing-catalog-revision.json`（新增） | 与上者同形但缺该字段；manifest 登记为 `valid:false` + `expectedKeyword: "required"` |
| `crates/node-link-protocol/src/handshake.rs` | `NodeChallenge` 增 `catalog_revision: DecimalString`（wire 名 `catalogRevision`），文档说明它是 tag 6 来源、缺失即握手不可完成 |
| `crates/node-link-protocol/tests/envelope_fixtures.rs` | `EXPECTED_BODY_REJECTED` 8 → 9（新反例在 body 层被拒，信封仍合法） |
| `crates/node-link-protocol/tests/transcript_vectors.rs` | 新增 `the_challenge_fixture_carries_the_connection_transcript_revision`：从 `valid/node-challenge.json` 解码出 `catalogRevision`，把它写回**挑战域与证明域两份固定向量的输入**再逐字节复算 `expected.transcriptBase64url`，并断言 fixture 的 `connectionId`/`nodeProof` 与挑战向量一致 |
| `crates/server/src/node_link/conn/session.rs` | `challenge_body` 增 `catalog_revision: u64` 参数并从本机水位装配（与进 `ChallengeRequest` 的值同一个来源）；模块文档把口径改成「挑战与 `node.ready` 同源」 |
| `crates/server/src/node_link/conn/tests.rs` | `send_proof` 只从 `node.challenge` 的 body 取 `catalogRevision`（新增 `challenge_catalog_revision`），删除带外参数与相关说明；`handshake_completes_and_enters_the_business_phase` 的挑战验签改用 wire 值；新增验收用例 `the_challenge_catalog_revision_is_the_proof_transcript_source`；fixture 消费用例补 `(Handshake, false)` 分支（握手反例在 body 级被拒） |

**2.29 的验收项**（任务原文：证明「拿到 `node.challenge` 的 `catalogRevision` 后能构造 `node-link-proof/v1` transcript 并通过验签路径」）：

- 连接级正例：`the_challenge_catalog_revision_is_the_proof_transcript_source` 从 wire 取值签 proof → 完整握手成功，且 `node.ready.catalogRevision` 与挑战同源；
- 连接级反例（证明该字段真的进了验签输入）：同一用例把取值 +1 后签名 → `nodelink.auth.proof_invalid` + 4401；
- 固定向量级：`the_challenge_fixture_carries_the_connection_transcript_revision` 用 wire 值复算两个连接 domain 的向量逐字节一致；
- Owner 挑战签发侧：`handshake_completes_and_enters_the_business_phase` 用 wire 值构造 `NodeLinkChallenge` 并验证 `nodeProof` 签名。

### 2.30（提交 `319c77b`）—— §2.5 的三个固定 JSON 结构上限

| 文件 | 内容 |
|---|---|
| `crates/node-link-protocol/src/structure.rs`（新增） | 三个常量（`MAX_NESTING_DEPTH = 64`、`MAX_OBJECT_FIELDS = 1_024`、`MAX_ARRAY_ELEMENTS = 10_000`）、`StructureError`（三种越界各带 `found`/`limit`）与只计数的扫描器 `check()`：字符串/转义感知、容器栈按对象与数组各自计数、深度按整帧文本计（最外层容器算第 1 层） |
| `crates/node-link-protocol/src/envelope.rs` | 新增公开错误变体 `EnvelopeError::Structure`；`Envelope::decode` 在 serde 解析**之前**先判结构，文档写明与语法判定的顺序与优先级 |
| `crates/node-link-protocol/src/lib.rs` | 导出 `structure` 模块 |
| `crates/node-link-protocol/tests/structure_limits.rs`（新增） | 三条上限各取边界正例（64 / 1024 / 10000）与越界反例（65 / 1025 / 10001）并断言具体错误变体与 `found`；另加：按容器而非累加计数、字符串与转义不改变计数、**不是 serde 默认值**（同帧经 serde 仍是合法 JSON）、结构与语法的判定顺序、结构判定不替代其它 wire 边界校验 |
| `crates/server/src/node_link/conn/wire.rs` | `judge` 把 `EnvelopeError::Structure` 映射为 `nodelink.protocol.schema_invalid`（消息级拒绝、连接保持可用，与其他 wire 形状失败同路径，message 用新的固定短文本 `STRUCTURE_MESSAGE`）；模块文档补该条；把「非法 JSON 还是信封形状错误」的那次通用 JSON 解析从「每条帧都做」改成「只在信封解码失败后做」 |
| `crates/server/src/node_link/conn/tests.rs` | 新增 `json_structure_limits_are_enforced_on_the_wire`：认证前用 `link.error` 帧（`details` 是 §2.4 的开放扩展点）发三条越界结构 → 各回 `schema_invalid`；再发三条边界值 → 随后握手照常完成（若任一条被拒，`expect_type("node.challenge")` 会先读到 `link.error`）；`protocol_constants_are_not_configurable` 锁定三个常量取值；覆盖映射表补 [R46] 行。同提交并入一行由 2.29 改动作废的陈旧注释（`Harness::view` 的说明不再提「带外取 revision」）。 |

**2.30 的执行点选择（任务要求的记录）**：判定放在 `node-link-protocol`（wire DTO 边界），而不是传输层或各家族 body 解码：

1. §2.4 的最后一条要求「ID、序号、时间戳、枚举和长度限制必须在 wire DTO 边界验证，不能延迟到 Agent adapter」；这三项与 `DecimalString`/`Base64Url` 的长度上限同类，属 `node-link-protocol` 既有的值对象校验面；
2. 放在 `Envelope::decode` 让所有入站帧（任何家族、任何阶段）一次生效，未来新增的消费方不会漏判；
3. 不适用的位置：传输层只管单条 WS message 的 1 MiB（`1009`）与 framing，看不到 JSON 结构；body 的类型化解码只覆盖对应家族、且 `payload.view`/`details` 等开放容器没有 DTO 可挂。
4. 不依赖 `serde_json` 的默认：它递归上限 128（与合同 64 不同值）、容器大小完全无上限；扫描器直接读原始文本，判定只依据本 crate 的常量（用例 `the_limits_do_not_come_from_serde_defaults` 锁定这一点）。

## 设计判断与理由（供 reviewer 判定，均为模块内部实现细节，代码注释已写明）

1. **结构判定先于语法判定**（`Envelope::decode` 的文档里写明）：`check()` 只读字节、不分配，把上限放在解析之前可避免「为了拒绝一条超限帧先分配整棵通用 JSON 树」，也让判定不依赖 serde 的递归上限。代价：「既畸形又超限」的帧按结构超限（`schema_invalid`）报告，而不是 `invalid_json`——两者同为「消息不生效、连接可用」，协议对这类叠加情形没有规定优先级。合法 JSON 的畸形仍按 `invalid_json` 报告（用例 `structure_is_checked_before_syntax` 同时锁定两侧）。
2. **越界映射到 `schema_invalid`（消息级拒绝）而不是连接级关闭**：任务允许两者，取前者与既有 decode 错误路径（未知信封字段、连接字段不符）一致；单条 message 过大仍由接入层以 `1009` 关闭（WP2 行为未改）。`link.error` 的 message 用新的固定短文本，不引入新错误码。
3. **`maxMessageBytes` 与结构上限分工**：前者是「帧的字节数」（可由 `node.ready.limits` 下调），后者是「帧内结构」（固定、不可下调）。一条 1 MiB 以内但结构超限的帧走 `schema_invalid`，不占用 1009。
4. **`judge` 的解析顺序调整**（顺带收益）：原实现每条帧都额外做一次 `serde_json::from_str::<Value>`；现在只在 `Envelope::decode` 已失败时才解析一次来区分 `invalid_json`/`schema_invalid`。对外可见的错误分类保持不变（用例 `envelope_and_sequence_violations_are_rejected_without_closing` 的 ①–⑧、`a_binary_frame_is_closed_with_4400` 全绿）。
5. **新增公开 API**：`node_link_protocol::structure`（常量 + `StructureError` + `check`）与 `EnvelopeError::Structure`。`EnvelopeError` 是 `#[non_exhaustive]`？——**不是**：它是普通 `enum`，下游 `match` 会因新增变体编译失败（本仓库内只有 `server::node_link::conn::wire` 做穷尽匹配，已在同一提交更新）。这是有意为之：新增信封级拒绝原因必须显式落地。

## 与 WP4 handoff 登记项的对应（核销）

| WP4 handoff 登记 | 本批处置 |
|---|---|
| 合同缺口 1：`node.challenge` 不带 `catalogRevision`（Access 首次连接无法验/签连接证明） | **已闭合**：用户裁决 A / D13，wire + schema + fixture + 类型 + Owner 填充 + 双向验收用例（提交 `a85a65d`） |
| 门禁归属问题 2：§2.5 的三个 JSON 结构上限在 Node Link 路径上没有执行点 | **已闭合**：判定点落在 wire DTO 边界（`node_link_protocol::structure` + `Envelope::decode`），适配器映射 `schema_invalid`，三条上限各有正/反用例（提交 `319c77b`） |
| WP4 的临时口径：`catalogRevision` 取 `store.head().global_sequence`，由 WP5 冻结 | **仍为临时口径**（本轮只把它补进 wire）：WP5 冻结 `catalog.snapshot.revision` 时必须与 `node.challenge`/`node.ready` 同源，`session.rs` 的模块文档与 docstring 已写明 |

## 不确定项与残留风险

| 项 | 说明 | 本批处置 |
|---|---|---|
| 结构上限的错误码 | 协议 §2.5 未规定越界的错误码/close 语义（只规定「不可下调」） | 取 `nodelink.protocol.schema_invalid`（消息级）；若 reviewer 认为应新增专用错误码或连接级关闭，需先改 contracts 再改实现 |
| `catalogRevision` 的最终语义 | wire 已补，但 revision 来源仍是全局水位 | 保持不变；WP5 冻结时核销（见上表） |
| Access 侧实现 | `node-link-client` 未落地，「Access 从挑战取值构造 proof」在用例里由测试扮演（同一素材与固定向量语义一致） | 不代表跨实现互操作；[PV5]/3.9 覆盖 |
| 结构扫描的语法假设 | 扫描器不判语法，畸形文本上的计数无意义 | 顺序固定为「结构 → 语法」，两侧都有用例；不影响合法帧的判定 |
| 1 MiB 上限与结构上限的组合 | 未新增组合用例（字节超限由 WP2 的 1009 覆盖） | 如需组合证据，可在 [PV5] 或 WP5 的边界用例中补 |

## 未执行项（不粉饰）

- 未执行 `[PV5]`（Windows 端到端 / 跨实现互操作）：属任务 3.7/3.9，本批只声称 `[PV3]`。
- 未执行 `cargo deny` / `gitleaks`（只在 CI）：本机未安装对应二进制，与 WP1–WP4 的口径一致。
- 未做真实第三方 Access 实现（Rust 侧 `node-link-client` 尚未落地）的互操作验证。

## 冻结形状变化（供 WP5/WP6/WP7 与 reviewer）

```text
node_link_protocol::structure
  MAX_NESTING_DEPTH: usize = 64      # §2.5，不可下调
  MAX_OBJECT_FIELDS: usize = 1_024
  MAX_ARRAY_ELEMENTS: usize = 10_000
  enum StructureError { NestingDepth{found,limit}, ObjectFields{found,limit}, ArrayElements{found,limit} }
  fn check(text: &str) -> Result<(), StructureError>   # 只计数；不判定语法

node_link_protocol::envelope
  enum EnvelopeError { …, Structure(StructureError) }   # Envelope::decode 在解析前先判定
  NodeChallenge { …, catalog_revision: DecimalString }  # wire 名 catalogRevision（新增必填）

server::node_link::conn::wire
  judge(): EnvelopeError::Structure → link.error(nodelink.protocol.schema_invalid)，消息级拒绝、连接可用
```
