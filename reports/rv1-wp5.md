# rv1-wp5.md — WP5（合同并入 / 关联文档 / 依赖边界脚本）独立对抗性 review

## 0. 检视对象、范围与方法

- 仓库 / 分支：`D:\Project\acp-remote`，`feat/admin-state-persistence-v2`
- 被检视内容 = 固定版本 `5404610`（`git rev-parse HEAD` = `5404610d35b22c9d427b88b0cea2c61d1e7a5e15`）**加上检视期间落入工作区的 WP5 同步改动**。`git status --porcelain`（检视时刻）：
  ` M README.md`、` M docs/CONFIG_REFERENCE.md`、` M docs/CORE_PORTS_AND_STORAGE.md`、` M docs/DEVELOPMENT_PLAN.md`、` M docs/MODULE_ARCHITECTURE.md`、` M openspec/changes/admin-state-persistence-v2/verification.md`，以及无关的 `?? .omp/`、`?? .pi/prompts/opsx-verify.md`、`?? .pi/skills/openspec-verify-change/`、`?? reports/parked-idle-identity-retention.patch` 与本报告自身。
- 工作区同步轮的性质（据调度者说明并由我复核）：只把「管理状态仍待实现 / 目标形状」的陈述改成与实现一致，并修掉指向已迁走规则的 §-引用。复核方式：`node scripts/check-contract-drift.mjs` 在该工作区仍报 `§7 的 36 条 DDL…；§5 的 15 个 trait / 87 个方法签名` 且退出 0，说明被绑定断言的两处正文未变。
- 基线：`28f8cb9`（变更起点）；W0 契约基线提交：`013f2b9`；区间内提交：`013f2b9`（W0：v2 DDL + §11.5–§11.9 并入 §3/§5/§7）、`1baea5b`（WP6 三个管理 store）、`f43f7a7`（`host_binding`/节点批准）、`5404610`（撤销身份的重新配对语义）。
- 检视范围（本切片）：`git diff 28f8cb9..5404610 -- docs AGENTS.md README.md scripts/check-crate-boundaries.mjs` 的 8 个文件（652 插入 / 568 删除），加上工作区同步改动（5 份文档 19 插入 / 17 删除）：`AGENTS.md`、`README.md`、`docs/CONFIG_REFERENCE.md`、`docs/CORE_PORTS_AND_STORAGE.md`、`docs/DEVELOPMENT_PLAN.md`、`docs/IDENTITY_AND_AUTH_CONTRACT.md`、`docs/MODULE_ARCHITECTURE.md`、`scripts/check-crate-boundaries.mjs`。
- 对照的权威：`docs/CORE_PORTS_AND_STORAGE.md` 的 §2、§3、§5、§7、§9、§11；`docs/IDENTITY_AND_AUTH_CONTRACT.md`；`docs/MODULE_ARCHITECTURE.md` 的 §3.1、§4.1、§4.7、§5；`docs/CONFIG_REFERENCE.md`；`docs/LOCAL_ADMIN_PROTOCOL.md`；`docs/SECURITY_DESIGN.md`；`compatibility/**`；变更目录的 proposal/design/plan/tasks/verification 与 5 份 spec；`AGENTS.md` 的 §1、§3、§4、§9、§10、§12；`crates/core/src/model/identity.rs`、`crates/core/src/model/error.rs`、`crates/core/src/model/tests.rs`、`crates/core/Cargo.toml`、`crates/storage-sqlite/src/migrate.rs`、`crates/storage-sqlite/src/admin/`；`package.json`；`.github/workflows/ci.yml`。
- 只读命令与退出码（**在含本报告的工作区上亲自复算**）：
  - `node scripts/check-contract-drift.mjs` → 退出码 **0**：`contract drift OK: §7 的 36 条 DDL 与 crates\storage-sqlite\src\migrate.rs 逐条一致；§5 的 15 个 trait / 87 个方法签名与 crates\core\src\ports.rs 一致`
  - `node scripts/check-crate-boundaries.mjs` → 退出码 **0**：`crate boundaries OK: 6 个 crate 的依赖方向与 §5 矩阵一致（6 个已在矩阵登记）`
  - `cargo tree -p core --edges normal` → 退出码 **0**，38 个 crate 名（清单见 §1.4）
  - `node scripts/check-doc-links.mjs` → 退出码 **0**：`doc links OK: 367 relative links, … section refs across … markdown files`；`npm run check` → 退出码 **0**，末行 `Totals: 1 passed, 0 failed (1 items)`（这次复算包含本报告文件）
- **未运行**（按调度约束）：`cargo fmt` / `cargo clippy` / `cargo test`（并行构建目录争用）。本报告不声称这些门禁的结果。
- 引用但**未独立复算**的实现者证据：`reports/w0-npm-check.log`、`reports/wp4-migration-tests.log`、`reports/wp6-admin-store-tests.log`、`reports/wp5-*.log`。本报告只引用其内容，不写成「已验证」。
- 范围外：`crates/**` 的实现细节（属 WP1–WP4/WP6，另有 `reports/rv1-wp6.md` 与 `reports/rv1-wp6b.md`）；`.omp/`、`.pi/`、`reports/parked-idle-identity-retention.patch`。
- **报告自身对门禁的影响**：`reports/*.md` 在 `check:docs` 的扫描范围内，且该门禁只在该引用同一行指名了某文档时才判定其 §-引用。本报告初稿因此让 `npm run check` 变红（13 个错误全部来自本报告自身），本稿已改为「文档名紧邻 §号」的写法；定稿后的复算结果记录在 §1.5。

