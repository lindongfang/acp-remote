# WP2 fix1 Handoff — RV1-WP2 的 F1/F2/F3 + N2/N3（`node-link-owner` / DU1）

## Shared Report

- **task_id**: `RV1-WP2-F1` / `RV1-WP2-F2` / `RV1-WP2-F3` / `N2` / `N3`（WP2 的 fix 阶段；对应 review 报告 `reports/rv1-wp2.md` 的 Findings 与 report-only note）
- **role**: coder
- **phase**: fix；**stage**: work-package
- **agent_context**: 任务级 coder 子 Agent（fix 轮次），**不继承** WP2 实现会话，只按主 Agent 下发的 fix 清单与 `rv1-wp2.md` 原文作业。工作目录 `D:\Project\acp-remote-wt\node-link-owner`（worktree，分支 `agentic/node-link-owner`）；权威规划根 `D:\Project\acp-remote`（报告与日志写在该处 `openspec/changes/node-link-owner/reports/`，该 reports 目录按 `.gitignore` 不入版本控制）。工具集含 shell 与 Git：本轮的 diff、stash 回退复现与门禁均由本 Agent 亲自执行。
- **target_revision**: `50a5af420ec5c25e521dd93aeb3dc9bf3a732ab5`（fix 提交；被检视的 base = `4e8a1075a48e004808f498447b54e45dbff824ce`，两者为单提交之差，工作区干净）
- **scope**: 写入范围严格限于 `crates/server/src/transport/net/{mod,host,listener,route,test_client,tests}.rs` 六个文件（= 派单允许的 `net/**`；`crates/server/src/transport/mod.rs` 未改，因为 `pub mod net;` 的现状注记仍然成立）。**未做**：不改 `listener` 之外的模块、不改 `crates/server/Cargo.toml` 与任何依赖、不改 `docs/**`、`compatibility/**`、`schemas/**`、`fixtures/**`、`plan.md`/`tasks.md`/`verification.md`；不处理 N1/N4/N5/N6/N7（其中 N1/N7 归 WP8 的文档收口，N5 是 WP3/WP4 的接线核对，N6 是证据对齐）。F3 按主 Agent 的裁决走方案 **(a)**（握手移出 accept 关键路径），不是 (b)「下调超时」。
- **changes**（1 个提交，6 个文件；385 insertions / 44 deletions）：
  - `net/mod.rs`（F1，文档）：模块组成行补「`direct` 模式的 TLS 握手在接入选层的有界并发池里完成，不占 accept 关键路径」；把「不把 `axum` 类型暴露给上层」改为如实表述——本层不暴露 axum 的**路由/提取器/服务设施**，上层只经 `WsHandler`/`HttpHandler` 注入处理器并消费本模块的值对象；值对象字段所用的 `http`/`bytes` 值类型（`Method`/`HeaderMap`/`StatusCode`/`Bytes` 等，经 axum 再导出）含义不变，上层按 axum 的公开路径引用，本模块不重复导出这批值类型。
  - `net/host.rs`（F2，行为）：`HostPolicy::new` 先对 `public_origin` 无条件做一次 `origin_host(...)` 校验（`None` 保持 `None`，非法即 `Err(NetError::InvalidPublicOrigin)`），再走原三分支；白名单非空分支不再跳过该校验。模块文档与 `new` 的文档各加一句说明该口径。新增回归用例 `invalid_public_origin_fails_closed_even_with_a_non_empty_allowlist`。
  - `net/listener.rs`（F3，行为 + 文档）：`direct` 模式的 TLS 握手移出 accept 关键路径。
    - 新增 `TLS_HANDSHAKE_POOL_SIZE = 64`（固定并发上限）与 `HANDSHAKE_READY_CAPACITY = 同一常量`（结果交接队列容量）；`TLS_HANDSHAKE_TIMEOUT = 10s` **保留**，语义变为「池里单次握手的超时」。
    - `NetAcceptor` 增加 `handshakes: JoinSet<()>`（在途握手任务的持有者）、`permits: Arc<Semaphore>`（槽位）、`ready_tx`/`ready_rx`（有界结果队列，容量 = 池容量）。`accept()` 改为 `tokio::select!`：已完成的握手结果优先返回、TCP accept、并顺带收割已结束的握手任务；`start_connection()` 取槽位（**池满即等待一个槽位＝对新连接施加背压**，而不是回到内联握手）后 `spawn` 握手任务。
    - 任务所有权与取消：握手任务由 `NetAcceptor.handshakes` 持有（不 detached）；`axum::serve` 结束时（宽限排空完成或 `abort()`）`NetAcceptor` 被丢弃 → `JoinSet` 丢弃 → 在途握手一并中止；尚未握完的连接从未进入 axum 的连接表，因此不影响 HTTP 侧排空。槽位在握手结束**立即归还**（早于结果投递），所以「结果队列满」不会死锁，背压等待上限由握手超时给出。
    - 新增回归用例 `idle_tcp_connections_do_not_block_new_connections`。
  - `net/route.rs`（N3）：`offers_subprotocol` 的文档写明「同名头多次 = 一个头里的逗号列表」同一口径，并指出 `axum::extract::ws::WebSocketUpgrade` 同样用 `get_all` + 逗号切分 + trim + `contains`，两侧在「任一头值里以 token 形式出现」时都成立；`subprotocol_detection_is_exact_and_token_wise` 追加三组断言（`(required, other)`、`(other, required)` 接受；`(other, v2)` 拒绝），**原有断言一条未删**。
  - `net/test_client.rs`（测试工具）：`ws_request(...)` 重构为「组装头列表 → 调新函数」；新增 `ws_request_with_headers(path, host, &[(name, value)])`，可按顺序追加同名头（构造双头形态必需）。已有调用点的语义与输出逐字节不变。
  - `net/tests.rs`（N2/N3/F3 用例）：新增 `wrong_host_is_rejected_on_the_ws_upgrade_path`（N2）、`repeated_subprotocol_headers_follow_the_token_rule`（N3）、`idle_tcp_connections_do_not_block_new_connections`（F3）与辅助处理器 `CountingWsHandler`，并把三条新用例登记到文件头的场景映射表（`[R8]`/`[R9]`/`[R10]`/`[R12]` 行）。
