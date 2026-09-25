## Scope and Contracts

- Specs Revision: 本变更 `specs/` 下 4 个新增能力规范的初版——`daemon-lifecycle`（5 条需求 / 11 个场景）、`local-admin-channel`（5 条需求 / 11 个场景）、`local-admin-methods`（7 条需求 / 19 个场景）、`cli-commands`（5 条需求 / 9 个场景）；`openspec/specs/` 之前没有这四个能力，全部为 ADDED，无 MODIFIED/REMOVED/RENAMED。
- Design Revision: `design.md` 的 Decisions 第 1–9 条与 Risks 初版（crate 与模块划分、运行时、Windows FFI 边界定案、单实例锁、`0x02` facade 缺席期失败方式、方法路由与身份/凭据接线、CLI 形态、配置加载、QR 取舍）。
- Convergence Check: 一致。核对结论：specs 的每条可观察行为在 design 中都有对应机制（生命周期与锁 ↔ D4；endpoint/ACL/framing/channel 绑定 ↔ D1/D3/D5；信封与方法 ↔ D1/D6；CLI 映射/仪式/字节泵 ↔ D7），且 design 的接口与数据约定（`DaemonControl` 注入、共享配对状态机实例、`SecretBytes` 流转、`AuditStore::query` 直达）足以支撑 WP 拆包与独立 review。未解决冲突：无。两条已定案的取舍（`0x02` facade 缺席期即连即关、`windows-local-ipc` 自研 path 依赖）已写入 design 的 Decisions，不改变本次拆包与验收条件。
- Skip Specs: no

## Contract Changes

本变更**改变**以下已冻结内容（原/新值、原因与影响分析随实现写入 `verification.md` 的 Check Plan Changes）：

- `docs/MODULE_ARCHITECTURE.md` §3/§3.1/§4.9/§4.10：`server`/`app` 从「待落地」改为已落地（本轮范围：`server` 仅 `transport` 本地通道部分与 `local_admin`，`acp_facade` 仍待切片 6）；§5 依赖矩阵收敛既有「缺列」注记（`server`/`app` 开始成为列）。
- `docs/LOCAL_ADMIN_PROTOCOL.md` §3.1：补一条**实现状态注记**（facade 缺席期 `0x02` 连接即连即关，design D5），不改任何语义。
- `Cargo.toml`：`[workspace] members` 增加 `crates/server`、`crates/app`（按波次串行）；`[workspace.dependencies]` 增量开启/新增 `tokio` 的 `net`/`io-util`/`signal` feature、`nix` 的 `socket`/`user` feature，新增 `clap`、`toml`、文件锁 crate（候选 `fs4`/`fd-lock`，§20 核验后定）；`workspace.exclude` 登记 `vendor/windows-local-ipc`（自研 FFI wrapper，不继承 workspace lint，不发布）。
- `deny.toml`：登记 path 来源依赖 `windows-local-ipc`（其许可证判定仍在 CI）。
- `commitlint.config.mjs`：**不需要**改动——`server`/`app` scope 已在词表内。

不改变的契约：`schemas/`、`fixtures/`、`compatibility/` 的全部既有资产、`core` 端口签名与值对象、`storage-sqlite` 表结构、Sync/Node Link/ACP wire、`docs/CONFIG_REFERENCE.md`（本变更不新增配置键）。

## Coverage Index

> evidence 使用变更目录相对路径（`reports/…`），规划阶段可以尚不存在；正文提及同一批文件时写仓库相对完整路径以免混淆。

