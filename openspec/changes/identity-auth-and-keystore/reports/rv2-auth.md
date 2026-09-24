<!-- 落盘说明：主 Agent 原样保留 reviewer 的正文与结论；仅当某处 `§` 引用会触发文档引用门禁的歧义归属时，补全被引文档名（不改变语义）。 -->
<!-- 由独立 reviewer 子 Agent 产出（RV2 轮，target revision fe52694），主 Agent 原样落盘。 -->
## 结论

**FAIL**（target revision = `fe52694`；无安全/数据破坏类 P0，但有 **2 条 P1** 应在发布前修复，另有 **1 条 P1 证据完整性**问题）。就 `plan.md` 的**字面阻断标准**（依赖方向、秘密不落可观察位置、规格场景缺覆盖、无 `from_der`、一次性消费先于验签、无跨 `await` 持锁）而言全部满足；本轮 FAIL 的依据是：合同 `§4.3` 的 **MUST**（secret "最迟在 expires_at 清除"）在"被拒绝"路径上**未实现**，而 `verification.md` 却把一条**不存在的修复**记为已完成（F1）；以及新增用例 `challenge_cache_is_bounded` **恒真、无法失败**（F2）。

---

## 1) 隔离与版本声明

- **隔离**：本 Agent 为新建子 Agent，未继承实现对话，未参与 `identity-auth` 的实现或实现讨论；本轮**只读**（未改代码/测试/计划/tasks/verification，未提交、未合并、未修复，未写任何文件，包括 `progress.md`——review-only 优先）。
- **版本**：`watchdog_diff` 报告"工作区相对 reviewer-launch HEAD `fe526941614d` 无任何变更"，即**我读到的文件内容 = 目标修订 fe52694 的内容**。我**不能**证明 fe52694 就是 `feat/identity-auth-and-keystore` 的 HEAD（该结论来自调度者声明）。
- **重要限制（如实声明，符合任务书）**：`watchdog_diff` **不覆盖已提交区间**，因此我**没有**读过 `6861046..fe52694` 的逐提交 diff，也**没有**核对"本轮只改了列出的 6 项 + 合同 0.3"这一范围声明。本轮实质是**目标修订内容 + 契约/规格的逐行检视**，不等价于 commit-range diff 检视。
- **未执行任何命令**：我没有 `cargo`/`npm` 可用，因此下文所有"通过/全绿"均来自既有日志或我对源码的静态推断；本轮声称的日志经我核实**不存在**（见 F3）。
- **隔离限制自述**：我只能声明自己收到的输入与限制，不能证明宿主未注入其它上下文。

## 2) 已读取范围

- **契约/规格**：`docs/IDENTITY_AND_AUTH_CONTRACT.md`（§1–§6.2，含 0.3 版本注记、§4.1 目标形状块与已实现形状块、§4.3、§5.1、§5.2、§14 相关行）；`specs/identity-pairing/spec.md`（全文）、`specs/identity-handshake/spec.md`（全文）；`plan.md`（全文，含 Coverage Index 与 Verification Strategy）、`verification.md`（全文）、`openspec/changes/identity-auth-and-keystore/reports/rv1-wp2.md`（上一轮报告全文）、`design.md` D3/D4 相关段。
- **源码（全读）**：`crates/identity-auth/src/{pairing,handshake,state,types,authority,error,lib}.rs`。
- **测试（全读相关段）**：`tests/{pairing,handshake,transcripts}.rs` 全部相关用例 + `tests/support/mod.rs`（全文）；`tests/` 目录清点（**无** `parity.rs`）。
- **对照物**：`crates/core/src/model/identity.rs`（`PairingState::is_terminal`、`PairingRecord::try_new` 不变式）；`fixtures/{sync,node-link}/v1/transcripts/*.json`（12 向量的 `hmacSha256`/`p1363Signature`/`sas` 键清点）；`.gitignore`（`openspec/changes/**/reports/**/*.log`）。
- **证据日志**：`openspec/changes/identity-auth-and-keystore/reports/` 目录**全量清点**（27 个文件）——**本轮 `rv2-*.log` 全部缺失**（F3）。

