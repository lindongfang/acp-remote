# RV1-WP1 Review 报告（变更 `node-link-owner` / WP1「合同口径与依赖登记」）

> 说明：本轮 reviewer 子 Agent 无写文件工具（只读检视），报告由主 Agent 按其返回正文原样持久化到本路径。

## Shared Report

- **task_id**: 1.2 / 1.3 / 2.1 / 2.2 / 2.3（WP1，交付单元 DU1）
- **role**: reviewer（独立代码检视子 Agent，只读，不继承实现对话）
- **phase**: review；**stage**: work-package
- **agent_context**: 本轮为任务级 reviewer 子 Agent（02978e92-04f4-40d5-b269-b4cd79eb5ae7），未参与 WP1 实现；工作目录 `D:\Project\acp-remote-wt\node-link-owner`（只读，未修改任何文件、未切换分支、未提交）。
- **target_revision**: `028a4d2a71ae01364012ffec4da9a85c7f8cf905`（= 工作区 `HEAD`）
- **scope**: `Cargo.toml`、`crates/server/Cargo.toml`、`Cargo.lock`、`docs/MODULE_ARCHITECTURE.md` 的 base→target diff；证据 `reports/wp1-deps.log`、`reports/wp1-handoff.md`。
- **changes**（独立复核结果，与实现者自报一致）：`docs/MODULE_ARCHITECTURE.md` 11 插入 / 1 删除；`Cargo.toml` 26 插入 / 0 删除；`crates/server/Cargo.toml` 14 插入 / 0 删除；`Cargo.lock` 600 插入 / 15 删除；合计 651 插入 / 16 删除。
- **checks**: [PV2] 静态复核 **通过**（脚本口径 + 矩阵格 + 负向对照的自洽性）；[PV1] **部分**（W0 轮次：`npm run check` / `cargo fmt --check` / `cargo check -p server` 有记录；`npm run verify` 的 clippy + 全 workspace 测试属 3.1/2.24，待返回证据）。
- **issues**: 无阻断；4 项非阻断（详见 Findings）。
- **result**: **PASS**（对应 target revision；不等于整个变更可归档，也不代表 E2E/候选验证已完成）。
- **evidence_paths**: `openspec/changes/node-link-owner/reports/wp1-deps.log`、`openspec/changes/node-link-owner/reports/wp1-handoff.md`、本报告 `reports/rv1-wp1.md`。
- **resource_cleanup**: 本轮只读；未创建端口/进程/临时目录，未写任何文件。

## Review Context

| 项 | 内容 |
|---|---|
| Review ID / Type / Stage / WP | RV1-WP1 / branch / 工作包交付前 / WP1（合同口径与依赖登记） |
| Repository / Base / Target | `D:\Project\acp-remote-wt\node-link-owner` / `94a64e1f15d26f54a6601985fd13b1440fec8170` / `028a4d2a71ae01364012ffec4da9a85c7f8cf905` |
| 读取的规则与需求 | `proposal.md`（Intent and Constraints：依赖核验、矩阵格、不改 wire 合同）、`design.md` D1/D2/D5（并确认 D1 已写入 `rustls-pemfile`→`rustls-pki-types` 偏离说明与 provider 结论）、`plan.md` WP1 行/Execution Waves/Runtime Resources/PV1/PV2/Code Review RV1、`tasks.md` 1.2/1.3/2.1/2.2/2.3、`AGENTS.md` §7/§9/§10/§12、`docs/SECURITY_DESIGN.md` §20、`scripts/check-crate-boundaries.mjs`、`deny.toml`、`rust-toolchain.toml` |
| 实际检查范围 | 上述 4 个变更文件在 target revision 的完整内容；`crates/node-link-protocol/src/{lib.rs,common.rs}` 与 `crates/acpr-wire/src/lib.rs`（矩阵格理由）；`Cargo.lock` 解析条目与 base `Cargo.lock` 的抽样比对；`target/debug/.fingerprint/**`（实际编译单元与 feature，只读）；`reports/wp1-deps.log` 全文；`reports/wp1-handoff.md` 全文 |
| 证据与限制 | (1) 无 shell/Git 权限：不能跑 `git diff`、不能重跑任何门禁，也不能枚举提交的完整文件清单——base 内容改用主仓库 `D:\Project\acp-remote` 的工作副本比对（对 4+1 个文件逐字节等价性由行数与内容抽样确认：`Cargo.lock` 2650→3235、根 `Cargo.toml` 105→131、`crates/server/Cargo.toml` 72→86、`MODULE_ARCHITECTURE.md` 693→703，均与自报 diff 数一致），target 侧由 `watchdog_diff` 确认工作区干净且 `HEAD`=target。残余风险：若该提交还改了 scope 之外的文件，本轮不可见。(2) 无网络/registry 权限：第三方 crate 的 license 字符串、发布时间、上游 `archived`、逐包 `rust-version` 无法独立重算；改用「解析版本 + 被排除包缺席 + 编译 fingerprint 的实际 feature」交叉验证。(3) 全部门禁通过结论来自实现者日志，本轮只做静态可信性核对（脚本报错模板、文档结构、锁文件内容对齐）。 |

