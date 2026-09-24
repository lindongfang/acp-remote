<!-- 由独立 reviewer 子 Agent 产出（任务 6.4 后续定向确认，target 工作区 = 4f9c418），主 Agent 原样落盘（仅：把引文中的相对链接 ./MODULE_ARCHITECTURE.md 改为 code span 形式，避免文档引用门禁对归档报告误报；报告正文其余未改动）。 -->
# rv2-du1.md — DU1 候选 P2 收口定向确认（任务 6.4 后续：上一轮 PASS + 5 条 P2 的最小修复复核）

> 本报告是本 reviewer 子 Agent 的**唯一交付物**。本会话无写文件工具（工具集只有 `read`/`grep`/`ls`/`watchdog_diff`），因此按运行时的输出路径覆盖，报告正文在最终响应中返回，由运行时持久化到
> `openspec/changes/identity-auth-and-keystore/reports/rv2-du1.md`。本次复核**未创建、未修改、未删除任何仓库文件**（含不写 `tasks.md`/`verification.md`、不写 `progress.md`）。

```
task_id: 6.4-followup（DU1 候选 ed98f6d 的 5 条 P2 收口定向确认）
role: 独立 reviewer（只读子 Agent；未继承任何产品实现/集成对话；不重做 6.4 全量检视、不新增检查面）
phase: candidate-fix-verify（复核「ed98f6d + 记录 md 提交 + 修复提交 4f9c418」这一工作区；未合入、非最终验收）
agent_context: 新建隔离上下文；工具集仅 read/grep/ls/watchdog_diff（无 shell、无 git、不能执行命令、不能改文件）；cwd = D:\Project\acp-remote
target_revision: 工作区 = 本轮 launch HEAD = 4f9c418（= ed98f6d + 记录 *.md 提交 + 修复提交「去掉正常路径的 expect 并订正状态口径与合同重复块」）；上一轮被检视候选 = ed98f6d；上一轮 reviewer launch HEAD = 27938ca（以上 revision 关系由编排者声明，我无 git、不能自证）
scope: 只做定向确认——(1) rv1-du1.md §3 登记的 5 条 P2 是否按最小修复落地；(2) 其中唯一的代码改动（PairingSecret::digest 的签名变更）是否行为等价、无新增失败语义、无新口径矛盾。附带只读确认：合同 §5.2 硬上限段与 state.rs 一致、reports/ 的 du1-* 现状与编排者声明一致。**不做**：6.4 的全量检视、RV1–RV9 逐条复核、specs 覆盖映射、tasks.md/verification.md 的更新
changes: 本次复核自身零改动（未创建/修改/删除任何文件；报告由运行时落盘）；被检视的修复面 = docs/IDENTITY_AND_AUTH_CONTRACT.md（§2/§5.1/§5.2）、docs/MODULE_ARCHITECTURE.md（§4.1）、docs/CORE_PORTS_AND_STORAGE.md（文档头 + §11）、crates/identity-auth/src/types.rs、crates/identity-auth/src/pairing.rs、crates/identity-auth/src/port.rs（详情见 §1；「4f9c418 相对 27938ca 只含记录 md 与这 5 条修复」由编排者声明，我无法自证）
checks: 逐条见 §1/§2；全部为只读核对（读源码/合同/文档/日志/清单），**未执行任何命令**
issues: 0 × P0、0 × P1、1 × P2（新发现，且**不在**本轮点名 grep 面内：openspec/config.yaml 的模块画像仍写两个 crate 未实现）；5 条 P2 全部闭环
result: PASS（5 条 P2 全部闭环；digest 签名改动行为等价、无新增失败语义；唯一新发现为范围外 P2，不构成阻断）
evidence_paths: 见 §1 逐条证据与 §4 文件清单；关键路径 = openspec/changes/identity-auth-and-keystore/reports/{rv1-du1.md, du1-selfcheck-rg.log, du1-integration.md}、crates/identity-auth/src/{types.rs, pairing.rs, port.rs, error.rs, state.rs, authority.rs, handshake.rs}、crates/core/src/model/{scalars.rs, mod.rs}、docs/IDENTITY_AND_AUTH_CONTRACT.md、docs/MODULE_ARCHITECTURE.md、docs/CORE_PORTS_AND_STORAGE.md、README.md、AGENTS.md、docs/DEVELOPMENT_PLAN.md、openspec/config.yaml
```

---

## 1. 5 条 P2 的逐条收口表

### P2-1 合同 §5.2 逐字重复的 `[决定]` 块 —— **已闭环**