## 1. 逐条核对结论

### 1.1 无重复正文 —— 通过（一处规则级重叠，属计划明确保留）

围栏清点（`grep -n '^```' docs/CORE_PORTS_AND_STORAGE.md`）：

- 5 个 ```rust 块，全部在 `docs/CORE_PORTS_AND_STORAGE.md` 的 §2（第 30 行）、§5.1（第 197 行）、§5.2（第 237 行）、§5.3（第 329 行）、§5.4（第 676 行）；
- 恰好 2 个 ```sql 块，分别在 `docs/CORE_PORTS_AND_STORAGE.md` 的 §7.3（第 765 行）与 §7.4（第 1068 行），与 plan 4.8 的「§7 必须恰好 2 个 sql 块」一致；
- `docs/CORE_PORTS_AND_STORAGE.md` 的 §11（第 1283 行到文件末尾）内**零围栏**：不存在与 §5.3/§7 重复的第二份签名或 DDL 正文。

§11 的形状核对（`docs/CORE_PORTS_AND_STORAGE.md`）：

- §11.5（第 1345 行）以 `[已并入]` 指向 §3.5 与 §5.3，只保留构造顺序、长度断言先于解析、指纹唯一入口、私钥不进 `core::model`、角色维度禁止「取第一行」这些理由；
- §11.6（第 1365 行）以 `[已并入]` 指向 §5.3，并在 §11 开头段明写「本节不再保留签名与 DDL 正文（第二份副本必然漂移）」；保留提交模型、7 条写集语义、两处删除权威、凭据边界与 `port_error_public` 通配臂陷阱；
- §11.7（第 1409 行）以 `[已并入]` 指向 §7.3/§7.4，只留跨族外键、不新增 catalog 投影表、BLOB 列、一次性探针的由来；
- §11.8（第 1421 行）以 `[已并入]` 指向 §7.2，只留 12-step、不补 grants、不降级三条理由；
- §11.9（第 1429 行）以 `[已并入]` 指向 §3.6/§5.1，只留「解析归 core」的理由；
- §11.1 的表是「哪张表承载哪类事实」的散文索引，并自指「可执行 DDL 与索引在 §7.3/§7.4」；`owned_pairing_peer` 行只列真实列（与 `docs/CORE_PORTS_AND_STORAGE.md` 的 §7.3 里该表定义一致，绑定与角色都不在该表），说明 `WP6B-2` 的修复已生效。

唯一残留的第二处**规则**正文是 §11.6 的 7 条写集语义，与 §5.3 的 DTO 文档注释语义重叠（例如 `ImportWrite.exports` 必须等于 `record.export_ids()`、`drop_import` 与 `ImportRemoval` 的删除权威不重叠）。plan 4.13 明确要求 §11.2/§11.6 保留行为规则与设计理由，且漂移门禁只绑 `docs/CORE_PORTS_AND_STORAGE.md` 的 §5 与 §7，因此判定为**信息级**（WP5-I5），不是「第二份绑定副本」。

### 1.2 无将来时 —— 通过（同步轮已修掉全部「已落地却仍写未实现」的实例）

类别 B（已落地却仍用计划措辞）在本轮同步后的逐处复核：

- `docs/CORE_PORTS_AND_STORAGE.md` 的 §5.3 第 327 行「实现状态」段 → 现写「SQLite 已实现附件端口、§7 的 v2 表结构与三个管理 store 的**落盘实现**（管理写集的一事务提交、失败关闭与容量纳入，见 `crates/storage-sqlite/src/admin/` 与 §9 判据 23–29）；core 侧同时保留测试替身」— 与树内 `crates/storage-sqlite/src/admin/{mod,trust,export,local_config}.rs` 一致；
- `docs/CORE_PORTS_AND_STORAGE.md` 的 §11 开头段（第 1286 行）与文档头修订行（第 11 行）→ 现写「已落地；仍未实现的是 Daemon/CLI 接线与 `identity-auth`/`identity-keystore`」；
- `README.md` 第 13 行（core 行）现写「运行时依赖只有 `async-trait`/`thiserror`/`p256`/`sha2`」，与 `crates/core/Cargo.toml` 的 `[dependencies]` 逐项一致；第 14 行（storage-sqlite 行）现写「以及 §7 管理表上的三个管理 store」；第 24 行现写「**落盘实现已落地**」；
- `docs/DEVELOPMENT_PLAN.md` 第 14/16/24/77 行 → 现写「core 端口与 SQLite 存储两层已落地（Daemon/CLI 接线仍未落地）」；
- `docs/MODULE_ARCHITECTURE.md` 第 209 行与第 296 行 → 现写「落盘已实现（三个管理 store + 用例层写集路径）」；
- `docs/CONFIG_REFERENCE.md` 第 23 行的节标题去掉了「，待实现」，第 11 行新增 0.7 修订记录。

