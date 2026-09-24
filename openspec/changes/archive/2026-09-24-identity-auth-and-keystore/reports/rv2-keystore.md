<!-- 落盘说明：主 Agent 原样保留 reviewer 的正文与结论；仅当某处 `§` 引用会触发文档引用门禁的歧义归属时，补全被引文档名（不改变语义）。 -->
<!-- 由独立 reviewer 子 Agent 产出（RV2 轮，target revision fe52694），主 Agent 原样落盘。 -->
# RV2-WP3 独立检视报告（identity-keystore 修复轮复核）

## 结论

**FAIL**（1 项阻断项：**G1**）。但需要把两条结论分开看，避免误读：

- **RV1-WP3 的两条阻断项（P0：Linux 上 `delete_secret` 返回 `SecretMissing`；P1：`get_secret` 在不可用平台返回 `Ok(None)`）经静态复核判定「已修复」**，且修复方式（入口级 `Availability::require()`）在实现与测试两侧自洽；本轮我没有发现产品代码回归。
- **唯一阻断项 G1 在测试与记录层**：spec 场景 R86 的唯一用例里那条「引用提交失败」死分支仍然存在（与 RV1 指出的位置同一处），而 `verification.md` 已把它记为「已修」。按 `plan.md` 的阻断标准「任一规格场景缺覆盖」，这使本轮不能记为 PASS。若主 Agent 认可该场景在本 crate 边界内已被 `design.md` D6 缩小为「新条目写入不干扰旧条目」（即接受该偏差并改写记录），则 G1 可降为 P2，本轮结论随之变成 PASS（附注 G2–G9）。

---

## 1) 隔离与版本声明

| 项 | 内容 |
| --- | --- |
| Review Type / Stage | code review（fix recheck），WP3 交付前复核轮 |
| Work Package | WP3（`crates/identity-keystore/**`、`.gitleaks.toml`） |
| Repository | `D:\Project\acp-remote`，分支 `feat/identity-auth-and-keystore` |
| Target Revision | `fe52694`（父提交 `6861046` = RV1 轮 target） |
| Review ID | RV2-WP3（本轮） |
| 隔离 | 新建子 Agent，未继承实现对话；本轮输入只有调度者任务文本、`openspec/schemas/agentic/roles/reviewer.md`、`AGENTS.md`、仓库文件。我只能声明自己收到的输入与限制，**不能证明宿主未注入其它上下文**。 |
| 只读边界 | 未修改任何代码/测试/计划/tasks/verification，未提交、未合并、未修复、未写文件（本报告由调度者落盘）。未执行任何构建或测试命令。 |
| 版本可信度 | `watchdog_diff`（launch HEAD = `fe526941614d`）= **无 staged/unstaged 改动**，因此我读到的文件内容对应 target revision。**限制（如实声明）**：`watchdog_diff` 不含 committed range，本轮也未提供 diff 工件，因此**我没有读过 `6861046..fe52694` 的逐提交 diff**，也无法判断哪些行属修复轮新增、哪些在 RV1 之前既有；下文结论只对「`fe52694` 的文件现状」负责。 |