| 项 | 内容 |
| --- | --- |
| 原 ID / 级别 | P2-1 / P2（`rv1-du1.md` §3） |
| 本轮结论 | **闭环**：重复块已去掉，只剩一份，且上下文完整 |
| 关键证据 | `grep 挑战缓存有硬上限 docs/IDENTITY_AND_AUTH_CONTRACT.md` → **仅 1 命中，行 345**（上一轮为 `:343` 与 `:350` 两处）。`docs/IDENTITY_AND_AUTH_CONTRACT.md:343` = `### 5.2 nonce、重放与时钟`；`:345-349` 为唯一一份硬上限块（读全文逐行确认）；`:351` = `` `[决定]`： ``，其后 `:352-358` 的列表**逐条完整**：① `challenge`/`server_nonce` 一次性消费（`auth.proof_invalid`）② 缓存只在内存、TTL 固定 15 秒、重启即丢弃 ③ 重放同一 proof 必须失败并追加 `device.auth_failed` 审计 ④ 时间判定一律用注入 `Clock` ⑤ `[open]` 时钟偏移 v1 不做事 ⑥ 每个新 WSS 连接完整 challenge-response、不签发长期 token；`:360` 起为 `## 6. 授权`。**未见误删**其它内容 |

### P2-2 三处权威文档仍把两个 crate 写成「未实现」 —— **已闭环（三处全改）**，另有 1 处范围外残留（见 §2）