- **checks**：
  - `cargo fmt --all -- --check` exit=0；`cargo clippy --locked -p server --all-targets --all-features -- -D warnings` exit=0；`cargo test --locked -p server --all-features` 全绿：165 lib（原 161，+4）+ 14/6/4/0/4/0 集成，0 failed / 0 ignored（原始输出见 `wp2-transport-net.log` §9.2）。`transport::net` 用例 72 → **76**。
  - `rg -n "unsafe" crates/server/src` **零命中**（exit=1）。仓库 `unsafe_code = "forbid"` 未被动过。
  - 提交前钩子（`.husky/pre-commit`）三步全通过：fmt → `npm run check`（16 passed / 0 failed，含 doc links / crate boundaries / contract drift / check:agentic）→ clippy（`--workspace --all-targets --all-features`）；commitlint 通过（`fix(server): …`）。
  - **回归有效性实测**（本轮新做的独立验证，不是照抄 review）：F3 用例在旧实现上 **FAILED**（`Elapsed(())`，5.02s 超时），在新实现上 0.01s ok；F2 用例在旧 `new()` 上 **FAILED**（返回 `HostPolicy { accepted: Allowlist(["proxy.internal"]) }`），在新实现上 ok。两处回退均为临时 `git stash`/单点回改并已恢复，`git stash list` 为空、`git status --porcelain` 为空（原始输出与命令见日志 §9.1）。
- **issues**: 五项全部落地，无阻断。三点需要下游知道的限制：
  1. **F3 的取舍如实记录**：池满（同时 64 个未认证握手在途）时新连接仍会被推迟到有槽位为止（背压），单个连接的等待上限 ≈ 一次握手超时；这是「有界资源」的必然，与旧实现「一条慢连接就能推迟所有新连接 10s」不同量级。是否把 64 或 10s 做成配置键不在本轮授权范围内（`CONFIG_REFERENCE.md` 未定义对应键，本轮未改配置面）。
  2. **N3 的结论是「按单一口径接受」**：本层与 axum 0.8.9 都按 `get_all` + 逗号切分判定，双头中任一处含约定 token 即 101 且回填；只有在「两个头都不含该 token」时才 400。用例里的 `("other", required)` 形态就是这条口径的探针：若 axum 将来只读第一个头值，101 不会回填，用例会失败，届时按派单要求改成「本层也拒绝双头」的失败关闭口径。
  3. F2 收紧后，`public_origin` 非法但 `allowed_hosts` 非空的配置从「静默接受」变为「启动即 `Err`」——这是契约要求的失败关闭；现有用例 `allowed_hosts_whitelist_is_used_when_configured` 用的是合法 `public_origin`，不受影响。
