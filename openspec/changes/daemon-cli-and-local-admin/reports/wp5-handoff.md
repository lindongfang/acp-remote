# WP5 交接：把「已落地」状态写回权威文档（任务 2.19）+ 最终成员集上的统一验证（任务 2.20）

```yaml
task_id: WP5（tasks.md 2.19、2.20；**未**触碰 tasks.md 本身的复选框）
role: 实现 Agent（coder，受限 worker；只写 docs/MODULE_ARCHITECTURE.md、docs/DEVELOPMENT_PLAN.md §2、docs/IDENTITY_AND_AUTH_CONTRACT.md §7、docs/CONFIG_REFERENCE.md、README.md、AGENTS.md §4、reports/**）
phase: apply（WP5；W3 完成点的一部分）
agent_context: >
  并行纪律：另一写者（task 2.26，`daemon stop` 排空延迟）同时在改 `crates/app/**`。本 Agent 全程只读
  `crates/**`，提交一律 `git commit --only <显式路径>`（未用 `git add` + 裸 `git commit`），
  提交后再以 `git show --stat` 核实只含自己的六个文件。未触碰 crates/**、vendor/**、schemas/**、
  fixtures/**、compatibility/**、tasks.md、verification.md、plan.md、design.md、proposal.md、specs/**。
target_revision:
  branch: feat/daemon-cli-and-local-admin
  base: f45231e（本轮起点，与任务书一致；`git log --oneline -3` 已核实）
  docs_commit: 94067e1        # docs(app): 回写切片 4 的已落地状态与 nodeId 派生式（6 files, +27/-16）
  handoff_commit: 本文件所在提交（`git log --oneline -1 -- openspec/changes/daemon-cli-and-local-admin/reports/wp5-handoff.md`）
  verified_head: 0a046370f778ef9585b09f856fc4a578e264d421   # [PV1]/[PV2] 对应的 HEAD（验证后由主 Agent 控制的分支再无 crates/** 提交）
  verified_rust_content_equals: 82d376d   # 最后一次改动 crates/** 的提交（其后两个提交只改 reports/** 与 verification.md）
scope:
  - docs/MODULE_ARCHITECTURE.md（§3 状态行 + 修订记录、§3 `[现状]`、§3.1 成员数、§4.9/§4.10 `[现状]`、§5 表下注记）
  - docs/DEVELOPMENT_PLAN.md（文件头基线段 + §2）
  - docs/IDENTITY_AND_AUTH_CONTRACT.md（版本记录 + §7 第 7 条）
  - docs/CONFIG_REFERENCE.md（仅 `daemon.instance_lock` 一行）
  - README.md（「仓库当前状态」crate 表 + 两处状态措辞 + 权威文档表 IDENTITY 一行的实现状态）
  - AGENTS.md（仅 §4 模块状态表与其后一句）
  - openspec/changes/daemon-cli-and-local-admin/reports/**（日志；`.log` 受 .gitignore 忽略、属本地证据）
```

## 1. 事实基线（改文档前先核过）

- `cargo metadata --no-deps`：workspace 成员 **12 个**——`acpr-transcript`、`acpr-wire`、`core`、`storage-sqlite`、
  `sync-protocol`、`node-link-protocol`、`acp-protocol`、`agent-host`、`identity-auth`、`identity-keystore`、
  `server`、`app`。**`node-link-client` 不存在**（\( \S \)3 列的十三个里唯一未落地的）。
- `crates/server/src` 实际只有 `transport/local/**` 与 `local_admin/**`：**`sync`/`node_link`/`acp_facade` 三个
  平级模块没有源码**，因此文档里一处都没有把它们写成已落地。
- `crates/app/src` 有 `cli`/`client`/`clock`/`compose`/`config`/`daemon`/`identity`/`lock`/`logging`/`stdio`：
  daemon、CLI、组合根确实已落地。
- `vendor/windows-local-ipc` 在 `workspace.exclude` 里，**不是成员**；§5 矩阵把它登记为「列」。
- §5 矩阵的列：12 个成员 + `windows-local-ipc`；行：12 个成员 + `node-link-client`（列外行）。

## 2. changes（逐文件改动与理由）

| 文件 | 改动 | 理由（与 `cargo metadata` 对齐） |
|---|---|---|
| `docs/MODULE_ARCHITECTURE.md` | §3 状态行把 `server`/`app` 加入「已实现」并注明范围只限切片 4；新增一条修订记录（2026-09-25）；§3 `[现状]` 由「已开始落地/落地中」改为「已落地」，并把结尾那句过期的「§5 的列比本节的行少是已知的、被门禁拦住的缺口」换成实际关系（成员=本节除 `node-link-client` 外的十二个；§5 列=这十二个 + `windows-local-ipc`；`node-link-client` 只作为列外行）；§3.1 末尾成员条目补「切片 4 落地后是十二个」；§4.9 `[现状]`「已开始落地」→「已落地」；§4.10 `[现状]`「在本切片落地」→「已落地」；§5 表下注记整段重写 | §5 矩阵的内容与判据**未动**（列已在 WP4a 加好，`check:boundaries` 仍逐条比对实际 `cargo metadata`）；只收敛说明段里那批写着「`storage-sqlite`/`node-link-client`/`server`/`app` 仍只作为行出现」「缺列有明确后果」的过期措辞 |
| `README.md` | 「仓库当前状态」表新增 `server`、`app` 两行；`尚未开始` 一句改为只剩前端 + `node-link-client` + 三个 `server::` 平级模块；结尾段落把「仍未实现的是 Daemon/CLI 接线与 `server::*` 入站适配器」收敛为已落地面 + 两个按合同的例外 + 仍未落地面；权威文档表中 IDENTITY 一行的「实现前合同」改为「已定型；`identity-auth`/`identity-keystore` 已按它落地」 | crate 表必须与 `cargo metadata` 的 12 个成员一致；「实现前合同」与 `IDENTITY_AND_AUTH_CONTRACT.md` 自身的状态行（「已定型并已落地（v1）」）矛盾，属同一类状态回写 |
| `docs/DEVELOPMENT_PLAN.md` | 文件头基线段补「2026-09-25 随 `daemon-cli-and-local-admin` 补充切片 4 的落地状态」；§2 的「（Daemon/CLI 接线仍未落地）」「尚未落地的是 `server`、`node-link-client`、`app` 和前端工程」按切片 4 的事实改写 | 文件头基线段就是 §2 状态陈述的版本记录，漏更新会让「基线」比正文旧 |
| `AGENTS.md` | §4 模块状态表：`server`/`app` 由「待落地」改为「已落地（仅切片 4 范围：…）」；表后一句补指向 `MODULE_ARCHITECTURE.md` §3 `[现状]` | §4 是模块状态的登记处，与 README、MODULE_ARCHITECTURE 三处必须同口径；§4 其他内容（依赖规则、边界纪律）未动 |
| `docs/IDENTITY_AND_AUTH_CONTRACT.md` | 新增「版本：0.4」记录；§7 端口小节末尾新增第 **7** 条 `[决定]`（`nodeId` 派生式、为什么没有独立持久记录、代价、不改变 wire）——见下节 | 该文档的既有体例就是「`[决定]`/`[已定型]` 条目 + 版本记录」；没有新造顶层小节，章节编号未变 |
| `docs/CONFIG_REFERENCE.md` | 仅 `daemon.instance_lock` 一行补「当前切片只接线 `file`；显式 `ipc` 会被解析但以 `daemon.config_unwired`（debug 级）注明未生效」 | 这是唯一一个「文档写了两个取值、而其中一个在本切片会落到另一个实现」的键（`crates/app/src/config.rs:297-307`、`crates/app/src/daemon.rs:745-750` 佐证）。其余「已知但未接线」的键（`daemon.listen`/`allowed_hosts`/`trusted_proxies`/`tls.*`/`sync.*`/`node_link.*`/`sessions.*`/`terminal.*`）属于尚未存在的 adapter，已在模块/协议文档里按切片登记，不在本 WP 补逐键注记（见 §5 残余项 4） |

提交：`94067e1 docs(app): 回写切片 4 的已落地状态与 nodeId 派生式`（`git show --stat` 确认 **6 files changed, 27 insertions(+), 16 deletions(-)**，无一条他人文件）。

## 3. `nodeId` 派生式的写入位置

- **位置**：`docs/IDENTITY_AND_AUTH_CONTRACT.md` **§7「`identity-keystore` 端口（已定型）」的第 7 条**，并在同一文件的版本记录里以 0.4 登记（与 `verification.md` §「WP5（2.19）须把该派生式与域分离串写进权威文档」的要求一致：§7 或 `MODULE_ARCHITECTURE.md` §4.10，本 Agent 选前者）。
- **写入的原文口径**：
  `nodeId = SHA-256("acp-remote/node-id/v1" 的 UTF-8 字节 ‖ 节点身份公钥的 65 字节 SEC1 未压缩编码)` 的前 16 字节，
  按 RFC 4122 形状置位（version nibble = 8、variant = `10xx`）后呈现为带连字符的小写 canonical UUID，交 `core::model::NodeId` 校验。
- **实现佐证**（只读核对，未改）：`crates/app/src/identity.rs` 的 `NODE_ID_DOMAIN = b"acp-remote/node-id/v1"`、
  `node_id_for()`（前 16 字节 + `bytes[6] |= 0x80`、`bytes[8] |= 0x80`）。
- **同一条里同时写清**：① 为什么不存在独立的本地 `nodeId` 持久记录（`CORE_PORTS_AND_STORAGE.md` 无本机节点身份的列、
  `CONFIG_REFERENCE.md` 无该键、keystore 只存密钥；`NodeRecord` 描述的是对端）——因此域分离串 + 公钥派生是**当前唯一来源**；
  ② 代价「轮换节点密钥即改变 `nodeId`」与 §6.3「身份材料变化不得自动接受、必须重新配对」一致；
  ③ 该派生式不改变任何 wire（Sync/Node Link 的 `nodeId` 字段仍由各自协议定义）。

## 4. 检查与验证证据（含退出码与 HEAD）

### 4.1 `npm run check`（每次文档提交前）

| 日志 | 命令 | 结果 |
|---|---|---|
| `reports/wp5-docs.log` | `npm run check`（提交前跑一次，另有一次等价的基线跑） | `EXIT(npm run check)=0`；其中 `doc links OK: 379 relative links, 4546 section refs`、`crate boundaries OK: 12 个 crate …（12 个已在矩阵登记）`、`contract drift OK: §7 的 36 条 DDL … §5 的 15 个 trait / 87 个方法签名`、`check:agentic` 全 PASS（`agentic 宿主入口检查完成：17 个文件`） |

### 4.2 [PV1] / [PV2]（任务 2.20；在 2.19 的文档提交之后）

| 项目 | 值 |
|---|---|
| 命令 | `npm run verify`（[PV1]）、`node scripts/check-crate-boundaries.mjs`（[PV2]） |
| 退出码 | `EXIT(npm run verify)=0`、`EXIT(node scripts/check-crate-boundaries.mjs)=0` |
| 验证对应的 HEAD | **`0a046370f778ef9585b09f856fc4a578e264d421`**（`feat/daemon-cli-and-local-admin`，`git status --short` 为空） |
| 覆盖的 Rust 内容 | 等于 `82d376d`（最后一次改 `crates/**` 的提交；其后的 `1ea291a`/`0a04637` 只改 `reports/**` 与 `verification.md`） |
| 工具链 | `rust-toolchain.toml` 的 `1.98.1`；Node v24.19.0 / npm 12.0.2 |
| 测试计数 | 82 个 test target（含 doc-tests）**全部 `ok`**；passed 合计 **730**、failed **0**、ignored **2** |
| 跳过（ignored）的 2 个用例 | `crash_child`（「由 `crash_recovery_leaves_an_consistent_database` 拉起」）、`regenerate_v2_fixtures`（「夹具生成器：只在需要重建 `fixtures/storage/v2` 时手动运行」）——两者都在源码里写明理由，属预期跳过 |
| 日志 | **`reports/wp5-verify.log`**（自描述：HEAD、工具链、命令、PV1/PV2 逐行原文、计数、退出码）；原始输出 `reports/wp5-pv1-raw.log`、`reports/wp5-pv2-raw.log`；`reports/du1-pv1.log` 追加了指针段 |

> **为什么另建 `wp5-verify.log`**：`du1-pv1.log` 与并发写者（task 2.26 的 PV1，`>` 截断写）发生了踩踏，
> 丢失了 WP5 那次的表头并留下一段 NUL 空档（文件头现在写着 `=== task 2.26 / PV1 …`）。为避免再次踩踏，
> WP5 的 PV1/PV2 原文改存 `reports/wp5-verify.log`，并只在 `du1-pv1.log` 末尾追加一段指针与退出码。
> 任务书要求的「日志追加 `du1-pv1.log`」已满足（追加+指针），同时留有一份无 race 的权威副本。

## 5. 未执行项与残余项（如实登记）

1. **`scripts/check-crate-boundaries.mjs` 的行内注释仍是旧口径**（`readDependencyMatrix` 上方那段写着「§5 矩阵当前有 9 个『列』… `node-link-client` / `storage-sqlite` / `server` / `app` 仍只作为『行』存在」）。**行为本身正确**（列从表头读取、行从表格读取；12 个成员全部登记且逐条比对通过），但注释与现状不符。`scripts/**` 不在本 WP 的写范围内，故未改——建议由主 Agent 指派的后续小改动（`scripts` scope）收敛。
2. `docs/DEVELOPMENT_PLAN.md` **§3 切片 1** 仍写着「本切片剩余的是 Daemon/CLI 接线与端到端验收」。本轮写范围被限定在 §2，故未动；该句在切片 4 落地后已半过期（端到端验收仍待 3.x 收口），需后续一并收敛。
3. `docs/LOCAL_ADMIN_PROTOCOL.md` §3.1 的实现状态注记**已核对、无需改动**：它写的「`server::acp_facade` 尚未落地；Daemon 对 `0x02` 在 framing 校验后立即关闭并记结构化警告」与实现一致（`crates/server/src/transport/local/connection.rs:12-13,164-171` 与 `CloseReason::FacadeUnavailable`），且该注记没有对 `app` 的落地状态作任何陈述，因此不存在需要改正的过期措辞。方法表/词表/错误码未动。
4. `docs/CONFIG_REFERENCE.md` 的处理范围：只改了 `daemon.instance_lock`。其余「已知但未接线」的键（`daemon.listen`、`allowed_hosts`、`trusted_proxies`、`tls.*`、`sync.*`、`node_link.*`、`sessions.*`、`terminal.*`、`dev_mode.allow_plaintext`）在本切片同样只在启动时被 `daemon.config_unwired` 注明，但它们对应的是**尚不存在的 adapter**（`server::sync`/`server::node_link`/`server::acp_facade`），已在模块与协议文档里按切片登记。给每个键单独写「本切片未接线」会凭空造一套文档约定，故未做；若主 Agent 认为需要在配置文档里建立这个约定，请给出统一措辞后再补。
5. **[3.7]/[3.9] 需按新 HEAD 重跑**：本轮的 [PV1]/[PV2] 对应 `0a04637`。其后若再有 `crates/**` 提交落地，该结论只对 `82d376d` 的 Rust 内容有效，必须重跑并重新登记 SHA。
6. **未做**：未跑 `cargo-deny`/`gitleaks`（只在 CI 运行，本地无等价物）；未跑独立 reviewer（3.10）、未更新 `tasks.md`/`verification.md`（不在写范围内）；未推送、未开 PR、未合并。

## 6. 给 reviewer（3.10）的核对入口

- 状态一致性：以 `cargo metadata --no-deps --format-version 1` 输出的 12 个包名逐条比对 `README.md` crate 表、`AGENTS.md` §4、`docs/MODULE_ARCHITECTURE.md` §3/§5。
- 未误写：三处都不存在把 `node-link-client`、`server::sync`、`server::node_link`、`server::acp_facade` 写成已落地的句子（可 `grep` 这三个名字在这些文件里的每一处上下文）。
- 未误改切片顺序/验收表述：`docs/DEVELOPMENT_PLAN.md` §3 的切片 1–8 与验收段本轮**零改动**（`git show 94067e1 -- docs/DEVELOPMENT_PLAN.md` 只显示文件头基线段与 §2）。
- `nodeId` 派生式的位置与口径见本文 §3。
- 证据链：`reports/wp5-verify.log`（HEAD/命令/退出码/计数）→ `reports/wp5-pv1-raw.log`、`reports/wp5-pv2-raw.log`（原文）→ `reports/wp5-docs.log`（提交前的 `npm run check`）。