| 项 | 内容 |
| --- | --- |
| 原 ID / 级别 | P2-2 / P2（同一根因、三处） |
| 本轮结论 | **闭环**：点名三处已统一为「两个身份 crate 已落地；仍未实现的是 Daemon/CLI 接线与 `server`/`app` 的入站适配器」 |
| 关键证据 | ① `docs/MODULE_ARCHITECTURE.md:215`（§4.1）现文：「管理状态的端口签名在 …§5.3，SQLite 落盘实现在 `crates/storage-sqlite/src/admin/`…。**仍未实现的是 Daemon/CLI 接线与 `server`/`app` 的入站适配器**（`identity-auth`/`identity-keystore` 两个身份 crate **已落地**，见 §4.8/§4.12）；在接线完成前不得声称这些管理能力已端到端可用。业务决定仍由 core 用例拥有。」<br>② `docs/CORE_PORTS_AND_STORAGE.md:11`（文档头修订行）现文：「…**仍未实现的是 Daemon/CLI 接线与 `server`/`app` 的入站适配器**（`identity-auth`/`identity-keystore` 两个身份 crate 已于 2026-09-24 落地，见 `docs/MODULE_ARCHITECTURE.md` §4.8/§4.12）…」<br>③ `docs/CORE_PORTS_AND_STORAGE.md:1312`（§11 索引段）现文：「…已落地；仍未实现的是 Daemon/CLI 接线与 `server`/`app` 的入站适配器（`identity-auth`/`identity-keystore` 已落地）——在接线完成之前，不得把这些存储测试通过解释为配对、撤销或本地配置已经端到端可用。」 |
| 与其它权威文档并列判定 | `README.md:28`（仓库当前状态段）：「…`identity-auth`/`identity-keystore` 两个身份 crate **已落地**（契约、状态机、平台 keystore 与固定向量回归测试）；仍未实现的是 Daemon/CLI 接线与 `server::*` 入站适配器，因此在它们完成前，不能声称配对、撤销或本地配置已经端到端可用。」<br>`README.md:18`/`:19`（crate 表）：两行分别描述 `identity-auth`（纯状态机）与 `identity-keystore`（平台安全存储适配器），无「未实现」措辞；`README.md:22`：「尚未开始：前端工程，以及 `server`、`node-link-client`、`app`。」<br>`AGENTS.md:96-101`（§4 模块状态表）：`node-link-client 待落地`／`identity-auth 已落地`／`identity-keystore 已落地`／`server 待落地`／`app 待落地`；`:104` 保留「待落地只是边界不代表已实现」的通用口径。<br>`docs/DEVELOPMENT_PLAN.md:14`（§2 基线）：「…切片 3 在 `identity-auth-and-keystore` 变更中落地了 `identity-auth`（纯状态机…）与 `identity-keystore`（Windows DPAPI 包裹 + 非 Windows 失败关闭）。**尚未落地的是 `server`、`node-link-client`、`app` 和前端工程。**」<br>→ 四处（三处修复点 + README + AGENTS + DEVELOPMENT_PLAN）**同一口径**，仅有措辞差异（`server`/`app` 对 `server::*`），无实质冲突。 |
| 事实前提独立核对 | `Cargo.toml:3-14` 的 `[workspace] members` 实测 **10 个**（`acpr-transcript`/`acpr-wire`/`core`/`storage-sqlite`/`sync-protocol`/`identity-auth`/`identity-keystore`/`node-link-protocol`/`acp-protocol`/`agent-host`），与 `ls crates/`（10 个目录）逐名一致 → 「已落地」断言成立，修复方向正确。 |
| 「没有其它仍在 docs/**、README.md、AGENTS.md 内写未实现」的清单 | 我把 `docs/**` 内所有 `identity-keystore` 命中逐条读完：`CORE_PORTS_AND_STORAGE.md:11/1305/1312`、`DEVELOPMENT_PLAN.md:14/36`、`IDENTITY_AND_AUTH_CONTRACT.md:3/7/8/13/20/26/32/46/51/391/393/400/444/445/446/456`、`MODULE_ARCHITECTURE.md:3/5/51/57/62/114/125/135/139/215/331/334/404/409/416/418/429/450/462/475/476/631/644/645`、`adr/0003:5`、`adr/0005:30`、`adr/0006:15/17/26/27`、`SECURITY_DESIGN.md:220/224/595/598`、`diagrams/*`。<br>**唯一与「未实现」相关且与 identity 有关的只有两处，且都正确**：(a) `MODULE_ARCHITECTURE.md:408-414` 的 `[现状]` 块「只有 Windows/DPAPI 一列已落地；macOS 与 Linux 后端**尚未实现**」——指平台后端，与 `crates/identity-keystore/src/platform/` 现状相符；(b) `AGENTS.md:256`（§9）「`node-link-client`、macOS/Linux 的 keystore 后端与 `server::*` 尚未落地」——同为平台后端 + 未落地 crate，正确。<br>`docs/IDENTITY_AND_AUTH_CONTRACT.md:3` 已是「**已定型并已落地（v1）**」；无一处再把两个 crate 整体写成未实现。 |

### P2-3 合同 §2 类型归属 + §5.1 公开面 —— **已闭环**，且与源码逐条一致

| 项 | 内容 |
| --- | --- |
| 原 ID / 级别 | P2-3 / P2 |
| 本轮结论 | **闭环**：(a)(b) 两条归属说明均已加，§5.1 清单已补 `Authority::new`；实测公开集合与登记集合一致，**无未登记项** |
| 关键证据 (a) | `docs/IDENTITY_AND_AUTH_CONTRACT.md:51`：「**由 `identity-auth` 定义、由 `identity-keystore` 实现**的 keystore 端口边界：`KeyPurpose`、`SecretPurpose`、`KeyHandle`、`SecretBytes`、`P1363Signature`、`EntropySource`/`EntropyError`（§7）。定义方是 `identity-auth`——端口 trait 与这些类型都在 `crates/identity-auth/src/port.rs`；`identity-keystore` 只 `use` 端口，不自己定义同名类型（§5 依赖矩阵不允许它依赖 `core`）。」→ 归属方向已由「只存在于 `identity-keystore` 边界」改为「identity-auth 定义 / identity-keystore 实现」。 |
| 关键证据 (b) | `docs/IDENTITY_AND_AUTH_CONTRACT.md:50`：「只存在于 `identity-auth`、且与其它 crate 有**同名类型**的（务必按下表指认，不要串用）：`FeatureList`（本 crate 自己实现的连接特性列表；与 `acpr-wire::FeatureList` **不是同一个类型**，依赖矩阵也禁止 `identity-auth` 依赖 `acpr-wire`——§5.1 签名块里的 `FeatureList` 指的就是本 crate 的这一个）、`ClientKind`（与 `sync_protocol::auth::ClientKind` 同名不同物）、`PairingProof`（配对期 HMAC 证明的载荷形状）。」 |
| 归属描述 vs 源码实测 | 合同描述正确：`crates/identity-auth/src/port.rs:17` `pub enum KeyPurpose`、`:35` `pub enum SecretPurpose`、`:56` `pub struct KeyHandle(String)`、`:98` `pub struct SecretBytes(Vec<u8>)`；`crates/identity-auth/src/transcript.rs:138` `pub struct FeatureList(Vec<String>)`（经 `lib.rs:45-46` 公开导出）、`crates/identity-auth/src/types.rs:197` `pub struct PairingProof([u8; 32])`、`:308` `pub enum ClientKind`。同名不同物台账成立：`crates/acpr-wire/src/lib.rs:211` `pub struct FeatureList(Vec<FeatureId>)`、`crates/sync-protocol/src/auth.rs:18` `pub enum ClientKind`。四者确由 `identity-auth` 定义、`identity-keystore` 侧只 `use`（`identity-keystore/src/{store,entry,ephemeral,error,entropy}.rs` 经 `identity_auth::…` 引入）。<br>（细节备注：rv1 报告写 `ClientKind:304`/`PairingProof:196`，实测为 `:308`/`:197`；合同不引用行号，故不构成问题。） |
| 关键证据 §5.1 | `docs/IDENTITY_AND_AUTH_CONTRACT.md:314`：「…以及构造子 `new(clock, entropy, local_node, keystore)`（组合根装配用，§2「调用方负责装配」）。」→ `Authority::new` 已登记。 |
| 公开面实测 vs 登记（`impl Authority` 全部方法） | `authority.rs`：`new:33`、`local_node:51`、`now:56`、`node_public_key:61`（async）、`sign_sync_pairing_host_proof:97`、`sign_sync_host_challenge:108`、`sign_node_link_pairing_owner_proof:119`、`sign_node_link_challenge:130`、`reset_memory:145`、`challenge_cache_len:153`。<br>`handshake.rs`：`hello:72`、`verify_proof:163`、`complete_auth:305`。<br>`pairing.rs`：`begin_pairing:29`、`pairing_sas:92`、`verify_claim:149`、`settle:274`、`due_pairings:330`、`unrecoverable_after_restart:349`、`pairing_status:363`、`mark_pairing_approved:405`、`failure_count:410`、`has_secret:415`；`ClaimedPairing::to_claim:452`。<br>→ **全部已在 §4.1/§5.1 登记**（§4.1 覆盖配对入口 + `to_claim`，§5.1 覆盖三握手入口 + 诊断入口 + 四个 `sign_*` + `new` + `MAX_CHALLENGES`）。**未登记的公开项：0**。<br>`pub(crate)` 面（不属公开面、无需登记）：`authority.rs:71 entropy`、`:78 state`、`:89 sign_transcript`；`handshake.rs:325 random_nonce_for`；`pairing.rs:430 complete`（§5.1 已说明「对外只有 `complete_auth` 一个名字」）。 |

### P2-4 正常路径的 `expect`（本轮唯一代码改动） —— **已闭环，行为等价、无新增失败语义**

| 项 | 内容 |
| --- | --- |
| 原 ID / 级别 | P2-4 / P2 |
| 本轮结论 | **闭环**；(a)(b)(c)(d) 四项全部确认，判断为**行为等价且不新增失败语义** |
| (a) 源码现状 | `crates/identity-auth/src/types.rs:257` `pub fn digest(&self) -> Result<Digest, InvalidValue> {`；函数体 `:258-260`：`use sha2::Digest as _; let digest = sha2::Sha256::digest(self.0); Digest::new(&acpr_transcript::encode_base64url(&digest))`（即直接返回 `Digest::new` 的结果，**无 `expect`**）。<br>调用点 `crates/identity-auth/src/pairing.rs:60` `secret.digest()?,`，位于 `pub fn begin_pairing(...)`（`:29-40`）的**正常路径**。非测试调用点**仅此一处**（全 crate grep `.digest()`：`src/pairing.rs:60` 与其定义；其余命中都在 `tests/ports.rs:72,80,84`）。 |
| (b) 独立复算命中集合 | 我按日志同一口径（`unwrap()|expect(|panic!`，另补 `unreachable!|from_der|todo!`）独立复算：<br>• `crates/identity-auth/src` = **0 命中**（含 `unreachable!`/`from_der`/`todo!` 亦 0）——`types.rs` 那一处确已消失；<br>• `crates/identity-keystore/src` = **16 命中**，逐行清单：`entropy.rs:37`、`entropy.rs:38`、`ephemeral.rs:54`、`store.rs:499`、`:515`、`:523`、`:525`、`:529`、`:530`、`:539`、`:567`、`:570`、`:571`、`:573`、`:581`、`:587` → 合计 **16**，与 `reports/du1-selfcheck-rg.log` 末行「命中总数：16」**逐项相等**；<br>• 逐行归类：`entropy.rs:37-38` 在 `#[cfg(test)] mod tests`（`entropy.rs:28-29`）；`store.rs:499-539` 在 `#[cfg(test)] mod mode_tests`（`store.rs:458-459`）；`store.rs:567-587` 在 `#[cfg(test)] mod atomic_tests`（`store.rs:550-551`）；**`ephemeral.rs:54` 是唯一例外**——位于 `pub fn with_seed`（`ephemeral.rs:45-63`，非 `#[cfg(test)]`，注释标「**仅供测试与本地开发**」），断言「新建实例的锁不可能中毒」，属不可失败路径；该处此前已在 `rv1-wp3.md:117-121`（F8）登记、`verification.md:41` 归类、`rv5-final.md:92` 复核，故为**已登记的非阻断项**（且不在本轮点名的 `crates/identity-auth/src` 范围内）；<br>• 与 `verification.md:41` 的旧计数 **17** 的差恰为 `types.rs:256` 那一处（17−1=16）✓ 自洽；<br>• 范围如实声明：日志的 grep 只覆盖 `src`（不含 `tests/`）；`crates/identity-auth/tests/ports.rs:72,80,84` 仍有 `secret.digest().expect("摘要必须可构造")`，属测试代码，`AGENTS.md` §7 不禁。 |
| (c) 行为等价 | • **唯一非测试调用点已传播**：`pairing.rs:60` 用 `?`（不是 `let _ =`/忽略）。<br>• **失败分支在正常输入下不可达**：`Digest` 由 `crates/core/src/model/scalars.rs:167-173` 的 `newtype!(Digest, check_digest)` 生成，`new` 返回 `Result<Self, InvalidValue>`（宏体 `crates/core/src/model/mod.rs:29`），`check_digest`（`scalars.rs:255-261`）只接受 `is_base64url_32`（43 字符规范无填充 base64url）。32 字节 SHA-256 的 base64url 恰为 43 字符且末字符低 2 位为 0 → **必然通过**，与原先「不可变式断言」等价；原 `expect` 因此也从未在正常路径触发。<br>• **未引入新语义**：错误经 `#[from]` 映射为既有 `PairingError::Invalid`（`crates/identity-auth/src/error.rs:83-84` `#[error("值非法：{0}")] Invalid(#[from] InvalidValue)`）。该 `From` 在改动前已被同函数的 `PairingRecord::try_new(…)?` 需要（`pairing.rs:69` 附近的 `)?;`），因此本次是**复用既有映射**，不是新增失败分类；`begin_pairing` 在正常输入下**没有**新增可达失败路径。调用方也不会把该错误误当其它语义——它仍是「值非法」，不会与「熵源不可用」「目标/集合不一致」等分类混淆。 |
| (d) 合同措辞 | `grep digest docs/IDENTITY_AND_AUTH_CONTRACT.md`：`:201`「secret 只在创建方（或 claim 方）内存中存在；落库的只有 SHA-256 digest（`owned_pairing.secret_digest`）」、`:204`「Daemon 重启不能凭 digest 恢复 secret」——合同**未固定 `digest()` 的返回类型**，也不把 `PairingSecret` 列进 §5.1 的 `Authority` 公开入口，故签名改动**不产生合同漂移**、§5.1「全部公开入口」清单也不因此失真 ✓。 |

### P2-5 `KeyHandle` 的悬空 doc 注释 —— **已闭环，无同类残片**

| 项 | 内容 |
| --- | --- |
| 原 ID / 级别 | P2-5 / P2 |
| 本轮结论 | **闭环** |
| 关键证据 | `crates/identity-auth/src/port.rs:51-54` 现文：`/// 不透明引用：**不是**密钥材料，可以落进 SQLite 的引用列，但不得进日志（`SECURITY_DESIGN.md` §14.1）。` → `///`（空行）→ `/// `Debug` 只表明类型、不打印内容（`AGENTS.md` §7「日志不得记录密钥」）：引用文本本身是` → `/// 可落库的定位符，但把它写进日志会把「哪把密钥存在」暴露给日志读者，因此一律不打印。` → `:55` `#[derive(...)]`。冒号后有实际内容，**无悬空冒号、无空注释块**。 |
| 同文件同类残片 | `grep '^\s*///\s*$' port.rs` 的空注释行仅 `:52`、`:85`、`:87`、`:92`、`:151`、`:199`，其后**均紧跟正文**（`:86` 引导 `compile_fail` 块、`:88/93` 为代码块起始、`:152/200` 为说明），属正常段落分隔；`grep '///.*：\s*$'` 在 `crates/identity-auth/src` 仅命中 `port.rs:86`（「…用 `compile_fail` 文档测试固定：」后紧跟代码块，正常引导句）。**无同类残片**。 |

