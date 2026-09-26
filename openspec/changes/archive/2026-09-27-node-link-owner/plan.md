# Plan: node-link-owner

## Scope and Contracts

- Specs Revision: 本变更 `specs/node-link-listener/spec.md`、`specs/node-link-pairing-http/spec.md`、`specs/node-link-owner-server/spec.md`（2026-09-26 初版，24 项 ADDED 需求 / 59 个场景）+ `specs/storage-schema-v2-migration/spec.md`（MODIFIED 增量，RV1-WP3C-F1 驱动：版本目标 2→3 与 v2→v3 重建语义，1 需求 / 3 场景）。
- Design Revision: 本变更 `design.md`（2026-09-26 初版，决策 D1–D11）。
- Convergence Check: 一致。specs 的 24 项需求逐一映射到 D2–D11 的模块与机制；D4（catalog revision 来源）与 D1（栈选型核验）是实现期核实项，两个出口都不改变 specs 的外部行为约定。
- Skip Specs: no。

## Contract Changes

无（规划时点）。实现期若 D4 确认需要增补管理变更计数器，或栈核验导致依赖口径调整，按 `design.md` 的对应条目在同一变更内同步 `CORE_PORTS_AND_STORAGE.md`/`MODULE_ARCHITECTURE.md` 并记入 verification 的 Check Plan Changes。

## Coverage Index

> evidence 使用变更目录相对路径（`reports/…`），规划阶段可以尚不存在；正文提及同一批文件时写仓库相对完整路径以免混淆。