```agentic-coverage
version: 1
target_ref: refs/heads/main
rows:
  - id: R1
    source:
      path: specs/daemon-lifecycle/spec.md
      heading: "### Requirement: Daemon 前台启动与单实例锁"
    tasks: ["2.14"]
    checks: [PV4]
    evidence: [reports/wp4-app-daemon.log]
  - id: R2
    source:
      path: specs/daemon-lifecycle/spec.md
      heading: "#### Scenario: 正常启动"
      requirement: "### Requirement: Daemon 前台启动与单实例锁"
    tasks: ["2.14"]
    checks: [PV4]
    evidence: [reports/wp4-app-daemon.log]
  - id: R3
    source:
      path: specs/daemon-lifecycle/spec.md
      heading: "#### Scenario: 重复启动被拒绝"
      requirement: "### Requirement: Daemon 前台启动与单实例锁"
    tasks: ["2.14"]
    checks: [PV4]
    evidence: [reports/wp4-app-daemon.log]
  - id: R4
    source:
      path: specs/daemon-lifecycle/spec.md
      heading: "#### Scenario: endpoint 创建失败即拒绝启动"
      requirement: "### Requirement: Daemon 前台启动与单实例锁"
    tasks: ["2.14"]
    checks: [PV4, PV5]
    evidence: [reports/wp4-app-daemon.log, reports/pv5-windows-ipc.log]
  - id: R5
    source:
      path: specs/daemon-lifecycle/spec.md
      heading: "### Requirement: 配置加载与首次种子导入"
    tasks: ["2.14"]
    checks: [PV4]
    evidence: [reports/wp4-app-daemon.log]
  - id: R6
    source:
      path: specs/daemon-lifecycle/spec.md
      heading: "#### Scenario: 首次启动导入种子"
      requirement: "### Requirement: 配置加载与首次种子导入"
    tasks: ["2.14"]
    checks: [PV4]
    evidence: [reports/wp4-app-daemon.log]
  - id: R7
    source:
      path: specs/daemon-lifecycle/spec.md
      heading: "#### Scenario: 重启不覆盖本地管理改动"
      requirement: "### Requirement: 配置加载与首次种子导入"
    tasks: ["2.14"]
    checks: [PV4]
    evidence: [reports/wp4-app-daemon.log]
  - id: R8
    source:
      path: specs/daemon-lifecycle/spec.md
      heading: "### Requirement: `daemon.status` 由组合根回答"
    tasks: ["2.10", "2.14"]
    checks: [PV3, PV4]
    evidence: [reports/wp3-server-methods.log, reports/wp4-app-daemon.log]
  - id: R9
    source:
      path: specs/daemon-lifecycle/spec.md
      heading: "#### Scenario: 运行中查询状态"
      requirement: "### Requirement: `daemon.status` 由组合根回答"
    tasks: ["2.10", "2.14"]
    checks: [PV3, PV4]
    evidence: [reports/wp3-server-methods.log, reports/wp4-app-daemon.log]
  - id: R10
    source:
      path: specs/daemon-lifecycle/spec.md
      heading: "#### Scenario: Daemon 未运行时查询状态"
      requirement: "### Requirement: `daemon.status` 由组合根回答"
    tasks: ["2.15"]
    checks: [PV4]
    evidence: [reports/wp4-app-cli.log]
  - id: R11
    source:
      path: specs/daemon-lifecycle/spec.md
      heading: "#### Scenario: 有锁但 IPC 不可达"
      requirement: "### Requirement: `daemon.status` 由组合根回答"
    tasks: ["2.15"]
    checks: [PV4]
    evidence: [reports/wp4-app-cli.log]
  - id: R12
    source:
      path: specs/daemon-lifecycle/spec.md
      heading: "### Requirement: `daemon.stop` 与关闭顺序"
    tasks: ["2.10", "2.14"]
    checks: [PV3, PV4]
    evidence: [reports/wp3-server-methods.log, reports/wp4-app-daemon.log]
  - id: R13
    source:
      path: specs/daemon-lifecycle/spec.md
      heading: "#### Scenario: 正常停止"
      requirement: "### Requirement: `daemon.stop` 与关闭顺序"
    tasks: ["2.14"]
    checks: [PV4]
    evidence: [reports/wp4-app-daemon.log]
  - id: R14
    source:
      path: specs/daemon-lifecycle/spec.md
      heading: "#### Scenario: 停止期间新请求失败关闭"
      requirement: "### Requirement: `daemon.stop` 与关闭顺序"
    tasks: ["2.10", "2.14"]
    checks: [PV3, PV4]
    evidence: [reports/wp3-server-methods.log, reports/wp4-app-daemon.log]
  - id: R15
    source:
      path: specs/daemon-lifecycle/spec.md
      heading: "### Requirement: 后台周期任务"
    tasks: ["2.14"]
    checks: [PV4]
    evidence: [reports/wp4-app-daemon.log]
  - id: R16
    source:
      path: specs/daemon-lifecycle/spec.md
      heading: "#### Scenario: 周期任务运行与取消"
      requirement: "### Requirement: 后台周期任务"
    tasks: ["2.14"]
    checks: [PV4]
    evidence: [reports/wp4-app-daemon.log]
  - id: R17
    source:
      path: specs/local-admin-channel/spec.md
      heading: "### Requirement: endpoint 位置与创建"
    tasks: ["2.8"]
    checks: [PV3, PV5]
    evidence: [reports/wp3-server-transport.log, reports/pv5-windows-ipc.log]
  - id: R18
    source:
      path: specs/local-admin-channel/spec.md
      heading: "#### Scenario: Windows 上创建 Named Pipe"
      requirement: "### Requirement: endpoint 位置与创建"
    tasks: ["2.8"]
    checks: [PV5]
    evidence: [reports/pv5-windows-ipc.log]
  - id: R19
    source:
      path: specs/local-admin-channel/spec.md
      heading: "#### Scenario: Unix 上创建 socket 且权限正确"
      requirement: "### Requirement: endpoint 位置与创建"
    tasks: ["2.8"]
    checks: [PV3]
    evidence: [reports/wp3-server-transport.log]
  - id: R20
    source:
      path: specs/local-admin-channel/spec.md
      heading: "### Requirement: OS 用户级访问控制"
    tasks: ["2.8"]
    checks: [PV3, PV5]
    evidence: [reports/wp3-server-transport.log, reports/pv5-windows-ipc.log]
  - id: R21
    source:
      path: specs/local-admin-channel/spec.md
      heading: "#### Scenario: 同一用户连接被接受"
      requirement: "### Requirement: OS 用户级访问控制"
    tasks: ["2.8"]
    checks: [PV3, PV5]
    evidence: [reports/wp3-server-transport.log, reports/pv5-windows-ipc.log]
  - id: R22
    source:
      path: specs/local-admin-channel/spec.md
      heading: "#### Scenario: 跨用户连接被拒绝"
      requirement: "### Requirement: OS 用户级访问控制"
    tasks: ["2.8"]
    checks: [PV3, PV5]
    evidence: [reports/wp3-server-transport.log, reports/pv5-windows-ipc.log]
  - id: R23
    source:
      path: specs/local-admin-channel/spec.md
      heading: "### Requirement: framing 与帧上限"
    tasks: ["2.7"]
    checks: [PV3]
    evidence: [reports/wp3-server-transport.log]
  - id: R24
    source:
      path: specs/local-admin-channel/spec.md
      heading: "#### Scenario: 合法帧被处理"
      requirement: "### Requirement: framing 与帧上限"
    tasks: ["2.7"]
    checks: [PV3]
    evidence: [reports/wp3-server-transport.log]
  - id: R25
    source:
      path: specs/local-admin-channel/spec.md
      heading: "#### Scenario: 空帧或超长帧关闭连接"
      requirement: "### Requirement: framing 与帧上限"
    tasks: ["2.7"]
    checks: [PV3]
    evidence: [reports/wp3-server-transport.log]
  - id: R26
    source:
      path: specs/local-admin-channel/spec.md
      heading: "#### Scenario: 未知 channel 关闭连接"
      requirement: "### Requirement: framing 与帧上限"
    tasks: ["2.7"]
    checks: [PV3]
    evidence: [reports/wp3-server-transport.log]
  - id: R27
    source:
      path: specs/local-admin-channel/spec.md
      heading: "### Requirement: channel 用途绑定"
    tasks: ["2.7", "2.8"]
    checks: [PV3]
    evidence: [reports/wp3-server-transport.log]
  - id: R28
    source:
      path: specs/local-admin-channel/spec.md
      heading: "#### Scenario: channel 混用被拒绝"
      requirement: "### Requirement: channel 用途绑定"
    tasks: ["2.7"]
    checks: [PV3]
    evidence: [reports/wp3-server-transport.log]
  - id: R29
    source:
      path: specs/local-admin-channel/spec.md
      heading: "#### Scenario: `0x02` 连接的 attachment 生命周期"
      requirement: "### Requirement: channel 用途绑定"
    tasks: ["2.7"]
    checks: [PV3]
    evidence: [reports/wp3-server-transport.log]
  - id: R30
    source:
      path: specs/local-admin-channel/spec.md
      heading: "#### Scenario: facade 缺席时的 `0x02` 失败"
      requirement: "### Requirement: channel 用途绑定"
    tasks: ["2.7"]
    checks: [PV3]
    evidence: [reports/wp3-server-transport.log]
  - id: R31
    source:
      path: specs/local-admin-channel/spec.md
      heading: "### Requirement: 未完成请求上限"
    tasks: ["2.7"]
    checks: [PV3]
    evidence: [reports/wp3-server-transport.log]
  - id: R32
    source:
      path: specs/local-admin-channel/spec.md
      heading: "#### Scenario: 超出未完成请求上限"
      requirement: "### Requirement: 未完成请求上限"
    tasks: ["2.7"]
    checks: [PV3]
    evidence: [reports/wp3-server-transport.log]
  - id: R33
    source:
      path: specs/local-admin-methods/spec.md
      heading: "### Requirement: 管理信封校验"
    tasks: ["2.9"]
    checks: [PV3]
    evidence: [reports/wp3-server-envelope.log]
  - id: R34
    source:
      path: specs/local-admin-methods/spec.md
      heading: "#### Scenario: 合法请求获得唯一响应"
      requirement: "### Requirement: 管理信封校验"
    tasks: ["2.9"]
    checks: [PV3]
    evidence: [reports/wp3-server-envelope.log]
  - id: R35
    source:
      path: specs/local-admin-methods/spec.md
      heading: "#### Scenario: 未知方法与参数错误分别报错"
      requirement: "### Requirement: 管理信封校验"
    tasks: ["2.9"]
    checks: [PV3]
    evidence: [reports/wp3-server-envelope.log]
  - id: R36
    source:
      path: specs/local-admin-methods/spec.md
      heading: "#### Scenario: 版本未知即关闭连接"
      requirement: "### Requirement: 管理信封校验"
    tasks: ["2.9"]
    checks: [PV3]
    evidence: [reports/wp3-server-envelope.log]
  - id: R37
    source:
      path: specs/local-admin-methods/spec.md
      heading: "### Requirement: 本地配置方法"
    tasks: ["2.10"]
    checks: [PV3]
    evidence: [reports/wp3-server-methods.log]
  - id: R38
    source:
      path: specs/local-admin-methods/spec.md
      heading: "#### Scenario: workspace.select 规范化路径"
      requirement: "### Requirement: 本地配置方法"
    tasks: ["2.10"]
    checks: [PV3]
    evidence: [reports/wp3-server-methods.log]
  - id: R39
    source:
      path: specs/local-admin-methods/spec.md
      heading: "#### Scenario: workspace.select 拒绝非法路径"
      requirement: "### Requirement: 本地配置方法"
    tasks: ["2.10"]
    checks: [PV3]
    evidence: [reports/wp3-server-methods.log]
  - id: R40
    source:
      path: specs/local-admin-methods/spec.md
      heading: "#### Scenario: provider.configure 不回显凭据"
      requirement: "### Requirement: 本地配置方法"
    tasks: ["2.10"]
    checks: [PV3]
    evidence: [reports/wp3-server-methods.log]
  - id: R41
    source:
      path: specs/local-admin-methods/spec.md
      heading: "#### Scenario: keystore 不可用时失败关闭"
      requirement: "### Requirement: 本地配置方法"
    tasks: ["2.10"]
    checks: [PV3]
    evidence: [reports/wp3-server-methods.log]
  - id: R42
    source:
      path: specs/local-admin-methods/spec.md
      heading: "### Requirement: 设备配对与信任方法"
    tasks: ["2.11"]
    checks: [PV3]
    evidence: [reports/wp3-server-methods.log]
  - id: R43
    source:
      path: specs/local-admin-methods/spec.md
      heading: "#### Scenario: 设备配对全流程"
      requirement: "### Requirement: 设备配对与信任方法"
    tasks: ["2.11"]
    checks: [PV3]
    evidence: [reports/wp3-server-methods.log]
  - id: R44
    source:
      path: specs/local-admin-methods/spec.md
      heading: "#### Scenario: 过期与拒绝不创建信任"
      requirement: "### Requirement: 设备配对与信任方法"
    tasks: ["2.11"]
    checks: [PV3]
    evidence: [reports/wp3-server-methods.log]
  - id: R45
    source:
      path: specs/local-admin-methods/spec.md
      heading: "#### Scenario: 撤销立即生效"
      requirement: "### Requirement: 设备配对与信任方法"
    tasks: ["2.11"]
    checks: [PV3]
    evidence: [reports/wp3-server-methods.log]
  - id: R46
    source:
      path: specs/local-admin-methods/spec.md
      heading: "### Requirement: 节点配对与信任方法"
    tasks: ["2.11"]
    checks: [PV3]
    evidence: [reports/wp3-server-methods.log]
  - id: R47
    source:
      path: specs/local-admin-methods/spec.md
      heading: "#### Scenario: Owner 模式节点配对"
      requirement: "### Requirement: 节点配对与信任方法"
    tasks: ["2.11"]
    checks: [PV3]
    evidence: [reports/wp3-server-methods.log]
  - id: R48
    source:
      path: specs/local-admin-methods/spec.md
      heading: "#### Scenario: access 模式明确不支持"
      requirement: "### Requirement: 节点配对与信任方法"
    tasks: ["2.11"]
    checks: [PV3]
    evidence: [reports/wp3-server-methods.log]
  - id: R49
    source:
      path: specs/local-admin-methods/spec.md
      heading: "### Requirement: Export 与 Import 管理方法"
    tasks: ["2.12"]
    checks: [PV3]
    evidence: [reports/wp3-server-methods.log]
  - id: R50
    source:
      path: specs/local-admin-methods/spec.md
      heading: "#### Scenario: 创建 Export 成功"
      requirement: "### Requirement: Export 与 Import 管理方法"
    tasks: ["2.12"]
    checks: [PV3]
    evidence: [reports/wp3-server-methods.log]
  - id: R51
    source:
      path: specs/local-admin-methods/spec.md
      heading: "#### Scenario: 有参数 template 被拒绝"
      requirement: "### Requirement: Export 与 Import 管理方法"
    tasks: ["2.12"]
    checks: [PV3]
    evidence: [reports/wp3-server-methods.log]
  - id: R52
    source:
      path: specs/local-admin-methods/spec.md
      heading: "#### Scenario: import.add 无快照时明确不可重试语义"
      requirement: "### Requirement: Export 与 Import 管理方法"
    tasks: ["2.12"]
    checks: [PV3]
    evidence: [reports/wp3-server-methods.log]
  - id: R53
    source:
      path: specs/local-admin-methods/spec.md
      heading: "### Requirement: 审计导出方法"
    tasks: ["2.12"]
    checks: [PV3]
    evidence: [reports/wp3-server-methods.log]
  - id: R54
    source:
      path: specs/local-admin-methods/spec.md
      heading: "#### Scenario: 导出审计记录"
      requirement: "### Requirement: 审计导出方法"
    tasks: ["2.12"]
    checks: [PV3]
    evidence: [reports/wp3-server-methods.log]
  - id: R55
    source:
      path: specs/local-admin-methods/spec.md
      heading: "#### Scenario: 输出路径冲突"
      requirement: "### Requirement: 审计导出方法"
    tasks: ["2.12"]
    checks: [PV3]
    evidence: [reports/wp3-server-methods.log]
  - id: R56
    source:
      path: specs/local-admin-methods/spec.md
      heading: "### Requirement: 重试与失败语义"
    tasks: ["2.12"]
    checks: [PV3]
    evidence: [reports/wp3-server-methods.log]
  - id: R57
    source:
      path: specs/local-admin-methods/spec.md
      heading: "#### Scenario: mutation 重试得到确定性错误"
      requirement: "### Requirement: 重试与失败语义"
    tasks: ["2.12"]
    checks: [PV3]
    evidence: [reports/wp3-server-methods.log]
  - id: R58
    source:
      path: specs/local-admin-methods/spec.md
      heading: "#### Scenario: 审计事件覆盖拒绝与撤销"
      requirement: "### Requirement: 重试与失败语义"
    tasks: ["2.11", "2.12"]
    checks: [PV3]
    evidence: [reports/wp3-server-methods.log]
  - id: R59
    source:
      path: specs/cli-commands/spec.md
      heading: "### Requirement: CLI 子命令映射与薄客户端约束"
    tasks: ["2.15"]
    checks: [PV4]
    evidence: [reports/wp4-app-cli.log]
  - id: R60
    source:
      path: specs/cli-commands/spec.md
      heading: "#### Scenario: 子命令经本地通道调用对应方法"
      requirement: "### Requirement: CLI 子命令映射与薄客户端约束"
    tasks: ["2.15"]
    checks: [PV4]
    evidence: [reports/wp4-app-cli.log]
  - id: R61
    source:
      path: specs/cli-commands/spec.md
      heading: "#### Scenario: Daemon 未运行时管理命令明确失败"
      requirement: "### Requirement: CLI 子命令映射与薄客户端约束"
    tasks: ["2.15"]
    checks: [PV4]
    evidence: [reports/wp4-app-cli.log]
  - id: R62
    source:
      path: specs/cli-commands/spec.md
      heading: "### Requirement: 退出码与错误输出契约"
    tasks: ["2.15"]
    checks: [PV4]
    evidence: [reports/wp4-app-cli.log]
  - id: R63
    source:
      path: specs/cli-commands/spec.md
      heading: "#### Scenario: 失败输出的双通道形态"
      requirement: "### Requirement: 退出码与错误输出契约"
    tasks: ["2.15"]
    checks: [PV4]
    evidence: [reports/wp4-app-cli.log]
  - id: R64
    source:
      path: specs/cli-commands/spec.md
      heading: "### Requirement: 配对安全确认仪式"
    tasks: ["2.16"]
    checks: [PV4]
    evidence: [reports/wp4-app-cli.log]
  - id: R65
    source:
      path: specs/cli-commands/spec.md
      heading: "#### Scenario: 交互式设备配对确认"
      requirement: "### Requirement: 配对安全确认仪式"
    tasks: ["2.16"]
    checks: [PV4]
    evidence: [reports/wp4-app-cli.log]
  - id: R66
    source:
      path: specs/cli-commands/spec.md
      heading: "#### Scenario: 非交互配对逐字校验"
      requirement: "### Requirement: 配对安全确认仪式"
    tasks: ["2.16"]
    checks: [PV4]
    evidence: [reports/wp4-app-cli.log]
  - id: R67
    source:
      path: specs/cli-commands/spec.md
      heading: "### Requirement: 结构化输入与凭据交互"
    tasks: ["2.16"]
    checks: [PV4]
    evidence: [reports/wp4-app-cli.log]
  - id: R68
    source:
      path: specs/cli-commands/spec.md
      heading: "#### Scenario: 凭据无回显录入"
      requirement: "### Requirement: 结构化输入与凭据交互"
    tasks: ["2.16"]
    checks: [PV4]
    evidence: [reports/wp4-app-cli.log]
  - id: R69
    source:
      path: specs/cli-commands/spec.md
      heading: "#### Scenario: 非交互环境拒绝凭据录入"
      requirement: "### Requirement: 结构化输入与凭据交互"
    tasks: ["2.16"]
    checks: [PV4]
    evidence: [reports/wp4-app-cli.log]
  - id: R70
    source:
      path: specs/cli-commands/spec.md
      heading: "### Requirement: `doctor` 与 `acp-stdio`"
    tasks: ["2.17"]
    checks: [PV4]
    evidence: [reports/wp4-app-cli.log]
  - id: R71
    source:
      path: specs/cli-commands/spec.md
      heading: "#### Scenario: doctor 离线完成"
      requirement: "### Requirement: `doctor` 与 `acp-stdio`"
    tasks: ["2.17"]
    checks: [PV4]
    evidence: [reports/wp4-app-cli.log]
  - id: R72
    source:
      path: specs/cli-commands/spec.md
      heading: "#### Scenario: acp-stdio 在 Daemon 未运行时明确退出"
      requirement: "### Requirement: `doctor` 与 `acp-stdio`"
    tasks: ["2.17"]
    checks: [PV4]
    evidence: [reports/wp4-app-cli.log]
```