类别 A（仍属规划，正确保留将来时，抽样）：

- `README.md` 第 19 行：「尚未开始：前端工程，以及 `server`、`agent-host`、`node-link-client`、`identity-auth`、`identity-keystore`、`acp-protocol`、`app`」——与 workspace 成员一致（只到 `node-link-protocol`）；
- `docs/IDENTITY_AND_AUTH_CONTRACT.md` 第 44 行：「只存在于 `identity-auth`（不进 `core::model`）：`ConnectionKind`…`P1363Signature`」；
- `docs/CORE_PORTS_AND_STORAGE.md` 的 §11.4：keystore 写失败、引用提交失败、连接清理失败的注入项与 QR payload 检查项（组合根与 `identity-auth` 未落地）；
- `docs/DEVELOPMENT_PLAN.md` 第 14 行：「Daemon/CLI 接线仍未落地」。

残留（信息级，见 WP5-I2）：`docs/CORE_PORTS_AND_STORAGE.md` 第 12 行的「版本：0.6」条目仍写「§11.5–§11.8 是 `[待实现]` 的目标形状」——版本历史行本身可接受，但第 11–14 行的顺序为「修订 / 0.6 / 0.8 / 0.7」，版本号非单调。

### 1.3 引用真实性（逐条）

- `docs/CORE_PORTS_AND_STORAGE.md` 的 §3.5 `PeerPublicKey` 行（第 142 行）声明出处为 `crates/core/src/model/identity.rs` 与身份合同：`PeerPublicKey` 在该文件第 610 行、`try_from_bytes` 第 620 行、`fingerprint` 第 638 行，33 字节压缩点的负例在 `crates/core/src/model/tests.rs` 第 2054 行起 ✔。但同一行声称的 `FromStr` 入口不存在（**WP5-4**）。
- `docs/CORE_PORTS_AND_STORAGE.md` 的 §3.5 `PairingPeer` 行（第 143 行）与 `PairingRecord` 行（第 139 行）声明 `host_binding: String(1..=2048)` 且必须与登记值逐字相等：`crates/core/src/model/identity.rs` 第 443 行是 `host_binding: String`、第 476 行是 `require_bounded(host_binding, 1, 2048)` ✔；同口径的规则在 `docs/CORE_PORTS_AND_STORAGE.md` 的 §11.2 第 1 条（第 1316 行）与 `docs/CORE_PORTS_AND_STORAGE.md` §7.3 的 `owned_pairing.host_binding` 列注释中 ✔。
- `docs/CORE_PORTS_AND_STORAGE.md` 的 §3.6 `ResolvedWorkspace` 行（第 159 行）指向`docs/CORE_PORTS_AND_STORAGE.md` §5.1：§5.1 的 `[决定]` 条目（第 232 行）确实含解析时机、绝对路径/存在/目录校验、`canonicalize` 权威值、两类失败分类、别名命名空间与「不进事件/错误 details/审计前像/catalog」✔。
- `docs/CORE_PORTS_AND_STORAGE.md` 的 §3.7 `SeedState` 行（第 173 行）指向 `docs/CONFIG_REFERENCE.md` 的「配置与管理状态的权威」与 `docs/CORE_PORTS_AND_STORAGE.md` §5.3：`docs/CONFIG_REFERENCE.md` 第 25–40 行与 `docs/CORE_PORTS_AND_STORAGE.md` §5.3 的 `SeedState`/`SeedWrite`/`mark_seeded` 一致 ✔（逐条见 §1.6）。
- `docs/CORE_PORTS_AND_STORAGE.md` 的 §9 判据 23–29（第 1241–1247 行）所引的执行面（`docs/CORE_PORTS_AND_STORAGE.md` §11.2、§11.3、§11.5、§11.6、§7.2、§7.4、§7.5、§8、§5.3）逐个小节都存在 ✔。
- `docs/CORE_PORTS_AND_STORAGE.md` 的 §9 判据 14（第 1232 行）本轮由「§11.8 追加的」改为「§7.3 的 `action` CHECK 里 … 五类」：`docs/CORE_PORTS_AND_STORAGE.md` §7.3 的 `owned_audit` CHECK（第 881 行起）确实列出这五个取值 ✔；但 `imported_audit` 的同类 CHECK 在`docs/CORE_PORTS_AND_STORAGE.md` §7.4（第 1133 行起），只提 §7.3 属漏项（**WP5-3**）。
- `docs/CORE_PORTS_AND_STORAGE.md` 的 §11.6 第 7 条（第 1385 行）本轮由「§11.8 第 7 条」改为「§7.3 的 `action` CHECK」：引用可解析 ✔，同一漏项同 **WP5-3**；原悬空引用已消除（复核通过：`docs/CORE_PORTS_AND_STORAGE.md` §11.8 现无编号条目，但已无任何引用指向它的条文号）。
- `docs/CORE_PORTS_AND_STORAGE.md` 的 §10 第 1277 行本轮改写为「枚举本体、`ALL`/`as_str` 与 §2 的取值表已同批落地」：`docs/CORE_PORTS_AND_STORAGE.md` §2 的枚举块（第 40 行 `ConflictKind`、第 41 行 `UnavailableKind`）仍只有旧取值，与 `crates/core/src/model/error.rs`（第 29/31/33 行的三个 `ConflictKind` 新取值、第 89 行的 `KeystoreUnavailable`）不符（**WP5-1**）；同一行的「见 §5.1 与 §11.6 末段」中，`docs/CORE_PORTS_AND_STORAGE.md` §5.1（会话后端，第 195 行起）不含写集映射义务或 `port_error_public` 陷阱（**WP5-2**），而`docs/CORE_PORTS_AND_STORAGE.md` §11.6 末段（第 1407 行）含 ✔。
- `docs/CORE_PORTS_AND_STORAGE.md` 的 §11.6 末段（第 1407 行）同样声称「已与 §2…同批落地」→ **WP5-1**。
- `docs/CORE_PORTS_AND_STORAGE.md` 的 §11.9（第 1429 行）被 `docs/SECURITY_DESIGN.md` 第 388 行与 `docs/LOCAL_ADMIN_PROTOCOL.md` 第 226 行引为路径校验规则的出处，而规则正文现在`docs/CORE_PORTS_AND_STORAGE.md` §5.1：一跳可达、内容不缺，plan 5.6 已登记为不改（信息级，**WP5-I6**）。
- `docs/IDENTITY_AND_AUTH_CONTRACT.md` 第 44 行与第 51 行分别指向`docs/CORE_PORTS_AND_STORAGE.md` §3.5 与 §5.3，两个小节都存在且含所声明规则 ✔（`FromStr` 例外见 **WP5-4**）。
- `docs/MODULE_ARCHITECTURE.md` 第 131 行指向`docs/CORE_PORTS_AND_STORAGE.md` §9 判据 13（第 1231 行）与 `scripts/check-crate-boundaries.mjs`：三方口径逐项一致 ✔。
- `docs/MODULE_ARCHITECTURE.md` 第 296 行指向`docs/CORE_PORTS_AND_STORAGE.md` §7.2/§7.3/§7.4 与 §11：三节加索引节都存在 ✔。
- `docs/CONFIG_REFERENCE.md` 第 10 行、第 32 行与第 154 行的引用分别指向`docs/CORE_PORTS_AND_STORAGE.md` §5.3 与 §7.2/§7.3/§7.4、§11：都存在且含对应规则 ✔。
- `README.md` 第 24 行与 `docs/DEVELOPMENT_PLAN.md` 第 16/24 行的锚点 `#11-管理状态持久化合同形状已并入-357` 指向该合同第 1283 行的标题；`node scripts/check-doc-links.mjs` 退出 0 ✔。
- `AGENTS.md` 第 291 行把`docs/CORE_PORTS_AND_STORAGE.md` §7 说成「`storage-sqlite` v1 表结构」：`docs/CORE_PORTS_AND_STORAGE.md` §7 现标题为「v2 表结构」（第 736 行）、且该 diff 已把 `AGENTS.md` 第 19 行的同一描述改成 v2 → 该行未同步（**WP5-I1**；该行不在本 diff 内，但被本 diff 证伪）。

