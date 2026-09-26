# WP2 fix2 Handoff — RV2-WP2FIX 的 F1/F3（`node-link-owner` / DU1）

## Shared Report

- **task_id**: `RV2-WP2FIX-F1` / `RV2-WP2FIX-F3`（WP2 修复阶段的第二轮；对应 review 报告 `reports/rv2-wp2fix.md` 的 Findings 与其中 F1 建议的 `drop(permit)` 承重注释）
- **role**: coder
- **phase**: fix；**stage**: work-package
- **agent_context**: 任务级 coder 子 Agent（fix2 轮次），**不继承** WP2/fix1 会话，只按主 Agent 下发的 fix 清单与 `rv2-wp2fix.md` 原文作业。工作目录 `D:\Project\acp-remote-wt\node-link-owner`（worktree，分支 `agentic/node-link-owner`，起点 HEAD = `50a5af42`，工作区干净）；权威规划根 `D:\Project\acp-remote`（报告与日志写在 `openspec/changes/node-link-owner/reports/`，该目录按 `.gitignore` 不入版本控制）。工具集含 shell 与 Git：本轮的红绿对照与全部门禁均由本 Agent 亲自执行。
- **target_revision**: `041aeb043d7b585b43faaaf1f40a330854b80784`（fix2 提交；被处理的 review 证据 RV2-WP2FIX 产生于 `50a5af420ec5c25e521dd93aeb3dc9bf3a732ab5`，两者为单提交之差）
- **scope**（严格限于派单允许的 `crates/server/src/transport/net/**`）：
  - 写入：`crates/server/src/transport/net/listener.rs`（+ 测试专用注入点、两处注释）、`crates/server/src/transport/net/tests.rs`（+1 条用例、映射表 1 行）。
  - 未写入：其余 `net/**` 文件、`docs/**`、`compatibility/**`、`schemas/**`、`fixtures/**`、`Cargo.toml`/锁文件、`plan.md`/`tasks.md`/`verification.md`。
  - **未做**（明确不在本轮授权范围）：不改公开 API（`pub use` 清单零增删、`NetConfig` 无新字段）、不改配置面（池容量 64、握手超时生产默认 10 s 均不变）、不处理 RV2-WP2FIX-F2（已由主 Agent 裁决为「已知取舍」，记入 WP4 评估）与 N-RV2-1/N-RV2-2（WP7/WP8 收口）。
- **changes**（1 个提交，2 个文件，96 insertions / 4 deletions）：
  - `net/listener.rs`（F1 的测试注入点 + F3 的两处注释）：
    - `NetListener` 增加私有字段 `handshake_timeout: Duration`；`bind()` 填生产常量 `TLS_HANDSHAKE_TIMEOUT`（10 s）；`serve()` 把它交给 `NetAcceptor` 的同名字段；握手任务改用该字段做 `tokio::time::timeout`（原来直接引用常量）。**公开语义与生产默认值不变**。
    - 新增 `#[cfg(test)] pub(super) fn NetListener::override_handshake_timeout(&mut self, Duration)`：只缩短超时，仅测试构建可见。
    - `TLS_HANDSHAKE_POOL_SIZE` 由私有 `const` 改为 `pub(super) const`（仍只在 `net` 子树可见）+ 一句文档说明用途；**池容量本身仍不可配置**，`Semaphore::new(TLS_HANDSHAKE_POOL_SIZE)` 与 `mpsc::channel(HANDSHAKE_READY_CAPACITY)`（= 池容量）都继续从常量取值，因此「结果队列容量 = 池容量」这条不变量在测试配置下同样成立。
    - F3-a：`accept()` 的 `ready_rx.recv()` 返回 `None` 分支加注释，说明不可达依据（`ready_tx` 由 `NetAcceptor` 自持，见该字段既有注释）以及该分支只做兜底、不得变成忙等。
    - F3-b（F1 的建议）：`drop(permit)` 处补承重注释——「本行先于结果投递是死锁自由性的前提，不得移动到 `send` 之后」，并写出理由（持有槽位的任务去等结果队列 + `accept` 因池满等槽位 ⇒ 互相等待）。
  - `net/tests.rs`（F1 的守卫用例）：新增
    `saturated_handshake_pool_waits_for_a_slot_instead_of_dropping_new_connections`，并把该用例登记到文件头映射表的 `[R12]/[R13]` 行。