## Work Packages

| ID | Goal / Scenarios | Dependencies | Owner | Reviewer | Branch / Worktree | Write Scope | Inputs / Outputs | Verification |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| WP1 | 契约与依赖口径冻结：把 `MODULE_ARCHITECTURE.md` §3/§3.1/§4.9/§4.10/§5 的本轮范围写到位（`server` 仅本地通道与 `local_admin`、矩阵缺列收敛），`LOCAL_ADMIN_PROTOCOL.md` §3.1 补实现状态注记，`[workspace.dependencies]` 与 `workspace.exclude`、`deny.toml` 的 path 来源登记 | 无（W0 起点） | 实现 Agent（coder） | 非文档作者（RV1） | `feat/daemon-cli-and-local-admin` / 主 worktree | `docs/MODULE_ARCHITECTURE.md`、`docs/LOCAL_ADMIN_PROTOCOL.md`（仅 §3.1 注记）、`Cargo.toml`、`deny.toml` | 输入：`design.md` D1/D3/D5/D9、`LOCAL_ADMIN_PROTOCOL.md` 现行版；输出：可被 `check:boundaries`/`check:doc-links` 读取的口径 | [PV1]、[PV2]（W0 阶段成员未加入，PV2 只核对既有成员） |
| WP2 | `vendor/windows-local-ipc` 自研 wrapper：safe API「以 SDDL 创建 Named Pipe」「查询对端 SID」两个函数（`cfg(windows)`），非 Windows 编译通过且运行期明确失败；unsafe 收敛在模块级并附不变量注释 | WP1 | 实现 Agent（coder） | 非实现者（RV1） | 同上 | `vendor/windows-local-ipc/**` | 输入：`design.md` D3、`LOCAL_ADMIN_PROTOCOL.md` §2.1/§2.2；输出：可被 `server` 以 path 依赖消费的 crate | [PV5]（Windows 本机）+ 非 Windows 编译断言 |
| WP3 | `server` crate：`transport::local`（endpoint、ACL、对端凭据、framing、channel 绑定、未完成请求上限、facade 缺席期 `0x02` 即连即关）与 `local_admin`（信封编解码 + schema/fixture 漂移测试、方法路由：本地配置族/配对/Export/Import/audit、`DaemonControl` 注入）；覆盖 `local-admin-channel` 与 `local-admin-methods` 全部需求与场景 | WP1、WP2 | 实现 Agent（coder） | 非实现者（RV1） | 同上 | `crates/server/**`（含 `Cargo.toml`）、`[workspace] members` 本 crate 条目、`Cargo.lock` | 输入：`design.md` D1/D5/D6、`LOCAL_ADMIN_PROTOCOL.md` 全文、`schemas/local-admin/v1/`、`fixtures/local-admin/v1/`、`core::use_cases` 与端口、`identity-auth` 状态机与 keystore 端口；输出：可被 `app` 装配的入站适配器 | [PV3]（+ [PV1]/[PV2]） |
| WP4 | `app` crate：组合根（配置加载与种子导入、单实例锁与 `instanceId`、启动/关闭序列、周期任务装配）、`daemon status|stop` 接线、CLI 全部子命令（映射/退出码/stderr JSON/配对仪式/凭据交互/`--file`）、`doctor`、`acp-stdio` 字节泵；真实子进程集成测试；覆盖 `daemon-lifecycle` 与 `cli-commands` 全部需求与场景 | WP1、WP3 | 实现 Agent（coder） | 非实现者（RV1） | 同上 | `crates/app/**`（含 `Cargo.toml`）、`[workspace] members` 本 crate 条目、`Cargo.lock` | 输入：`design.md` D2/D4/D6/D7/D8、WP3 的服务端形状、`CONFIG_REFERENCE.md`；输出：`acp-remote` 可执行程序 | [PV4]（+ [PV1]/[PV2]） |
| WP5 | 文档与状态收口：`MODULE_ARCHITECTURE.md` §5 缺列注记收敛与已落地标记、`README.md`「仓库当前状态」、`docs/DEVELOPMENT_PLAN.md` §2、`AGENTS.md` §4；全量统一入口 | WP3、WP4 | 实现 Agent（coder） | 非文档作者（RV1） | 同上 | `docs/MODULE_ARCHITECTURE.md`（§5 注记与状态）、`README.md`、`docs/DEVELOPMENT_PLAN.md`、`AGENTS.md` | 输入：WP3/WP4 的测试证据；输出：与代码一致的仓库状态 | [PV1]、[PV2]（成员已加入后逐条核对 §5） |

