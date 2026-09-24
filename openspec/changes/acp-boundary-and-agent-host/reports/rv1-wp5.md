# 独立检视报告 · WP5（配置、凭据、目录与空闲回收）

> 转录说明：本文件由主 Agent 从独立复核者的输出逐字转录。为通过仓库的**文档引用门禁**（`check:docs`），仅对文档引用写法做了规范化（例如把 `CORE_PORTS` 写全为 `docs/CORE_PORTS_AND_STORAGE.md`），**不改任何结论、级别或证据**。

检视对象：HEAD `4c24fbd`（提交范围名义 `64a7582..HEAD`）。工具限制：该 reviewer 上下文**没有 shell**，未执行任何命令，结论为静态阅读 + 契约比对；需要主 Agent 代跑的判定见报告 §5。

# 独立检视报告 — WP5（配置、凭据、目录与空闲回收）

## 0. 范围、方法与能力边界（先说清能证什么）

**被检视对象**：`openspec/changes/acp-boundary-and-agent-host/` 变更的 WP5（任务 2.25/2.26/2.27 + 3.9），提交范围名义为 `64a7582..HEAD`（HEAD = `4c24fbd`，基线 `094009b`）。

| 文件 | 检视的行 |
|---|---|
| `crates/agent-host/src/launch.rs` | 全文（31、41–52、54–72、74–80、86–93、98–110） |
| `crates/agent-host/src/host.rs` | 全文（166–204、250–274、277–286、288–302、305–310、356–375、380–452、459–478、505–513、516–547） |
| `crates/agent-host/src/config.rs` | 全文 |
| `crates/agent-host/src/error.rs` | 13、116–135 |
| `crates/agent-host/src/limits.rs` | 41（`IDLE_SWEEP_INTERVAL`） |
| `crates/agent-host/src/process.rs` | 119–121、325–360、601–650（交叉引用，WP3 所有权） |
| `crates/agent-host/src/session.rs` | 156–159、292、455–475、534–572、761–840（交叉引用） |
| `crates/agent-host/tests/catalog.rs` | 全部 7 个用例（55、84、143、200、242、287、313） |
| `crates/agent-host/tests/support/mod.rs`、`tests/supervision.rs` | FakeConfig/FakeCredentials（`mod.rs` 236–408）、`supervision.rs:314–340` |
| 合同侧（只读比对） | `specs/local-agent-host/spec.md:162–203`；`docs/CORE_PORTS_AND_STORAGE.md:610–617/810–835/1205/1259`；`docs/MODULE_ARCHITECTURE.md:284`；`crates/core/src/ports.rs:76–91/812–849`；`crates/core/src/broker.rs:1214/2091–2105`；`crates/core/src/model/ids.rs:488–505` |

**能力边界（影响结论强度，必须读）**

1. **我没有 shell 工具**，本次检视**未执行任何命令**（`cargo test` / `npm run check` / `git` 都没有跑）。下表所有结论都是静态阅读 + 契约比对，**不构成"测试通过"的背书**。
2. `watchdog_diff` 只覆盖工作树相对 HEAD 的增量，**不含已提交区间**，因此我无法核对 `64a7582..4c24fbd` 的逐提交差异。可确认的是：工作树相对 HEAD 只有 `.pi/settings.json` 与 `verification.md` 两处未提交改动（`watchdog_diff` 输出），`crates/agent-host/**` 无未提交改动 ⇒ **我读到的 WP5 文件内容就是 HEAD 内容**。提交范围本身由 `verification.md:127–129` 的自述（`git rev-parse refs/heads/main` → `4c24fbd…`）间接印证，未独立复核。
3. 需要主 Agent 代跑的判定（见 §5）。

---

## 1. 逐条结论表

编号规则：`Qn-k` 对应任务里的第 n 问第 k 条。

| 编号 | 结论 | 级别 | 证据（文件:行 / 命令与输出摘要） | 说明 |
|---|---|---|---|---|
| **Q1-1** 白名单外变量被注入？ | **PASS** | — | `launch.rs:54–64`（逐名校验 allowlist，越界 → `HostError::EnvNotAllowed`）；`process.rs:119–121`（`env_clear()` 后**只**遍历 `spec.env`）；`launch.rs:31`（`NECESSARY_ENV` 是 6 名常量表）；`catalog.rs:287–306`（端口返回 `NOT_ALLOW_LISTED` → `create` 必失败 + `!dump.exists()`）；`supervision.rs:327–338`（`!text.contains("CARGO_PKG_NAME=")` — 该变量确实存在于父进程环境，是真正的反证） | 两道闸：端口契约（`ports.rs:842–844`）+ 本层复核（纵深防御）。`NECESSARY_ENV` 是**规格内的固定例外**（`spec.md:164`「加必要进程环境」），不是白名单放松。 |
| **Q1-2** 白名单内变量被静默丢弃？ | **FAIL** | **MINOR（P2，报告项）** | `launch.rs:54–72` 只做「返回值 ⊆ 白名单」的**上限**校验，不比对「白名单 ∩ 绑定」的**期望集合**；丢变量的检测手段（`profile.env()` ∩ `env_allowlist()`）在手边却未用（`crates/core/src/model/config.rs:180–186` 已公开这两个读取器） | 真实 `CredentialResolver` 若违反契约（部分返回而不报错），本层与任何现有用例都**看不见**：`catalog.rs:143` 用的是 `FakeCredentials`（`support/mod.rs:389–408`），它自己就实现了交集语义，因此该用例只证明了 launch 侧不「多注入」，**没有**验证「交集不漏」。最小修法：在 `launch.rs` 收集后断言返回值名集合等于 `{b.name() \| b.name() ∈ env_allowlist()}` 且含绑定者（不一致即 `HostError`，失败关闭）。 |
| **Q1-3** 失败是否 fail-closed（spawn 前拒绝）？ | **PASS** | — | `host.rs:198–201`：`resolve_launch(...).await?` 在 `Supervisor::start(spec).await?`**之前**；全仓唯一 spawn 点是 `process.rs:130`，只能经 `Supervisor::start` 到达；`launch.rs:48–52` 把端口错误一律折叠成 `CredentialUnavailable`（→ `error.rs:120–122` → `Unavailable(KeystoreUnavailable)`）；`catalog.rs:132`（错误类别断言）+ `catalog.rs:135–138`（进程未启） | 反证强度足够：若解析被挪到 spawn 之后，`dump` 文件会出现 → 135 行必红。 |
| **Q2-1** `env_clear` 顺序 | **PASS** | — | `process.rs:119` `command.env_clear()`；`process.rs:120–121` 只注入 `spec.env`；`spec.env` 的唯一构造点 `launch.rs:53–80` | 顺序正确：先清空再逐项注入，不存在「先注入后清空」。 |
| **Q2-2** 节点/设备私钥、token、`ACPR_*` 能否进子进程？ | **PASS**（在现配置下） | — | `grep -rn "ACPR\|KEY\|TOKEN" crates/agent-host` → `src/` 内**零命中**（仅 `tests/catalog.rs:187–190`、`tests/support`、`tests/supervision.rs:322–335` 出现）；`launch.rs:31` 六名常量；`agent-host/Cargo.toml` 不依赖 `identity-keystore`/`identity-auth`，本 crate 无从读取密钥材料（`lib.rs:17` 亦如此声明） | 残余口子：注入集合由**配置**决定——若 `env_allowlist` 与 `env` 绑定里出现 `ACPR_*` 名，`launch.rs` 不会拦（`launch.rs:58–64` 只查白名单）。建议加一条静态拒绝（`name.starts_with("ACPR_")` → `EnvNotAllowed`），把「MUST NOT 出现在子进程环境」（`spec.md:176–179`）落到注入边界本身而不是配置卫生。 |
| **Q2-3** `catalog.rs` 对 `ACPR_*` 的断言有效吗？ | **FAIL** | **MINOR（P2）** | `catalog.rs:186–193`：`assert!(!names.contains("ACPR_NODE_KEY"))` 等四条 — 父进程环境里根本没有这四个名字，`env_clear` 存不存在都会通过 | 是**空断言**（tautology）。同一用例真正的反证来自 `catalog.rs:179–183` 的集合差（依赖宿主机恰好有别的变量），以及 `supervision.rs:338` 的 `CARGO_PKG_NAME` 断言。最小修法：删掉四条循环（它给出的安全感是假的），或让父进程显式 set 一个 `ACPR_*` 再比对。 |
| **Q2-4** `LaunchSpec` 形态 | **FAIL** | **MINOR（P2）** | `launch.rs:17–24`：公开导出（`lib.rs:36`）、`#[derive(Debug, Clone, PartialEq, Eq)]`，而 `env` 里是 `launch.rs:71` `expose_secret().to_owned()` 的**明文**凭据 | 当前无任何调用点打印它（`grep tracing::` 在 `launch.rs` 为零命中，`Supervisor` 只存 `program`），所以不是现行泄漏；但公开 `Debug` 让「谁 Debug 一下就进日志」变成一键事故。建议手写 `Debug` 只打变量名/数量（与 `CORE_PORTS_AND_STORAGE.md:1259`「日志只记变量名与数量」同口径）。 |
| **Q3-1** profile 是否只走端口、有无旁路？ | **PASS** | — | `launch.rs:41` `config.profile(agent)`；`host.rs:357` `self.config.profiles()`；对 `crates/agent-host/src` 穷举 `std::env/fs/File/read_to_string/vars()`：命中仅 `launch.rs:78`（`NECESSARY_ENV` 取值，与 profile 无关）与 `host.rs:508–512`（可用性探测读 `PATH`，只 `is_file()` 判断） | 无文件读、无「按环境变量覆盖 profile」的路径。注意：`host.rs:508–512` 读的是**宿主** `PATH`，与 profile 来源无关，属允许范围。 |
| **Q3-2** 未知 agent 的错误语义 | **PASS** | — | `launch.rs:45` `ok_or(HostError::UnknownProfile)` → `error.rs:116` `PortError::InvalidRequest("agent profile is not registered")`；与 core 同族错误一致（`use_cases.rs:151–155` 的 workspace 未登记同样用 `InvalidRequest`）；`design.md:124`「按 id 取 profile（缺失 = 参数类错误）」；`crates/core/src/model/ids.rs:488–505` 的 `EntityRef` **没有 Agent 变体** ⇒ `NotFound` 不可表达 | 唯一没覆盖的地方：该分支**没有任何用例**（`catalog.rs` 无 `UnknownProfile` 断言，`grep UnknownProfile` 全仓只命中 `error.rs`/`launch.rs`）。要固定语义，加一条「未登记 agent → `InvalidRequest`」的断言即可。 |
| **Q3-3** 「目录查询不读出凭据值」是否属实？ | **FAIL** | **MINOR（P2）** | `host.rs:11–13` 的模块注释与 `crates/agent-host/src/host.rs:361–362`（`agents()` 里 `self.credentials.resolve_env(&profile).await.is_ok()`）；`ports.rs:845–848` 的返回类型是 `Vec<(String, SecretValue)>` | 注释不实：探测就会把凭据值从 keystore 取出来（每次目录查询、每个 profile 一次）。不影响安全不变量，但会误导后续读者，且在「keystore 会弹窗」的平台上放大副作用。最小修法：改注释为「不做 spawn、不使用凭据值；只探测解析是否可用」，或让端口提供 `is_resolvable()`。 |
| **Q4-1** 回收后映射/端点是否还活着？ | **FAIL** | **MAJOR（P1）** | `host.rs:250–274`：sweep **只** `close_session()` 与 `supervisor.shutdown()`，**不** `self.runtimes.remove(agent)`、也不 `remove_session`；`session.rs:558–572` 的 `close_session` 不触碰 `by_core`/`by_acp`；`host.rs:433–452` 的 `open()` 遍历**所有** runtime（含已回收的那个），命中残留映射后 `runtime.remove_session` + `rebind` + `insert_session` 并返回端点；`session.rs:534–555` `rebind` 把 `closed` **重置为 false** | 语义后果：`open()` 会交出一个「指向已关闭 supervisor、却被重新标记为活跃」的端点 — 比「仍持有死 supervisor」更糟，因为状态位变成「活跃」。与 `spec.md:193`「既有会话映射**不再被复用**」直接冲突。**可达性披露**：core 侧 broker 的 `create` 立刻缓存端点（`broker.rs:1214`），`open` 仅在 slot 无端点时才被调用（`broker.rs:2096–2103`），因此在**当前工作树内**这条路径拿不到触发点；它一旦被 server/app 接线（或 slot 端点被清）就会生效。最小修法：sweep 命中回收时 `runtimes.lock().await.remove(agent)`（破坏映射），且 `open()` 在 `!runtime.supervisor.is_running()` 时显式报错而不是 rebind。 |
| **Q4-2** `spec.md:190–193` 的第二、三子句 | **FAIL** | **MAJOR（P1）** | `spec.md:193`「其进程树被关闭，**目录条目转为不可用**，既有会话映射不再被复用」 vs `host.rs:361–362`：`available` 只由「命令可解析 + 凭据可解析」决定，**与运行状态无关** ⇒ 回收/崩溃后 `agents()` 依然报 `available = true`；`catalog.rs:242–283` 也没有任何回收后的 `agents()` 断言 | 两种裁定都要有人拍：①按字面实现「回收后该条目不可用」；②认定该子句措辞有误（`AgentDescriptor.available` 在 core 合同里没有定义，`CORE_PORTS_AND_STORAGE.md:165` 只有形状）并改写 spec + 记录裁定。**当前状态是「规格子句既没实现也没用例、也没登记为未处理」**，属静默缺口（与 RV1 对 WP4「用例不可证伪」的同类问题）。 |
| **Q4-3** sweep / shutdown 与 ensure_runtime 的互斥 | **FAIL** | **MINOR（P2）** | 交错 1：`host.rs:166–175` 的 `is_running()` 判定与 `host.rs:255` 的 map 快照**不在同一临界区**，`host.rs:256–273` 的逐 runtime 处理（含最长 5 s grace 的 `shutdown().await`）在锁外；`host.rs:380–402` 的 `create` 在 `ensure_runtime` 返回后、`request("session/new").await` 期间**不持锁** ⇒ 窗口内 `create` 仍可在正在关闭的 supervisor 上派发，甚至把新会话 `insert_session` 进正在关闭的 runtime（`host.rs:445–447` 同样的锁外窗口）。交错 2：`host.rs:277–286` 先从 map `remove` 再 `shutdown().await` ⇒ 同一 agent 短暂可被 `ensure_runtime` 起**第二棵树**。交错 3：`host.rs:288–302` 先清空 map 再逐个 shutdown ⇒ `shutdown_all` 期间新的 `ensure_runtime` 可起进程且**不被这一轮覆盖** | 后果可控（都是「显式失败」或「进程随 Daemon 退出被 Job 句柄清掉」），所以给 P2；但「关闭中又启动新进程」与「关闭中的 supervisor 被派发」是真实存在的结构缺口，修法是把回收判定与 `closing` 置位放进同一临界区（或给 runtime 加 reclaiming 状态位让 `ensure_runtime` 拒绝复用）。**无任何用例覆盖**（`catalog.rs` 全是串行调用）。 |
| **Q4-4** `ensure_runtime` 跨 await 持 `runtimes` 锁 | **FAIL** | **MINOR（P2）** | `host.rs:167` 取锁（`let mut runtimes = ...lock().await`），guard 直到 `host.rs:202` `runtimes.insert(...)` 才释放，其间包含 `launch::resolve_launch().await`、`Supervisor::start().await`、`initialize(..., STARTUP_TIMEOUT=10 s)`（`limits.rs:11`）以及崩溃恢复路径的 `shutdown().await`（最长 grace 5 s） | 任一 agent 启动慢/卡住，会串行阻塞 `open()`（`host.rs:433–434`）、`sweep_idle`、`shutdown_all` 以及**其它 agent**的启动。功能正确性未破，属活性/吞吐缺陷。修法：锁内只做 map 读写，启动过程用 per-agent 的一次性初始化锁/`OnceCell`。 |
| **Q4-5** 空闲时钟的定义 | **FAIL** | **MINOR（P2）** | `session.rs:156–159` 的 `is_idle_for` 只读 `last_activity`；该值仅在 `session.rs:292`（收到 agent 入站信封）与 `prompt/finish_turn`（`session.rs:194/198/254`）刷新 — `set_mode`（`session.rs:761–786`）、`set_config`（`session.rs:796–840`）、`cancel_turn`、`resolve`、`modes/list_config` **都不刷新**；`host.rs:302–313` 的回收判定就用它；`host.rs:47–48` 注释称「协商与目录活动也刷新它」，但 `host.rs:168–171` 复用已有 runtime 时**不 touch** | 后果：一个正在被用户操作（改模式/改配置）但没有 prompt 的会话，会在 `idle_timeout` 后被回收，而回收**没有重开路径**（见 Q4-1）⇒ 该会话此后对客户端表现为永久不可用。另外注释与实现不符（复用已运行 runtime 时 `agent_capabilities` 不延寿）。 |
| **Q4-6** `runtime_running` 作为主断言的可靠性 | **FAIL** | **MINOR（P2）** | `host.rs:531–543`：`try_lock().ok().and_then(...).unwrap_or(false)` — 锁被占用时返回 `false`，与「未运行」不可区分；`process.rs:203–205` `is_running()` = `!exit.done && !closing`，而 `process.rs:326–328` `shutdown()` **第一件事**就是 `closing.swap(true)` | 两个后果：①`catalog.rs:273/332` 的「必须关闭」断言可以被「锁恰好被占」假通过；②即便真跑完，它证明的是 `closing` 标志已置位，**不是**「进程树已结束」——grace/terminate/join 全不被该断言约束（`process.rs:333–360`）。建议补一条进程外可观测断言（如心跳文件停止，`supervision.rs` 已有此模式）。 |
| **Q4-7** sweep 是否走 2.19 关闭顺序 | **PASS** | — | `host.rs:271` → `process.rs:325–360`：`fail_pending` → 丢弃写出队列（关 stdin）→ grace 5 s → `tree.terminate()` → `tree.close()` → join 全部子任务（超时 `abort`） | 顺序与 `spec.md:188/200–203` 一致；回收路径额外先 `close_session()`（`host.rs:268–270`），无 turn 时该步只置 `closed`。 |
| **Q4-8** 零值不回收 / 未超时不回收 | **PASS** | — | `host.rs:250–253`（`is_zero()` 早返回）+ `host.rs:37–47`（`HostConfig::idle_timeout()` 过滤零值）；`catalog.rs:264–269`、`catalog.rs:328–329` | 与 `spec.md:195–198` 一致。 |
| **Q5** 用例可证伪性 | 见下表 | — | `tests/catalog.rs` | 7 个用例（`verification.md:49` 记 6 个为 WP5 PV4，`verification.md:149` 记 RV2-F5 补第 7 个）。 |