- **checks**（原始输出见 `wp2-transport-net.log` §10）：
  - `cargo fmt --all -- --check` exit=0；`cargo clippy --locked -p server --all-targets --all-features -- -D warnings` exit=0；`cargo test --locked -p server --all-features` 全绿：**166** lib（fix1 的 165，+1）+ 14/6/4/0/4/0 集成，0 failed / 0 ignored；`transport::net` 76 → **77**。既有用例计数只增不减、无用例被删或弱化。
  - `rg -n "unsafe" crates/server/src` **零命中**（exit=1）。
  - 提交前钩子（`.husky/pre-commit`）三步全通过：fmt → `npm run check`（16 passed / 0 failed，含合同漂移、crate 依赖方向、doc links、`check:agentic`）→ clippy（`--workspace --all-targets --all-features`）；commitlint 通过（`test(server): …`）。提交后 `git status --porcelain` 为空。
  - **回归有效性实测（旧实现红 / 新实现绿）**：临时把 `accept()` 改回 `4e8a1075` 的「在 accept 里内联握手」形态（仅超时来源换成注入值；改动前先 `git add` 暂存计划落地状态），新用例在该形态下 **FAILED**（`池满时新连接必须等槽位，不得被丢弃或无限推迟: Elapsed(())`，5.06 s 触到 5 s 上限）；随后 `git checkout -- crates/server/src/transport/net/listener.rs` 从暂存区恢复（`git diff --name-only` 为空），同用例 **ok**（0.52 s）。临时改动已完全恢复，无残留。
- **issues**: 两项均落地，无阻断。需要下游知道的三点：
  1. **用例的区分度是时间上限**：注入 500 ms 后，背压实现只等**一个**既有握手超时（实测 0.52 s，与排队连接数无关）；旧实现要顺序等「池容量 × 单次握手超时」= 32 s，因此 5 s 上限能稳定分开两者（≥ 4× 余量）。用例**不**依赖具体槽位数写死：空闲连接数取自 `TLS_HANDSHAKE_POOL_SIZE`，容量调整时不会静默失去覆盖。
  2. **只注入超时、不注入池容量**是有意的：注入容量会让「结果队列容量 = 池容量」这条不变量在测试配置下失真，而注入超时已足够让饱和路径在 0.5 s 内真实经过。
  3. **RV2-WP2FIX-F2 仍成立**（背压方案的成本门槛 = 64 条并发预认证连接 / 单次 10 s）：本轮按主 Agent 裁决不改，作为已知取舍随 WP2 交付记录，待 WP4 接线认证限流时评估「按 IP 限制在途握手数」；池容量与握手超时仍是硬编码常量、`CONFIG_REFERENCE.md` 无对应键（本轮未动配置面）。
- **result**: **PASS**（本轮 fix 范围内全部适用条件满足、门禁全绿；不代表 RV3 独立复验、WP8 文档收口、候选验证、E2E 或合并已完成）
- **evidence_paths**:
  - 本报告：`openspec/changes/node-link-owner/reports/wp2-fix2-handoff.md`
  - 被处理的 review：`openspec/changes/node-link-owner/reports/rv2-wp2fix.md`（Findings RV2-WP2FIX-F1/F3 与 F1 的 `drop(permit)` 注释建议）
  - 本轮原始输出（新增分节）：`openspec/changes/node-link-owner/reports/wp2-transport-net.log` §10（§10.1 注入点、§10.2 修复前失败证据、§10.3 修复后门禁、§10.4 新用例清单、§10.5 钩子与提交）
  - 修改对象：`crates/server/src/transport/net/{listener,tests}.rs`
