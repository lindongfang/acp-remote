# WP3 fix2 Handoff — RV1-WP3 的 F2/F3/F4/F5（`node-link-owner` / WP3 配对 HTTP）

## Shared Report

- **task_id**: `RV1-WP3-F2` / `RV1-WP3-F3` / `RV1-WP3-F4` / `RV1-WP3-F5`（对应 review 报告 `reports/rv1-wp3.md` 的 Findings；F1 已由主 Agent 以 spec 修订闭环，不在本轮）
- **role**: coder；**phase**: fix；**stage**: work-package；**evidence_id**: `RV1-WP3`
- **agent_context**: 任务级 coder 子 Agent（fix 轮次），**不继承** WP3 实现或 review 会话，只按主 Agent 下发的 fix 清单与 `rv1-wp3.md` 的 Findings 原文作业。工作目录 `D:\Project\acp-remote-wt\node-link-owner`（worktree，分支 `agentic/node-link-owner`）；权威规划根 `D:\Project\acp-remote`（报告与日志写在该处 `openspec/changes/node-link-owner/reports/`，该目录不入版本控制）。工具集含 shell 与 Git：本轮 diff、门禁均由本 Agent 亲自执行。
- **base / target_revision**: base = `d127a802b75d63cd14789ea3ad504d89ce57f3f3`（开工时 HEAD 与之一致，`git status --short` 为空）；本轮 fix 唯一提交 = `443c6c09ce724ca1446e733a17da445d6d0b8547`。提交后 `git status --porcelain` 为空。
- **scope**（严格按派单允许写入面，**无越界**）：
  - `docs/IDENTITY_AND_AUTH_CONTRACT.md`：**仅 §5.1** 的 4 行（F2 两处自相矛盾 + F3 的 `has_secret` 登记条目）。
  - `docs/NODE_LINK_PROTOCOL.md`：**仅 §13.3** 新增一句澄清（secret 已清除时的终态/非终态口径）。
  - `crates/server/src/node_link/pairing.rs`：**仅** `Conflict(AlreadyClaimed) → 409` 分支的注释（F5）。
  - `crates/server/src/node_link/tests.rs`：**仅** `every_pairing_response_carries_the_four_security_headers` 的注释与 4 个状态码断言（F4）。
  - **未做**：不改 `design.md`/`plan.md`/`tasks.md`/`verification.md`/`proposal.md`/specs（D12 由主 Agent 另行同步）、`openspec/specs/**`、`schemas/**`、`compatibility/**`、`fixtures/**`、其他 crate 与文档。