- **result**: **PASS**（本轮 fix 范围内全部适用条件满足、门禁全绿；不代表 RV2 独立复验、WP8 文档收口、E2E 或合并已完成）
- **evidence_paths**:
  - 本报告：`openspec/changes/node-link-owner/reports/wp2-fix1-handoff.md`
  - 被处理的 review：`openspec/changes/node-link-owner/reports/rv1-wp2.md`（Findings F1/F2/F3 与 note N2/N3）
  - 本轮原始输出（新增分节）：`openspec/changes/node-link-owner/reports/wp2-transport-net.log` §9（§9.1 修复前失败证据、§9.2 修复后门禁、§9.3 新用例清单、§9.4 交付提交）
  - 检视对象与修复对象：`crates/server/src/transport/net/**`（6 个文件）
- **resource_cleanup**: 未新增端口、数据库、容器、证书或外部账号；未安装依赖、未改锁文件。临时资源：`git stash` 两条（已 `pop`，`git stash list` 为空）、`/tmp/wp2fix1/*.log`（本机临时输出，仓库外）、用例自建 `TestCertificate` 临时目录（由 `Drop` 删除）。提交后 `git status --porcelain` 为空，无残留未跟踪文件。

## RV1-WP2-F1（MINOR/P2）：`net/mod.rs` 的模块文档与公开面口径不一致

- **发现原文要点**：模块文档写「不把 `axum` 类型暴露给上层」，但公开 API 的字段/参数就是 axum 再导出的值类型（`Method`/`HeaderMap`/`Bytes`/`StatusCode`），且 `net/mod.rs` 未再导出它们；WP3 必然要写 `axum::http::StatusCode` 之类路径。
- **修复**：选原建议的 (a)「改成如实表述」而不是 (b)「再导出一批值类型」——(b) 会让本模块多出一层与 `axum` 重复的公开面，而 (a) 只改文本、零行为影响。新表述把三条边界分开：本层不暴露 axum 的**路由/提取器/服务设施**；上层只经 `WsHandler`/`HttpHandler` + 本模块值对象交互；值对象内的 `http`/`bytes` 值类型含义不变、按 axum 公开路径引用。
- **本轮独立核对**：`grep -n "pub use" crates/server/src/transport/net/mod.rs` 确认再导出清单里确实没有 `Method`/`HeaderMap`/`StatusCode`/`Bytes`；`net/http.rs`、`net/ws.rs`、`net/route.rs` 的公开签名逐处核对，确认「值类型直出」是事实，因此原文只有表述问题、没有行为问题（与 review 的判断一致）。

## RV1-WP2-F2（MINOR/P2）：`HostPolicy::new` 在白名单非空时跳过 `public_origin` 校验

- **发现原文要点**：白名单非空分支提前 `return`，`HostPolicy::new(Some("not-an-origin"), &["proxy.internal"])` 返回 `Ok`，`NetError::InvalidPublicOrigin` 在该组合下不可达。
- **修复**：把 `origin_host(...)` 校验提到分支之前（`.map(...).transpose()?`），三分支只消费校验结果；模块文档与 `new` 的文档各加一句「白名单非空只改变谁来判定 Host，不豁免该值的合法性」。
- **本轮独立核对**：
  - 新用例覆盖五种非法形态（`not-an-origin`、`https://`、`ftp://…`、`https://user@…`）在「白名单非空」下全部 `Err`，并断言错误消息含原值（错误里不回显其它配置、不含密钥材料）；同时断言合法组合仍按白名单判定（`proxy.internal` 接受、`owner.example.com` 拒绝）。
  - 回退实测：旧实现下该用例 FAILED（`HostPolicy { accepted: Allowlist(["proxy.internal"]) }`），新实现下 ok —— 用例确实钉住 F2 的差异，而不是恒真的形式断言。
  - 现有 `host::tests::*`（8 条）与 `tests::allowed_hosts_whitelist_is_used_when_configured`、`tests::wrong_host_is_rejected_before_routing` 均未改且全绿。

## RV1-WP2-F3（MINOR/P2，主 Agent 裁决按 (a)）：`direct` 模式的 TLS 握手占用 accept 关键路径

