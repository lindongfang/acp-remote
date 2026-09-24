## Scope and Contracts

- Specs Revision: 本变更 `specs/` 下 2 个新增能力规范的初版——`acp-wire-protocol`（8 条需求 / 12 个场景）与 `local-agent-host`（12 条需求 / 30 个场景）；`openspec/specs/` 之前没有这两个能力，全部为 ADDED，无 MODIFIED/REMOVED/RENAMED。
- Design Revision: `design.md` 的 D1–D11 与 Migration Plan 初版（边界与依赖、raw 保真机制、消息分类与错误模型、supervisor 结构与关闭顺序、与 core 端口的对接语义、平台进程树与 Job 粒度、固定 v1 常量、profile 与凭据注入、日志、测试基座、门禁同步面）。
- Convergence Check: 一致。核对结论：specs 的每条可观察行为在 design 中都有对应机制（raw 保真 ↔ D2；未知判别子/`_` 方法 ↔ D3；进程树清理 ↔ D6；固定常量与有界采集 ↔ D7；凭据注入与失败关闭 ↔ D8；turn 同步接受与终态唯一 ↔ D5/D4），且 design 的接口约定（`LaunchSpec`、`ProcessTree`、`EndpointEvent.turn = None`、`AcpRaw` 只带不含换行的原文）足以支撑 WP 拆包。未解决冲突：无。已知取舍三条（`TurnAccepted.turn` 占位、每 Agent 一个 Job 的解释、view 字段级 schema 校验归 Sync 切片）已写入 design 的 Decisions/Risks，不改变本次拆包与验收条件。
- Skip Specs: no

## Contract Changes

本变更**改变**以下已冻结内容（原/新值、原因与影响分析随实现写入 `verification.md` 的 Check Plan Changes）：

- `docs/MODULE_ARCHITECTURE.md` §4.5：原文只说「把 `KILL_ON_JOB_CLOSE` 设在 Daemon 持有的 Job 上」，本变更收口为「**每个 Agent 一个由 Daemon 侧 supervisor 持有的 Job**」并写明理由（单 Job 无法只结束一棵树）；`KILL_ON_JOB_CLOSE`、Daemon 持有句柄、父→孙清理三条约束不变。
- `docs/MODULE_ARCHITECTURE.md` §3/§3.1/§4.2/§5：`acp-protocol`、`agent-host` 从「待落地」改为已落地（本轮）；§5 矩阵新增 `agent-host` 列并写明 `app → agent-host` 允许；§3.1 增加 `tracing`/`win32job`/`libc` 的版本口径。
- `Cargo.toml`：`[workspace] members` 增加两个 crate；`[workspace.dependencies]` 增加 `tracing`/`win32job`/`libc`；修正「sqlite 适配器是唯一的 runtime 与数据库依赖持有者」注释。
- `core` 的 `TurnAccepted.turn` 语义在实现中被记录为「当前不被 core 消费、由适配器返回占位值」——**不改变** `core` 端口签名（若未来要真正收口，属于 core 端口变更，需用户决策）。

不改变的契约：ACP wire 与矩阵条目（`compatibility/acp/v1/matrix.json`、`schemas/acp/**`）、`fixtures/**` 的既有内容、`core` 端口签名与值对象、`storage-sqlite` 表结构、Sync/Node Link 与本地管理通道的 wire。

## Coverage Index

