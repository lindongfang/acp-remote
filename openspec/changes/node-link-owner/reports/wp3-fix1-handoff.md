# WP3 fix1 Handoff — RV1-WP3C 的 F2/F3/F5 + 安全头覆盖（`node-link-owner` / DU1）

## Shared Report

- **task_id**: `RV1-WP3C-F2` / `RV1-WP3C-F3` / `RV1-WP3C-F5` + 「安全头覆盖」（主 Agent 裁决项①；对应 review 报告 `reports/rv1-wp3c.md` 的 Findings 与 WP3 handoff 的「不确定项」第 1 条）
- **role**: coder；**phase**: fix；**stage**: work-package；**evidence_id**: `RV1-WP3C`
- **agent_context**: 任务级 coder 子 Agent（fix 轮次），**不继承** WP3 实现会话，只按主 Agent 下发的 fix 清单、`rv1-wp3c.md` 的 Findings 原文与 `wp3-handoff.md` 作业。工作目录 `D:\Project\acp-remote-wt\node-link-owner`（worktree，分支 `agentic/node-link-owner`）；权威规划根 `D:\Project\acp-remote`（报告与日志写在该处 `openspec/changes/node-link-owner/reports/`，该目录不入版本控制）。工具集含 shell 与 Git：本轮的 diff、红向判别力实测与门禁均由本 Agent 亲自执行。
- **target_revision**: `d127a802b75d63cd14789ea3ad504d89ce57f3f3`（本轮 fix 唯一提交）；被检视的 base = `84678e4b98250ca8e0fe302051d5d3072f1a7397`（WP3 交付提交，开工时 HEAD 与之一致，工作区干净），两者为单提交之差，提交后 `git status --porcelain` 为空。
- **scope**（严格按派单的允许写入面，**无越界**）：
  - `crates/server/src/transport/net/**`：`route.rs`（每路径默认响应头 seam 的实现）、`listener.rs`（`register_post` 签名与 `serve` 的装配）、`tests.rs`（12 个既有调用点补 `&[]` + 3 个新用例 + 文件头场景表一行）；**未改** `mod.rs`/`host.rs`/`proxy.rs`/`http.rs`/`ws.rs`/`ratelimit.rs`/`tls.rs`/`permissions.rs`/`shutdown.rs`/`config.rs`/`test_client.rs`。
  - `crates/server/src/node_link/**`：`pairing.rs`（四个安全头的单一定义 + `default_response_headers` + `secure` 改为同源循环）、`mod.rs`（注册纪律的文档一句）、`tests.rs`（Harness 按生产口径注册 + Host 可指定的发送辅助 + 1 个新用例 + 覆盖表一行）。
  - `crates/storage-sqlite/tests/migration.rs`：**仅**模块头注释（F5）。
  - `docs/CORE_PORTS_AND_STORAGE.md`：**仅** §3.5 的 `Actor` 行（F2）与 §7.2 的 v1→v2 步骤 4（F3）。
  - **未做**：不改 `plan.md`/`tasks.md`/`verification.md`/`proposal.md`/`design.md`、`openspec/specs/**`、`schemas/**`、`compatibility/**`、`fixtures/**`、`crates/app/**`（WP7）、`crates/server/tests/**`、`docs/MODULE_ARCHITECTURE.md`（见「残留风险」第 2 条）。
