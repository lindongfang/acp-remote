# Verification — daemon-cli-and-local-admin

> 主 Agent 持续维护执行证据与变更历史；结构以 `openspec/schemas/agentic/templates/verification.md` 为准。

## Target

- 变更：`daemon-cli-and-local-admin`
- 仓库：`D:\Project\acp-remote`
- 目标主分支：`refs/heads/main`
- 基线核实（任务 1.1，2026-09-25，主 Agent 直接执行机械核实）：
  - `git rev-parse refs/heads/main` → `ab62773d8768f3c6a8424f6aefa862a738482eb8`
  - `git status --porcelain` → 仅本变更目录与误建的临时文件（已删除）；无用户未提交改动
  - `git worktree list` → 单一 worktree `D:/Project/acp-remote`（main）
  - 工具链：cargo 1.98.1（`rust-toolchain.toml` channel 1.98.1）、Node v24.19.0、npm 12.0.2
  - 基线检查（在 ab62773 工作区）：`npm run check` 退出码 0（日志：`reports/baseline-npm-check.log`）；`cargo test --locked --workspace --all-features` 退出码 0（日志：`reports/baseline-cargo-test.log`，全部 crate 无失败）
- 变更分支：`feat/daemon-cli-and-local-admin`（自 ab62773 创建）

## Handoff Index