```agentic-coverage
version: 1
target_ref: refs/heads/main
rows:
  - id: R1
    source:
      path: specs/acp-wire-protocol/spec.md
      heading: "### Requirement: 已知消息的解码与方向校验"
    tasks: ["2.6", "2.8", "3.3"]
    checks: [PV4]
    evidence: [reports/wp2-acp-protocol-tests.log]
  - id: R2
    source:
      path: specs/acp-wire-protocol/spec.md
      heading: "#### Scenario: 合法请求、通知与响应可解码"
      requirement: "### Requirement: 已知消息的解码与方向校验"
    tasks: ["2.6", "2.8", "3.3"]
    checks: [PV4]
    evidence: [reports/wp2-acp-protocol-tests.log]
  - id: R3
    source:
      path: specs/acp-wire-protocol/spec.md
      heading: "#### Scenario: 缺失必填字段或方向错误被拒绝"
      requirement: "### Requirement: 已知消息的解码与方向校验"
    tasks: ["2.6", "3.3"]
    checks: [PV4]
    evidence: [reports/wp2-acp-protocol-tests.log]
  - id: R4
    source:
      path: specs/acp-wire-protocol/spec.md
      heading: "### Requirement: 未知字段、`_meta` 与扩展 payload 的逐字节保真"
    tasks: ["2.7", "2.10", "2.11", "3.3"]
    checks: [PV4]
    evidence: [reports/wp2-acp-protocol-tests.log]
  - id: R5
    source:
      path: specs/acp-wire-protocol/spec.md
      heading: "#### Scenario: 未知字段与超大整数往返字节相等"
      requirement: "### Requirement: 未知字段、`_meta` 与扩展 payload 的逐字节保真"
    tasks: ["2.7", "2.11", "3.3"]
    checks: [PV4]
    evidence: [reports/wp2-acp-protocol-tests.log]
  - id: R6
    source:
      path: specs/acp-wire-protocol/spec.md
      heading: "#### Scenario: 未识别字段不进入公共领域视图"
      requirement: "### Requirement: 未知字段、`_meta` 与扩展 payload 的逐字节保真"
    tasks: ["2.7", "2.11", "3.3"]
    checks: [PV4]
    evidence: [reports/wp2-acp-protocol-tests.log]
  - id: R7
    source:
      path: specs/acp-wire-protocol/spec.md
      heading: "### Requirement: 未知判别子的可见降级"
    tasks: ["2.9", "2.11", "3.3"]
    checks: [PV4]
    evidence: [reports/wp2-acp-protocol-tests.log]
  - id: R8
    source:
      path: specs/acp-wire-protocol/spec.md
      heading: "#### Scenario: 未来 sessionUpdate 保留原文并可辨认"
      requirement: "### Requirement: 未知判别子的可见降级"
    tasks: ["2.9", "2.11", "3.3"]
    checks: [PV4]
    evidence: [reports/wp2-acp-protocol-tests.log]
  - id: R9
    source:
      path: specs/acp-wire-protocol/spec.md
      heading: "### Requirement: 下划线扩展方法的显式不支持"
    tasks: ["2.9", "3.3"]
    checks: [PV4]
    evidence: [reports/wp2-acp-protocol-tests.log]
  - id: R10
    source:
      path: specs/acp-wire-protocol/spec.md
      heading: "#### Scenario: 扩展方法得到显式拒绝"
      requirement: "### Requirement: 下划线扩展方法的显式不支持"
    tasks: ["2.9", "3.3"]
    checks: [PV4]
    evidence: [reports/wp2-acp-protocol-tests.log]
  - id: R11
    source:
      path: specs/acp-wire-protocol/spec.md
      heading: "### Requirement: 结构化内容不被文本化"
    tasks: ["2.8", "2.10", "3.3"]
    checks: [PV4]
    evidence: [reports/wp2-acp-protocol-tests.log]
  - id: R12
    source:
      path: specs/acp-wire-protocol/spec.md
      heading: "#### Scenario: tool call 与 diff 保持结构"
      requirement: "### Requirement: 结构化内容不被文本化"
    tasks: ["2.8", "2.10", "3.3"]
    checks: [PV4]
    evidence: [reports/wp2-acp-protocol-tests.log]
  - id: R13
    source:
      path: specs/acp-wire-protocol/spec.md
      heading: "### Requirement: capability wire 形状与真实性"
    tasks: ["2.8", "3.3"]
    checks: [PV4]
    evidence: [reports/wp2-acp-protocol-tests.log]
  - id: R14
    source:
      path: specs/acp-wire-protocol/spec.md
      heading: "#### Scenario: 能力声明往返保真"
      requirement: "### Requirement: capability wire 形状与真实性"
    tasks: ["2.8", "2.11", "3.3"]
    checks: [PV4]
    evidence: [reports/wp2-acp-protocol-tests.log]
  - id: R15
    source:
      path: specs/acp-wire-protocol/spec.md
      heading: "#### Scenario: 未实现能力不出现在宣告中"
      requirement: "### Requirement: capability wire 形状与真实性"
    tasks: ["2.8", "3.3"]
    checks: [PV4]
    evidence: [reports/wp2-acp-protocol-tests.log]
  - id: R16
    source:
      path: specs/acp-wire-protocol/spec.md
      heading: "### Requirement: 固定 v1 消息上限与失败关闭"
    tasks: ["2.5", "2.10", "3.3"]
    checks: [PV4]
    evidence: [reports/wp2-acp-protocol-tests.log]
  - id: R17
    source:
      path: specs/acp-wire-protocol/spec.md
      heading: "#### Scenario: 超限消息被拒绝"
      requirement: "### Requirement: 固定 v1 消息上限与失败关闭"
    tasks: ["2.5", "2.10", "3.3"]
    checks: [PV4]
    evidence: [reports/wp2-acp-protocol-tests.log]
  - id: R18
    source:
      path: specs/acp-wire-protocol/spec.md
      heading: "### Requirement: 夹具与矩阵驱动的契约证据"
    tasks: ["2.11", "3.1", "3.3"]
    checks: [PV3, PV4]
    evidence: [reports/wp2-acp-protocol-tests.log, reports/wp1-acp-assets.log]
  - id: R19
    source:
      path: specs/acp-wire-protocol/spec.md
      heading: "#### Scenario: 夹具用例全部真实执行"
      requirement: "### Requirement: 夹具与矩阵驱动的契约证据"
    tasks: ["2.11", "3.1", "3.3"]
    checks: [PV3, PV4]
    evidence: [reports/wp2-acp-protocol-tests.log]
  - id: R20
    source:
      path: specs/acp-wire-protocol/spec.md
      heading: "#### Scenario: 矩阵不变量有对应证据"
      requirement: "### Requirement: 夹具与矩阵驱动的契约证据"
    tasks: ["2.11", "3.1", "3.3"]
    checks: [PV3, PV4]
    evidence: [reports/wp2-acp-protocol-tests.log, reports/wp1-acp-assets.log]
  - id: R21
    source:
      path: specs/local-agent-host/spec.md
      heading: "### Requirement: Agent 目录暴露与可用性判定"
    tasks: ["2.25", "3.9"]
    checks: [PV4]
    evidence: [reports/wp5-agent-host-config.log]
  - id: R22
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: 未启动进程也能列出 Agent"
      requirement: "### Requirement: Agent 目录暴露与可用性判定"
    tasks: ["2.25", "3.9"]
    checks: [PV4]
    evidence: [reports/wp5-agent-host-config.log]
  - id: R23
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: 凭据引用失效只影响该条目"
      requirement: "### Requirement: Agent 目录暴露与可用性判定"
    tasks: ["2.25", "2.26", "3.9"]
    checks: [PV4]
    evidence: [reports/wp5-agent-host-config.log]
  - id: R24
    source:
      path: specs/local-agent-host/spec.md
      heading: "### Requirement: 会话创建与 ACP 会话标识映射"
    tasks: ["2.20", "3.7"]
    checks: [PV4]
    evidence: [reports/wp4-agent-host-session.log]
  - id: R25
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: 新建会话完成映射并只启动一个进程"
      requirement: "### Requirement: 会话创建与 ACP 会话标识映射"
    tasks: ["2.20", "3.7"]
    checks: [PV4]
    evidence: [reports/wp4-agent-host-session.log]
  - id: R26
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: 相同 core 会话重复创建被拒"
      requirement: "### Requirement: 会话创建与 ACP 会话标识映射"
    tasks: ["2.20", "3.7"]
    checks: [PV4]
    evidence: [reports/wp4-agent-host-session.log]
  - id: R27
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: 会话恢复只接受已存在的映射"
      requirement: "### Requirement: 会话创建与 ACP 会话标识映射"
    tasks: ["2.20", "3.7"]
    checks: [PV4]
    evidence: [reports/wp4-agent-host-session.log]
  - id: R28
    source:
      path: specs/local-agent-host/spec.md
      heading: "### Requirement: turn 的同步接受与流式事件交付"
    tasks: ["2.21", "2.23", "3.7"]
    checks: [PV4]
    evidence: [reports/wp4-agent-host-session.log]
  - id: R29
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: prompt 先被接受，事件随后到达"
      requirement: "### Requirement: turn 的同步接受与流式事件交付"
    tasks: ["2.23", "3.7"]
    checks: [PV4]
    evidence: [reports/wp4-agent-host-session.log]
  - id: R30
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: 结构化事件不被文本化"
      requirement: "### Requirement: turn 的同步接受与流式事件交付"
    tasks: ["2.21", "3.7"]
    checks: [PV4]
    evidence: [reports/wp4-agent-host-session.log]
  - id: R31
    source:
      path: specs/local-agent-host/spec.md
      heading: "### Requirement: 交互请求的转发与回传"
    tasks: ["2.22", "3.7"]
    checks: [PV4]
    evidence: [reports/wp4-agent-host-session.log]
  - id: R32
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: 权限请求等待外部解析"
      requirement: "### Requirement: 交互请求的转发与回传"
    tasks: ["2.22", "3.7"]
    checks: [PV4]
    evidence: [reports/wp4-agent-host-session.log]
  - id: R33
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: 解析结果按原标识回传"
      requirement: "### Requirement: 交互请求的转发与回传"
    tasks: ["2.22", "3.7"]
    checks: [PV4]
    evidence: [reports/wp4-agent-host-session.log]
  - id: R34
    source:
      path: specs/local-agent-host/spec.md
      heading: "### Requirement: 能力协商与可选调用门控"
    tasks: ["2.24", "3.7"]
    checks: [PV4]
    evidence: [reports/wp4-agent-host-session.log]
  - id: R35
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: 未宣告的可选能力不被调用"
      requirement: "### Requirement: 能力协商与可选调用门控"
    tasks: ["2.24", "3.7"]
    checks: [PV4]
    evidence: [reports/wp4-agent-host-session.log]
  - id: R36
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: 已宣告的能力按真实结果返回"
      requirement: "### Requirement: 能力协商与可选调用门控"
    tasks: ["2.24", "3.7"]
    checks: [PV4]
    evidence: [reports/wp4-agent-host-session.log]
  - id: R37
    source:
      path: specs/local-agent-host/spec.md
      heading: "### Requirement: 模式与配置选项的读取与写入"
    tasks: ["2.24", "3.7"]
    checks: [PV4]
    evidence: [reports/wp4-agent-host-session.log]
  - id: R38
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: 未宣告时读取返回空而不是编造"
      requirement: "### Requirement: 模式与配置选项的读取与写入"
    tasks: ["2.24", "3.7"]
    checks: [PV4]
    evidence: [reports/wp4-agent-host-session.log]
  - id: R39
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: 未宣告时写入被显式拒绝"
      requirement: "### Requirement: 模式与配置选项的读取与写入"
    tasks: ["2.24", "3.7"]
    checks: [PV4]
    evidence: [reports/wp4-agent-host-session.log]
  - id: R40
    source:
      path: specs/local-agent-host/spec.md
      heading: "### Requirement: 取消与 turn 终态唯一"
    tasks: ["2.23", "3.7"]
    checks: [PV4]
    evidence: [reports/wp4-agent-host-session.log]
  - id: R41
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: 取消后只有一份终态"
      requirement: "### Requirement: 取消与 turn 终态唯一"
    tasks: ["2.23", "3.7"]
    checks: [PV4]
    evidence: [reports/wp4-agent-host-session.log]
  - id: R42
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: 取消不影响其他会话"
      requirement: "### Requirement: 取消与 turn 终态唯一"
    tasks: ["2.23", "2.20", "3.7"]
    checks: [PV4]
    evidence: [reports/wp4-agent-host-session.log]
  - id: R43
    source:
      path: specs/local-agent-host/spec.md
      heading: "### Requirement: 失败路径的明确结果与 turn 不设超时"
    tasks: ["2.15", "2.16", "2.19", "3.5"]
    checks: [PV4]
    evidence: [reports/wp3-agent-host-supervision.log]
  - id: R44
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: 启动超时失败并回收"
      requirement: "### Requirement: 失败路径的明确结果与 turn 不设超时"
    tasks: ["2.16", "2.18", "3.5"]
    checks: [PV4, PV5]
    evidence: [reports/wp3-agent-host-supervision.log, reports/pv5-windows-tree.log]
  - id: R45
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: turn 不因超时被杀"
      requirement: "### Requirement: 失败路径的明确结果与 turn 不设超时"
    tasks: ["2.16", "3.5"]
    checks: [PV4]
    evidence: [reports/wp3-agent-host-supervision.log]
  - id: R46
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: 异常退出成为明确事件"
      requirement: "### Requirement: 失败路径的明确结果与 turn 不设超时"
    tasks: ["2.15", "2.19", "3.5"]
    checks: [PV4]
    evidence: [reports/wp3-agent-host-supervision.log]
  - id: R47
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: 未知响应标识不被误配"
      requirement: "### Requirement: 失败路径的明确结果与 turn 不设超时"
    tasks: ["2.15", "3.5"]
    checks: [PV4]
    evidence: [reports/wp3-agent-host-supervision.log]
  - id: R48
    source:
      path: specs/local-agent-host/spec.md
      heading: "### Requirement: 进程树清理"
    tasks: ["2.17", "2.18", "3.5"]
    checks: [PV4, PV5]
    evidence: [reports/wp3-agent-host-supervision.log, reports/pv5-windows-tree.log]
  - id: R49
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: 关闭 Job 句柄后整棵树停止"
      requirement: "### Requirement: 进程树清理"
    tasks: ["2.17", "2.18", "3.5"]
    checks: [PV5]
    evidence: [reports/pv5-windows-tree.log]
  - id: R50
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: 强制结束 Agent 时孙进程不残留"
      requirement: "### Requirement: 进程树清理"
    tasks: ["2.17", "2.18", "3.5"]
    checks: [PV5]
    evidence: [reports/pv5-windows-tree.log]
  - id: R51
    source:
      path: specs/local-agent-host/spec.md
      heading: "### Requirement: stderr 有界采集与脱敏"
    tasks: ["2.16", "3.5"]
    checks: [PV4]
    evidence: [reports/wp3-agent-host-supervision.log]
  - id: R52
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: 超限丢弃最旧并计数"
      requirement: "### Requirement: stderr 有界采集与脱敏"
    tasks: ["2.16", "3.5"]
    checks: [PV4]
    evidence: [reports/wp3-agent-host-supervision.log]
  - id: R53
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: stderr 不进入协议通道"
      requirement: "### Requirement: stderr 有界采集与脱敏"
    tasks: ["2.16", "3.5"]
    checks: [PV4]
    evidence: [reports/wp3-agent-host-supervision.log]
  - id: R54
    source:
      path: specs/local-agent-host/spec.md
      heading: "### Requirement: profile 来源与凭据注入边界"
    tasks: ["2.26", "3.9"]
    checks: [PV4]
    evidence: [reports/wp5-agent-host-config.log]
  - id: R55
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: 白名单是注入上限"
      requirement: "### Requirement: profile 来源与凭据注入边界"
    tasks: ["2.26", "3.9"]
    checks: [PV4]
    evidence: [reports/wp5-agent-host-config.log]
  - id: R56
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: 引用失效时失败关闭"
      requirement: "### Requirement: profile 来源与凭据注入边界"
    tasks: ["2.26", "3.9"]
    checks: [PV4]
    evidence: [reports/wp5-agent-host-config.log]
  - id: R57
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: 不注入节点与设备密钥"
      requirement: "### Requirement: profile 来源与凭据注入边界"
    tasks: ["2.26", "3.9"]
    checks: [PV4]
    evidence: [reports/wp5-agent-host-config.log]
  - id: R58
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: 不使用启动配置文件中的 profile"
      requirement: "### Requirement: profile 来源与凭据注入边界"
    tasks: ["2.26", "3.9"]
    checks: [PV4]
    evidence: [reports/wp5-agent-host-config.log]
  - id: R59
    source:
      path: specs/local-agent-host/spec.md
      heading: "### Requirement: 空闲回收与进程关闭顺序"
    tasks: ["2.19", "2.27", "3.5", "3.9"]
    checks: [PV4]
    evidence: [reports/wp3-agent-host-supervision.log, reports/wp5-agent-host-config.log]
  - id: R60
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: 无活动会话且超过空闲超时时关闭"
      requirement: "### Requirement: 空闲回收与进程关闭顺序"
    tasks: ["2.27", "3.9"]
    checks: [PV4]
    evidence: [reports/wp5-agent-host-config.log]
  - id: R61
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: 零值不因空闲关闭"
      requirement: "### Requirement: 空闲回收与进程关闭顺序"
    tasks: ["2.27", "3.9"]
    checks: [PV4]
    evidence: [reports/wp5-agent-host-config.log]
  - id: R62
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: 关闭顺序有界且不留后台任务"
      requirement: "### Requirement: 空闲回收与进程关闭顺序"
    tasks: ["2.19", "3.5"]
    checks: [PV4]
    evidence: [reports/wp3-agent-host-supervision.log]
```