### Q5 明细：哪些断言改实现后会红

| 用例 | 断言 | 可证伪？ |
|---|---|---|
| `catalog_query_never_spawns_a_process`（55） | `!dump.exists()`（66–68） | **可证伪**：`agents()` 一旦 spawn，子进程会写快照。 |
| | `capabilities.is_empty()`（75）+ `dump.exists()`（76） | **组合可证伪**：伪造能力 → 75 红；未真协商 → 76 红。 |
| | `generation == Some(1)`（78） | **可证伪**（`host.rs:186–195` 的代计数器）。 |
| `availability_is_per_entry_and_credentials_fail_closed`（84） | 条目级可用性（106–107）、凭据失败→不可用（115） | **可证伪**（`host.rs:361–362`）。 |
| | `create` 返回 `Unavailable(_)`（132） | **可证伪**（`error.rs:120–122`）。 |
| | `!dump.exists()`（135–138） | **可证伪**，且是本题最强的反证（见 Q1-3）。 |
| `child_environment_is_exactly_the_launch_spec`（143） | `names ⊆ allowed`（179–183） | **半可证伪**：能抓「多注入宿主机恰有名字的变量」，但**依赖宿主机环境**——新增一个宿主不存在的 `NECESSARY_ENV` 名（例如某个密钥名）不会被抓到；`env_clear` 被删则一定会被抓到（宿主机变量很多）。 |
| | `names.contains("FAKE_TOKEN")`（184） | **可证伪**（去掉凭据注入即红）。**方向单边**：删掉 `NECESSARY_ENV` 注入（PATH/HOME…）该用例仍然通过 ⇒「必要进程环境」这半个需求无用例（`spec.md:164`）。 |
| | `ACPR_*` 不得出现（186–193） | **空断言**（见 Q2-3）。 |
| `profile_selection_ignores_startup_configuration_files`（200） | `dump.exists()`（230）、`!dumped.contains("acpr-from-config-file")`（234） | **不可证伪**：配置文件名/路径（203–209）与实现之间**没有任何接缝**（`src/` 无任何文件读，见 Q3-1），所以「实现改成读配置文件」这个假设根本无法让本用例进入它写的那份文件。`spec.md:181–184` 这个场景目前**没有真实覆盖**（RV1 已指出"配置文件用例不可证伪"，`verification.md:79`，但修复表与"仍未处理"清单里都没有它 ⇒ 被静默丢掉）。 |
| `idle_reclaim_needs_timeout_and_never_fires_for_zero`（242） | 零值/未超时不回收（265、269） | **可证伪**。 |
| | `!runtime_running`（273） | **弱**：只证明 `closing` 置位（见 Q4-6），且 `try_lock` 失败时假通过。 |
| | `runtime_generation == Some(2)`（280） | **可证伪**（回收后重启新的进程代，是本题唯一"跨代缓存失效"的正向证据）。 |
| `credential_variable_outside_the_allowlist_is_refused_before_spawn`（287） | `outcome.is_err()`（302）+ `!dump.exists()`（303–306） | **可证伪**：删掉 `launch.rs:58–64` 的白名单复核 → create 成功且 dump 出现 ⇒ 双红。 |
| `idle_reclaim_also_applies_to_processes_without_sessions`（313） | `runtime_running` 三条（327、329、331–333） | **可证伪但弱**（同 Q4-6）。覆盖 `host.rs:250–262` 的"无会话→按进程级 `idle_for` 回收"分支，这条确实是 RV2-F5 的有效补充。 |

**覆盖缺口（无任何用例）**：`shutdown_agent`（`host.rs:277`，公开 API，全仓只有测试里的 `shutdown_all` 被调用）、`spawn_idle_sweep`（`host.rs:516`，真正的周期驱动与"随 Daemon 关闭"）、`UnknownProfile` 分支、`open()` 的全部路径（`grep "AgentHost::open"` / `backends.open` 在测试目标里零命中）。`grep -rn "shutdown_agent\|spawn_idle_sweep"` 的输出见 §0 证据说明（仅定义与 re-export）。

---

## 2. 未解决阻断项清单

**无 BLOCKER。** 需要在下一轮处置或由用户裁定的是：

1. **Q4-1 / Q4-2（MAJOR）** — `spec.md:190–193` 的三个子句：「进程树被关闭」已实现；「目录条目转为不可用」「既有会话映射不再被复用」**未实现且无用例**。二选一：改实现（sweep 时移除 runtime 与映射；`agents()` 若需反映运行态则补语义与用例），或改 spec 措辞并把这个偏离登记到 `verification.md` 的"仍未处理"清单（当前它不在任何清单里）。
2. **Q5 `profile_selection_ignores_startup_configuration_files`（MINOR，但属"静默丢掉的 RV1 发现"）** — `spec.md:181–184` 无真实覆盖。要么删掉这个假用例并说明"本 crate 无此接缝、该场景归组合根"，要么把它搬到 `app` 切片用真实接缝验证。
3. **Q1-2 / Q2-2（MINOR）** — 缺少「白名单 ∩ 绑定」的期望集合比对（漏注入不可见）与 `ACPR_` 前缀静态拒绝（配置驱动注入不可见）。二者都是小改动、失败关闭。
4. **Q4-3 / Q4-4 / Q4-5 / Q4-6（MINOR）** — 回收/关闭与启动不互斥、跨 await 持锁、空闲时钟不含出站活动、`runtime_running` 的 `try_lock` 假通过通道。建议在接线 `app`（真正会并发调用这些入口）之前把 4-3 与 4-4 一起收口。
5. **Q3-3 / Q2-4 / Q3-2（MINOR）** — 注释与实现不符、`LaunchSpec` 公开 `Debug` 带明文凭据、`UnknownProfile` 无用例。

---

## 3. 可证伪依据（每条 PASS 是被什么排除的，不是复述实现）