| Task ID | Role / Phase / Stage | Target Revision | Evidence Type / ID | Report Path | Result / Evidence Status | Applicability / Source Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| 1.1 | environment / recon / recon（主 Agent 机械核实） | ab62773d8768f3c6a8424f6aefa862a738482eb8 | RESOURCE / NOT_APPLICABLE | reports/baseline-npm-check.log, reports/baseline-cargo-test.log | PASS / NEW | 基线绿，见 Target 节 |
| 2.1–2.3 | coder / implement / work-package | b56b829ae2119db77a1907271a5ce799962ac10a（+报告 7c6aea2030a4108859dd3089e55e9371ae3817af） | DELIVERY / NOT_APPLICABLE | reports/wp1-handoff.md | PASS / NEW | WP1 交付；RV1 独立 review 待调度（3.2） |
| 2.3 | coder / implement / work-package | b56b829ae2119db77a1907271a5ce799962ac10a | CHECK / PV1（W0 轮，npm run check 部分） | reports/wp1-handoff.md（日志 reports/du1-pv1.log） | PASS / NEW | 退出码 0，10 道门禁全绿 |
| 2.3 | coder / implement / work-package | b56b829ae2119db77a1907271a5ce799962ac10a | CHECK / PV2（W0 轮） | reports/wp1-handoff.md（日志 reports/wp1-boundaries.log） | PASS / NEW | 退出码 0 |
| 2.3 | coder / implement / work-package | b56b829ae2119db77a1907271a5ce799962ac10a | CHECK / PV1 附加（npm run verify 全量） | reports/wp1-handoff.md（日志 reports/wp1-verify.log） | PASS / NEW | 513 passed / 0 failed / 2 ignored（RV1-WP1-F1 修正后的日志实数），与基线逐项等价 |
| 3.1 | coder / project-verify / work-package | b56b829ae2119db77a1907271a5ce799962ac10a | CHECK / PV1、PV2（WP1 交付前轮次） | reports/wp1-handoff.md（日志 reports/wp1-verify.log、reports/du1-pv1.log、reports/wp1-boundaries.log） | PASS / NEW | 与 2.3 同批证据；在 WP1 最终工作树上执行 |
| 3.2 | reviewer / work-package review / work-package | b56b829ae2119db77a1907271a5ce799962ac10a | REVIEW / RV1 | reports/rv1-wp1.md | PASS / NEW | 隔离子 Agent（kimi-coding/k3）；无 CRITICAL/MAJOR；F1–F4 处理见 Review Findings |
| 2.4–2.6 | coder / implement / work-package | 6361f5d2a34d9f6306f62b41c852749e7d86bc4d（+报告 d9c7c6e） | DELIVERY / NOT_APPLICABLE | reports/wp2-handoff.md | PASS / NEW | WP2 交付；RV1（3.4）待调度 |
| 2.4–2.6 | coder / implement / work-package | 6361f5d2a34d9f6306f62b41c852749e7d86bc4d | CHECK / PV5（WP2 部分） | reports/wp2-handoff.md（日志 reports/pv5-windows-ipc.log） | PASS / NEW | 11 passed / 0 failed；跨用户拒绝以 SDDL 文本静态证据替代（本机单账号，已如实记录） |
| 2.7–2.9 | coder / implement / work-package | c12957ee3c4ffdcab9db8533d23b42e4673107ba（+报告 3390e9cc5688d667a9b2c4e4a1d9230873ca3297） | DELIVERY / NOT_APPLICABLE | reports/wp3a-handoff.md | PASS / NEW | WP3a 交付；RV1（3.6）待 WP3b 后调度 |
| 2.7–2.9 | coder / implement / work-package | c12957ee3c4ffdcab9db8533d23b42e4673107ba | CHECK / PV3（WP3a 部分） | reports/wp3a-handoff.md（日志 reports/wp3-server-transport.log、reports/wp3-server-envelope.log、reports/wp3-verify.log） | PASS / NEW | 53 passed / 0 failed / 0 ignored；fmt/clippy（含 Linux 目标）零告警 |
| 2.8 | coder / implement / work-package | c12957ee3c4ffdcab9db8533d23b42e4673107ba | CHECK / PV5（WP3a 部分） | reports/wp3a-handoff.md（日志 reports/pv5-windows-ipc.log） | PASS / NEW | 真实 Named Pipe 4 用例通过（sid_hash 已记录）；跨用户拒绝本机不可构造，如实登记 |
| 2.7–2.9 | coder / implement / work-package | c12957ee3c4ffdcab9db8533d23b42e4673107ba | CHECK / PV2（WP3a 轮） | reports/wp3a-handoff.md（日志 reports/wp3-server-boundaries.log） | PASS / NEW | 11 个 crate 全登记；server 唯一工作区内依赖边 = windows-local-ipc |
| 3.4 | reviewer / work-package review / work-package | 6361f5d2a34d9f6306f62b41c852749e7d86bc4d | REVIEW / RV1 | reports/rv1-wp2.md | PASS / NEW | 隔离子 Agent（deepseek/deepseek-flash）；无 P0/P1；3 条 P2 处理见 Review Findings；重派前的失败运行见 Failures and Retests |
| 2.10, 2.12 | coder / implement / work-package | f1a3cd4（+报告 4ec6bf3；另 fee6073 含中间版本） | DELIVERY / NOT_APPLICABLE | reports/wp3b1-handoff.md | PASS / NEW | WP3b1 交付；RV1 待 2.11/2.21 后并入 3.6 |
| 2.10, 2.12 | coder / implement / work-package | f1a3cd4 | CHECK / PV3（WP3b1 部分） | reports/wp3b1-handoff.md（日志 reports/wp3-server-methods.log） | PASS / NEW | 91 passed / 0 failed / 0 ignored；fmt/clippy（Win + Linux 目标）零告警；npm run verify 697 passed |
| 2.21 | coder / implement / work-package | 1cd7a3b（主 Agent 并发提交，含 2.21 全部 12 个文件；后接 dfb1c99 vendor 修复与 8dd24e4 报告） | DELIVERY / NOT_APPLICABLE | reports/wp321-handoff.md | PASS / NEW | 正则三处一致；drift 25 项；fixtures 118 valid/24 invalid；F1/F2 已修（dfb1c99） |
| 2.21 | coder / implement / work-package | dfb1c9972aad35e3bbb880e1e9ecfbe01a696257 | CHECK / PV1、PV2（2.21 轮） | reports/wp321-handoff.md（日志 reports/wp321-contract.log、reports/wp3-server-methods.log） | PASS / NEW | `npm run check` exit 0（显式 EXIT 行）；`cargo test -p server` 93 项
| 2.11, 2.13 | coder / implement / work-package | eef75f4（+测试 9b30ab4；报告 dbebd0c） | DELIVERY / NOT_APPLICABLE | reports/wp3b2-handoff.md | PASS / NEW | WP3b2 交付；RV1（3.6）待 2.23 后调度 |
| 2.11, 2.13 | coder / implement / work-package | eef75f4 | CHECK / PV3（WP3b2 部分） | reports/wp3b2-handoff.md（日志 reports/wp3-server-pairing.log、reports/wp3-server-methods.log、reports/du1-pv1.log） | PASS / NEW | 112 passed（新增 19）；fmt/clippy（Win+Linux 目标）零告警；npm run verify 627 passed |
| 2.23 | coder / implement / work-package | 313d2a2（含 0968c7a；报告 reports/wp323-handoff.md） | DELIVERY / NOT_APPLICABLE | reports/wp323-handoff.md | PASS / NEW | 三个 revoke 入口的前置读取 + `local.not_found`；fake 保真修正使断言非退化；RED 证据已留 |
| 2.23 | coder / implement / work-package | 313d2a2 | CHECK / PV3（2.23 轮） | reports/wp323-handoff.md（日志 reports/wp3-server-methods.log、reports/du1-pv1.log） | PASS / NEW | 114 passed；fmt/clippy 零告警；`npm run check` EXIT=0 |
| 3.6 | reviewer / work-package review / work-package | 313d2a2 | REVIEW / RV1 | reports/rv1-wp3.md | PASS / NEW | 隔离子 Agent（deepseek/deepseek-flash）；C1–C10 静态核对；无 CRITICAL/MAJOR；F1/F2/F3/F6 交 2.24，F4/F5 已由主 Agent 收敛措辞 |
| 3.5 | main（代行 coder 的 Project Verify）/ project-verify / work-package | 313d2a2 | CHECK / PV1、PV2、PV3、PV5（WP3 最终轮） | reports/wp3-final-verify.log、reports/pv5-windows-ipc.log | PASS / NEW | `npm run verify` EXIT=0（75 个测试块全 ok）；合同门禁全绿；Windows IPC 用例 EXIT=0 |
| MD2（AuditStore 缺口） | main / plan-review / work-package | f1a3cd4 | DELIVERY / NOT_APPLICABLE | 《本行自身》 | BLOCKED / PENDING | `storage-sqlite` 缺 `AuditStore` 生产实现（已核实）；新增任务 2.22 处理，完成后关行 |
| MD1（WP3a 移交的契约不一致） | main / design-review / work-package | c12957ee3c4ffdcab9db8533d23b42e4673107ba | DELIVERY / NOT_APPLICABLE | reports/wp3a-handoff.md（契约问题①②③④节） | BLOCKED / PENDING | ① `node.rotate-key.begin` 与 §4 正则/schema 词表不一致：**用户已裁决选项 a**（新增任务 2.21 原子交付契约补齐 + Rust 变体），本行待 2.21 完成后关；②nil UUID 哨兵、③message 回显方法名、④Unix 凭据仅 Linux/Android——已接受并登记（见 Check Plan Changes） |