## Work Packages

| ID | Goal / Scenarios | Dependencies | Owner | Reviewer | Branch / Worktree | Write Scope | Inputs / Outputs | Verification |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| WP1 | 契约与依赖边界冻结：把 `acp-protocol`/`agent-host` 落入 §3/§3.1/§4.2/§4.5/§5，登记 `tracing`/`win32job`/`libc`，同步三处状态表；覆盖 R18/R20 的资产登记前提 | 无（W0 起点） | 实现 Agent（coder） | 非文档作者（RV1） | `feat/acp-boundary-and-agent-host` / 主 worktree | `docs/MODULE_ARCHITECTURE.md`、`Cargo.toml`、`README.md`、`docs/DEVELOPMENT_PLAN.md`、`AGENTS.md` | 输入：`design.md` D1/D6/D11；输出：可被 `check:boundaries`、`check:docs`、`check:acp` 读取的文档与依赖口径 | 3.1（PV2/PV3）；3.2 review |
| WP2 | `acp-protocol`：信封分类、方向与 required 校验、`RawDocument` 原文承载、11 种已知判别子 + 未知判别子可见降级、`_` 方法显式不支持、结构化内容不文本化、1 MiB 上限，以及 fixture/矩阵驱动的契约测试；覆盖 R1–R20 | WP1 | 实现 Agent（coder） | 非实现者（RV1） | 同上 | `crates/acp-protocol/**`（含 `Cargo.toml` 与 `[workspace] members` 中的本 crate 条目） | 输入：`design.md` D2/D3/D7、`fixtures/acp/v1/manifest.json`、`compatibility/acp/v1/matrix.json`；输出：可被 `agent-host` 依赖的 wire 层与其契约测试 | 3.3（PV4）；3.4 review |
| WP3 | `agent-host` 进程监督与平台进程树：crate 骨架与 `limits`、fake ACP child 基座、spawn（接收 `LaunchSpec`）、stdio 分帧、request id 注册表、短请求/启动超时与 turn 不设超时、stderr 环形缓冲、`ProcessTree`（Windows Job / Unix 进程组）、关闭顺序与无 detached task；覆盖 R43–R53、R59、R62 | WP1、WP2 | 实现 Agent（coder） | 非实现者（RV1） | 同上 | `crates/agent-host/{Cargo.toml,src/lib.rs,src/error.rs,src/limits.rs,src/supervisor.rs,src/platform/**,src/bin/acpr-fake-acp-agent.rs}`、`[workspace] members` 中的本 crate 条目 | 输入：`design.md` D4/D6/D7/D10、`SECURITY_DESIGN.md` §12.2、`INITIAL_DESIGN.md` §16 第 4 条；输出：进程生命周期、进程树清理与常驻回归测试 | 3.5（PV4/PV5）；3.6 review |
| WP4 | `agent-host` 会话语义与 core 对接：`SessionBackendFactory`/`SessionEndpoint`、SessionId↔ACP sessionId 映射与 generation、`mapper`（结构化 view + `AcpRaw`）、`interaction`（权限/elicitation 转发与回传）、turn 同步接受/终态唯一/取消、能力门控与 modes/config 读写；覆盖 R24–R42 | WP3 | 实现 Agent（coder） | 非实现者（RV1） | 同上 | `crates/agent-host/src/{session.rs,mapper.rs,interaction.rs}`、`crates/agent-host/tests/session_*.rs` | 输入：`design.md` D5、`core::ports` 的 `SessionEndpoint` 形状、`SYNC_PROTOCOL.md` §10.3 的 raw 约定；输出：可被 `core::use_cases` 直接使用的本地 backend | 3.7（PV4）；3.8 review |
| WP5 | `agent-host` 目录、profile 与凭据：`catalog`（目录 + 可用性 + `agent_capabilities` 真实协商与缓存）、`LaunchSpec` 组装（`env_allowlist ∩ 绑定`、`CredentialResolver` 失败关闭、不注入节点密钥、不读启动配置文件）、空闲回收（注入 `sessions.idle_timeout_ms`）；覆盖 R21–R23、R54–R58、R60–R61 | WP3（`LaunchSpec` 接口由 WP3 落地） | 实现 Agent（coder） | 非实现者（RV1） | 同上 | `crates/agent-host/src/{catalog.rs,config.rs,credentials.rs}`、`crates/agent-host/tests/config_*.rs` | 输入：`design.md` D7/D8、`LocalConfigStore`/`CredentialResolver` 端口签名、`CONFIG_REFERENCE.md` §7；输出：目录与启动参数的唯一来源 | 3.9（PV4）；3.10 review |