## handoff_index

```yaml
handoff_index:
  - task_id: "2.1"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: "028a4d2a71ae01364012ffec4da9a85c7f8cf905"
    evidence_type: REVIEW
    evidence_id: RV1-WP1
    report_path: "openspec/changes/node-link-owner/reports/rv1-wp1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "本轮在固定 HEAD=028a4d2a… 上只读检视 §5 矩阵 server→acpr-wire 格与表下限定注记、表下注记与 check-crate-boundaries.mjs 判定口径、cj1 实际导出面（acpr-wire 有、node-link-protocol 无）；结论对应本目标版本。"
    source_evidence: "docs/MODULE_ARCHITECTURE.md:487/497、crates/server/Cargo.toml:49、crates/acpr-wire/src/lib.rs:22、crates/node-link-protocol/src/lib.rs:1-16 与 common.rs:18-22、scripts/check-crate-boundaries.mjs:22-56/129-152"
  - task_id: "2.2"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: "028a4d2a71ae01364012ffec4da9a85c7f8cf905"
    evidence_type: REVIEW
    evidence_id: RV1-WP1
    report_path: "openspec/changes/node-link-owner/reports/rv1-wp1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "§20 四判据证据的静态真实性核对：解析版本与 Cargo.lock 一致、被排除候选（aws-lc-rs/aws-lc-sys/cmake/rustls-pemfile）在锁与编译产物中均缺席、ring provider 生效（rustls 编译 feature 集为 std/tls12/logging/ring）、rcgen 解析为 0.14.7、deny.toml 未改且 allow 表覆盖新增 SPDX id；第三方元数据（license/发布日期/archived）无法独立重算，已在限制中列明。"
    source_evidence: "Cargo.lock、target/debug/.fingerprint/rustls-06b6d9c2cd1dbe65/lib-rustls.json、…/axum-7d2f7607801e5bd7/lib-axum.json、…/tokio-tungstenite-07f5401521e8b8c7/lib-tokio_tungstenite.json、deny.toml:80-97、reports/wp1-deps.log §3/§7/§8/§11"
  - task_id: "2.3"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: "028a4d2a71ae01364012ffec4da9a85c7f8cf905"
    evidence_type: REVIEW
    evidence_id: RV1-WP1
    report_path: "openspec/changes/node-link-owner/reports/rv1-wp1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "[PV2] 通过与 [PV1] 部分完成的判定及待补项；权限不足未重跑命令，属静态复核，已在 Assessment 中按 Check ID 记录。"
    source_evidence: "reports/wp1-deps.log §9/§12.1、scripts/check-crate-boundaries.mjs"
```

## Findings