结论：章节号未漂移；`§11.8 第 7 条` 的悬空引用已在本轮同步中消除；仍存的问题是「引用了不含该规则的章节」（WP5-1 的 §2、WP5-2 的 §5.1）、「只引一半」（WP5-3）与「声称的 API 不存在」（WP5-4）。

### 1.4 allow-list 三处一致 + 与实测相等 —— 通过（脚本注释计数除外）

- 实测 `cargo tree -p core --edges normal`（去掉 `core` 自身）38 项：`async-trait, base16ct, base64ct, block-buffer, cfg-if, const-oid, cpufeatures, crypto-bigint, crypto-common, der, digest, ecdsa, elliptic-curve, ff, generic-array, getrandom, group, hmac, hybrid-array, p256, pem-rfc7468, pkcs8, primeorder, proc-macro2, quote, rand_core, rfc6979, sec1, sha2, signature, spki, subtle, syn, thiserror, thiserror-impl, typenum, unicode-ident, zeroize`。
- `scripts/check-crate-boundaries.mjs` 第 171 行起的 `CORE_ALLOWED_CLOSURE` 逐项就是上述 38 项；脚本对该集合做双向比对（缺项与未登记项都报错），退出 0 → 实测与脚本登记逐项相等。
- 直接依赖：`crates/core/Cargo.toml` 的 `[dependencies]` = `async-trait`/`thiserror`/`p256`/`sha2`（4 项）；`docs/CORE_PORTS_AND_STORAGE.md` §9 判据 13（第 1231 行）写「`core` 的直接依赖固定为 `async-trait`/`thiserror`/`p256`/`sha2`」；`AGENTS.md` 第 320 行同；`docs/MODULE_ARCHITECTURE.md` 第 131 行同，并与 `README.md` 第 13 行（本轮已改成四个）一致 → 五处一致。
- 缺陷：`scripts/check-crate-boundaries.mjs` 第 172 行的注释写「core 的直接依赖（§2：**五个**）」，其下只列 4 个；且该脚本所引的 `docs/CORE_PORTS_AND_STORAGE.md` §2 并不含「直接依赖清单」（同一 docstring 上一段已把权威指为 `docs/CORE_PORTS_AND_STORAGE.md` §9 判据 13）。见 **WP5-5**。