## 3) 逐条发现

### F1（P1）：被拒绝配对的 secret 在到达 `expires_at` 后**没有任何清除路径**，且三处权威记录声称"已清除"

- **位置**：`crates/identity-auth/src/pairing.rs:315-328`（`due_pairings`）；`pairing.rs:260-266`（`settle` 文档注释）；`docs/IDENTITY_AND_AUTH_CONTRACT.md:149`；`docs/IDENTITY_AND_AUTH_CONTRACT.md:213`（§4.3）；`openspec/changes/identity-auth-and-keystore/design.md:72`；`openspec/changes/identity-auth-and-keystore/verification.md:105`；`crates/identity-auth/tests/pairing.rs:414-421`、`468-475`。
- **事实（关键：任务书描述的行为与实际代码不符）**：任务书写的"`due_pairings` 现在对**所有**到达 `expires_at` 的记录清除 secret，但返回值只含未终结记录"**没有实现**。实际代码把 `clear_secret` 放进了同一 `filter` 内：

  ```rust
  .filter(|record| !record.state().is_terminal() && at_or_after(&now, record.expires_at()))  // pairing.rs:320-322
  .map(|record| { self.state().clear_secret(record.id()); record.id().clone() })             // :324
  ```
  而 `PairingState::is_terminal() == Rejected | Expired | Consumed`（`crates/core/src/model/identity.rs:418-420`）——**`Rejected` 是终结态**，因此被拒绝的配对**永远不进清扫**。全 crate 的 `clear_secret` 调用点只有 3 处（`due_pairings:324`、`unrecoverable_after_restart:339`（只收 `created`/`pending_confirmation`）、`complete:408`）+ `reset_memory`（`authority.rs:145-149`，启动语义）。**结论：被拒绝配对的 32 字节 secret（连同 nonce 与 request id）会一直驻留 `Authority` 的内存 map，直到进程退出**，违反 `specs/identity-pairing/spec.md` 的 R20 要求原文（"secret MUST 最迟在原过期时间被清除；被拒绝的配对…MUST NOT 超过该时间"）与合同 §4.3:213。
- **佐证（自相矛盾的记录）**：`pairing.rs:265-266` 注释写"两条路径的上界都是 `expires_at`，由 `due_pairings` 在过期时统一清除"；`verification.md:105` 写"`due_pairings` 对**所有**到达 `expires_at` 的记录清除 secret（secret 生命周期的硬上界）"。两者都与代码相反。
- **用例为何没抓到**：`tests/pairing.rs:414-421`（批准态用例）与 `:468-475`（拒绝态用例）在断言"到达 `expires_at` 必须清除"时传给 `due_pairings` 的是 **`created.draft.record`（状态 `Created`，非终结）**，不是被落定的那条记录；因此断言恒真，拒绝路径的"永不清除"被掩盖。`tests/pairing.rs:643`（`rejection_survives_until_original_expiry`）反而**显式断言** rejected 记录不在 `due_pairings` 返回集里——即当前行为被固化。
- **影响**：① 规格/合同 MUST 未实现（secret 生命周期上界失效）；② 内存条目随被拒绝的配对数量单调累积（每次约 100+ 字节，含 secret/nonce/id），且**没有清理 API**（`clear_secret` 是 crate 私有）；③ `verification.md` 记录了一条**未实现**的修复——证据链与实际不符，比缺陷本身更值得纠正。**无安全可利用性**：被拒绝配对的 secret 已无消费者（`verify_claim` 在 `pairing.state() != Created` 时就 `NotClaimable`；`pairing_sas` 要求 `PendingConfirmation`；`complete` 只清除），也不落库，因此不是 P0。
- **最小修复建议**：把清除移出过滤（返回集仍只含未终结记录），并补一条**用 `rejected` 记录**（terminal）的用例断言 `has_secret == false`；同时修正 `pairing.rs:265-266`、契约 `:149`、`design.md:72` 的措辞（`settle` 两条路径都**不**清除）。