**关于任务给出的证据日志路径**：任务与 `verification.md` 写的 `reports/rv2-*.log` **不在变更目录** `openspec/changes/identity-auth-and-keystore/reports/`（该目录下只有 `rv1-wp{1..4}.md` 与上一轮的 `wp*/bv*/du1*/pv5*` 日志），实际位于**仓库根的历史遗留目录** `D:\Project\acp-remote\reports\`（`rv2-pv1/pv2/pv3/pv4/pv5.log`、`rv2-linux-clippy.log`、`rv2-selfcheck-rg.log` 均在那里，我已读取）。见 G9。

---

## 2) 已读取范围

**代码（全量通读）**：`crates/identity-keystore/src/{lib,entry,store,error,entropy,ephemeral}.rs`、`src/platform/{mod,windows,unsupported}.rs`；`crates/identity-keystore/Cargo.toml`；`crates/identity-keystore/tests/{keystore,fail_closed,entry_format,permissions,dpapi}.rs`、`tests/support/mod.rs`；`crates/identity-auth/src/port.rs`（`SecretBytes` 清零与 `KeystoreError` 封闭分类）、`crates/identity-auth/tests/ports.rs`（引用存在于 WP2 侧，未逐行读）。

**配置**：`.gitleaks.toml`（全文）、`.gitignore`（日志策略）、`.github/workflows/ci.yml`（未重读，引用 RV1 已核事实）。

**需求与契约**：`openspec/changes/identity-auth-and-keystore/specs/platform-keystore/spec.md`（全文）、`plan.md`（全文，含 Coverage Index / Work Packages / Project Verify / 阻断标准 / Completion Criteria）、`tasks.md`（2.10–2.17、3.5–3.8 行）、`design.md`（D6/D7/D8/D9 与 Risks）、`verification.md`（全文）、`docs/IDENTITY_AND_AUTH_CONTRACT.md` §7 第 1–6 条与 §8、`reports/rv1-wp3.md`（上一轮报告全文，作为复核基线）。

**证据日志（读取，未复算）**：`reports/rv2-pv4.log`（1+5+6+8+12 = 32 用例 + `permissions` running 0 tests + `EXIT=0`）、`reports/rv2-pv5.log`（dpapi 5 用例 + `EXIT=0`）、`reports/rv2-linux-clippy.log`（`Checking identity-keystore`（linux 目标）+ `EXIT=0`）、`reports/rv2-pv1.log`（`9 passed, 0 failed (9 items)`、`crate boundaries OK: 10 个 crate`、`doc links OK: 369 relative links, 3454 section refs`、`EXIT=0`）、`reports/rv2-pv2.log`（boundaries OK）、`reports/rv2-pv3.log`（`EXIT=0`，本 WP 外）、`reports/rv2-selfcheck-rg.log`（4 处命中）。

**未读**：`crates/identity-auth/**` 其余实现、WP1/WP4 文档改动细节（范围外）；`schemas/`、`fixtures/`。

---

## 3) 逐条发现

先给任务点名的 7 个重点项与 7 个测试项的**逐条判定**（无问题处直接给结论），再列需要行动的发现。

### 3.1 任务重点项的逐条判定

**(1) 7 个入口的 `require()` 闸门与绕过路径 —— 成立（无发现）**

- 7 个入口的第一条语句都是 `self.availability.require()?`，且都在任何文件系统/句柄解析之前：`generate` `store.rs:224`、`public_key` `store.rs:232`、`sign` `store.rs:248`、`delete` `store.rs:264`（其前只有一行注释）、`get_secret` `store.rs:275`、`put_secret` `store.rs:290`、`delete_secret` `store.rs:297`。`sign` 的用途判定在闸门之后，与注释「可用性先于句柄形状」一致。
- 无绕过路径：`read_entry`(`store.rs:133`)、`write_entry`(`store.rs:152`)、`read_secret`(`store.rs:186`)、`remove_entry`(`store.rs:197`)、`generate_scalar`(`store.rs:207`)、`handle_of`/`parse_handle` 全部是**私有**函数，只能经 7 个入口到达；`wrap_secret`/`unwrap_secret` 只被 `write_entry`/`read_secret` 调用。`identity-keystore` 内没有第二个 `impl IdentityKeystore`（另一个实现是显式选择的 `EphemeralKeystore`，符合 spec「开发档位必须显式选择」）。
- 外部可见的两个未闸门公开方法见 **G4**。
- 反向风险已排除：即使组合根错误地以 `with_availability(..., Availability::Platform)` 在 Linux 上构造，`write_entry` 也会在 `wrap_secret` 处以 `PlatformUnavailable → Unavailable` 失败（`store.rs:170-171` + `platform/unsupported.rs:17-19`），**不会**降级为明文/内存实现。

**(2) `read_secret` → `SecretBytes` —— 调用点正确；仍有残余明文副本（G3）**

- 调用点：`public_key` `store.rs:233-234`、`sign` `store.rs:249-250` 都用 `secret.as_bytes()`；`get_secret` 直接返回 `SecretBytes`（`store.rs:276-281`）。类型上正确，`SecretBytes` 是 `Drop` 清零类型（`identity-auth/src/port.rs:128-131`）且不实现 `Debug`/`Clone`。
- 残余：`unwrap_secret` 的返回值仍是 `Vec<u8>`（`windows.rs:31-35`），`store.rs:189-193` 把它**复制**进 `SecretBytes` 后把原 `Vec` 直接 drop（未清零）→ 每次读/签名仍留一份未清零的明文堆副本；`EphemeralKeystore` 更彻底（`ephemeral.rs:23-24`、`75-79` 明文 `Vec<u8>` + `clone()`）。详见 G3。

**(3) `write_atomic` 失败路径与并发 —— 成立（无发现）；仅 1 条 durability 备注（G7）**

逐条核对 `store.rs:352-386`：`File::create` 失败(`:369`) / `set_permissions` 失败(`:374`) / `write_all` 失败(`:377`) / `sync_all` 失败(`:379`) 都以 `?` 返回，此时 `TempFileGuard`(`:362-365`) 仍处 armed，`Drop`(`:393-400`) 删临时文件；`rename` 失败(`:383`) 同样由 Drop 清理；成功路径 `guard.armed = false`(`:384`) 在 `rename` 之后，因此不会误删已改名目标。所有「返回 Err」的路径都不留临时文件。
并发/重入：临时名 `. {name}.tmp-{pid}-{seq}`（`:361`），`SEQUENCE` 为函数内 `static AtomicU64`（`:353`,`:360`），`fetch_add` 保证同进程内每次调用唯一 → RV1-F4 指出的「同进程并发写复用同一临时路径导致半截文件」已消除；`list()` 只识别 `.{FORMAT_VERSION}` 后缀（`store.rs:118-121`），临时名以 `-{seq}` 结尾，不会被误当条目。
残余（informational）：`Drop` 内 `let _ = fs::remove_file(...)`(`:397`) 会吞掉清理失败；`write_atomic` 只 fsync 文件、不 fsync 父目录（G7）。

**(4) `entry.rs::is_valid_label` 与 `entry_path` 一致性 —— 成立（无发现）；边界项见 G5**

- `is_valid_label`（`entry.rs:46-51`）= 非空 + `len() <= MAX_LABEL_LEN` + 无 `/`、`\`、`\0` + 无控制字符；`EntryHeader::new`（`entry.rs:133`）与 `decode`（`entry.rs:181`）都调用它，符合同一份校验、无不对称。
- `entry_path` 注释（`store.rs:99-104`）与实现一致：注释明确「调用方必须先校验标签」，并点出「把未校验文本传进来就会得到目录穿越」，实现只做拼接。`/` 与 `\` 都被拒，标签不可能包含路径分隔符 → 不存在 `../` 形式穿越。大小写折叠/保留字符见 G5。

**(5) `ephemeral.rs::entries()` 与保留的 1 处 `expect` —— 成立（理由成立，无发现）**

- `entries()`（`ephemeral.rs:38-40`）把锁中毒映射为 `KeystoreError::Unavailable`，5 处 `lock().expect(...)` 已消除。
- 保留的 `ephemeral.rs:50`（`with_seed` 内）：`keystore` 是**本函数刚构造的局部实例**，锁无人共享，毒化只可能由「持锁期间 panic」造成，而该处唯一持锁动作是 `BTreeMap::insert`（不 panic、不 panic-unwind）→ **理由成立**，不是可失败的外部输入路径。`verification.md` 的自检行也如实记录了这 4 处命中（`rv2-selfcheck-rg.log` 与我复核的 `grep` 结果一致：`types.rs:256`、`entropy.rs:37/38`、`ephemeral.rs:50`）。

**(6) 测试逐条判定（是否能失败 / 是否恒真 / 平台跳过）**

| 测试 | 真能失败？ | 平台行为 | 判定 |
| --- | --- | --- | --- |
| `fail_closed.rs::platform_availability_is_reported_honestly` | 能（改 `platform_supported` 常量即失败） | 两平台都执行 | 通过 |
| `default_availability_follows_the_platform`（`:25-36`） | 只能因**装配**改动失败（`new` 若硬编码 `Platform`，Linux 上必失败）→ 属 wiring 断言，非恒真 | 两平台都执行 | 通过（弱断言，见 G8） |
| `unavailable_platform_writes_nothing_at_all`（`:39-76`） | 能；**且 RV1-F1 的失败点就在这里**——修复后 `delete_secret` 走 `require()` 返回 `Unavailable`，断言已与实现一致 | Linux 真实执行；Windows 早退（`:40-43`，且已注明由强制用例补） | 通过（P0 修复的关键证据） |
| `unavailable_backend_fails_closed_on_every_entry_point`（`:78-143`） | 能（7 入口逐一断言 + `root.files()` 零写入 + 非法句柄顺序）；用注入缝，**任何平台都真执行** | 两平台都执行 | 通过（P0/P1 修复的核心回归） |
| `available_backend_is_not_unconditionally_unavailable`（`:146-166`） | 能（Windows 断 `is_ok`、非 Windows 断 `Err(Unavailable)`） | 两平台都执行（分支不同但都断言） | 通过（无恒真） |
| `errors_never_carry_secret_material`（`:168-185`） | `expect_err` 能失败；但 `!text.contains(plaintext)` 对「无载荷的枚举错误」**结构上不可失败** | 两平台都执行 | 通过，弱断言（G8） |
| `node_identity_entry_contains_no_plaintext_key`（`:187-210`） | 能（定序熵源使明文标量可预期） | Windows 真执行；非 Windows 早退（有注明） | 通过 |
| `corrupted_header_is_reported_as_corrupt`（`:212-235`） | 能 | Windows 真执行；非 Windows 早退 | 通过 |
| `keystore::salts_are_per_entry_and_plaintext_is_absent`（`:315-361`，含 R74 双向断言） | 能（`assert_ne!` 两公钥、`input.verify(&second_public_key, &first_signature).is_err()`） | Windows 真执行；Linux 在 `generate` 处早退（无条目，本就不可能有等价断言） | 通过（R74 已补，RV1-F3 修复）；测试名里的「plaintext_is_absent」在该用例内只由 magic + `len() > 41` 间接体现（明文缺席由 `fail_closed` 用例断言）——命名略大于断言，报告项 |
| `keystore::failed_reference_commit_keeps_the_old_entry_resolvable`（`:197-226`） | **不能**：`let reference_commit_failed = true; if !reference_commit_failed {…}`(`:212-215`) 恒假，`if` 体不可达 | Windows 真运行（`rv2-pv4.log` 显示 ok） | **G1（P1）** |
| `keystore::missing_referenced_entry_is_reported_without_recreation`（`:149-168`）、`purpose_mismatch_and_illegal_handles_are_rejected`（`:277-313`） | 能（改用 `available_store` = `Availability::Platform`） | **Linux 与 Windows 都真执行**（RV1-F2 把「EntryMissing 固化成语义」的问题已消除） | 通过 |
| `tests/permissions.rs::unix_permissions_are_private` | **不能**：两重早退在 Linux/Unix 上必然触发 | Windows 编译为空（`rv2-pv4.log`：`running 0 tests`）；Unix 早退 | **G2（P2）** |
| `dpapi.rs` 5 用例 | 能（真实 DPAPI，无 mock；直接调用生产 `wrap_secret/unwrap_secret`） | 仅 Windows（`#![cfg(windows)]`），`rv2-pv5.log` 5 passed | 通过 |

不存在 `#[ignore]`/`filtered out` 被用来掩盖待验证断言的情况（`rv2-pv5.log` 的 `filtered out` 是 `dpapi` 过滤器本身造成，属正常）。

**(7) `.gitleaks.toml` 注释与正则 —— 声明已诚实（F11 修复），但规则命中能力存疑（G6）**

- `:32-44` 已明确「本规则只覆盖 `ACPK` 的 base64/base64url 编码形态；原始二进制条目与 DPAPI 包裹后的密文都无法用模式识别」，`description` 也写了「（base64url/base64 编码形态）」→ 与正则 `(?i)QUNQS[A-Za-z0-9+_-]{16,}={0,2}` 一致，**不再过度声明**（RV1-F11 成立）。
- 新疑点：同一段落声明「`keywords` 只用于缩小评估范围，命中判定仍由 regex 决定」；若 gitleaks 的 `keywords` 是**前置过滤**（只在其出现的分片内评估该规则），则 base64 化后的文本不含字面 `ACPK`，该规则永不被评估 → 规则与「命中即说明被编码后的明文条目被提交」这一承诺不相符。本地无 gitleaks，无法执行确认（CI-only），故列为待确认项而非已确认缺陷。

### 3.2 需要行动的发现

**G1（P1，阻断）R86 的唯一用例里「引用提交失败」分支仍是死分支，而 `verification.md` 已记为「已修」**

- 位置：`crates/identity-keystore/tests/keystore.rs:212-215`（`let reference_commit_failed = true;` + `if !reference_commit_failed { store.delete(&old)… }`）；记录位置 `openspec/changes/identity-auth-and-keystore/verification.md` RV1-WP3 行的「Resolution」列（"R86 死分支改为真实两阶段替换语义"）与 `## Failures and Retests` 未提该项。
- 事实：条件恒假，分支体（唯一会调用引用/回收语义的代码）不可达；用例剩余断言只有「两个条目都可读/可签」，因此它证明的是「`generate` 新标签不覆盖旧标签」，而不是「引用提交失败时旧引用仍可用、新条目不被当作有效身份」。`reports/rv2-pv4.log:59-63` 显示该用例 `ok`——**一个恒真/空转的用例同样会绿**，日志不能作为该场景的覆盖证据。RV1-WP3（`reports/rv1-wp3.md` F9）在 `6861046` 上已就同一构造判 P2；本轮**代码未变**（构造逐字保留），但 `verification.md` 已把它写成已修，属新增的、独立于 RV1 的问题。
- 影响：① 命中 `plan.md` 的阻断标准「任一规格场景缺覆盖」——R86 是 `specs/platform-keystore/spec.md` 的 Scenario，其 WHEN 的第一半（引用更新提交失败）从未被模拟，THEN 的第二半（新条目不被当作有效身份）无任何断言；`plan.md` Coverage Index 的 R86 行只指向本轮这条用例；② 最终验收会把 `verification.md` 的「已修」当作闭环证据读取，而该陈述与文件内容不符，属可核对为假的记录。
- 建议（最小）：二选一——(a) 删掉 `reference_commit_failed` 与整个 `if`，把用例名/注释改为它实际断言的性质（「写入新标签不干扰旧条目」），并在 `verification.md` 如实写「R86 在本 crate 边界内按 design D6 缩小为……，未模拟引用层」；(b) 用一个最小的 fake 引用持有者（`commit()` 返回失败）驱动 `delete` 决策，断言旧 handle 仍可解析、新 handle 不被引用。无论哪种，都必须同时订正 `verification.md` 的 Resolution 文本。

**G2（P2）`tests/permissions.rs` 在任何平台都不执行任何断言；且 Unix `0700`/`0600` 代码路径在当前构建下不可达 → RV1-F5 实质未闭合**

- 位置：`crates/identity-keystore/tests/permissions.rs:8`（`#![cfg(unix)]`）、`:19-25`（`if !platform_supported() { return; }`）、`:29-32`（`let Ok(handle) = … else { return; }`）；实现 `src/store.rs:334-345`（目录 `0700`）与 `:373-376`（文件 `0600`）；证据 `reports/rv2-pv4.log:65-71`（`Running tests\permissions.rs` → `running 0 tests`）。
- 事实：该文件只有 1 个测试。`platform_supported()` = `cfg!(windows)`（`src/lib.rs:33-35`），因此在 [unix]（= 文件唯一被编译的平台）上第一个早退**必然**触发，测试体永不执行；即使去掉第一个早退，`generate` 也会在 `wrap_secret` 处失败（`platform/unsupported.rs:17-19`）而被第二个早退吞掉；在 Windows 上整个 crate 被 `#![cfg(unix)]` 清空——日志证据正是 `running 0 tests`。文件头注释（`:1-5`）「Linux CI 上是真实的权限位断言」与事实相反。
- 附带的更根本事实：`store.rs` 的两处 `#[cfg(unix)]`（`:341` 目录 chmod、`:374` 文件 `0600`）在**任何构建里都不可能执行**——在 unix 上 `wrap_secret` 恒失败、`write_entry` 在此之前返回，在 Windows 上被 cfg 剔除。因此 `design.md` D6 / 任务 2.10 的「Unix `0700`/`0600`」目前既无执行路径也无断言，且 `create_private_directory` 对**已存在**目录不收紧权限（`store.rs:334-336`）。
- 影响：安全相关的默认权限（私有目录/文件模式）没有任何回归护栏；一处常量写错（例如 `0o600 → 0o644`）不会被任何测试发现；`verification.md`/本轮任务描述把该文件当作可执行证据，会高估覆盖率。
- 建议（最小且可执行）：把 `create_private_directory`/`write_atomic` 在 `src/store.rs` 内加 `#[cfg(all(test, unix))] mod tests`（同模块可调用私有函数，无需 DPAPI），直接断言目录 `0700`、文件 `0600`——这会随 crate 的 lib 单测二进制在 Linux CI 真实执行；并删掉 `permissions.rs`（或在保留时把它改成只覆盖「可用平台」形态并写明其不可达条件），同时纠正文档与记录中的覆盖率表述。

**G3（P2）明文堆副本未清零的残余（RV1-F10 改进但未闭合，需如实登记）**

- 位置：`src/store.rs:189-193`（`let secret = unwrap_secret(...)?; … Ok(SecretBytes::new(&secret))`，中间 `Vec<u8>` 未清零即 drop）、`src/platform/windows.rs:31-35`（`decrypt_data` 的明文 `Vec<u8>` 直接返回）、`src/ephemeral.rs:23-24`（`Entry { secret: Vec<u8> }`，`#[derive(Clone)]`）、`:75-79`（`get()` 克隆一份明文）。
- 影响：签名/读公钥/读凭据每次都在堆上留下未清零的私钥标量或凭据副本（`SecretBytes` 的清零设计被绕过）；`EphemeralKeystore`（显式开发档位）在内存中长期保存明文且从不擦除。端口无私钥导出路径、crate 无日志，因此这不是可直接观察的泄漏，属纵深防御缺口。
- 建议：`read_secret` 内 `let mut secret = unwrap_secret(...)?; … ; secret.fill(0);`（或在 `platform` 边界就返回清零类型）；`EphemeralKeystore` 的条目改为持有 `SecretBytes`（注意其不实现 `Clone`，`get()` 需按需复制并同样清零），若决定不修，请在 `verification.md` 明确保留该残余而不是写成已消除。

**G4（P2）公开 API 面比自述不变量更宽：`list()`/`entry_path()` 未过闸门，且 `list`/`with_seed` 无调用方**

- 位置：`src/store.rs:111`（`pub fn list`，会 `read_dir`，不检查 `availability`）、`src/store.rs:104`（`pub fn entry_path`，用调用方传入的任意 label 拼路径并返回裸 `PathBuf`）、自述 `src/lib.rs:9-11`（"**所有**入口……一律在触碰文件系统之前返回 `KeystoreError::Unavailable`"）、`src/ephemeral.rs:48`（`pub fn with_seed`）。
- 事实：自述里的「所有入口」在实现上指 7 个端口方法（这一点措辞可自洽），但 `list()` 是**公开且会触碰文件系统**的诊断 API：在不可用平台它返回 `Ok(vec![])`（目录不存在）或 `Io`，既不是 `Unavailable` 也不是「条目缺失」的语义收敛点；`entry_path` 虽然注释警告「调用方必须先校验标签」，但签名上无法强制。全仓库检索：`list(`/`entry_path(`/`with_seed` 只被本 crate 的测试使用（`keystore.rs:190`、多处测试），**无生产调用方**。
- 影响：`AGENTS.md` §7「保持公开 API 最小」与「正常运行路径不得使用 `expect`」的口径下，这两个公开项（`list`、`with_seed`）目前是无人使用的公开面；后续调用方（CLI/审计/切片 4 迁移）若用 `decode` + `entry_path` 或直接用 `list`，就会绕过本轮建立的失败关闭闸门（不泄漏秘密，但会得到「看起来像空仓库」的结果）。
- 建议：给 `list()` 加 `self.availability.require().map_err(StoreError::into_port)?`（或明确文档为 diagnostic-only 并注明不可用平台语义）；`with_seed` 若确实是测试辅助，收进 `#[cfg(test)]`/feature 或补一个真实使用它的用例；同时把 `lib.rs` 的不变量措辞收窄为「7 个端口入口」并单列 `list`/`entry_path` 的约束。

**G5（P2）标签形状的边界：`decode` 侧新增校验无负例测试；大小写折叠会静默替换同一条目**

- 位置：`src/entry.rs:181-183`（`decode` 新增 `is_valid_label` 校验，即 RV1-F7 的修复）、`tests/entry_format.rs:90-139`（只测 `EntryHeader::new`，**没有**构造带非法标签的字节喂给 `decode`）；`src/store.rs:104`（大小写敏感与否由文件系统决定）。
- 影响：① F7 的另一半（读取路径校验）目前无测试，若该分支被误删，无任何用例失败；② `is_valid_label` 不做大小写折叠/尾随空格/Windows 保留字符处理：在大小写不敏感的文件系统（Windows/macOS）上，标签 `Primary` 与 `primary` 映射到**同一文件**，第二次 `generate` 会覆盖第一个条目（读旧 handle 时由 `matches()` 报 `IdentityMismatch` → `EntryCorrupt`，属可检出，但旧密钥已不可恢复，对节点身份意味着重新配对）；`:`、`*`、`?` 等 Windows 保留字符被放行，结果是在 `File::create` 处报 `Io` → 端口层映射为 `Unavailable`（而不是更准确的 `EntryInvalid`）；尾随空格在 Windows 上可能被 Win32 层裁剪（对 `head.matches()` 后果是 `IdentityMismatch`）。
- 说明：以上都不会造成路径穿越（`/`、`\` 已拒）或秘密泄漏；影响面是「错误分类语义」与「同义标签互相覆盖」，且当前无生产调用方传入此类标签。
- 建议：在 `entry_format.rs` 补一条「手工构造 label 含 `/` 或 `\u{7}` 的字节 → `decode` 返回 `HeaderInvalid`」；若认为大小写折叠不可接受，在 `is_valid_label` 或文档中明确「标签按文件系统语义唯一」这一约束；把 Windows 保留字符一并拒绝以获得稳定的 `EntryInvalid`。

**G6（P2，待确认）`.gitleaks.toml` 的 `keywords` 可能使规则永不评估**

- 位置：`.gitleaks.toml:44-47`（`keywords = ["ACPK"]` 与正则 `QUNQS…`）。
- 事实/影响：若 gitleaks 把 `keywords` 当作「仅当分片内含该字面量才评估本规则」的前置过滤（gitleaks v8 文档口径），则被 base64/base64url 编码后的负载里**不存在**字面 `ACPK`，规则永远不评估——注释里「命中即说明被编码后的明文条目被提交」就是空承诺（与 F11 的过度声明是不同问题：这次是规则可能无效，而非注释夸大）。
- 证据限制：本地无 gitleaks（`plan.md` 已声明 CI-only），我**未执行**任何验证，因此列为「需确认」；结论以文件内容为准。
- 建议：去掉 `keywords`，或把它改成 `["QUNQS"]`（编码后的前缀），并在 CI 用一份合成的 base64 条目文本做一次「规则确实命中」的复核（否则该规则永远无法被证明有效）。

**G7（P2）`write_atomic` 的 durability：只 fsync 文件，不 fsync 父目录**

- 位置：`src/store.rs:377-384`（`write_all` → `sync_all` → `rename`）。
- 影响：`rename` 本身未随文件数据落盘，掉电/崩溃后可能看不到新条目或看到旧条目；对「Provider 凭据」表现为 `get_secret → Ok(None)`（看起来像「未配置」）。spec/design 未对 keystore 提出崩溃持久性要求，故不作为缺陷，仅登记。
- 建议：若需要，unix 上对父目录做一次 `File::open(dir)?.sync_all()`，Windows 上使用 write-through 语义的替换；同时把 `store.rs:1-6` 的「原子写」措辞与「持久性」区分开。

**G8（P2）两处弱/不可失败断言（报告项，不阻断）**

- `tests/fail_closed.rs:174-181`：`KeystoreError` 是无载荷枚举（`identity-auth/src/port.rs:135-149`），因此 `assert!(!text.contains(plaintext))` 结构上不可能失败；真正有判别力的载体是「条目文件不含明文」（`fail_closed.rs:187-210`、`dpapi.rs:136-170`）与「错误分类封闭」。
- `tests/fail_closed.rs:25-36`：期望值由同一个 `cfg!(windows)` 推出，只能验证 `new()` → `detected()` 的装配连线，不能验证平台判定本身（后者由 `platform_availability_is_reported_honestly` 覆盖）。
- 建议：把 `errors_never_carry_secret_material` 的断言改成对**具体错误分类**的断言（例如 `matches!(error, KeystoreError::EntryMissing)`）+「`StoreError::Io` 只保存 `ErrorKind` 而非路径/消息」的结构性说明，避免读者的「已断言不泄漏」印象高于实际。

**G9（P2）证据文件位置与 `verification.md` 的引用口径不一致（记录卫生）**

- 位置：证据实际在 `D:\Project\acp-remote\reports\rv2-*.log`（仓库根），而同一 `verification.md` 其余行引用的是变更目录内的 `reports/wp3-*.log`、`reports/pv5-windows-dpapi.log`；`.gitignore:22-25` 明确「仓库根的 `reports/` 是早期变更的历史遗留……仓库根不再接受证据/日志目录」。
- 影响：按 `verification.md` 的相对路径去变更目录找 RV2 日志会找不到（我第一遍就因此判断证据缺失）；根目录同时混放着**另一个已归档变更**（`2026-09-23-fix-admin-store-integrity-gaps`）的 `rv1-*.md`/`du1-*.log`，容易误归属；`AGENTS.md` §10 的「一文件一写者/证据路径」口径被破坏。
- 建议：把 7 个 `rv2-*.log` 移到 `openspec/changes/identity-auth-and-keystore/reports/`（与 `rv1-wp*.md` 同处），或在记录里写明确路径；不要在根遗留目录继续累积新证据。

---

## 4) plan.md 覆盖索引与本 WP 的 Check ID 核对

### 4.1 Coverage Index 的 WP3 行（R72–R90）与 `fe52694` 现状核对

| R | 场景 | 本轮结论 | 依据（`fe52694` 文件内容） |
| --- | --- | --- | --- |
| R72 | 生成/读公钥/签名同源、无私钥导出 | 成立 | `store.rs:223-260`；`entry.rs:109-116`（用途收口）；`keystore.rs:414-443`（65 字节 SEC1 / 64 字节 P1363 + 公钥验签） |
| R73 | 生成后读公钥与签名一致 | 成立 | 同上 + `dpapi.rs:71-94`（真实 DPAPI 条目） |
| R74 | 不同条目是不同密钥 | **已补双向断言 → 成立**（RV1-F3 修复） | `keystore.rs:341-360`（两公钥 `assert_ne!` + 交叉验签失败 + 自验通过） |
| R75 | 删除后条目不可用 | 成立 | `keystore.rs:85-111`（重复删除报 `Missing`） |
| R76 | 签名只接受待签内容、无 DER | 成立 | `store.rs:241-260`；`src` 无 `from_der`；`Cargo.toml` 未开 `pkcs8` |
| R77 | 平台不可用失败关闭 | **成立（RV1-F2 修复）** | `store.rs:224/232/248/264/275/290/297` + `fail_closed.rs:78-143`（7 入口、零写入、非法句柄顺序）；`tests/keystore.rs:33-40` 已改用注入缝，旧的「不可用即 EntryMissing」断言不再存在 |
| R78 | 缺条目时不生成新身份 | 成立 | `keystore.rs:149-168`（`Availability::Platform`，两平台都真断言） |
| R79 | 损坏条目不被静默替换 | 成立（Windows 真实执行） | `keystore.rs:113-147`、`fail_closed.rs:212-235`；解包完整性由 DPAPI + 附加熵保证（`dpapi.rs:46-69`、`:96-134`） |
| R80 | 非 Windows 明确失败 | **成立（RV1-F1 修复）** | `fail_closed.rs:39-76` 在非 Windows 上真实执行，`delete_secret` 现在走闸门返回 `Unavailable`，RV1 的必然失败点已消失 |
| R81/R82 | 调试输出不含密钥材料 | 部分成立（WP3 侧无 `Debug` 断言，载体在 `identity-auth/tests/ports.rs`） | `entry.rs:102`（`EntryHeader` 的 `Debug` 无秘密字段）、`ephemeral.rs:22-25`（`Entry` 无 `Debug`）、`SecretBytes` 无 `Debug`（compile_fail 文档测试） |
| R83 | 记录与错误不携带秘密 | 成立（弱断言，见 G8） | `fail_closed.rs:168-185`；`error.rs:10-40`（`Io` 只保存 `ErrorKind` 字符串） |
| R84 | 普通数据库只保存引用 | 本变更无载体（切片 4），**不得记为已覆盖** | 本 crate 不写数据库 |
| R85 | 无分布式事务但可恢复 | 部分成立 | 见 R86/R87 |
| R86 | 引用提交失败不影响旧条目 | **不成立（G1）** | `keystore.rs:212-215` 死分支；`rv2-pv4.log` 的绿不能作为覆盖证据 |
| R87 | 孤儿可回收且不影响现有身份 | 成立（Windows 执行） | `keystore.rs:170-195` |
| R88/R89/R90 | 非硬件保护实现默认不启用 | 成立 | `ephemeral.rs:33-46`（无 `Default`）、`keystore.rs:363-400`、`lib.rs:30-35`（组合根判据） |

**证据指针的时效问题**：`plan.md` Coverage Index 的 R72–R90 行仍引用 `reports/wp3-identity-keystore.log` / `reports/pv5-windows-dpapi.log`（修复**前**的 WP 轮日志，位于变更目录），而 `verification.md` 已改用 `reports/rv2-*.log`（修复**后**、位于仓库根遗留目录，见 G9）。R77/R80/R86 这 3 行必须在最终验收前指到 `fe52694` 上的证据（R86 目前**没有**可用证据），否则覆盖索引与记录会互相矛盾。

### 4.2 Check ID 核对（本轮可核对的证据）

| Check ID | 本轮核对结论 | 依据 / 限制 |
| --- | --- | --- |
| PV1（`npm run verify`） | 已执行且绿，但**只在 Windows** | `reports/rv2-pv1.log`：`9 passed, 0 failed (9 items)`、`doc links OK: 369 relative links, 3454 section refs`、`crate boundaries OK: 10 个 crate`、`EXIT=0`，尾部含全 workspace 测试输出；`plan.md` 对 PV1 的「无零测试、无全跳过」按字面无法满足（多个库目标天然 0 测试，如 `identity_auth` lib、`acpr_transcript` lib），应理解为「无被掩盖的待验证断言」 |
| PV2（依赖方向） | 绿；**未受本轮改动影响** | `reports/rv2-pv2.log`：10 个 crate 与 §5 一致；本轮改动未引入任何新依赖（仅 `std::sync::atomic`），`Cargo.toml` 仍是 `identity-auth` + `async-trait`/`thiserror`/`p256`/`sha2`/`getrandom`（+ `cfg(windows)` 的 `windows-dpapi`） |
| PV3（`identity-auth`） | 存在且 `EXIT=0`，**本 WP 范围外，不代为判定** | `reports/rv2-pv3.log` |
| PV4（`identity-keystore` 全量） | Windows 侧绿（32 = 1+5+6+8+12），**但 `permissions` 目标为 0 用例**；Linux 运行时**仍无证据** | `reports/rv2-pv4.log:1-78`；`permissions` 的断言不可达见 G2 |
| PV5（真实 DPAPI） | 绿，且日志与源码互证「用的是真实 DPAPI」 | `reports/rv2-pv5.log`（5 passed、`EXIT=0`）；`dpapi.rs:21` 直接调用生产 `wrap_secret`/`unwrap_secret`，`platform/` 无测试替身；Linux 不覆盖该路径（`plan.md` 已声明） |
| RV1（WP3） | 本轮即 RV2 复核；`tasks.md:35` 的 3.6 仍是 `[ ]`（未勾选） | `reports/rv1-wp3.md` = RV1 FAIL；本轮报告落 `reports/rv2-keystore.md` |

未执行/不可本地执行：`cargo-deny`（`deps`/`advisories`）与 `gitleaks`（`secrets`）——与 `plan.md` 的声明一致，本轮**不记为通过**。

---

## 5) 未覆盖 / 无法确认项与所需证据

1. **Linux 运行时行为（本轮最关键的剩余不确定性）**：本机无 Linux 运行时（无 WSL/Docker），我无法执行非 Windows 分支。P0/P1 的判定依据是三类：① `reports/rv2-linux-clippy.log`（`--target x86_64-unknown-linux-gnu --all-targets --all-features -- -D warnings` 通过 → Linux 分支可编译、`#[cfg(unix)]` 与 `platform/unsupported.rs` 参与类型检查）；② 注入缝 `with_availability(Unavailable)` 在 Windows 上执行同一批断言（`fail_closed.rs:78-143`）；③ 对 `unavailable_platform_writes_nothing_at_all`（`fail_closed.rs:39-76`）的错误映射链做静态推导（`require()` 是入口第一条语句，`delete_secret` 不再可能返回 `SecretMissing`）。**仍无法证实**的是「Linux 上一次真实 `cargo test` 的通过」，唯一 venue 是 CI 的 `checks` job。所需证据：`checks` job 的 Linux 日志，期望看到 `fail_closed` 8 passed（含 `unavailable_platform_writes_nothing_at_all`）、`keystore` 中 `missing_referenced_entry_is_reported_without_recreation`/`purpose_mismatch_and_illegal_handles_are_rejected` 真断言、`permissions` 1 passed（但按其当前实现是空跑，见 G2）。
2. **`6861046..fe52694` 的 literal diff**：本轮未提供 diff 工件，`watchdog_diff` 不含 committed range，我也无 Git 访问 → 「新增/改动行归属」未核对；本报告只对 `fe52694` 的文件现状负责，无法断言修复轮是否顺带改动了范围外内容。
3. **Unix 权限位 `0700`/`0600`**：当前无任何可执行断言（G2）→ 需要 `store.rs` 的 `#[cfg(all(test, unix))]` 单测（可直接调用同模块私有函数）或等 Linux 后端落地。
4. **`.gitleaks.toml` 规则的真实命中能力**（G6）：需要一次 gitleaks 执行（合成 base64 条目文本），本地无等价物；建议在 CI 或本机临时安装一次以复核「规则能命中」而非只有「规则存在」。
5. **R84（普通数据库只保存引用）**：本变更内无载体，需在切片 4 的 `owned_provider_ref`/`local_admin` 写路径补断言；本轮不记为已覆盖。
6. **我未执行任何命令**（reviewer 只读）。若主 Agent 需要在 `fe52694` 上重建本轮证据，建议按序执行并留证：`cargo test --locked -p identity-keystore --all-features`（Windows 期望 32 用例 + `permissions` 0）、`cargo test --locked -p identity-keystore --all-features fail_closed`、`cargo clippy --locked -p identity-keystore --target x86_64-unknown-linux-gnu --all-targets --all-features -- -D warnings`、`node scripts/check-crate-boundaries.mjs`、`npm run verify`；并把日志落在变更目录 `openspec/changes/identity-auth-and-keystore/reports/`（G9）。
7. **范围外未复核**：`identity-auth`（WP2）、文档/状态表（WP1/WP4）、`schemas/`、`fixtures/`；`tasks.md` 的 3.2/3.4/3.6/3.8 复选框状态属主 Agent 维护项（3.6 至今未勾，而 WP3 已完成两轮 review）。

### 资源与清理

未创建任何文件、临时目录、worktree 或构建产物；未写 `target/`；报告由调度者落盘（我无写工具，按 runtime 指示返回完整文本）。

---