| 结论 | 反假设备选路径 | 排除手段 |
|---|---|---|
| Q1-1 白名单外不注入 | ①`launch.rs` 漏掉复核；②`process.rs` 未 `env_clear` 或额外注入 | 逐行读 `launch.rs:54–64`（命中即 `Err`，且 `resolved` 来自 `launch.rs:48–52`）与 `process.rs:119–121`；穷举全仓 `env_clear`/`command.env(`/`vars()`/`set_var` 的命中位置（只有 `process.rs:119/121` 与测试 bin）；再用 `supervision.rs:338` 的 `!text.contains("CARGO_PKG_NAME=")` 证明父进程确有宿主变量而子进程没有它 —— 这条断言在删掉 `env_clear` 后必然变红，是真正的反例排除。 |
| Q1-3 spawn 前失败关闭 | 存在「先 spawn 再解析」或「部分注入后继续」的路径 | 读 `host.rs:198–201` 的调用顺序，并用全仓搜 `command.spawn()`/`Supervisor::start` 的调用点确认唯一 spawn 点（`process.rs:130`）只能由 `Supervisor::start` 到达；`catalog.rs:135` 的 `!dump.exists()` 是"进程没启动"的外观测证据。 |
| Q2-2 无 `ACPR_*`/密钥进子进程 | `NECESSARY_ENV` 或别处硬编码了 `ACPR_*`；本 crate 通过依赖能拿到密钥 | 全仓 `grep "ACPR"` → `src/` 零命中（仅测试出现）；`launch.rs:31` 是显式六名常量；`Cargo.toml` 无 `identity-keystore`/`identity-auth` 依赖 ⇒ 无密钥来源。残余路径（配置驱动的 `ACPR_*` 注入）**未排除**，已如实登记为 Q2-2 的残余口子。 |
| Q3-1 profile 无旁路 | 存在读启动配置文件/环境变量覆盖 profile 的隐藏路径 | 对 `crates/agent-host/src` 做穷举式检索（`std::env`/`std::fs`/`File::`/`read_to_string`/`env::var`/`vars()`），命中只有 `launch.rs:78`（`NECESSARY_ENV` 取值）与 `host.rs:508–512`（探测用 `PATH`）；`profile` 的唯一来源是 `launch.rs:41 config.profile()` 与 `host.rs:357 config.profiles()`，两者都来自注入的 `Arc<dyn LocalConfigStore>`。这是"反向穷举"而不是"读一遍实现"。 |
| Q3-2 未知 agent 错误类别 | 更精确的 `PortError::NotFound(EntityRef)` 应当可用 | 读 `crates/core/src/model/ids.rs:488–505` 确认 `EntityRef` 无 Agent 变体（构造上不可表达）；`use_cases.rs:151–155` 显示 core 对「未登记」类资源一律用 `InvalidRequest`；`design.md:124` 明写"参数类错误"。 |
| Q4-7 sweep 走完整关闭顺序 | sweep 直接 `terminate_tree()` 绕过 grace/join | 读 `host.rs:271` 的唯一调用是 `supervisor.shutdown()`，并逐行核对 `process.rs:325–360` 的五步顺序；`terminate_tree`（`process.rs:322`）在 sweep 路径上零命中。 |
| Q4-8 零值不回收 | 零值被 `filter` 后仍落入判定 | 读 `host.rs:250–253` 的早返回与 `host.rs:37–47` 的零值过滤，二者构成双重条件；`catalog.rs:264–269` 与 `catalog.rs:328–329` 是外部行为证据。 |

---

## 4. 需要主 Agent 代跑的命令（我无 shell，未执行）

```
cargo test --locked -p agent-host --all-features --test catalog -- --nocapture
cargo test --locked -p agent-host --all-features
cargo clippy --locked -p agent-host --all-targets --all-features -- -D warnings
npm run check          # 合同门禁（WP5 未触碰合同资产，预期无漂移）
```

**若要我给出可复现 Q4-3 竞态的最小实验**（自述需要新写测试，不在本次只读范围内）：串行化不可行时，可在 `sweep_idle(Duration::from_nanos(1))` 与一个并发 `create` 之间插入一个慢速 fake child 场景（`session/new` 延迟应答），断言「`create` 成功后该 endpoint 上第一个 `prompt` 必须有明确失败、且 `agents()`/`runtime_generation` 状态与失败语义一致」。

---

## 5. Merge verdict

**OK with notes**（不阻断本地已完成的 fast-forward 合入；但 **Q4-1/Q4-2 是两处 MAJOR，必须"改实现或改规格措辞 + 登记"二者之一**，不能像 RV1 的"配置文件用例不可证伪"那样无声留在原地）。
---

## RV4 复核轮（独立 reviewer，新隔离只读上下文 + bash，2026-09-24）

复核对象：`d0fa731`（基线 `4c24fbd`）。本轮实跑 fmt/clippy/测试与两道门禁（全部 exit 0；45 passed，9 次重复运行无抖动）。

# RV4 独立复核报告 · WP5（配置、凭据、目录、空闲回收）@ `d0fa731`

| 字段 | 值 |
|---|---|
| task_id | `RV4-WP5-recheck`（3.2/3.10 的复核轮） |
| role / phase | 独立 reviewer（新隔离上下文，只读 + bash）/ 修复后复核（recheck） |
| agent_context | 全新子 Agent，未参与实现与修复；有只读文件工具 + bash（仅用于 git 只读、grep/find/ls、跑门禁与测试） |
| repository | `D:/Project/acp-remote`（Windows x64） |
| base_revision | `4c24fbd665738b7bbb34311d525896caa0215e89` |
| target_revision | `d0fa73162a029cabe64715939db807b89aca383c`（`git rev-parse HEAD` = `d0fa731…`，工作区就是该提交） |
| scope | 修复提交 `d0fa731`（`git diff 4c24fbd..d0fa731`: 20 文件 / +1070 −112）中的 WP5 相关面：`crates/agent-host/src/{host.rs,launch.rs,lib.rs,process.rs,session.rs}`、`tests/{catalog.rs,support/mod.rs}`、`src/bin/acpr-fake-acp-agent.rs`、delta spec `specs/local-agent-host/spec.md`、`verification.md`（及 WP1 的文档连带修改，仅在影响本 WP 判定时引用） |
| requirements | `openspec/changes/acp-boundary-and-agent-host/specs/local-agent-host/spec.md`（§Agent 目录暴露与可用性判定、§profile 来源与凭据注入边界、§空闲回收与进程关闭顺序）、`design.md` D7/D8/D11、`plan.md` WP5、`docs/CORE_PORTS_AND_STORAGE.md` §3.6/§5.1、`docs/SECURITY_DESIGN.md` §12.2/§14.1、`docs/CONFIG_REFERENCE.md` §1、`AGENTS.md` §1/§3/§7/§9/§10/§12 |
| previous_findings | `reports/rv1-wp5.md`（WP5 第一轮，Q1-1…Q5）、`reports/rv1-wp1.md` 的「RV3 复核轮」段（WP1，6 个 MINOR） |
| version_stability | 复核对象固定为 `d0fa731`；工作区相对 HEAD 只有 ` M .pi/settings.json`（宿主安装物，与本变更无关）与 ` M openspec/changes/acp-boundary-and-agent-host/verification.md`（主 Agent 正在写的 RV4 登记，非 crate 改动）。`crates/**`、`specs/**` 无未提交改动 ⇒ 我读到的源码就是 `d0fa731` 的内容。本轮我只写 `target/` 构建产物，未改任何被跟踪文件（复核结束时 `git status --porcelain` 仍只有上述两行）。 |

---

## ① 范围与方法

**实跑命令与退出码（本轮亲自执行，全部在 `d0fa731` 上）**

| 命令 | 退出码 | 输出摘要 |
|---|---|---|
| `cargo fmt --all -- --check` | **0** | 无输出 |
| `cargo clippy --locked -p agent-host --all-targets --all-features -- -D warnings` | **0** | `Finished dev profile … in 0.23s`（缓存命中，源码未变）；随后 `cargo clean -p agent-host`（Removed 3306 files, 1.2GiB）再跑一次：`Checking agent-host` → `Finished in 1.28s`，零告警 |
| `cargo test --locked -p agent-host --all-features` | **0** | `3 passed`（lib 单元）+ `0`（bin）+ `15 passed`（`tests/catalog.rs`）+ `12 passed`（`tests/session.rs`）+ `15 passed`（`tests/supervision.rs`）+ `0`（doc-tests）= **45 passed / 0 failed / 0 ignored**；`cargo clean -p agent-host` 后重新编译再跑一次仍为 45 passed |
| 重复稳定性 | **0** | `--test catalog` 连跑 5 次：5×`15 passed`；整包再连跑 4 次：4×45 ok，无 `FAILED`/`failures:` |
| `node scripts/check-crate-boundaries.mjs` | **0** | `crate boundaries OK: 8 个 crate 的依赖方向与 §5 矩阵一致（8 个已在矩阵登记）` |
| `npm run check`（额外，非必跑项） | **0** | 10 道门禁全绿（含 `contract drift OK: §7 的 36 条 DDL…；§5 的 15 个 trait / 87 个方法签名…`）+ `check:agentic`：`PASS`×8、`Totals: 6 passed, 0 failed (6 items)` |

**未执行（不得当作已验证）**：`cargo test --locked --workspace --all-features`（`verification.md` 的「371 条」未由本轮复算）；`cargo check --target x86_64-unknown-linux-gnu`（unix 分支只做静态阅读）；任何 E2E。**「反向探针」（注释掉 `sweep_idle` 的移除/短路静态拒绝）未由本轮实跑**——那需要改源码，越出只读边界；我改为逐条给出可构造的反例与「现断言能否抓住」的判定（见 ④）。

### ①b 并发交错分析（要求 1）：临界区边界与代码行

先把并入同一把锁的边界写清楚（`runtimes` 是 `tokio::sync::Mutex`）：

- **CS-A（`sweep_idle`）**：`runtimes` guard 自 `host.rs:300` 取得，至 `host.rs:319` 释放。临界区内依次做：`is_idle` 判定（`301-305` → `AgentRuntime::is_idle` `host.rs:86-92` → `sessions()` 取 `by_acp`、`idle_for` 取 `last_activity`）→ `runtimes.remove(&agent)`（`308`）→ 逐会话 `close_session()`（`311-313`，只动会话自己的 `inner` 锁）→ `clear_sessions()`（`314` → `host.rs:98-101` 先清 `by_core` 再清 `by_acp`）。**`supervisor.shutdown().await`（`320-322`）在锁外**。
- **CS-B（`ensure_runtime`）**：guard 自 `host.rs:203` 取得，直到函数返回（`244` 的 `runtimes.insert`）才释放，其间跨越 `shutdown().await`（`216`）、`resolve_launch(...).await`（`219-220`）、`Supervisor::start(...).await`（`221`）、`initialize(..., 10s).await`（`230`）。
- **CS-C（关闭入口）**：`shutdown_agent`（`328` 只做 `remove`，shutdown 在 `334` 锁外）、`shutdown_all`（`344-348` 取快照并 `clear`，shutdown 在 `349-355` 锁外；`342` 先置 `shutting_down`）。
- **CS-D（`open`）**：`489-490` 取快照后**立即释放**锁；随后 `492-494`（读 `by_core`）、`496-500`（读 `is_running`）、`501-503`（读 `by_acp`）、`506-510`（`remove_session` + `rebind` + `insert_session`）全部在锁外。

**收敛性判定（逐交错）**

| 交错 | 是否收敛 | 依据 |
|---|---|---|
| `sweep_idle` × 顺序 `open`（回收已完成） | **收敛到显式失败** | 运行时已不在目录（`308`）且映射已清（`314`）⇒ `open` 快照（`489-490`）里没有它 ⇒ 落到 `513` 的 `InvalidRequest`。用例 `catalog.rs:511-518` 断言这一点 |
| `sweep_idle` × `open`（`open` 快照先于回收） | **不收敛到显式失败（残余）** | `open` 拿到的 `Arc` 在 `308` 之后仍存活；只要它的 `by_core`/`is_running`/`by_acp` 三次读都落在 `314` 的 `clear_sessions` 之前，`506-510` 就会 `rebind`（`session.rs:538-559` 把 `closed` 重置为 `false`）并返回 `Ok`，而该 runtime 已被移出目录、其 supervisor 随后在 `321` 被关闭 ⇒ 返回一个「看起来活跃、随即必失败」的端点。现有用例全是串行调用，抓不到 |
| `sweep_idle` × `create`（`create` 已从 `ensure_runtime` 拿到 `Arc`） | **不收敛到显式失败（残余）** | `create` 在 `449-476` 期间不持锁、不复查运行状态：`session/new`（`450-454`）与 `insert_session`（`474-476`）可落在已被回收的 runtime 上（会话变成孤儿，`shutting_down`/映射都不再可达）。结果仍是显式错误或不返回（客户端 prompt 明确失败），但 `create` 可能返回 `Ok` |
| `ensure_runtime` × `ensure_runtime`（同/异 agent） | **收敛** | CS-B 覆盖整个「启动 + 协商」，后到者只会复用或重建，不会造出两个进程/两条协商（`204-208`） |
| `ensure_runtime` 的 stale 分支 × 其它入口 | **收敛** | stale 处理（`211-216`）在 CS-B 内完成（含 `shutdown().await`），其它入口拿不到锁 |
| `sweep_idle` × `sweep_idle` / × `shutdown_all` | **收敛** | 同一 runtime 的 `supervisor.shutdown()` 幂等（`process.rs:353-355` 的 `closing.swap(true)` 早返回）；清目录在锁内，第二者看到的是空目录 |
| `shutdown_agent` × `ensure_runtime` | **不收敛到单棵树（残余）** | `328` 先 `remove`、`334` 才 shutdown：窗口内 `ensure_runtime` 找不到条目 ⇒ 起**第二棵树**，两棵树短暂并存（旧树正在被结束、其映射已清，无害但结构上不互斥） |
| `shutdown_all` × `ensure_runtime` | **基本收敛，残留一个极窄窗口** | `342` 先置标志、`200-202` 短路，`shutting_down` 之前的 3 种交错里这一条被闭合；但 `200` 的检查在 `203` 取锁**之前**：若某调用已读到 `false` 而锁被占用（本交错内唯一可能：读标志与首次 `lock()` poll 之间被插入），它仍会在 `shutdown_all` 清表之后启动新进程并 `insert`（`244`）⇒ 关闭返回后残留一棵树，由 `Supervisor::drop`（`process.rs:418-423` → `tree.close()`）或 Windows 的 Job 句柄兜底。因 `tokio::sync::Mutex` 按 FIFO 授予，只有在「读标志→poll 锁」这一纳秒级区间被插入时才发生（若它先排队，则 `shutdown_all` 的快照必然覆盖它新起的 runtime） |