```agentic-coverage
version: 1
target_ref: refs/heads/main
rows:
  - id: R1
    source: { path: specs/node-link-listener/spec.md, heading: "### Requirement: 共享 listener 绑定与失败关闭" }
    tasks: ["2.4", "2.21"]
    checks: [PV3, PV4]
    evidence: [reports/wp2-transport-net.log, reports/wp7-app-wiring.log]
  - id: R2
    source: { path: specs/node-link-listener/spec.md, heading: "#### Scenario: 默认 loopback 启动", requirement: "### Requirement: 共享 listener 绑定与失败关闭" }
    tasks: ["2.4", "2.21"]
    checks: [PV3, PV4]
    evidence: [reports/wp2-transport-net.log, reports/wp7-app-wiring.log]
  - id: R3
    source: { path: specs/node-link-listener/spec.md, heading: "#### Scenario: 绑定失败即拒绝启动", requirement: "### Requirement: 共享 listener 绑定与失败关闭" }
    tasks: ["2.4", "2.21"]
    checks: [PV3, PV4]
    evidence: [reports/wp2-transport-net.log, reports/wp7-app-wiring.log]
  - id: R4
    source: { path: specs/node-link-listener/spec.md, heading: "#### Scenario: 非 loopback 监听告警", requirement: "### Requirement: 共享 listener 绑定与失败关闭" }
    tasks: ["2.4", "2.21"]
    checks: [PV3, PV4]
    evidence: [reports/wp2-transport-net.log, reports/wp7-app-wiring.log]
  - id: R5
    source: { path: specs/node-link-listener/spec.md, heading: "### Requirement: 按 path 的路由与 WebSocket 升级规则" }
    tasks: ["2.4"]
    checks: [PV3]
    evidence: [reports/wp2-transport-net.log]
  - id: R6
    source: { path: specs/node-link-listener/spec.md, heading: "#### Scenario: Node Link 端点正常升级", requirement: "### Requirement: 按 path 的路由与 WebSocket 升级规则" }
    tasks: ["2.4"]
    checks: [PV3]
    evidence: [reports/wp2-transport-net.log]
  - id: R7
    source: { path: specs/node-link-listener/spec.md, heading: "#### Scenario: Sync 路径明确不可用", requirement: "### Requirement: 按 path 的路由与 WebSocket 升级规则" }
    tasks: ["2.4"]
    checks: [PV3]
    evidence: [reports/wp2-transport-net.log]
  - id: R8
    source: { path: specs/node-link-listener/spec.md, heading: "#### Scenario: 压缩或错误 subprotocol 被拒绝", requirement: "### Requirement: 按 path 的路由与 WebSocket 升级规则" }
    tasks: ["2.4"]
    checks: [PV3]
    evidence: [reports/wp2-transport-net.log]
  - id: R9
    source: { path: specs/node-link-listener/spec.md, heading: "### Requirement: Host 与代理头边界" }
    tasks: ["2.4"]
    checks: [PV3]
    evidence: [reports/wp2-transport-net.log]
  - id: R10
    source: { path: specs/node-link-listener/spec.md, heading: "#### Scenario: Host 不匹配被拒绝", requirement: "### Requirement: Host 与代理头边界" }
    tasks: ["2.4"]
    checks: [PV3]
    evidence: [reports/wp2-transport-net.log]
  - id: R11
    source: { path: specs/node-link-listener/spec.md, heading: "#### Scenario: 不可信来源的转发头被忽略", requirement: "### Requirement: Host 与代理头边界" }
    tasks: ["2.4"]
    checks: [PV3]
    evidence: [reports/wp2-transport-net.log]
  - id: R12
    source: { path: specs/node-link-listener/spec.md, heading: "### Requirement: TLS 两种终止模式" }
    tasks: ["2.5", "2.21"]
    checks: [PV3, PV5]
    evidence: [reports/wp2-transport-net.log, reports/pv5-windows-nodelink.log]
  - id: R13
    source: { path: specs/node-link-listener/spec.md, heading: "#### Scenario: direct 模式正常终止 TLS", requirement: "### Requirement: TLS 两种终止模式" }
    tasks: ["2.5"]
    checks: [PV3, PV5]
    evidence: [reports/wp2-transport-net.log, reports/pv5-windows-nodelink.log]
  - id: R14
    source: { path: specs/node-link-listener/spec.md, heading: "#### Scenario: direct 模式证书缺失即拒绝启动", requirement: "### Requirement: TLS 两种终止模式" }
    tasks: ["2.5", "2.21"]
    checks: [PV3, PV4, PV5]
    evidence: [reports/wp2-transport-net.log, reports/wp7-app-wiring.log, reports/pv5-windows-nodelink.log]
  - id: R15
    source: { path: specs/node-link-listener/spec.md, heading: "#### Scenario: 非 loopback 明文被拒绝", requirement: "### Requirement: TLS 两种终止模式" }
    tasks: ["2.5", "2.21"]
    checks: [PV3, PV4]
    evidence: [reports/wp2-transport-net.log, reports/wp7-app-wiring.log]
  - id: R16
    source: { path: specs/node-link-listener/spec.md, heading: "### Requirement: 请求体与单消息上限" }
    tasks: ["2.5"]
    checks: [PV3]
    evidence: [reports/wp2-transport-net.log]
  - id: R17
    source: { path: specs/node-link-listener/spec.md, heading: "#### Scenario: 配对请求体超限", requirement: "### Requirement: 请求体与单消息上限" }
    tasks: ["2.5", "2.7"]
    checks: [PV3]
    evidence: [reports/wp2-transport-net.log, reports/wp3-pairing-http.log]
  - id: R18
    source: { path: specs/node-link-listener/spec.md, heading: "#### Scenario: 超大 WebSocket 消息被拒绝", requirement: "### Requirement: 请求体与单消息上限" }
    tasks: ["2.5"]
    checks: [PV3]
    evidence: [reports/wp2-transport-net.log]
  - id: R19
    source: { path: specs/node-link-pairing-http/spec.md, heading: "### Requirement: claim 成功路径" }
    tasks: ["2.7"]
    checks: [PV3, PV5]
    evidence: [reports/wp3-pairing-http.log, reports/pv5-windows-nodelink.log]
  - id: R20
    source: { path: specs/node-link-pairing-http/spec.md, heading: "#### Scenario: 合法 claim 进入待确认", requirement: "### Requirement: claim 成功路径" }
    tasks: ["2.7"]
    checks: [PV3, PV5]
    evidence: [reports/wp3-pairing-http.log, reports/pv5-windows-nodelink.log]
  - id: R21
    source: { path: specs/node-link-pairing-http/spec.md, heading: "#### Scenario: 重复 claim 被拒绝", requirement: "### Requirement: claim 成功路径" }
    tasks: ["2.7"]
    checks: [PV3]
    evidence: [reports/wp3-pairing-http.log]
  - id: R22
    source: { path: specs/node-link-pairing-http/spec.md, heading: "### Requirement: claim 失败语义与幂等重试" }
    tasks: ["2.7"]
    checks: [PV3]
    evidence: [reports/wp3-pairing-http.log]
  - id: R23
    source: { path: specs/node-link-pairing-http/spec.md, heading: "#### Scenario: proof 无效返回 401 且不泄露差异", requirement: "### Requirement: claim 失败语义与幂等重试" }
    tasks: ["2.7"]
    checks: [PV3]
    evidence: [reports/wp3-pairing-http.log]
  - id: R24
    source: { path: specs/node-link-pairing-http/spec.md, heading: "#### Scenario: 相同内容的网络重试幂等", requirement: "### Requirement: claim 失败语义与幂等重试" }
    tasks: ["2.7"]
    checks: [PV3]
    evidence: [reports/wp3-pairing-http.log]
  - id: R25
    source: { path: specs/node-link-pairing-http/spec.md, heading: "#### Scenario: 过期配对返回 410", requirement: "### Requirement: claim 失败语义与幂等重试" }
    tasks: ["2.7"]
    checks: [PV3]
    evidence: [reports/wp3-pairing-http.log]
  - id: R26
    source: { path: specs/node-link-pairing-http/spec.md, heading: "### Requirement: status 查询语义" }
    tasks: ["2.8"]
    checks: [PV3, PV5]
    evidence: [reports/wp3-pairing-http.log, reports/pv5-windows-nodelink.log]
  - id: R27
    source: { path: specs/node-link-pairing-http/spec.md, heading: "#### Scenario: 各业务状态都以 200 表达", requirement: "### Requirement: status 查询语义" }
    tasks: ["2.8"]
    checks: [PV3]
    evidence: [reports/wp3-pairing-http.log]
  - id: R28
    source: { path: specs/node-link-pairing-http/spec.md, heading: "#### Scenario: status 的 proof 无效返回 401", requirement: "### Requirement: status 查询语义" }
    tasks: ["2.8"]
    checks: [PV3]
    evidence: [reports/wp3-pairing-http.log]
  - id: R29
    source: { path: specs/node-link-pairing-http/spec.md, heading: "#### Scenario: 网络重试复用 nonce 返回原响应", requirement: "### Requirement: status 查询语义" }
    tasks: ["2.8"]
    checks: [PV3]
    evidence: [reports/wp3-pairing-http.log]
  - id: R30
    source: { path: specs/node-link-pairing-http/spec.md, heading: "### Requirement: 安全响应头与凭据边界" }
    tasks: ["2.7", "2.8"]
    checks: [PV3]
    evidence: [reports/wp3-pairing-http.log]
  - id: R31
    source: { path: specs/node-link-pairing-http/spec.md, heading: "#### Scenario: 响应头齐全", requirement: "### Requirement: 安全响应头与凭据边界" }
    tasks: ["2.7", "2.8"]
    checks: [PV3]
    evidence: [reports/wp3-pairing-http.log]
  - id: R32
    source: { path: specs/node-link-pairing-http/spec.md, heading: "#### Scenario: secret 按期清除", requirement: "### Requirement: 安全响应头与凭据边界" }
    tasks: ["2.8", "2.10"]
    checks: [PV3]
    evidence: [reports/wp3-pairing-http.log, reports/wp4-handshake.log]
  - id: R33
    source: { path: specs/node-link-pairing-http/spec.md, heading: "### Requirement: 配对限流" }
    tasks: ["2.8"]
    checks: [PV3]
    evidence: [reports/wp3-pairing-http.log]
  - id: R34
    source: { path: specs/node-link-pairing-http/spec.md, heading: "#### Scenario: claim 超限返回 429", requirement: "### Requirement: 配对限流" }
    tasks: ["2.8"]
    checks: [PV3]
    evidence: [reports/wp3-pairing-http.log]
  - id: R35
    source: { path: specs/node-link-pairing-http/spec.md, heading: "#### Scenario: status 超限返回 429", requirement: "### Requirement: 配对限流" }
    tasks: ["2.8"]
    checks: [PV3]
    evidence: [reports/wp3-pairing-http.log]
  - id: R36
    source: { path: specs/node-link-owner-server/spec.md, heading: "### Requirement: 握手准入与版本/feature 协商" }
    tasks: ["2.10"]
    checks: [PV3, PV5]
    evidence: [reports/wp4-handshake.log, reports/pv5-windows-nodelink.log]
  - id: R37
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: 正常握手完成", requirement: "### Requirement: 握手准入与版本/feature 协商" }
    tasks: ["2.10"]
    checks: [PV3, PV5]
    evidence: [reports/wp4-handshake.log, reports/pv5-windows-nodelink.log]
  - id: R38
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: 认证前发送业务消息", requirement: "### Requirement: 握手准入与版本/feature 协商" }
    tasks: ["2.10"]
    checks: [PV3]
    evidence: [reports/wp4-handshake.log]
  - id: R39
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: 必需 feature 未满足", requirement: "### Requirement: 握手准入与版本/feature 协商" }
    tasks: ["2.10"]
    checks: [PV3]
    evidence: [reports/wp4-handshake.log]
  - id: R40
    source: { path: specs/node-link-owner-server/spec.md, heading: "### Requirement: 节点双向认证与凭据状态" }
    tasks: ["2.10", "2.11"]
    checks: [PV3, PV5]
    evidence: [reports/wp4-handshake.log, reports/pv5-windows-nodelink.log]
  - id: R41
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: proof 无效被拒绝", requirement: "### Requirement: 节点双向认证与凭据状态" }
    tasks: ["2.10"]
    checks: [PV3]
    evidence: [reports/wp4-handshake.log]
  - id: R42
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: 已撤销节点连接被拒绝", requirement: "### Requirement: 节点双向认证与凭据状态" }
    tasks: ["2.10"]
    checks: [PV3]
    evidence: [reports/wp4-handshake.log]
  - id: R43
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: 未知节点不泄露存在性之外的能力", requirement: "### Requirement: 节点双向认证与凭据状态" }
    tasks: ["2.10"]
    checks: [PV3]
    evidence: [reports/wp4-handshake.log]
  - id: R44
    source: { path: specs/node-link-owner-server/spec.md, heading: "### Requirement: `node.ready` 的 limits 下调语义" }
    tasks: ["2.11"]
    checks: [PV3]
    evidence: [reports/wp4-handshake.log]
  - id: R45
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: 下调生效", requirement: "### Requirement: `node.ready` 的 limits 下调语义" }
    tasks: ["2.11"]
    checks: [PV3]
    evidence: [reports/wp4-handshake.log]
  - id: R46
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: 固定常量不可协商", requirement: "### Requirement: `node.ready` 的 limits 下调语义" }
    tasks: ["2.11"]
    checks: [PV3]
    evidence: [reports/wp4-handshake.log]
  - id: R47
    source: { path: specs/node-link-owner-server/spec.md, heading: "### Requirement: 信封与 connectionSequence 校验" }
    tasks: ["2.10"]
    checks: [PV3]
    evidence: [reports/wp4-handshake.log]
  - id: R48
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: 序号回退被拒绝", requirement: "### Requirement: 信封与 connectionSequence 校验" }
    tasks: ["2.10"]
    checks: [PV3]
    evidence: [reports/wp4-handshake.log]
  - id: R49
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: post_mvp 消息显式拒绝", requirement: "### Requirement: 信封与 connectionSequence 校验" }
    tasks: ["2.10"]
    checks: [PV3]
    evidence: [reports/wp4-handshake.log]
  - id: R50
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: 未知字段被拒绝", requirement: "### Requirement: 信封与 connectionSequence 校验" }
    tasks: ["2.10"]
    checks: [PV3]
    evidence: [reports/wp4-handshake.log]
  - id: R51
    source: { path: specs/node-link-owner-server/spec.md, heading: "### Requirement: Export 过滤的 catalog 投影" }
    tasks: ["2.13"]
    checks: [PV3, PV5]
    evidence: [reports/wp5-catalog-resource.log, reports/pv5-windows-nodelink.log]
  - id: R52
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: 按信任记录过滤", requirement: "### Requirement: Export 过滤的 catalog 投影" }
    tasks: ["2.13"]
    checks: [PV3, PV5]
    evidence: [reports/wp5-catalog-resource.log, reports/pv5-windows-nodelink.log]
  - id: R53
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: 超大批次稳定切分", requirement: "### Requirement: Export 过滤的 catalog 投影" }
    tasks: ["2.13"]
    checks: [PV3]
    evidence: [reports/wp5-catalog-resource.log]
  - id: R54
    source: { path: specs/node-link-owner-server/spec.md, heading: "### Requirement: attachment 申请与 generation 隔离" }
    tasks: ["2.14"]
    checks: [PV3, PV5]
    evidence: [reports/wp5-catalog-resource.log, reports/pv5-windows-nodelink.log]
  - id: R55
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: 重新 attach 使旧 generation 失效", requirement: "### Requirement: attachment 申请与 generation 隔离" }
    tasks: ["2.14"]
    checks: [PV3, PV5]
    evidence: [reports/wp5-catalog-resource.log, reports/pv5-windows-nodelink.log]
  - id: R56
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: 未导出会话不可 attach", requirement: "### Requirement: attachment 申请与 generation 隔离" }
    tasks: ["2.14"]
    checks: [PV3]
    evidence: [reports/wp5-catalog-resource.log]
  - id: R57
    source: { path: specs/node-link-owner-server/spec.md, heading: "### Requirement: 快照与 origin cursor 增量重放" }
    tasks: ["2.14"]
    checks: [PV3, PV5]
    evidence: [reports/wp5-catalog-resource.log, reports/pv5-windows-nodelink.log]
  - id: R58
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: 首次订阅走快照", requirement: "### Requirement: 快照与 origin cursor 增量重放" }
    tasks: ["2.14"]
    checks: [PV3, PV5]
    evidence: [reports/wp5-catalog-resource.log, reports/pv5-windows-nodelink.log]
  - id: R59
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: epoch 不一致被拒绝", requirement: "### Requirement: 快照与 origin cursor 增量重放" }
    tasks: ["2.14"]
    checks: [PV3]
    evidence: [reports/wp5-catalog-resource.log]
  - id: R60
    source: { path: specs/node-link-owner-server/spec.md, heading: "### Requirement: `resource.event` 的持久化顺序与保真" }
    tasks: ["2.15"]
    checks: [PV3, PV5]
    evidence: [reports/wp5-catalog-resource.log, reports/pv5-windows-nodelink.log]
  - id: R61
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: 先持久化后发布", requirement: "### Requirement: `resource.event` 的持久化顺序与保真" }
    tasks: ["2.15"]
    checks: [PV3, PV5]
    evidence: [reports/wp5-catalog-resource.log, reports/pv5-windows-nodelink.log]
  - id: R62
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: 未登记事件类型保留 raw", requirement: "### Requirement: `resource.event` 的持久化顺序与保真" }
    tasks: ["2.15"]
    checks: [PV3]
    evidence: [reports/wp5-catalog-resource.log]
  - id: R63
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: 超大内嵌内容显式降级", requirement: "### Requirement: `resource.event` 的持久化顺序与保真" }
    tasks: ["2.15"]
    checks: [PV3]
    evidence: [reports/wp5-catalog-resource.log]
  - id: R64
    source: { path: specs/node-link-owner-server/spec.md, heading: "### Requirement: `resource.ack` 的单调性与归属" }
    tasks: ["2.15"]
    checks: [PV3]
    evidence: [reports/wp5-catalog-resource.log]
  - id: R65
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: ACK 回退被拒绝", requirement: "### Requirement: `resource.ack` 的单调性与归属" }
    tasks: ["2.15"]
    checks: [PV3]
    evidence: [reports/wp5-catalog-resource.log]
  - id: R66
    source: { path: specs/node-link-owner-server/spec.md, heading: "### Requirement: 命令授权、幂等与终态" }
    tasks: ["2.17"]
    checks: [PV3, PV5]
    evidence: [reports/wp6-command.log, reports/pv5-windows-nodelink.log]
  - id: R67
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: 相同 requestId 重试不重复派发", requirement: "### Requirement: 命令授权、幂等与终态" }
    tasks: ["2.17"]
    checks: [PV3, PV5]
    evidence: [reports/wp6-command.log, reports/pv5-windows-nodelink.log]
  - id: R68
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: 同键不同语义被拒绝", requirement: "### Requirement: 命令授权、幂等与终态" }
    tasks: ["2.17"]
    checks: [PV3]
    evidence: [reports/wp6-command.log]
  - id: R69
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: 越权命令被拒绝", requirement: "### Requirement: 命令授权、幂等与终态" }
    tasks: ["2.17"]
    checks: [PV3, PV5]
    evidence: [reports/wp6-command.log, reports/pv5-windows-nodelink.log]
  - id: R70
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: 崩溃窗口进入 uncertain", requirement: "### Requirement: 命令授权、幂等与终态" }
    tasks: ["2.17"]
    checks: [PV3]
    evidence: [reports/wp6-command.log]
  - id: R71
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: 命令限流", requirement: "### Requirement: 命令授权、幂等与终态" }
    tasks: ["2.18"]
    checks: [PV3]
    evidence: [reports/wp6-command.log]
  - id: R72
    source: { path: specs/node-link-owner-server/spec.md, heading: "### Requirement: `session.create` 的硬约束与结果契约" }
    tasks: ["2.18"]
    checks: [PV3, PV5]
    evidence: [reports/wp6-command.log, reports/pv5-windows-nodelink.log]
  - id: R73
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: 正常创建并回传复合引用", requirement: "### Requirement: `session.create` 的硬约束与结果契约" }
    tasks: ["2.18"]
    checks: [PV3, PV5]
    evidence: [reports/wp6-command.log, reports/pv5-windows-nodelink.log]
  - id: R74
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: 禁带字段被拒绝且不创建会话", requirement: "### Requirement: `session.create` 的硬约束与结果契约" }
    tasks: ["2.18"]
    checks: [PV3]
    evidence: [reports/wp6-command.log]
  - id: R75
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: 未知 workspaceAlias 被拒绝", requirement: "### Requirement: `session.create` 的硬约束与结果契约" }
    tasks: ["2.18"]
    checks: [PV3]
    evidence: [reports/wp6-command.log]
  - id: R76
    source: { path: specs/node-link-owner-server/spec.md, heading: "### Requirement: 撤销的即时传播" }
    tasks: ["2.19"]
    checks: [PV3, PV5]
    evidence: [reports/wp6-command.log, reports/pv5-windows-nodelink.log]
  - id: R77
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: Export 撤销即断资源", requirement: "### Requirement: 撤销的即时传播" }
    tasks: ["2.19"]
    checks: [PV3, PV5]
    evidence: [reports/wp6-command.log, reports/pv5-windows-nodelink.log]
  - id: R78
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: 节点撤销即关连接", requirement: "### Requirement: 撤销的即时传播" }
    tasks: ["2.19"]
    checks: [PV3, PV5]
    evidence: [reports/wp6-command.log, reports/pv5-windows-nodelink.log]
  - id: R79
    source: { path: specs/node-link-owner-server/spec.md, heading: "### Requirement: 心跳、超时与慢连接隔离" }
    tasks: ["2.11"]
    checks: [PV3]
    evidence: [reports/wp4-handshake.log]
  - id: R80
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: 心跳超时关闭", requirement: "### Requirement: 心跳、超时与慢连接隔离" }
    tasks: ["2.11"]
    checks: [PV3]
    evidence: [reports/wp4-handshake.log]
  - id: R81
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: 慢连接不影响其他连接", requirement: "### Requirement: 心跳、超时与慢连接隔离" }
    tasks: ["2.11"]
    checks: [PV3]
    evidence: [reports/wp4-handshake.log]
  - id: R82
    source: { path: specs/node-link-owner-server/spec.md, heading: "### Requirement: 审计与日志边界" }
    tasks: ["2.11", "2.17"]
    checks: [PV3]
    evidence: [reports/wp4-handshake.log, reports/wp6-command.log]
  - id: R83
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: 拒绝留痕且不含秘密", requirement: "### Requirement: 审计与日志边界" }
    tasks: ["2.11", "2.17"]
    checks: [PV3]
    evidence: [reports/wp4-handshake.log, reports/wp6-command.log]
  - id: R84
    source: { path: specs/storage-schema-v2-migration/spec.md, heading: "### Requirement: 版本常量与 migration 幂等" }
    tasks: ["2.25", "2.26"]
    checks: [PV3]
    evidence: [reports/wp3-contract.log]
  - id: R85
    source: { path: specs/storage-schema-v2-migration/spec.md, heading: "#### Scenario: 连续两次打开 schema 文本不变", requirement: "### Requirement: 版本常量与 migration 幂等" }
    tasks: ["2.26"]
    checks: [PV3]
    evidence: [reports/wp3-contract.log]
  - id: R86
    source: { path: specs/storage-schema-v2-migration/spec.md, heading: "#### Scenario: 升级中途失败整体回滚", requirement: "### Requirement: 版本常量与 migration 幂等" }
    tasks: ["2.26"]
    checks: [PV3]
    evidence: [reports/wp3-contract.log]
  - id: R87
    source: { path: specs/storage-schema-v2-migration/spec.md, heading: "#### Scenario: v2 到 v3 升级保留审计并扩展词表", requirement: "### Requirement: 版本常量与 migration 幂等" }
    tasks: ["2.26"]
    checks: [PV3]
    evidence: [reports/wp3-contract.log]
  - id: R88
    source: { path: specs/node-link-pairing-http/spec.md, heading: "#### Scenario: secret 已清除后的终态查询", requirement: "### Requirement: status 查询语义" }
    tasks: ["2.8"]
    checks: [PV3]
    evidence: [reports/wp3-pairing-http.log]
```

