# WP7 Handoff — app 组合根接线 + 受控路径全链路集成测试（`node-link-owner` / DU1）

## Shared Report

- **task_id**：2.21（app 组合根接线，D11）、2.22（受控路径全链路集成测试，[PV5] 主体）。
- **role / phase**：coder / implement。
- **agent_context**：worker 子 Agent（本机 worktree 独占，未继承父会话实现对话）。工作目录
  `D:\Project\acp-remote-wt\node-link-owner`（分支 `agentic/node-link-owner`）；规划根 `D:\Project\acp-remote`。
  本轮为**恢复轮次**：上一轮在 30 分钟宿主上限处被终止（非代码问题），工作区改动完整保留；本轮从
  「盘点 → 修完 [PV5] 测试 → 分批提交 → 全量门禁 → 报告」继续。
- **target_revision**：`a3109c8a80ec806cd7899b72709ec07e579cec28`（HEAD，含 WP7 全部提交）；起点
  `1efd69ff886e72a932bf897e44003273217d670c`（WP6 之后）。两批按依赖顺序：
  - `07b956c` **feat(app)**：2.21 组合根接线（6 文件：`crates/app/src/{config,compose,daemon}.rs`、
    `crates/app/Cargo.toml`、`Cargo.lock`、`crates/app/tests/daemon_lifecycle.rs`）；
  - `a3109c8` **test(app)**：2.22 集成测试与测试支撑（5 文件：`crates/app/tests/{node_link_e2e.rs,
    node_link_listener.rs}`、`crates/app/tests/support/{mod,nodelink,owner}.rs`）。
  - **提交边界为何是这样**：`07b956c` 只含「生产接线 + 既有用例的口径更新」，它单独可编译可测（接线
    不依赖任何新测试文件）；`a3109c8` 是纯新增测试与支撑，靠已接线的组合根跑。开发依赖
    （`rcgen`/`rustls`/`tokio-rustls`/`rustls-pki-types`/`node-link-protocol`/`identity-auth`/`p256`/
    `base64`）随 `07b956c` 一起进 `Cargo.toml`/`Cargo.lock`（都是 workspace 已有的包，`Cargo.lock`
    只增加 app 的依赖边 7 行），因此每批都能用 `--locked` 构建。
- **scope**：
  - 新增：`crates/app/tests/node_link_listener.rs`（[PV4]，8 用例）、`crates/app/tests/node_link_e2e.rs`
    （[PV5]，2 用例）、`crates/app/tests/support/nodelink.rs`（wire 编解码客户端 + 配对/握手辅助）、
    `crates/app/tests/support/owner.rs`（真实 SQLite + 真实 `Authority` + 真实 `LocalAdminRouter` 的
    Owner 侧装配替身 + 可注入 commit 故障的 `SessionStore` 装饰器 + 脚本化会话端点）；
  - 修改：`crates/app/src/config.rs`（四个 `daemon.*` 键 + `dev_mode.allow_plaintext` 从 unwired 移除并
    接线、`resolve_tls` 失败关闭）、`crates/app/src/daemon.rs`（启动/关闭序列、`net_config`、
    `NodeLinkIngress`/`register_node_link_paths`/`NetIngress`、`NodeLinkCloser`、`status.listen`）、
    `crates/app/src/compose.rs`（`RevocationCloser` → 真实的 `NodeLinkCloser`、`ForkedPublisher` 事件扇出、
    `assemble(config, publisher)`）、`crates/app/tests/support/mod.rs`（配置构造器支持 `listen`/`[daemon.tls]`/
    `[dev_mode]` 注入）、`crates/app/tests/daemon_lifecycle.rs`（`daemon.status.listen` 断言从「空数组」改为
    「一个 loopback 地址且 TCP 可达」）、`crates/app/Cargo.toml` + `Cargo.lock`（开发依赖）；
  - **未改**：`openspec/**`、`schemas/**`、`compatibility/**`、`fixtures/**`、`docs/**`、
    `crates/server/**`、`crates/core/**`、`crates/storage-sqlite/**`、其它 `crates/app/src/**` 模块。
    `docs/**` 的「现状/未接线」注记收敛属 2.23（WP8），本 WP 不代做。