---

## 2. 新发现项

### P2-A（**范围外**、报告项、非阻断）`openspec/config.yaml` 的模块画像仍写两个 crate 未实现，且成员数过时

- **位置**：`openspec/config.yaml:11`（`context` 的「主要模块、入口与职责边界」条目）。
- **原文**：「主要模块、入口与职责边界：`crates/` 现有 **8 个**已登记成员（`acpr-transcript`、`acpr-wire`、`core`、`storage-sqlite`、`sync-protocol`、`node-link-protocol`、`acp-protocol`、`agent-host`）；**`node-link-client`、`identity-auth`、`identity-keystore`、`server`、`app` 与前端尚未实现**。职责边界唯一权威：`docs/MODULE_ARCHITECTURE.md`。」
- **证据（相反口径）**：`Cargo.toml:3-14` 实测 10 个成员（含 `crates/identity-auth`、`crates/identity-keystore`），`ls crates/` 亦为 10 个目录；`docs/MODULE_ARCHITECTURE.md:3`/`:215`、`docs/CORE_PORTS_AND_STORAGE.md:11`/`:1312`、`README.md:18-19`/`:28`、`AGENTS.md:98-99`、`docs/DEVELOPMENT_PLAN.md:14` 均为「两个身份 crate 已落地」。
- **影响**：纯画像/记录口径陈旧，无代码、合同或门禁语义依赖；但 `AGENTS.md` §12 把该字段定义为「项目画像事实」，后续 agentic run 的 recon 会直接读它，属可维护性缺口。
- **本地无门禁覆盖**：`scripts/agentic-gate.mjs:12-13` 只断言 `openspec/config.yaml` 的 `schema == agentic` 与 `x-agentic.configVersion == 1`，**不校验 `context` 文本**；`scripts/check-crate-boundaries.mjs` 只读 `docs/MODULE_ARCHITECTURE.md` §5 矩阵。因此 `npm run check:agentic` 通过（`du1-pv1.log` 转抄的「agentic OK」）**不能**发现本项。
- **最小修复**：把该 bullet 改为「`crates/` 现有 **10 个**已登记成员（`acpr-transcript`、`acpr-wire`、`core`、`storage-sqlite`、`sync-protocol`、`node-link-protocol`、`acp-protocol`、`agent-host`、`identity-auth`、`identity-keystore`）；`node-link-client`、`server`、`app` 与前端尚未实现。」
- **范围说明**：本项**不在**本轮点名的 grep 面（`docs/**`、`README.md`、`AGENTS.md`）内；我是在全库范围内核对「是否还有把这两个 crate 写成未实现的地方」时命中，故作为发现如实登记，**不判为本轮的 P2-2 未闭环**。是否需要在本交付单元内修掉由编排者决定（`openspec/config.yaml` 也不在本变更 `design.md:134-136` 声明的同步面内）。