## Work Packages

说明：Main E2E mode 为 `not-applicable`，因此不生成独立的测试工作包（TP）与 Test Design 分组；各工作包自带其行为测试（局部自检），受控路径全链路集成测试归入 WP7，最终替代验证由主 Agent 任务（7.1）统一执行并留证。

| ID | Goal / Scenarios | Dependencies | Owner | Reviewer | Branch / Worktree | Write Scope | Inputs / Outputs | Verification |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| WP1 | 合同口径与依赖登记：§5 矩阵 `server`→`acpr-wire` 格、§3.1/§4.9 口径、栈核验留证 | 无 | coder | 独立 reviewer | 变更分支 worktree | `docs/MODULE_ARCHITECTURE.md`、`Cargo.toml`、`deny.toml` | D1/D5 → 依赖口径 + `reports/wp1-deps.log` | PV1、PV2 |
| WP2 | `server::transport::net`：listener、路由、Host 边界、TLS 两模式、连接级上限（R1–R18） | WP1 | coder | 独立 reviewer | 同上 | `crates/server/src/transport/net/**`（新增） | D1/D2/D9/D10 → net 公共形状 | PV3、PV5 |
| WP3 | `server::node_link` 配对 HTTP：claim/status、安全头、限流（R19–R35）；**前置合同扩展（用户裁决 A，2026-09-26，design D12）**：core `Actor::PairingClaimant` 变体与配对通道用例入口、`consume_pairing` 写入口、`TrustStore` 消耗方法、`owned_audit`/`imported_audit` CHECK 的 v2→v3 重建与全部权威文档同步 | WP2 | coder | 独立 reviewer | 同上 | `crates/server/src/node_link/pairing*`、`crates/core/src/{model/identity.rs,use_cases.rs,ports.rs}`（限 D12）、`crates/storage-sqlite/src/**`（限 v3 迁移与 consume 落盘）、`docs/CORE_PORTS_AND_STORAGE.md`、`docs/SECURITY_DESIGN.md`（§14.2）、`docs/IDENTITY_AND_AUTH_CONTRACT.md`（§5.1） | D2/D3（状态机入口）+ D12 → HTTP 处理器 | PV3、PV5 |
| WP4 | 握手与连接生命周期：握手驱动、信封/序号、feature、limits、心跳/超时、认证限流（R36–R50、R79–R81、R32 首次认证清除）；**含合同 seam（任务 2.28，延续裁决 A 方向）**：core 节点握手只读视图与认证审计写入口 | WP2 | coder | 独立 reviewer | 同上 | `crates/server/src/node_link/conn*`、`crates/core/src/{use_cases.rs,ports.rs}`（限 2.28 窄入口）、`docs/CORE_PORTS_AND_STORAGE.md`（§4/§5） | D2/D3/D9 → 连接注册表公共形状 | PV3、PV5 |
| WP5 | catalog 与 resource：投影/revision、attach/generation、snapshot/replay、event 扇出、ack（R51–R65） | WP4 | coder | 独立 reviewer | 同上 | `crates/server/src/node_link/{catalog,resource}*` | D4/D5/D6 → 事件映射与扇出 | PV3、PV5 |
| WP6 | command 管线与撤销传播：授权/幂等/终态、session.create、限流、撤销缝（R66–R78、R82–R83） | WP4 | coder | 独立 reviewer | 同上 | `crates/server/src/node_link/command*`、`crates/server/src/local_admin/pairing.rs`（缝扩展） | D7/D8 → 命令管线与撤销通知 | PV3、PV5 |
| WP7 | app 组合根接线与受控路径全链路集成测试（R1–R4、R12–R15 的 daemon 侧；切片 5 验收闭环） | WP2–WP6 | coder | 独立 reviewer | 同上 | `crates/app/**`、`crates/server/tests/`（集成） | D11 → 接线 + `reports/wp7-integration.log` | PV4、PV5 |
| WP8 | 收口：状态写回（README/DEVELOPMENT_PLAN/AGENTS.md/MODULE_ARCHITECTURE 现状注记/CONFIG_REFERENCE unwired 收敛/CORE_PORTS 两处现状句）与全量验证 | WP1–WP7 | coder | 独立 reviewer | 同上 | `README.md`、`docs/DEVELOPMENT_PLAN.md`、`AGENTS.md`、`docs/CONFIG_REFERENCE.md`、`docs/MODULE_ARCHITECTURE.md`、`docs/CORE_PORTS_AND_STORAGE.md`（限两处现状句，RV1-WP8 观察项 O2 登记：该扩展经主 Agent 批准、不动 §5/§7） | 实现完成状态 → 文档一致 | PV1、PV2 |