写范围说明（如实登记重叠）：`Cargo.toml` 的 `[workspace] members` 由 WP3 与 WP4 各写一次（按波次串行）；`Cargo.lock` 随成员与直接依赖各变更一次；`docs/MODULE_ARCHITECTURE.md` 由 WP1（口径）与 WP5（已落地标记）先后串行写入；`reports/*` 按检查 ID 一文件一写者。

说明：Main E2E mode 为 `not-applicable`，因此不生成独立的测试工作包（TP）与 Test Design 分组；各工作包自带其行为测试（局部自检），最终替代验证由主 Agent 任务（7.1）统一执行并留证。

## Execution Waves

| Wave | Work Packages | 依赖满足条件 | 并发上限 | 说明 |
| --- | --- | --- | --- | --- |
| W0 契约与依赖冻结（串行，单执行者） | 1.1 → WP1（2.1–2.3） | 已完成的仓库工作区 | 1 | 文档与依赖登记在 W0 只有一个写入者；完成条件 = [PV1]/[PV2] 绿 + 一个基线提交 |
| W1 `windows-local-ipc` | WP2（2.4–2.6） | W0 的基线提交 | 1 | wrapper 的 safe API 形状在本波冻结，供 WP3 消费 |
| W2 `server` | WP3（2.7–2.13） | WP1 口径 + WP2 API 冻结 | 1 | 本波把 `crates/server` 写入 `members`；方法路由的公共形状（`DaemonControl`、信封类型）本波冻结 |
| W3 `app` | WP4（2.14–2.18） | WP3 服务端形状冻结 | 1 | 本波把 `crates/app` 写入 `members`；`Cargo.toml` 两次成员写入按 W2 → W3 串行 |
| W4 文档收口 | WP5（2.19–2.20） | WP3、WP4 完成 | 1 | 已落地标记只在实现确实完成后写入 |
| W5 交付前验证 | 3.1–3.10 | W0–W4 完成 | 1 | [PV1]–[PV5] 与 RV1；review 必须由不继承实现对话的 reviewer 完成 |
| W6 集成与合入 | 5.1–5.2 → 6.1–6.8 | 全部 3.x 完成 | 1 | `integrated` 单一交付单元；每个目标分支只允许一个集成执行者串行更新 |
| W7 最终替代验证与验收 | 7.1–7.3 → 8.1 | 6.7、6.8 | 1 | `not-applicable` 的替代验证 + `[e2e-owned]` 门禁 + 最终验收 |