说明：Main E2E mode 为 `not-applicable`，因此不生成独立的测试工作包（TP）与 Test Design 分组；各工作包自带其行为测试（局部自检），最终替代验证由主 Agent 任务（7.1）统一执行并留证。

## Execution Waves

| Wave | Work Packages | 依赖满足条件 | 并发上限 | 说明 |
| --- | --- | --- | --- | --- |
| W0 契约与依赖冻结（串行，单执行者） | 1.1 → WP1（2.1–2.4） | 已完成的仓库工作区 | 1 | `Cargo.toml` 与文档在 W0 只有一个写入者；本波完成条件 = PV2/PV3 绿（新 crate 尚未加入 members，因此矩阵新增列不影响判定）+ 一个基线提交 |
| W1 `acp-protocol` | WP2（2.5–2.11） | W0 的基线提交 | 1 | 本 crate 是 `agent-host` 的接口提供方，先落地并冻结 public API；本波同时把 `crates/acp-protocol` 写入 `members` |
| W2 `agent-host` 监督层 | WP3（2.12–2.19） | WP2 的 public API 冻结 | 1 | 本波把 `crates/agent-host` 写入 `members`；`Cargo.toml` 的两次成员写入按波次串行（W1 → W2），不并发 |
| W3 并行实现 | WP4（2.20–2.24）∥ WP5（2.25–2.27） | WP3 的 `LaunchSpec` 与 supervisor 接口冻结 | 2 | 两条轨道文件不重叠：`session.rs`/`mapper.rs`/`interaction.rs` ／ `catalog.rs`/`config.rs`/`credentials.rs`；各自 test 文件独立 |
| W4 交付前验证 | 3.1–3.10 | W1–W3 对应轨道完成 | 2 | PV 与独立 review 并行；review 必须由不继承实现对话的 reviewer 完成 |
| W5 集成与合入 | 5.1–5.2 → 6.1–6.8 | 全部 3.x 完成 | 1 | `integrated` 单一交付单元；每个目标分支只允许一个集成执行者串行更新 |
| W6 最终验证 | 7.1–7.3 → 8.1 | 6.7、6.8 | 1 | not-applicable 的替代验证 + `[e2e-owned]` 门禁 + 最终验收 |