### F2（P1，测试）：`challenge_cache_is_bounded` 恒真——即使删掉上限与清扫逻辑它也会通过

- **位置**：`crates/identity-auth/tests/handshake.rs:626-641`；`tests/support/mod.rs:212-243`（`SequenceEntropy`）；`src/state.rs:118-134`；`src/handshake.rs:139-152`（唯一写入点）。
- **事实与算术**：该用例循环 `MAX_CHALLENGES + 64 = 1088` 次 `hello`，然后断言 `challenge_cache_len() <= MAX_CHALLENGES`。但 `ChallengeId` 由注入的 `SequenceEntropy` 生成，而它的计数器是 **`Mutex<u8>` + `wrapping_add`**（`support/mod.rs:216/235`）。每次 `hello` 恰好消耗 48 字节（`server_nonce` 32 + `challenge_id` 16，`handshake.rs:80/83`），故 id 起始偏移的轨道为 `{48k+32 mod 256}`，大小 = `256/gcd(48,256) = 16`——**整个用例里只有 16 个互不相同的 `connectionId`**，`challenges` 是 `BTreeMap<connectionId, _>`，因此缓存条目数**恒 ≤ 16**（且反复覆盖同一批键）。于是：`<= 1024` 恒成立；`retain` 清扫分支（同一 `FakeClock`，所有条目 `expires_at` 相同）与 `len() >= MAX_CHALLENGES` 淘汰分支**都从未执行**。这也是"无恒真断言"判据下最清楚的一例（任务书第 6 项点名要判断的"是否真能失败"）。
- **影响**：本轮为 R30 加的"资源边界"防线是**虚假保证**——`MAX_CHALLENGES`/清扫/淘汰的实现若被删或被改坏（例如把 `>=` 写成 `>` 而永不淘汰、或把 `retain` 谓词取反），现有 73 个用例不会失败；且该用例也无法证明"不再单调增长"（它证明的只是"≤16"）。实现代码本身经我逐行确认是**正确**的（`state.rs:121-134`：先 `retain(!at_or_after(now, expires_at))`，满则 `min_by(expires_at)` 淘汰一条，再插入，故尺寸上界 = `MAX_CHALLENGES`）。
- **最小修复建议**（测试侧）：让 fake 熵源跨 `fill` 使用更宽的状态（如 `Mutex<u64>` 计数器按字节递增），使 1088 次 `hello` 得到互异 id；或把淘汰/清扫拆成两条用例（推进 `FakeClock` 到过期后断言旧条目被清扫；构造满缓存后断言 `challenge_cache_len() <= MAX_CHALLENGES` 且最旧一条被淘汰）。

### F3（P1，证据完整性）：本轮声明的日志 `reports/rv2-*.log` 在磁盘上不存在

- **位置**：`openspec/changes/identity-auth-and-keystore/reports/`；引用方 `verification.md:66`（`reports/rv2-pv1.log`、`rv2-pv2.log`、`rv2-pv3.log`、`rv2-pv4.log`、`rv2-pv5.log`、`rv2-linux-clippy.log`）、`verification.md:72`/`105`/`107`、`verification.md:67`（`rv2-selfcheck-rg.log`）。
- **事实**：`ls` 该目录得到 **27 个文件**（`bv-pv2..pv5`、`du1-pv1`、`pv5-windows-dpapi`、`rv1-wp1..wp4`、`w0-*`、`wp2-*`、`wp3-*`、`wp4-boundaries`），**没有任何 `rv2-*` 文件**；对 `reports/rv2-pv3.log` 的读取返回 `ENOENT`。这些日志被 `.gitignore:20` 忽略（因此不在版本控制内），但**上一轮的 `wp*/bv*/du1*` 日志都还在磁盘上**，说明不是统一清理。
- **影响**：本轮（RV1 修复轮）的 PV1/PV2/PV3/PV4/PV5 与"自检"结论**全部无法核对**；任务书要求我引用的"73 用例"我也**不能**引用为证据（我只能从源码用例清单算出 15+19+23+9+5 = 71 + 2 个 `compile_fail` 文档测试 = 73，与 `verification.md:66` 的"identity-auth 73 用例 / pairing 23 条"自洽，但这不是执行证据）。这不影响我对代码的静态判断，但**阻碍 Check ID 核对**，且与 `verification.md` 当前记录的 PASS 不符。
- **最小修复建议**：在候选轮次重跑并落盘（若本轮日志尚未生成，请在 3.3/3.4 前补齐）；建议同时按 RV1-WP2 §5-2 的要求在日志头部记录 `git rev-parse HEAD` 与 `git status --porcelain`（现有日志均无 revision 标注，无法自证来源）。