- **changes**：见「改动与需求映射（R1–R4、R12–R15）」与「[PV5] 全链路覆盖映射」。
- **checks**：
  - `[PV4]`（WP7 轮次，原始输出 `reports/wp7-app-wiring.log`）：`cargo test --locked -p app --all-features
    --test node_link_listener -- --test-threads=1` → **8 passed / 0 failed**（真实二进制
    `CARGO_BIN_EXE_acp-remote` 起停，含绑定、失败关闭、TLS 终止与非 loopback 告警）；
  - `[PV5]`（WP7 本机轮次，原始输出 `reports/wp7-integration.log` 与追加进
    `reports/pv5-windows-nodelink.log`）：`cargo test --locked -p app --all-features --test node_link_e2e
    -- --test-threads=1` → **2 passed / 0 failed**（3.2 s / 5.3 s，0 ignored，无全跳过）；
  - 全量 app：`cargo test --locked -p app --all-features` → lib 52、`audit_export` 1、`cli_commands` 11、
    `daemon_lifecycle` 10、`node_link_e2e` 2、`node_link_listener` 8，**84 passed / 0 failed / 0 ignored**；
  - `cargo fmt --all -- --check` 退 0；`cargo clippy --locked --workspace --all-targets --all-features --
    -D warnings` 退 0（两批提交的 pre-commit 钩子各跑一次）；
  - 补充证据（非 WP7 计划内 ID，供 3.13/3.15 参考）：`cargo test --locked --workspace --all-features`
    退 0，85 个测试目标 0 failed（2 处 `1 ignored` 是 `storage-sqlite` 既有的 `#[ignore]` 子进程探针与
    夹具生成器）；原始输出 `reports/wp7-workspace.log`；
  - `npm run check` 退 0（17 个合同/文档/词表门禁 + agentic 宿主入口与规格检查；
    两批提交的 pre-commit 各跑一次，本轮另跑一次确认）；
  - `rg -n "unsafe" crates/app/src` **0 命中**（workspace 级 `unsafe_code = "forbid"` 由 clippy 兜底）。
- **issues**：无阻断项。**11 条**需要主 Agent / reviewer 知晓的事实与已知残余列在「需要主 Agent /
  reviewer 知晓的偏差与残余」；其中 3 条是任务书要求登记的既有残余，另 8 条是本 WP 的偏差登记
  （含 1 条本轮新发现、影响 `[PV5]` 断言口径的事实）。
- **result**：**PASS**（本工作包检查全部满足）。**不代表**独立 review（3.14）、`[PV1]`/`[PV2]`/`[PV3]`
  的其它 WP、或合并已完成。
- **evidence_paths**：`reports/wp7-app-wiring.log`、`reports/wp7-integration.log`、
  `reports/pv5-windows-nodelink.log`（WP7 轮次追加）、`reports/wp7-workspace.log`（补充）、本文件。
- **resource_cleanup**：用例全部在 `127.0.0.1:0` 或显式占用的随机回环端口上跑，端口随进程/监听器释放；
  临时数据目录与自签证书目录在 `TempRoot`/`TestCertificate` 的 `Drop` 里删除；`node_link_e2e` 在结尾
  调 `OwnerNode::stop()`（触发 `Shutdown`、逐任务 join 后释放 SQLite 句柄，SQLite 仍有其它持有者即 panic）；
  `node_link_listener` 用真实二进制并以 `SIGINT`/`daemon.stop` 走完整关闭序列，子进程随用例结束回收；
  无容器、无数据库服务、无外部网络、无 detached task；`target/` 作为缓存保留（被 git 忽略）。