## Checks

| Check ID / Stage / Work Package | Revision / Base | Scope | Executor | Command / Steps | Environment | Result / Exit Code | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| baseline / recon / 全变更 | ab62773 | 合同门禁基线 | 主 Agent | `npm run check` @ 仓库根 | Node v24.19.0 | PASS / 0 | reports/baseline-npm-check.log |
| baseline / recon / 全变更 | ab62773 | cargo 测试基线 | 主 Agent | `cargo test --locked --workspace --all-features` @ 仓库根 | cargo 1.98.1, Windows x64 | PASS / 0 | reports/baseline-cargo-test.log |

## Check Plan Changes

- 2026-09-25（WP1 交付后）：**§5 `storage-sqlite` 列的归属调整**。原值：WP1 收敛 §5 缺列注记、WP5 收口状态。新值：WP1 已新增 `server`/`app`/`windows-local-ipc` 三列并在 §5 注记写明后果；`storage-sqlite` 列必须在 `crates/app` 加入 `members` 的同一提交补齐（否则 `check:boundaries` 对 `app → storage-sqlite` 硬失败），因此 **WP4 的写范围扩至包含 `docs/MODULE_ARCHITECTURE.md` §5 矩阵一处**（仅新增 `storage-sqlite` 列与 `app` 行对应格，不动其他内容），WP5 仍负责状态收口。理由：门禁只校验实际 path 依赖边，列与成员加入必须原子。风险覆盖：该调整不改变需求与设计语义；影响任务 2.14（写范围）。依据：WP1 handoff（`reports/wp1-handoff.md`）开放问题第 1 项与 §5 注记。
- 2026-09-25（WP1 交付后）：**WP2 附带 `.gitignore` 维护**。依据：WP1 handoff 开放问题第 2 项——vendor crate 独立构建会在 `vendor/windows-local-ipc/` 下产生 `Cargo.lock` 与 `target/`，根 `.gitignore` 不覆盖；WP2 写范围因此包含根 `.gitignore`（仅新增 vendor 条目）。
- 2026-09-25（用户裁决选项 a，WP3a 报告契约问题①）：**`node.rotate-key.begin` 进入 v1 方法词表**。原值：§4 方法名正则与 `envelope.schema.json#/$defs/methodName`（enum 24 项、pattern 不允许连字符）与 §5.7/§5.4 的连字符名字矛盾，该调用实际被判 `local.invalid_request`。新值：pattern 放宽为段内允许连字符（`^[a-z][a-z0-9]*(\.[a-z0-9]+(-[a-z0-9]+)*)*$`），enum 增至 25 项（含 `node.rotate-key.begin`），实现按 §5.7 返回 `local.unsupported`；同步 `docs/LOCAL_ADMIN_PROTOCOL.md` §4 表格、§8 版本注记、`fixtures/local-admin/v1/`（新增一条 valid 与一条 invalid）与 `scripts/check-command-catalog.mjs` 的方法标题正则（原正则同样排除连字符，不改则集合比对必然失败，已核实）。`commands.json` 的 `localCapabilities` 不变（`local.node.rotate-key` 已登记）。新增任务 **2.21**（契约与 Rust 枚举同批原子交付，避免分支上 drift 测试变红）；`npm run check` 必须全绿。**规划资产同步**：本变更 `specs/local-admin-methods/spec.md` 的 method 正则同步为新取值（错误码正则不变）；另外 §5.7 原先用 h3 标题且名称叠在标题里，门禁只认 `#### \`name\``，因此 2.21 按 §5.6 同款结构改为 `### 5.7 明确推迟（\`local.node.rotate-key\`）` + `#### \`node.rotate-key.begin\``（三条 bullet 语义逐字不变；主 Agent 已批准），门禁保持单一条正则。依据：用户在 2026-09-25 本会话对选项 a 的答复（原话「a」）。
- 2026-09-25（WP3a 交付后，主 Agent 接受）：**三处实现细节登记为已接受**，不改契约：② 信封 `id` 缺失/非法时以 nil UUID 哨兵回 `local.invalid_request`（§4 规则 2「响应必带相同 id」与规则 4「非法信封回错误」在 id 不可用时无法同时满足；哨兵是最小偏差且已写入代码注释）；③ `local.unsupported` 的 `error.message` 回显方法名（仅方法名、≤512、不含 params 与路径）；④ Unix 对端凭据校验收窄到 Linux/Android（其它 Unix 目标每次连接失败关闭，避免 macOS 编译失败；与 §2.2 写明的 `SO_PEERCRED` 口径一致）。三者在 WP5 文档收口时如需写回合同，再单独提出。
- 2026-09-25（WP3b1 交付后，主 Agent 核实）：**新增任务 2.22：`storage-sqlite` 实现 `AuditStore`**。原因：`core::UseCases` 的 `UseCaseDeps.audit: Arc<dyn AuditStore>` 是必需字段，`audit.export`（任务 2.12，本变更范围内）靠 `AuditStore::query` 读 `owned_audit`，但工作区里只有 core 测试内的 `TestAudit`，生产实现缺失（已核实：`crates/storage-sqlite/src/admin/` 只有 export/local_config/trust 三个 impl；`owned_audit` 表与 `insert_audit_rows` 已存在）。写范围：`crates/storage-sqlite/src/admin/audit.rs`、`crates/storage-sqlite/src/admin/mod.rs`；归属 WP4 波次（在 2.14 之前），检查 PV1/PV2 + 该 crate 测试。影响：任务 2.22（新增）；需求未变。
- 2026-09-25（WP3b1 待澄清项裁定，主 Agent）：② `agent.configure` 保留既有 `ProviderEnvBinding`——**接受**（合同 §5.2 的 `params` 不含绑定字段，保留既有绑定是正确行为）；③ keystore 条目命名约定（`keystore_ref = sha256(providerId)[..16]@v<version>`、标签 `<ref>.<field>`）需写回权威文档 `IDENTITY_AND_AUTH_CONTRACT.md` §7 与 `CORE_PORTS_AND_STORAGE.md` §11.6——**并入 WP5 文档收口**；④ 错误消息回显标识符类输入（参数名/方法名/类别名）——与 WP3a 同口径，接受；⑤ 三处收窄（`values` 非空、嵌套 closed、alias 用 node-link 同类正则）——接受（经核实 node-link 的 `workspaceAlias` 与文档 §5.2 正则**完全相同**，无行为差异）；⑥ §7「方法失败」无 §14.2 对应类别——本轮结构化日志 + 保留接线点，接受并登记；①AuditStore 缺口见上条新增任务。
- 2026-09-25（RV1-WP1 后）：**WP5 写范围增加两处 MINOR 修复**——`scripts/check-crate-boundaries.mjs` 的过期注释（RV1-WP1-F2）与 `docs/MODULE_ARCHITECTURE.md` §5 注记补「`windows-local-ipc` 依赖面靠人工 review 约束」（RV1-WP1-F3）。两者均非行为变化；影响任务 2.19（写范围）。
- 2026-09-25（2.21 交付后）：**WP5 写范围再增一处文档 bug**——`schemas/local-admin/v1/README.md:13` 引用了不存在的 `scripts/check-local-admin-contract.mjs`（本地管理词表实际由 `scripts/check-command-catalog.mjs` 断言）；属既有问题，2.21 未改，归 WP5 顺手修正。
- 2026-09-25（WP1 交付后）：**WP4 调用点约束传递**——固定工具链 1.98.1 下 `std::fs::File::try_lock`（1.89 稳定）与 `fs4::FileExt` 同名且优先级更高，WP4 必须全限定调用 `fs4::FileExt::try_lock`，否则等于把 MSRV 抬到 1.89（依据：WP1 handoff 开放问题第 3 项，已写入 `MODULE_ARCHITECTURE.md` §3.1）。