## Execution Waves

| Wave | Work Packages | 依赖满足条件 | 并发上限 | 说明 |
| --- | --- | --- | --- | --- |
| W0 基线与口径（串行，单执行者） | 1.1 → WP1（2.1–2.3） | 已完成的仓库工作区 | 1 | 栈核验先于编码；矩阵格与依赖登记在本波冻结 |
| W1 transport::net | WP2（2.4–2.6） | W0 基线 + WP1 口径 | 1 | listener/路由/TLS 公共形状本波冻结，供 WP3/WP4 消费 |
| W2 配对 HTTP | WP3（2.25、2.26、2.27、2.7–2.9） | WP2 路由形状冻结 | 1 | D12 合同扩展（core/存储/文档）先行（2.25→2.26），seam 补全（2.27）与 HTTP 处理器在其后；与 WP4 可并行（共享只有 transport::net 的公开形状），默认串行 |
| W3 握手与连接 | WP4（2.28、2.10–2.12） | WP2 形状冻结 | 1 | 连接注册表与信封校验形状本波冻结 |
| W4 catalog/resource | WP5（2.13–2.16） | WP4 连接形状冻结 | 1 | 事件扇出与 digest 映射本波冻结 |
| W5 command/撤销 | WP6（2.17–2.20） | WP4 连接形状冻结 | 1 | 与 WP5 可并行（都只依赖 WP4 形状），默认串行；`local_admin/pairing.rs` 的缝扩展在本波由 WP6 单一写入 |
| W6 app 接线 + 全链路 | WP7（2.21–2.22） | WP2–WP6 完成 | 1 | 受控路径全链路集成测试在本波落地 |
| W7 收口 | WP8（2.23–2.24） | WP1–WP7 完成 | 1 | 已落地标记只在实现确实完成后写入 |
| W8 交付前验证 | 3.1–3.16 | W0–W7 完成 | 1 | PV1–PV5 与 RV1；review 必须由不继承实现对话的执行者完成 |
| W9 集成与合入 | 5.1–5.2 → 6.1–6.8 | 全部 3.x 完成 | 1 | `integrated` 单一交付单元 |
| W10 最终替代验证与验收 | 7.1–7.3 → 8.1 | 6.7、6.8 | 1 | `not-applicable` 的替代验证 + `[e2e-owned]` 门禁 + 最终验收 |