- **changes**（1 个提交，8 个文件，365 insertions / 45 deletions）：
  1. **安全头覆盖（裁决项①，行为）**：`net/route.rs` 新增每路径默认响应头 seam——`register_post` 增加第三个参数 `default_headers: &[(&str, &str)]`，注册时解析（`HeaderName`/`HeaderValue`，非法即 `RouteError::InvalidDefaultHeader`）；该 path 的 `MethodRouter` 外层加一层 `from_fn`，把默认头**补齐**到该 path 的**每条**响应（处理器响应、请求体超限的 413、方法不匹配的 405）；Host 边界在路由之前的 400 由 `boundary` 按 `request.uri().path()` 查同一张表补齐（新增 `NetState::post_default_headers`）。`net/listener.rs` 的 `register_post`/`serve` 随动。`node_link/pairing.rs` 把四个安全头收成一个 `SECURITY_HEADERS` 常量，`secure()` 与新的 `PairingHttp::default_response_headers()` 同源消费；`node_link/tests.rs` 的 Harness 按生产口径把该值传给 `register_post`。
  2. **RV1-WP3C-F3（文档精度）**：`CORE_PORTS_AND_STORAGE.md` §7.2 的 v1→v2 步骤 4 改为「这一段**不单独落盘版本**：两个 schema 版本键与 `PRAGMA user_version` 都由同一事务内紧随其后的 v2 → v3 段在全部表重建结束后统一写为 `'3'`/`3`」；该条的小标题从「四步都在同一事务内」改为「都在同一事务内」。与 `crates/storage-sqlite/src/migrate.rs` 的 `migrate()` 逐行核对一致（中间无任何 `'2'`/`user_version = 2` 的写入）。
  3. **RV1-WP3C-F5（注释歧义）**：`migration.rs` 模块头注释的「把它的临时副本顶到 `FILE_FORMAT_VERSION + 1`」改为「用 `empty.sqlite3` 的临时副本判定（把该副本的 `user_version` 顶到 `FILE_FORMAT_VERSION + 1`）」，与用例 `too_new_database_is_rejected_without_writing_rows` 的实际输入（`copy_fixture("empty.sqlite3", …)`）一致。
  4. **RV1-WP3C-F2（合同钉死）**：`CORE_PORTS_AND_STORAGE.md` §3.5 的 `Actor` 行写明 `Node` 的 `node` 是**对端（claimant／连接对端）节点 id**——Owner 侧即 claim 里声明的 `accessNodeId`，Access 侧即本地 Import 的 `ownerNodeId`；配对消费与握手的对端比对（`TrustStore::consume_pairing`、`PairingConsumption`）只比它，`AuditRecord.via_node` 与 broker 判定 Import 授权时的 `ImportRecord.owner_node_id` 同一口径；`access_node` 的既有含义不变。