| ID | 级别 | 位置 | 问题 / 触发条件 | 证据 | 最小修复 |
|---|---|---|---|---|---|
| RV1-WP1-F1 | P2（MINOR，非阻断） | `Cargo.toml:123`（本次新增注释）；次要同源问题见 `reports/wp1-deps.log` §11.4 | 注释断言「除 `base64 0.22` 外不引入新 crate」，并暗示 `base64 0.22` 是本批新增的重复版本。实际：`base64 0.22.1` 在 base 锁里已存在（不是本次引入）；而本批确实新增了 `pem`/`time`/`yasna`/`ring`/`rustls-pki-types` 等多项。触发条件：任何后续按该注释做供应链复核的人会得出错误结论（本 WP 的产物正是「依赖口径」）。 | base `Cargo.lock:170-172` 已有 `base64 0.22.1`，且被 `sqlx-core`（base `Cargo.lock:2059` 段）等消费；target 锁中 `axum`、`pem` 只是复用既有 0.22.1；本变更日志 §7.2 列出 54 个新增节点，含 `pem 3.0.6`、`time 0.3.45`、`yasna 0.5.2`、`ring 0.17.14`、`rustls-pki-types 1.15.1`，而 `base64` 不在该清单内；日志 §11.4「本次解析新增 `base64 0.22.1`」与其自身 §7.2 的清单互相矛盾。 | 把该句改为可核对的表述，例如「启用链新增 `pem`/`yasna`/`time`（及其 `time-core`/`time-macros`/`deranged`/`powerfmt`/`num-conv`）与 `rustls-pki-types`；`base64 0.22.1` 是 base 锁既有版本（axum 的 `ws` 与 `pem` 复用），`zeroize`/`x509-parser`/`aws-lc-rs` 为可选未启用」；同步订正日志 §11.4 一句。 |
| RV1-WP1-F2 | P2（MINOR，需主 Agent 确认，非阻断） | `crates/server/Cargo.toml:46-53,78` | 该文件不在 WP1 在权威规划工件中声明的写入范围内：`plan.md` WP1 `Write Scope` 只有 `docs/MODULE_ARCHITECTURE.md`、`Cargo.toml`、`deny.toml`；「文件单一写入者」清单同样未列它；`tasks.md`（2.2）只写「在 `Cargo.toml` 的 `[workspace.dependencies]` 登记…、`deny.toml` 同步」。handoff 以「任务 2.2 明确授权」为依据，但该授权只存在于主 Agent 下发的任务提示中，仓库内可见的契约没有它（本轮无法读取主 Agent 的任务提示，故只报事实、不判定越权）。 | `plan.md` WP1 行与 Execution Waves 文本；`tasks.md` 2.2；`reports/wp1-handoff.md`（自承「这一文件不在白名单四文件之内」）。 | 主 Agent 二选一：(a) 接受该预登记并在 `plan.md` WP1 写范围/单一写入者表补上 `crates/server/Cargo.toml`（含 `Cargo.lock`）；(b) 要求回退为「只登记 workspace 版本」，此时 `Cargo.lock` 无新增节点、[PV2] 的 `server→acpr-wire` 格将因无可判定的依赖边而失去真实断言力。 |
| RV1-WP1-F3 | SUGGESTION | `docs/MODULE_ARCHITECTURE.md:1-11` | 文件头部 `修订记录` 未按既有惯例为本变更加一行（此前每次改动该文档的切片都加过），`版本：0.3` 也未变；本次新增的 §3.1 `[决定]`、§4.9 注记、§5 注记在头部没有对应索引。 | 文件头 1-11 行与各次修订记录对照。 | 在头部加一行 `修订记录（2026-09-26，node-link-owner 变更 WP1）`，一句话点明 §3.1 依赖口径、§4.9 现状、§5 矩阵格与注记。 |
| RV1-WP1-F4 | SUGGESTION | `reports/wp1-deps.log` §7.1 | 日志里引用的 `Cargo.toml` diff 与最终提交内容不一致（`rcgen` 注释第三行不同：日志引的是「依赖链只有 `pem`/`yasna`/`time`/`zeroize` 与 `ring`」，提交里是「实际启用的依赖链只有…」）。该差异已由 §12.1 的「一次 `--amend`」说明覆盖，但 §7.1 本身没有任何指针，后来读者会把这段引用当成提交内容。 | `wp1-deps.log` §7.1 vs `Cargo.toml:123-124`；`wp1-deps.log` §12.1 的 amend 说明。 | 在 §7.1 标题后加一句「本段引用于首次提交；注释第三行在 §12.1 的 amend 后已改为提交现状」，或直接换成 amend 后的 diff。 |

