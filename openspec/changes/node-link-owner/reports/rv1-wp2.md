# RV1-WP2 Review 报告（变更 `node-link-owner` / WP2「`server::transport::net`」）

> 说明：本轮 reviewer 子 Agent 无写文件工具（只读检视），报告由主 Agent 按其返回正文原样持久化到本路径。

## Shared Report

- **task_id**: `2.4` / `2.5` / `2.6`（WP2，交付单元 DU1）；附带复核 `RV1-WP1-F1` / `RV1-WP1-F3` / `RV1-WP1-F4`
- **role**: reviewer（独立代码检视子 Agent，只读；未参与 WP2 实现，未参与实现讨论）
- **phase**: review；**stage**: work-package（工作包交付前）
- **agent_context**: 本轮为新建的任务级 reviewer 子 Agent（ac26e1d8-cb60-47f4-a293-a9b5ded9ca03；隔离上下文启动；只能说明收到的输入与自身限制，不能自证宿主未注入其他上下文）。工作目录 `D:\Project\acp-remote-wt\node-link-owner`；未修改任何文件、未切换分支、未提交、未创建端口/进程/临时目录。权威规划根 `D:\Project\acp-remote`（只读）。
- **base_revision**: `af9064ed6dcd3e88ab76b825253fb2dc95b74967`；**target_revision**: `4e8a1075a48e004808f498447b54e45dbff824ce`
- **版本稳定性**：`watchdog_diff` 报工作区相对 4e8a1075 无改动 → 工作区干净且 = target revision。
- **scope（实际检查范围）**：`crates/server/src/transport/net/**`（14 个文件，逐行读完）、`crates/server/src/transport/mod.rs`、`crates/server/src/lib.rs`、`crates/server/Cargo.toml`；为复核 F1/F3/F4 另读根 `Cargo.toml` 的 rcgen 注释块与 `docs/MODULE_ARCHITECTURE.md` 头部；契约侧读 `docs/NODE_LINK_PROTOCOL.md` §2.1/§2.5/§13.2/§13.3/§13.4、`docs/CONFIG_REFERENCE.md` §1/§3、`docs/SECURITY_DESIGN.md` §7.1/§7.2/§7.3/§13.2/§14.1/§15/§20、`docs/CORE_PORTS_AND_STORAGE.md` §6/§7.1、`docs/MODULE_ARCHITECTURE.md` §3.1/§4.9/§5；对照实现读 `crates/storage-sqlite/src/migrate.rs`、`crates/app/src/config.rs`。
- **changes**：新增 `transport/net` 14 文件 + `transport/mod.rs` 的 `pub mod net;` + `lib.rs` 现状注记，共 16 个 `crates/server/src/**` 文件（自报 4318 insertions / 7 deletions）；`crates/server/Cargo.toml` 无改动。**工具集无 shell/Git 权限，无法独立重算 base→target 的逐文件 diff**（限制，不是覆盖声明）。
- **checks**：
  - `[PV3]` 在目标版本满足：`wp2-transport-net.log` §3/§8 记录 161 lib + 14/6/4/4 集成 + 0 doc-tests，全绿；net 用例 72 个（按模块逐一点数：config 2 + host 8 + http 3 + listener 4 + permissions 4 + proxy 8 + ratelimit 6 + route 4 + shutdown 3 + tls 3 + ws 3 + tests 24 = 72），无 ignored / 无全跳过。
  - `[PV5]`（Windows 轮次）满足 WP2 部分：`pv5-windows-nodelink.log` 记录权限用例 1 passed 与 net 整轮 72 passed；Linux 专属 `cfg(unix)` 用例本机不编译，如实记为未执行（由 Linux CI 覆盖）。
  - `[PV1]` / `[PV2]`：见 Assessment 的 N6（证据位置在 `du1-pv1.log` 的 S9–S13 段，绑定目标版本；末段截断）。
  - `[PV4]`：不适用（WP7 的 `app` 侧）。
- **issues**: 无阻断；3 条 MINOR（P2，见 Findings）；6 条非阻断 note；3 项未验证/限制。
- **result**: **PASS**（对应 `4e8a1075…`；不代表候选验证、E2E 或合并已完成，也不代表整个变更可归档）
- **evidence_paths**：本报告 `reports/rv1-wp2.md`；检视对象 `crates/server/src/transport/net/**` 等；证据 `reports/wp2-handoff.md`、`reports/wp2-transport-net.log`、`reports/pv5-windows-nodelink.log`、`reports/du1-pv1.log`（S9–S13 段）、`reports/rv1-wp1.md`、`reports/wp1-fix1-handoff.md`、`reports/wp1-deps.log`
- **resource_cleanup**: 本轮无资源创建；未写任何文件。