**多 Agent 分派规则**（宿主支持子 Agent / 可开独立会话时）：

- 每个 WP 一个独立 coder 执行者；构建目录用 `CARGO_TARGET_DIR` 隔离，不允许两个执行者共用同一个 `target/`。本变更的五条轨道存在顺序依赖（口径 → wrapper → server → app → 收口），默认**串行**；无并发能力时按 roles 串行执行并如实记录，串行不改变各 WP 的完成条件与证据要求。
- 每个 WP 的交付前独立 review（RV1）必须由**不继承实现对话**的执行者完成；缺少隔离上下文时相关任务记 BLOCKED，不得以自审替代。
- 只有主 Agent 可写 `plan.md`/`tasks.md`/`verification.md`；子 Agent 返回结构化 handoff（固定提交、命令、日志路径、差异范围）。
- 文件单一写入者：`docs/MODULE_ARCHITECTURE.md` 与 `docs/LOCAL_ADMIN_PROTOCOL.md`（WP1，随后 WP5 在 W4 串行接管）、`Cargo.toml`/`deny.toml`（WP1 登记依赖，WP3/WP4 各加 members 一行）、`vendor/windows-local-ipc/**`（WP2）、`crates/server/**`（WP3）、`crates/app/**`（WP4）、`README.md`/`docs/DEVELOPMENT_PLAN.md`/`AGENTS.md`（WP5）。
- Windows 本机的 [PV5] 与其它使用同一用户 profile 的检查串行执行（Named Pipe 命名空间是本机资源，靠 `instanceId` 随机段隔离）。