**多 Agent 分派规则**（宿主支持子 Agent / 可开独立会话时）：

- 每个 WP 一个独立 coder 执行者；构建目录用 `CARGO_TARGET_DIR` 隔离，不允许两个执行者共用同一个 `target/`。各波次存在顺序依赖（口径 → net → 协议层 → 接线 → 收口），默认**串行**；W2/W3、W4/W5 两对内部可并行的波次在有隔离构建目录时可以并行，无并发能力时按 roles 串行执行并如实记录，串行不改变完成条件与证据要求。
- 每个 WP 的交付前独立 review（RV1）必须由**不继承实现对话**的执行者完成；缺少隔离上下文时相关任务记 BLOCKED，不得以自审替代。
- 只有主 Agent 可写 `plan.md`/`tasks.md`/`verification.md`；子 Agent 返回结构化 handoff（固定提交、命令、日志路径、差异范围）。
- 文件单一写入者：`docs/MODULE_ARCHITECTURE.md`（WP1 口径，WP8 收口串行接管）、`Cargo.toml`/`Cargo.lock`/`deny.toml`/`crates/server/Cargo.toml`（WP1 依赖登记）、`crates/server/src/transport/net/**`（WP2）、`crates/server/src/node_link/**`（WP3/WP4/WP5/WP6 按子目录分工，波次串行）、`crates/core/src/{model/identity.rs,use_cases.rs,ports.rs}` 与 `crates/storage-sqlite/src/**`、`docs/CORE_PORTS_AND_STORAGE.md`、`docs/SECURITY_DESIGN.md`、`docs/IDENTITY_AND_AUTH_CONTRACT.md`（WP3 的 D12 合同扩展，任务 2.25/2.26）、`crates/server/src/local_admin/pairing.rs`（仅 WP6 扩展缝）、`crates/app/**` 与 `crates/server/tests/`（WP7）、`README.md`/`docs/DEVELOPMENT_PLAN.md`/`AGENTS.md`/`docs/CONFIG_REFERENCE.md`（WP8）。
- 网络测试只绑定 loopback 随机端口（`127.0.0.1:0` 或临时配置），不占用固定端口；与其它检查串行即可，无额外独占资源。