- 2026-09-25（WP3b2 裁决，主 Agent）：**配对 URL 的 origin 来源与 QR payload 组装方式**。D1：`public_origin: Option<String>` 由 WP4 组合根注入 `PairingSessions::new`（server 不读配置、不经 `DaemonControl` 取值）；`None` 时 `device.pair.begin`/`node.pair.begin(owner)` 在**任何副作用之前**失败关闭并回 `local.unavailable`（§6 无「配置缺失」专用码，语义最贴近；成因需在 message 与注释中说明）。D2：`crates/server` 新增 `sync-protocol` + `node-link-protocol` 生产依赖（§5 矩阵允许），用两边的 `QrPayload` 组装 `#data=`，不手写 wire 键名，并补往返测试；若协议 crate 有既有 URL helper 优先使用，否则用 base64 `URL_SAFE_NO_PAD`（server 不引入 `acpr-wire`）。影响：WP4 的 `PairingSessions` 构造签名 + server 的两条依赖边 + `Cargo.lock`。依据：WP3b2 实现者的 D1/D2 提问与本会话裁定。
- 2026-09-25（WP3b2 交付后，主 Agent 裁定）：**五条开放项处置**。(1) `*.revoke` 重试语义与 §7 冲突——**实现侧修正**（已核实 `export.rs:401`/`trust.rs:519`/`trust.rs:552` 均为 `COALESCE(revoked_at, ?)`；新增任务 2.23：三个 revoke 入口先读、不存在或已撤销 → `local.not_found`）。(2) 待澄清项作废：`PairingRecord` 本身持久化 `requested_scopes`/`requested_grants`（`core/src/model/identity.rs:440-441`），因此 status 返回的就是登记的请求集合，**不是偏离**。(3) `consumed → "approved"`、设备 `pending_confirmation → "claimed"` 的字面量映射——在 §5.3/§5.4 的封闭词表内取值，接受。(4) `preset.*` 在 `device.pair.begin` 被拒——与 §5.3「取值限于 `pack.*` 与命令名集合」的字面表述一致，接受（展开由 CLI 承担）。(5) `window::add_millis` 与 `storage-sqlite` 的日期算术重复——core 未暴露时间工具，接受并登记为已知重复（不新增跨 crate 依赖）。

