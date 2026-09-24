<!-- 落盘说明：主 Agent 原样保留 reviewer 的正文与结论；仅当某处 `§` 引用会触发文档引用门禁的歧义归属时，补全被引文档名（不改变语义）。 -->
<!-- 由独立 reviewer 子 Agent 产出（未继承实现对话，只读），主 Agent 原样落盘。 -->
<!-- review run: 6aa306be-bbdf-4a87-9e1a-f0ac928c0346；target revision: 6861046（RV1 轮次）；落盘时间：2026-09-24 -->

# RV1-WP3-2026-09-24 独立代码检视报告

**结论：FAIL**（存在 1 项 P0 阻断 + 1 项 P1 阻断；均为可证实的当前缺陷，不是待返回证据）

---

## 0. 固定字段

| 字段 | 内容 |
| --- | --- |
| task_id | RV1-WP3-2026-09-24（Review Type：branch；Work Package：WP3） |
| role | 独立代码检视子 Agent（只读，未修改任何文件、未切分支、未提交/合并/修复） |
| phase | 交付前独立 review（plan 第 3 组 3.6 / VW W4 的 RV1；WP3 = 任务 1.4、2.10–2.15） |
| agent_context | 新建子 Agent，未继承实现对话；本轮输入仅为调度者的任务文本 + `openspec/schemas/agentic/roles/reviewer.md` + `AGENTS.md`，无其它交接材料 |
| target_revision | `6861046`（= `refs/heads/main` 起点 `30f0d78` 之后的实现分支 HEAD；工作树对 HEAD `686104647e49` 无跟踪文件改动） |
| scope | `crates/identity-keystore/**`、`.gitleaks.toml`、`Cargo.toml` 的 members 与 target-specific 依赖；另核对 `openspec/changes/identity-auth-and-keystore/reports/` 中的 WP3/PV4/PV5 证据 |
| changes | 无（只读检视，未产出代码改动） |
| checks | 未执行任何命令；仅静态阅读源码、测试、日志文件与合同文档（见第 2、4 节） |
| issues | F1（P0）、F2（P1）、F3–F11（P2，report-only） |
| result | **FAIL** |
| evidence_paths | `crates/identity-keystore/{src,tests}/**`、`.gitleaks.toml`、`Cargo.toml`、`Cargo.lock`、`crates/identity-auth/src/port.rs`、`crates/identity-auth/tests/ports.rs`、`docs/{MODULE_ARCHITECTURE.md,IDENTITY_AND_AUTH_CONTRACT.md,SECURITY_DESIGN.md}`、`openspec/changes/identity-auth-and-keystore/{plan.md,tasks.md,verification.md,design.md,specs/platform-keystore/spec.md}`、`reports/{wp3-identity-keystore.log,pv5-windows-dpapi.log,wp3-dpapi-verification.log,wp3-boundaries.log,du1-pv1.log,bv-pv4.log}` |
| resource_cleanup | 不适用（只读；未创建临时目录、未写日志、未占用 `target/`） |

---

## 1. 版本与隔离声明

- **版本**：目标版本为 `6861046`（实现分支 `feat/identity-auth-and-keystore` 当前 HEAD），基线与 `verification.md` 记录一致为 `30f0d78b803b94346ebd8972078febceec8af8d3`。`watchdog_diff`（launch HEAD = `686104647e49`）显示**跟踪文件无 staged/unstaged 改动**，因此我读到的源码内容对应目标版本。
- **限制（如实声明）**：我没有 Git 访问能力，**无法直接读取 `30f0d78..6861046` 的已提交范围 diff**（`watchdog_diff` 明确不含 committed ranges，且本轮未提供 diff 工件）。本报告基于「在目标版本上按 scope 逐文件通读 + 交叉读取合同/规范/日志」，不是对 literal diff 的逐行核对；也没有对比 base 版本内容，因此**无法判断哪些行是本分支新增、哪些是既有**（我在下文只对「目标版本现状」负责）。
- **证据目录在检视期间被并发追加**：我第一次列 `reports/` 时有 20 个 `.log`；随后 `bv-pv2.log`、`bv-pv4.log`、`bv-pv5.log` 出现（均为 `.gitignore` 忽略的未跟踪文件，`watchdog_diff` 看不到）。这些都是日志而非代码，不影响代码结论，但说明「候选轮次证据」在本轮期间仍在生成。
- **隔离**：我是新建的、不继承实现对话的子 Agent，未参与该实现，也未与本轮实现者/主 Agent 讨论过实现细节。我只能声明自己收到的输入与限制，**不能证明宿主未注入其它上下文**。
- 未执行测试、未执行 E2E；所有 PASS/FAIL 判断均来自源码与文档的静态推导（含一处可精确推导的跨平台测试失败，见 F1）。