### 1.5 门禁清单四处同步 —— 通过

- `package.json` 第 12 行的 `check` 串行执行 **10** 道：`check:schemas`、`check:commands`、`check:errors`、`check:features`、`check:assets`、`check:acp`、`check:docs`、`check:boundaries`、`check:drift`、`check:agentic`。
- `README.md` 第 55 行写「`npm run check` 串行执行**十项**检查」并逐项列出 ①–⑩，名称与顺序与 `package.json` 一致。
- `.github/workflows/ci.yml` 第 4–8 行的注释列出 10 项：协议 fixture、命令目录与封闭词表（含本地管理方法集与错误码）、错误码 registry、feature 词表、需要真正计算的资产绑定、ACP 矩阵、文档链接与锚点、crate 依赖方向、合同漂移、agentic 流程与规范 → 数量与名称齐全。
- `AGENTS.md` 第 291 行不重复枚举，写「顺序与数量以 `package.json` 的 `check` 脚本为准」并指向 `README.md` 的「合同检查」小节 → 无数量断言，不会漂移。
- 本轮 diff 未增删门禁（`package.json` 与 `.github/workflows/ci.yml` 不在 diff 内），四处一致性是既有事实；独立复算见本节末。
- **定稿后复算**：`node scripts/check-doc-links.mjs` → 退出码 **0**：`doc links OK: 367 relative links, … section refs across … markdown files`（文件与引用计数含并行落盘的其它 review 报告，逐次变动）；`npm run check` → 退出码 **0**，末行 `Totals: 1 passed, 0 failed (1 items)`。
- 信息：`AGENTS.md` 第 291 行同一句写「只固定**三条**独有硬约束」，随后实际列了四项（文档引用门禁、合同漂移门禁、crate 依赖方向门禁、封闭词表门禁）。该行与基线逐字相同（非本轮引入），记为 **WP5-I3**。

### 1.6 `CONFIG_REFERENCE.md` 的权威与 seed 语义 vs `docs/CORE_PORTS_AND_STORAGE.md` §5.3 —— 无冲突

| 规则 | `docs/CONFIG_REFERENCE.md` | 合同侧（`docs/CORE_PORTS_AND_STORAGE.md`） | 判定 |
|---|---|---|---|
| 首次初始化单事务导入全部合法 profile 并记录完成标记 | 第 37 行 | §5.3 的 `SeedWrite` 与 `mark_seeded` | 一致 |
| 空列表也标记完成 | 第 37 行 | §5.3 的 `SeedWrite` 注释与 `mark_seeded` 注释（「空列表也写标记」） | 一致 |
| 任一项非法则整批失败 | 第 37 行 | §9 判据 27（第 1245 行）与 `docs/CORE_PORTS_AND_STORAGE.md` §11.6 的提交模型（「任一约束失败 → 整事务回滚」） | 一致（不冲突） |
| 初始化后数据库是唯一权威、重启不重导 | 第 38 行 | §11.6 的本地配置写集（「初始化完成后数据库是唯一权威，不再重导配置」） | 一致 |
| 已有同 ID 记录与种子不一致 → 显式报错、不覆盖、不合并 | 第 37 行 | §5.3 与 §11.6 未复述该条；与 `openspec/changes/admin-state-persistence-v2/specs/local-agent-config/spec.md` 的「种子 MUST 被忽略而不覆盖已有配置」相容 | 不冲突（配置侧单独声明） |
| 旧配置不得复活已撤销 Export / 已删除 Import | 第 39 行 | §11.3 末条与 `docs/CORE_PORTS_AND_STORAGE.md` §11.4 清单 | 一致 |
| 凭据注入经 `CredentialResolver`（`env` ∩ `env_allowlist`） | 第 154 行 | §5.3 的 `CredentialResolver::resolve_env` | 一致（本轮把引用由 §11.6 改到 §5.3，目标确实含该签名） |

WP6 对上述语义的**实现**判定属 `reports/rv1-wp6.md` 的范围，本报告不重复。

## 2. 文档要求 → 实际文本 对照表