**P0/P1：无。** 未发现由这 5 条修复（尤其 `digest()` 签名改动）引入的新口径矛盾或行为差异。

---

## 3. 附带只读确认（不扩大范围）

1. **合同 §5.2「挑战缓存有硬上限」段 ↔ `state.rs` 实现：仍一致，去重未丢语义。**
   - `docs/IDENTITY_AND_AUTH_CONTRACT.md:345-349`：「`MAX_CHALLENGES = 1024`，签发时先用注入时钟清扫已过期条目，满时淘汰**最早过期**的一条再插入（`state.rs` 的 `put_challenge`）」，并说明「只会被 `verify_proof` 消费的挑战…内存上界必须与真实并发连接数解耦」「被淘汰/被清扫的客户端拿到统一的证明失败分类…不泄露存在性，也不改变一次性消费语义」。
   - 实现：`crates/identity-auth/src/state.rs:22` `pub const MAX_CHALLENGES: usize = 1024;`；`:150-162` `put_challenge` 先 `retain(|_, record| !at_or_after(now, &record.expires_at))`（按**注入时钟**清扫）、`if self.challenges.len() >= MAX_CHALLENGES` 时 `min_by(|l, r| l.1.expires_at…cmp(…))` 取**最早过期**者 `remove`，再插入；`:146-149` 的注释与合同理由同口径。→ **一致**。