- 2026-09-25（RV1-WP3 后，主 Agent）：**F4/F5 契约措辞收敛**（不改语义）——`docs/LOCAL_ADMIN_PROTOCOL.md` §7 的「审计与日志」条改为「拒绝连接 + §14.2 类别覆盖的安全动作 + 撤销记审计；方法失败只记结构化日志」（§14.2 封闭词表无「方法失败」，不得复用 `authorization.denied`）；`specs/local-admin-methods/spec.md` 场景 WHEN 列表同步。`specs/local-admin-channel/spec.md` 的 attachment MUST 句加「facade 落地后」范围限定并指向 §3.1 实现状态注记。影响：无行为变化，仅合同/规范措辞与实现口径对齐。新增任务 2.24（F1/F2/F3/F6 实现侧修复）。

## Dependency Handoffs

| Downstream | Upstream | Accepted Revision / Evidence | Transfer / Inclusion Check | Invalidation |
| --- | --- | --- | --- | --- |
| WP3 | WP2 | `6361f5d2a34d9f6306f62b41c852749e7d86bc4d`：冻结 API 三函数（`create_pipe_server`/`current_user_sid`/`client_user_sid`，均 `cfg(windows)`，`io::Result`）+ `reports/wp2-handoff.md`（11 用例通过，`cargo check --target x86_64-unknown-linux-gnu` exit 0，根 `npm run check` 绿） | `server` 以 path 依赖只调用三个公开函数；wrapper 非 Windows 为空 crate，server 调用点必须 cfg-gate | wrapper API 形状变化 → WP3/WP4 复验 |