| 指派要求 | 实际文本（含 file:line） | 判定 |
|---|---|---|
| §11.5–§11.9 的形状已并入 §3/§5/§7/§9，`docs/CORE_PORTS_AND_STORAGE.md` 的 §11 只剩「已实现」索引（设计理由 + 链接） | `docs/CORE_PORTS_AND_STORAGE.md` §11.5（1345）、§11.6（1365）、§11.7（1409）、§11.8（1421）、§11.9（1429）全部以 `[已并入]` 开头并给出指针；§11 内零代码围栏 | 通过 |
| 不残留与 `docs/CORE_PORTS_AND_STORAGE.md` 的 §5.3/§7 重复的第二份签名或 DDL 正文 | 全文 5 个 ```rust 块（§2/§5.1/§5.2/§5.3/§5.4）+ 2 个 ```sql 块（§7.3/§7.4），§11 内无围栏 | 通过 |
| 已落地部分不得再用「计划/待验证/目标形状」措辞，仍属规划的部分保留将来时 | 类别 B 在同步轮全部修正（该文档第 11/327/1286 行、`README.md` 第 13/14/24 行、`docs/DEVELOPMENT_PLAN.md` 第 14/16/24/77 行、`docs/MODULE_ARCHITECTURE.md` 第 209/296 行、`docs/CONFIG_REFERENCE.md` 第 11/23 行）；类别 A 正确保留（`README.md` 第 19 行、`docs/IDENTITY_AND_AUTH_CONTRACT.md` 第 44 行、`docs/CORE_PORTS_AND_STORAGE.md` §11.4） | 通过（残留 1 处信息级：版本行顺序，WP5-I2） |
| 每处「§x.y」引用的目标必须真的包含所声明的规则 | 抽查 4 处不符/欠精确：`docs/CORE_PORTS_AND_STORAGE.md` §2（WP5-1）、§5.1（WP5-2）、只引 §7.3 漏 §7.4（WP5-3）、`FromStr`（WP5-4）；其余逐条命中（见 §1.3） | **不通过（4 项建议级）** |
| `AGENTS.md` §12 的 core 依赖名单、`docs/CORE_PORTS_AND_STORAGE.md` §9 判据 13、脚本 `CORE_ALLOWED_CLOSURE` 三者逐项一致并与 `cargo tree` 相等 | 38 项闭包 + 4 项直接依赖四处一致（`scripts/check-crate-boundaries.mjs` 第 171 行起、`docs/CORE_PORTS_AND_STORAGE.md` §9 判据 13、`AGENTS.md` 第 320 行、`docs/MODULE_ARCHITECTURE.md` 第 131 行、`README.md` 第 13 行） | 通过（脚本注释计数另计 WP5-5） |
| `package.json` / `AGENTS.md` §10 / `README.md` 合同检查 / `.github/workflows/ci.yml` 四处门禁数量与名称一致 | 10 道 ↔ 「十项 ①–⑩」↔ 10 项注释；`AGENTS.md` 第 291 行只做指针 | 通过 |
| `docs/CONFIG_REFERENCE.md` 的配置权威与 seed 语义与 `docs/CORE_PORTS_AND_STORAGE.md` §5.3 的 `LocalConfigStore`/`SeedWrite` 一致、无冲突 | 7 条规则逐条一致（见 §1.6） | 通过 |
| 登记偏差必须以 verification 的 Check Plan Changes 为准核对；代码/文档声称「记录在案」但查不到 = 发现 | Check Plan Changes 第 1/3/11 条对应的文档改动（`EntityRef.Provider`、`ImportWrite.exports`、§11 改名连带引用）都在位 ✔；本轮同步轮未新增偏差声明（`openspec/changes/admin-state-persistence-v2/verification.md` 由调度者与本报告之外的角色维护，其内容我只读引用） | 通过 |

## 3. 未解决发现

### 阻断（0 项）

无。本切片的缺陷都在文档/注释层：`node scripts/check-contract-drift.mjs`、`node scripts/check-crate-boundaries.mjs`、`node scripts/check-doc-links.mjs`、`npm run check` 在定稿后的工作区全部退出 0，`docs/CORE_PORTS_AND_STORAGE.md` 的 §5 与 §7 与被检视实现逐条一致，因此不构成 `BLOCKED`/`FAIL` 判据。以下 4 项建议在归档（任务 8.x）前修完。

### 建议