### F4（P2）：本轮新增/改变的两处实现约束**未回写权威文档**（合同 0.3 的对齐不完整）+ 一处本地错误文案不适配

- **位置与事实**：
  - (a) `docs/IDENTITY_AND_AUTH_CONTRACT.md:149` 仍写"拒绝可从 created/pending_confirmation 进入。**两条路径都清除内存 secret**"——与实现相反，且与**同一文档** §4.3:213（rejected 可保留到原过期时间）自相矛盾；`design.md:72` 同句未改。这正是 RV1-WP2 **P2-2** 的残余：0.3 只把**签名**对齐了实现，注释句仍停在草案。
  - (b) `docs/IDENTITY_AND_AUTH_CONTRACT.md:341`（§5.2）是"挑战/nonce 一次性与缓存"的权威节，但**未登记**本轮新增的硬上限 `MAX_CHALLENGES = 1024` 与"满则淘汰最早过期条目"（`state.rs:21-22/118-134`）。`AGENTS.md` §10 要求 nonce/重放规则的实现约束变化回写该节；实现侧的注释本身也以 `AGENTS.md` §5（数量/资源限制）为依据。
  - (c) 新增的 `trust.peer != submission.peer → UntrustedPeer`（`handshake.rs:192-195`）未登记在 §5.1（握手入口的权威节，`:280-300` 的"已实现形状"块）；且 `src/error.rs:102` 的文案"对端没有持久化信任材料（未知或不可用）"不适用于"快照主体与请求主体错配"，会让本地诊断误判原因（对端可见分类不受影响，仍是统一 `AuthenticationFailed`）。
  - (d) 新增公开访问器 `Authority::challenge_cache_len()`（`authority.rs:151-153`）与常量 `MAX_CHALLENGES`（`lib.rs:44`）未出现在 §4.1/§5.1 的入口清单里；连同既有的 `has_secret`/`failure_count`，这些"测试/诊断用"入口进入了**公开 API**（`AGENTS.md` §7 要求保持公开 API 最小）。`is_usable()` 的删除方向是对的（全仓 grep 零残留引用）。
- **影响**：切片 4–7 的 adapter/组合器按 §4.1:149 理解会认为"落定后 secret 已清除"，进而可能自行持有 `PairingDraft.secret` 或错判 `consume_pairing` 的语义；§5.2 缺上限会让后续实现者在"缓存不设界"上重复踩坑。非运行期行为缺陷。
- **最小修复建议**：§4.1:149 改为"`settle` 不改变内存 secret：批准后保留到首次认证成功，拒绝后保留到 `expires_at`（`due_pairings`）"；§5.2 补一行"缓存硬上限 1024，满则淘汰最早过期条目，被淘汰的连接按统一证明失败并重新握手"；§5.1 补一行快照主体一致性检查；`error.rs:102` 文案改为"没有可用信任材料或快照主体不匹配"。

### F5（P2）：合同 §4.1 的"目标形状"代码块仍声明实现中**不存在**的类型与字段