**结论（要求 1）**：`d0fa731` 之后，**回收/关闭对目录与映射的破坏是在同一临界区完成的**（CS-A 的 `308`+`314`，CS-C 的 `328`+`333`、`346`+`353`），`open` 另有独立于映射的 `is_running` 短路（`496-500`），`ensure_runtime` 对「已退出但还挂在目录」的 runtime 改为作废后重建（`209-217`）——**RV3 的两条 MAJOR 路径在顺序与「锁序」交错下都已不存在**。剩下的三类残余都不是「静默错误状态」，而是「显式失败或短暂双树/孤儿端点」，且都必须由调用方在锁外持有 `Arc` 或越过一次性标志检查才会出现（详见 ③/F9）。

---

## ② 逐条结论表（RV3 编号 | 结论 | 级别 | 证据）

| 编号 | 结论 | 级别 | 证据（文件:行 / 命令+输出摘要） |
|---|---|---|---|
| **Q4-1** `sweep_idle` 不破坏运行时与映射 → `open()` 复活死端点 | **PASS（已修，MAJOR 闭合）** | MAJOR→无 | 修：CS-A 同临界区 `remove`+`close_session`+`clear_sessions`（`host.rs:300-319`）；`open` 命中映射后先查 `is_running()` 再决定（`host.rs:496-500`）；stale runtime 作废重建（`host.rs:209-217`）。证：`catalog.rs:477-531` 的四条断言（① `runtime_generation==None`、② 心跳停止增长=进程外证据、③ `open`→`Err(InvalidRequest)`、④ 可用性不变）；`catalog.rs:611-639` 覆盖「运行时还在目录、进程已退出」这条 `is_running` 路径（`assert_eq!(runtime_generation, Some(1))` 证明命中的是 2 而非 fallthrough）；本地实跑 45 passed / 0 failed |
| **Q4-2** spec「目录条目转为不可用」未实现/未用例/未登记 | **PASS（按 spec 措辞路径闭合）** | MAJOR→无 | 修：`spec.md:193` 改为「可用性 MUST NOT 因回收而改变（仍只由『命令可解析 + 凭据可解析』决定）+ 后续 `open` MUST 显式失败 + 下次请求按需重启（新进程代）」；`host.rs:10-11`、`415-416` 同步；用例 ④（`catalog.rs:520-526`）与 `idle_reclaim_needs_timeout_and_never_fires_for_zero`（`catalog.rs:411-416`，重启后 `generation==2`）分别钉住两条子句。措辞与 `spec.md:9`（可用性=启动前提）自洽（见 ⑤） |
| **Q1-1** 白名单外变量不得注入 | **PASS（无回归）** | — | `launch.rs:76-94`（逐名校验 + 越界 `EnvNotAllowed`，`85`）；`process.rs:119-121`（`env_clear` 后只注入 `spec.env`）；`catalog.rs:422-445`（`leak` → `create` 必失败 + `!dump.exists()`）；`src` 内唯一 spawn 点仍是 `process.rs:129`（`grep -rn "\.spawn()" crates/agent-host/src` 只命中 `process.rs:129` 与 bin 的 fake 孙进程） |
| **Q1-2** 缺「白名单 ∩ 绑定」期望集合比对 | **PASS（漏的方向已修）** | MINOR→见 F1/F2 | 修：`launch.rs:96-114` 在 **NECESSARY_ENV 注入之前**（`116-123`）比对「已绑定且在白名单内」的期望集合，缺一即 `CredentialUnavailable`（失败关闭）；用例 `catalog.rs:746-777`（`FakeCredentials::dropping`，`support/mod.rs:403-413/441-445`）断言 `Unavailable` + `!dump.exists()`。残余：只查 ⊇，不查「多给的白名单内未绑定名」，且「必须在校对前」这一 load-bearing 顺序无用例（F1/F2） |
| **Q1-3** 失败是否 fail-closed（spawn 前） | **PASS** | — | `host.rs:219-221`：`resolve_launch(...).await?` 先于 `Supervisor::start`；歧义失败一律折叠成 `CredentialUnavailable`（`launch.rs:70-73`）→ `error.rs:120-122` → `Unavailable(KeystoreUnavailable)`；`catalog.rs:135-138`/`266-269`/`439-442`/`771-774`/`803-806` 四处 `!dump.exists()`（新增两处随修复一并加上） |
| **Q2-2** 无 `ACPR_` 静态拒绝 | **PASS（已修）** | MINOR→见 F5 | 修：`launch.rs:53` `RESERVED_ENV_PREFIXES = ["ACPR_","ACP_REMOTE_"]`，`validate_env_name`（`139-153`，`to_ascii_uppercase` 后前缀比较）→ `EnvNotAllowed`；单元用例 `launch.rs:172-194`（含 `ACPR_`、`ACP_REMOTE_CONFIG`、`acp_remote_config`，并断言 `TOKEN`/`ACP_REMOTE` 不被误拒）；集成用例 `catalog.rs:779-809`（`InvalidRequest` + `!dump.exists()`，实跑 ok）。残余：`ACPR_` 未在任何权威文档登记（F5）；该拒绝只落在 `resolve_launch`，`LaunchSpec` 字段与 `process::Supervisor::start` 均 `pub`（`lib.rs:33/39`） |
| **Q2-3** `catalog.rs` 的 4 条 `ACPR_*` 断言是恒真 | **PASS（已修）** | MINOR→无 | `git diff` 删掉这 4 条，并在 `catalog.rs:317-319` 写明原因与替代覆盖（静态拒绝用例 + `launch.rs` 单元测试）；现用例仍有 `names ⊆ allowed` 的集合差（`311-315`）与 `contains("FAKE_TOKEN")`（`316`） |
| **Q2-4** `LaunchSpec` 公开 `Debug` 带明文凭据 | **PASS（env 值方向已修）** | MINOR→见 F4 | 修：手写 `Debug`（`launch.rs:33-43`）只输出 `program`/`args`/`env_names`/`env_count`；单元用例 `launch.rs:196-216` 断言 `!text.contains("secret-value-do-not-log")` 且仍含变量名与数量。残余：`args` 原样输出（F4） |
| **Q3-1** profile 只走端口、无旁路 | **PASS（无回归）** | — | 本轮重跑穷举：`grep -rn "std::env|std::fs|File::|read_to_string|env::var|vars()" crates/agent-host/src`（排除 `bin/`）只命中 `launch.rs:120`（NECESSARY_ENV 取值）与 `host.rs:569/573`（可用性探测读宿主 `PATH`）；profile 唯一来源仍是 `launch.rs:61-67`（`config.profile`）与 `host.rs:412`（`config.profiles`） |
| **Q3-2** `UnknownProfile` 无用例 | **FAIL（未修，已登记）** | MINOR | 分支仍在 `launch.rs:67` → `error.rs:116`（`InvalidRequest`）；全仓 `grep -rn UnknownProfile crates/agent-host/tests` 无命中；该路径在测试里**可构造**（`support/mod.rs:315-323` 对未登记 id 返回 `Ok(None)`），所以不是「无法覆盖」而是「未覆盖」 |
| **Q3-3** `agents()` 注释不实（探测会读出凭据值） | **PASS（已修）** | MINOR→无 | `host.rs:8-9`「不做 spawn、不消费凭据值：它只探测『凭据能否解析』，而探测本身会从 keystore 取值（端口没有 `is_resolvable()`）」；`host.rs:415-416` 同口径 |
| **Q4-3** 回收/关闭与启动不互斥（3 种交错） | **FAIL（部分修复，残余已登记）** | MINOR（P2） | 已闭合：交错 3 的顺序版本（`shutting_down`，`host.rs:155-156/200-202/342`）与「死 runtime 被复用」（见 Q4-1）。仍未闭合：① `create`/`open` 在 `ensure_runtime` 之后不复查运行状态（`host.rs:449-476`、`496-510`）；② `shutdown_agent` 先 `remove` 后 shutdown（`328`→`334`）⇒ 可短暂双树；③ 标志检查在锁外（F9）。三者都只产出显式失败/短暂双树，`verification.md:171` 已登记「留待接线 `app` 前与 Q4-3 的残余交错一起收口」 |
| **Q4-4** `ensure_runtime` 跨 `await` 持 `runtimes` 锁 | **FAIL（未修，已登记）** | MINOR（P2，活性） | `host.rs:203`→`244` 的 guard 覆盖 `shutdown`（`216`）、`resolve_launch`（`219-220`）、`start`（`221`）、`initialize`（`230`，`limits.rs:10`=10s）；`verification.md:170` 登记为活性/吞吐缺陷，修法（per-agent 初始化锁/`OnceCell`）留待接线前。功能正确性未见破坏（测试用 `wait_for_generation`/重复采样适配了「启动期间 `try_lock` 读不到」） |
| **Q4-5** 空闲时钟不含出站活动 | **PASS（已修）** | MINOR→见 F3 | 修：`session.rs:767`（`set_mode`）、`795`（`list_config`）、`800`（`modes`）、`810`（`set_config`）、`459`（`cancel_turn`）、`483`（`resolve`）各加 `touch`；入站仍是 `session.rs:292`，prompt/终态仍刷新（`192/254`）；协议门控提前 `touch` 再判能力（`765-770`），语义不变。用例 `catalog.rs:534-583`（空闲 300ms → `modes()`+`set_mode` → 200ms 超时不回收；再等 400ms → 回收且心跳停止）。残余：该用例无法独立钉住 `set_mode` 的 `touch`（F3） |
| **Q4-6** `runtime_running` 的 `try_lock` 假通过 | **FAIL（部分修复，残余已登记）** | MINOR（P2） | 修：`try_read_runtimes`（`host.rs:595-606`）有界重试（8 次 + yield），并在 `590-594` **如实写明**「`None` 与『没有该条目』不可区分，因此这两个读函数是诊断手段，不是进程真的结束了的证据；进程外证据请用心跳文件」；回收/活动两条用例改成心跳断言（`catalog.rs:502-509`、`573-579`）。残余：`idle_sweep_task_converges_on_shutdown_all` 的「关闭后不得再起进程」仍只由这两条诊断函数支撑（F7） |
| **Q4-7** sweep 走完整 2.19 关闭顺序 | **PASS（无回归）** | — | `host.rs:321` 唯一调用 `supervisor.shutdown()`；顺序仍为 `fail_pending` → 丢弃写出端 → 5s grace → `tree.terminate()` → `tree.close()` → join（`process.rs:353-375`）；回收路径额外先 `close_session`（`host.rs:311-313`） |
| **Q4-8** 零值不回收 / 未超时不回收 | **PASS（无回归）** | — | `host.rs:296-298`（`is_zero()` 早返回）+ `is_idle`（`86-92`，`all(|s| s.is_idle_for(timeout))`，`session.rs:156-159` 要求 `!closed && !turn_running && elapsed>=timeout`）；用例 `catalog.rs:399-405`、`464-465` |
| **Q5-A** `catalog_query_never_spawns_a_process` | **PASS** | — | `catalog.rs:191-212`：`!dump.exists()` 后 `agent_capabilities` 才让 `dump.exists()` 为真 + `generation==Some(1)` |
| **Q5-B** `availability_is_per_entry_and_credentials_fail_closed` | **PASS** | — | `catalog.rs:238-239`（逐条目）+ `264`（`Unavailable`）+ `266-269`（spawn 前失败关闭） |
| **Q5-C** `child_environment_is_exactly_the_launch_spec` | **PASS（删掉恒真断言）** | SUGGESTION | `catalog.rs:311-319`；恒真的 4 条已删并留注释。残余：`NECESSARY_ENV` 注入只有上界方向被断言（删掉注入该用例仍绿） |
| **Q5-D** `profile_selection_ignores_startup_configuration_files` | **PASS（改为可证伪）** | — | `catalog.rs:341-352`：断言 `FakeConfig::profiles_calls()` 增长（`support/mod.rs:283-306` 计数）——「实现改成只读配置文件/用文件条目覆盖」都会让该断言或 `dump.exists()`（`364-367`）失败；`verification.md:172` 仍如实登记「纯否定命题（不读任何配置文件）在本层无接缝」，该否定由静态检索支撑（本轮重跑见 Q3-1） |
| **Q5-E** `idle_reclaim_needs_timeout_and_never_fires_for_zero` | **PASS** | — | `catalog.rs:399-416`：零值/未超时不回收 + 回收后 `generation==Some(2)`（跨代缓存失效的正向证据） |
| **Q5-F** `credential_variable_outside_the_allowlist_is_refused_before_spawn` | **PASS** | — | `catalog.rs:438-442` |
| **Q5-G** `idle_reclaim_also_applies_to_processes_without_sessions` | **PASS** | — | `catalog.rs:463-470`（覆盖 `is_idle` 的 sessions-empty 分支） |
| **Q5-H（新增用例）** `reclaim_invalidates_runtime_and_session_mappings` | **PASS** | — | `catalog.rs:477-531`（见 Q4-1）——本轮最强的一条修复证据：目录移除 + 进程外心跳 + `open` 显式失败 + 可用性不变，四者齐备 |
| **Q5-I（新增）** `open_refuses_a_runtime_whose_process_has_exited` | **PASS** | — | `catalog.rs:611-639`（覆盖 `host.rs:496-500`；`generation==Some(1)` 证明命中该分支） |
| **Q5-J（新增）** `open_unknown_session_is_an_explicit_error` | **PASS** | — | `catalog.rs:593-605`（`InvalidRequest` + `generation==None` + `!dump.exists()`） |
| **Q5-K（新增）** `shutdown_agent_closes_one_agent_without_touching_others` | **PASS** | — | `catalog.rs:643-693`（另一 agent 不受影响 + 重启后 `generation==2`；补上 RV3 的「`shutdown_agent` 无覆盖」） |
| **Q5-L（新增）** `idle_sweep_task_converges_on_shutdown_all` | **PASS** | SUGGESTION | `catalog.rs:695-744`（`spawn_idle_sweep` 有覆盖、`shutting_down` 由最后一条 `create`→`Unavailable` 钉住）；证据强度见 F7 |
| **Q5-M（新增）** `dropped_allowlisted_credential_variable_fails_before_spawn` / `acpr_prefixed_credential_name_is_refused_before_spawn` | **PASS** | — | `catalog.rs:746-777` / `779-809`（见 Q1-2、Q2-2） |
| **RV4-WP5-F1**（本轮新发现）期望集合校验相对 `NECESSARY_ENV` 的顺序无用例 | **FAIL** | MINOR | 见 ④(b)-3：把 `launch.rs:96-114` 移到 `116-123` 之后，全套 45 用例仍绿；后果是「绑定名为 PATH/HOME/… 的凭据被漏给时，宿主值会冒充凭据静默注入」，直接违反 `spec.md:169`/`171-174`。建议补 `profile_with_env_vars(..., &["PATH"])` + `FakeCredentials::dropping("PATH")` 的用例 |
| **RV4-WP5-F2**（本轮新发现）期望集合只查 ⊇ | **FAIL** | SUGGESTION | `launch.rs:96-114` 只验证「已绑定且白名单内的名都在结果里」；端口返回「白名单内但未绑定」的额外名会被 `76-94` 放行（白名单是上限，不是等式）。当前 `FakeCredentials` 按绑定迭代（`support/mod.rs:432-445`），造不出这种反例 ⇒ 无用例；影响受「仍在白名单内」约束，非安全越界 |
| **RV4-WP5-F3**（本轮新发现）`set_mode` 的 `touch` 无法被该用例钉住 | **FAIL** | MINOR | `catalog.rs:549-563`：`modes()`（`551`，其 `touch` 在 `session.rs:800`）先于 `set_mode`（`553-556`）刷新了同一个时钟 ⇒ 删掉 `session.rs:767` 的 `self.session.touch()` 后该用例仍绿；`cancel_turn`/`resolve`/`set_config`/`list_config` 的 `touch` 完全无用例。修法：`modes()` 挪到 `tokio::time::sleep` 之前（或用固定 mode id），让窗口内只有 `set_mode` 一个刷新点 |
| **RV4-WP5-F4**（本轮新发现）`Debug` 仍原样输出 `args` | **FAIL** | SUGGESTION | `launch.rs:38` 打印 `&self.args`，而 `docs/SECURITY_DESIGN.md` §14.1 明确「截断并脱敏的 Agent executable 名称，**不记录完整参数中的 secret**」；当前无调用点打印 spec（`grep` 只命中 `launch.rs:209` 的用例），故非现行泄漏 |
| **RV4-WP5-F5**（本轮新发现）`ACPR_` 保留前缀未登记 | **FAIL** | SUGGESTION | `launch.rs:51-53` 自称「`ACPR_*` 是本 crate 的约定」，但全仓检索只在 agent-host 源码/测试与 RV3 报告里出现；有依据的只有 `ACP_REMOTE_`（`docs/CONFIG_REFERENCE.md:21` 的 `ACP_REMOTE_CONFIG` + `crates/core/src/model/config.rs:27-28`）。建议把该前缀写进权威文档（`docs/SECURITY_DESIGN.md` §12.2 注入边界 / `docs/CORE_PORTS_AND_STORAGE.md` §3.7）或只保留 `ACP_REMOTE_` |
| **RV4-WP5-F6**（本轮新发现）stale runtime 作废分支无用例 | **FAIL** | SUGGESTION | `host.rs:209-217` 新增的「进程已退出但仍在目录 → 作废 + 清映射 + 关闭 + 重建」无任何用例：`open_refuses_a_runtime_whose_process_has_exited` 只覆盖 `open`，`idle_reclaim_*` 覆盖的是「已移出目录」。删掉 `211-216`（只剩 `244` 的 `insert` 覆盖）各用例仍绿。建议补「crash-on-prompt → 再 `agent_capabilities` → `generation==2` + 旧 endpoint 明确失败」 |
| **RV4-WP5-F7**（本轮新发现）关闭后「不得再起进程」缺进程外证据 | **FAIL** | SUGGESTION | `catalog.rs:715-728` 只用 `runtime_generation`/`runtime_running`（`host.rs:590-606` 自述不可靠）；该 profile 已带 `--dump-env`（`catalog.rs:698`），只要在 `shutdown_all` 之前删掉 dump 并在最后的 `create` 之后加 `assert!(!dump.exists())` 即可把结论落到进程外 |
| **RV4-WP5-F8**（本轮新发现）spec 的「并给出原因」在本层无落点 | **FAIL** | SUGGESTION | `spec.md:9` 与 `:18-19` 要求「标记为不可用并给出原因」，而 `AgentDescriptor`（`crates/core/src/model/backend.rs:18-22`、`docs/CORE_PORTS_AND_STORAGE.md:165`）没有 `reason` 字段；可观察的「原因」只有 `create`/`agent_capabilities` 的错误类别（`catalog.rs:260-264` 已断言 `Unavailable(KeystoreUnavailable)`）。建议把该句写实或登记（见 ⑤） |
| **RV4-WP5-F9**（本轮新发现）`shutting_down` 检查在锁外 | **FAIL** | MINOR | `host.rs:200`（读标志）与 `203`（取锁）之间无复查；`shutdown_all` 在 `342` 置位后才取锁，因此只在「读标志 → 首次 poll 锁」的纳秒窗口被插入时可越过，进而在清表后启动新进程。修法：在 `203` 之后（或 `insert` 之前）再查一次标志，或把注释（`155-156`、`340`「此后一律拒绝」）收窄为「已在启动流程中的调用除外」 |
| **RV4-WP5-F10**（本轮新发现，**先于本轮存在、非本修复引入**）已 `close` 的会话让 runtime 永不空闲 | **FAIL** | MINOR（潜在） | `session.rs:156-159` 的 `!inner.closed` 使已关闭会话恒不满足 `is_idle_for`，而 `host.rs:86-92` 用 `all(...)` ⇒ 只要 `by_acp` 里留着一个 `closed=true` 的会话，该 runtime 永不因空闲回收。`Endpoint::close()`（`session.rs:871-874`）正是这样一条只置 `closed`、不摘映射的路径；core 现在**没有**调用它（`grep -rn "\.close()" crates/core/src` 无生产调用），故为潜在风险，但一旦 Sync 适配器按端口用 `close()` 就会变成真实进程泄漏 |