| ID | 位置 | 问题 | 证据 | 建议 |
|---|---|---|---|---|
| WP5-1 | `docs/CORE_PORTS_AND_STORAGE.md` 第 1277 行（§10）与第 1407 行（§11.6 末段） | 两处都断言四个新枚举取值「与 §2 的取值表已同批落地」/「已与 §2…同批落地」，但`docs/CORE_PORTS_AND_STORAGE.md` §2 的 `PortError` 块（第 40/41 行）仍只列旧取值——合同里最权威的错误类型枚举与代码不一致，且 §2 不在任何门禁的断言面（漂移门禁只绑`docs/CORE_PORTS_AND_STORAGE.md` §5/§7；`scripts/*.mjs` 中没有任何 `PortError` 引用） | `crates/core/src/model/error.rs` 第 29/31/33 行（`AlreadyExists`/`IdentityMismatch`/`DuplicateOwnership`）、第 89 行（`KeystoreUnavailable`）与第 44–48、100–101 行的 `ALL`；`git diff 28f8cb9..5404610 -- docs/CORE_PORTS_AND_STORAGE.md` 中 `pub enum ConflictKind` 出现 0 次（§2 未被改） | 把`docs/CORE_PORTS_AND_STORAGE.md` §2 的两行枚举补成含四个新取值（与 `error.rs` 逐项一致），或撤掉「已与 §2 同批落地」的断言并注明 §2 未同步；建议顺带给 §2 的错误枚举加一条门禁 |
| WP5-2 | `docs/CORE_PORTS_AND_STORAGE.md` 第 1277 行（§10） | 「写集侧映射义务与 `port_error_public` 不得用通配臂吞掉新取值的陷阱见 §5.1 与 §11.6 末段」——`docs/CORE_PORTS_AND_STORAGE.md` §5.1（会话后端，第 195 行起）是 `AgentCatalog`/`SessionBackendFactory`/`SessionEndpoint` 的签名与 workspace 解析规则，既不含写集映射义务，也不含 `port_error_public` 陷阱 | `grep -n port_error_public docs/CORE_PORTS_AND_STORAGE.md` 只命中第 1277 行与第 1407 行；`docs/CORE_PORTS_AND_STORAGE.md` §5.3 第 329 行起是写集 DTO 与 `TrustStore`/`ExportStore`/`LocalConfigStore` 签名 | 把该行的指针由 §5.1 改成 §5.3（写集 DTO 与错误分类）与 §11.6 末段（`port_error_public` 陷阱） |
| WP5-3 | `docs/CORE_PORTS_AND_STORAGE.md` 第 1278 行（§10）与第 1385 行（§11.6 第 7 条） | 两处都只引`docs/CORE_PORTS_AND_STORAGE.md` §7.3 来指审计动作的 `action` CHECK，并写「§7.3 两张审计表」；实际 §7.3 只有 `owned_audit`（第 878 行起），`imported_audit` 在`docs/CORE_PORTS_AND_STORAGE.md` §7.4（第 1133 行起）。同一处第 1232 行（§9 判据 14）改为单指 §7.3，同样漏掉 imported 一侧 | `grep -n "CREATE TABLE \(owned_audit\|imported_audit\)\|^### 7\.[34]" docs/CORE_PORTS_AND_STORAGE.md` → §7.3 第 763 行、`owned_audit` 第 878 行；§7.4 第 1066 行、`imported_audit` 第 1133 行 | 改成「`docs/CORE_PORTS_AND_STORAGE.md` §7.3 与 §7.4 的 `action` CHECK」（第 1278/1385 行），第 1232 行的判据 14 同 |
| WP5-4 | `docs/CORE_PORTS_AND_STORAGE.md` 第 142 行（§3.5 `PeerPublicKey` 行）与 `docs/IDENTITY_AND_AUTH_CONTRACT.md` 第 51 行 | 两处（均为本轮新增/新增内容）声称 `PeerPublicKey` 有 `FromStr` 构造入口（「`try_from_bytes`/`FromStr` 任一步失败即 `InvalidValue`」），但 `crates/core/src/model/identity.rs` 只有 `try_from_bytes`（第 620 行）、`TryFrom<&[u8]>`（第 657 行）与 `Debug`（第 648 行）；`grep -rn "impl FromStr for PeerPublicKey" crates/` 无命中，且该文件第 679 行起的十六进制解码是 `#[cfg(test)]` 且注释写明「不做通用解析」 | `crates/core/src/model/identity.rs` 第 610–694 行 | 二选一：删掉 `/FromStr` 的表述；或在 `core::model` 补 `impl FromStr for PeerPublicKey`（hex 文本入口）以匹配合同，并补相应用例 |

### 信息

| ID | 位置 | 内容 |
|---|---|---|
| WP5-I1 | `AGENTS.md` 第 291 行 | 仍把 `docs/CORE_PORTS_AND_STORAGE.md` §7 说成「`storage-sqlite` v1 表结构」；`docs/CORE_PORTS_AND_STORAGE.md` §7 已是 v2，且本 diff 已把 `AGENTS.md` 第 19 行改成 v2。该行不在本 diff 内（基线逐字相同），但被本 diff 证伪 |
| WP5-I2 | `docs/CORE_PORTS_AND_STORAGE.md` 第 11–14 行 | 文档头顺序为「修订（2026-09-23）」→「版本 0.6」→「版本 0.8」→「版本 0.7」，版本号非单调；0.6 行仍含「§11.5–§11.8 是 `[待实现]` 的目标形状」——版本历史行可接受，但建议把 0.6 行移到 0.7 之前或标注「已被 0.7/0.8 取代」 |
| WP5-I3 | `AGENTS.md` 第 291 行 | 「只固定三条独有硬约束」后实际列四项；与基线逐字相同，非本轮引入 |
| WP5-I4 | `scripts/check-crate-boundaries.mjs` 第 172 行 | 注释「core 的直接依赖（§2：五个）」只列 4 个，且`docs/CORE_PORTS_AND_STORAGE.md` §2 不含该清单（权威是同文件上一段点名的 §9 判据 13 与 `crates/core/Cargo.toml`）。该脚本不在本轮同步的文件集内，故仍存 |
| WP5-I5 | `docs/CORE_PORTS_AND_STORAGE.md` 第 1379–1385 行（§11.6 写集语义） | 与 `docs/CORE_PORTS_AND_STORAGE.md` §5.3 的 DTO 文档注释存在规则级重叠（`ImportWrite.exports`、删除权威、默认 profile 原子切换）；plan 4.13 明确保留，非缺陷 |
| WP5-I6 | `docs/CORE_PORTS_AND_STORAGE.md` 第 1429 行（§11.9） | 被 `docs/SECURITY_DESIGN.md` 第 388 行与 `docs/LOCAL_ADMIN_PROTOCOL.md` 第 226 行引为路径校验规则的出处，而规则正文已迁到`docs/CORE_PORTS_AND_STORAGE.md` §5.1；一跳可达，plan 5.6 已登记为不改 |
| WP5-I7 | `docs/CONFIG_REFERENCE.md` 第 37 行 | 「已有同 ID 管理记录与种子不一致时显式报错」只在该文档与 `openspec/changes/admin-state-persistence-v2/specs/local-agent-config/spec.md` 出现，合同 §5.3/§11.6 未复述；属权威分工，非冲突 |