- **位置**：`docs/IDENTITY_AND_AUTH_CONTRACT.md:80-115`（"`[决定]` 目标形状的入口（实现时新增到 `identity-auth`…）"）。
- **事实**：该块仍定义 `PairingTarget::{Device{canonical_origin}, Node{endpoint,kind}}`（与 §2 的"`PairingTarget` 是 core 的纯 token 枚举，已有并直接复用"以及实现相反）、`PairingDraft{target, display_name, requested, expires_at, secret}`（实现是 `{record, secret, server_nonce, pairing_request_id}`，`types.rs:435-448`），以及全仓不存在的 `ClaimVerification`/`SettlementRequest` 结构体。0.3 的版本注记声称"把 §2 清单里不存在的类型名（`HandshakeCompletion`/`ClaimVerification`/`SettlementRequest`）换成实现名"——**只改了 §2**，§4.1 的草案块未动。
- **影响**：文档自查自相矛盾（同一节内两份互斥形状）；下游按草案建模会编译不过（可发现，非静默）。
- **最小修复建议**：把该块标注为"历史草案，已被下一块取代"或直接删除，仅保留 §4.1 的"已实现形状"块。

### F6（P2，测试精度）：两条新增配对用例的命名/断言语义弱于其声称覆盖的性质

- **位置与事实**：
  - `tests/pairing.rs:691-710` `approved_pairing_still_accepts_status_proofs_before_first_auth`：唯一断言是 `has_secret == true`。本 crate **没有**"用 `Authority` 内存 secret 校验 pairing-status HMAC"的入口（`SyncPairingStatus::hmac` 的 key 由调用方从 `PairingDraft.secret` 自行持有），因此"仍接受状态证明"这一命题**未被任何断言覆盖**；`pairing.rs:264-265` 用"pairing-status 之类的 HMAC 证明"作为保留理由也因此不准确（保留的真实作用只有：作为"尚未被首次认证消费"的内存标记 + 满足 R23 的轮询保留）。
  - `tests/pairing.rs:713-730` `approved_secret_is_cleared_at_expiry_even_if_never_authenticated`：传给 `due_pairings` 的是 **`pending_record(...)`**（`PendingConfirmation`），不是文件里已有的 `approved_record()`（`:590`）。因 `Approved` 与 `PendingConfirmation` 都非终结、走同一分支，**当前**不会漏检；但若将来把 `due_pairings` 改成"跳过 `Approved`"，本用例仍会通过——即它**不保护自己命名的性质**。
- **影响**：非阻断；但对"批准态 secret 在过期时清除"这一 R20/R22 相邻性质，目前没有一条用例真正以 `Approved` 记录验证。
- **最小修复建议**：到期清除用例改用 `approved_record(&created.draft.record)`；状态证明用例在入口出现前改成断言实际入口（或改名，明确它验证的是"保留 secret"而不是"接受证明"）。

### F7（P2，跨 crate 边界假设，WP2 内无法判定）：`due_pairings` 返回"已批准但已过期"的记录，而 core 的记录不变式可能不允许对应的终态写集

- **位置**：`src/pairing.rs:315-328`（`!is_terminal()` 使 `Approved` 进入返回集）与 `:315` 的注释"调用方据此请求存储做 `expire_pairings`"；`design.md:73`；对照 `crates/core/src/model/identity.rs:481-483`。
- **事实**：`PairingRecord::try_new` 要求 `approved_at.is_some() ⟺ state ∈ {Approved, Consumed}`，因此 `Approved → Expired` 的写集**无法保留** `approved_at`（要么丢弃该事实，要么该写集不合法）。本轮把 `Approved` 的 secret 上界交给 `expires_at` 后，这条缝会更常被走到。
- **影响**：切片 4/5 需要决定"已批准但过期"的落库终态语义；不影响 WP2 内部正确性。
- **最小修复建议**：在 `due_pairings` 的文档/合同 §4.1 明确"已批准记录返回后由调用方按 §11.6 决定终态（不得丢弃 `approved_at` 必要性由存储侧定）"，或把已批准记录排除在"待 expire"集合之外并另设清理路径。

### F8（P2，报告项）：`complete_auth` 用"内存里还有 secret"代理"已批准且未被消费"，无法保证 `consume_pairing` 一定是已批准配对