**基线对照（确认修复方向正确）**：`git show 4c24fbd:crates/agent-host/src/session.rs` 显示 `is_idle_for` 的 `!inner.closed` 与 `git show 4c24fbd:crates/agent-host/src/host.rs` 的 sweep（只 `close_session`+`shutdown`、无 `remove`/无 `clear_sessions`）与 RV3 描述逐字一致 ⇒ RV3 的两条 MAJOR 确实是本轮被修的对象，F10 不是本修复引入。

---

## ③ 未解决阻断项清单

**无 CRITICAL/MAJOR 未解决项。** 需要在后续处置或由用户裁定的（全部非阻断）：

1. `RV3-Q4-3` 残余交错（① `create`/`open` 在锁外不复查运行状态；② `shutdown_agent` 先 `remove` 后 shutdown 的双树窗口；③ `RV4-WP5-F9` 的纳秒窗口）——已登记，要求接线 `app` **之前**收口。
2. `RV3-Q4-4` `ensure_runtime` 跨 `await` 持锁（活性/吞吐）——已登记，同上。
3. `RV3-Q3-2` `UnknownProfile` 无用例——已登记，但代价极小（4 行）。
4. `RV3-Q4-6` 残余 + `RV4-WP5-F7`：关闭路径的「不得再起进程」只有诊断型证据。
5. 本轮新增的 10 项（F1–F10）中，**MINOR 三条**（F1 顺序无用例、F3 `set_mode` 无法独立钉住、F9 标志检查在锁外、F10 已关闭会话阻塞回收——F10 先于本轮存在），其余为 SUGGESTION。
6. 交付面上「不以本轮结论代替」的部分：`verification.md` 的 371 条工作区测试、Linux 目标 `cargo check`、以及实现者自述的「反向探针双红」均未由本轮复算。

---

## ④ 可证伪依据（要求 3：反例构造 + 现断言能否抓住）

| 目标结论 | 具体反例构造 | 现断言是否能抓住 |
|---|---|---|
| **(a) 回收后 `open` 必须失败** | 1. 删掉 `host.rs:308` 的 `runtimes.remove`（只 `close_session`+`clear_sessions`） | **能**：`catalog.rs:495-499` 断言 `runtime_generation==None` 先红 |
| | 2. 保留 `remove`、删掉 `314` 的 `clear_sessions` | **不能**（顺序调用下 `open` 的快照 `489-490` 已看不到该 runtime，`③` 仍绿）。但该状态无害：runtime 已被丢弃，映射不可达且不会被重新 `insert`（每次重建都是新 `Arc`） |
| | 3. **并发构造**：在 CS-A 的 `308` 与 `314` 之间插入一次让出（等价于真实调度切走，例如 `yield_now().await`），让并发 `open` 的三次读（`492`/`498`/`501`）都落在 `clear_sessions` 之前 | **不能**：所有相关用例都是串行调用（`sweep_idle().await` 返回后才 `open`），`③` 必绿——这正是 ①b 的残余交错 |
| | 4. 删掉 `host.rs:496-500` 的 `is_running` 短路 | **能**：`catalog.rs:627-635` 的 `assert!(outcome.is_err())` 红（该用例就是为这条路径新增的；`generation==Some(1)` 排除了「走 fallthrough」的假通过） |
| **(b) 漏注入必须 spawn 前失败** | 1. 把 `Supervisor::start` 提到 `resolve_launch` 之前 | **能**：`catalog.rs:135-138`/`266-269`/`439-442`/`771-774`/`803-806` 的 `!dump.exists()` 红 |
| | 2. 删掉 `launch.rs:96-114` 的期望集合块 | **能**：`catalog.rs:767-770` 的 `Err(Unavailable)` 红 + `771-774` 的 `!dump.exists()` 红（`create` 会成功、dump 会出现） |
| | 3. 把该块**移到** `116-123`（NECESSARY_ENV 注入）之后 | **不能**（新发现 F1）：现用例的漏给变量是 `FAKE_DROPPED`（`catalog.rs:757`），不在 `NECESSARY_ENV` 六名内，移序后仍红不了 ⇒ `spec.md:169/171-174` 的 fail-closed 会在「绑定名 = PATH/HOME/…」这一类配置上被静默破坏 |
| | 4. 端口返回「白名单内但未绑定」的额外名 | **不能**（F2）：`FakeCredentials` 遍历绑定（`support/mod.rs:432-445`），构造不出这种输出；`catalog.rs:311-315` 的 `allowed` 是硬编码 7 名，只有当额外名不在其中时才红 |
| | 5. 端口返回绑定名但值为空串/被截断 | **不能**：契约只约束名字集合，本层无「值必须非空」不变式（属端口契约，非本层义务） |
| **(c) `ACPR_`/`ACP_REMOTE_` 静态拒绝** | 1. 删掉 `launch.rs:143-151` 的前缀判断 | **能**：`catalog.rs:799-806` 双断言红（`create` 会成功且 dump 出现）+ 单元用例 `launch.rs:173-194` 红 |
| | 2. 用小写/混合大小写（`acpr_token`、`Acpr_X`） | **能**：`to_ascii_uppercase`（`143`）+ 用例里的 `acpr_remote_config`（`181`）覆盖 |
| | 3. 绕过 `resolve_launch`：直接 `Supervisor::start(LaunchSpec{ env: [("ACPR_NODE_KEY", …)] })`（模块与字段均 `pub`，`tests/support/mod.rs:27-49` 已在这么做） | **不能**，且无用例；但生产路径只有 `launch.rs:125` 一处构造（`grep -rn LaunchSpec crates/` 证实），因此是结构性提示而非现行漏洞 |
| | 4. 把保留名只放进 `env_allowlist`（不放绑定） | 不构成反例：不被解析也不被注入，`spec.md:166-169` 已满足 |
| **(d) `Debug` 不泄露凭据** | 1. 换回 `#[derive(Debug)]`（或删掉手写 impl） | **能**：`launch.rs:196-216` 红 |
| | 2. 用 `{:#?}`（alternate）格式化 | **不能也没有必要**：alternate 走同一手写 impl，只影响缩进 ⇒ 实现上安全，仅无显式断言 |
| | 3. 把凭据放进 `args`（如 `--api-key=…`） | **不能**（F4）：Debug 原样打印 `args`，而 `SECURITY_DESIGN` §14.1 明确「不记录完整参数中的 secret」 |
| **(e) 空闲时钟覆盖出站活动** | 1. 只删掉 `session.rs:767`（`set_mode` 的 touch） | **不能**（F3）：`catalog.rs:551` 的 `modes()`（其 touch 在 `session.rs:800`）在同一次空闲窗口里已经刷新了时钟 ⇒ 用例仍绿 |
| | 2. 删掉全部出站 `touch`（含 `modes`） | **能**：`catalog.rs:559-563` 的 `Some(1)` 断言红 |
| | 3. 只删 `cancel_turn`/`resolve`/`set_config`/`list_config` 的 touch | **不能**：无任何用例覆盖这四处 |