- **resource_cleanup**: 未新增端口、数据库、容器、证书或外部账号；未安装依赖、未改锁文件；无 `git stash` 残留（本轮用的是「暂存 + `git checkout --` 单文件恢复」，无 stash 条目）。临时资源只有用例自建的 `TestCertificate` 临时目录（由 `Drop` 删除）与 64 条 loopback 连接（用例结束、`server.stop()` 后释放）。提交后 `git status --porcelain` 为空，无残留未跟踪文件。

## RV2-WP2FIX-F1（MINOR/P2）：「池满 → 背压等待槽位」是唯一无用例覆盖的新路径

- **发现原文要点**：`listener.rs:516-527` 的 `acquire_owned().await` 与新连接背压路径没有用例覆盖；死锁自由性取决于 `drop(permit)` 先于 `send` 这一承重顺序，现有用例抓不到未来回归。主 Agent 裁决：补饱和用例，允许测试专用注入缩小池容量/超时。
- **修复**：
  - 用例按**实际容量**填满池（`0..TLS_HANDSHAKE_POOL_SIZE` 条「只连接不握手」的连接），再发起第 `64+1` 条**真实 TLS** 连接并断言它拿到 200：
    - 未被丢弃/拒绝 —— 新连接没有拿到任何响应头就不再是失败；它按背压等槽位，既有握手超时归还槽位后完成握手。
    - 排队顺序保证「被 accept 时池已耗尽」：新连接排在既有连接之后（TCP 接收队列 FIFO），因此它拿不到现成槽位，只能等既有槽位归还 —— 这正是被测路径。
  - 测试专用注入：`#[cfg(test)] pub(super) fn NetListener::override_handshake_timeout`，用例取 500 ms。生产默认 10 s、池容量 64 均不变；`NetConfig` 与 `pub use` 面零改动。
  - 红绿证据：旧实现（accept 内联握手）FAILED（`Elapsed`，5.06 s）；新实现 ok（0.52 s）。
- **本轮独立核对**：
  - 用例失败信息与 panicked 位置（`tests.rs:1142`）对应到断言文本，且失败发生在**5 s 上限**而不是别的断言 → 说明旧实现的顺序握手确实把新连接推迟到超出上限。
  - 恢复后 `git diff --name-only` 为空，说明临时回退没有污染交付内容；恢复后同用例再次 ok。
  - 既有 `idle_tcp_connections_do_not_block_new_connections`、`direct_mode_terminates_tls_and_rejects_plaintext`、关闭排空等用例全部保持绿（net 76 → 77，仅新增 1 条）。

## RV2-WP2FIX-F3（SUGGESTION）：`ready_rx.recv()` 的 `None` 分支无可达性说明

- **发现原文要点**：`listener.rs:557-562` 的 `None` 分支当前不可达且为空循环体——边界条件若被破坏会变忙等；主 Agent 裁决补注释说明不可达依据。
- **修复**：在该分支加注释，引用 `ready_tx` 字段既有注释（`listener.rs:494-496`：`NetAcceptor` 自己持有一份发送端，正是为了避免池空闲时 `select` 空转），写明「只要这个值还活着，接收端就至少有一个存活的发送端 ⇒ `None` 在本结构里不可达；该分支只做兜底，不能在此空转（空循环体会变成忙等）」。
- **附带的承重注释（F1 建议）**：`drop(permit)` 处补「本行先于结果投递是死锁自由性的前提，不得移动到 `send` 之后」及理由，作为未来改动者的显式约束。
- **本轮独立核对**：`rg -n "unsafe" crates/server/src` 零命中；两处只改注释、未改控制流（`git diff --cached` 中 `drop(permit);` 与 `if let Some(connection) = ready` 语句本身逐字未动）。

## handoff_index

