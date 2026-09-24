# 独立检视报告 · WP5（配置、凭据、目录与空闲回收）

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