## 2. 已读取范围与文件

**代码（目标版本全文通读）**：`crates/identity-keystore/src/{lib,entry,store,error,entropy,ephemeral}.rs`、`src/platform/{mod,unsupported,windows}.rs`、`tests/{keystore,entry_format,fail_closed,dpapi}.rs`、`tests/support/mod.rs`；`crates/identity-keystore/Cargo.toml`；根 `Cargo.toml`；`Cargo.lock`（`getrandom`/`windows-dpapi`/`winapi`/`uuid` 条目）；`.gitleaks.toml`；`deny.toml`；`.github/workflows/ci.yml`；`crates/identity-auth/src/port.rs`、`crates/identity-auth/tests/ports.rs`（端口形状与秘密类型的上游合同）。

**需求与契约**：`specs/platform-keystore/spec.md`（全文）、`plan.md`（全文，含 Coverage Index/Work Packages/Project Verify/Runtime Resources）、`tasks.md`（1.4、2.10–2.15、3.5、3.6）、`design.md` 的 D6/D7/D8/D9 与 Risks 7/10/11、`verification.md`（全文）、`docs/MODULE_ARCHITECTURE.md` §3/§3.1/§4.12/§5/§6、`docs/IDENTITY_AND_AUTH_CONTRACT.md` §7 第 3/4/6 条、`docs/SECURITY_DESIGN.md` §13.1/§13.2/§20。

**证据日志**：`wp3-identity-keystore.log`（全文）、`pv5-windows-dpapi.log`（全文）、`wp3-dpapi-verification.log`（全文）、`wp3-boundaries.log`（全文）、`du1-pv1.log`（头部 40 行 + 定向检索）、`bv-pv4.log`（前 30 行）。未读：`wp3-npm-check.log`、`bv-pv2/bv-pv5.log`、`wp4-boundaries.log`。

**按重点项的执行结果**：(a) 条目格式与原子写 → F4；失败清理不完整；(b) 失败关闭 → 零持久化写入成立，但**错误分类不成立**（F2），无进程内回退/弱随机源回退（确认成立）；(c) 密钥不进 Debug/日志/错误 → 未发现泄漏路径（成立，见后）；(d) DPAPI 用法 → `Scope::User` 固定、仅「包裹 + 进程内签名」、附加熵绑定「域分离 + 版本 + 用途 + 标签 + 盐」（成立）；(e) 引用恢复语义 → 部分（R86 测试空转，F9）；(f) `.gitleaks.toml` → 「包裹后密文不可模式识别」如实声明（成立），但规则覆盖面注释过度（F11）；(g) PV5 日志 → 支持「Windows 上真实执行了 5 个 DPAPI 用例」，见第 4 节。

## 3. 逐条发现

### F1（P0，阻断合并）非 Windows 上 `unavailable_platform_writes_nothing_at_all` 的第三条断言不可能通过 → CI `checks` job 必红

- **位置**：`crates/identity-keystore/tests/fail_closed.rs:24-52`（失败断言在 `:47-52`）；对应实现 `crates/identity-keystore/src/store.rs:232-238`（`delete_secret`）与 `src/store.rs:140-147`（`remove_entry`）。
- **触发条件**：在非 Windows 平台运行 `cargo test --locked -p identity-keystore --all-features`。测试体在 `platform_supported()` 为 false 时**不再提前返回**（`fail_closed.rs:25-28` 只在 Windows 早退），因此 `:47-52` 的 `assert_eq!(store.delete_secret(...).await, Err(KeystoreError::Unavailable))` 会真实执行。
- **预期 / 实际**：
  - 预期（测试与合同）：`Err(KeystoreError::Unavailable)`。
  - 实际：`delete_secret` → `remove_entry(purpose, "codex")` → `fs::remove_file("<root>/provider-credential/codex.1")`。该文件（连父目录）在非 Windows 上**从未被创建**，`unlink` 返回 ENOENT → `ErrorKind::NotFound` → `StoreError::Missing` → 命中 `store.rs:235` 的 `Err(StoreError::Missing) => Err(KeystoreError::SecretMissing)`，即实际为 `Err(KeystoreError::SecretMissing)`。`assert_eq!(Err(SecretMissing), Err(Unavailable))` 直接 panic。