## handoff_index

```yaml
handoff_index:
  - task_id: "2.4"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: "4e8a1075a48e004808f498447b54e45dbff824ce"
    evidence_type: REVIEW
    evidence_id: RV1-WP2
    report_path: "openspec/changes/node-link-owner/reports/rv1-wp2.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在固定 HEAD=4e8a1075…（工作区干净）上逐行只读检视 transport::net 的路由/升级/Host/代理头/请求体上限实现与 24 个集成用例；对应 R5–R11、R16–R18。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.5"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: "4e8a1075a48e004808f498447b54e45dbff824ce"
    evidence_type: REVIEW
    evidence_id: RV1-WP2
    report_path: "openspec/changes/node-link-owner/reports/rv1-wp2.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "同一版本上检视 TLS proxy/direct 失败关闭与私钥边界（tls.rs/permissions.rs/listener.rs）、单消息与请求体上限；对应 R12–R18。Linux cfg(unix) 分支本机不可执行，如实记为未验证。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.6"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: "4e8a1075a48e004808f498447b54e45dbff824ce"
    evidence_type: REVIEW
    evidence_id: RV1-WP2
    report_path: "openspec/changes/node-link-owner/reports/rv1-wp2.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "交付前冻结形状检视：公开 API 面（mod.rs 再导出、HttpHandler/WsHandler/NetListener/WsConnection/PeerInfo/Shutdown/限流器常量）与 WP3/WP4 的消费方式；发现 3 条非阻断项（F1–F3）。"
    source_evidence: NOT_APPLICABLE
  - task_id: "RV1-WP1-F1"
    role: reviewer
    phase: recheck
    stage: work-package
    target_revision: "4e8a1075a48e004808f498447b54e45dbff824ce"
    evidence_type: REVIEW
    evidence_id: RV1-WP1
    report_path: "openspec/changes/node-link-owner/reports/rv1-wp2.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "原问题由 RV1-WP1 在 028a4d2a 上提出；本轮在 af9064ed→4e8a1075 的版本上重读 Cargo.toml 注释与 wp1-deps.log §11 第 4 条勘误，结论=F1 已解决。"
    source_evidence: NOT_APPLICABLE
  - task_id: "RV1-WP1-F3"
    role: reviewer
    phase: recheck
    stage: work-package
    target_revision: "4e8a1075a48e004808f498447b54e45dbff824ce"
    evidence_type: REVIEW
    evidence_id: RV1-WP1
    report_path: "openspec/changes/node-link-owner/reports/rv1-wp2.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "同 F1 的版本口径；重读 docs/MODULE_ARCHITECTURE.md:6 的 2026-09-26 修订记录行，结论=F3 已解决。"
    source_evidence: NOT_APPLICABLE
  - task_id: "RV1-WP1-F4"
    role: reviewer
    phase: recheck
    stage: work-package
    target_revision: "4e8a1075a48e004808f498447b54e45dbff824ce"
    evidence_type: REVIEW
    evidence_id: RV1-WP1
    report_path: "openspec/changes/node-link-owner/reports/rv1-wp2.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "同 F1 的版本口径；重读 reports/wp1-deps.log §7.1 的指针句，结论=F4 已解决（该日志不入版本控制，为人工比对）。"
    source_evidence: NOT_APPLICABLE
```

## Findings