- **发现原文要点**：`NetAcceptor::accept` 在 accept 循环内联完成握手（超时 10s），慢连接/慢握手可把新连接接入速率压到 ≈1/10s；无需凭据即可显著延迟其他客户端接入。
- **修复（方案 a）**：
  - 池化：固定 64 个槽位（`Semaphore`）+ `JoinSet` 持有任务 + 容量 64 的有界结果队列；`accept()` 只做 TCP accept 与结果回收，握手在后台任务里做，单次超时仍是 `TLS_HANDSHAKE_TIMEOUT = 10s`。
  - 背压 vs 立即关闭：选**背压**（等一个槽位）。理由：立即关闭会让「攻击者占满 64 槽」时合法客户端直接吃连接重置（可用性更差），而背压只是推迟到有槽位为止，且槽位在握手结束立即归还，等待上限由握手超时给出；同时 accept 循环不再因**单条**慢连接阻塞（要阻塞需要 64 条同时在途）。
  - 取消路径：任务由 `NetAcceptor` 的 `JoinSet` 持有，`axum::serve` 结束/被 abort 时随 `NetAcceptor` 一起中止；结果队列另一端的 `ready_tx` 由 `NetAcceptor` 自己持有一份，避免池空闲时 `select` 空转。
- **本轮独立核对**：
  - 新用例 `idle_tcp_connections_do_not_block_new_connections`：4 条只 TCP 连接、不发 ClientHello 的连接在途时，新的 TLS 客户端必须在 5s 内完成握手并拿到 200（旧实现 5.02s 超时 FAILED → 新实现 0.01s ok）。
  - 既有 `direct_mode_terminates_tls_and_rejects_plaintext`、`direct_mode_does_not_warn_about_plaintext`、`shutdown_drains_and_releases_the_listener`（关闭后端口释放、会话被取消）全部保持通过，说明握手池没有破坏 direct 语义、告警集与关闭序列。
  - 所有权核对：读 tokio 1.53.1 源码确认 `JoinSet` 的 `Drop` 会 `abort` 全部任务，`select!` 在 handler 执行前已丢弃各分支 future（`let mut output = { … }` 作用域），因此 `accept()` 内的 `&mut self` 用法与取消语义都成立，不引入 detached task。

## N2（report-only note）：`/node-link/v1` 升级路径的 Host 拒绝断言

- **发现原文要点**：Host 拒绝用例只覆盖配对 path 与未知 path，未覆盖 WS 升级 path。
- **修复**：新增 `wrong_host_is_rejected_on_the_ws_upgrade_path`：`public_origin = https://owner.example.com` 下，`Host: evil.example.com` 的升级请求必须 400 且 WS 处理器零调用；随后用 `Host: owner.example.com` 断言同一路径能 101，证明上一条 400 来自 Host 判定而不是升级路径本身失败。为此新增只计数的 `CountingWsHandler`。

## N3（report-only note）：同一 token 拆成两个 `Sec-WebSocket-Protocol` 头

- **发现原文要点**：双头边界与 axum 回填口径的差异未定论，建议补用例钉死。
- **本轮定论（读上游源码 + 端到端实测）**：axum 0.8.9 的 `WebSocketUpgrade::from_request_parts` 用 `headers.get_all(SEC_WEBSOCKET_PROTOCOL)` 收集**全部**头值、按 `,` 切分并 `trim_ascii`，`protocols([...])` 再用 `contains` 精确匹配；本层的 `offers_subprotocol` 是同一口径。因此结论是**接受**：任一头值里以 token 形式出现约定 subprotocol 即升级成功，101 会回填该 token；两个头都不含该 token 时一律 400。
- **修复/证据**：
  - `net/route.rs`：`offers_subprotocol` 文档写明该口径与上游实现依据；单测追加三组断言（`(required, other)` / `(other, required)` 接受、`(other, v2)` 拒绝），原有断言一条未删。
  - `net/tests.rs`：`repeated_subprotocol_headers_follow_the_token_rule` 端到端发送双头：三组「含约定 token」形态断言 101 且 `Sec-WebSocket-Protocol` 回填为约定值（含 `(other, required)` 与「第一个头是逗号列表」两种顺序），两组「都不含」形态断言 400。注释写明：若上游改成只读第一个头值，`(other, required)` 那一组会失败，届时必须改成「本层也拒绝双头」的失败关闭，而不是保留「校验通过但没回填」的不一致。
  - `net/test_client.rs`：新增 `ws_request_with_headers(...)` 以便构造同名重复头；`ws_request` 改为它的薄封装，输出逐字节不变（既有 6 条升级用例全绿）。

## 验证命令与原始结论（target_revision = `50a5af42…`）