**多 Agent 分派规则**（宿主支持子 Agent / 可开独立会话时）：

- 每个轨道一个独立 coder 执行者与其自己的 worktree（`git worktree add`）；构建目录用 `CARGO_TARGET_DIR` 隔离，**不允许两个执行者共用同一个 `target/`**。
- 每个 WP 的交付前独立 review（RV1）必须由**不继承实现对话**的执行者完成；缺少隔离上下文时相关任务记 BLOCKED，不得以自审替代。
- 只有主 Agent 可写 `plan.md`/`tasks.md`/`verification.md`；子 Agent 返回结构化 handoff（固定提交、命令、日志路径、差异范围）。
- 文件单一写入者（跨轨道也不得并发写）：`Cargo.toml`（按波次 W0 → W1 → W2 串行写）、`docs/*` 与状态表（WP1）、`crates/acp-protocol/**`（WP2）、`crates/agent-host/src/{lib,error,limits,supervisor}.rs` 与 `platform/**`、`bin/**`（WP3）、`src/{session,mapper,interaction}.rs`（WP4）、`src/{catalog,config,credentials}.rs`（WP5）、`reports/*` 按检查 ID 一文件一写者。
- `Cargo.lock` 在 W0 因新增 workspace 依赖变更一次；W1/W2 因新成员再各变更一次（只增成员，不做 `cargo update`/版本升级）；W3 之后不得再解析依赖。
- 无并行能力时按 roles 串行执行并如实记录；串行不改变上面每条轨道的完成条件与证据要求。