## 改动与需求映射（R1–R4、R12–R15）

| 需求 | 落点（生产代码） | [PV4] 用例 |
| --- | --- | --- |
| R1 监听地址可配置且实际绑定 | `config.rs` 的 `listen`/`daemon` 段消费 + `daemon.rs::net_config` → `NetListener::bind` | `the_configured_listen_address_is_bound_and_reported` |
| R2 `daemon.status.listen` 回实际地址 | `daemon.rs::AppDaemonControl::status`（`listen` 取 `NetIngress::listen()`；`links` 恒空） | 同上（含 `daemon_lifecycle` 的口径更新） |
| R3 绑定失败/非法字面量必须拒绝启动 | `daemon.rs` 的 `NetIngress::start`（bind 失败 → `DaemonError::Network` 退出）+ `NetError::InvalidListen` | `an_occupied_listen_address_refuses_startup`、`an_invalid_listen_literal_refuses_startup` |
| R4 非 loopback 的告警 | `daemon.rs` 的 `net.listen_non_loopback`/`net.plaintext_beyond_loopback`/`daemon.listener_warning` | `proxy_mode_off_loopback_starts_with_a_warning`、`plaintext_dev_mode_is_refused_off_loopback` |
| R12 TLS 终止点在节点主机内 | `config.rs::resolve_tls` → `TlsMode::Direct` → `NetListener` | `direct_tls_terminates_tls_and_refuses_plaintext`、`tls_direct_terminates_the_same_handshake`（[PV5]） |
| R13 明文只在显式开发模式、且仅 loopback | `dev_mode.allow_plaintext` 接线（`NetError::PlaintextDevNonLoopback`） | `plaintext_dev_mode_is_refused_off_loopback` |
| R14 PEM 不可读即失败关闭 | `config.rs::resolve_tls`（缺 PEM 拒绝配置）+ `NetError::TlsFileUnreadable`（启动期读取） | `direct_tls_without_readable_pem_refuses_startup` |
| R15 TLS 与明文不混跑 | `TlsMode::Direct` 时 listener 只做 TLS；proxy 模式不允许给 PEM | `direct_tls_terminates_tls_and_refuses_plaintext`、`contradictory_tls_configuration_is_rejected`（config 单测） |

## [PV5] 全链路覆盖映射

`the_controlled_path_runs_end_to_end_and_revocation_propagates`（`a3109c8`）按下列顺序串联，每一步都有
断言（不是「跑通即通过」）：

01. 本机注册：`workspace.select`（真实临时目录）+ `agent.configure` + 两个 `export.create`（可见
    `grant.observe/interact/remote-work`、不可见 `grant.approve`）；
02. 配对：`node.pair.begin` 取二维码 → fake Access 侧 `POST /node-link/v1/pair/claim`（201 + 验
    `ownerProof`）→ `node.pair.confirm`（本地管理入口）→ `POST .../status` = `approved`；
03. 握手：真实 WSS（`node.hello`/`challenge`/`proof`）并**验 Owner 的 `nodeProof`**（含
    `catalogRevision`，tag 6 的 transcript）；
04. `catalog.snapshot`：可见集**恰好** `[export.visible]`，与节点 grants 不相交的 Export 不出现；
05. `session.create`：`command.accepted(result=null)` → `command.terminal(SessionCreateResult)`，断言
    `remoteSessionRef.ownerNodeId == Authority::local_node()`（把「传错 node id 会让全部 attach/ack 被拒」
    钉成断言）、`exportId`、`sessionMeta.state`；
06. 幂等：同 requestId + 同 payload 直接回首次结果（同一 `sessionId`，不建第二个会话）；同 requestId
    换语义（多一个空的 `templateParams`，仍是合法 payload）→ `nodelink.command.idempotency_conflict`；