- **位置**：`src/pairing.rs:399-414`；对照 `docs/IDENTITY_AND_AUTH_CONTRACT.md:283`（"`consume_pairing: Option<PairingId>`，已批准配对的首次认证时非空"）；规格 R54。
- **事实**：入口只收到 `Option<&PairingId>`，判断依据是 `state().clear_secret(pairing)` 的返回值（secret 存在）。`created`/`pending_confirmation`/`rejected` 记录**也可能仍持有 secret**（本轮之后 rejected 一定持有），因此若 adapter 传入这些配对 id（例如同一对端已存在旧信任、仍能通过握手时），会得到错误的 `Some`，进而请求把未批准/已拒绝的配对推进为 `Consumed`（而 `Consumed` 要求 `approved_at`，见 F7 的同一不变式）。反向窗口同样存在：`due_pairings` 已清除 secret 但存储仍为 `approved` 的瞬间，首次认证得到 `None`，该配对不会进 `consumed`（影响仅限记账/审计）。
- **影响**：需要 adapter 侧选择 id 有偏差才可达；无语义安全影响（不产生信任、不放宽授权），属"内存即状态代理"的记账不一致 + 契约措辞未被入口强制。
- **最小修复建议**：在 `PairingMaterial` 里加一个"已批准"标记（`settle(Approve)` 时置位——内存态只允许比已提交状态更严格，与 `design.md:77` 的既有约定一致），或让入口接收配对状态/记录；至少在合同 §5.1 写明"由调用方保证传入该对端当前已批准的配对 id"。

---

### 已核对且**满足**的项（含本轮 6 项改动）

| 项 | 结论 | 证据 |
|---|---|---|
| 改动 1（`settle` 不清除 secret） | 与预期一致，且**方向正确**：修复了"批准后 `consume_pairing` 与 status-HMAC 都不可用" | `pairing.rs:267-313`（无 `clear_secret`）；`tests/pairing.rs:665`（批准后仍持有）、`:679`（首次认证后清除）、`:685`（第二次为 `None`）；F1 是它未完成的部分 |
| 改动 2（`MAX_CHALLENGES` / 清扫 / 淘汰 / `challenge_cache_len`） | 实现**正确**，**无绕过增长点**（`put_challenge` 唯一调用点在 `handshake.rs:139-152`；`take_challenge`/`clear_challenges`/`reset_memory` 只减不增）；一次性消费语义未被破坏（被淘汰者拿到统一 `UnknownChallenge` 并重新握手）；淘汰扫描 O(n≤1024) 在锁内同步执行，未跨 `await` | `state.rs:21-22/118-134`；`handshake.rs:147`（显式 `drop(state)`，`put_challenge` 调用不含 `await`） |
| 改动 3（`trust.peer != submission.peer → UntrustedPeer`） | **位置恰当**：在挑战消费（`handshake.rs:175`）与 kind/nonce/过期（`:176-191`）之后、绑定校验（`:196`）与验签（`:199-230`）之前；不泄露存在性（所有失败经 `HandshakeFailure::public_class()` 收敛为 `AuthenticationFailed`，审计动作仍按连接类型给（`SyncDevice→device.auth_failed`））；对既有用例零影响（`tests/handshake.rs:89-99` 的 `trust()` 主体与所有 `submission()`/node 快照主体一致，我逐处核对） | `handshake.rs:192-195`；`types.rs` 的 `HandshakeFailure::public_class`/`audit`；`tests/handshake.rs:643-663` |
| 改动 4（删除 `PeerTrust::is_usable`） | 已删除，全仓**零残留引用**（仅上轮报告提及）；`types.rs` 其余形状与 `lib.rs` 导出与合同 §2 的 30 个类型名**逐项一致**（我逐个核对，全部存在且已导出） | 全仓 `grep is_usable` 只命中 `reports/rv1-wp2.md`；`lib.rs:15-59` |
| 改动 5（固定向量精确断言） | HMAC 域已精确断言（`Some(true)` + 换 secret 必须 `Some(false)`），SAS 域用 `derive_sas` 与 `Sas::from_hmac_output` 精确比对；12 个向量中 6 个 HMAC、6 个签名、2 个 SAS 域，两条分支**都会被真实执行**（无静默跳过） | `tests/transcripts.rs:359-408`；fixtures 键清点（`sync|node-link` 各 3 HMAC + 3 签名，两个 `pairing-sas.json` 同时有 `hmacSha256`+`sas`） |
| 改动 6（新增 6 条用例） | 4 条可失败且非恒真（`first_authentication_consumes_the_approved_pairing_once`：改动前 `settle` 清 secret 时 `:665` 必失败；`trust_snapshot_must_belong_to_the_same_peer`：无该检查时会认证成功致 `expect_err` panic；`approved_pairing_still_accepts_status_proofs_before_first_auth` 可分辨修复前后；`approved_secret_is_cleared_at_expiry...` 分支真实）；R54 正向路径**已覆盖** | `tests/pairing.rs:649-688`、`:691-710`、`:713-730`；`tests/handshake.rs:644-663` |
| 回归面 | 无平台 `cfg`（`cfg(windows)/cfg(unix)/cfg(target_os` 零命中）、无 `from_der`、`src` 内 `unwrap()/panic!/unreachable!` 零命中、`expect(` 仅 `types.rs:256` 一处（有说明且不可达）；`src` 内 9 处 `.await` 与 RV1 相同且全在锁外；`src/state.rs` 新逻辑全同步 | 我本轮自行 grep 复核（不依赖日志） |