## Dependency Handoffs

| Downstream | Upstream | Accepted Revision / Evidence | Start Revision | Transfer / Inclusion Check | Invalidation |
| --- | --- | --- | --- | --- | --- |
| WP2 | WP1 | 2.1–2.3 的依赖口径（矩阵格、workspace 依赖登记）与 [PV1]/[PV2] 通过证据 | 变更分支起点 | `cargo metadata --no-deps` 可解析；`check:boundaries` 的 `server` 行含 `acpr-wire` 格 | 依赖口径或矩阵变化 → WP2–WP7 复验 |
| WP3 | WP2 | WP2 冻结的路由/upgrade 公开形状 + 3.3 的 [PV3] 证据 | W1 完成点 | `cargo build -p server` 只经 transport::net 公开项接入 HTTP 处理器 | 路由形状变化 → WP3 复验 |
| WP4 | WP2 | WP2 冻结的 WS 连接形状 + 3.3 的 [PV3] 证据 | W1 完成点 | 同上 | 连接形状变化 → WP4–WP6 复验 |
| WP5 | WP4 | WP4 冻结的连接注册表/信封校验形状 + 3.7 的 [PV3] 证据 | W3 完成点 | catalog/resource 只经 conn 模块公开项挂载 | 注册表形状变化 → WP5/WP6 复验 |
| WP6 | WP4 | 同 WP5 行 | W3 完成点 | 同上；`local_admin` 缝扩展不改变既有方法行为（既有 local_admin 测试保持绿） | 同上 |
| WP7 | WP2–WP6 | 各 WP 的 [PV3] 证据与冻结形状 | W5 完成点 | `cargo build -p app` 只使用 `server` 公开项；组合根不承载业务规则 | 任一上游形状变化 → WP7/WP8 复验 |
| WP8 | WP1–WP7 | [PV3]/[PV4]/[PV5] 证据与 RV1 报告 | W6 完成点 | 文档「已落地」陈述与 `cargo metadata` 实际一致 | 成员集合或能力状态变化 → WP8 复验 |