## Dependency Handoffs

| Downstream | Upstream | Accepted Revision / Evidence | Start Revision | Transfer / Inclusion Check | Invalidation |
| --- | --- | --- | --- | --- | --- |
| WP2 | WP1 | 2.1–2.3 的依赖口径（`workspace.exclude` 与 `deny.toml` path 登记）与 [PV1]/[PV2] 通过证据 | 变更分支起点 | `cargo metadata --no-deps` 可解析且 `vendor/` 不在 workspace 成员内 | 依赖口径变化 → WP2–WP5 复验 |
| WP3 | WP1、WP2 | WP2 冻结的两个 safe 函数签名 + 3.3 的 [PV5]/编译断言证据 | W1 完成点 | `cargo build -p server` 仅经 path 依赖使用 wrapper 的公开 API；`check:boundaries` 的 `server` 行通过 | wrapper API 或 §5 矩阵口径变化 → WP3/WP4 复验 |
| WP4 | WP1、WP3 | WP3 冻结的 `DaemonControl`、信封类型与方法路由形状 + 3.5 的 [PV3] 证据 | W2 完成点 | `cargo build -p app` 只使用 `server` 的公开项；CLI 不复制信封定义 | 服务端公共形状变化 → WP4/WP5 复验 |
| WP5 | WP3、WP4 | [PV3]/[PV4]（+ [PV5]）证据与 RV1 review 报告 | W3 完成点 | 文档中的「已落地」与 `cargo metadata` 实际一致 | 成员集合或能力状态变化 → WP5 复验 |
| DU1 | WP1–WP5 | 各 WP 的检查证据与 RV1 review 报告 | W5 完成点 | `npm run verify` 全绿 + 变更 diff 无未登记文件 | 任一上游变化 → 候选重建、[PV1] 重跑 |

## Runtime Resources

| Checks / Work Packages | Resource | Isolation / Configuration | Exclusive Scheduling | Owner / Cleanup |
| --- | --- | --- | --- | --- |
| [PV1]–[PV5]、WP1–WP5 | Rust 构建目录 `target/` | 每个并行执行者设自己的 `CARGO_TARGET_DIR`；本变更默认串行，同一 `target/` 不得并发写 | 按 WP 分阶段串行；同 crate 并发时独占构建目录 | 各执行者，只清理自身产生的临时目录与 worktree |
| [PV3]、WP3 | 临时数据目录（每个集成用例一个独立目录） | 用例自建临时目录并在结束时删除；不写仓库或共享临时路径 | 无（目录级隔离） | 实现 Agent |
| [PV4]、WP4 | 临时 Daemon 数据目录、单实例锁文件、Unix socket 目录 / Windows pipe 名 | 每个集成用例一个独立 data dir；pipe 名含随机 `instanceId` | 无（目录/命名隔离）；同机多轮测试串行 | 实现 Agent |
| [PV5]、WP2/WP3 的 Windows 用例 | 本机 Windows Named Pipe 命名空间与当前用户 SID | 无法在 Linux CI 复现；必须在本地 Windows 执行并保存原始日志 | 同一时刻只允许一轮 Windows IPC 用例 | 主 Agent／实现 Agent |

说明：本变更不涉及数据库服务、容器、端口、外部账号或网络等共享运行资源；`target/`、临时数据目录与本机 pipe 命名空间是本变更仅有的运行设施，已按上表隔离。

## Target Repository and Main Branch

- Code Repository: `D:\Project\acp-remote`
- Target Kind / Ref: local；`refs/heads/main`
- Version Confirmation Owner: 主 Agent（机械核实可派发 environment/recon）
- Confirmation Method / Evidence: `git rev-parse refs/heads/main`、`git status --porcelain` 与 `git worktree list`；实际提交与核实结果记录在 `verification.md`

## Merge Strategy

| Delivery Unit / Mode | WP / TP | Owners: Implementation / Test / Review / Merge | Start / Readiness | Candidate Checks / Critical E2E | Order |
| --- | --- | --- | --- | --- | --- |
| DU1 / integrated | WP1–WP5 | 实现 Agent / 实现 Agent（各 WP 自带测试）/ 独立 reviewer / 主 Agent（按当前授权） | 契约口径冻结 + wrapper API 冻结 + server 公共形状冻结 | [PV1]、[PV2]、[PV3]、[PV4]、[PV5]；无关键 E2E（mode = not-applicable） | 唯一单元，先候选后合入 |

- Integration Branch / Worktree: `feat/daemon-cli-and-local-admin`（主 worktree；如需并行分片则用 `CARGO_TARGET_DIR` 隔离构建目录）
- Source Revision / Handoff: 变更分支起点 = 计划确认时的 `refs/heads/main` 提交（在 6.1 记录固定值）
- Operation Boundary / Report Path: apply 覆盖本地合入；推送远端、回滚与发布另需明确授权；集成报告写入 `openspec/changes/daemon-cli-and-local-admin/reports/du1-integration.md`