## Dependency Handoffs

| Downstream | Upstream | Accepted Revision / Evidence | Start Revision | Transfer / Inclusion Check | Invalidation |
| --- | --- | --- | --- | --- | --- |
| WP2 | WP1 | 2.1–2.4 的文档/依赖口径与 `check:boundaries`、`check:docs` 通过证据 | 变更分支起点 | `cargo metadata --no-deps` 中 workspace 依赖集合与 `[workspace.dependencies]` 一致；§5 矩阵已含 `acp-protocol` 列 | §5 矩阵或依赖口径变化 → WP2 复验 |
| WP3 | WP1、WP2 | WP2 冻结的 public API（`RawDocument`、信封分类、错误枚举）+ 3.3 的 PV4 证据 | WP2 完成点 | `cargo build -p agent-host` 通过且未使用 `acp-protocol` 的非公开项 | `acp-protocol` public API 变化 → WP3/WP4/WP5 复验 |
| WP4 | WP3 | WP3 冻结的 `LaunchSpec`、supervisor 句柄与 `ProcessTree` 接口 + 3.5 的 PV4/PV5 证据 | W2 完成点 | `cargo test -p agent-host --test session_*` 在固定版本上通过 | supervisor/`LaunchSpec` 接口变化 → WP4 复验 |
| WP5 | WP3 | 同上（`LaunchSpec` 组装侧） | W2 完成点 | `cargo test -p agent-host --test config_*` 在固定版本上通过 | `LaunchSpec` 或凭据端口签名变化 → WP5 复验 |
| DU1 | 全部 WP | 各 WP 的 PV 证据与 RV1 review 报告 | W5 完成点 | `npm run verify` 全绿 + 变更 diff 无未登记文件 | 任一上游变化 → 候选重建、PV1 重跑 |

## Runtime Resources

| Checks / Work Packages | Resource | Isolation / Configuration | Exclusive Scheduling | Owner / Cleanup |
| --- | --- | --- | --- | --- |
| PV1–PV5、WP1–WP5 | Rust 构建目录 `target/` | 每个并行执行者设自己的 `CARGO_TARGET_DIR`；同一 `target/` 不得并发写 | 按 WP 分 worktree；同 crate 并发时独占构建目录 | 各执行者，只清理自身产生的临时目录与 worktree |
| WP3–WP5 的 fake ACP child 用例 | 子进程 + 临时心跳文件目录（每个用例一个独立临时目录） | 用例用 `tempfile` 风格的自建临时目录并在结束时删除；心跳文件只由该用例的孙进程写 | 无（进程与目录级隔离） | 实现 Agent |
| WP3/R48–R50 的进程树回归测试 | 本机操作系统进程表与 Job 句柄 | Windows 上每个用例创建并关闭自己的 Job；用例之间不共用 Job | 同一时刻只允许一个用例断言同一心跳文件 | 实现 Agent |
| PV1（`npm run verify`） | Node ≥ 22.12 与仓库内 `node_modules` | 只读脚本，不写仓库以外的状态 | 无 | 主 Agent |
| PV5（Windows 进程树留证） | 本机 Windows x64 主机 | 无法在 Linux CI 复现；必须在本地 Windows 执行并保存原始日志 | 与其它使用子进程的检查串行，避免进程表噪声 | 主 Agent |

说明：本变更不涉及数据库服务、容器、端口或外部账号等共享运行资源；`target/`、临时心跳目录与子进程是本变更仅有的运行设施，已按上表隔离。

## Target Repository and Main Branch

- Code Repository: `D:\Project\acp-remote`
- Target Kind / Ref: local；`refs/heads/main`
- Version Confirmation Owner: 主 Agent（机械核实可派发 environment/recon）
- Confirmation Method / Evidence: `git rev-parse refs/heads/main`、`git status --porcelain` 与 `git worktree list`；实际提交与核实结果记录在 `verification.md`

## Merge Strategy

| Delivery Unit / Mode | WP / TP | Owners: Implementation / Test / Review / Merge | Start / Readiness | Candidate Checks / Critical E2E | Order |
| --- | --- | --- | --- | --- | --- |
| DU1 / integrated | WP1–WP5 | 实现 Agent / 实现 Agent（各 WP 自带测试）/ 独立 reviewer / 主 Agent（按当前授权） | 契约与依赖边界冻结 + WP2 public API 冻结 + `LaunchSpec` 接口冻结 | PV1、PV2、PV3、PV4、PV5；无关键 E2E（mode = not-applicable） | 唯一单元，先候选后合入 |