07. 拒绝路径：payload 带 `cwd` → `nodelink.command.unsupported_field` + `details.field=cwd`；未知
    `workspaceAlias` → `nodelink.export.not_granted` + `details.parameter=workspaceAlias`；
08. `resource.attach` → `resource.attached`（generation=1）；`resource.subscribe(cursor=null)` →
    `snapshot_begin`/`snapshot_chunk`/`snapshot_end` 的 `snapshotId` 一致、`chunkCount=1`、唯一 item 是
    `session_meta`（无正文）；`resource.ack`（静默，靠后续 `link.ping` 证明连接健康）；
09. 事件扇出 + 终态推送：`session.prompt` → `command.accepted`（带 `turnId`，此时 turn 未结束）→
    会话状态先走 `turn.queued`/`turn.started` → 释放脚本端点暂存的后端事件 → `broker.pump` →
    `resource.event(agent.message.delta)`（正文 = 本次 prompt 内容、`ownerNodeId` 正确、无伪造 `acp`）→
    终态由 `CommandRoute::dispatch` 观察循环推给提交方连接（`status=completed`、`result.turnId` 与
    accepted 一致、`terminalEventId` 非空）→ `command.status` 重查同一条 mutation 回同一终态；
10. **R61（RV1-WP5-F2）**：`FlakySessionStore` 只让「带 marker 的那一批」（= delta 批）提交失败 →
    `resource.event` 一条也不出现（负向断言：500 ms 内收到的全部消息里没有该 marker、也没有 delta），
    同 turn 的终态属于另一批（§6.10 按终态切批）因此照常到达，连接继续可用（`link.ping` 往返）；
11. `export.revoke`（本地管理）→ 受影响连接收到 `export.revoked` → 该 Export 上的后续 `session.prompt`
    一律 `nodelink.export.not_granted`；
12. `node.revoke` → 连接收到 `node.trust.revoked` 且**以 4410 关闭**（帧级断言 `close_code()==4410`）。

`tls_direct_terminates_the_same_handshake`：`[daemon.tls] mode="direct"` + 现算自签证书（`rcgen`，
SAN = `acpr-test.example.invalid`，unix 下 `chmod 0600`）→ 配对 HTTP 也走 TLS（claim 201 + `ownerProof`
验签）→ 本地确认 + status approved → **TLS 之上的 WSS 握手**（同一 `nodeProof`/revision 校验）→
`catalog.snapshot` 往返。按任务书「可直接停在 catalog.snapshot」收敛。

## 需要主 Agent / reviewer 知晓的偏差与残余

1. **`node_link.*`（以及 `sync.*`/`sessions.*`/`terminal.*`）仍未接线**（登记，不修）：本 WP 只按 2.21
   收敛 `daemon.listen`/`daemon.tls.*`/`daemon.allowed_hosts`/`daemon.trusted_proxies` 与
   `dev_mode.allow_plaintext`。生产配置走 `NetConfig::default()` 的限额，而 `NodeLinkConfig::default()`
   的同类限额与之同值（1 MiB 等），因此当前没有限额分叉；但**显式配置 `node_link.max_message_bytes`
   等键会被接受却只记 unwired**，这是 WP8（2.23）的文档收敛项。唯一接线点是
   `app::daemon::net_config(config)`，WP8 需要时只改这一处。
2. **`daemon.status.links` 恒空**（设计如此）：出站重连管理器属切片 6，`links` 现在没有来源，代码注释
   已写明原因（`AppDaemonControl::status`）。
3. **`daemon.instance_lock = "ipc"` 仍记 unwired**（既有行为，未动）：两者在本切片语义等价，不静默换实现。
4. **集成测试不经 `daemon::run` 进程**：`[PV5]` 用 in-process harness（`support/owner.rs`）装配，
   但它调用组合根自己的接线点（`app::daemon::net_config`、`register_node_link_paths`、
   `app::compose::NodeLinkCloser`、`forked_publisher`），并用真实 `SqliteStore`/`Authority`/
   `LocalAdminRouter`/`NetListener`；只有「进程生命周期与信号」这件事由 `[PV4]` 的真实二进制轮次覆盖。
   两轮合起来才等于 2.21 的完成条件。