采用 integrated 的理由：`Cargo.toml` 的 `members`、`MODULE_ARCHITECTURE.md` §5 的记载与两个新 crate 的实现/测试由 `check:boundaries` 相互绑定；`app` 的编译依赖 `server` 的公开形状，`server` 的 Windows 路径依赖 `vendor/windows-local-ipc`。任何一方单独先合入都会让 `main` 上的 `npm run check` 或 `cargo build --workspace` 变红，因此必须作为一个交付单元一次性合入。

## Verification Strategy

### Local Checks

- 实现期：`cargo fmt --all -- --check`、`cargo clippy --locked -p server -p app --all-targets --all-features -- -D warnings`、`cargo test --locked -p server --all-features`、`cargo test --locked -p app --all-features`；`vendor/windows-local-ipc` 单独 `cargo build/test`（它不在 workspace 内）。
- 合同侧：`node scripts/check-crate-boundaries.mjs`、`node scripts/check-doc-links.mjs`（改文档后必跑）、`node scripts/check-command-catalog.mjs`（确认 `commands.json` 与相关表格未被改动）。
- 平台侧：Windows 本机执行 server/app 的 `cfg(windows)` 用例并保存原始日志。
- 审查重点自检：`grep -rn "unwrap()\|expect(\|panic!\|unreachable!" crates/server/src crates/app/src`（**可失败路径**零命中；已确认为不变的构造与测试内部断言不计入，命中处必须在 `verification.md` 逐条登记理由）、`rg -n "unsafe" crates/`（workspace crate 零命中；unsafe 只允许出现在 `vendor/windows-local-ipc`）、`rg -n "rusqlite|sqlx|SQLite" crates/app/src`（CLI 路径不得直连数据库）。

### Project Verify

| Check ID | Stages / Work Packages | Command / Working Directory | Configuration / Environment | Scope / Pass Criteria | Evidence |
| --- | --- | --- | --- | --- | --- |
| PV1 | 候选、主分支、最终（WP1–WP5） | `npm run verify`（= `npm run check` + `cargo fmt --all -- --check` + `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` + `cargo test --locked --workspace --all-features`）@ `D:\Project\acp-remote` | Node ≥ 22.12、`rust-toolchain.toml` 固定的工具链；不联网 | 退出码 0 且逐子项无失败、无零测试、无全跳过；`npm run check` 的合同门禁全绿 | `openspec/changes/daemon-cli-and-local-admin/reports/du1-pv1.log`、`openspec/changes/daemon-cli-and-local-admin/reports/du1-main-verify.log` |
| PV2 | 候选、主分支（WP1、WP5） | `node scripts/check-crate-boundaries.mjs` @ 仓库根 | 需要本地 `cargo`（`cargo metadata`/`cargo tree`），不联网 | §5 矩阵逐条成立：`server` 行只依赖矩阵允许的对象、`app` 可依赖全部、`core` 闭包与 allow-list 逐项相等；`vendor/windows-local-ipc` 不是 workspace 成员 | `reports/wp1-boundaries.log`、`openspec/changes/daemon-cli-and-local-admin/reports/du1-pv1.log` |
| PV3 | 各 WP 交付前、最终（WP3） | `cargo test --locked -p server --all-features` @ 仓库根 | 本地 Rust 工具链；临时数据目录（用例自建自删） | framing/channel 绑定/上限/信封校验/方法路由/错误码/schema 与 fixture 漂移/审计导出边界全部通过，且新增用例确实被执行（无 0 用例 / 无全跳过） | `openspec/changes/daemon-cli-and-local-admin/reports/wp3-server-transport.log`、`openspec/changes/daemon-cli-and-local-admin/reports/wp3-server-envelope.log`、`openspec/changes/daemon-cli-and-local-admin/reports/wp3-server-methods.log` |
| PV4 | 各 WP 交付前、最终（WP4） | `cargo test --locked -p app --all-features` @ 仓库根 | 本地 Rust 工具链；临时 Daemon 数据目录 | 生命周期/锁/种子导入/CLI 映射/退出码契约/配对仪式/凭据交互/`acp-stdio` 明确退出全部通过（真实子进程集成测试），无 0 用例 / 无全跳过 | `openspec/changes/daemon-cli-and-local-admin/reports/wp4-app-daemon.log`、`openspec/changes/daemon-cli-and-local-admin/reports/wp4-app-cli.log` |
| PV5 | 候选、主分支（WP2 与 WP3/WP4 的 Windows 用例） | Windows 本机：`cargo test --locked -p server --all-features windows -- --nocapture`、`cargo test --locked -p app --all-features windows -- --nocapture` 及 `vendor/windows-local-ipc` 的自身测试 @ `D:\Project\acp-remote` | **只在 Windows x64 执行**；Linux CI 只覆盖 `#[cfg(unix)]` 与共享路径 | SDDL 创建、同用户接受/跨用户拒绝的对端凭据校验、`acp-stdio` 在 Daemon 缺席时明确退出，输出原始日志 | `openspec/changes/daemon-cli-and-local-admin/reports/pv5-windows-ipc.log` |
| RV1 | 各 WP 交付前、主分支复核 | 独立 reviewer 按 `roles/reviewer.md` 在隔离上下文检视固定版本 diff 与契约 | 只读；不修改代码与证据 | 报告完整、无未解决阻断项；覆盖 specs 场景与 design 决策 | `openspec/changes/daemon-cli-and-local-admin/reports/rv1-wp1.md` … `rv1-wp5.md`、`rv1-du1.md` |

### Code Review