**未发现** P0/P1 级问题：无依赖方向破坏、无 wire 合同改动、无 MSRV 抬高、无「未落地却写成已落地」、无被吞掉的失败或无依据的证据复用。

### Correct（已核实无误的部分，含证据）

1. **§5 矩阵格与限定注记准确且与门禁口径一致**：`MODULE_ARCHITECTURE.md:487` 的 `server` 行 `acpr-wire` 格为 ✓；`:497` 的限定注记成立——`acpr-wire` 确实持有 `pub mod cj1`（`crates/acpr-wire/src/lib.rs:22`），而 `node-link-protocol` 只做值对象命名空间再导出、不含 `cj1`（`crates/node-link-protocol/src/lib.rs:7-16` 无 `cj1` 模块，`common.rs:18-22` 的 `pub use acpr_wire::{...}` 列表无 `cj1`），因此 `server::node_link` 直接依赖 `acpr-wire` 是唯一可行路径；`check-crate-boundaries.mjs` 确实逐格解析 §5 表并对「成员依赖不在列」与「格为空白」分别报错，日志记录的负向对照报错文本与该脚本模板逐字一致。
2. **provider = ring 的防 feature 加法落实到位**：根 `Cargo.toml` 两处 `default-features = false` + `ring`；编译产物证明生效——rustls 编译 feature 集为 `["log","logging","ring","std","tls12"]`（无 `aws_lc_rs`/`prefer-post-quantum`），deps 含 `ring`；`Cargo.lock` 与全部 `.fingerprint` 中均**不存在** `aws-lc-rs`/`aws-lc-sys`/`cmake`/`rustls-pemfile`；`target/debug/build/ring-*/out/` 内有 ring 预生成汇编编译出的 `.o`，印证「只需 C 编译器」。
3. **axum 注册与「不协商压缩」的注释属实**：只增量开 `ws`；`axum` 指纹 feature 集无 `http2`/`multipart`/`macros`；`tokio-tungstenite` 指纹 feature 集为 `["connect","default","handshake","stream"]`，无任何 `deflate`/压缩 feature。
4. **未抬高 MSRV、未改工具链**：`[workspace.package] rust-version = "1.85"` 未变，`rust-toolchain.toml` 仍 pin `1.98.1`；`rcgen` 解析到 `0.14.7` 而非最新 0.14.10，与「resolver = 3 的 MSRV 感知选版」机理自洽。
5. **`deny.toml` 未改动可信**：target 与 base 的 `[licenses] allow` 相同；日志列出的新增 SPDX id 中唯一不在表内的 `LGPL-2.1-or-later` 只作为 `r-efi` 的 `MIT OR Apache-2.0 OR LGPL-2.1-or-later` 第三分支，而 `deny.toml` 既有说明恰好把这一情形写明「不需要、也不应该出现在清单里」——声明与配置自洽。
6. **未把「未落地」写成「已落地」**：§4.9 明写 `node_link`「该变更交付中、**尚未落地**」；§3.1 明写这批依赖「在 WP2 起被 `server::transport::net` 与 `server::node_link` 消费（WP1 只登记与核验）」；`crates/server/Cargo.toml` 同样标注「使用代码自 WP2 起落地」。
7. **偏离理由成立且 design 已同步**：`design.md` D1 已把候选写为「原列 `rustls-pemfile 2`；WP1 核验发现其上游已归档，实际改用 `rustls-pki-types` 的 `pem` 模块」，并记录 provider 定案与落选候选；doc/design 两处口径一致，无需再回退。
8. **diff 数量与 base/target 完全可复现**（旁证 base 未混入其它改动）：文档 693→703 行、§5 各行右移 9 行、根 `Cargo.toml` 105→131、`crates/server/Cargo.toml` 72→86、`Cargo.lock` 2650→3235，正好等于 651 插入 / 16 删除。