| ID | 级别 | Location | Trigger / Evidence | Impact | Recommendation | Recheck |
|---|---|---|---|---|---|---|
| RV1-WP2-F1 | MINOR（P2） | `crates/server/src/transport/net/mod.rs:15-16`（文档断言） vs `net/http.rs:39-60,74,84`、`net/ws.rs:32-36,132` | 模块文档写「不把 `axum` 类型暴露给上层」，但公开 API 的字段/参数就是 axum 再导出的值类型（`Method`/`HeaderMap`/`Bytes`/`StatusCode` 等），而 `net/mod.rs` 未再导出它们；WP3 必然要在 `node_link` 里写 `axum::http::StatusCode` 之类路径 | 非行为缺陷、不破坏 §5 依赖方向；影响是文档与公开面口径不一致 | (a) 把该句改成如实表述；或 (b) 在 `net/mod.rs` 再导出这批值类型 | 不适用（本轮新发现） |
| RV1-WP2-F2 | MINOR（P2） | `crates/server/src/transport/net/host.rs:44-63`；用例缺口 `host.rs:234-240` | `HostPolicy::new` 只在「`allowed_hosts` 为空」分支校验 `public_origin`；白名单非空时完全跳过，`HostPolicy::new(Some("not-an-origin"), &["proxy.internal"])` 返回 `Ok`，`NetError::InvalidPublicOrigin` 在该组合下不可达 | 非法 `public_origin` 在「配了 `allowed_hosts`」的正式配置里被静默接受，配置错误推迟到下游暴露 | `public_origin: Some(_)` 无条件做一次 `origin_host(...)` 校验，再走原三分支；补一条该组合必须 `Err` 的用例 | 不适用（本轮新发现） |
| RV1-WP2-F3 | MINOR（P2，可由主 Agent 决定是否升级） | `crates/server/src/transport/net/listener.rs:473-483`（`NetAcceptor::accept`） | `direct` 模式下 TLS 握手在 accept 循环内联完成（超时 10s），慢连接/慢握手可把新连接接入速率压到 ≈1/10s；已建立连接不受影响 | `direct` 模式下预先认证的可用性削弱（无需凭据即可显著延迟其他客户端接入）；不涉及越权或既有连接 | (a) 握手移出 accept 关键路径（有界并发握手池 + 有界队列回给 axum）；或 (b) 维持内联但下调超时并在文档写明取舍。主 Agent 决定：**已裁决按 (a) 修复** | 不适用（本轮新发现） |

**未发现** P0/P1 级问题：无 specs 需求违反、无依赖方向破坏、无秘密进日志/错误/Debug、无 `unsafe`、无证据造假或证据错用。

### Correct（已核实无误，含证据）

1. **HostPolicy 三分支口径与主 Agent 裁决一致，注释引用 `SECURITY_DESIGN.md` §7.2**；三分支各有单测 + 真实 listener 拒绝用例；Host 缺失/重复/非 ASCII/含空白或逗号一律拒绝。
2. **Host 边界不可绕过（结构层面）**：边界中间件覆盖全部路由（含 fallback），先 Host 判定后插入 `PeerInfo`；处理器拿不到 `PeerInfo` 返回 500（失败关闭）。
3. **压缩/subprotocol 拒绝逐条落地**：`Sec-WebSocket-Extensions` 多值解析并在 upgrade 前 400；subprotocol 精确 token 口径；101 回填且无 extensions 头；axum 未开任何 deflate feature。
4. **上限在分配前生效**：`max_message_size`/`max_frame_size` 绑定配置值，1009 端到端断言；配对请求体超限即 413 且处理器零调用；64 KiB 请求体默认值写明依据；0 上限失败关闭。
5. **TLS 失败关闭与私钥边界**：bind 固定顺序失败即 Err；权限判定 `OwnerOnly/Relaxed/Unverifiable`（Relaxed 拒绝启动）；错误不携带文件内容；`TlsMode::Direct` 手写 `<redacted>` Debug；provider 固定 ring、ALPN 只留 http/1.1；平台口径与 CORE_PORTS §7.1 同构。
6. **异步任务所有者与取消路径**：会话 future 经有界 channel 进 `JoinSet`，关闭时停止接收→宽限等待→超时 abort，无 detached 任务；HTTP 侧 graceful shutdown；用例证明关闭后端口释放。
7. **与协议层解耦**：net 无 `node-link-protocol`/`acpr_wire`/`identity_auth`/`core::` import（grep 零命中）；不写死协议 path/subprotocol。
8. **无 unsafe、clippy/fmt 绿**（独立 grep 复核）。
9. **证据真实性核对**：72 个 net 用例名与源码逐名吻合；未执行项如实列出。

## 复核 RV1-WP1-F1 / F3 / F4（基线 `af9064ed`，按原问题 ID）

| 原问题 ID | 原级别 | 结论 | 复核依据 | 回归检查 |
|---|---|---|---|---|
| RV1-WP1-F1 | P2 | **已解决** | Cargo.toml rcgen 注释块已改为可核对表述；wp1-deps.log §11 第 4 条已加勘误行 | 无版本/feature/锁文件改动；check:boundaries 仍 exit=0 |
| RV1-WP1-F3 | SUGGESTION | **已解决** | MODULE_ARCHITECTURE.md:6 已有 2026-09-26 修订记录行，格式与既有各行同构 | check:doc-links 通过（378 links / 4066 section refs） |
| RV1-WP1-F4 | SUGGESTION | **已解决** | wp1-deps.log §7.1 已加 amend 指针句 | 日志不入版本控制，人工比对一致 |