## 4) `plan.md` 覆盖索引与本 WP 的 Check ID 核对

- **WP2 行**：`plan.md` 的 Work Packages 表定义 WP2 写范围 = `crates/identity-auth/**`（含 `Cargo.toml`）+ `members` 条目 + `Cargo.lock`；Verification = `[PV3]`（+ `[PV1]`）。Coverage Index 中 **R1–R71 → tasks `2.4/2.5/2.7` → checks `[PV3]`**，证据路径为 `reports/wp2-identity-auth-{pairing,handshake,expansion,ports}.log`（**这些文件确实存在**，但它们是**上一轮 revision 6861046** 的产物，不能作为本轮 fe52694 的证据）。
- **Check ID 逐项**：

| Check ID | 计划判据 | 本轮核对 | 是否影响本轮判断 |
|---|---|---|---|
| PV3 | `cargo test --locked -p identity-auth --all-features`：12 向量重算/SAS/畸形拒绝/失败路径全过、无 0 用例、无全跳过 | **待核对（证据缺失）**：`reports/rv2-pv3.log` 不存在（F3）。静态侧我只能核对用例清单（71 + 2 doc-tests = 73，与 `verification.md:66` 的"73 用例/pairing 23 条"自洽）；F1/F2 两条缺陷的可用例化方案已给出 | 不改变我对 F1/F2 的判定（判据来自源码与规格），但把"本轮 PV3 通过"记为未证实 |
| PV1 | `npm run verify` 全绿 | **待核对**：`reports/rv2-pv1.log` 不存在；`du1-pv1.log` 属 6861046 轮次 | 否（不影响静态结论） |
| PV2 | `check-crate-boundaries.mjs`：`identity-auth` 行只依赖 core/两协议/`acpr-transcript`，`acpr-wire` 格为空 | **待核对**：`reports/rv2-pv2.log` 不存在；`wp2-boundaries.log` / `wp4-boundaries.log` 属旧 revision。本轮未新增依赖（我读了 `crates/identity-auth/Cargo.toml` 未变动的证据 = 无新增 import），**未发现**依赖方向问题 | 否 |
| 计划自检行（`rg "unwrap\(|expect\(|from_der"`） | 非测试代码可失败路径零 `unwrap/expect` | **待核对**：`reports/rv2-selfcheck-rg.log` 不存在；我用 grep **自行复核**：`from_der`/`unwrap()`/`panic!`/`unreachable!`/平台 `cfg` 零命中，`expect(` 1 处（`types.rs:256`）——与 `verification.md:67` 的记录一致 | 否（我已独立复核） |
| RV1 | 独立 reviewer 报告完整、无未解决阻断项 | 本报告即本轮（RV2-WP2）输入；上轮 `reports/rv1-wp2.md` 的 P2-1/P2-5/P2-7/P2-8(b)/P2-2 有处置记录，P2-3（`tests/parity.rs`）**未见处置**（见 §5） | 是 |