## Runtime Resources

| Checks / Work Packages | Resource | Isolation / Configuration | Exclusive Scheduling | Owner / Cleanup |
| --- | --- | --- | --- | --- |
| 全部 cargo 检查 | `target/` 构建目录 | 每个执行者独立 `CARGO_TARGET_DIR` | 不共享即无需排队 | 各执行者自建自清理 |
| PV3/PV4/PV5 集成用例 | loopback 端口 | 一律 `127.0.0.1:0` 随机端口，不写死端口 | 无需独占 | 用例结束随进程释放 |
| PV3/PV4/PV5 集成用例 | Daemon 临时数据目录（SQLite、锁文件、自签证书 PEM） | 每用例自建临时目录 | 无需独占 | 用例结束删除；异常残留只删自身创建的目录 |
| WP1 | crates.io index 访问（核验与锁定） | 本机网络；失败如实记录重试次数 | 串行 | WP1 负责人 |

本变更不涉及数据库服务、容器、外部账号或云资源；loopback listener 是进程内资源。

## Target Repository and Main Branch

- Code Repository: `D:\Project\acp-remote`
- Target Ref: `refs/heads/main`（已核实的本地主分支引用；核实证据见 verification 的运行时基线）
- Version Confirmation Owner: 主 Agent（机械核实可交 environment/recon，目标选择由主 Agent 确认）
- Confirmation Method / Evidence: `git rev-parse refs/heads/main`、`git status --porcelain`、`git worktree list`，结果写入 `verification.md` 与 6.1 的核实记录

## Merge Strategy

`integrated`：八个工作包共享 `crates/server` 与 `crates/app` 两个 crate，存在接口与交付依赖（D2 的模块间形状、WP7 的接线依赖全部上游），不确定也无法独立交付，因此单一交付单元 DU1 包含 WP1–WP8，一次候选验证、一次合入。

| Delivery Unit / Mode | WP / TP | Owners: Implementation / Test / Review / Merge | Start / Readiness | Candidate Checks / Critical E2E | Order |
| --- | --- | --- | --- | --- | --- |
| DU1 / integrated | WP1–WP8（无 TP） | coder / 各 WP 自带测试 / 独立 reviewer（RV1）/ 独立集成 Agent | 全部 3.x 完成且证据有效 | [PV1]–[PV5]；E2E not-applicable（替代验证见 7.1） | 唯一单元 |

- Integration Branch / Worktree: 独立集成 worktree（绝对路径在 5.1 交接时登记），基于已核实的 `refs/heads/main` 构造候选
- Source Revision / Handoff: 固定源提交 + 各 WP 已验收提交（verification 的 Dependency Handoffs 记录）
- Local Merge Conditions / Report Path: 候选 [PV1]/[PV2] 全绿 + RV1 无阻断 + 基线复核一致；报告写入 `reports/du1-merge.md`；满足后直接本地合入 `refs/heads/main`，无须再次询问用户

## Verification Strategy

### Local Checks

- 开发中快速检查：`cargo test --locked -p server --all-features`、`cargo test --locked -p app --all-features`、`cargo fmt --all -- --check`、`cargo clippy --locked -p <crate> --all-targets --all-features -- -D warnings`。
- 合同门禁：`npm run check`（含 `check:doc-links`、`check:contract-drift`、`check:command-catalog`、`check:boundaries` 等，门禁清单以 `package.json` 的 `check` 脚本为准）。
- 结构自检：`rg -n "unsafe" crates/server/src crates/app/src` 零命中；`server::node_link` 只经 `core::use_cases` 与 `identity-auth` 公开入口（review 核对 import）。

### Project Verify

| Check ID | Stages / Work Packages | Command / Working Directory | Configuration / Environment | Scope / Pass Criteria | Evidence |
| --- | --- | --- | --- | --- | --- |
| PV1 | 分支、候选、主分支 | `npm run verify` / 仓库根 | rust-toolchain.toml 固定工具链；Node ≥ 22.12 | 合同门禁 + fmt/clippy/全 workspace 测试全绿 | `reports/du1-pv1.log` |
| PV2 | 分支、候选、主分支 | `node scripts/check-crate-boundaries.mjs` / 仓库根 | 同上 | §5 矩阵逐条通过（含新增 `server`→`acpr-wire` 格） | `reports/du1-pv1.log` |
| PV3 | WP2–WP6 交付前、候选 | `cargo test --locked -p server --all-features` / 仓库根 | 独立 `CARGO_TARGET_DIR` | 全绿且无零用例/全跳过；schema/fixture 漂移测试消费同一 manifest | `reports/wp2-*.log` … `reports/wp6-*.log` |
| PV4 | WP7 交付前、候选 | `cargo test --locked -p app --all-features` / 仓库根 | 同上 | 全绿；配置接线、启动/关闭、`daemon.status.listen` 用例真实执行 | `reports/wp7-app-wiring.log` |
| PV5 | WP2–WP7 交付前、候选（Windows 本机） | `cargo test --locked -p server -p app --all-features` 的 `cfg(windows)` 与全链路用例 / 本机 | 本机 Windows；临时数据目录与自签证书 | 受控路径全链路（配对→握手→catalog→attach→event→command→撤销）与平台差异用例真实执行并留证 | `reports/pv5-windows-nodelink.log`、`reports/wp7-integration.log` |

### Code Review

- RV1（每个 WP 一次）：新建不继承实现对话的只读 reviewer 子 Agent，按 `roles/reviewer.md` 检视固定版本。关注点分层：WP1（矩阵格与依赖登记、§20 核验证据真实性）；WP2（TLS 失败关闭、Host 边界不可绕过、压缩/subprotocol 拒绝）；WP3（401 不泄露差异、幂等重试、安全头、secret 生命周期）；WP4（握手只经 `Authority` 入口、验签公钥只来自持久化快照、序号/信封规则逐条）；WP5（先持久化后发布、raw 保真、digest 用 ACPR-CJ1、快照无正文）；WP6（幂等键完整性、越权拒绝无副作用、撤销推送不回滚提交、审计边界）；WP7（组合根零业务规则、关闭顺序、集成测试真实性）；WP8（文档与实际一致、未把未落地能力写成已落地）。
- 阻断标准：违反 specs 任一需求、依赖方向破坏、秘密进日志/错误、unsafe、证据造假；修复后由新子 Agent 复核。报告写入 `reports/rv1-wp<N>.md`。