---

## ⑤ 对「登记而非修复」条目的可接受性判断（要求 5，独立判断）

| 条目 | 我的判断 | 理由 |
|---|---|---|
| **Q4-3 的残余交错**（本轮细化为 ①b 的三类） | **可接受（登记即可）**，但登记的绑定条件必须明确 | 三类残余都不是「静默错误状态」：要么是显式失败（`create` 拿到被回收 runtime → `session/new`/`prompt` 明确报错），要么是短暂双树/孤儿端点（旧树正在被结束、映射已清）。触发都需要调用方在锁外持 `Arc` 或越过一次性标志；当前 `open`/`create` 的并发入口尚未接线 —— core 的 broker 会缓存端点（`crates/core/src/broker.rs:1209-1214`），只在 slot 为空时才调 `open`（`:2091-2103`），因此现实不可达。`verification.md:171` 已把它们与 Q4-4 绑在「接线 `app` 前收口」，我认为这个绑定是正确的最小处置；但我要求登记文字在把 ③ 也纳入（现在只写了「残余交错」）后**不得**再被当作「已闭合」引用 |
| **Q4-4 跨 `await` 持锁** | **可接受** | 只损活性/吞吐，且被 `STARTUP_TIMEOUT`(10s) 与 `SHUTDOWN_GRACE`(5s) 双重界定；不会造成错误状态。注意其副作用已被本轮正确吸收：`runtime_running`/`runtime_generation` 在此期间返回 `false`/`None`，测试改用 `wait_for_generation`/重复采样（`catalog.rs:140-154`、`715-723`）——这是必要的适配，不是掩盖 |
| **Q3-2 `UnknownProfile` 无用例** | **可接受（偏低优先），但建议顺手补** | 分支只有一行（`launch.rs:67`）且映射已在 `error.rs:116` 收敛，与 core 的「未登记 → `InvalidRequest`」同族（`use_cases.rs` 的 workspace 未登记同口径）；`FakeConfig::profile` 对未登记 id 返回 `Ok(None)`（`support/mod.rs:315-323`），所以这是「4 行可测而未测」，不是「不可测」。登记可以接受，但不该跨过最终验收 |
| **`runtime_running` 的 `try_lock` 局限** | **可接受，且本轮处置合格** | 修复做了正确的两件事：把局限写进代码（`host.rs:590-594`）而不是继续假装它是证据；把回收/活动的关键结论迁到心跳文件（进程外）。残余（F7）建议顺手补一条 `!dump.exists()`，不构成阻断 |
| **Q5 配置文件名场景的归属** | **可接受，不需要移交 `app` 切片** | 现在有一个真实接缝（`profiles_calls` 计数），能证伪「改用配置文件的条目作为来源」这两类真实回退；纯否定命题「不读任何配置文件」在本层确实没有接缝，但它是**可静态穷举**的（本轮重跑 `grep` 只命中 `launch.rs:120` 与 `host.rs:569/573`），与「`cfg` 只落在一处」的同类判据一致。把它移交 `app` 只会让「profile 来源」这条边界失去本层最直接的证据 |
| **Q4-2 的 spec 措辞修正（要求 4 的判定）** | **判定：修正正确、与实现一致，不掩盖缺陷** | ① 与同一规范的 `spec.md:9` 直接自洽：那里的 Requirement 已把可用性定义为「启动前提（命令可解析、凭据引用可用）不满足时标记不可用」，原句「目录条目转为不可用」与它自相矛盾（回收不是启动前提）；② 与 core 的形状自洽：`AgentDescriptor{agent,available,origin}`（`docs/CORE_PORTS_AND_STORAGE.md:165`）无非运行态字段，且实现 `host.rs:415-418` 的 `available` 只由 `command_resolves` + `resolve_credentials` 决定；③ 修正后的两条子句都有实现与用例（`open` 显式失败：`catalog.rs:511-518`；可用性不变：`catalog.rs:520-526`；按需重启新代：`catalog.rs:411-416`）；④ 另一处修正（`spec.md:184`）把 profile 场景改成可证伪，配了 `profiles_calls` 断言。**唯一需要补的**是同规范 `spec.md:9/18-19` 的「并给出原因」在本层没有落点（F8）——但那个子句的原句在本轮之前就是这样，且「原因」在 `create` 边界以 `Unavailable(KeystoreUnavailable)` 可观察并被用例断言，因此我不判 FAIL，只建议写实或登记 |

---

## ⑥ 最终结论

- 上一轮的 **2 个 MAJOR 都已闭合**：`sweep_idle` 现在在**同一临界区**（`host.rs:300-319`）移出 runtime 并清空映射，`open` 另有 `is_running` 短路（`496-500`），`ensure_runtime` 对已退出的 runtime 作废重建（`209-217`）；三条路径在顺序与锁序交错下都不再产出「被重新标记为活跃的死端点」。spec 的可用性子句按「与自身 Requirement 自洽」的方向修正，并配了实现、注释与用例，不构成掩盖。上一轮的 MINOR 中，`Q1-2`（漏给变量失败关闭）、`Q2-2`（`ACPR_`/`ACP_REMOTE_` 静态拒绝）、`Q2-3`（删除恒真断言）、`Q2-4`（手写 `Debug`）、`Q3-3`（注释口径）、`Q4-5`（时钟覆盖出站活动）与 `Q5` 的用例可证伪性/覆盖缺口（`open`、`shutdown_agent`、`spawn_idle_sweep`、回收、漏给变量、`ACPR_`）均已落地并实跑通过；`Q3-2`、`Q4-3` 残余、`Q4-4`、`Q4-6` 残余属**已登记**的非阻断项。
- 我实跑的四条必跑命令全部退出 0，`cargo test -p agent-host --all-features` 为 **45 passed / 0 failed**，并在 9 次重复运行（catalog ×5 + 整包 ×4）中无一次抖动；额外 `npm run check` 亦退出 0。
- 本轮新增 10 项发现（F1–F10），其中 MINOR 4 项（F1 顺序无用例、F3 `set_mode` 无法独立钉住、F9 标志检查在锁外、F10 已关闭会话阻塞回收，F10 先于本修复存在）、SUGGESTION 6 项；**没有任何一项构成阻断**。
- **该 WP 是否存在未解决阻断项：否。** 但「无阻断」不等于「可归档」：`Q4-3` 残余 / `Q4-4` 必须在接线 `app` 前收口，F1/F3/F7/F9 建议与之一并处理，且本轮结论不能替代 Project Verify（工作区 371 条、Linux 目标、E2E）。
---

## RV5 复核轮（独立 reviewer，新隔离只读上下文 + bash，2026-09-24）

复核对象：`e4a4492`（基线 `d0fa731`）。本轮由复核者**亲手**做反向探针（不接受实现者自述），并做六组全仓 grep 核实「同类实例是否扫干净」。WP1 侧的对应条目亦在本轮范围内（结论见本文件，WP1 报告只留指针）。

# RV5 独立复核报告 · `e4a4492`（基线 `d0fa731`）

**task_id**: `RV5-recheck`（3.2 / 3.10 的复核轮） · **role**: 独立 reviewer（新隔离只读上下文 + bash） · **phase**: 修复后复核（recheck）
**reviewer role 依据**: `openspec/schemas/agentic/roles/reviewer.md` 全文
**repository**: `D:/Project/acp-remote`（Windows x64，rustc 1.98.1，Node 24.19.0）
**base_revision**: `d0fa73162a029cabe64715939db807b89aca383c` · **target_revision**: `e4a4492`（`git diff --stat d0fa731..e4a4492` = 16 文件 / +816 −59）
**agent_context**: 全新子 Agent，未参与本批实现；只读文件工具 + bash（仅用于 `git` 只读、`grep`、`cargo`/`npm` 门禁）。**未做任何 git 写操作**；三处反向自检的改坏均以 `git checkout -- <file>` 还原。

---

## 0. 版本稳定性声明（先于结论）

- 复核对象固定为提交 `e4a4492`。`crates/**`、`docs/**`、`openspec/**`、`AGENTS.md`、`README.md` 在复核期间相对 HEAD **无我的改动**（`git diff --stat -- crates/` 为空）。
- **但工作区在我复核期间被另一个写入者改动**：开始时 `git status --porcelain` 只有 ` M .pi/settings.json`；复核进行中新增 ` M openspec/changes/acp-boundary-and-agent-host/reports/du1-integration.md` 与 ` M .../verification.md`（内容为主 Agent 的任务 5.1/5.2/6.5 记录，mtime `11:35:54`/`11:36:02`）。这不是我的改动。因此「结束时 `git status --porcelain` 与开始时一致」这一条**无法字面满足**，原因是并发写入，不是复核者越界。
- 为消除漂移影响，所有涉及 `verification.md` 的判据我都改为在 **`git show e4a4492:<path>`** 上复算（见 §2、§5）。

---

## 1. 反向自检（实测 4 条，超出要求的 3 条）

方法：改坏源码一个语义点 → 跑对应用例 → 必须变红 → `git checkout -- <file>` → 再跑必须变绿。每条都给出我实测的用例名与输出摘要。

| # | 改坏方式 | 文件 | 用例（实跑名） | 变红输出摘要 | 还原后 |
|---|---|---|---|---|---|
| 1 | 删掉 `Endpoint::set_mode` 的 `self.session.touch()`（`session.rs:770`） | `crates/agent-host/src/session.rs` | `catalog::idle_clock_is_refreshed_by_set_mode_without_declared_modes` | `FAILED`，exit 101：`panicked at catalog.rs:629: 刚调用过 set_mode 的会话不得在超时前被回收; left: None, right: Some(1)` | `1 passed; 0 failed`，exit 0 |
| 2 | 把期望集合校验块（`launch.rs:100–118`）整体移到 `NECESSARY_ENV` 注入（`120–127`）**之后** | `crates/agent-host/src/launch.rs` | `catalog::dropped_credential_bound_to_a_necessary_env_name_fails_before_spawn` | `FAILED`，exit 101：`panicked at catalog.rs:1076: 漏给绑定到 PATH 的凭据必须失败关闭（宿主机 PATH 不得冒充凭据）` | `1 passed; 0 failed`，exit 0 |
| 3 | 把 stderr 内容**接进协议处理**：`stderr_loop` 增加 `pending/incoming/exit/tree` 参数，用与 `read_loop` 同构的**持久缓冲 + LF 分帧**调 `handle_line` | `crates/agent-host/src/process.rs` | `session::stderr_protocol_messages_never_reach_the_endpoint` | `FAILED`，exit 101：`panicked at session.rs:804: stderr 内容不得进入事件视图：{"deltaIndex":"0","messageId":"…0002","text":"ACPR-STDERR-PROTOCOL-NOISE-MARKER"}` | `1 passed; 0 failed`，exit 0 |
| 4 | 删掉 `AgentRuntime::is_idle` 的 `filter(|session| !session.is_closed())` → `.filter(|_session| true)` | `crates/agent-host/src/host.rs` | `catalog::closed_session_does_not_block_idle_reclaim` | `FAILED`，exit 101：`panicked at catalog.rs:764: 已关闭的会话不得阻塞空闲回收（否则进程泄漏）; left: Some(1), right: None` | `1 passed; 0 failed`，exit 0 |

**关于第 3 条的额外发现（可证伪性边界）**：先用「按 `read` 分块切分、不做跨块持久缓冲」的粗糙 tee 试了一次，用例**仍绿**——因为 stderr 的读写边界会把一条 JSON 拆成 `{` + 其余两段（我用 `eprintln!` 探针实测到 3 段：`{`、`"jsonrpc":…}`、`{"id":987654321,…}`），两段都解析失败而被忽略。改用与 `read_loop` **同构的持久缓冲分帧**后立即变红。结论：该用例**能**证伪「stderr 以正确分帧被接进协议处理」这一现实失误形态，但**不能**证伪「stderr 以错误分帧被接进协议处理」——后者本身也不构成「stderr 报文进入端点视图」，因此不算覆盖缺口，仅作记录。

---

## 2. 逐条核实本批修复是否真实落地（`git diff d0fa731..e4a4492` 逐文件）