2. **`reports/` 的 du1-* 现状 ↔ 编排者声明：一致。**
   - 现存 `du1-*` = `du1-baseline.log`、`du1-candidate-build.log`、`du1-composition.log`、`du1-integration.md`、`du1-selfcheck-rg.log`（5 个），与编排者声明的清单**逐名相同**。
   - `du1-pv{1..5}.log` 与 `du1-linux-clippy.log` 此刻**不存在** → 与编排者「刚删除、新候选需由集成执行者重跑」的声明一致，**不记为缺陷**。
   - 但需如实登记其连带效果：`reports/du1-integration.md` §0（`evidence_paths`）、§4（PV 表）、§9 与 `reports/rv1-du1.md` 的 `evidence_paths` 都以 `du1-pv{1..5}.log` 为证据路径，**这些引用此刻不可解析**；按编排者声明这属预期（旧日志不得与新候选混用）。我未能读到 `du1-pv1..pv5.log`（不存在），故 `du1-integration.md` 里转抄的退出码/用例数（513/79/36/5、`Totals: 9 passed`）本轮**无法复核**。
   - `du1-selfcheck-rg.log` 头部自述 revision = `4f9c418`（工作区干净），与其末行计数 16 与本轮工作区实测一致 ✓。
   - 两报告（`du1-integration.md`、`rv1-du1.md`）均已在变更目录 ✓。