```yaml
handoff_index:
  - task_id: "RV2-WP2FIX-F1"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "041aeb043d7b585b43faaaf1f40a330854b80784"
    evidence_type: REVIEW
    evidence_id: RV2-WP2FIX
    report_path: "openspec/changes/node-link-owner/reports/wp2-fix2-handoff.md"
    result: PASS
    evidence_status: REUSED
    applicability_basis: "RV2-WP2FIX 产生于 50a5af42（其 F1 指出「池满 → 背压等待槽位」无用例覆盖）。本行是该 finding 的最小修复：新增 saturated_handshake_pool_waits_for_a_slot_instead_of_dropping_new_connections（按 TLS_HANDSHAKE_POOL_SIZE 精确填满池后，新连接仍完成 TLS 握手并拿到 200），并在 drop(permit) 处补承重注释。改动只触及 listener.rs 的私有字段/测试专用注入点与注释、tests.rs 的 1 条新用例，公开 API 面零增删、配置面零改动，因此 review 的其余正确性结论不受影响。红绿实测：旧实现（accept 内联握手）FAILED（Elapsed，5.06s）→ 新实现 ok（0.52s）。F1 是否闭环仍需下一轮独立复验。"
    source_evidence:
      id: "RV2-WP2FIX"
      report_path: "openspec/changes/node-link-owner/reports/rv2-wp2fix.md"
      target_revision: "50a5af420ec5c25e521dd93aeb3dc9bf3a732ab5"
  - task_id: "RV2-WP2FIX-F3"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "041aeb043d7b585b43faaaf1f40a330854b80784"
    evidence_type: REVIEW
    evidence_id: RV2-WP2FIX
    report_path: "openspec/changes/node-link-owner/reports/wp2-fix2-handoff.md"
    result: PASS
    evidence_status: REUSED
    applicability_basis: "同 F1 的来源与目标版本。F3 是 SUGGESTION（仅可维护性）：在 accept() 的 ready_rx.recv() None 分支补不可达依据注释（引用 ready_tx 自持发送端的既有注释），并把『drop(permit) 先于结果投递 = 死锁自由性前提』写成显式承重注释。两处都只改注释、不改控制流，因此需要在目标版本上重核的只是注释与代码事实是否一致（不是行为正确性）；本轮的替代证据是门禁全绿 + 无 unsafe + net 用例计数 76 → 77。"
    source_evidence:
      id: "RV2-WP2FIX"
      report_path: "openspec/changes/node-link-owner/reports/rv2-wp2fix.md"
      target_revision: "50a5af420ec5c25e521dd93aeb3dc9bf3a732ab5"
```

## 遗留与下一步

- **不在本轮范围、需要下游处理的提醒**：
  - RV2-WP2FIX-F2（已知取舍）：背压方案把「预认证连接的成本门槛」提到 64 条并发 / 单次 10 s，本层仍无按源 IP 的准入闸门；请在 WP4 接线认证限流时评估「按 IP 限制在途握手数」。本轮未动配置面。
  - N-RV2-1/N-RV2-2：F1 用例在覆盖表的归属与「非需求性回归保护」的显式注记、`origin_host` 接受 `http://` 与协议侧 `CanonicalOrigin` 只接受 `https://` 的口径差，均属 WP7/WP8 收口。
  - 池容量与握手超时仍是硬编码常量（`CONFIG_REFERENCE.md` 无对应键）。
- **建议下一步**：对 `041aeb043d7b585b43faaaf1f40a330854b80784` 做独立复验，重点看新用例是否真的钉住背压路径（而不是恒真）、`#[cfg(test)]` 注入点是否越出「不改公开语义」的边界（`pub use` 清单、`NetConfig` 字段、生产常量取值），以及两处注释与代码事实是否一致。
- **未执行（如实）**：整 workspace 的 `cargo test`（本轮按派单只跑 `-p server`；提交前钩子跑了 workspace 级 clippy 与 `npm run check`）；`cargo test -p app`（本轮改动未触及 app 的消费面：`NetListener` 只在 `net` 模块内构造）；Linux `cfg(unix)` 权限用例（Windows 本机不编译，由 Linux CI 覆盖）；CI 的 `deps`/`advisories`/`secrets`（本地无 gitleaks，无等价物）；任何 E2E 与候选验证。