备注：WP2 移交的未核实项——同名后续实例是否继承首实例的受限 DACL（需 `GetSecurityInfo` 或跨用户用例），列入 WP3 的 [PV5] 范围。

## Runtime Resources

- 本变更不涉及数据库服务、容器、端口、外部账号或网络资源（依据：仅本机 IPC 与临时目录；见 plan.md「Runtime Resources」说明行）。
- 已登记隔离方案：集成用例临时 Daemon 数据目录（用例自建自删）；并行执行者各自 `CARGO_TARGET_DIR`（本变更默认串行，暂无并行占用）；[PV5] Windows IPC 用例串行轮次。

## Review Findings

| ID | Revision | Reviewer | Location | Severity / Impact | Resolution | Recheck Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| RV1-WP1-F1 | b56b829 | RV1-WP1（隔离子 Agent） | reports/wp1-handoff.md | MINOR：handoff 总数 515 与日志实数 513 不符 | 主 Agent 已按日志实数（513 passed / 0 failed / 2 ignored）登记本文件；不阻断 | 不适用（记录修正） |
| RV1-WP1-F2 | b56b829 | 同上 | scripts/check-crate-boundaries.mjs 内嵌注释 | MINOR：注释仍写「9 列」与现状漂移；门禁行为不受影响 | 并入 WP5 写范围同步该注释（Check Plan Changes 已登记） | WP5 交付时由 RV1-WP5 复核 |
| RV1-WP1-F3 | b56b829 | 同上 | docs/MODULE_ARCHITECTURE.md §5 | MINOR：`windows-local-ipc` 作为发起方的依赖边不受门禁强制 | 接受缺口并在 WP5 于 §5 注记补「该 crate 依赖面靠人工 review 约束」一句（Check Plan Changes 已登记） | WP5 交付时由 RV1-WP5 复核 |
| RV1-WP1-F4 | b56b829 | 同上 | docs/LOCAL_ADMIN_PROTOCOL.md 头部 | SUGGESTION：状态/版本行略超字面写范围 | 主 Agent 确认：头部状态行视为 §3.1 注记的载体，不返工 | 不适用 |
| RV1-WP2-F1 | 6361f5d | RV1-WP2（隔离子 Agent） | vendor/windows-local-ipc/Cargo.toml:32 | MINOR：注释称 `fs4`/`clap` 引入 windows-sys 0.61.2，实际由 `mio`/`tokio` 引入 | 并入任务 2.21 顺手修复（一行注释） | 2.21 交付时由 RV1-WP3 复核 |
| RV1-WP2-F2 | 6361f5d | 同上 | vendor/windows-local-ipc/Cargo.toml:16 | MINOR：`license = "MIT OR Apache-2.0"`，但仓库仅含 Apache-2.0 正文 | 并入任务 2.21：改为 `license = "Apache-2.0"` | 2.21 交付时由 RV1-WP3 复核 |
| RV1-WP2-F3 | 6361f5d | 同上 | reports/du1-pv1.log、reports/pv5-windows-ipc.log、wp2-handoff.md | MINOR：两个 [PV1] WP2 轮未记退出码；fmt 无运行记录 | 主 Agent 已补录：`du1-pv1.log` 追加 `EXIT(npm run check)=0` 轮次；`pv5-windows-ipc.log` 追加 `EXIT(cargo fmt -- --check)=0`（均真实执行） | 追加日志已核实 |
| RV1-WP3-F1 | 313d2a2 | RV1-WP3（隔离子 Agent） | crates/server/src/local_admin/router.rs:461-470 | MINOR：`import.remove` 缺「未知合法 id」常驻断言 | 并入任务 2.24 | 2.24 交付后由 RV2 复核 |
| RV1-WP3-F2 | 313d2a2 | 同上 | crates/server/src/local_admin/test_support.rs:493-505 | MINOR：`FakeExports::put_export` 比真实存储更严（未撤销的既有 id 在存储里是覆盖 `Ok`）⇒ `export.create` 查重断言无但例能力 | 并入任务 2.24（改 fake 同形 + RED 验证） | 同上 |
| RV1-WP3-F3 | 313d2a2 | 同上 | crates/server/src/transport/local/platform/windows.rs:67-80 | MINOR：`connect()`/补建失败时 `pending` 保持 None ⇒ endpoint 之后永久不可用（与代码自述不变量冲突） | 并入任务 2.24（失败时重建 pending 或先补建再 connect） | 同上 |
| RV1-WP3-F4 | 313d2a2 | 同上 | docs/LOCAL_ADMIN_PROTOCOL.md:594 + spec 场景 WHEN | MINOR（契约措辞）：§7 要求「方法失败」入审计，但 §14.2 封闭类别无此类别 | **主 Agent 已收敛**：§7 改为「方法失败只记结构化日志」并说明不得复用其他类别；spec 同步 | 已改由 workflow check --stage plan 复验（PASS） |
| RV1-WP3-F5 | 313d2a2 | 同上 | specs/local-admin-channel/spec.md（MUST 句） | MINOR（契约措辞）：`0x02` 立即分配 `FacadeAttachmentId` 在本切片无可行路径 | **主 Agent 已收敛**：MUST 句加「facade 落地后」限定并指向 §3.1 注记 | 同上 |
| RV1-WP3-F6 | 313d2a2 | 同上 | params.rs:317-326、envelope.rs:255-262 | SUGGESTION：`ProviderConfigure` 的 Debug 可打印明文凭据；两处不可达分支 | 并入任务 2.24（手写 Debug + 清理/兜底不可达分支） | 同上 |
| RV1-WP2-R1（残余风险） | 6361f5d | RV1-WP2 | crates/server/src/transport/local/platform/windows.rs（后续实例） | 风险（非缺陷）：后续实例 DACL 是否继承首实例未证实 | 已登记入 Dependency Handoffs；[PV5] 在 WP3b/WP4 用 `GetSecurityInfo`/跨账号用例钉死 | 待 [PV5] |