- **影响**：`checks` job 跑在 `ubuntu-latest` 且执行 `npm run check:rust`（`.github/workflows/ci.yml:41-83`，末段 `run: npm run check:rust`），其内包含 `cargo test --locked --workspace --all-features`。因此该测试**只在 Linux/非 Windows 上才真正运行**，而它在非 Windows 上必然失败：这条必需检查会让 DU1 在 CI 上变红，阻断合并。本地 Windows 证据永远看不到它：在 Windows 上 `fail_closed.rs:25-27` 早退，测试体及其断言一次都不执行——这正是该缺陷逃过 PV4/WP3 轮次的原因，也说明 task 3.5 要求的「记录 Windows 上被跳过的用例数与原因」没有覆盖这种**早退型空跑**（日志里全是 `0 ignored`，看不出有测试什么都没断言）。
- **修复建议（最小）**：与 F2 同一处修复——在端口入口按 `platform_supported()`（或 `cfg!(windows)`）做可用性闸门，使 `delete`/`delete_secret`/`get_secret` 在非 Windows 上返回 `KeystoreError::Unavailable`，则该断言自然成立。若团队有意让「缺失」优先于「不可用」，则必须同时改测试与合同文本（见 F2），不能只改一方。
- **严重度**：MAJOR / 阻断（P0）。

### F2（P1，阻断）非 Windows 的读/删入口不返回「不可用」分类，违反本 crate 自述不变量、design D7 与 `platform-keystore` R77/R80

- **位置**：`src/lib.rs:9-11`（不变量声明）、`src/store.rs:154-166`（`read_entry` 把 NotFound 映射为 `Missing`）、`src/store.rs:189-206`（`public_key`/`sign`）、`src/store.rs:208-226`（`get_secret` 把 `Missing` 映射为 `Ok(None)`）、`src/store.rs:228-250`（`delete`/`delete_secret`）；`tests/keystore.rs:130-147` 把该行为固化为断言。
- **触发条件**：非 Windows 平台调用 `public_key` / `sign` / `get_secret` / `delete` / `delete_secret`。
- **预期 / 实际**：

| 入口 | 预期（`lib.rs:10`「任何写入/读取都返回 `KeystoreError::Unavailable`」、design D7、spec R80/R77） | 实际（非 Windows） |
| --- | --- | --- |
| `generate` | `Unavailable` | `Unavailable` ✅ |
| `put_secret` | `Unavailable` | `Unavailable` ✅ |
| `public_key` / `sign` | `Unavailable` | `EntryMissing` ❌（`tests/keystore.rs:132-144` 还把 `EntryMissing` 断言成期望值） |
| `get_secret` | `Unavailable` | `Ok(None)` ❌（与「凭据未配置」不可区分） |
| `delete` / `delete_secret` | `Unavailable` | `EntryMissing` / `SecretMissing` ❌ |

- **影响**：① 平台不可用与「条目缺失/凭据未配置」在同一封闭分类里被混淆，违反 `AGENTS.md` §3「一个状态只能有一个权威写入者」之外的失败关闭约定与合同 §7 第 6 条（「Linux 保持失败关闭」）；② 组合根按 design/合同预期区分 `Unavailable`（拒绝启动）与 `EntryMissing`（提示重新配对）时，在 Linux 上会得到「身份材料不存在」而不是「本平台没有安全存储」，`get_secret` 更会静默表现为「没有凭据」——这是失败关闭边界上的静默降级，正是 plan「阻断标准」点名要避免的一类路径；③ spec R77/R80 的验收语（「可以识别的**不可用**分类」）在非 Windows 上对 5 个入口不成立，因此 verification.md 把 R77/R80 记为已覆盖是**不成立的**（见第 4 节）。
- **修复建议**：在 `IdentityKeystore` 的六个入口（或至少在 `read_entry`/`remove_entry` 前）加统一的可用性判断，非 Windows 直接返回 `KeystoreError::Unavailable`（比在 `remove_entry` 里判断更清楚，且能让「零持久化写入」与「零文件系统探测」同时成立）；若团队选择保留「Missing 优先」语义，则必须同步改 `lib.rs:9-11` 的自述、`design.md` D7、`specs/platform-keystore/spec.md` R80 场景与 `IDENTITY_AND_AUTH_CONTRACT.md` §7 第 6 条，并明确 `get_secret` 在平台不可用时的返回——但那会实质放宽已冻结的失败关闭语义。
- **严重度**：MAJOR / 阻断（P1）。

### F3（P2，report-only）R74「不同条目是不同密钥」在 WP3 证据里没有任何断言