| 期望修复 | 结论 | 证据（target 位置 / 用例名 / 实跑） |
|---|---|---|
| 期望集合校验顺序用例 | **PASS** | `crates/agent-host/src/launch.rs:100–118` 的期望集合块位于 `116–123`（现 `120–127`）的 `NECESSARY_ENV` 注入**之前**，注释显式声明 load-bearing；新增用例 `catalog.rs:1058 dropped_credential_bound_to_a_necessary_env_name_fails_before_spawn`（`profile_with_env_vars(..., &["PATH"])` + `FakeCredentials::dropping("PATH")`，断言 `Unavailable` + `!dump.exists()`）。**我以变异测试独立确认该顺序被钉住**（§1 第 2 条） |
| `set_mode` 单点 `touch` 钉住 | **PASS** | 新增 `catalog.rs:602 idle_clock_is_refreshed_by_set_mode_without_declared_modes`，用 `--no-modes` 造出「未宣告模式 → 不发任何消息」的早退路径，使窗口内只有 `set_mode` 一个刷新点；并把 `modes()` 挪到窗口之前。**变异实测红**（§1 第 1 条） |
| `set_config` 单点 `touch` 钉住 | **PASS** | 新增 `catalog.rs:658 idle_clock_is_refreshed_by_set_config`（fake child 对 `session/set_config_option` 只回空 result、不发通知）；`session.rs:813` 的 `touch()` 是真刷新点 |
| `cancel_turn` 单点 `touch` 钉住 | **PASS** | 新增 `catalog.rs:703 idle_clock_is_refreshed_by_cancel_turn`（无 turn 时 `cancel` 幂等空操作、不发消息）。注：`Endpoint::cancel`（`session.rs:762`）经 `AcpSession::cancel_turn`（`session.rs:458–462`，`touch()` 在 `462`）——本用例钉的是该路径 |
| `shutting_down` 锁内复查 | **PASS（防线，未钉住）** | `host.rs:208`（取锁前）与 `host.rs:215`（取锁后）两处 `SeqCst` 复查；`shutdown_all` 先置位（`host.rs:~342`）再取锁。**诚实说明**：该复查只覆盖纳秒级窗口，无任何用例能钉住它（删掉 `215` 全套用例仍绿）——属纯纵深防御，不是可证伪断言 |
| 已关闭会话不阻塞回收 | **PASS** | `host.rs:86–97` 改为 `filter(|session| !session.is_closed())`，全部关闭后退回进程级时钟；`session.rs:156–159` 的注释写明不变量；新增 `catalog.rs:745 closed_session_does_not_block_idle_reclaim`（含心跳进程外证据）。**变异实测红/绿**（§1 第 4 条） |
| `LaunchSpec::Debug` 不打印 `args` | **PASS** | `launch.rs:33–49` 改为 `program` + `arg_count` + `arg_lengths` + `env_names` + `env_count`；单元用例改名 `debug_never_prints_credential_values_or_argument_bodies`，注入 `sk-secret-argument-do-not-log` 并断言不出现在 `{spec:?}`，且断言 `arg_count`/`arg_lengths` 可见。实跑：lib 单元 `3 passed` |
| stale runtime 重建用例 | **PASS** | `catalog.rs:786 exited_runtime_is_rebuilt_as_a_new_generation`：`crash-on-prompt` 造出「已退出但仍在目录」窗口 → 删 `--dump-env` 快照 → 再次 `agent_capabilities` → 断言 `generation==Some(2)` + **`dump.exists()`**（进程外证据）→ 旧端点必须 `Err(InvalidRequest)`（证明确实先 `close_session`）+ 旧映射不可 `open` |
| 关闭后进程外证据（`--dump-env`） | **PASS** | `catalog.rs:965 idle_sweep_task_converges_on_shutdown_all` 新增两处：窗口内 `assert!(dump.exists())` + 删快照 + 关闭后 `assert!(!dump.exists())`。这补上了 RV4-WP5-F7 |
| `stderr-protocol-noise` 场景 | **PASS** | `src/bin/acpr-fake-acp-agent.rs:21` 的 `STDERR_NOISE_MARKER`、`:615–646` 的 `stderr-protocol-noise` 分支（stderr 上写**语法合法**的 `session/update` 通知 + 带 id 响应，stdout 仍正常应答）；bin 头注释与 `tasks.md` 2.13 的场景清单同步 |
| 「stderr 上的合法 ACP 报文永不进入端点视图」用例 | **PASS** | `tests/session.rs:776 stderr_protocol_messages_never_reach_the_endpoint`：① 请求成功 + `turn.completed`；② 逐事件断言 marker 不在 `payload.view` 也不在 `AcpRaw` 原文；③ 后续 `modes()` 可用 + `runtime_running`。**变异实测红**（§1 第 3 条）——这正是 RV4-WP1-F2（此前「未修复且未登记」的静默缺口）的闭合 |

**小结**：本批 11 项声称的修复**全部真实落地**，且其中 5 项（期望集合顺序、`set_mode`/`set_config`/`cancel_turn` 单点、已关闭会话、stderr 报文不入端点）经我亲手变异验证为**可证伪断言**，不是靠实现者自评。

---

## 3. 实跑命令与计数（我亲自执行，退出码实测）

| 命令 | 退出码 | 输出摘要 |
|---|---|---|
| `cargo fmt --all -- --check` | **0** | 无输出 |
| `cargo clippy --locked -p agent-host --all-targets --all-features -- -D warnings` | **0** | 首跑缓存命中 `Finished … in 0.10s`；`cargo clean -p agent-host`（Removed 2556 files, 949.9MiB）后复跑打印 `Checking agent-host v0.0.0 (D:\Project\acp-remote\crates\agent-host)` → `Finished`，零告警 |
| `cargo test --locked -p agent-host --all-features` | **0** | 逐目标：lib 单元 **3**、`bin/acpr-fake-acp-agent` 0、`tests/catalog.rs` **21**、`tests/session.rs` **13**、`tests/supervision.rs` **15**、doc-tests 0 = **52 passed / 0 failed / 0 ignored**。逐行 `test result:` 六条全 `ok`，无 `FAILED`/`failures:`；重复跑两次计数一致 |
| `npm run check` | **0** | **十道门禁全绿**：`check:schemas`（117 valid, 23 invalid, 39 event views）· `check:commands`（12 commands）· `check:errors`（58 codes）· `check:features`（11 feature ids）· `check:assets`（17 schemas, 155 fixture files, 12 transcript vectors, 20 negative vectors, 2 SAS）· `check:acp`（25 methods, 11 updates, 5 content blocks, 3 tool content types, 19 capabilities, 8 invariants, 10 test families / 71 rows）· `check:docs`（367 relative links, 2811 section refs, 143 files）· `check:boundaries`（8 个 crate 与 §5 矩阵一致）· `check:drift`（§7 36 条 DDL + §5 15 trait/87 方法签名一致）· `check:agentic`（PASS×8 + `Totals: 6 passed, 0 failed`） |

**与 `verification.md` 声称的对账**：`e4a4492` 的 PV4/W2 行写「52 个测试全通过（supervision 15、session 13、catalog 21、单元 3；含 RV3/RV4 两批修复新增的 17 个）」（`git show e4a4492:…/verification.md:46`）——**与我的实测逐项一致**；本批新增用例数也对得上（catalog 15→21 = +6、session 12→13 = +1，加上 `d0fa731` 的 +10 = 17）。

**未执行（不得当作已验证）**：`cargo test --locked --workspace --all-features`（`verification.md` 的 378 条未由本轮复算）；`cargo check --target x86_64-unknown-linux-gnu`；任何 E2E。`cargo-deny`/`gitleaks` 本地无等价物。

---

## 4. 同类实例是否扫干净（全仓 grep，范围 `docs/**`、`AGENTS.md`、`README.md`、`openspec/changes/acp-boundary-and-agent-host/**`、`crates/**`）

### 4.1 「stderr 有界脱敏」→ **已扫干净**
- 权威口径四处同值：`SECURITY_DESIGN.md:376`（改为「内容原样进入…（不做内容脱敏），**内容不进日志**」）、`SECURITY_DESIGN.md:366`、`MODULE_ARCHITECTURE.md:282`、`INITIAL_DESIGN.md:139`（改为「只做有界采集与结构化计数（内容不进日志）」）、`README.md:18`。
- `grep "有界脱敏"` 全仓**唯一命中**在 `reports/rv1-wp1.md:167`，即 RV4 自己对该缺陷的引文记录 → **合理保留**。
- `grep "脱敏"` 其余命中（`adr/0008:61` gitleaks `--redact`、`CONFIG_REFERENCE.md:234` 日志格式、`SECURITY_DESIGN.md:55/121/437/546`、`AGENTS.md:192`、`design.md:45/128` D9）均与 stderr 采集无关 → **合理保留**。
- 观察（非本次判据）：`SECURITY_DESIGN.md:366` 的括注「（变量名与数量级）」是从 env 口径串来的残留短语，读起来与 stderr 不搭；不构成错误，列为 SUGGESTION。

### 4.2 「`§13.3`」→ **已扫干净（就本变更的取舍指针而言）**
- 在 `e4a4492` 上：`cat` 级逐文件核对，`verification.md:86` 现为 `§14.1`（RV4-F4 闭合）；`verification.md` 内仅剩 `:166`（RV3 历史记录「§13.3 → §14.1」，正反两处都点名 → **合理保留**）。
- 其余 §13.3 命中都是**别的小节本身**或别的文档的 §13.3：`INITIAL_DESIGN.md:581`（§13.3 CLI）、`LOCAL_ADMIN_PROTOCOL.md:367`（NODE_LINK §13.3 状态确认）、`MODULE_ARCHITECTURE.md:382`（INITIAL_DESIGN §13.3）、`NODE_LINK_PROTOCOL.md:762`（自身 §13.3）、`SECURITY_DESIGN.md:413`（§13.3 保留/压缩/删除，即被误引的那节）、`crates/node-link-protocol/src/pairing.rs:9`（NODE_LINK §8.2/§13.3）→ **全部合理保留**。

### 4.3 「`TerminateJobObject` 作为实际结束手段」→ **已扫干净**
- 实现侧的表述统一为「结束手段是关闭/丢弃 Job 句柄（`KILL_ON_JOB_CLOSE`），wrapper 未封装 `TerminateJobObject`」：`MODULE_ARCHITECTURE.md:281`、`design.md:46`、`design.md:99`、`tasks.md:26`、`crates/agent-host/src/platform.rs:7` → **全部合规**。
- 剩余命中 `INITIAL_DESIGN.md:704`：那是 2026-09-18 **一次性探针**（仓库外、直接 FFI）的三种场景记录之一，紧跟其后的结论明确写「必须用 Job Object + 把 `KILL_ON_JOB_CLOSE` 设在守护进程持有的 Job 上」→ **合理保留（历史探针记录，非实现手段声明）**。

### 4.4 「`platform/windows` / `platform/unix` 幻影路径」→ **未扫干净（残留）**
| 命中 | 判断 |
|---|---|
| `openspec/changes/acp-boundary-and-agent-host/plan.md:534` | **残留未修**。该「文件单一写入者」条仍写 `crates/agent-host/src/{lib,error,limits,supervisor}.rs` 与 `platform/**`（WP3）、`src/{session,mapper,interaction}.rs`（WP4）、`src/{catalog,config,credentials}.rs`（WP5）。实际文件是 `process.rs`/`platform.rs`/`session.rs`/`mapper.rs`/`config.rs`/`launch.rs`；`supervisor.rs`/`interaction.rs`/`credentials.rs` 不存在，`catalog.rs` 只是测试文件。**这是 plan.md 内第二份所有权清单**——同一批修复只实名化了 WP 表（`:509–511`）与写范围说明（`:513`），把 `:534` 整个漏掉 |
| `openspec/changes/acp-boundary-and-agent-host/verification.md:54` | **残留未修（低）**。Check Plan Change 1 的「受影响任务：2.17（`platform/unix`）」仍在用幻影模块路径；应为 `platform.rs` 的 unix 分支 |
| `openspec/changes/acp-boundary-and-agent-host/design.md:98` | **边界情况**。`platform::windows`/`platform::unix` 是设计层对内部抽象的概念命名（实现是 `platform.rs` 内 `#[cfg] mod inner`），不是文件路径；但同一句「crate 其余部分不出现 `cfg`」是后文 4.5 的同类绝对判据（见下） |
| `reports/rv1-wp1.md:*` | **合理保留**（RV4 报告对缺陷的引文） |

### 4.5 「`cfg` 只落 `platform`/`bin`」→ **未完全扫干净（残留）**
- **已修**：`tasks.md:45`（3.6 检视判据，现写「**平台分支** `cfg` 是否只落在 `platform.rs`」）、`plan.md:601`（关注点⑦）、`tasks.md:26`（2.17 完成条件）、`crates/agent-host/src/lib.rs:21`（显式说明 `launch.rs` 1 处 `#[cfg(test)]`、`bin/` 零命中）→ 这一组是 RV4-F5 点名的实例，**确已闭合**。
- **残留未修**：`openspec/changes/acp-boundary-and-agent-host/design.md:77`「`platform` | …；**只有这里出现 `cfg`**」与 `:98`「crate 其余部分不出现 `cfg`」。两处都是**绝对判据**，与实测冲突（`launch.rs:98` 有 `#[cfg(test)]`；`bin/` 零命中；平台分支确在 `platform.rs`）。与 RV3/RV4 反复判 FAIL 的正是同一类措辞。design.md 属本变更目录、在 grep 范围内 → 判为**残留未修（低，MINOR/说明性）**。
- 其余 `cfg` 命中（`design.md:15/137`、`plan.md:595`、`proposal.md:74`、`verification.md:36`、`docs/MODULE_ARCHITECTURE.md:123/327/407/618`、`AGENTS.md:121`、`IDENTITY_AND_AUTH_CONTRACT.md:24`）都是 `#[cfg(unix)]` 平台路径或 `identity-*` 的独立约束 → **合理保留**。