---

## 4. 未覆盖与不可确认项（必须照实声明）

1. **未执行任何命令**。本会话无 shell、无 git、不能运行 `npm run verify` / `cargo test` / `cargo metadata` / `node scripts/*.mjs`。所有退出码、用例数、门禁数、NUL 计数均为**转抄**（来自 `du1-integration.md`、`du1-selfcheck-rg.log`），本轮未独立复算；我只对**可从磁盘自洽核对**的部分做了独立核对（如 16 命中的逐行复算、§5 公开面集合、合同行号与原文）。
2. **无提交区间 diff**。`watchdog_diff`（无 path、stat）本轮返回的是 **untracked 清单**：唯一一条 `openspec/changes/identity-auth-and-keystore/reports/rv1-du1.md`，**且未报告任何工作区 delta**，即 `工作区 == 我的 launch HEAD 4f9c418`。但「`4f9c418` 相对 `27938ca`/`ed98f6d` **只含**记录 `*.md` 与这 5 条 P2 的修复」是**编排者声明**，**我无法用 git 自证**；相应地「修复提交没有顺带改动其它代码/文档」我也无法证明——我只能证明**当前工作区在这 5 个点上的状态符合要求**。
3. **`ed98f6d` 的原始文本未逐字对照**。我不持有 `ed98f6d` 的快照，因此「P2-1 的重复块原先确实在 `:343-347`/`:350-354`」「`types.rs:256` 原先确为 `.expect(...)`」这类**差分判断**只能引用 `rv1-du1.md` 与 `verification.md:41` 的登记，不能独立复现；我确认的是**现状**（唯一一份块、`digest()` 返回 `Result`、`src` 内 0 命中）。
4. **Linux 运行时行为不在本地可证范围**：`#[cfg(unix)]` 的 0700/0600 真实权限位、非 Windows 失败关闭的**运行时**表现，Windows 开发机不可执行（与本轮 5 条 P2 无直接关系，仅作范围声明）。
5. **CI 专属判定不在可证范围**：`cargo-deny`（`deps`/`advisories`）与 `gitleaks`（`secrets`）本地无等价物；本轮 5 条 P2 均不触碰这两类判定。
6. **完整性与门禁未复跑**：`npm run check` 的十道门禁、`cargo fmt/clippy/test` 是否在 `4f9c418` 上仍全绿，我**未验证**（无命令能力）。本轮唯一的代码改动是 `types.rs::digest` 的签名与 `pairing.rs:60` 的调用点，`rustdoc` 与类型层面看不会引入 `clippy -D warnings` 违规（无未使用结果：返回值经 `?` 使用），但这是**推理不是证据**；建议 supervisor 至少执行：`cargo test --locked -p identity-auth --all-features`、`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`、`cargo fmt --all -- --check`、`npm run check`（含 `check:doc-links`/`check:agentic`，因为合同两处 + 两份权威文档被改动）。
7. **未复核范围**：6.4 的全量检视面（合同定型↔实现逐条、Cargo 成员与依赖、gitleaks 诚实性、两 crate 公共形状、`[PV2]` 与 §5 矩阵对应）**未重做**；`crates/identity-auth/tests/**`、`crates/identity-keystore/**`、`proposal.md`/`specs/**`、`tasks.md`/`verification.md` 的正文均未读或仅 grep 命中行；RV1–RV9 逐条结论未复核。
8. **不可确认的并行环境事实**：`du1-integration.md` §7 登记的「双写者并发污染 → 单写者重建」我无法复核（无文件历史）；我只确认现存 5 个 `du1-*.log`/`.md` 存在、`du1-selfcheck-rg.log` 的 revision 与计数自洽。

---

## 5. 实际读过的文件清单