## Assessment（Check ID 与重点项逐条）

| Check / 重点项 | 已核对的证据 | 差异 / 待补项 | 是否影响本轮判断 |
|---|---|---|---|
| **[PV2]**（`node scripts/check-crate-boundaries.mjs`） | 脚本判定口径（读 §5 表、成员依赖逐条比对、行缺席静默）；文档矩阵行与注记；日志记录 exit=0 与负向对照报错 | 本轮**无法重跑**（无 shell）；实现者报文字与脚本错误模板一致、且该结论可由「`acpr-wire` 是 `server` 的 path 依赖」+ 脚本逻辑独立推出，故采信 | 否（判为满足） |
| **[PV1]**（`npm run verify`） | 日志：`npm run check`（10 道逐条 OK）、`cargo fmt --all -- --check`、`cargo check --locked -p server --all-targets --all-features`；`.husky/pre-commit` 另跑过 clippy 全绿 | **未跑** `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` 与 `cargo test --locked --workspace --all-features`；这是派单给定的 W0 范围（PV1 全量在 3.1 / 2.24）。**需在 3.1 补齐并返回 `reports/du1-pv1.log`** | 否（非本 WP 交付条件；但合并前必须补齐） |
| 重点 1：§5 格与判定口径 | 见 Correct 1 | 无 | 否 |
| 重点 2：§20 核验证据真实性 | 解析版本、被排除包缺席、编译 feature、`deny.toml` 对齐、`rcgen 0.14.7`、ring 现场编译；无法逐条重算的部分已用锁/指纹交叉验证 | 第三方元数据本轮无法独立重算；日志 §7.5 输出夹带一段 bash 引号报错，属日志卫生问题、数字仍可读 | 否（作为限制记录，不构成发现） |
| 重点 3：`rustls-pemfile`→`rustls-pki-types` 偏离 | `design.md` 已同步；`wp1-deps.log` 给出归档证据与一行回退方式；不登记该 crate 使依赖图更小 | 结论本身只能靠日志上游证据（无网络无法复核 `archived: true`） | 否 |
| 重点 4：ring provider 落实 | 见 Correct 2 | 无 | 否 |
| 重点 5：`deny.toml` 未改动声明 | 见 Correct 5 | `cargo-deny` 本地不可执行（实现者如实记录）；新增包的 license 字符串无法本地重算 | 否（CI `deps`/`advisories` 仍为最终判定） |
| 重点 6：文档口径/格式 | 无「已落地」误写；MSRV 未抬；注记格式与既有 `[决定]`/`[现状]` 结构同构 | 头部 `修订记录` 未加行（F3，建议级） | 否 |

## 结论

- **判定：PASS**（Target Revision `028a4d2a71ae01364012ffec4da9a85c7f8cf905`）。无未解决 CRITICAL/MAJOR；4 项非阻断（F1/F2 为 P2，F3/F4 为建议）不影响本 WP 交付前放行，但 F1 与 F2 建议在 WP2 开始前处理或由主 Agent 明确裁决。
- **必须在候选/合并前补齐的证据**：`npm run verify`（[PV1] 全量，含 clippy 与全 workspace 测试，任务 3.1 / 2.24）；CI 的 `deps`/`advisories`/`secrets` 三个 job（本地无等价物）。
- 本轮未执行、也不会执行 E2E；PASS 仅表示约定范围的只读检视完成，不代表候选验证通过或变更可归档。