### 假想回退与覆盖缺口（可证伪性）

每条建议都写明「实现/文本改回什么会让它失败」，以及当前**没有**任何断言覆盖该点：

- **WP5-1**：假想回退 = 把 `crates/core/src/model/error.rs` 的四个新取值删掉，该合同 §2 的枚举块与 §10/§11.6 的「已同批落地」断言都不受影响（二者同时错、同时对，无人比对）。失败点 = 漂移门禁只绑该合同 §5/§7，`scripts/*.mjs` 中没有任何 `PortError` 引用。**无用例覆盖**。
- **WP5-2**：假想回退 = 把该合同第 1277 行的「§5.1」换成任意小节号，`check:docs` 仍会退出 0（它只解析到小节是否存在，不校验内容归属）。失败点 = 该合同 §5.1（第 195 行起）既无写集映射义务也无 `port_error_public` 陷阱。**无用例覆盖**。
- **WP5-3**：假想回退 = 把该合同第 1278/1385 行的「§7.3」保留、把 `imported_audit` 挪走或删掉，门禁同样退出 0。失败点 = 该合同 §7.4（第 1133 行）才有 `imported_audit` 的同类 CHECK。**无用例覆盖**。
- **WP5-4**：假想回退 = 在 `crates/core/src/model/identity.rs` 里删掉（或补上）`impl FromStr for PeerPublicKey`，该合同 §3.5 与 `docs/IDENTITY_AND_AUTH_CONTRACT.md` 第 51 行的表述都不受影响；`cargo test -p core` 也不会因此变红（该类型没有 `FromStr` 用例）。失败点 = 声称的 API 与 `identity.rs` 第 610–694 行的实现集不符。**无用例覆盖**。
- **WP5-5**：假想回退 = 把 `scripts/check-crate-boundaries.mjs` 第 172 行注释里的「四个」改回「五个」（现状）或任意数字，脚本退出码不变（注释不参与断言）。失败点 = 同行实际只列 4 个 crate，且该脚本自身比对的权威是该合同 §9 判据 13。**无用例覆盖**。

## 4. 结论

- **PASS**（无阻断项）。WP5 切片的核心要求成立：`docs/CORE_PORTS_AND_STORAGE.md` 的 §11.5–§11.9 确已把形状并入`docs/CORE_PORTS_AND_STORAGE.md` §3/§5/§7/§9，§11 只剩索引与设计理由（§11 内零代码围栏，全文只有`docs/CORE_PORTS_AND_STORAGE.md` §2/§5.1/§5.2/§5.3/§5.4 的 5 个 ```rust 块与 §7.3/§7.4 的 2 个 ```sql 块），因此不存在与被漂移门禁绑定的第二份签名/DDL 副本；将来时在本轮同步后已无「已落地却写未实现」的实例（类别 A 的合法将来时保留）；`AGENTS.md` §1/§10/§12、`README.md`、`docs/DEVELOPMENT_PLAN.md`、`docs/MODULE_ARCHITECTURE.md`、`docs/CONFIG_REFERENCE.md`、`docs/IDENTITY_AND_AUTH_CONTRACT.md` 的引用与锚点全部解析到存在的小节；allow-list 在 `scripts/check-crate-boundaries.mjs`、`docs/CORE_PORTS_AND_STORAGE.md` §9 判据 13、`AGENTS.md` §12、实测 `cargo tree -p core --edges normal` 与 `README.md` 第 13 行五处逐项相等（38 项普通闭包、4 项直接依赖）；门禁清单在 `package.json`（10）、`README.md`（十项 ①–⑩）、`.github/workflows/ci.yml`（10 项）与 `AGENTS.md` §10（只做指针）四处一致；`docs/CONFIG_REFERENCE.md` 的权威与 seed 语义与 `docs/CORE_PORTS_AND_STORAGE.md` §5.3 的 `LocalConfigStore`/`SeedWrite` 逐条一致、无冲突。
- **归档前建议修完 4 项**：WP5-1（`docs/CORE_PORTS_AND_STORAGE.md` §2 的错误枚举未含四个新取值，而两处断言声称已同步）、WP5-2（第 1277 行把写集/`port_error_public` 指向 §5.1）、WP5-3（审计 `action` CHECK 只引 §7.3、漏 §7.4）、WP5-4（声称 `PeerPublicKey::FromStr` 存在）。
- **检视基线**：固定版本 `5404610` + 工作区的 WP5 同步改动（`git status` 见 §0）；命令证据见 §0 与 §1.5。本轮未运行 `cargo fmt/clippy/test`，未修改任何仓库文件（唯一写入是本报告）。