- **位置**：`tests/keystore.rs:284-311`（`salts_are_per_entry_and_plaintext_is_absent`：只断言两个盐不同、第二个公钥「可读」）；`tests/dpapi.rs:98-140`（`dpapi_entropy_binds_entries_to_their_identity`：断言剪接解不开、第一条仍可用，但**从未比较两把公钥**）。
- **触发/证据**：全仓库检索 `assert_ne!` 与公钥比较，没有任何断言要求「两条目的公钥不同」或「A 的签名不能用 B 的公钥验过」；`crates/identity-auth` 的 `FakeKeystore` 用固定私钥（`tests/support/mod.rs:267-273`），天然无法覆盖该场景。
- **影响**：覆盖索引把 R74 指向 `[PV4,PV5]` 与 `wp3-identity-keystore.log`/`pv5-windows-dpapi.log`，但这两份日志无法支撑该场景。若「熵源/标量派生」出现回归（例如所有标签共用同一标量），现有测试不会失败。
- **修复建议**：在 `salts_are_per_entry_and_plaintext_is_absent` 或 `dpapi_entropy_binds_entries_to_their_identity` 中补三行：两把公钥 `assert_ne!`、用 A 的公钥验 B 的签名失败、用 A 的公钥验 A 的签名成功。
- **严重度**：MINOR（P2；属测试侧遗漏，但被覆盖索引宣称已覆盖）。

### F4（P2）原子写的「失败清理」与「临时文件名唯一性」比注释弱

- **位置**：`src/store.rs:282`（注释「失败时清理临时文件」）对 `src/store.rs:283-305`（实现）。
- **触发/证据**：临时文件仅在 `fs::rename` 失败的分支里被删除（`:302-305`）；`write_all`/`sync_all` 失败（ENOSPC、EIO 等）时 `?` 直接返回，`.<label>.<version>.tmp-<pid>` 残留在 keystore 目录里且**没有任何清理者**（`list()` 只识别 `.<version>` 后缀，因此不会被误当条目）。另外临时名只带 `std::process::id()`，同一进程内对同一条目的两次并发写会复用同一临时路径：一个 writer 可能 rename 到另一个 writer 只写了部分内容的临时文件，最终路径出现**截断条目**（原子性承诺在最坏情况下被打破，且对节点身份条目而言只能重新配对恢复）。
- **影响**：注释承诺的行为与实际不符；残留文件不会被回收；理论上存在低概率的撕裂写。不影响「非 Windows 零写入」（包裹先于任何 fs 操作，成立）。
- **修复建议**：把写临时文件的过程放进一个「失败即删」的作用域（或每次用 `AtomicU64`/随机后缀命名），并把注释改为与实际一致。
- **严重度**：MINOR（P2）。

### F5（P2）Unix `0700`/`0600` 权限路径无任何断言，且已存在目录不会被收紧

- **位置**：`src/store.rs:268-280`（`create_private_directory`，`:269` 目录已存在时直接 `Ok(())`）、`:295-300`（文件 `0600`）。
- **触发/证据**：`crates/identity-keystore` 全 crate 检索无 `mode()`/权限位断言；`tests/permissions.rs` 只存在于 `crates/storage-sqlite`（该文件与 CI 注释里的「测试权限路径」指的不是本 crate）。因此在 Linux CI 上这两段 `#[cfg(unix)]` 代码只被编译、从不被验证；Windows 本地更不会执行。
- **影响**：`design.md` D6 与 task 2.10 的「Unix `0700`/`0600`」没有回归护栏；若 root 目录（`<root>/keystore`）是预先存在或宽松权限，本实现不会收紧（`create_dir_all` 只对叶子目录 chmod，中间目录保持 umask 默认值）。
- **修复建议**：加一个 `#[cfg(unix)]` 用例断言 `<root>/<purpose>` 模式为 `0700`、条目文件为 `0600`（照 `crates/storage-sqlite/tests/permissions.rs` 的既有做法）；并明确「已存在目录不收紧」是否为接受的设计（如需收紧，`create_private_directory` 应无条件 `set_permissions`）。
- **严重度**：MINOR（P2）。

### F6（P2）crate 内两处「`cfg` 只出现在 platform 模块」的声明与事实不符