- 范围：WP1（口径与 design D1/D3/D5 一致、依赖登记与 `deny.toml`）、WP2（unsafe 收敛范围与不变量注释、safe API 语义、非 Windows 失败关闭）、WP3（framing 关闭规则逐条、对端凭据校验不可绕过、信封 closed object 与 `v` 规则、方法路由是否只经 `core::use_cases`、凭据值是否可能进入日志/错误/`Debug`、审计导出内容边界）、WP4（CLI 是否零业务规则、是否绝不打开 SQLite、退出码/stderr 契约、SAS/指纹逐字校验、关闭顺序与任务取消）、WP5（状态表与 `cargo metadata` 实际一致）。
- 关注点：① 任何路径不经对端凭据校验就处理首帧；② framing 错误是否可能返回错误帧；③ CLI 是否存在直连 SQLite 或自启核心的路径；④ pairingUrl/secret/凭据是否可能进入日志、shell 可见参数或文件；⑤ `unsafe` 是否只出现在 `vendor/windows-local-ipc`；⑥ 是否为通过检查而放宽 lint、抬高 MSRV 或自造管理方法/错误码。
- 阻断标准：任一规格场景缺覆盖、任何 [PV2] 依赖差异、任何秘密材料出现在可观察位置、任何「Daemon 缺席时打开数据库」的路径、任何绕过对端凭据校验的连接、任何 workspace crate 内的 unsafe。
- 复核安排：修复后由**新的**隔离子 Agent 对新固定版本复核，并保留原报告与复核结论。

### Main E2E

```yaml
mode: not-applicable
reason: "本变更是首个产生可执行二进制的切片，但产品级端到端路径仍不存在：`server::node_link`、`node-link-client`、`server::acp_facade`（ACP 语义）与 `server::sync`/前端均未实现，`import.add` 无 catalog 快照、`node.pair.begin mode = access` 返回 local.unsupported、channel 0x02 在 facade 缺席期即连即关；没有第二个节点、没有真实 Agent 会话、没有可驱动的产品闭环。可运行的最大闭环（CLI ↔ Daemon 本机管理）已由 PV4 的真实子进程集成测试覆盖，不构成跨节点/跨客户端的产品 E2E 场景。"
basis: "`openspec/config.yaml` 的 context 规则（当前阶段没有可端到端运行的产品路径时按变更记 not-applicable 并逐变更批准）与 `x-agentic.e2e.command` 为空的事实；切片 2/3 的同款处理已归档（`2026-09-24-acp-boundary-and-agent-host`、`2026-09-24-identity-auth-and-keystore`）。本变更不改 wire 协议与封闭词表，不影响 E2E 适用性判断。"
alternative_checks:
  - "cargo test --locked -p server --all-features [PV3]：framing 关闭规则、channel 绑定与 attachment 生命周期、未完成请求上限、信封校验与 local.* 错误码、方法路由到 core 用例（fake 端口）、schema/fixture 漂移、审计导出内容边界。"
  - "cargo test --locked -p app --all-features [PV4]：真实子进程集成测试——daemon start/stop/status、重复 start 拒绝、种子导入与重启不覆盖、CLI 全子命令映射与退出码/stderr JSON 契约、配对仪式（含非交互 --sas/--fingerprint 逐字校验）、provider 凭据交互、acp-stdio 在 Daemon 缺席时明确退出。"
  - "Windows 本机 [PV5]：Named Pipe SDDL 创建与对端 SID 校验（同用户接受/跨用户拒绝）、vendor/windows-local-ipc 自身测试。"
  - "npm run check / npm run verify [PV1]/[PV2]：合同门禁与全 workspace 测试，确认新增成员、vendor crate 与依赖不破坏既有 crate。"
downgrade_approval: "2026-09-25，本会话，用户原话：「1. 同意降级」。对应本会话提问第 1 项（本变更 Main E2E 记 not-applicable 及上述替代验证清单：本变更相关 cargo 测试含 CLI↔Daemon 真实子进程集成测试 + npm run check / npm run verify），来源为用户对该提问的批准；本记录不沿用 2026-09-23 的首次确认，也不沿用 2026-09-24 两次实现切片的批准。"
```

#### E2E Ownership and Cases

不适用（mode = not-applicable），本节按模板要求删除；实际替代验证已列为主 Agent 任务（7.1）并在 Coverage Index 中引用。

## Failure and Recovery

- 失败修复：按原 WP ID 重新派发（不新增 WP），修复后重跑该 WP 的 [PV3]/[PV4]/[PV5] 与 RV1；受影响的下游按 Dependency Handoffs 的失效列重开。
- 传递下游失效：`LOCAL_ADMIN_PROTOCOL.md` 口径或 §5 矩阵变化 → WP3–WP5 与 DU1 的既有证据失效；wrapper API 变化 → WP3/WP4 复验；服务端公共形状（`DaemonControl`、信封类型）变化 → WP4/WP5 复验。
- 主分支失败：`main` 上的 `npm run verify` 失败时停止后续合入，按同一验证路径交付修复；必要时回滚该交付单元（`git revert` 或恢复候选前提交），不允许「红着继续合」。
- 共享资源异常：临时数据目录/锁文件/socket 残留时只删自身用例创建的资源；`target/` 污染时 `cargo clean -p server -p app` 后重跑受影响检查。
- 平台限制：[PV5] 无法在 Linux CI 复现；若本地 Windows 不可用，该检查记 BLOCKED 并如实上报，不得用 Unix 路径的结果替代。
- 无 E2E 执行，故不涉及 E2E 失败上限；若后续有人把 mode 改为 required，必须先取得新的用户批准并补齐 E2E 任务与入口。
- 未执行项：`cargo-deny`（`deps`/`advisories`）与 `gitleaks`（`secrets`）本地没有等价物，只在 CI 运行；本变更新增依赖（clap、toml、文件锁 crate、tokio/nix 新 feature）与 `vendor/windows-local-ipc` 的许可证/来源/advisory 判定**只在 CI 完成**，本地 `npm run verify` 不是证据，最终验收中明确记录。

## Completion Criteria

- 全部任务勾选（`verification.md` 中逐 ID 关联证据；结果仅 PASS/FAIL/BLOCKED/NOT_APPLICABLE）。
- 最终主分支版本（`refs/heads/main` 的实际提交）上：[PV1]、[PV2]、[PV3]、[PV4] 全绿；[PV5] 在本地 Windows 上执行并留证（或如实记 BLOCKED）；RV1 无未解决阻断项；E2E 记 NOT_APPLICABLE 且替代验证（7.1）已完成并留证。
- 阻断问题清零：无未闭环的 FAIL/BLOCKED、无未登记漂移、无秘密材料进入日志/错误/测试快照/普通文件、无「Daemon 缺席时打开数据库或自启核心」的路径、workspace crate 内无 unsafe。
- 最终验收由主 Agent 按 `.agents/skills/agentic-verify/SKILL.md` 执行，并在验收块中以唯一 `[final-verification]` 任务记录。
- 本计划不构成合并、推送、回滚或发布授权。