5. **两个刻意的测试替身**（登记）：
   - `ScriptedBackends`/`ScriptedEndpoint` 取代真实 ACP 子进程（跨 crate 拿不到
     `CARGO_BIN_EXE_acpr-fake-acp-agent`；真 Agent 属可选兼容套件，不应成为普通用例硬依赖）；
   - 脚本端点采用「**先暂存、后释放**」两段式：`prompt` 只暂存 delta + `turn.completed`，用例在观察到
     `command.accepted` 之后才 `release` 并驱动一次 `broker.pump`（真实 daemon 里这一步由
     `storage.flush_interval_ms` 的合并窗口任务周期驱动）。这样做是**为了真的覆盖终态观察循环
     （RV1-WP6-F9/F4）**：若端点在 `prompt` 内一次性投递，broker 会在 `submit_command` 内跑完整轮
     turn，回复直接是 `command.terminal`，观察循环那条路径就完全没被执行。
6. **同一件事的另一面（reviewer 请注意）**：`CommandRoute::on_mutation` 在「提交后记录已终结」时**只回
   `command.terminal`**（不先回 `command.accepted`），这是 §12.5「已终结的回首次结果」的既有实现行为
   （WP6 冻结），本 WP 未改动，但 `[PV5]` 因此把「accepted → 观察循环 → terminal」定为断言口径；
   同一路径在「turn 未结束」时才回 accepted 并挂观察。
7. **顺序偏差**：任务书列的链路把 attach/subscribe 写在 `command.submit` 之前，而 `resource.attach`
   要求会话已存在，因此实际顺序是 `session.create` → attach/subscribe → `session.prompt`。断言集合不变。
8. **`node_link_e2e` 的 R61 在链路中段执行**：故障注入后该 requestId 的 turn 仍会进入终态，之后的
   `export.revoke`/`node.revoke` 不依赖会话健康状态，因此放在撤销之前既满足「负向断言发生在订阅仍
   有效时」，也不污染撤销断言。
9. **新发现的已知残余（登记，不修）**：delta 批次提交失败时，同一 turn 的 `turn.completed` 属于另一批
   并会正常落盘，因此客户端会看到「turn 完成、但正文缺失」，且该 requestId 的终态是 `completed` 而非
   `uncertain`（§6.9 的 uncertain 只覆盖带终态的那一批）。用例把这一事实**显式断言**为
   `status=completed` 并在注释里标注残余，避免它被当作回归静默漂移。归属 RV1-WP5 残余①的相邻面 /
   RV2-WP6 残余⑤。
10. **任务书要求登记的既有残余（均未修，均在测试里留痕）**：
    - 扇出队列满即丢（`EVENT_QUEUE_CAPACITY`，RV1-WP5 残余①）——`support/nodelink.rs` 模块头与
      `node_link_e2e.rs` 模块头各有一处说明；
    - 「会话阻塞在 socket 写」路径（RV1-WP4 L5）——同上，用例不做慢消费者模拟；
    - 预持久化失败可能留下无端点的会话行（RV2-WP6 残余⑤）——同上。
11. **测试支撑里的两处非产品旋钮**（reviewer 可能想核）：
    - `ACPR_NODELINK_TRACE=1`：把客户端双向报文打到 stderr（排障用，默认关）；
    - `ACPR_TEST_LOG=1`：把 daemon 侧结构化日志（DEBUG，含 sqlx 语句）装到 stderr（默认关）。
    两者都只影响测试进程输出，不改变被测行为。

## handoff_index