- Integration Branch / Worktree: `feat/acp-boundary-and-agent-host`（主 worktree；如需并行分片则用 `CARGO_TARGET_DIR` 隔离构建目录）
- Source Revision / Handoff: 变更分支起点 = 计划确认时的 `refs/heads/main` 提交（在 6.1 记录固定值）
- Authorization / Report Path: 合并与推送授权仍受当前会话限制；集成报告写入 `reports/du1-integration.md`

采用 integrated 的理由：`MODULE_ARCHITECTURE.md` §5 矩阵、`Cargo.toml` 的 members 与新增依赖、两个 crate 的实现与测试由 `check:boundaries`/`check:docs` 相互绑定；且 `agent-host` 的编译依赖 `acp-protocol` 的 public API。任何一方单独先合入都会让 `main` 上的 `npm run check` 或 `cargo build --workspace` 变红，因此必须作为一个交付单元一次性合入。

## Verification Strategy

### Local Checks

- 实现期：`cargo fmt --all -- --check`、`cargo clippy --locked -p acp-protocol -p agent-host --all-targets --all-features -- -D warnings`、`cargo test --locked -p acp-protocol -p agent-host --all-features`。
- 合同侧：`node scripts/check-crate-boundaries.mjs`、`node scripts/check-acp-compatibility.mjs`、`node scripts/check-schema-fixtures.mjs`、`node scripts/check-doc-links.mjs`（改文档后必跑）。
- 平台侧：Windows 本机执行 `cargo test --locked -p agent-host --all-features -- --nocapture tree_`（进程树用例名族）并保存原始日志。

### Project Verify

| Check ID | Stages / Work Packages | Command / Working Directory | Configuration / Environment | Scope / Pass Criteria | Evidence |
| --- | --- | --- | --- | --- | --- |
| PV1 | 候选、主分支、最终（WP1–WP5） | `npm run verify`（= `npm run check` + `cargo fmt --all -- --check` + `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` + `cargo test --locked --workspace --all-features`）@ `D:\Project\acp-remote` | Node ≥ 22.12、`rust-toolchain.toml` 固定的工具链；不联网 | 退出码 0 且逐子项无失败、无零测试、无全跳过；`npm run check` 的十道门禁全绿 | `reports/du1-pv1.log`、`reports/du1-main-verify.log` |
| PV2 | 候选、主分支（WP1–WP3） | `node scripts/check-crate-boundaries.mjs` @ 仓库根 | 需要本地 `cargo`（`cargo metadata`/`cargo tree`），不联网 | §5 矩阵逐条成立：`acp-protocol` 行无工作区依赖、`agent-host` 只依赖 `core`/`acp-protocol`、`core` 闭包与 allow-list 逐项相等 | `reports/wp1-boundaries.log`、`reports/du1-pv1.log` |
| PV3 | 候选、主分支（WP1、WP2） | `node scripts/check-acp-compatibility.mjs` + `node scripts/check-schema-fixtures.mjs` @ 仓库根 | 只读脚本；固定上游快照 sha256 | 矩阵结构与枚举、夹具 manifest 逐例、快照 sha256 全部通过；夹具未被改动时逐字节不变 | `reports/wp1-acp-assets.log` |
| PV4 | 各 WP 交付前、最终（WP2–WP5） | `cargo test --locked -p acp-protocol -p agent-host --all-features` @ 仓库根 | 本地 Rust 工具链；临时目录与子进程 | 变更相关 crate 全部测试通过，新增用例确实被执行（无 0 用例 / 无全跳过） | `reports/wp2-acp-protocol-tests.log`、`reports/wp3-agent-host-supervision.log`、`reports/wp4-agent-host-session.log`、`reports/wp5-agent-host-config.log` |
| PV5 | 候选、主分支（WP3 的进程树用例） | Windows 本机：`cargo test --locked -p agent-host --all-features tree_ -- --nocapture` @ `D:\Project\acp-remote` | **只在 Windows x64 执行**；Linux CI 只跑 `#[cfg(unix)]` 分支 | 「关闭 Job 句柄后父与孙停止」「强制结束 Agent 后孙停止」两条断言通过，心跳文件字节数在结束前后不再增长，且输出原始日志 | `reports/pv5-windows-tree.log` |
| RV1 | 各 WP 交付前、主分支复核 | 独立 reviewer 按 `roles/reviewer.md` 在隔离上下文检视固定版本 diff 与契约 | 只读；不修改代码与证据 | 报告完整、无未解决阻断项；覆盖 specs 场景与 design 决策 | `reports/rv1-wp1.md`、`reports/rv1-wp2.md`、`reports/rv1-wp3.md`、`reports/rv1-wp4.md`、`reports/rv1-wp5.md`、`reports/rv1-du1.md` |

### Code Review

- 范围：WP1（§5 矩阵与 §4.5 收口措辞、`Cargo.toml` 依赖登记）、WP2（raw 保真机制、未知判别子与 `_` 方法、上限与结构化内容）、WP3（子进程所有权与关闭顺序、stdio 分帧与 request id、进程树清理、stderr 有界脱敏）、WP4（`EndpointEvent` 组装、`AcpRaw` 的 sha256 与换行约定、交互 id 保真、turn 终态唯一）、WP5（凭据注入交集与失败关闭、不注入节点密钥、不读启动配置文件）。
- 关注点：① 是否存在「经 `Value` 往返后声称字节保真」；② 是否有任何未列出常量（`SECURITY_DESIGN.md` §12.2 之外的超时/上限）；③ 是否遗留 detached task；④ 进程树在失败路径（`unwrap` 之外的错误分支）也会被清理；⑤ 日志/错误/测试快照中不出现凭据值、prompt 正文或规范化路径；⑥ `agent-host` 未读启动配置文件；⑦ `cfg` 只出现在 `platform` 与 `bin`（fake child）。
- 阻断标准：任一规格场景缺覆盖、任一 `check:boundaries`/`check:acp` 差异、任何凭据或路径泄漏、任何「看似成功但没有清理进程树」的路径、任何把结构化事件文本化的实现。
- 复核安排：修复后由同一 reviewer 对新固定版本复核，并保留原报告与复核结论。