- **R 编号 → 用例映射的本轮变化**：R54 正向覆盖点现在在 `tests/pairing.rs:649`（原判断为"无覆盖"）；R22（批准后提前清除）现在由 `:649-688` 断言（原由 `approve_requires_pending_and_within_requested` 断言 `has_secret == false`，该用例已改为断言"保留 + 到期清除"）；**R20 的"最迟在 `expires_at` 清除"仍只对未终结记录成立（F1）**；R30 被新用例借用的"资源边界"实际恒真（F2）。
- **编号不一致（沿用 RV1 报告项，未处理）**：`tasks.md`（2.7 行）把展开场景编号为 `[R53]–[R68]`，而 `plan.md` 的 Coverage Index 把展开场景编号为 `R56–R71`、把端口/秘密类型归到 `R81–R84`（`[PV3, PV4]`）。同含义编号在两份文件里指向不同场景；不影响"Check ID + 日志路径"的证据链，但最终验收若按 R 编号逐行关联会错配，建议在 5.2/8.1 统一。

## 5) 未覆盖 / 无法确认项与所需证据

1. **未能读 `6861046..fe52694` 的已提交 diff**（工具限制）。因此"本轮只改了列出的 6 项 + 合同 0.3"这一范围声明我**未**核对；本轮结论只对**文件内容**负责。**所需**：`git show --stat fe52694`（任务书已给出该命令作为定位线索，但它的输出我无法在此环境复算，请主 Agent 保留该清单作为范围旁证）。
2. **本轮全部证据日志缺失**（F3）。**所需**：补齐 `reports/rv2-{pv1,pv2,pv3,pv4,pv5,linux-clippy,selfcheck-rg}.log`；若本轮日志尚未生成，请在 3.3（[PV3]/[PV2]/[PV1]）与 3.4（RV1 复核）门禁前补齐并在日志头部记录 `git rev-parse HEAD` / `git status --porcelain`（**这会影响 Check ID 核对，但不影响我对 F1/F2 的静态判定**）。
3. **F1/F2 的修复后复核**需由**新的**隔离子 Agent 用新 Review ID 按 F 编号逐条确认（尤其：用 `rejected` 记录断言过期清除、以及替换掉恒真的缓存用例）。
4. **RV1-WP2 P2-3 未闭环**：`tests/` 下无 `parity.rs`，但 `src/types.rs:32/40/73` 的注释仍写"等价比对由测试语料保证，见 `tests/parity.rs`"（`design.md` D8 要求一份接受/拒绝语料）。本轮 6 项改动未涉此项，也未在 `verification.md` 见到处置记录 → 建议明确"补 `tests/parity.rs`"或"登记 design D8 的调整"，避免"计划写了、证据里没有"。
5. **F7/F8 需要切片 4/5 的证据**（`core::ports` 的 `ExpiryWrite`/`PairingSettlementWrite` 语义、adapter 传入哪个 pairing id）。WP2 内无法判定，只能登记为待确认的边界假设。
6. **本轮未检视**：`crates/identity-keystore/**`（WP3，含 RV2-WP3 的修复）、`docs/MODULE_ARCHITECTURE.md`/`README.md`/`DEVELOPMENT_PLAN.md`/`AGENTS.md`（WP4）、`.gitleaks.toml`；这些请以对应的 RV2 报告为准。
7. **`npm run check` 的十道门禁、`cargo fmt/clippy`、跨平台 Linux 目标**：我未执行，也没有本轮日志可引；与 WP2 相关的判据（依赖方向、`cfg` 零命中、`from_der` 零命中）我已用 grep 独立复核，其余判据留给 Project Verify。