```console
$ git status --porcelain
（空）
$ git rev-parse HEAD
50a5af420ec5c25e521dd93aeb3dc9bf3a732ab5
$ cargo fmt --all -- --check
exit=0
$ cargo clippy --locked -p server --all-targets --all-features -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.17s
exit=0
$ cargo test --locked -p server --all-features | grep "test result"
running 165 tests
test result: ok. 165 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.25s
running 14 tests / 6 tests / 4 tests / 0 tests / 4 tests / 0 tests → 全 ok（与 §9.2 原文一致）
exit=0
$ cargo test --locked -p server --all-features --lib -- transport::net | grep "test result"
test result: ok. 76 passed; 0 failed; 0 ignored; 0 measured; 89 filtered out; finished in 2.25s
$ rg -n "unsafe" crates/server/src
exit=1（1 = 零命中）
$ git show --stat 50a5af42
 crates/server/src/transport/net/host.rs        |  54 +++++--
 crates/server/src/transport/net/listener.rs    | 121 ++++++++++++---
 crates/server/src/transport/net/mod.rs         |  12 +-
 crates/server/src/transport/net/route.rs       |  27 ++++
 crates/server/src/transport/net/test_client.rs |  21 ++-
 crates/server/src/transport/net/tests.rs       | 194 ++++++++++++++++++++++++-
 6 files changed, 385 insertions(+), 44 deletions(-)
```

回归有效性（修复前，临时回退后立即恢复；原文见日志 §9.1）：

```console
$ cargo test … --lib idle_tcp_connections_do_not_block_new_connections   # 旧 listener.rs
test result: FAILED. 0 passed; 1 failed; … finished in 5.02s
（panicked: 只连接不握手的对端不得阻塞新连接的接入: Elapsed(())）
$ cargo test … --lib invalid_public_origin_fails_closed_even_with_a_non_empty_allowlist  # 旧 new()
test result: FAILED. 0 passed; 1 failed; … finished in 0.00s
（panicked: 白名单非空不豁免 public_origin 的校验: HostPolicy { accepted: Allowlist(["proxy.internal"]) }）
```

## handoff_index