- **checks**（原始输出见 `wp3-pairing-http.log` 的「wp3-fix1 轮次」§1–§9）：
  - `cargo fmt --all -- --check` exit=0。
  - `cargo clippy --locked -p server --all-targets --all-features -- -D warnings` exit=0（无 warning）；另跑 `cargo clippy --locked -p storage-sqlite --all-targets --all-features -- -D warnings` exit=0（注释改动所在测试目标的编译核对）。
  - `cargo test --locked -p server --all-features` 全绿：lib **193**（WP3 的 189 + 4 个新用例）、集成目标 14/6/4/**0**（`cfg(unix)`，本机不编译）/4/0，0 failed / 0 ignored。
  - `npm run check` exit=0（10 道；`check:drift` 报「§7 的 36 条 DDL 与 §5 的 15 个 trait / 88 个方法签名逐条一致」——§7.2 的 `[决定]` 正文不属于被逐条绑定的 DDL 文本，本轮改写未触碰 DDL）。
  - **红向判别力实测**：临时把 `apply_default_headers` 改成空跑后重跑 server 测试 → `access_layer_rejections_on_the_pairing_paths_carry_the_security_headers`、`path_default_headers_cover_access_layer_rejections_and_keep_route_isolation`、`path_default_headers_do_not_override_or_duplicate_handler_headers` **FAILED**（190 passed / 3 failed），还原后 193 全绿。注册期校验用例（`invalid_path_default_headers_are_rejected_at_registration`）与空跑无关，因此两轮都 ok。
  - 提交前钩子（`.husky/pre-commit`）三步全通过：fmt → `npm run check`（16 passed / 0 failed）→ `cargo clippy --workspace --all-targets --all-features`；commitlint 通过（`fix(server): …`）。
- **issues**: 四项全部落地，无阻断。需要下游知晓的判断与影响见「残留风险」。
- **result**: **PASS**（本轮 fix 范围内全部适用条件满足、门禁全绿；**不代表**独立复验（RV2）、WP4/WP7 的接线、E2E 或合并已完成）
- **evidence_paths**:
  - 本报告：`openspec/changes/node-link-owner/reports/wp3-fix1-handoff.md`
  - 被处理的 review：`openspec/changes/node-link-owner/reports/rv1-wp3c.md`（F2/F3/F5）；被修的 WP3 交付与接线口径：`reports/wp3-handoff.md`
  - 本轮原始输出：`openspec/changes/node-link-owner/reports/wp3-pairing-http.log` 的「wp3-fix1 轮次」（§0 工作区、§1 fmt、§2 clippy(server)、§3 clippy(storage)、§4 server 测试、§5 新用例点名、§6 `npm run check`、§7 红向判别力、§8 提交与工作区、§9 提交 SHA）
- **resource_cleanup**: 未新增端口、数据库、容器、证书或外部账号；未安装依赖、未改 `Cargo.toml`/`Cargo.lock`。临时资源：三次 `route.rs` 的 `/tmp` 备份（红向判别力实验用，每次都立即还原并用 `grep -c ACPR_RED_CHECK` = 0 核对）；用例自建的 `TestWorld`/`TestCertificate` 临时目录由 `Drop` 删除；`target/` 作为缓存保留（被 git 忽略）。提交后 `git status --porcelain` 为空，无残留未跟踪文件、无 staged 文件。

## handoff_index

```yaml
handoff_index:
  - task_id: "RV1-WP3C-F3"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "d127a802b75d63cd14789ea3ad504d89ce57f3f3"
    evidence_type: DOC
    evidence_id: RV1-WP3C
    report_path: "reports/wp3-fix1-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "CORE_PORTS_AND_STORAGE.md §7.2 的 v1→v2 步骤 4 已与实际实现一致（两段同事务、中间不落盘、最终由 v2→v3 段统一写 '3'/user_version=3）；本轮逐行核对 crates/storage-sqlite/src/migrate.rs 的 migrate() 后判定，npm run check 的 check:drift 未受影响"
    source_evidence: "84678e4b98250ca8e0fe302051d5d3072f1a7397"
  - task_id: "RV1-WP3C-F5"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "d127a802b75d63cd14789ea3ad504d89ce57f3f3"
    evidence_type: DOC
    evidence_id: RV1-WP3C
    report_path: "reports/wp3-fix1-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "migration.rs 模块头注释改为「用 empty.sqlite3 的临时副本判定」，与用例 too_new_database_is_rejected_without_writing_rows 的 copy_fixture(\"empty.sqlite3\", …) 一致；cargo clippy -p storage-sqlite --all-targets 退 0"
    source_evidence: "84678e4b98250ca8e0fe302051d5d3072f1a7397"
  - task_id: "RV1-WP3C-F2"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "d127a802b75d63cd14789ea3ad504d89ce57f3f3"
    evidence_type: CHECK
    evidence_id: RV1-WP3C
    report_path: "reports/wp3-fix1-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "§3.5 已钉死 Actor::Node.node = 对端（claimant/连接对端）节点 id，与 use_cases::pending_audit 的 via_node、storage-sqlite/src/admin/trust.rs 的 peer_matches_actor、broker::node_allowed 的 ImportRecord.owner_node_id 比对三处口径一致；本轮为文档钉死，WP4 接线按此核对（不声称 WP4 已通过）"
    source_evidence: "84678e4b98250ca8e0fe302051d5d3072f1a7397"
  - task_id: "安全头覆盖（主 Agent 裁决项①，覆盖 R30/R31）"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "d127a802b75d63cd14789ea3ad504d89ce57f3f3"
    evidence_type: CHECK
    evidence_id: RV1-WP3C
    report_path: "reports/wp3-pairing-http.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "配对路径上接入层预拒绝的 413 与 Host-400 现在都带四个安全头：transport 层每路径默认响应头 seam + node_link 注册时声明；新增 4 个用例（1 个端到端 413/Host-400、3 个 transport 语义），红向判别力已实测（空跑后 3 个行为用例 FAILED）"
    source_evidence: "84678e4b98250ca8e0fe302051d5d3072f1a7397"
```

## 「安全头覆盖」（裁决项①）：接入层预拒绝也带四个安全头

- **派单原文要点**：spec `node-link-pairing-http` 的「安全响应头与凭据边界」要求**所有**配对 HTTP 响应（无论成功或失败）携带 `Cache-Control: no-store`/`Pragma: no-cache`/`Referrer-Policy: no-referrer`/`X-Content-Type-Options: nosniff`；但 `transport::net` 在调用处理器前产生的 413（请求体超限）与 400（Host 不匹配）不带——WP3 handoff 已把它登记为不确定项第 1 条，本轮按主 Agent 裁决实现。
- **seam 的形状（`transport::net`）**：
  - `NetListener::register_post(path, handler, default_headers: &[(&str, &str)])`：注册时声明该 path 的默认头。**改签名而不是加 `register_post_with_headers`**：同一概念只留一种拼写，且「未声明」走同一条代码路径（`&[]`）而不是旁路。12 个既有调用点（`net/tests.rs`）补 `&[]`，语义逐字不变。
  - 解析在注册期完成（`parse_default_headers`）：头名/值非法 → 新增的 `RouteError::InvalidDefaultHeader { name, value }` 拒绝注册，不留到响应期失败，也不留下半注册的路由。
  - 附加点是**两层**，覆盖面与拒绝来源一一对应：① 该 path 的 `MethodRouter` 外层 `from_fn` 层（处理器响应、413、方法不匹配的 405）；② `boundary` 中间件在 Host 拒绝时按 `request.uri().path()` 查 `NetState::post_default_headers` 补齐（Host 边界在路由之前，因此只能在中间件里按 path 查表）。
  - **语义是「补齐」而不是「追加」**：响应已带同名头时保留响应自己的值（`!headers.contains_key(name)` 才插入）。这样处理器自带的四个头不会被复制成重复头（HTTP 上重复头虽然合法，但会让「四个头各一份」的断言与实际报文脱节）。
- **`node_link` 侧的声明（单一来源）**：`pairing.rs` 的 `SECURITY_HEADERS` 常量是四个头的**唯一**定义；`secure()` 改为遍历它，新增的 `PairingHttp::default_response_headers()` 返回同一常量供注册使用。
- **WP7 接线口径变更（重要）**：`wp3-handoff.md` 的「冻结的公开形状」里 `register_post(CLAIM_PATH, pairing.claim_handler())?` 现在需要第三个参数：
  `listener.register_post(CLAIM_PATH, pairing.claim_handler(), pairing.default_response_headers())?;`（status 同）。漏传会退回旧行为（413/Host-400 不带四个头），`node_link/mod.rs` 与 `register_post`/`default_response_headers` 的文档都写明了这条要求。
- **本轮独立核对**：
  - 端到端（`node_link::tests::access_layer_rejections_on_the_pairing_paths_carry_the_security_headers`，真实 loopback listener + 自带 HTTP 客户端）：80 KiB body → 413 带四个头；`Host: evil.example.com` → 400 带四个头（claim 与 status 两个 path 都断言）。
  - transport 语义（`net/tests.rs` 三个新用例）：同一台 server 上「声明了默认头的 path（claim）」与「空切片声明的 path（status）+ 未注册 path」对照——前者的 413/Host-400/处理器响应/**方法不匹配的 405（GET）**都补齐、后者一个都不带；处理器自带同名头时保留自己的值且只有一份；非法头名/值在注册期被拒且不占用该 path。
  - 既有行为：`net/tests.rs` 的 Host/413/路由用例与 `node_link` 的 25 → 26 个用例全绿（含 `every_pairing_response_carries_the_four_security_headers` 的 201/400/401/403/404/409/410/429 断言，说明默认头没有引入重复头或改写处理器值）。

## RV1-WP3C-F3（P2，文档精度）：§7.2 的 v1→v2 步骤 4 描述了一次已不存在的中途落盘

- **发现原文要点**：`CORE_PORTS_AND_STORAGE.md:798` 的步骤 4 写「`meta.*_schema_version` 写为 `'2'`，并置 `PRAGMA user_version = 2`」，但实现里 v1→v2 段只做表重建，版本号与 `user_version` 由 v2→v3 段结束后统一写。
- **修复**：步骤 4 改为否定式的精度描述（本段不单独落盘版本；统一写 `'3'`/`3` 由下一条即 v2 → v3 段完成；因此不存在「已写 v2 版本号、表仍是 v1 形状」的中间落盘），并把该条小标题里的「四步」去掉（第 4 步不再是独立落盘动作）。
- **本轮独立核对**：读 `crates/storage-sqlite/src/migrate.rs`：升级分支内只有 `V2_UPGRADE_{OWNED,IMPORTED}`、`V3_UPGRADE_{OWNED,IMPORTED}`、`restore_audit_sequences`、`mark_schema_versions`（`INSERT ... ON CONFLICT DO UPDATE` 写当前常量 = 3），随后 `if file_version != FILE_FORMAT_VERSION { PRAGMA user_version = 3 }`；`grep` 确认 `V2_UPGRADE_*` 常量内没有任何 `meta`/`user_version` 写入。文档现在与实现逐条一致。
- **门禁影响**：`check:drift` 绑定的是 §7.3/§7.4 的 DDL 文本与 §5 的端口签名，§7.2 的 `[决定]` 正文不在逐条比对范围内；本轮 `npm run check` 仍然 exit=0 并报「36 条 DDL / 15 trait / 88 签名逐条一致」。

## RV1-WP3C-F5（P2，注释歧义）：`migration.rs` 模块头注释的「它」

- **发现原文要点**：`migration.rs:3-7` 的「过新用例改为把**它**的临时副本顶到 `FILE_FORMAT_VERSION + 1`」按上下文读作 `too-new.sqlite3`，实际用的是 `empty.sqlite3` 的副本。
- **修复**：改为「用 `empty.sqlite3` 的临时副本判定（把该副本的 `user_version` 顶到 `FILE_FORMAT_VERSION + 1`）」，并重排折行。同文件 §「`too-new.sqlite3`（`user_version = 3`）…」的夹具说明（第 236 行附近、`too_new_database_is_rejected_without_writing_rows` 的文档）本来就写明是 empty 的副本，本轮未改。
- **本轮独立核对**：用例体是 `let path = copy_fixture("empty.sqlite3", &dir);` 再 `PRAGMA user_version = FILE_FORMAT_VERSION + 1`（第 239–242 行），与注释一致。

## RV1-WP3C-F2（P2，主 Agent 裁决）：把 `Actor::Node.node` 的节点语义钉死

- **发现原文要点**：`TrustStore::consume_pairing` 的对端比对（`storage-sqlite/src/admin/trust.rs` 的 `peer_matches_actor`）与用例面的绑定校验只比 `actor.node`，但合同没写 `node` 指哪个节点 id；WP4 若接反，Owner 侧的配对消费会全失败。
- **修复（按裁决原样落地）**：§3.5 的 `Actor` 行写明 `node` = **对端（claimant／连接对端）节点 id**：Owner 侧即 claim 里声明的 `accessNodeId`，Access 侧即本地 Import 的 `ownerNodeId`；配对消费与握手的对端比对只比它；`AuditRecord.via_node` 与 broker 判定 Import 授权时的 `ImportRecord.owner_node_id` 同一口径；`access_node` 的既有含义不变（该连接的 Access 端点）。措辞沿用该行既有的行内风格（不新开段落、不改表格结构）。
- **本轮独立核对（三处口径一致）**：
  - `crates/core/src/use_cases.rs` 的 `pending_audit`：`Actor::Node { node, .. } => Some(node.clone())` → `AuditRecord.via_node` 取的就是 `node`。
  - `crates/core/src/broker.rs` 的 `node_allowed`：Access 侧 `import.owner_node_id() == node`（本地 Import 的 Owner 节点）、Owner 侧再按 Export 判定 → 「对端节点 id」在两侧都成立。
  - `crates/storage-sqlite/src/admin/trust.rs` 的 `peer_matches_actor`：`(Actor::Node { node, .. }, PeerIdentity::Node(id)) => node == id`，而配对对端行是 claimant 自己 → Owner 侧 `node` = 对方 `accessNodeId`。
- **不代 WP4 担保**：本项只钉合同与口径，握手/消费的接线、`node.authenticated` 写入与 R83 对应用例仍属 WP4（`rv1-wp3c.md` 的 F4 也归 WP4）。

## 残留风险与需要下游知晓的点

1. **WP7 接线必须带第三个参数**（见上）：`wp3-handoff.md` 的「冻结的公开形状 / WP7 接线要点」需要同步这一行；漏传只是退回旧行为、不会编译失败，因此这是本轮唯一「靠文档传递」的约束，建议主 Agent 在派 WP7 时显式写出。
2. **`docs/MODULE_ARCHITECTURE.md` §4.9 未同步**（不在本轮允许写入面）：`server::transport::net` 的职责描述里没有「每路径默认响应头」这一条。是否需要在 §4.9 补一句（或认为它属于适配器内部细节）请主 Agent 裁决；本轮把该 seam 的说明放在 `route.rs` 的模块文档、`register_post` 的文档与 `node_link/mod.rs` 的注册纪律里。
3. **413/Host-400/405 之外的接入层形态**：本轮已覆盖 405（方法不匹配，随 `MethodRouter` 外层一并补齐，两条 path 上都有断言），但**未**覆盖「未注册 path 的 404」「WS 升级路径的 400/426」——它们的 path 上没有默认头声明，与派单要求（配对路径的 413 与 Host-400）一致；若 reviewer 认为还需覆盖未注册 path（那不属于配对端点），需要新的裁决。
4. **`register_post` 的签名变更影响面**：仓库内调用点只有 `net/tests.rs`（12，已补 `&[]`）与 `node_link/tests.rs`（2，已带安全头）；`crates/app/**` 尚未注册（WP7），因此无更多兼容负担。`RouteError` 新增变体不影响既有穷尽匹配（无 `match` 消费它）。
5. **红向判别力只在「补齐动作被空跑」这一维度实测**：注册期校验与非配对 path 的隔离性由用例断言直接钉住，但没有做「删掉 seam 后编译失败」之外的反向实验；若 reviewer 需要更强的反向证据，可要求 WP3 复验轮补做。
6. **`npm run verify` 的 workspace 轮与 CI 专属判定未执行**：`cargo-deny`（`deps`/`advisories`）与 `gitleaks`（`secrets`）只在 CI 生效；`npm run verify` 的全 workspace `cargo test` 属 3.5/2.24。`#[cfg(unix)]` 的集成目标在本机（Windows）不编译，由 Linux CI 覆盖。
7. **本轮不改任何协议/端口/DDL/夹具**：`compatibility/**`、`schemas/**`、`fixtures/**`、`crates/core`、`crates/storage-sqlite/src` 全部未动；§3.5 与 §7.2 的改动是合同文本的精度修复与语义钉死，不引入新行为。

## 未执行项（不粉饰）

1. 独立复验（RV2）与 WP4/WP7 的接线未执行——本报告不声称它们通过。
2. `cargo-deny`/`gitleaks`/`npm run verify`（见残留风险第 6 条）。
3. `[PV5]` 本机轮次：本轮没有新增 `cfg(windows)`/`cfg(unix)` 分支，未重跑该轮。