- **changes**（1 个提交，4 个文件，30 insertions / 17 deletions；**无行为改动**）：
  1. **F2（合同自相矛盾，文档）**：`IDENTITY_AND_AUTH_CONTRACT.md` §5.1 把 `Actor::PairingClaimant` 的构造口径由「只能由配对 HTTP 端点在 claim/status 的 **proof 验证成功后**构造」收窄为「在 claim/status 的**证明校验路径上**构造（绑定该配对的只读可在校验前、写集只在通过后）」；同节顺序规则条把 status 的描述由「先校验 proof 再读」改为与实现一致——「先以绑定该配对的 claimant 只读该配对（零写入），再验 proof（`verify_node_link_pairing_status`），proof 失败只回 401」。与 `node_link/pairing.rs` 的 status 分支（③ 限流 → ④ 绑定 claimant 只读 → ⑤ 有 secret 先验 proof / 无 secret 按终态回、非终态 401）逐行一致。
  2. **F3（登记与事实不符，文档）**：§5.1 两处 `has_secret` 登记补记其生产用途——「除测试/诊断外，也由配对 HTTP 端点的 status 通道用于 secret 生命周期分支（本机已不持有其 secret 时终态配对回 200、非终态回 401，见 §13.3）」，并把同条「只供回归测试与本地诊断使用」的措辞收窄到排除 `pairing_status` 与 `has_secret`。
  3. **F4（注释与断言不符，测试）**：`every_pairing_response_carries_the_four_security_headers` 的注释改为如实描述——本用例覆盖 201 与两类 400（JSON 语法错误、缺字段）、409（同一 Access Node 换 nonce 的重复 claim），其余状态码的安全头由各专测覆盖；并为 4 个响应补 `assert_eq!(status, …)`：201（`§13.2`）、400、409、400。
  4. **F5（注释，代码）**：`pairing.rs` 的 `Err(PortError::Conflict(ConflictKind::AlreadyClaimed | ConflictKind::IdentityMismatch))` 分支补注释说明自愈语义——同内容并发 claim 极窄窗口内后落库者撞存储层 `AlreadyClaimed` → 409；客户端用**相同内容**再试一次即被 `verify_claim` 识别为幂等重试（`Repeat`）并按原 pairing request 回 201，不产生第二条记录。
  5. **§13.3 澄清（文档）**：`NODE_LINK_PROTOCOL.md` §13.3 在「状态查询一律返回 200」之后补一条——「本机已不持有配对 secret 时（到期，或 `approved` 配对首次 WSS 认证成功后按上方末条清除），终态配对（`rejected`/`expired`/`consumed`）的状态查询仍按 `200` 报告其业务状态（终态判定不依赖对端输入），非终态配对返回 `401`」。与变更 spec 的 R26 修订口径（「终态配对 200、非终态 401」与新增场景）逐字兼容；澄清已隐含语义，不改 wire 形状。
- **checks**（原始输出见 `wp3-pairing-http.log` 的「wp3-fix2 轮次」§1–§7）：
  - `cargo fmt --all -- --check` exit=0。
  - `cargo clippy --locked -p server --all-targets --all-features -- -D warnings` exit=0（无 warning）。
  - `cargo test --locked -p server -p identity-auth -p core --all-features`：全绿，0 failed / 0 ignored（`server` lib **193**、集成目标 102/0/15/20/30/2/9/5/14/6/4/0/4/0/2、doc-tests 2）。
  - `npm run check` exit=0（10 道；`check:drift` 报「§7 的 36 条 DDL 与 §5 的 15 个 trait / 88 个方法签名逐条一致」；`check:docs` 378 links / 4087 refs OK）。
  - 提交前钩子（`.husky/pre-commit`）三步全通过：fmt → `npm run check`（16 passed / 0 failed）→ `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`；commitlint 通过。
- **issues**: 4 项全部落地，无阻断、无新增不确定项。
- **result**: **PASS**（本轮 fix 范围内全部适用条件满足、门禁全绿；**不代表**独立复验（RV2/RV1 复检）、WP4/WP7 的接线、E2E 或合并已完成）
- **evidence_paths**:
  - 本报告：`openspec/changes/node-link-owner/reports/wp3-fix2-handoff.md`
  - 被处理的 review：`openspec/changes/node-link-owner/reports/rv1-wp3.md`（F2/F3/F4/F5）
  - 本轮原始输出：`openspec/changes/node-link-owner/reports/wp3-pairing-http.log` 的「wp3-fix2 轮次」（§1 修复内容、§2 fmt、§3 clippy、§4 测试、§5 `npm run check`、§6 未执行、§7 提交与工作区）
- **resource_cleanup**: 未新增端口、数据库、容器、证书或外部账号；未安装依赖、未改 `Cargo.toml`/`Cargo.lock`。无临时文件残留（`target/` 作为缓存保留，被 git 忽略）；提交后 `git status --porcelain` 为空，无未跟踪文件、无 staged 文件。

## handoff_index