## Merge History

（合入前记录唯一 agentic-premerge 块。）

## Test Design and Authoring

不适用（Main E2E mode = not-applicable）。

## Candidate E2E

不适用（mode = not-applicable）。

## Main E2E

- 项目开关：`npx --quiet --no-install openspec-agentic e2e --json` 实测 `enabled=true`、`command=""`、`maxAttempts=3`。
- mode = `not-applicable`；reason/basis/alternative_checks/downgrade_approval 见 `plan.md` 的 Main E2E 块（2026-09-25 本会话用户原话「1. 同意降级」）。
- 替代检查在上方 Checks 表逐项留证（执行后填入）。

## Failures and Retests

| Issue ID / Task | Source / Check or E2E ID / Attempt / Version | Owner | Fix / Recovery | Review / Readiness Evidence | Retest Evidence | Current Status / Basis |
| --- | --- | --- | --- | --- | --- | --- |
| PRO-1（主 Agent 流程异常） | commit `fee6073`（登记 RV1-WP2 记录时误用 `git add -A`，把仍在运行的 WP3b1 部分工作树一并提交） | 主 Agent | 未做历史改写（子 Agent 同 worktree 并发工作，改写有风险）；已 steer 通知 WP3b1、要求其交付提交用显式路径；后续主 Agent 提交一律用显式路径 | 无（不影响代码内容与门禁：`npm run check`/fmt/clippy 均绿） | WP3b1 交付后核对其 handoff 的提交组成与最终 `git status` | 已登记；待 WP3b1 交付时核实完成组成 |
| PRO-2（主 Agent 流程异常，同一根因） | commit `1cd7a3b`（我提交 spec/证据时，并发暂存的 2.21 十二个文件被一并带入；提交信息未提及契约补齐） | 主 Agent | 不做历史改写；自本次起主 Agent 一律用 `git commit --only <paths>`；已在 WP3b2 任务单里明确要求实现者只 add 自身路径 | 无（内容完整：正则三处一致、drift 25 项、fixtures 118/24；门禁与测试全绿） | WP3b2/3.6 复核时会核对 1cd7a3b 的完整组成 | 已登记；`1cd7a3b` 为 2.21 的权威交付提交（内容层面） |
| RV1-WP2-RUN1 | 任务 3.4，reviewer 子 Agent 运行 `ecc2d081-dfcb-41eb-b845-800aceb7512d`（目标 6361f5d）；运行在回报前中止，只输出了开场句，未产生报告 | 主 Agent | 原因判定为子 Agent 运行时中断（非产品/证据问题：目标提交与输入未变）；已用 deepseek/deepseek-flash 重新派发 RV1-WP2 | 旧运行无报告产出，未作为结论 | 待重派结果 | 未解决（等待重派；不影响 WP3a 进行） |

## Final Assessment

（最终验收时填写。）