### Main E2E

```yaml
mode: not-applicable
reason: "本切片落地后仍没有可端到端运行的产品路径：`node-link-client` 与 `server::acp_facade` 属切片 6（未实现），不存在第二个真实节点或真实 ACP Client 可以驱动产品闭环；本变更的最大可运行闭环是「脚本化 fake Access 客户端 ↔ 真实 Daemon 的 server::node_link」，它属于 Rust 集成测试（PV3/PV5），不是产品级 E2E。Sync/PWA 属切片 7。"
basis: "`openspec/config.yaml` 的 context 规则（当前阶段没有可端到端运行的产品路径时按变更记 not-applicable 并逐变更批准）与 `x-agentic.e2e.command` 为空的事实；切片 2/3/4 的同款处理已归档（`2026-09-24-acp-boundary-and-agent-host`、`2026-09-24-identity-auth-and-keystore`、`2026-09-25-daemon-cli-and-local-admin`）。本变更按用户裁决 A 调整了 `node.challenge` 的 wire 必填字段（design D13，已在 verification.md Check Plan Changes 登记），不改变 E2E 适用性判断。"
alternative_checks: ["cargo test --locked -p server --all-features [PV3]：listener 绑定/路由/Host 边界/TLS 失败关闭、配对 HTTP 全状态码与幂等、握手与信封/序号规则、catalog 过滤、attach/generation、snapshot/replay、event 持久化顺序与 raw 保真、ack 单调性、命令授权/幂等/终态/session.create 约束、撤销传播、心跳与慢连接、审计边界，schema/fixture 漂移测试消费同一 manifest。", "cargo test --locked -p app --all-features [PV4]：配置 unwired 收敛、启动/关闭序列含网络 listener、daemon.status.listen 实际地址。", "受控路径全链路集成测试 [PV5]（crates/server 或 crates/app 的集成用例，本机 Windows 执行并留证）：脚本化 fake Access 客户端经真实 loopback listener 完成 配对 claim → 本地确认 → 握手 → catalog.snapshot → attach → snapshot/event/ack → command（含 session.create 正常与各拒绝路径）→ 撤销传播；TLS direct 用自签证书跑通同一握手。", "npm run check / npm run verify [PV1]/[PV2]：合同门禁与全 workspace 测试，确认新增依赖与矩阵格不破坏既有 crate。"]
downgrade_approval: "2026-09-26，本会话，用户原话：「1B 2A 3批准」。其中「3批准」对应本会话提问第 3 项——批准本变更 Main E2E 记 not-applicable，以上述替代验证（本变更相关 cargo 测试，含 fake Access 全链路集成测试 + npm run check / npm run verify）代替；来源为用户对该提问的批准。本记录不沿用 2026-09-23 的首次确认，也不沿用此前各切片的批准。"
```

#### E2E Ownership and Cases

不适用（mode = not-applicable），本节按模板要求删除；实际替代验证已列为主 Agent 任务（7.1）并在 Coverage Index 中引用。

## Failure and Recovery

- 失败修复：按原 WP ID 重新派发（不新增 WP），修复后重跑该 WP 的 [PV3]/[PV4]/[PV5] 与 RV1；受影响的下游按 Dependency Handoffs 的失效列重开。
- 传递下游失效：矩阵或依赖口径变化 → WP2–WP7 与 DU1 的既有证据失效；transport::net 形状变化 → WP3/WP4 复验；conn 注册表形状变化 → WP5/WP6 复验；任一上游变化 → WP7/WP8 复验。
- 主分支失败：`main` 上的 `npm run verify` 失败时停止后续合入，按同一验证路径交付修复；必要时回滚该交付单元（`git revert` 或恢复候选前提交），不允许「红着继续合」。
- 共享资源异常：临时数据目录/证书/锁文件残留时只删自身用例创建的资源；`target/` 污染时 `cargo clean -p server -p app` 后重跑受影响检查。
- 栈核验失败（D1）：停止相关编码，把核验证据与候选取舍交用户决策，不擅自换栈或放开约束。
- 平台限制：[PV5] 无法在 Linux CI 复现的 Windows 用例在本机执行；若本机不可用，该检查记 BLOCKED 并如实上报，不得用 Linux 结果替代。
- 无 E2E 执行，故不涉及 E2E 失败上限；若后续有人把 mode 改为 required，必须先取得新的用户批准并补齐 E2E 任务与入口。
- 未执行项：`cargo-deny`（`deps`/`advisories`）与 `gitleaks`（`secrets`）本地没有等价物，只在 CI 运行；本变更新增依赖（axum、tokio-tungstenite、rustls 系等）的许可证/来源/advisory 判定**只在 CI 完成**，本地 `npm run verify` 不是证据，最终验收中明确记录。

## Completion Criteria

- 全部任务勾选（`verification.md` 中逐 ID 关联证据；结果仅 PASS/FAIL/BLOCKED/NOT_APPLICABLE）。
- 最终主分支版本（`refs/heads/main` 的实际提交）上：[PV1]、[PV2]、[PV3]、[PV4] 全绿；[PV5] 在本机 Windows 执行并留证（或如实记 BLOCKED）；RV1 无未解决阻断项；E2E 记 NOT_APPLICABLE 且替代验证（7.1）已完成并留证。
- 候选合入前执行 `npx --quiet --no-install openspec-agentic workflow check --change node-link-owner --stage premerge --planning-root <权威规划根> --json` 并核对候选 Verify、独立 review 与替代检查证据；非 PASS 不合入。
- 最终验收经 `.agents/skills/agentic-verify/SKILL.md` 入口执行，`workflow check --stage final` PASS 后才报告具备归档条件；远端 main 交付走 AGENTS.md §8 的 PR 路径。