```yaml
handoff_index:
  - task_id: "2.21"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "07b956c"
    evidence_type: CHECK
    evidence_id: PV4
    report_path: "reports/wp7-app-wiring.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "工作区 = 07b956c 的接线（同内容包含在 a3109c8 里）上的本机轮次：cargo test --locked -p app --all-features --test node_link_listener -- --test-threads=1 退 0，8 passed / 0 failed / 0 ignored（真实二进制 CARGO_BIN_EXE_acp-remote 起停：绑定、占用/非法字面量拒绝启动、direct 缺 PEM 拒绝启动、真实 TLS 终止且明文无响应、dev_mode 明文非 loopback 拒绝、proxy 非 loopback 告警、四个 daemon.* 键不再进 unwired）。R1–R4、R12–R15 的 daemon 侧逐条映射见本文件表格；R12/R13/R14 的本机轮次另计入 [PV5]。cargo fmt --all -- --check、cargo clippy --locked --workspace --all-targets --all-features -- -D warnings、npm run check 三者在提交前均退 0（pre-commit 钩子）与提交后复跑退 0。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.21"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "a3109c8"
    evidence_type: CHECK
    evidence_id: PV4
    report_path: "reports/wp7-app-wiring.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "同上一条检查在 a3109c8（HEAD，含 07b956c 全部内容 + 仅新增测试文件）上的复跑：8 passed / 0 failed。两批之间没有任何接线文件的差异，因此 [PV4] 对两个提交都成立。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.21"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "a3109c8"
    evidence_type: VALIDATION
    evidence_id: NOT_APPLICABLE
    report_path: "reports/wp7-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "启动顺序（恢复 → TLS 配置 → 绑定 listener → 单实例锁 → 本地 endpoint → 发布锁记录 → 开放接入）与关闭顺序（停接入面含 listener 排空 daemon.shutdown_grace_ms → 取消任务 → 停 Agent → 刷盘 → 清理）以 crates/app/src/daemon.rs 模块头逐条声明并有实现；daemon.status.listen 回真实地址、links 恒空；proxy + 非 loopback 额外告警；组合根零业务规则（只做装配与日志）。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.22"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "a3109c8"
    evidence_type: CHECK
    evidence_id: PV5
    report_path: "reports/wp7-integration.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "工作区 = a3109c8（git status 为空）上的本机轮次：cargo test --locked -p app --all-features --test node_link_e2e -- --test-threads=1 退 0，2 passed / 0 failed / 0 ignored（the_controlled_path_runs_end_to_end_and_revocation_propagates 覆盖配对→握手→catalog 过滤→session.create（幂等/unsupported_field/not_granted）→attach/subscribe/snapshot/ack→事件扇出与终态观察循环→R61 负向断言→export.revoke/node.revoke 撤销传播与 4410；tls_direct_terminates_the_same_handshake 覆盖 TLS direct 上的同一握手与 catalog 往返）。同一轮追加进 reports/pv5-windows-nodelink.log。全量 cargo test --locked -p app --all-features = 84 passed / 0 failed / 0 ignored。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.22"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "a3109c8"
    evidence_type: VALIDATION
    evidence_id: NOT_APPLICABLE
    report_path: "reports/wp7-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "三条强制端到端断言各有对应断言：[R61] 故障注入批次不发布（marker 与 delta 双向负向断言）且连接存活；终态推送经 CommandRoute::dispatch 观察循环到达提交方连接（accepted 在 turn 未结束时先回，terminal 后到，result.turnId 与 accepted 一致）；撤销端到端 export.revoke → export.revoked + 后续命令 not_granted，node.revoke → node.trust.revoked + 4410。ResourceRoute 的 node id 取 Authority::local_node 由 remoteSessionRef.ownerNodeId 断言钉住。三处既有残余（扇出队列满丢、socket 写阻塞、无端点会话行）与一处新登记残余（delta 批失败而终态批成功）在测试注释与本报告登记，均未修。"
    source_evidence: NOT_APPLICABLE
```