### Main E2E

```yaml
mode: not-applicable
reason: "本变更只落地两个库 crate（ACP wire 层与本地 Agent 后端）。仓库中没有可端到端运行的产品入口——`server`、`app`/Daemon、CLI、前端均未实现，没有监听器、没有 CLI 子命令、没有 ACP facade 可驱动；`agent-host` 的唯一消费者 `core::use_cases` 在仓库内只能由测试替身驱动。因此不存在可执行产品路径的用例。"
basis: "`openspec/config.yaml` 的 context 已记录「当前阶段没有可端到端运行的产品路径，Main E2E 按变更记 not-applicable，需用户逐变更批准并写入 plan.md 的 downgrade_approval」，且 `x-agentic.e2e.command` 为空（`openspec-agentic e2e --json` 实测：enabled=true、command=\"\"）。本变更是库级实现，不改任何 wire 协议，不影响 E2E 适用性判断。"
alternative_checks:
  - "cargo test --locked -p acp-protocol -p agent-host --all-features [PV4]：覆盖夹具驱动的解码/编码与 raw 逐字节保真（原始字节比较）、未知判别子可见降级、下划线扩展方法显式不支持、1 MiB 上限、fake ACP child 驱动的完整 turn、startup/短请求超时、取消、乱序与未知 id、异常退出、权限/elicitation 待解析与回传、能力门控、目录与可用性、凭据注入交集与失败关闭、空闲回收、stderr 有界。"
  - "Windows 本机 `cargo test -p agent-host --all-features tree_` [PV5]：把 `INITIAL_DESIGN.md` §16 第 4 条的探针结论固化为常驻回归测试（关闭 Job 句柄后父→孙停止、强制结束 Agent 后孙停止）。"
  - "npm run check [PV2/PV3]：十道合同门禁，重点为 §5 依赖矩阵扫描、ACP 矩阵与夹具结构、夹具快照 sha256、文档引用。"
  - "npm run verify [PV1]：统一合同门禁 + fmt/clippy/全 workspace 测试，确认新增成员与依赖不破坏既有 crate。"
downgrade_approval: "2026-09-24，本会话，用户原话：「1. 同意 2. 同意」。其中第 2 项对应本变更 Main E2E 记 not-applicable 及上述替代验证清单（提问原文点名 `cargo test --locked -p acp-protocol -p agent-host --all-features` + `npm run check` + `npm run verify`），来源为用户对本会话第二个问题的批准；本记录不沿用 2026-09-23 的首次确认。"
```

#### E2E Ownership and Cases

不适用（mode = not-applicable），本节按模板要求删除；实际替代验证已列为主 Agent 任务（7.1）并在 Coverage Index 中引用。

## Failure and Recovery

- 失败修复：按原 WP ID 重新派发（不新增 WP），修复后重跑该 WP 的 PV4/PV5 与 RV1；受影响的下游按 Dependency Handoffs 的失效列重开。
- 传递下游失效：`acp-protocol` 的 public API 变化 → WP3/WP4/WP5 与 DU1 的既有证据失效；`LaunchSpec` 或 supervisor 接口变化 → WP4/WP5 复验；§5 矩阵或依赖口径变化 → WP2/WP3 复验。
- 主分支失败：`main` 上的 `npm run verify` 失败时停止后续合入，按同一验证路径交付修复；必要时回滚该交付单元（`git revert` 或恢复候选前提交），不允许「红着继续合」。
- 共享资源异常：临时心跳目录或 `target/` 污染时清理由测试创建的临时目录并 `cargo clean -p acp-protocol -p agent-host` 后重跑受影响检查；进程树用例失败可能留下子进程，重跑前必须确认上一轮的子进程已退出（否则先手工清理再复跑，并在报告中记录）。
- 平台限制：PV5 无法在 Linux CI 复现；若本地 Windows 不可用，该检查记 BLOCKED 并如实上报，不得用 Unix 路径的结果替代。
- 无 E2E 执行，故不涉及 E2E 失败上限；若后续有人把 mode 改为 required，必须先取得新的用户批准并补齐 E2E 任务。
- 未执行项：`cargo-deny` 与 `gitleaks` 本地没有等价物，只在 CI 运行；本变更在最终验收中明确记录为「未在本地执行」，也不得用 `cargo install` 尝试（本机到 crates.io 的传输不稳定）。

## Completion Criteria

- 全部任务勾选（`verification.md` 中逐 ID 关联证据；结果仅 PASS/FAIL/BLOCKED/NOT_APPLICABLE）。
- 最终主分支版本（`refs/heads/main` 的实际提交）上：PV1、PV2、PV3、PV4 全绿；PV5 在本地 Windows 上执行并留证（或如实记 BLOCKED）；RV1 无未解决阻断项；E2E 记 NOT_APPLICABLE 且替代验证（7.1）已完成并留证。
- 阻断问题清零：无未闭环的 FAIL/BLOCKED、无未登记漂移、无凭据/路径/正文进入日志或错误消息、无遗留孤儿子进程。
- 最终验收由主 Agent 按 `.agents/skills/agentic-verify/SKILL.md` 执行，并在验收块中以唯一 `[final-verification]` 任务记录。
- 本计划不构成合并、推送、回滚或发布授权。