- **位置**：`src/platform/mod.rs:3`（「本模块是本 crate 里唯一出现 `cfg` 的地方」）、`src/lib.rs:11`（「`cfg` 只出现在这里」）；事实：`src/store.rs:273`、`src/store.rs:295` 各有一处 `#[cfg(unix)]`。
- **影响**：`AGENTS.md` §4/§5 的平台隔离审查（「平台 `cfg` 是否只出现在 `identity-keystore`」）会被这句误导——若后续有人用 `rg 'cfg\(' crates/identity-keystore/src` 复核，会得到与注释矛盾的结论，或误以为存在越界。`identity-auth` 侧「零 `cfg`」本身成立（已核对）。
- **修复建议**：改为「平台**后端**选择只在 `platform` 模块；`store.rs` 另有 Unix 权限位路径」。
- **严重度**：MINOR（P2）。

### F7（P2）标签校验在 `new` 与 `decode` 之间不对称，`entry_path` 不校验标签

- **位置**：`src/entry.rs:113-124`（`new` 拒绝分隔符/控制字符）、`src/entry.rs:150-187`（`decode` 只校验长度与 UTF-8）、`src/store.rs:51-54`（`entry_path` 用未校验的 label 拼路径，且是 `pub`）。
- **触发/证据**：当前各处调用都安全（`write_entry` 先经 `new`；`read_entry` 在 `decode` 后立刻 `matches()` 比对已校验的 label）；但 `decode` 单独使用时其 `label` 可能含 `../` 或控制字符，而 `entry_path` 会把它直接拼进路径。任何后续调用方（例如 CLI/审计工具列目录、切片 4 的迁移代码）用 `decode` + `entry_path` 组合就会得到目录穿越。
- **修复建议**：把 label 形状校验下沉到 `EntryHeader::decode`（或在 `entry_path` 上做同样的校验并返回 `Result`），并把该不变量写在 `entry_path` 的文档注释里。
- **严重度**：MINOR（P2）。

### F8（P2）plan 声明的自检「`crates/identity-keystore/src` 正常路径零 `expect(`」未满足且未登记

- **位置**：`src/ephemeral.rs:45,73,86,123,153`（5 处 `lock().expect("进程内条目锁")`，非 `#[cfg(test)]` 代码）；对照 `plan.md`「Local Checks」的审查重点自检「`rg -n "from_der|unwrap\(|expect\(" crates/identity-auth/src crates/identity-keystore/src`（正常路径零命中）」，以及 `verification.md` 只为 `identity-auth` 记了该自检。
- **影响**：锁中毒时是 `panic` 而不是封闭分类错误（`AGENTS.md` §7「正常运行路径不得使用 `unwrap()`/`expect()`」）；该文件是显式构造的开发档位，可能是「可接受偏差」，但目前**没有任何记录**说明这是有意为之。
- **修复建议**：把锁中毒映射成 `KeystoreError::Unavailable`（或 `EntryCorrupt`），或在 `verification.md` 的 WP3 行如实记录该偏差（并说明仅限开发档位）。同时 `with_seed`/`EphemeralKeystore` 目前无任何调用方（全仓库检索确认），建议按 `AGENTS.md` §7「保持公开 API 最小」补一个真正使用它的用例，或收进 test-only 面。
- **严重度**：MINOR（P2）。

### F9（P2）R86 的测试是空转：死分支使「引用提交失败」路径不可达

- **位置**：`tests/keystore.rs:172-200`，关键行 `:187-190`（`let reference_commit_failed = true; if !reference_commit_failed { store.delete(&old)... }`）。
- **触发/证据**：`reference_commit_failed` 恒为 `true`，`if` 体不可达（未执行任何引用层代码），测试剩余断言只有「两个条目都可读/可签」——它证明的是「`generate` 不会覆盖同用途其他标签」，并不能证明「引用提交失败时旧引用仍可用」这一 plan task 2.14 要求「在 fake 引用侧模拟」的结论。`tests/keystore.rs:186` 的注释也自述「引用在存储层，这里用开关表达」。
- **影响**：R86 名义上有测试、实质无覆盖；死分支会让后续读者误以为该场景已经被模拟过。
- **修复建议**：要么删掉死分支并把用例名/注释改为实际断言的性质（「生成新标签不覆盖旧标签」），要么引入一个最小 fake 引用持有者（提交失败 → 断言旧 handle 仍解析、新 handle 不被当作有效身份）。
- **严重度**：MINOR（P2）。

### F10（P2）明文标量的堆副本未清零

- **位置**：`src/store.rs:171-180`（`read_secret` 返回 `Vec<u8>`）、`:189-206`（`sign` 使用后直接 drop）、`:208-226`（`get_secret` 复制后 drop）；对照 `crates/identity-auth/src/port.rs:128-132`（`SecretBytes` 显式清零）。
- **影响**：签名/读公钥路径上每次都在堆上留下未清零的私钥标量副本（`SecretBytes` 的清零设计被绕过）。这是纵深防御问题，不构成可直接观察的泄漏（端口无私钥导出路径，crate 无日志）。
- **修复建议**：内部路径改用 `SecretBytes`（或显式 `fill(0)`）承载解包后的明文。
- **严重度**：MINOR（P2）。