```yaml
handoff_index:
  - task_id: "RV1-WP2-F1"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "50a5af420ec5c25e521dd93aeb3dc9bf3a732ab5"
    evidence_type: REVIEW
    evidence_id: RV1-WP2
    report_path: "openspec/changes/node-link-owner/reports/wp2-fix1-handoff.md"
    result: PASS
    evidence_status: REUSED
    applicability_basis: "复用的 review 证据 RV1-WP2 产生于 base 4e8a1075a48e004808f498447b54e45dbff824ce；本行针对其 F1（mod.rs 文档与公开面口径不一致）的最小修复，落地于 50a5af42（单提交之差）。修复只改 net/mod.rs 的模块文档文本，未增删任何公开项、未改行为，因此 review 的其余结论（依赖方向、公开 API 面、用例名与源码吻合）不受影响；F1 是否闭环仍需 RV2 独立复验。"
    source_evidence:
      id: "RV1-WP2"
      report_path: "openspec/changes/node-link-owner/reports/rv1-wp2.md"
      target_revision: "4e8a1075a48e004808f498447b54e45dbff824ce"
  - task_id: "RV1-WP2-F2"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "50a5af420ec5c25e521dd93aeb3dc9bf3a732ab5"
    evidence_type: REVIEW
    evidence_id: RV1-WP2
    report_path: "openspec/changes/node-link-owner/reports/wp2-fix1-handoff.md"
    result: PASS
    evidence_status: REUSED
    applicability_basis: "同 F1 的来源与目标版本。F2 的修复改 host.rs 的 HostPolicy::new（public_origin 无条件校验）+ 1 条新用例；被 review 判定为「不可达」的 NetError::InvalidPublicOrigin 在白名单非空组合下现已可达（回归实测：旧实现 FAILED → 新实现 ok）。该改动同时是 host.rs 内行为变化，因此 review 中「HostPolicy 三分支口径与裁决一致」这条 Correct 的适用范围需要在 RV2 上按新版本重核（三分支判定逻辑本身未变，变的是构造期校验）。"
    source_evidence:
      id: "RV1-WP2"
      report_path: "openspec/changes/node-link-owner/reports/rv1-wp2.md"
      target_revision: "4e8a1075a48e004808f498447b54e45dbff824ce"
  - task_id: "RV1-WP2-F3"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "50a5af420ec5c25e521dd93aeb3dc9bf3a732ab5"
    evidence_type: REVIEW
    evidence_id: "RV1-WP2"
    report_path: "openspec/changes/node-link-owner/reports/wp2-fix1-handoff.md"
    result: PASS
    evidence_status: REUSED
    applicability_basis: "同 F1 的来源与目标版本；F3 按主 Agent 裁决走方案 (a)：listener.rs 的 direct 模式握手改为有界并发池（64 槽 + JoinSet + 有界结果队列，单次超时仍 10s）。这是本轮最大的一处行为改动，review 中「异步任务所有者与取消路径」与「TLS 失败关闭」两条 Correct 需在 50a5af42 上由 RV2 重核（本轮的替代证据：新增 F3 回归用例在旧实现 FAILED / 新实现 ok，且既有 direct/shutdown 用例全绿）。"
    source_evidence:
      id: "RV1-WP2"
      report_path: "openspec/changes/node-link-owner/reports/rv1-wp2.md"
      target_revision: "4e8a1075a48e004808f498447b54e45dbff824ce"
  - task_id: "N2"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "50a5af420ec5c25e521dd93aeb3dc9bf3a732ab5"
    evidence_type: REVIEW
    evidence_id: "RV1-WP2"
    report_path: "openspec/changes/node-link-owner/reports/wp2-fix1-handoff.md"
    result: PASS
    evidence_status: REUSED
    applicability_basis: "N2 是 RV1-WP2 的 report-only note（非 Findings），本轮按派单补覆盖：新增 wrong_host_is_rejected_on_the_ws_upgrade_path（/node-link/v1 上错误 Host → 400 且处理器零调用，合法 Host → 101）。这是新版本上的新增用例，其 PASS 只表示该用例在本轮全绿；note 的闭环判定仍归 RV2。"
    source_evidence:
      id: "RV1-WP2"
      report_path: "openspec/changes/node-link-owner/reports/rv1-wp2.md"
      target_revision: "4e8a1075a48e004808f498447b54e45dbff824ce"
  - task_id: "N3"
    role: coder
    phase: fix
    stage: work-package
    target_revision: "50a5af420ec5c25e521dd93aeb3dc9bf3a732ab5"
    evidence_type: REVIEW
    evidence_id: "RV1-WP2"
    report_path: "openspec/changes/node-link-owner/reports/wp2-fix1-handoff.md"
    result: PASS
    evidence_status: REUSED
    applicability_basis: "N3 同为 report-only note，本轮把它从「未定论」钉成有依据的口径：读 axum 0.8.9 源码确认升级回填用 get_all + 逗号切分 + trim + contains（与本层 offers_subprotocol 同口径），并补 route.rs 单测断言与 tests.rs 端到端双头用例（含 (other, required) 探针：若上游改为只读第一个头值，该断言会失败）。口径『按单一 token 口径接受』由此锁定；note 的闭环判定归 RV2。"
    source_evidence:
      id: "RV1-WP2"
      report_path: "openspec/changes/node-link-owner/reports/rv1-wp2.md"
      target_revision: "4e8a1075a48e004808f498447b54e45dbff824ce"
```

## 遗留与下一步

- **不在本轮范围、需要下游处理的提醒**：
  - N1/N7（WP8 文档收口）：`docs/MODULE_ARCHITECTURE.md` §4.9 的 `[现状]` 尚未包含 `transport::net`；`docs/CONFIG_REFERENCE.md` §1 还需显式写明「两者都空 → 只接受 loopback 形态 Host」。本轮未改 `docs/**`（写入范围仅 `net/**`）。
  - N5（WP3/WP4）：认证限流器的准入闸门必须在 `node_link` 真正接线，WP4 的 RV1 需逐条核对拒绝点与计数键。
  - N4、N6：本轮未触及（前者是非授权面组合，后者是 PV1/PV2 证据对齐）。
- **建议下一步**：对 `50a5af420ec5c25e521dd93aeb3dc9bf3a732ab5` 做 RV2 只读复验，重点看 F2 的构造期校验与 F3 的握手池（任务所有权/取消/背压下界、direct 语义与关闭序列），并确认 N2/N3 两条 note 已闭环。
- **未执行（如实）**：`npm run verify` 的整 workspace `cargo test`（本轮只按派单跑 `-p server`；WP2 的 PV3 全量证据在 `reports/wp2-transport-net.log` §3 与 `reports/du1-pv1.log`）；Linux `cfg(unix)` 用例（Windows 本机不编译，由 Linux CI 覆盖）；CI 的 `deps`/`advisories`/`secrets`（本地无等价物）；任何 E2E。