**源码（定向读 + grep）**
- `crates/identity-auth/src/types.rs`（:230-310，`PairingSecret::digest` 全文与导入块）、`pairing.rs`（:25-125 含 `begin_pairing` 与调用点，及全部 `pub fn` 清单）、`port.rs`（:40-120、:149-201，含 `KeyHandle` doc 注释段）、`error.rs`（grep `InvalidValue`/`from`）、`state.rs`（grep `MAX_CHALLENGES`/`put_challenge` 段）、`authority.rs`/`handshake.rs`（全量 `pub fn`/`pub async fn`/`pub(crate)` 枚举）、`lib.rs`（grep 重导出）
- `crates/core/src/model/scalars.rs`（`newtype!(Digest, check_digest)` 段）、`mod.rs`（`new` 宏体）、`model/error.rs`（`InvalidValue::Digest` 文案）
- `crates/identity-keystore/src/ephemeral.rs`（:1-90）、`crates/identity-keystore/src` 的 `#[cfg(test)]` 边界与 `expect(` 全量命中
- `crates/acpr-wire/src/lib.rs`、`crates/sync-protocol/src/auth.rs`（同名类型旁证，grep）
- `crates/identity-auth/tests/ports.rs`（grep `.digest()`）

**构建与配置**
- `Cargo.toml`（workspace members）、`openspec/config.yaml`（:1-25）

**权威文档**
- `docs/IDENTITY_AND_AUTH_CONTRACT.md`（§2 :44-51、§3 :52-72、§5.1 :225-364、§7 grep）
- `docs/MODULE_ARCHITECTURE.md`（:3-7 状态与修订记录、:215、:404-418，grep `identity-keystore` 全量）
- `docs/CORE_PORTS_AND_STORAGE.md`（:9-13、:1305-1314，grep 全量）
- `README.md`（:14-45 crate 表与状态段，grep `identity-*` 全量）、`AGENTS.md`（:88-104 §4 状态表、:256 §9，全库 grep 命中行）、`docs/DEVELOPMENT_PLAN.md`（:1-40 §2/§3）

**变更资产与证据**
- `reports/rv1-du1.md`（全文，§3 五条 P2 与 §4/§5）、`reports/du1-integration.md`（全文）、`reports/du1-selfcheck-rg.log`（全文）、`reports/rv1-wp3.md`（grep `ephemeral`/`锁不可能中毒`）、`reports/rv5-final.md`（grep `锁不可能中毒`）
- `reports/` 目录清单（`ls`，确认 du1-* 现状）
- 全库 grep 命中行：`verification.md:41`、`plan.md`、`design.md`、`tasks.md`、`proposal.md`、archive 变更记录

**工具输出**
- `watchdog_diff`（stat，无工作区 delta；untracked = `reports/rv1-du1.md`）、`ls crates/`

---

## 6. 结论

1. **5 条 P2 是否全部闭环**：**是**。
   - P2-1（合同 §5.2 重复块）：**闭环**（仅剩 1 份，上下文与 `[决定]` 列表完整）。
   - P2-2（三处「未实现」）：**闭环**（`MODULE_ARCHITECTURE.md:215`、`CORE_PORTS_AND_STORAGE.md:11`、`:1312` 三处均已统一，且与 README/AGENTS/DEVELOPMENT_PLAN 同口径；`docs/**` 内无其它反向陈述）。
   - P2-3（§2 类型归属 + §5.1 公开面）：**闭环**（两条归属说明已加并与源码一致；`Authority::new` 已登记；公开集合与实测**逐项相等，无未登记项**）。
   - P2-4（正常路径 `expect`）：**闭环**（`digest()` 返回 `Result<Digest, InvalidValue>`，`pairing.rs:60` 用 `?`；`identity-auth/src` 命中 0，`identity-keystore/src` 16 命中与日志逐项相等，非测试模块仅 `ephemeral.rs:54` 一处且为既有已登记项）。
   - P2-5（悬空 doc 注释）：**闭环**（`port.rs:51-54` 为完整句子；同文件无同类残片）。
2. **`digest()` 的签名改动是否行为等价、无新增失败语义**：**是，行为等价且无新增失败语义**。唯一非测试调用点以 `?` 传播；`Digest::new` 的失败分支在正常输入下**不可达**（43 字符规范 base64url 恒成立）；错误映射到**既有**的 `PairingError::Invalid(#[from] InvalidValue)`，`begin_pairing` 在正常输入下没有新的可达失败路径，也没有可被误读为其它语义的分类。
3. **新发现**：1 条 **P2**（`openspec/config.yaml:11` 模块画像仍写两个 crate 未实现、成员数仍为 8），**在本轮点名范围之外**、无门禁覆盖、无行为影响，**不构成阻断**；最小修复已在 §2 给出。
4. **Merge verdict：OK with notes**（0 × P0、0 × P1；5 条 P2 全部闭环；唯一 note 为 §2 的范围外 P2，可由编排者决定是否在同一交付单元收口）。本结论**不等于已合入，也不等于最终验收 PASS**。