### F11（P2）`.gitleaks.toml` 的规则覆盖面注释比规则本身大

- **位置**：`.gitleaks.toml:27-40`，规则正则在 `:37`（`(?i)QUNQS[A-Za-z0-9+_-]{16,}={0,2}`，`:38` `entropy = 0`，`:39-41` `keywords = ["ACPK"]`）。
- **触发/证据**：`:27-30` 的注释说该规则覆盖「一个**明文**落盘的条目…命中即说明有明文条目被提交」，但正则只匹配 `ACPK` 的 **base64/base64url 编码形态**（`QUNQS…`）。真实落盘的明文条目文件是二进制（`ACPK` + `\x00\x01` + 二进制字节段），不匹配该正则；`keywords` 只用于缩小规则评估范围，本身不是命中条件。`:32-34` 声明的诚实限制只覆盖「DPAPI 包裹后的密文」，未提「原始二进制明文条目形态同样不可模式识别」。
- **影响**：规则本身的防护强度被注释高估；`ACPK` 只在「被 base64 化后提交」（例如日志、dump、粘贴的代码块）时才可能命中。
- **修复建议**：把注释改成「本规则只覆盖 base64/base64url 编码形态；原始二进制条目与包裹后密文都无法用模式识别，只能靠路径约定（条目不进仓库）与轮换流程」。**注意**：任务点名要核对的「包裹后密文不可模式识别、不假装覆盖密文」这一条**是诚实声明的**（`:32-34`），此处不是它的缺陷。
- **严重度**：MINOR（P2）。

### 已核对为「无问题」的重点项（不作为发现）

- **(c) 秘密不进可观察位置**：`entry.rs:102`（`EntryHeader` 的 `Debug` 只含用途/标签/版本/盐，无秘密字节）、`ephemeral.rs:21-31`（内存条目**未**派生 `Debug`）、`error.rs:10-26`（`StoreError` 文案只含类别与 label，`Io(String)` 只放 `ErrorKind` 而非 OS 路径/消息）、`platform/windows.rs:20-28`（`anyhow::Error` 被 `map_err(|_| …)` 丢弃，不外泄）。crate 无 `tracing`/`log` 依赖，无任何日志调用。测试快照：无 insta/快照文件，条目只写入 `std::env::temp_dir()` 下的用例自建目录，`TempRoot` 析构即删。
- **(d) DPAPI 用法**：`platform/windows.rs:20`/`:27` 两处固定 `Scope::User`，公开入口 `wrap_secret(plaintext, entropy)` 不接受 scope（`platform/mod.rs:9-19`）；`Cargo.toml` 只在 `[target.'cfg(windows)'.dependencies]` 引入 `windows-dpapi`；附加熵由 `entry.rs:196-208` 从「`acp-remote/keystore-entry/v1` 域标签 + 版本 u16be + 用途 token + 标签长度 u32be + 标签 + 盐」派生，测试覆盖盐/标签/用途变化（`tests/entry_format.rs:75-90`）与真实 DPAPI 下的剪接拒绝（`tests/dpapi.rs:98-140`）。
- **(b) 零持久化写入/无降级**：`store.rs:114` 先包裹再建目录/写文件；`tests/fail_closed.rs:24-56`（+ `tests/keystore.rs:44-68`、`:313-333`）断言不可用平台「连目录都不建」；`getrandom` 失败只返回 `EntropyError::Unavailable`（`entropy.rs:22-26`），无时间戳/`RandomState` 回退；`platform/unsupported.rs:17-21` 两个后端函数恒失败。**F1/F2 只涉及错误分类，不涉及写出字节**。

## 4. plan.md 覆盖索引（WP3 行）与 Check ID 核对

**R 编号核对**（WP3 覆盖行为 R72–R90，其中 R81–R84 与 WP2 共享）：