```yaml
handoff_index:
  - task_id: "RV1-WP3-F2"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "443c6c09ce724ca1446e733a17da445d6d0b8547"
    evidence_type: DOC
    evidence_id: RV1-WP3
    report_path: "reports/wp3-fix2-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "IDENTITY_AND_AUTH_CONTRACT §5.1 的 Actor::PairingClaimant 构造口径已收窄为『证明校验路径上构造（只读可在校验前、写集只在通过后）』、status 顺序描述已改为『先绑定 claimant 只读（零写入）再验 proof，失败只回 401』，与 node_link/pairing.rs 的 status 分支逐行一致；npm run check 的 check:docs/check:drift 未受影响"
    source_evidence: "d127a802b75d63cd14789ea3ad504d89ce57f3f3"
  - task_id: "RV1-WP3-F3"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "443c6c09ce724ca1446e733a17da445d6d0b8547"
    evidence_type: DOC
    evidence_id: RV1-WP3
    report_path: "reports/wp3-fix2-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "§5.1 两处 has_secret 登记已补记其在 status 端点的生产用途（secret 生命周期分支：终态 200/非终态 401），与 pairing.rs:404 的 has_secret 分支一致；『只供回归测试与本地诊断使用』的措辞已收窄到排除 pairing_status 与 has_secret"
    source_evidence: "d127a802b75d63cd14789ea3ad504d89ce57f3f3"
  - task_id: "RV1-WP3-F4"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "443c6c09ce724ca1446e733a17da445d6d0b8547"
    evidence_type: CHECK
    evidence_id: RV1-WP3
    report_path: "reports/wp3-fix2-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "every_pairing_response_carries_the_four_security_headers 注释已如实描述覆盖范围（201 与两类 400、409；其余状态码在各专测），4 个响应均已 assert_eq!(status)：201/400/409/400；cargo test -p server 该用例 ok（lib 193 passed / 0 failed）"
    source_evidence: "d127a802b75d63cd14789ea3ad504d89ce57f3f3"
  - task_id: "RV1-WP3-F5"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "443c6c09ce724ca1446e733a17da445d6d0b8547"
    evidence_type: DOC
    evidence_id: RV1-WP3
    report_path: "reports/wp3-fix2-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "pairing.rs 的 Conflict(AlreadyClaimed|IdentityMismatch) → 409 分支已补注释说明自愈语义（同内容重试被 verify_claim 识别为 Repeat 并按原 pairing request 回 201）；cargo clippy -p server --all-targets 退 0，注释不改行为"
    source_evidence: "d127a802b75d63cd14789ea3ad504d89ce57f3f3"
  - task_id: "RV1-WP3-F1（§13.3 澄清配套）"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "443c6c09ce724ca1446e733a17da445d6d0b8547"
    evidence_type: DOC
    evidence_id: RV1-WP3
    report_path: "reports/wp3-fix2-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "NODE_LINK_PROTOCOL §13.3 已补一句澄清『secret 已清除时终态配对仍 200、非终态 401』，与变更 spec 的 R26 修订文本（终态 rejected/expired/consumed 200、非终态 401）逐字兼容；不改 wire 形状。F1 本身的 spec 修订由主 Agent 负责，本条目只是文档侧配套澄清"
    source_evidence: "d127a802b75d63cd14789ea3ad504d89ce57f3f3"
```

## 残留风险与下游注意

1. **待独立复检**：本报告是 coder 自检（`[PV3]` 范围），不代替 RV1-WP3 的复检轮次或候选门禁；建议 reviewer 在 `443c6c0` 上核对 F2/F3/F4/F5 四处落点。
2. **D12 由主 Agent 同步**：`design.md` 的 D12 仍有与 F2 同款的措辞，按派单约定由主 Agent 另行同步；本 Agent 未改 `openspec/` 下的文件。若二者未同步，合同（`docs/IDENTITY_AND_AUTH_CONTRACT.md`，权威）与 design 会短暂不一致。
3. **未执行（如实记录）**：`cargo-deny`/`gitleaks` 只在 CI 生效，本地无等价物；全 workspace `cargo test --locked --workspace --all-features` 本轮未跑（按派单只跑 `-p server -p identity-auth -p core`，WP3 触点；提交前钩子的 `cargo clippy --workspace` 已覆盖全 workspace 编译）。