### 4.6 「`tracing`/`nix` 许可证」→ **已扫干净（且我做了第三方来源核验）**
- 我**不听文档**，直接从本地 registry 的 `cargo metadata --locked` 读 `license` 字段：`tracing 0.1.44 = "MIT"`、`nix 0.30.1 = "MIT"`、`win32job 2.0.3 = "MIT OR Apache-2.0"`。
- 文档对账：`design.md:159`（已从「`tracing`…MIT/Apache-2.0」改为「`tracing 0.1` 与 `nix 0.30.1` 为 `MIT`、`win32job 2.0.3` 为 `MIT OR Apache-2.0`」）✓；`MODULE_ARCHITECTURE.md:135`（tracing MIT / win32job MIT OR Apache-2.0 / nix MIT）✓；`MODULE_ARCHITECTURE.md:281`（win32job MIT OR Apache-2.0、nix 只提供 MIT）✓；`design.md:46/47` ✓。**无残留错误陈述**。
- 唯一剩者为 `reports/rv1-wp1.md:162` 的 RV4-F1 记录 → **合理保留**。

---

## 5. 对「登记为未处理」条目的可接受性判断（我的独立判断，不默认接受登记里的理由）

| 条目 | 登记里的理由 | 我的判断 | 我的理由 |
|---|---|---|---|
| **RV4-WP5-F2** 期望集合只查 ⊇ | 「非安全越界（仍在白名单内），fake 造不出反例」 | **可接受** | 逐字读 `launch.rs:76–94`：白名单是硬上界（越界即 `EnvNotAllowed`），端口返回「白名单内但未绑定」的额外名会被注入，**但仍在白名单内**，即未越过声明边界（`spec.md:165–169`「白名单是注入上限」）。真正的安全判据（不得注入白名单外/保留前缀）有 `EnvNotAllowed` 与 `validate_env_name` 两道独立断言。属「与端口「恰为交集」措辞的偏差」，非越界。**不改判** |
| **RV4-WP5-F8** spec「并给出原因」无落点 | 「`AgentDescriptor` 无 `reason`；`create`/`agent_capabilities` 的 `Unavailable(KeystoreUnavailable)` 是可观察原因」 | **可接受，但必须与端口形状对齐后收口（不宜长期挂）** | 我独立核对：`crates/core/src/model/backend.rs:18–22` 的 `AgentDescriptor{agent,available,origin}` **确无** `reason`，且该形状是 `docs/CORE_PORTS_AND_STORAGE.md` §3.6 的既有冻结契约，本变更明示不改 core。⇒ `spec.md:9` 与 `:18–19` 的「并给出原因」在本层**字面不可实现**。它不阻断，因为：① 目录的 `available` 是真实观测且有用例；② 原因以错误类别在 `agent_capabilities`/`create` 边界可观察。但这是**本变更自己的 delta spec 里的悬空子句**，登记只写「措辞待与端口形状对齐」而无落点。我判：非阻断，但应在最终验收前改写该子句或以「原因 = 错误类别」写实，不能只登记 |
| **RV3-Q4-3** 残余交错 | 「现实不可达（core broker 缓存端点），留待接线 `app` 前收口」 | **部分不再成立 → 剩余部分可接受** | 我独立核验两点：① 本批**已闭合**其中一支——`host.rs:215` 的锁内复查把「标志检查在锁外」的纳秒窗口关掉了；② RV4 的「现实不可达」论据我独立复核为真：`crates/core/src/broker.rs:2091` 的 `endpoint()` **先** `lock(&slot.endpoint).clone()`，只在空槽才调 `backends.open(...)`。⇒ 顺行调用下不会反复触发 `open`。剩下的是 `create`/`open` 在 `ensure_runtime` 之后不复查运行状态、以及 `shutdown_agent` 先 `remove` 后 shutdown 的短暂双树。**二者都只产出显式失败或「正在被结束的旧树」**，且 `open` 另有 `is_running` 短路。**可接受**，但我要求登记文字把「已含 `host.rs:215` 锁内复查」写进去，否则下一轮会把已闭合的一半当成未闭合 |
| **RV3-Q4-4** 跨 `await` 持锁 | 「只损活性/吞吐，被 10s/5s 双重界定」 | **可接受（但把它当「接线前必修」看待）** | `ensure_runtime` 的 `runtimes` guard 覆盖 `resolve_launch`（含 keystore IO）+ `Supervisor::start` + `initialize`（上限 `STARTUP_TIMEOUT`=10s）。**全局单锁**意味着一个慢 Agent 的启动会阻塞 `sweep_idle`、`shutdown_agent`、`shutdown_all` 与其它 Agent 的 `create`。10s 恰等于 `daemon.shutdown_grace_ms`，`shutdown_all` 有可能被拖到边界。功能正确性无破坏（我已看到测试用 `wait_for_generation`/重复采样适配，这是**必要适配而非掩盖**）。判非阻断，但登记若把它降级为「吞吐优化」则我不接受——它是**关闭时延与 daemon grace 的耦合风险**，接线 `app` 前必修 |
| **RV3-Q3-2** `UnknownProfile` 无用例 | 「4 行、可测而未测」 | **可接受但偏低优先，且不构成阻断** | 我核查：spec 内**没有任何场景**要求「未登记 profile」的行为（`grep 未登记/UnknownProfile` 在 `specs/local-agent-host/spec.md` 零命中），因此这不是 spec 覆盖缺口；分支在 `launch.rs:67` → `error.rs:116` 映射为 `InvalidRequest`，与 core 的「未登记 → InvalidRequest」同族。**非阻断**。但代价确实只有 4 行，我不接受把它拖过最终验收 |
| **`runtime_running` 的 `try_lock` 局限** | 「已写进代码注释 + 关键结论已迁到心跳文件」 | **可接受，且本批处置合格** | `host.rs:590–606` 的 `try_read_runtimes` 有界重试并**如实声明**「`None` 与『没有该条目』不可区分；进程外证据请用心跳文件」——把不确定写进代码而不是继续假装它是证据，这是正确做法。本批还把 `idle_sweep_task_converges_on_shutdown_all` 的「关闭后不得再起进程」落到 `--dump-env` 快照（RV4-WP5-F7 闭合）。剩余仅「其它用例仍用该诊断函数」，非阻断 |

---

## 6. 未解决阻断项清单

**CRITICAL / MAJOR：无。**

未解决的非阻断项（全部 MINOR 级，且都不影响产品行为/协议/安全/用例有效性）：

1. **`plan.md:534`** 幻影文件清单（`supervisor.rs`/`interaction.rs`/`catalog.rs`/`credentials.rs`/`platform/**`）——同一批实名化漏掉的第二处实例。
2. **`verification.md:54`** 幻影模块路径 `platform/unix`。
3. **`design.md:77` / `:98`** 绝对 `cfg` 判据（「只有这里出现 `cfg`」/「crate 其余部分不出现 `cfg`」），与 `launch.rs:98` 的 `#[cfg(test)]` 冲突——与 RV4-F5 同类、未被点名因而未被扫。
4. **`spec.md:9` / `:18–19`** 「并给出原因」在本层无落点（RV4-WP5-F8 已登记；我判应写实而非长期挂）。
5. `verification.md:179` 的登记叙述「已把 … 命名写进本文件与 plan」——`本文件` 已为真（我以 `git show e4a4492:` 复核），`plan` 只对 WP 表为真、对 `:534` 不成立 → **叙述仍部分不实**（程度轻于 RV4-F3 当时的程度）。
6. `SECURITY_DESIGN.md:366` 括注「（变量名与数量级）」串味（SUGGESTION）。

**可证伪依据**：①②③⑤ 都是**可 grep、可逐字核对的静态事实**（`grep -n "platform/unix\|supervisor\.rs\|cfg"` + `git show e4a4492:<path>`），不依赖任何执行；④ 由 `crates/core/src/model/backend.rs:18–22` 的字段清单反证。三处命令均 exit 0（grep 命中即输出、无命中的返回 1，不构成门禁失败——这些项**没有任何门禁会抓**：`check:docs` 只统计小节引用不断言语义，`check:contract-drift`/`check:boundaries` 不读 plan/design 措辞）。

---

## 7. 与本轮结论相关的检查证据状态

| 检查 | 我核对到的证据 | 差异 / 待返回 |
|---|---|---|
| 3.5 / PV4（WP3） | `verification.md:46` 声称 52 个测试、10 道门禁 | **与我的实测逐项一致**（§3）；**无差异** |
| 3.7 / PV4（WP4） | 声称 session 13 | **一致**（我实跑 13） |
| 3.9 / PV4（WP5） | 声称 catalog 21 | **一致**（我实跑 21） |
| 3.5 / PV5（进程树） | `reports/pv5-windows-tree.log`，`2 passed, 0 failed, 13 filtered out` | 未由我复跑（不在本轮任务）；`13 filtered out` 与 supervision 15 自洽 |
| DU1-PV1 | 声称 `npm run verify` 378 条 workspace 测试 | **未由本轮复算**；我实跑的是 `npm run check`（10 门禁，exit 0）与 `-p agent-host`（52）。**不影响本轮代码判断**，应在 6.x/最终验收前由主 Agent 补齐 |
| 反向探针 | `RV3-2` 行自述「反向自检」 | 我**没有接受其自述**，改为亲手做 4 条（§1）；3 条点名项全部复现为红/绿 |

---

## 8. Report（固定字段）

| 字段 | 值 |
| --- | --- |
| task_id | `RV5-recheck`（3.2 / 3.10 的复核轮） |
| role / phase | 独立 reviewer（新隔离只读上下文 + bash）/ 修复后复核（recheck） |
| agent_context | 全新子 Agent，未参与实现与本批修复；只读工具 + bash（git 只读、grep、cargo/npm 门禁） |
| target_revision | `e4a4492`（基线 `d0fa731`） |
| scope | 本批 16 文件 diff 的全部：`agent-host` 的 `launch.rs`/`host.rs`/`session.rs`/`bin/acpr-fake-acp-agent.rs`、`tests/{catalog,session}.rs`、`docs/MODULE_ARCHITECTURE.md`/`docs/SECURITY_DESIGN.md`、变更目录的 `design/plan/proposal/tasks/verification/specs` |
| checks | §3 四条命令（全 exit 0）；§4 六组全仓 grep；§1 四条变异自检 |
| issues | 6 项非阻断（§6），**无 CRITICAL/MAJOR** |
| result | **无未解决阻断项**（就本批修复的声称而言）；但「同类实例扫干净」**不成立**（§4.4/4.5） |
| evidence_paths | 本报告；我实跑的终端摘要（§1/§3）；`crates/core/src/broker.rs:2091`、`crates/core/src/model/backend.rs:18–22`、`launch.rs:100–127`、`host.rs:86–97/208/215`、`session.rs:458–462/770/813`、`catalog.rs:602/658/703/745/786/965/1058`、`session.rs(session endpoint)776` |
| resource_cleanup | 无遗留：`git diff -- crates/` 为空、无 untracked 文件；临时脚本写在 `%TEMP%`；三处改坏已 `git checkout --` 还原 |

### 最终结论

**该变更在本批修复范围（`e4a4492`）内是否存在未解决阻断项：否。**

- 本批 11 项声称的修复**逐条真实落地**，其中 5 项经我亲手变异验证为可证伪（含 RV4 曾判「未修复且未登记」的 stderr 报文不入端点这一静默缺口）。
- 实跑四条命令全 exit 0：`fmt` 0、`clippy` 0（含冷编译复跑）、`agent-host` 测试 **52 passed / 0 failed**（catalog 21 / session 13 / supervision 15 / 单元 3）、`npm run check` **十道门禁**全绿。计数与 `verification.md` 声称**逐项一致**。
- **但「同类实例扫干净」不成立**：§4.4 的 `plan.md:534` 与 §4.5 的 `design.md:77/98` 是**只改点名实例**留下的同类残留；§4.6 的许可证事实我已用第三方 registry 独立核验为已扫干净。
- 「无阻断」**不等于「可归档」**：§6 的第 1/4/5 项与 §5 的 `Q4-4`、`Q4-3` 残余应在接线 `app` / 最终验收前收口；本报告不代替 Project Verify（378 条 workspace、Linux 目标、E2E），也不代替主 Agent 更新任务状态。

**残留风险**：① 工作区在本轮非由我改动（`verification.md`、`du1-integration.md`），任何以当前工作区而非固定提交引用的 `verification.md` 行号都可能漂移；② 未复算 workspace 级测试与 Linux 目标编译；③ `p pv5-windows-tree.log` 未复跑。