| R | 场景要点 | 核对结论 | 依据 |
| --- | --- | --- | --- |
| R72 | 生成/读公钥/签名同源，无私钥导出 | **成立** | `store.rs:167-206`；`tests/keystore.rs:347-373`（公钥验签通过，65 字节 SEC1 / 64 字节 P1363） |
| R73 | 生成后读公钥与签名一致 | **成立** | 同上 + `tests/dpapi.rs:64-96`（真实 DPAPI 条目） |
| R74 | 不同条目是不同密钥 | **不成立（F3）** | 无任何断言（见 F3） |
| R75 | 删除后条目不可用 | **成立** | `tests/keystore.rs:71-97`（重复删除报缺失） |
| R76 | 签名只接受待签内容、无 DER 路径 | **成立** | `store.rs:189-206`；`identity-keystore/src` 无 `from_der`；`Cargo.toml` 未开 `pkcs8` |
| R77 | 平台不可用失败关闭 | **不成立（F2）** | 5 个入口在非 Windows 返回 `Missing`/`SecretMissing`/`Ok(None)` |
| R78 | 缺条目时不生成新身份 | **成立** | `tests/keystore.rs:130-147` |
| R79 | 损坏条目不被静默替换 | **成立** | `tests/keystore.rs:99-128`（Windows）+ `tests/fail_closed.rs:117-135`（跨平台头部损坏） |
| R80 | 非 Windows 明确失败 | **不成立（F1+F2）** | 唯一断言的用例在 Windows 早退、在非 Windows 必失败；且读/删入口不返回不可用类 |
| R81/R82 | 调试输出不含密钥材料 | **部分成立** | WP3 侧无 `Debug` 断言但这些类型没有可泄漏的 `Debug`；断言载体在 `crates/identity-auth/tests/ports.rs:28-32,46-64`（R82 的 WP3 侧证据不足，属可接受但需注明） |
| R83 | 记录与错误不携带秘密 | **成立** | `tests/fail_closed.rs:56-79` + `error.rs` 文案设计 |
| R84 | 普通数据库只保存引用 | **本变更内无法验证** | 本 crate 不写数据库；「引用列只存字段名/标识/版本」的断言落在切片 4（`owned_provider_ref` 写路径）。覆盖索引把它指向 WP3 日志，只能算未证实 |
| R85/R86 | 无分布式事务但可恢复、引用提交失败旧引用可用 | **名义覆盖、实质空转（F9）** | `tests/keystore.rs:172-200` |
| R87 | 孤儿可回收且不影响现有身份 | **成立（仅 Windows 执行）** | `tests/keystore.rs:150-169` |
| R88/R89/R90 | 非硬件保护实现默认不启用 | **成立** | 无 `Default`（`ephemeral.rs:33-43`）；`tests/keystore.rs:313-333` 证明无进程内兜底 |

**Check ID 核对**：

| Check ID | 核对结论 | 说明 |
| --- | --- | --- |
| PV1（`npm run verify`） | **本地证据完整，但只覆盖 Windows** | `du1-pv1.log` 头部确实是完整的 `npm run verify`：十道门禁逐条 OK（schemas/commands/errors/features/assets/acp/docs/boundaries/drift/agentic）+ 后续 cargo 测试；`verification.md` 的「verify exit 0、67 个测试二进制」与日志一致。**但平台是 Windows**，非 Windows 分支未被这一轮验证 |
| PV2（`check-crate-boundaries.mjs`） | **成立** | `wp3-boundaries.log`：`crate boundaries OK: 10 个 crate 的依赖方向与 §5 矩阵一致`；§5 中 `identity-keystore` 行只勾 `identity-auth`、`identity-auth` 行不勾 `identity-keystore`（`MODULE_ARCHITECTURE.md` §5 表 + §4.12 约束），且 `identity-keystore` 未出现 `core` 依赖 |
| PV3（`identity-auth`） | **未复核（本轮 scope 外）** | 仅确认 `bv-pv3.log`、`wp2-*.log` 存在；WP2 的独立 review（3.4）仍未完成，其结论不属本轮 |
| PV4（`identity-keystore`） | **PASS 声明只在 Windows 成立** | `wp3-identity-keystore.log` / `bv-pv4.log` 均为 Windows 运行（`.exe` 路径），29 = 1+5+6+5+12 与 `verification.md` 一致；但「失败关闭（…非 Windows）全部通过」这一判据在非 Windows 上会被 F1 直接推翻，且「跳过数"如实记录」没有覆盖早退型空跑（`fail_closed.rs:25`、`keystore.rs` 中 6 处 `let Ok(...) else { return }`） |
| PV5（DPAPI，仅 Windows） | **日志支持「已执行且通过」，但单独不能证明「真实 DPAPI」** | `pv5-windows-dpapi.log` 是原始 cargo 输出（`.exe` 路径、5 个用例全绿），且 `verification.md` 如实写了「Linux CI 不覆盖该路径」。日志本身无法区分真实 DPAPI 与 mock；**结合源码可成立**：`tests/dpapi.rs` 直接调用生产 `wrap_secret`/`unwrap_secret` → `windows_dpapi::encrypt_data/decrypt_data`，`platform/` 中没有任何 `#[cfg(test)]` 替身，`EphemeralKeystore` 也不出现在该文件中。wrapper 版本/许可证则由 `wp3-dpapi-verification.log` + `Cargo.lock`（`windows-dpapi 0.2.0`）支撑 |
| RV1 | **本轮即 3.6 的检视；3.2/3.4/3.8 仍未完成** | `tasks.md` 的 3.2/3.4/3.6/3.8 均未勾选，`reports/` 下没有 `rv1-wp*.md`；`verification.md` 的 Review Findings 表仍为「尚未进行独立 review」。**不得**把本轮 review 当成其他 WP 的 review，也不得把「代码 review 无阻断」当作 PV 通过 |