**附带（供主 Agent 参考）**：RV1-WP1-F2 在「文件单一写入者」行已覆盖，但 plan.md WP1 行的 `Write Scope` 列仍只列三个文件——是否补全由主 Agent 决定。

## Assessment（Check ID 与重点关注项逐条）

| Check / 重点项 | 已核对的证据 | 差异 / 待补项 | 是否影响本轮判断 |
|---|---|---|---|
| [PV3] | wp2-transport-net.log §3/§5/§8 + du1-pv1.log S10/S11b（目标版本，PASS；189 用例全绿） | 无 | 否 |
| [PV5] | pv5-windows-nodelink.log（WP2 部分 PASS） | Linux cfg(unix) 分支与真实第二账号 ACL 未执行 | 否 |
| [PV1]/[PV2] | du1-pv1.log S9–S13 绑定目标版本，PV1_EXIT_CODE=0 | 该文件前半段绑定 028a4d2a、末段截断；本轮不把 PV1/PV2 记为满足，由主 Agent 对齐 3.3 证据口径 | 否（候选/合并前必须补齐对齐） |
| [PV4] | 不适用（WP7） | — | 否 |
| 重点 1：HostPolicy 裁决落地 | Correct 1/2 | F2（P2） | 否 |
| 重点 2：TLS 失败关闭与私钥边界 | Correct 5 | 无 | 否 |
| 重点 3：Host/代理头不可绕过 | Correct 2；proxy.rs 8 单测 + 2 真实连接用例 | N4 组合形态（非授权面） | 否 |
| 重点 4：压缩/subprotocol 拒绝 | Correct 3 | N3 双头边界未验证 | 否 |
| 重点 5：上限在分配前生效 | Correct 4 | 无 | 否 |
| 重点 6：无 unsafe | Correct 8 | 无 | 否 |
| 重点 7：解耦约束 | Correct 7 | F1（P2） | 否 |
| 重点 8：异步任务所有者/取消/排空 | Correct 6 | F3（P2） | 否 |
| 附带复核 F1/F3/F4 | 三项均「已解决」 | 无 | 否 |

### 非阻断 note（report-only）

- **N1**：`docs/MODULE_ARCHITECTURE.md` §4.9 `[现状]` 未包含本 WP 已落地的 `transport::net`——计划归 WP8 收口，WP8 必须补（并按惯例在文件头加修订记录行）。
- **N2**：Host 拒绝用例未覆盖 `/node-link/v1` 升级路径（只测了配对 path 与未知 path）；建议补一条 WS path 的 400 断言。
- **N3**：`Sec-WebSocket-Protocol` 双头边界与 axum 回填口径差未能定论；建议补一条双头用例钉死。
- **N4**：非 loopback + 无 public_origin/allowed_hosts 组合下 Host 判定退化为客户端可控（非授权面，不构成越权；CONFIG_REFERENCE §1 三形态表本就不支持该组合）。
- **N5（跨 WP 待办，供 WP4）**：认证限流 10/min/IP 的机制已交付（`SlidingWindowLimiter`），但准入闸门必须在 WP3/WP4 真正接线；WP4 的 RV1 需逐条核对拒绝点与计数键。
- **N6**：PV1/PV2 证据归属见 Assessment；3.3 的对齐证据待返回。
- **N7**：WP8 补 CONFIG_REFERENCE §1 条文时把「两者都空 → 只接受 loopback 形态 Host」显式写入并指向 §7.2。

### 未验证内容与限制（必须与结论一起读）

1. base→target 逐文件 diff 未独立重算（无 shell/Git；残余风险：scope 外的改动本轮不可见；主 Agent 可用 `git show --stat 4e8a1075` 一行确认）。
2. 未执行任何命令；PV* 原始输出由实现者/验证者提供，本轮只做静态可信性核对。code review PASS 不等于 Project Verify PASS。
3. 平台/环境未覆盖：Linux cfg(unix) 权限路径（Linux CI 覆盖）、真实跨用户 ACL、非 loopback 外部可达性、真实第三方客户端 TLS 互操作；cargo-deny/gitleaks 只在 CI。

## 结论

- **判定：PASS**（Target Revision `4e8a1075a48e004808f498447b54e45dbff824ce`）。无未解决 CRITICAL/MAJOR；3 条 MINOR 与 7 条 note 不阻断 WP2 交付前放行。
- 主 Agent 对 F3 的裁决：**按 (a) 修复**（握手移出 accept 关键路径，有界并发握手池）。
- 必须在候选/合并前补齐：PV1/PV2 在目标版本上的证据对齐（N6）；CI 的 deps/advisories/secrets。