## 5. 未覆盖 / 无法确认项与所需证据

1. **非 Windows 运行结果（关键，影响本轮判断）**：F1 是静态推导（错误类型映射链无分支可绕），但我没有执行环境。请在类 Unix 主机或 CI 上跑 `cargo test --locked -p identity-keystore --all-features fail_closed`（或直接以 `npm run check:rust` 走 Linux runner）。这条结果是 F1 的唯一剩缺口；它不改变 F2 的判断（F2 由源码直接可判）。
2. **base→target 的 literal diff**：本轮未提供 diff 工件、我也无 Git 访问，因此「新增/改动行归属」未核对；如需按 diff 复核请提供 `git diff 30f0d78..6861046` 输出或允许只读 `git show`。
3. **CI-only 判定**：`cargo-deny`（licenses/bans/sources/advisories）与 `gitleaks` 本地无等价物。`windows-dpapi 0.2.0` 新带入的 `winapi 0.3.9`（其元数据许可证字段是 `MIT/Apache-2.0` 旧式斜杠写法）、`anyhow`、`log` 是否全部通过 `deny.toml` 的 allow 列表与 `[graph] targets`（`x86_64-pc-windows-msvc` 已登记，故不会被跳过），只能由 CI 的 `deps`/`advisories` job 回答；`verification.md` 已如实标注「只在 CI 完成」，本轮不把它记为通过，也不记为失败（残余风险见文末）。
4. **WP3 侧「秘密不进 `Debug`」的独立证据**：R82 的证据列指向 `wp2-identity-auth-ports.log` + `wp3-identity-keystore.log`，但 WP3 的测试里没有针对 `EntryHeader`/`EphemeralKeystore::Entry`/`StoreError` 的 `Debug` 断言（当前无泄漏路径，故我不把它列为缺陷）。若要求逐条证据，需要一条 `format!("{:?}", …)` 断言或一段说明「这些类型不含秘密字段」的记录。
5. **R84（普通数据库只保存引用）**：本变更内没有可断言的载体，需在切片 4 的 `owned_provider_ref`/`local_admin` 写入路径上补测试。
6. **`EphemeralKeystore`/`with_seed` 无调用方**：属公开 API 规模问题（`AGENTS.md` §7），需要一次「补用例或收窄 API」的决定（见 F8）。

### Assessment

- 本轮范围为 WP3（`crates/identity-keystore`、`.gitleaks.toml`、`Cargo.toml` 的成员与 target-specific 依赖），已在目标版本 `6861046` 上完成约定范围的静态检视：条目格式/原子写、失败关闭、秘密不进可观察位置、DPAPI 用法与附加熵、引用与条目恢复语义、gitleaks 规则诚实性、PV4/PV5 证据一致性。
- **结论：FAIL** —— F1（P0，Linux CI/必需检查必红）、F2（P1，非 Windows 失败关闭分类与 spec R77/R80 不符）。其余为 P2 报告项（F3–F11），其中 F3（R74 无断言）与 F9（R86 空转）使两份覆盖行不能在最终验收中记为已覆盖。
- 与 Project Verify 的关系：本轮**没有**替代 PV1–PV5 的执行，也没有把任何 CI-only 判定写成已通过；`verification.md` 中「PV4 PASS」「候选轮次 PASS」的结论在**Windows 平台上**与其日志一致，但 cover 不到 F1/F2 所指向的非 Windows 行为。
- 资源：无残留（只读，无临时目录、无 worktree、无 `target/` 写入）。修复后应由新的隔离子 Agent 按 F1/F2 原 ID 复核，并重跑 PV4（含非 Windows 路径）与受影响的 PV1/PV2。

---