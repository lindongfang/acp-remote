<!-- main 维护安排，tasks 写步骤，verification 写历史；协作判据见本次 plan instruction，角色细则见 roles/。 -->

## Scope and Contracts

- Specs Revision: `specs/sync-snapshot-scope/spec.md`（新建，4 需求 / 13 场景）、`specs/core-derived-events/spec.md`（新建，5 需求 / 19 场景）、`specs/pwa-web-client/spec.md`（新建，11 需求 / 36 场景）、`specs/workspace-resolution/spec.md`（MODIFIED「路径不泄漏」，1 需求 / 4 场景）、`specs/local-agent-host/spec.md`（ADDED 2 需求 / 7 场景）。（Round 10 的 F43：原写 15 / 35 / 4 场景三处与各 spec 的实际 `#### Scenario:` 计数不符，已按 `grep -c "^#### Scenario:"` 逐个复算改正）
- Design Revision: `design.md`，8 条决策 D1–D8，含 D1 快照收窄、D2 复合游标、D3 分页只在 Sync 侧、D4 行级 diff、D5 路径相对化、D6 节点级 Agent 状态、D7 标题单向更新、D8 前端七层与单一数据入口。
- Convergence Check: 一致。design 的 D2 游标口径与 specs「游标由时间与标识复合构成」逐条对齐；D5 的越界标记与 `workspace-resolution` 的 MODIFIED 正文一致；D6 的「列表来自快照、事件作覆盖层」与 `pwa-web-client` 的「Agent 目录与连接状态分两层呈现」一致；D1 的快照三资源与 `pwa-web-client` 的目录页离线要求一致。无待澄清项。
- Skip Specs: no

## Contract Changes

| 契约 | 原 | 新 | 原因 | 受影响安排 |
| --- | --- | --- | --- | --- |
| `sync.schema.json#/$defs/snapshotResource` | 8 个取值 | 保留枚举，仅 `sessions`/`workspaces`/`agents` 会被下发 | 与 `NODE_LINK_PROTOCOL.md` §12.4「正文绝不入快照」对齐 | WP1、TP1、WP5a、TP3 |
| `command.schema.json#/$defs/sessionRead` | payload 只有 `{include}` | 增加可选 `before`（复合游标）与 `limit` | 明细必须可翻页；无游标时单页可能撞 1 MiB 上限 | WP1、TP1、WP3、WP6、TP3 |
| `command.schema.json#/$defs/sessionReadResult` | `{sessionId, resources}` | 增加「是否仍有更早内容」布尔值 | 客户端据此决定是否继续加载 | WP1、TP1、WP6、TP3 |
| `event-views.schema.json#/$defs/file.changed` | 4 个必填字段 | 增加可选 `addedLines`/`deletedLines`/`outsideWorkspace` | 原型右栏的 `+N −M` 与工作区外提示无数据源 | WP2、TP1、WP3、WP7 |
| `event-views.schema.json#/$defs/agent.connected`+`/agent.disconnected` | `state: NonEmptyText<32>` | `state` 收成封闭枚举，各自唯一取值 | 客户端没有映射依据；fixture 已用固定取值 | WP2、TP1、WP4、WP7 |
| `crates/core/src/ports.rs#SessionUpdate` | 无标题字段 | 增加两层可选标题字段 | 目录页现在只能显示「未命名会话」 | WP3、TP2 |
| `docs/SYNC_PROTOCOL.md` §9.4 资源范围 | 仅 imported 会话豁免明细 | 全部会话豁免 | D1 | WP1 |

## Coverage Index


每个增量 Requirement 一行；其全部 Scenario 作为该行的验证材料，由对应 TP 覆盖并在 verification 的 `## Checks` 逐条记录。

| Coverage ID | Requirement | Source | Responsible Units | Candidate Checks / Contribution | Closure Unit / Stage | Final Checks |
| --- | --- | --- | --- | --- | --- | --- |
| R1 | 快照只承载清单类资源 | `specs/sync-snapshot-scope/spec.md` | WP1, TP1 | WP1: schema/fixture/文档收窄并让 `npm run check` 全绿；TP1: 清单快照固定向量 | MU1b/premerge | IV1, EV1 |
| R2 | 会话明细经分页读取 | `specs/sync-snapshot-scope/spec.md` | WP1, TP1, WP6 | WP1: payload/result 形状与错误码；WP6: 客户端按页拉取与拼接 | MU3c/premerge | IV1, EV1 |
| R3 | 游标由时间与标识复合构成 | `specs/sync-snapshot-scope/spec.md` | WP1, TP1, WP6 | WP1: 游标 schema 与排序约束；WP6: 客户端游标构造 | MU3c/premerge | IV1, EV1 |
| R4 | 分页只作用于客户端同步面 | `specs/sync-snapshot-scope/spec.md` | WP1, TP1 | WP1: 文档写明适用面；TP1: 断言 node-link-protocol 的 DTO 未变 | MU1b/premerge | IV1 |
| R5 | 文件改动事件由类型化 Diff 派生 | `specs/core-derived-events/spec.md` | WP2, TP1, WP3, WP4 | WP2: view 字段；WP4: Diff 元素随工具调用交付；WP3: core 派生 | MU2/premerge | IV1, EV1 |
| R6 | 改动行数按行级差异统计 | `specs/core-derived-events/spec.md` | WP2, TP1, WP3 | WP2: 两个可选字段；WP3: 行级 diff 实现与省略口径 | MU2/premerge | IV1, EV1 |
| R7 | 展示路径相对化且不泄漏工作区外结构 | `specs/core-derived-events/spec.md`, `specs/workspace-resolution/spec.md` | WP2, TP1, WP3 | WP2: `outsideWorkspace` 字段；WP3: 规范化前缀判定 | MU2/premerge | IV1, EV1 |
| R8 | Agent 连接状态是节点级生命周期 | `specs/core-derived-events/spec.md`, `specs/local-agent-host/spec.md` | WP2, TP1, WP4, WP3 | WP2: `state` 枚举与 `VIEW_ENUMS` 登记；WP4: 进程生命周期上报；WP3: 节点级事件落库 | MU2/premerge | IV1, EV1 |
| R9 | 会话标题只由 Agent 通知更新 | `specs/core-derived-events/spec.md` | WP2, TP1, WP3 | WP2: 文档写明单向性；WP3: 两层可选字段与通知映射 | MU2/premerge | IV1, EV1 |
| R10 | 客户端分层与依赖方向 | `specs/pwa-web-client/spec.md` | WP5a, TP3 | WP5a: 目录分层与依赖约束测试 | MU3e/premerge | IV1, EV1 |
| R11 | 设备身份由不可导出密钥承载 | `specs/pwa-web-client/spec.md` | WP5b, TP3, TP4 | WP5b: WebCrypto + IndexedDB；TP3: 与 `device-proof.json` 固定向量对拍 | MU3e/premerge | IV1, EV1 |
| R12 | 连接状态是互斥状态机 | `specs/pwa-web-client/spec.md` | WP6, TP3 | WP6: 12 态（8 常规态 + 4 阻断态）迁移实现；TP3: 状态迁移覆盖 | MU3e/premerge | IV1, EV1 |
| R13 | 命令以稳定标识与终态收敛 | `specs/pwa-web-client/spec.md` | WP6, TP3 | WP6: requestId 复用与 uncertain 呈现 | MU3e/premerge | IV1, EV1 |
| R14 | 事件按稳定标识去重且不跳过 | `specs/pwa-web-client/spec.md` | WP6, TP3 | WP6: 去重与快照暂存原子替换 | MU3e/premerge | IV1, EV1 |
| R15 | 目录页以目录为主角且可离线渲染 | `specs/pwa-web-client/spec.md` | WP5a, WP7, TP3, TP4 | WP5a: 目录视图模型；WP7: 目录页与目录详情；TP4: 浏览器渲染验证 | MU3e/final | EV1 |
| R16 | 会话创建是受限入口 | `specs/pwa-web-client/spec.md` | WP7, TP3, TP4 | WP7: 创建弹层与无权限态；TP4: 载荷只含两个引用的浏览器验证 | MU3e/final | EV1 |
| R17 | 未上报的上下文不得以零冒充 | `specs/pwa-web-client/spec.md` | WP7, TP3 | WP7: 上下文条；TP3: 三种状态（已知/未上报/零窗口） | MU3e/final | EV1 |
| R18 | 未识别与不可用的结构化内容显式降级 | `specs/pwa-web-client/spec.md` | WP7, TP3, TP4 | WP7: 降级卡片与诊断抽屉；TP3: 三类不支持可区分 | MU3e/final | EV1 |
| R19 | Agent 目录与连接状态分两层呈现 | `specs/pwa-web-client/spec.md` | WP5a, WP6, WP7 | WP5a: 目录视图模型；WP6: 覆盖层状态；WP7: 主机面板 | MU3d/final | EV1 |
| R20 | imported 资源不落盘且离线只显示元数据 | `specs/pwa-web-client/spec.md` | WP5b, WP6, TP3, TP4 | WP5b: 缓存端口；WP6: no-content-cache 分流；TP3: 落盘断言 | MU3e/final | EV1 |
| R21 | Agent 目录暴露与可用性判定（既有，未改动） | `openspec/specs/local-agent-host/spec.md` | WP4 | WP4: 确认复用既有目录派生路径，不重复实现 | MU2/premerge | IV1 |
| R22 | turn 的同步接受与流式事件交付（既有，未改动） | `openspec/specs/local-agent-host/spec.md` | WP4 | WP4: Diff 随该交付路径一并下发 | MU2/premerge | IV1 |

```agentic-coverage
version: 1
target_ref: refs/heads/main
rows:
  - id: R001
    source:
      path: specs/sync-snapshot-scope/spec.md
      heading: "### Requirement: 快照只承载清单类资源"
    tasks: ["2.1", "4.4", "4.5", "4.2"]
    checks: [PV1, AC2]
    evidence: ["reports/PV1.log", "reports/AC2.log"]
  - id: R002
    source:
      path: specs/sync-snapshot-scope/spec.md
      heading: "#### Scenario: 本地会话的明细不进入快照"
      requirement: "### Requirement: 快照只承载清单类资源"
    tasks: ["2.1", "4.4", "4.5", "4.2"]
    checks: [PV1, AC2]
    evidence: ["reports/PV1.log", "reports/AC2.log"]
  - id: R003
    source:
      path: specs/sync-snapshot-scope/spec.md
      heading: "#### Scenario: 未协商目录 feature 时快照缩为单类"
      requirement: "### Requirement: 快照只承载清单类资源"
    tasks: ["2.1", "4.4", "4.5", "4.2"]
    checks: [PV1, AC2]
    evidence: ["reports/PV1.log", "reports/AC2.log"]
  - id: R004
    source:
      path: specs/sync-snapshot-scope/spec.md
      heading: "#### Scenario: 明细缺席不被表述为完整"
      requirement: "### Requirement: 快照只承载清单类资源"
    tasks: ["2.1", "4.4", "4.5", "4.2"]
    checks: [PV1, AC2]
    evidence: ["reports/PV1.log", "reports/AC2.log"]
  - id: R005
    source:
      path: specs/sync-snapshot-scope/spec.md
      heading: "### Requirement: 会话明细经分页读取"
    tasks: ["2.1", "2.7", "4.4", "4.5"]
    checks: [PV1, PV3]
    evidence: ["reports/PV1.log", "reports/PV3.log"]
  - id: R006
    source:
      path: specs/sync-snapshot-scope/spec.md
      heading: "#### Scenario: 省略游标时返回最新一页"
      requirement: "### Requirement: 会话明细经分页读取"
    tasks: ["2.1", "2.7", "4.4", "4.5"]
    checks: [PV1, PV3]
    evidence: ["reports/PV1.log", "reports/PV3.log"]
  - id: R007
    source:
      path: specs/sync-snapshot-scope/spec.md
      heading: "#### Scenario: 带上游标时返回更早一页"
      requirement: "### Requirement: 会话明细经分页读取"
    tasks: ["2.1", "2.7", "4.4", "4.5"]
    checks: [PV1, PV3]
    evidence: ["reports/PV1.log", "reports/PV3.log"]
  - id: R008
    source:
      path: specs/sync-snapshot-scope/spec.md
      heading: "#### Scenario: 到达最早一条时不再指示更多"
      requirement: "### Requirement: 会话明细经分页读取"
    tasks: ["2.1", "2.7", "4.4", "4.5"]
    checks: [PV1, PV3]
    evidence: ["reports/PV1.log", "reports/PV3.log"]
  - id: R009
    source:
      path: specs/sync-snapshot-scope/spec.md
      heading: "#### Scenario: 超出上限的 limit 被收敛"
      requirement: "### Requirement: 会话明细经分页读取"
    tasks: ["2.1", "2.7", "4.4", "4.5"]
    checks: [PV1, PV3]
    evidence: ["reports/PV1.log", "reports/PV3.log"]
  - id: R010
    source:
      path: specs/sync-snapshot-scope/spec.md
      heading: "#### Scenario: 到达保留窗口之前时显式不可读"
      requirement: "### Requirement: 会话明细经分页读取"
    tasks: ["2.1", "2.7", "4.4", "4.5"]
    checks: [PV1, PV3]
    evidence: ["reports/PV1.log", "reports/PV3.log"]
  - id: R011
    source:
      path: specs/sync-snapshot-scope/spec.md
      heading: "### Requirement: 游标由时间与标识复合构成"
    tasks: ["2.1", "2.7", "4.4", "4.5"]
    checks: [PV1, PV3]
    evidence: ["reports/PV1.log", "reports/PV3.log"]
  - id: R012
    source:
      path: specs/sync-snapshot-scope/spec.md
      heading: "#### Scenario: 同一时刻的多条消息不重不漏"
      requirement: "### Requirement: 游标由时间与标识复合构成"
    tasks: ["2.1", "2.7", "4.4", "4.5"]
    checks: [PV1, PV3]
    evidence: ["reports/PV1.log", "reports/PV3.log"]
  - id: R013
    source:
      path: specs/sync-snapshot-scope/spec.md
      heading: "#### Scenario: 游标不可由标识前缀构造"
      requirement: "### Requirement: 游标由时间与标识复合构成"
    tasks: ["2.1", "2.7", "4.4", "4.5"]
    checks: [PV1, PV3]
    evidence: ["reports/PV1.log", "reports/PV3.log"]
  - id: R014
    source:
      path: specs/sync-snapshot-scope/spec.md
      heading: "#### Scenario: 不按标识排序"
      requirement: "### Requirement: 游标由时间与标识复合构成"
    tasks: ["2.1", "2.7", "4.4", "4.5"]
    checks: [PV1, PV3]
    evidence: ["reports/PV1.log", "reports/PV3.log"]
  - id: R015
    source:
      path: specs/sync-snapshot-scope/spec.md
      heading: "### Requirement: 分页只作用于客户端同步面"
    tasks: ["2.1", "4.4", "4.5"]
    checks: [PV1]
    evidence: ["reports/PV1.log"]
  - id: R016
    source:
      path: specs/sync-snapshot-scope/spec.md
      heading: "#### Scenario: 节点协议侧不受影响"
      requirement: "### Requirement: 分页只作用于客户端同步面"
    tasks: ["2.1", "4.4", "4.5"]
    checks: [PV1]
    evidence: ["reports/PV1.log"]
  - id: R017
    source:
      path: specs/sync-snapshot-scope/spec.md
      heading: "#### Scenario: 同步面未提供参数时使用默认页"
      requirement: "### Requirement: 分页只作用于客户端同步面"
    tasks: ["2.1", "4.4", "4.5"]
    checks: [PV1]
    evidence: ["reports/PV1.log"]
  - id: R018
    source:
      path: specs/core-derived-events/spec.md
      heading: "### Requirement: 文件改动事件由类型化 Diff 派生"
    tasks: ["2.2", "2.3", "2.4", "4.4", "4.5", "4.1"]
    checks: [PV1, PV2, AC1]
    evidence: ["reports/PV1.log", "reports/PV2.log", "reports/AC1.log"]
  - id: R019
    source:
      path: specs/core-derived-events/spec.md
      heading: "#### Scenario: 含 Diff 的工具调用产生一个改动事件"
      requirement: "### Requirement: 文件改动事件由类型化 Diff 派生"
    tasks: ["2.2", "2.3", "2.4", "4.4", "4.5", "4.1"]
    checks: [PV1, PV2, AC1]
    evidence: ["reports/PV1.log", "reports/PV2.log", "reports/AC1.log"]
  - id: R020
    source:
      path: specs/core-derived-events/spec.md
      heading: "#### Scenario: 不含文件改动的工具调用不产生事件"
      requirement: "### Requirement: 文件改动事件由类型化 Diff 派生"
    tasks: ["2.2", "2.3", "2.4", "4.4", "4.5", "4.1"]
    checks: [PV1, PV2, AC1]
    evidence: ["reports/PV1.log", "reports/PV2.log", "reports/AC1.log"]
  - id: R021
    source:
      path: specs/core-derived-events/spec.md
      heading: "#### Scenario: 派生不改写 ACP 原文"
      requirement: "### Requirement: 文件改动事件由类型化 Diff 派生"
    tasks: ["2.2", "2.3", "2.4", "4.4", "4.5", "4.1"]
    checks: [PV1, PV2, AC1]
    evidence: ["reports/PV1.log", "reports/PV2.log", "reports/AC1.log"]
  - id: R022
    source:
      path: specs/core-derived-events/spec.md
      heading: "### Requirement: 改动行数按行级差异统计"
    tasks: ["2.2", "2.3", "4.4", "4.8"]
    checks: [PV1, PV2]
    evidence: ["reports/PV1.log", "reports/PV2.log"]
  - id: R023
    source:
      path: specs/core-derived-events/spec.md
      heading: "#### Scenario: 行数相等但内容不同时仍报告改动"
      requirement: "### Requirement: 改动行数按行级差异统计"
    tasks: ["2.2", "2.3", "4.4", "4.8"]
    checks: [PV1, PV2]
    evidence: ["reports/PV1.log", "reports/PV2.log"]
  - id: R024
    source:
      path: specs/core-derived-events/spec.md
      heading: "#### Scenario: 新建文件全部计为新增"
      requirement: "### Requirement: 改动行数按行级差异统计"
    tasks: ["2.2", "2.3", "4.4", "4.8"]
    checks: [PV1, PV2]
    evidence: ["reports/PV1.log", "reports/PV2.log"]
  - id: R025
    source:
      path: specs/core-derived-events/spec.md
      heading: "#### Scenario: 判定不出时省略而非填零"
      requirement: "### Requirement: 改动行数按行级差异统计"
    tasks: ["2.2", "2.3", "4.4", "4.8"]
    checks: [PV1, PV2]
    evidence: ["reports/PV1.log", "reports/PV2.log"]
  - id: R026
    source:
      path: specs/core-derived-events/spec.md
      heading: "### Requirement: 展示路径相对化且不泄漏工作区外结构"
    tasks: ["2.2", "2.3", "4.4", "4.8"]
    checks: [PV1, PV2]
    evidence: ["reports/PV1.log", "reports/PV2.log"]
  - id: R027
    source:
      path: specs/core-derived-events/spec.md
      heading: "#### Scenario: 工作区内的路径相对化下发"
      requirement: "### Requirement: 展示路径相对化且不泄漏工作区外结构"
    tasks: ["2.2", "2.3", "4.4", "4.8"]
    checks: [PV1, PV2]
    evidence: ["reports/PV1.log", "reports/PV2.log"]
  - id: R028
    source:
      path: specs/core-derived-events/spec.md
      heading: "#### Scenario: 工作区外的路径被显式标记"
      requirement: "### Requirement: 展示路径相对化且不泄漏工作区外结构"
    tasks: ["2.2", "2.3", "4.4", "4.8"]
    checks: [PV1, PV2]
    evidence: ["reports/PV1.log", "reports/PV2.log"]
  - id: R029
    source:
      path: specs/core-derived-events/spec.md
      heading: "#### Scenario: 同名前缀不被误判为工作区内"
      requirement: "### Requirement: 展示路径相对化且不泄漏工作区外结构"
    tasks: ["2.2", "2.3", "4.4", "4.8"]
    checks: [PV1, PV2]
    evidence: ["reports/PV1.log", "reports/PV2.log"]
  - id: R030
    source:
      path: specs/core-derived-events/spec.md
      heading: "### Requirement: Agent 连接状态是节点级生命周期"
    tasks: ["2.2", "2.3", "2.4", "4.4", "4.8", "4.1"]
    checks: [PV1, PV2, AC1]
    evidence: ["reports/PV1.log", "reports/PV2.log", "reports/AC1.log"]
  - id: R031
    source:
      path: specs/core-derived-events/spec.md
      heading: "#### Scenario: 进程建立时产生一次连接事件"
      requirement: "### Requirement: Agent 连接状态是节点级生命周期"
    tasks: ["2.2", "2.3", "2.4", "4.4", "4.8", "4.1"]
    checks: [PV1, PV2, AC1]
    evidence: ["reports/PV1.log", "reports/PV2.log", "reports/AC1.log"]
  - id: R032
    source:
      path: specs/core-derived-events/spec.md
      heading: "#### Scenario: 进程退出时产生一次断开事件"
      requirement: "### Requirement: Agent 连接状态是节点级生命周期"
    tasks: ["2.2", "2.3", "2.4", "4.4", "4.8", "4.1"]
    checks: [PV1, PV2, AC1]
    evidence: ["reports/PV1.log", "reports/PV2.log", "reports/AC1.log"]
  - id: R033
    source:
      path: specs/core-derived-events/spec.md
      heading: "#### Scenario: 节点级事件不被会话级投递路径误收"
      requirement: "### Requirement: Agent 连接状态是节点级生命周期"
    tasks: ["2.2", "2.3", "2.4", "4.4", "4.8", "4.1"]
    checks: [PV1, PV2, AC1]
    evidence: ["reports/PV1.log", "reports/PV2.log", "reports/AC1.log"]
  - id: R034
    source:
      path: specs/core-derived-events/spec.md
      heading: "#### Scenario: 状态取值来自封闭词表"
      requirement: "### Requirement: Agent 连接状态是节点级生命周期"
    tasks: ["2.2", "2.3", "2.4", "4.4", "4.8", "4.1"]
    checks: [PV1, PV2, AC1]
    evidence: ["reports/PV1.log", "reports/PV2.log", "reports/AC1.log"]
  - id: R035
    source:
      path: specs/core-derived-events/spec.md
      heading: "#### Scenario: 不表达会话活跃程度"
      requirement: "### Requirement: Agent 连接状态是节点级生命周期"
    tasks: ["2.2", "2.3", "2.4", "4.4", "4.8", "4.1"]
    checks: [PV1, PV2, AC1]
    evidence: ["reports/PV1.log", "reports/PV2.log", "reports/AC1.log"]
  - id: R036
    source:
      path: specs/core-derived-events/spec.md
      heading: "### Requirement: 会话标题只由 Agent 通知更新"
    tasks: ["2.2", "2.3", "4.4", "4.8", "4.1"]
    checks: [PV1, PV2, AC1]
    evidence: ["reports/PV1.log", "reports/PV2.log", "reports/AC1.log"]
  - id: R037
    source:
      path: specs/core-derived-events/spec.md
      heading: "#### Scenario: 首次通知写入标题"
      requirement: "### Requirement: 会话标题只由 Agent 通知更新"
    tasks: ["2.2", "2.3", "4.4", "4.8", "4.1"]
    checks: [PV1, PV2, AC1]
    evidence: ["reports/PV1.log", "reports/PV2.log", "reports/AC1.log"]
  - id: R038
    source:
      path: specs/core-derived-events/spec.md
      heading: "#### Scenario: 只有更新时间时保持标题不变"
      requirement: "### Requirement: 会话标题只由 Agent 通知更新"
    tasks: ["2.2", "2.3", "4.4", "4.8", "4.1"]
    checks: [PV1, PV2, AC1]
    evidence: ["reports/PV1.log", "reports/PV2.log", "reports/AC1.log"]
  - id: R039
    source:
      path: specs/core-derived-events/spec.md
      heading: "#### Scenario: 通知清空标题时呈现为未命名"
      requirement: "### Requirement: 会话标题只由 Agent 通知更新"
    tasks: ["2.2", "2.3", "4.4", "4.8", "4.1"]
    checks: [PV1, PV2, AC1]
    evidence: ["reports/PV1.log", "reports/PV2.log", "reports/AC1.log"]
  - id: R040
    source:
      path: specs/core-derived-events/spec.md
      heading: "#### Scenario: 客户端无重命名入口"
      requirement: "### Requirement: 会话标题只由 Agent 通知更新"
    tasks: ["2.2", "2.3", "4.4", "4.8", "4.1"]
    checks: [PV1, PV2, AC1]
    evidence: ["reports/PV1.log", "reports/PV2.log", "reports/AC1.log"]
  - id: R041
    source:
      path: specs/core-derived-events/spec.md
      heading: "#### Scenario: 标题未生成时保持未命名"
      requirement: "### Requirement: 会话标题只由 Agent 通知更新"
    tasks: ["2.2", "2.3", "4.4", "4.8", "4.1"]
    checks: [PV1, PV2, AC1]
    evidence: ["reports/PV1.log", "reports/PV2.log", "reports/AC1.log"]
  - id: R042
    source:
      path: specs/pwa-web-client/spec.md
      heading: "### Requirement: 客户端分层与依赖方向"
    tasks: ["2.5", "4.10", "4.11"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R043
    source:
      path: specs/pwa-web-client/spec.md
      heading: "#### Scenario: 页面不直接接触平台能力"
      requirement: "### Requirement: 客户端分层与依赖方向"
    tasks: ["2.5", "4.10", "4.11"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R044
    source:
      path: specs/pwa-web-client/spec.md
      heading: "#### Scenario: 视图模型不暴露协议载荷"
      requirement: "### Requirement: 客户端分层与依赖方向"
    tasks: ["2.5", "4.10", "4.11"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R045
    source:
      path: specs/pwa-web-client/spec.md
      heading: "### Requirement: 设备身份由不可导出密钥承载"
    tasks: ["2.6", "4.10", "4.11", "4.3"]
    checks: [PV3, AC3]
    evidence: ["reports/PV3.log", "reports/AC3.log"]
  - id: R046
    source:
      path: specs/pwa-web-client/spec.md
      heading: "#### Scenario: 密钥不以可导出形式落盘"
      requirement: "### Requirement: 设备身份由不可导出密钥承载"
    tasks: ["2.6", "4.10", "4.11", "4.3"]
    checks: [PV3, AC3]
    evidence: ["reports/PV3.log", "reports/AC3.log"]
  - id: R047
    source:
      path: specs/pwa-web-client/spec.md
      heading: "#### Scenario: 每次连接重新握手"
      requirement: "### Requirement: 设备身份由不可导出密钥承载"
    tasks: ["2.6", "4.10", "4.11", "4.3"]
    checks: [PV3, AC3]
    evidence: ["reports/PV3.log", "reports/AC3.log"]
  - id: R048
    source:
      path: specs/pwa-web-client/spec.md
      heading: "#### Scenario: 来源变化后不再自动信任"
      requirement: "### Requirement: 设备身份由不可导出密钥承载"
    tasks: ["2.6", "4.10", "4.11", "4.3"]
    checks: [PV3, AC3]
    evidence: ["reports/PV3.log", "reports/AC3.log"]
  - id: R049
    source:
      path: specs/pwa-web-client/spec.md
      heading: "#### Scenario: 本地数据被清除后按身份丢失处理"
      requirement: "### Requirement: 设备身份由不可导出密钥承载"
    tasks: ["2.6", "4.10", "4.11", "4.3"]
    checks: [PV3, AC3]
    evidence: ["reports/PV3.log", "reports/AC3.log"]
  - id: R050
    source:
      path: specs/pwa-web-client/spec.md
      heading: "### Requirement: 连接状态是互斥状态机"
    tasks: ["2.7", "4.10", "4.11"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R051
    source:
      path: specs/pwa-web-client/spec.md
      heading: "#### Scenario: 阻断态阻断自动重连"
      requirement: "### Requirement: 连接状态是互斥状态机"
    tasks: ["2.7", "4.10", "4.11"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R052
    source:
      path: specs/pwa-web-client/spec.md
      heading: "#### Scenario: 追平期不谎称已在线"
      requirement: "### Requirement: 连接状态是互斥状态机"
    tasks: ["2.7", "4.10", "4.11"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R053
    source:
      path: specs/pwa-web-client/spec.md
      heading: "#### Scenario: 顶替后旧页面停止争抢"
      requirement: "### Requirement: 连接状态是互斥状态机"
    tasks: ["2.7", "4.10", "4.11"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R054
    source:
      path: specs/pwa-web-client/spec.md
      heading: "#### Scenario: 离线时输入不被标记为已发送"
      requirement: "### Requirement: 连接状态是互斥状态机"
    tasks: ["2.7", "4.10", "4.11"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R055
    source:
      path: specs/pwa-web-client/spec.md
      heading: "### Requirement: 命令以稳定标识与终态收敛"
    tasks: ["2.7", "4.10", "4.11"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R056
    source:
      path: specs/pwa-web-client/spec.md
      heading: "#### Scenario: 重连后以同一标识确认结果"
      requirement: "### Requirement: 命令以稳定标识与终态收敛"
    tasks: ["2.7", "4.10", "4.11"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R057
    source:
      path: specs/pwa-web-client/spec.md
      heading: "#### Scenario: 结果不确定不被自行断定"
      requirement: "### Requirement: 命令以稳定标识与终态收敛"
    tasks: ["2.7", "4.10", "4.11"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R058
    source:
      path: specs/pwa-web-client/spec.md
      heading: "#### Scenario: 未确认接受前不显示已接受"
      requirement: "### Requirement: 命令以稳定标识与终态收敛"
    tasks: ["2.7", "4.10", "4.11"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R059
    source:
      path: specs/pwa-web-client/spec.md
      heading: "### Requirement: 事件按稳定标识去重且不跳过"
    tasks: ["2.7", "4.10", "4.11"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R060
    source:
      path: specs/pwa-web-client/spec.md
      heading: "#### Scenario: 重复事件不重复呈现"
      requirement: "### Requirement: 事件按稳定标识去重且不跳过"
    tasks: ["2.7", "4.10", "4.11"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R061
    source:
      path: specs/pwa-web-client/spec.md
      heading: "#### Scenario: 重建快照时不破坏已完成状态"
      requirement: "### Requirement: 事件按稳定标识去重且不跳过"
    tasks: ["2.7", "4.10", "4.11"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R062
    source:
      path: specs/pwa-web-client/spec.md
      heading: "### Requirement: 目录页以目录为主角且可离线渲染"
    tasks: ["2.5", "2.8", "4.13", "4.14"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R063
    source:
      path: specs/pwa-web-client/spec.md
      heading: "#### Scenario: 断连时目录仍可渲染"
      requirement: "### Requirement: 目录页以目录为主角且可离线渲染"
    tasks: ["2.5", "2.8", "4.13", "4.14"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R064
    source:
      path: specs/pwa-web-client/spec.md
      heading: "#### Scenario: 目录项不暴露路径"
      requirement: "### Requirement: 目录页以目录为主角且可离线渲染"
    tasks: ["2.5", "2.8", "4.13", "4.14"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R065
    source:
      path: specs/pwa-web-client/spec.md
      heading: "#### Scenario: 同名目录不使用路径消歧"
      requirement: "### Requirement: 目录页以目录为主角且可离线渲染"
    tasks: ["2.5", "2.8", "4.13", "4.14"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R066
    source:
      path: specs/pwa-web-client/spec.md
      heading: "#### Scenario: 两种空态可区分"
      requirement: "### Requirement: 目录页以目录为主角且可离线渲染"
    tasks: ["2.5", "2.8", "4.13", "4.14"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R067
    source:
      path: specs/pwa-web-client/spec.md
      heading: "### Requirement: 会话创建是受限入口"
    tasks: ["2.8", "4.13", "4.14"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R068
    source:
      path: specs/pwa-web-client/spec.md
      heading: "#### Scenario: 提交载荷只含两个引用"
      requirement: "### Requirement: 会话创建是受限入口"
    tasks: ["2.8", "4.13", "4.14"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R069
    source:
      path: specs/pwa-web-client/spec.md
      heading: "#### Scenario: 无权限时入口不可用且说明原因"
      requirement: "### Requirement: 会话创建是受限入口"
    tasks: ["2.8", "4.13", "4.14"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R070
    source:
      path: specs/pwa-web-client/spec.md
      heading: "#### Scenario: 创建中与终态分别呈现"
      requirement: "### Requirement: 会话创建是受限入口"
    tasks: ["2.8", "4.13", "4.14"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R071
    source:
      path: specs/pwa-web-client/spec.md
      heading: "#### Scenario: 结果不确定显眼且不被自动消解"
      requirement: "### Requirement: 会话创建是受限入口"
    tasks: ["2.8", "4.13", "4.14"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R072
    source:
      path: specs/pwa-web-client/spec.md
      heading: "### Requirement: 未上报的上下文不得以零冒充"
    tasks: ["2.8", "4.10", "4.11"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R073
    source:
      path: specs/pwa-web-client/spec.md
      heading: "#### Scenario: 未收到用量事件时显示未上报"
      requirement: "### Requirement: 未上报的上下文不得以零冒充"
    tasks: ["2.8", "4.10", "4.11"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R074
    source:
      path: specs/pwa-web-client/spec.md
      heading: "#### Scenario: 窗口大小为零时同样视为未上报"
      requirement: "### Requirement: 未上报的上下文不得以零冒充"
    tasks: ["2.8", "4.10", "4.11"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R075
    source:
      path: specs/pwa-web-client/spec.md
      heading: "#### Scenario: 未上报不影响其它交互"
      requirement: "### Requirement: 未上报的上下文不得以零冒充"
    tasks: ["2.8", "4.10", "4.11"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R076
    source:
      path: specs/pwa-web-client/spec.md
      heading: "### Requirement: 未识别与不可用的结构化内容显式降级"
    tasks: ["2.8", "4.10", "4.11", "4.13", "4.14"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R077
    source:
      path: specs/pwa-web-client/spec.md
      heading: "#### Scenario: 未识别事件保留原文可查"
      requirement: "### Requirement: 未识别与不可用的结构化内容显式降级"
    tasks: ["2.8", "4.10", "4.11", "4.13", "4.14"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R078
    source:
      path: specs/pwa-web-client/spec.md
      heading: "#### Scenario: 三类不支持可区分"
      requirement: "### Requirement: 未识别与不可用的结构化内容显式降级"
    tasks: ["2.8", "4.10", "4.11", "4.13", "4.14"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R079
    source:
      path: specs/pwa-web-client/spec.md
      heading: "#### Scenario: 原文未下发时不渲染占位"
      requirement: "### Requirement: 未识别与不可用的结构化内容显式降级"
    tasks: ["2.8", "4.10", "4.11", "4.13", "4.14"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R080
    source:
      path: specs/pwa-web-client/spec.md
      heading: "### Requirement: Agent 目录与连接状态分两层呈现"
    tasks: ["2.5", "2.7", "2.8", "4.10", "4.11"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R081
    source:
      path: specs/pwa-web-client/spec.md
      heading: "#### Scenario: 断连时仍列出已配置 Agent"
      requirement: "### Requirement: Agent 目录与连接状态分两层呈现"
    tasks: ["2.5", "2.7", "2.8", "4.10", "4.11"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R082
    source:
      path: specs/pwa-web-client/spec.md
      heading: "#### Scenario: 状态未知不等于已断开"
      requirement: "### Requirement: Agent 目录与连接状态分两层呈现"
    tasks: ["2.5", "2.7", "2.8", "4.10", "4.11"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R083
    source:
      path: specs/pwa-web-client/spec.md
      heading: "#### Scenario: 标识区分同名 Agent"
      requirement: "### Requirement: Agent 目录与连接状态分两层呈现"
    tasks: ["2.5", "2.7", "2.8", "4.10", "4.11"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R084
    source:
      path: specs/pwa-web-client/spec.md
      heading: "### Requirement: imported 资源不落盘且离线只显示元数据"
    tasks: ["2.6", "2.7", "4.10", "4.11"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R085
    source:
      path: specs/pwa-web-client/spec.md
      heading: "#### Scenario: imported 会话正文不进入持久存储"
      requirement: "### Requirement: imported 资源不落盘且离线只显示元数据"
    tasks: ["2.6", "2.7", "4.10", "4.11"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R086
    source:
      path: specs/pwa-web-client/spec.md
      heading: "#### Scenario: 来源离线时输入不被标记为已发送"
      requirement: "### Requirement: imported 资源不落盘且离线只显示元数据"
    tasks: ["2.6", "2.7", "4.10", "4.11"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R087
    source:
      path: specs/pwa-web-client/spec.md
      heading: "#### Scenario: 本节点摘要缓存按固定上限淘汰"
      requirement: "### Requirement: imported 资源不落盘且离线只显示元数据"
    tasks: ["2.6", "2.7", "4.10", "4.11"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R088
    source:
      path: specs/pwa-web-client/spec.md
      heading: "#### Scenario: 清除数据与撤销是不同的操作"
      requirement: "### Requirement: imported 资源不落盘且离线只显示元数据"
    tasks: ["2.6", "2.7", "4.10", "4.11"]
    checks: [PV3]
    evidence: ["reports/PV3.log"]
  - id: R089
    source:
      path: specs/workspace-resolution/spec.md
      heading: "### Requirement: 路径不泄漏"
    tasks: ["2.2", "2.3", "4.4", "4.8"]
    checks: [PV1, PV2]
    evidence: ["reports/PV1.log", "reports/PV2.log"]
  - id: R090
    source:
      path: specs/workspace-resolution/spec.md
      heading: "#### Scenario: 事件与错误都不含路径"
      requirement: "### Requirement: 路径不泄漏"
    tasks: ["2.2", "2.3", "4.4", "4.8"]
    checks: [PV1, PV2]
    evidence: ["reports/PV1.log", "reports/PV2.log"]
  - id: R091
    source:
      path: specs/workspace-resolution/spec.md
      heading: "#### Scenario: 工作区内的派生路径只下发相对形式"
      requirement: "### Requirement: 路径不泄漏"
    tasks: ["2.2", "2.3", "4.4", "4.8"]
    checks: [PV1, PV2]
    evidence: ["reports/PV1.log", "reports/PV2.log"]
  - id: R092
    source:
      path: specs/workspace-resolution/spec.md
      heading: "#### Scenario: 工作区之外的派生路径被标记且不回退"
      requirement: "### Requirement: 路径不泄漏"
    tasks: ["2.2", "2.3", "4.4", "4.8"]
    checks: [PV1, PV2]
    evidence: ["reports/PV1.log", "reports/PV2.log"]
  - id: R093
    source:
      path: specs/workspace-resolution/spec.md
      heading: "#### Scenario: 同名前缀不构成工作区内"
      requirement: "### Requirement: 路径不泄漏"
    tasks: ["2.2", "2.3", "4.4", "4.8"]
    checks: [PV1, PV2]
    evidence: ["reports/PV1.log", "reports/PV2.log"]
  - id: R094
    source:
      path: specs/local-agent-host/spec.md
      heading: "### Requirement: 文件改动的转发现源限于类型化 Diff"
    tasks: ["2.4", "4.8"]
    checks: [PV2]
    evidence: ["reports/PV2.log"]
  - id: R095
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: 含 Diff 的工具调用连同 Diff 元素一并交付"
      requirement: "### Requirement: 文件改动的转发现源限于类型化 Diff"
    tasks: ["2.4", "4.8"]
    checks: [PV2]
    evidence: ["reports/PV2.log"]
  - id: R096
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: 不含 Diff 的工具调用不构成文件改动"
      requirement: "### Requirement: 文件改动的转发现源限于类型化 Diff"
    tasks: ["2.4", "4.8"]
    checks: [PV2]
    evidence: ["reports/PV2.log"]
  - id: R097
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: 不解析原始输入以推断改动"
      requirement: "### Requirement: 文件改动的转发现源限于类型化 Diff"
    tasks: ["2.4", "4.8"]
    checks: [PV2]
    evidence: ["reports/PV2.log"]
  - id: R098
    source:
      path: specs/local-agent-host/spec.md
      heading: "### Requirement: profile 进程的连接生命周期可被观察"
    tasks: ["2.4", "4.8", "4.1"]
    checks: [PV2, AC1]
    evidence: ["reports/PV2.log", "reports/AC1.log"]
  - id: R099
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: 首次建立时上报一次连接"
      requirement: "### Requirement: profile 进程的连接生命周期可被观察"
    tasks: ["2.4", "4.8", "4.1"]
    checks: [PV2, AC1]
    evidence: ["reports/PV2.log", "reports/AC1.log"]
  - id: R100
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: 复用既有进程不重复上报"
      requirement: "### Requirement: profile 进程的连接生命周期可被观察"
    tasks: ["2.4", "4.8", "4.1"]
    checks: [PV2, AC1]
    evidence: ["reports/PV2.log", "reports/AC1.log"]
  - id: R101
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: 退出时上报一次断开"
      requirement: "### Requirement: profile 进程的连接生命周期可被观察"
    tasks: ["2.4", "4.8", "4.1"]
    checks: [PV2, AC1]
    evidence: ["reports/PV2.log", "reports/AC1.log"]
  - id: R102
    source:
      path: specs/local-agent-host/spec.md
      heading: "#### Scenario: 超限退出时断开不早于退出判定"
      requirement: "### Requirement: profile 进程的连接生命周期可被观察"
    tasks: ["2.4", "4.8", "4.1"]
    checks: [PV2, AC1]
    evidence: ["reports/PV2.log", "reports/AC1.log"]
```

**未列入的实现性要求**（不改变行为，归入对应 WP 的 Verification 列）：`pwa-web-client` 的分层约束由 WP5a 的静态检查承担；`core-derived-events` 的「不改写 ACP 原文」由 WP3 的既有保真测试扩展承担。

## Work Packages

- Test Authoring Protocol: staged-v2

| ID | Goal / Scenarios | Dependencies | Contract Freeze | Owner | Role | Reviewer | Branch / Worktree | Write Scope | Inputs / Outputs | Verification |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| WP1 | R1–R4：快照范围收窄 + `session.read` 分页的机器合同与文档（schema、fixture、Rust 类型化镜像与文档） | none | NOT_APPLICABLE | coding-1 | coder | review-1 | wt/wp1 | `schemas/sync/v1/`, `fixtures/sync/v1/`, `crates/sync-protocol/src/command.rs`, `crates/sync-protocol/src/sync.rs`, `docs/SYNC_PROTOCOL.md`, `docs/NODE_LINK_PROTOCOL.md` | D1/D2/D3 → schema + fixture + 文档 | PV1 |
| WP2 | R5–R9 的字段侧：`file.changed` 可选字段、`agent.*` 的 `state` 枚举与 `VIEW_ENUMS` 登记、标题单向性的文档 | none | NOT_APPLICABLE | coding-2 | coder | review-2 | wt/wp2 | `schemas/sync/v1/event-views.schema.json`, `crates/sync-protocol/src/views.rs`, `fixtures/sync/v1/valid/view-*.json`, `fixtures/sync/v1/manifest.json`, `docs/SYNC_PROTOCOL.md`, `docs/ACP_COMPATIBILITY_MATRIX.md` | D4/D5/D6/D7 → schema + Rust DTO + 登记 | PV1 |
| WP3 | R5–R9 的实现侧：core 派生 `file.changed`、行级 diff、路径相对化、节点级 Agent 事件、标题更新 | contract:WP1, contract:WP2 | schemas/sync/v1/event-views.schema.json@WP2 | coding-3 | coder | review-3 | wt/wp3 | `crates/core/src/`, `crates/storage-sqlite/src/` | D4/D5/D6/D7 → core 生产者 | PV1, PV2 |
| WP4 | R8/R21/R22：`agent-host` 的 Diff 转发与 profile 进程生命周期上报 | contract:WP2 | schemas/sync/v1/event-views.schema.json@WP2 | coding-4 | coder | review-4 | wt/wp4 | `crates/agent-host/src/`, `crates/core/src/ports.rs#SessionEndpoint` | D6 → 进程生命周期与 Diff 透传 | PV1, PV2 |
| WP5a | R10/R15/R19：前端工程骨架、七层目录、`src/protocol/` 类型化 DTO、`src/domain/` 视图模型 | contract:WP1, contract:WP2 | schemas/sync/v1/event-views.schema.json@WP1 | coding-5 | coder | review-5 | wt/wp5a | `clients/app/package.json`, `clients/app/app/`（空壳路由）, `clients/app/src/{domain,protocol,components}/` | D8 → 工程骨架 + 协议层 + 视图模型 | PV1, PV3 |
| WP5b | R11/R20 的平台侧：`src/platform/secure-storage.web.ts` 的 WebCrypto 不可导出 P-256 密钥与 IndexedDB 持久化、`local-cache.web.ts`、`lifecycle.web.ts` | code:WP5a, contract:WP1 | fixtures/sync/v1/transcripts/device-proof.json@WP1 | coding-5b | coder | review-5b | wt/wp5b | `clients/app/src/platform/` | D8 → 设备身份与平台端口；与 WP5a 分包是为了把跨语言字节对拍这块最高风险单独隔离。`contract:WP1` 的依据：本包的全部存在理由是按 `fixtures/sync/v1/transcripts/device-proof.json@WP1` 的字节级规范实现 transcript 编码，该向量由 WP1 产出并冻结（Round 8 的 F27） | PV1, PV3 |
| WP6 | R2/R3/R12–R14/R19/R20：Sync Client、连接与命令状态机、去重与快照暂存 | code:WP5a, code:WP5b, contract:WP1 | schemas/sync/v1/command.schema.json@WP1 | coding-6 | coder | review-6 | wt/wp6 | `clients/app/src/sync-client/`, `clients/app/src/state/` | D2/D8 → 同步客户端与状态机 | PV1, PV3 |
| WP7 | R15–R19：目录页、目录详情、对话页、配对页、降级卡片、诊断抽屉、主机面板 | code:WP6, code:WP5a, code:WP5b, contract:WP1, contract:WP2 | schemas/sync/v1/event-views.schema.json@WP2 | coding-7 | coder | review-7 | wt/wp7 | `clients/app/app/`, `clients/app/src/features/`, `clients/app/src/components/` | D8 → 页面与组件 | PV1, PV3 |
| TP1 | 合同与固定向量：快照清单向量、`session.read` 分页向量、`file.changed` 新字段向量、`state` 枚举向量、node-link 侧未受影响断言 | code:WP1, code:WP2 | schemas/sync/v1/event-views.schema.json@WP1 | testing-1 | tester | review-1 | wt/tp1 | `crates/*/tests/`, `fixtures/sync/v1/`（只增不改） | R1–R9 的机器可判定部分 | PV1 |
| TP2 | core 生产者行为：Diff 派生、行级 diff 口径、越界标记、节点级事件、标题单向更新与不重命名 | code:WP3, contract:WP2 | crates/core/src/ports.rs@WP3 | testing-2 | tester | review-3 | wt/tp2 | `crates/core/src/**/tests.rs`, `crates/storage-sqlite/tests/` | R5–R9 的行为判定 | PV2 |
| TP3 | 前端契约与状态机：固定向量对拍（含 `device-proof`）、连接与命令状态迁移、幂等重试、去重、no-content-cache 落盘断言、上下文三态 | code:WP5a, code:WP5b, code:WP6, code:WP7, contract:WP1 | schemas/sync/v1/event-views.schema.json@WP1 | testing-3 | tester | review-5 | wt/tp3 | `clients/app/**/*.test.ts` | R11–R14、R17–R20 的可判定部分 | PV3 |
| TP4 | 前端浏览器验证：目录页离线渲染、创建载荷只含两个引用、降级卡片与诊断抽屉 | code:WP7 | NOT_APPLICABLE | testing-4 | tester | review-7 | wt/tp4 | `clients/app/e2e/` | R15、R16、R18 的可观察行为 | PV3 |

- Dependency Declaration Review: DDR-sync-scope-and-pwa-client-r1（独立 reviewer，phase plan / stage plan，核实 code/contract/resource 声明属实并绑定当前 planningDigest；结果与可读报告登记 verification 的同名节）

**code 依赖的依据（只列真实的 Rust/TypeScript 代码依赖）**：
- WP6 的 `code:WP5a`/`code:WP5b` — Sync Client 消费 WP5a 建立的 `clients/app/src/protocol/` 与 WP5b 建立的 `src/platform/` 模块，缺上游无法编译或运行。
- TP2 的 `code:WP3` — 行为测试针对 WP3 改后的 `crates/core/src/ports.rs` 与 broker 提交面，按公开端口编写，缺上游无可测对象。
- TP4 的 `code:WP7` — 浏览器用例针对 WP7 产出的页面路由与稳定选择器。

**已从 code 依赖移除并改为 contract 的项（Round 1 审查 F1/F2 的依据）**：
- WP3 原声明 `code:WP1`/`code:WP2`——`crates/core/Cargo.toml` 的依赖闭包只有 `async-trait`/`thiserror`/`p256`/`sha2`，`scripts/check-crate-boundaries.mjs` 把 `sync-protocol` 列入 `CORE_FORBIDDEN`，`MODULE_ARCHITECTURE.md` §5 矩阵中 `core → sync-protocol` 为空。core 通过 `crates/core/src/model/json.rs` 的 `ViewJson`（原始 JSON 文本）上报事件，**不 import `sync-protocol` 的任何类型**，因此不存在「缺上游无法编译」的事实。WP3 对 WP1/WP2 只有 schema 形状依赖，属 `contract:`。
- WP4 原声明 `code:WP2`——`crates/agent-host/Cargo.toml` 不依赖 `sync-protocol`；它经 `crates/agent-host/src/mapper.rs`/`session.rs` 的 `EndpointEvent` + `EventSink` 向 core 上报，载荷同样是 `ViewJson`。属 `contract:`。
- WP5a/WP6 原声明 `code:WP1`/`code:WP2`——WP1/WP2 产出的是 schema、fixture、文档与 `crates/sync-protocol` 的类型化镜像，前端把它们当作**数据**读取而非作为已编译产物被链接。属 `contract:`。

**contract 依赖的依据**：`schemas/sync/v1/**`、`fixtures/sync/v1/**` 与 `crates/sync-protocol/src/{views,command}.rs` 的冻结产物在 WP3–WP7 与 TP1/TP3 中被直接消费，路径可核对（见 Contract Freeze 列）。WP1/WP2 是本变更产出的契约，按 plan instruction 安排为更早冻结前置。

**resource 依赖**：无。`npm run check` 与 `cargo test` 共享仓库根 `target/`，各 WP 使用独立 `CARGO_TARGET_DIR` 隔离，不构成串行理由。

## Execution Waves

| Wave | Work Package | Enter Condition | Serialization Reason |
| --- | --- | --- | --- |
| W1 | WP1 | 无 code 与 contract 依赖，契约起点 | NOT_APPLICABLE |
| W1 | WP2 | 无 code 与 contract 依赖，契约起点 | NOT_APPLICABLE |
| W2 | WP5a | `schemas/sync/v1/**@WP1`、`fixtures/sync/v1/manifest.json@WP1` 已冻结可读 | capacity: openspec/agentic.yaml |
| W3 | WP5b | `code:WP5a` 的交付提交已合入主分支（MU3a） | code-dependency: openspec/changes/sync-scope-and-pwa-client/plan.md |
| W3 | TP2 | `code:WP3` 的交付提交已合入主分支（MU2） | code-dependency: openspec/changes/sync-scope-and-pwa-client/plan.md |
| W4 | WP6 | `code:WP5a`/`code:WP5b` 已合入（MU3a/MU3b），`schemas/sync/v1/command.schema.json@WP1` 已冻结 | code-dependency: openspec/changes/sync-scope-and-pwa-client/plan.md |
| W5 | WP7 | `code:WP5a`/`code:WP5b`/`code:WP6` 已合入（MU3a/MU3b/MU3c），`schemas/sync/v1/event-views.schema.json@WP2` 已冻结 | code-dependency: openspec/changes/sync-scope-and-pwa-client/plan.md |
| W6 | TP3 | `code:WP5a`/`code:WP5b`/`code:WP6`/`code:WP7` 已合入（MU3a–MU3d），`contract:WP1` 已冻结（`fixtures/sync/v1/transcripts/**@WP1`、`schemas/sync/v1/**@WP1`） | code-dependency: openspec/changes/sync-scope-and-pwa-client/plan.md |
| W6 | TP4 | `code:WP7` 已合入（MU3d） | code-dependency: openspec/changes/sync-scope-and-pwa-client/plan.md |

| W2 | WP3 | `schemas/sync/v1/command.schema.json@WP1`、`schemas/sync/v1/event-views.schema.json@WP2` 已冻结可读 | capacity: openspec/agentic.yaml |
| W2 | WP4 | `schemas/sync/v1/event-views.schema.json@WP2` 已冻结可读 | capacity: openspec/agentic.yaml |
| W2 | TP1 | `code:WP1`/`code:WP2` 已交付（Wave 1）。TP1 的 `schema_drift.rs` 与负例向量必须编译并跑在 WP1/WP2 的交付提交上，因此实际开工落在 MU1a 合入之后；该等待是 Merge Strategy 的 Order 约束，不是 code/contract/resource 依赖，故静态层级仍为 W2。冻结的是上列工作包 ID，MU1a 的合入提交落在 Dependency Handoffs 的 Accepted Revision 列 | NOT_APPLICABLE |

W2 及以后各行的 Serialization Reason 依据（附证据路径供机械核对）：

- **WP3 / WP4 / WP5a —— `capacity`**：这三包的依赖全是 `contract:` 边（指向 W1 的 WP1/WP2），静态层级不计入 Merge Strategy 的 Order，故它们的最早可开工层级与 WP1/WP2 同为第 1 批；而第 1 批同角色（coder）工作包数为 4，超过 `openspec/agentic.yaml` 的 `dispatch.pool.coding = 3`，因此窗口不足必须串行。这三包的**实际**开工顺序由 MU1a（Order 1）先合入决定，与静态波次是两个概念。
- **WP5b / WP6 / WP7 / TP2 / TP3 / TP4 —— `code-dependency`**：这些包存在 `code:` 边，而其上游（WP5a、WP5b、WP6、WP7、WP3）的**声明波次**晚于其自身最早可开工层级（同样因为上游的 contract 边只随 MU1a 冻结），构成 `delayed(codeDeps)`，故必须排到上游之后。证据为上表本身（各行的 Wave 列）。


W1 同层有两个 coder（WP1、WP2），构成并发层级；主 Agent 按 `openspec/agentic.yaml` 的 `dispatch.pool.coding` 实际容量核对，若容量 <2 则该层只能串行，须在 verification 的调度证据中如实记录（`capacity: openspec/agentic.yaml`）。写入范围重叠（`fixtures/sync/v1/`、`docs/SYNC_PROTOCOL.md`）按 Shared File Ownership 登记为多写者，不构成串行理由。契约编写与测试设计的并行由 WP1/WP2 与 TP1 的分层承载：TP1 的机器可判定部分可依 `schemas/sync/v1/**@WP1` 起草，只有引用具体固定向量版本的断言需等待 WP1/WP2 冻结。

## Shared File Ownership

| File | Writers (WP) | Merge Owner | Merge Order | Re-verify After Merge | Region Note |
| --- | --- | --- | --- | --- | --- |
| `fixtures/sync/v1/manifest.json` | WP1, WP2, TP1 | merger | WP1 → WP2 → TP1 | TP1: PV1 | TP1 只追加用例并保持排序；条目不重不漏 |
| `fixtures/sync/v1/valid/` | WP1, WP2 | merger | WP1 → WP2 | WP2: PV1 | 按文件前缀划分：WP1 写 `sync-*`/`command-*`，WP2 写 `view-*` |
| `docs/SYNC_PROTOCOL.md` | WP1, WP2 | merger | WP1 → WP2 | WP2: PV1 | WP1 改 §9.4/§11.5，WP2 改 §10.3；不同小节 |
| `schemas/sync/v1/event-views.schema.json` | WP2 | coding-2 | — | WP2: PV1 | WP1 的 Write Scope 是 `schemas/sync/v1/` 目录级，字面包含本文件；按区域收窄：WP1 只改 `command.schema.json`/`sync.schema.json`/`common.schema.json`，本文件由 WP2 独占 |
| `crates/core/src/ports.rs` | WP3, WP4 | merger | WP3 → WP4 | WP4: PV1 | WP3 改 `SessionUpdate` 等状态面，WP4 只扩展 `SessionEndpoint` 端口；`check:drift` 需两侧同时绿 |
| `crates/core/src/broker.rs` | WP3 | coding-3 | — | WP3: PV1 | 单写者 |
| `clients/app/package.json` | WP5a, WP6, WP7 | merger | WP5a → WP6 → WP7 | WP7: PV3 | 后续 WP 只在既有 dependencies 上追加（WP5b 的 `src/platform/` 不写本文件，故不入列） |
| `clients/app/src/protocol/` | WP5a | coding-5 | — | WP5a: PV3 | 单写者，后续 WP 只读 |
| `crates/sync-protocol/src/command.rs` | WP1 | coding-1 | — | WP1: PV1 | 单写者；WP2 的 Write Scope 只含 `views.rs`，与本文件不重叠 |
| `crates/sync-protocol/src/sync.rs` | WP1 | coding-1 | — | WP1: PV1 | 单写者（apply 中补登记：实现实例发现该文件是 `sync.schema.json` 的类型化镜像却无人认领，导致 schema 收窄后 Rust 侧比合同更宽松）；WP2 的 Write Scope 只含 `views.rs`，与本文件不重叠 |
| `crates/sync-protocol/src/views.rs` 与 `crates/*/tests/` | WP2, TP1 | merger | WP2 → TP1 | TP1: PV1 | WP2 改 `src/views.rs`；TP1 只在 `tests/` 目录新增用例文件，不改 `src/`；分处 `src/` 与 `tests/` 两个目录 |
| `crates/core/src/**/tests.rs` | WP3, TP2 | merger | WP3 → TP2 | TP2: PV2 | WP3 的 Write Scope 是 `crates/core/src/` 目录级；按区域收窄：WP3 改实现文件，TP2 只改同目录的 `tests.rs` 测试模块 |
| `clients/app/**/*.test.ts` | TP3, WP7 | merger | WP7 → TP3 | TP3: PV3 | WP7 的 Write Scope 是 `app/`、`src/features/`、`src/components/` 目录级；按区域收窄：WP7 只写实现文件，测试文件由 TP3 独占 |
| `crates/sync-protocol/tests/envelope_fixtures.rs` | WP1, TP1 | merger | WP1 → TP1 | TP1: PV1 | Round 5 的 F15 + TP1 实施中暴露：两个用例计数常量（`EXPECTED_VALID_MESSAGE_CASES`、`EXPECTED_BODY_REJECTED`）的最终值是 WP1 与 TP1 新增向量数之和，任何一方单独都定不下来。MU1a 由 WP1 定其中间值，MU1b 由 TP1 在 MU1a 合入提交上定最终值 |
| `crates/storage-sqlite/tests/` | TP1, TP2 | merger | TP1 → TP2 | TP2: PV2 | TP1 的 Write Scope 是 glob `crates/*/tests/`，字面覆盖本目录；按区域收窄：TP1 只新增契约向量用例文件，TP2 只新增行为测试文件 |
| `clients/app/`（WP5a/WP5b 的目录级范围）对 `clients/app/**/*.test.ts`（TP3） | WP5a, WP5b, TP3 | merger | WP5a → WP5b → TP3 | TP3: PV3 | Round 3 的 F12 + Round 8 的 F31：拆分后前端包的 Write Scope 覆盖 `clients/app/` 目录级，字面覆盖 TP3 的测试文件。按区域收窄：WP5a 只写 `src/{domain,protocol,components}/`、`app/`、`package.json`，WP5b 只写 `src/platform/`；TP3 独占 `*.test.ts` |
| `clients/app/`（WP5a/WP5b 的目录级范围）对 `clients/app/e2e/`（TP4） | WP5a, WP5b, TP4 | merger | WP5a → WP5b → TP4 | TP4: PV3 | Round 3 的 F12 + Round 8 的 F31：同上，前端包的目录级范围字面覆盖 TP4 的 e2e 目录。按区域收窄：WP5a/WP5b 都不写 `e2e/`；TP4 独占该目录 |
| `clients/app/app/` 与 `clients/app/src/components/` | WP5a, WP7 | merger | WP5a → WP7 | WP7: PV3 | Round 9 的 F40：WP5a 的 Write Scope 含 `clients/app/app/` 与 `src/components/`，WP7 也含同两处，字面重叠但登记表无对应行。两包分处 W2/W5 与 MU3a/MU3d，WP7 声明 `code:WP5a`，不构成同批冲突。按区域收窄：WP5a 只建空壳路由与共享展示组件的骨架，WP7 接管这两处的页面实现与后续全部改动；后合入方 WP7 重跑 PV3 |

## Dependency Handoffs

| Downstream | Upstream | Accepted Revision / Evidence | Start Revision | Transfer / Inclusion Check | Invalidation |
| --- | --- | --- | --- | --- | --- |
| WP3 | WP1, WP2 | WP1/WP2 各自的 premerge PASS 与独立 review PASS 所绑定的提交 | 该提交（跨交付单元时为主分支合入后的提交） | 固定提交包含上游交付提交；`npm run check` 与 `check:drift` 在集成点复跑 | 上游契约变更 → WP3 的字段假设失效，需重跑 PV1/PV2 |
| WP4 | WP2 | WP2 premerge PASS 绑定的提交 | 同上 | 同上 | 同上 |
| WP5a | WP1, WP2 | 同上 | 同上 | `npm run check` 须绿；前端类型检查基于冻结的 schema | schema 新增必填字段 → WP5a 的 DTO 失效 |
| WP5b | WP5a, WP1 | WP5a premerge PASS 绑定的合入提交（`code:WP5a`）；WP1 冻结的 `fixtures/sync/v1/transcripts/device-proof.json@WP1`（`contract:WP1`） | 同上 | `src/platform/` 导入 `src/protocol/` 可解析；固定向量路径可读且字节级规范未被改写 | WP5a 的 transcript 编码变更 → WP5b 的签名结果失效，需重跑 PV3 与 AC3；WP1 的 `device-proof.json` 变更 → WP5b 的编码依据失效，需重跑 PV3 与 AC3（Round 10 的 F42：本行 Upstream 原只写 WP5a，而 WP5b 的 Dependencies 是 `code:WP5a, contract:WP1`，是 10 行 handoff 中唯一 Upstream 不等于依赖目标的一行，跨交付单元的契约前置因此没有失效登记） |
| WP6 | WP5a, WP5b, WP1 | WP5a/WP5b 已验收集成基线提交 | 该基线提交 | 基线包含 WP5a/WP5b 交付提交；WP6 的 `protocol/` 与 `platform/` 导入可解析 | WP5b 的 `platform/` 端口签名变更 → WP6 重编并重跑 PV3；WP1 的 `command.schema.json` 变更 → WP6 的请求模型失效，重跑 PV3 |
| WP7 | WP5a, WP5b, WP6, WP1, WP2 | WP5a/WP5b/WP6 已验收集成基线提交，WP1/WP2 冻结契约 | 该基线提交 | 页面消费的视图模型与状态机入口存在且签名稳定 | 状态机状态集变更 → WP7 的门控分支需同步；WP1 的 schema 变更 → WP7 的类型化载荷失效，重跑 PV3 |
| TP2 | WP2, WP3 | WP3 的接口边界（ports 与事件形状）可读；WP2 的 `views.rs` 冻结镜像作为断言的形状基准 | WP3 开工时的基线 | 测试按公开端口编写，不依赖实现私有结构；断言的字段名与取值域逐项对照 WP2 的 `views.rs` | core 端口签名变更 → TP2 断言位置失效；WP2 的 `views.rs` 字段或枚举取值变更 → 形状基准失效；两者都需重跑 |
| TP3 | WP1, WP5a, WP5b, WP6, WP7 | WP5a/WP5b/WP6/WP7 已验收集成基线提交；MU1a 合入后的冻结契约 | 同上 | 固定向量路径可读且未被改写；`src/protocol/`、`src/platform/`、`src/sync-client/`、`src/state/`、`src/features/`、`src/components/` 的导入可解析 | `fixtures/sync/v1/transcripts/**` 变更 → 对拍基线失效；WP5a/WP5b/WP6/WP7 的模块签名变更 → TP3 编译失败，需重跑 |
| TP1 | WP1, WP2 | MU1a 的合入提交（其 premerge PASS 与独立候选检视绑定的候选提交）；WP1/WP2 各自的冻结产物路径见 TP1 的 Contract Freeze 列 | MU1a 合入后的 `refs/heads/main` | TP1 从该合入提交分支；`crates/sync-protocol/src/{sync,command,views}.rs` 的新枚举与分页类型在该提交中可编译 | MU1a 的契约变更 → TP1 的向量与 `schema_drift.rs` 断言失效，需重跑 PV1 |
| TP4 | WP7 | WP7 的页面路由与交互契约可读 | WP7 开工时的基线 | 按稳定选择器编写，不依赖组件层级 | 页面路由或选择器变更 → TP4 用例选择器失效 |

## Runtime Resources

| Checks / Work Packages | Resource | Isolation / Configuration | Exclusive Scheduling | Allocate / Isolate / Start / Ready / Cleanup | Use / Cleanup Boundary |
| --- | --- | --- | --- | --- | --- |
| WP1, WP2, TP1 | 仓库根 `node_modules/`（合同校验脚本） | 共享只读，执行者不得写入 | NOT_APPLICABLE | 仓库自带 | `npm run check` 只读消费 |
| WP3, WP4, WP2, TP1, TP2 | `cargo` 构建目录 | 每包独立 `CARGO_TARGET_DIR=.target-wt/<wp>` | NOT_APPLICABLE | provisioner 按 WP 分配 | 各包只写自己的 `CARGO_TARGET_DIR`；仓库根 `target/` 保留给合并验证 |
| WP5a, WP5b, WP6, WP7, TP3, TP4 | `clients/app/node_modules/` | 每包独立依赖安装目录（随 worktree 隔离） | NOT_APPLICABLE | provisioner 随 worktree 创建 | 各包只操作自身 worktree 内的依赖目录（Round 8 的 F32：区间写法 `WP5a–WP7` 无法判定是否含 WP5b，故逐个列出） |
| TP4 | 前端静态服务器（Vite dev server） | 独立端口；进程生命周期随分片，端口需错开 | NOT_APPLICABLE | provisioner 为每个 TP4 分片分配端口、启动并做就绪检查 | 分片结束即终止其服务器进程；服务器只服务该分片的 worktree。Round 8 的 F33：本行原写「Vite/Metro dev server」，Metro 是 React Native 打包器，与 `tasks.md` 2.6「v1 只交付 Web、原生端不在本次范围」的裁定矛盾，故只保留 Vite；该行的限定语义（仅验证环境、不连真实 Daemon、只服务 fixtures）一并写入本列 |
| TP4 | 浏览器运行环境（Chromium/Chrome） | 每分片独立 profile 目录 | 同层多 TP4 分片时排队（`resource-exclusive: 浏览器实例不可并行隔离`） | provisioner 按分片分配 profile 目录 | 分片退出即清理其 profile |

## Target Repository and Main Branch

- Code Repository: `D:\Project\acp-remote`
- Target Ref: `refs/heads/main`（核实于规划时：`git rev-parse refs/heads/main` → `353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f`；`git rev-parse --abbrev-ref HEAD` → `main`）
- Version Confirmation Owner: 规划时的只读目标调查由 scout 执行；候选构建及合入瞬间的基线复核由 merger 执行；目标选择由 main 确认。
- Confirmation Method / Evidence: `git rev-parse refs/heads/main` 与 `git rev-parse --abbrev-ref HEAD`，执行目录 `D:\Project\acp-remote`；结果记录在 verification 的 `## Target Verification`。

## Merge Strategy

模式：**independent** 逐交付单元合入。MU1 拆为 MU1a（WP1+WP2，契约本体）与 MU1b（TP1，向量）：TP1 的测试必须编译并跑在 WP1/WP2 的**交付提交**上，把两者绑进同一单元会使 TP1 的验证对象退化为漂移的工作区而非固定提交。拆开后 TP1 从 MU1a 的合入提交分支，验证对象固定，且后续 MU2/MU3 不再重复同一困境。

| Delivery Unit / Mode | WP / TP | Owners: Implementation / Test / Review / Merge | Start / Readiness | Candidate Checks / Critical E2E | Order |
| --- | --- | --- | --- | --- | --- |
| MU1a / independent | WP1, WP2 | coding-1, coding-2 / NOT_APPLICABLE（本单元无 TP）/ review-1, review-2 / merger | 契约起点，无上游 | PV1 | 1 |
| MU1b / independent | TP1 | NOT_APPLICABLE / testing-1 / review-1 / merger | MU1a 已合入主分支，TP1 从 MU1a 的合入提交分支；`code:WP1`/`code:WP2` 的交付提交已可读 | PV1 | 2 |
| MU2 / independent | WP3, WP4, TP2 | coding-3, coding-4 / testing-2 / review-3 / merger | MU1b 已合入主分支且契约冻结可读 | PV1, PV2 | 3 |
| MU3a / independent | WP5a | coding-5 / NOT_APPLICABLE / review-5 / merger | MU1b 已合入（契约冻结） | PV1, PV3 | 4 |
| MU3b / independent | WP5b | coding-5b / NOT_APPLICABLE / review-5b / merger | MU3a 已合入，WP5a 的交付提交可读 | PV1, PV3 | 5 |
| MU3c / independent | WP6 | coding-6 / NOT_APPLICABLE / review-6 / merger | MU3a/MU3b 已合入，`command.schema.json@WP1` 已冻结 | PV1, PV3 | 6 |
| MU3d / independent | WP7 | coding-7 / NOT_APPLICABLE / review-7 / merger | MU3a/MU3b/MU3c 已合入 | PV1, PV3 | 7 |
| MU3e / independent | TP3, TP4 | NOT_APPLICABLE / testing-3, testing-4 / review-5, review-7 / merger | MU3a–MU3d 已合入；TP3/TP4 的断言在固定提交上编译并跑通 | PV1, PV3 | 8 |

- Merge Worktree: 各 WP 使用 provisioner 分配的独立 worktree（`.worktrees/<wp>`）；每单元的候选在独立的合入 worktree（`.worktrees/mu1a-merge`、`.worktrees/mu1b-merge`、…）构建，不复用执行 worktree、不新建于单元之间。MU3a–MU3e 各自独立构建候选 worktree，逐单元串行合入。
- Source Revision / Handoff: 单元起点为该单元开工时 `refs/heads/main` 的提交；上游交接按 Dependency Handoffs 表的「Accepted Revision / Evidence」填写 verification 中已验收提交，并核对包含关系。
- Local Merge Conditions / Report Path: 候选建立在当前最新主分支上；`npm run verify` 全绿（`check` 十道脚本 + `check:rust` 三条）；`deps`/`advisories`/`secrets` 三个 CI-only job 在本地无等价物，**不得声称已通过**，其判定留给 PR 的 CI；满足后直接本地合入，无须重复询问。集成报告写入 `openspec/changes/sync-scope-and-pwa-client/reports/merge-mu<N>.log`。

## Verification Strategy

### Local Checks

开发中：`cargo fmt --all` 与 `cargo clippy --locked -p <crate> --all-targets -- -D warnings`（按包，隔离 `CARGO_TARGET_DIR`）；前端按 `clients/app` 自身的类型检查与测试脚本（不接入根 `npm run check`）。

### Project Verify

| Check ID | Stages / Work Packages | Command / Working Directory | Configuration / Environment | Scope / Pass Criteria | Evidence |
| --- | --- | --- | --- | --- | --- |
| PV1 | 单元集成、候选、主分支（全部 WP/TP） | `npm run verify`（`D:\Project\acp-remote`） | Node ≥ 22.12；rust-toolchain.toml 钉定的工具链；根 `node_modules/` | `check` 十道脚本全绿（含 `check:schemas`/`check:drift`/`check:boundaries`）且 `check:rust` 的 fmt/clippy/test 全绿；任一子检查失败即非零退出 | `reports/PV1.log` |
| PV2 | MU2 候选、主分支（WP3, WP4, TP2） | `cargo test --locked -p core -p storage-sqlite -p agent-host --all-features`（`D:\Project\acp-remote`，隔离 `CARGO_TARGET_DIR`） | fake ACP Agent；无外部服务 | 派生事件、行级 diff 口径、越界标记、节点级事件、标题单向更新的行为测试全绿 | `reports/PV2.log` |
| PV3 | MU3a–MU3e 候选、主分支（WP5a, WP5b, WP6, WP7, TP3, TP4） | `clients/app` 的类型检查、单元/契约测试与浏览器用例入口（`D:\Project\acp-remote\clients\app`） | 独立 `clients/app/node_modules/`；Chromium；契约测试读仓库根 `fixtures/sync/v1/` | 固定向量对拍（含 `device-proof`）、状态机迁移、幂等与去重、no-content-cache 落盘断言、目录页与创建入口的浏览器验证全绿 | `reports/PV3.log` |
| IV1 | 最终主分支固定版本 | `npm run verify` + `cargo test` 全包 + `clients/app` 全量（`D:\Project\acp-remote`） | 同上 | `## Independent Validation` 表的通过条件 | `reports/IV1.log` |
| AC1 | 主分支（MU2 收口，替代 Main E2E 的真实 Daemon 路径） | `cargo test --locked -p app --test node_link_e2e`（`D:\Project\acp-remote`，隔离 `CARGO_TARGET_DIR`） | 真实 Daemon 进程、真实 SQLite、fake ACP Agent | `file.changed`（会话级）经 owned 路径落库并按 origin 顺序回放、`agent.connected`（节点级）落库后可按事件库读回且会话标识为空、且会话级投递路径不误收它、会话标题在 `session_info_update` 后更新，四项断言全绿。**不含广播断言**：节点级事件当前无投递通道，见 Risks 的 F5 条 | `reports/AC1.log` |
| AC2 | 主分支（替代 Main E2E 的合同与存储门禁） | `npm run verify`（`D:\Project\acp-remote`） | 同 PV1 | `check:drift` 对 `docs/CORE_PORTS_AND_STORAGE.md §5` 与 `crates/core/src/ports.rs` 的逐字比对、`check:schemas` 与 `check:fixtures` 双向门禁全绿 | `reports/AC2.log` |
| AC3 | 主分支（替代 Main E2E 的跨语言协议一致性） | `clients/app` 的契约测试入口（`D:\Project\acp-remote\clients\app`） | 独立 `clients/app/node_modules/`；读仓库根 `fixtures/sync/v1/` | 读同一份 `fixtures/sync/v1/manifest.json` 与 `schemas/sync/v1/**`，且对 `fixtures/sync/v1/transcripts/device-proof.json` 完成跨语言字节级对拍 | `reports/AC3.log` |

`deps`/`advisories`/`secrets` 三个 CI-only job 无本地等价物（`docs/adr/0008-ci-supply-chain-tooling.md` 残余风险第 5 条），本地不得声称通过。

## Independent Validation

| Task | Target Revision | Assignment | Pass Condition | Report Path |
| --- | --- | --- | --- | --- |
| IV1 | 最终主分支固定提交（MU1a–MU3e 全部合入后），并附前端构建产物标识与依赖版本 | 覆盖充分性审查、待验证假设与风险盲区：重点核对路径越界判定是否覆盖规范化前缀边界、会话明细分页是否覆盖不重不漏、imported 资源是否有落盘断言、设备身份固定向量对拍是否为真实字节比对 | 覆盖上述风险盲区且无未解释缺口；发现缺口记 FAIL 并列出具体条目 | `openspec/changes/sync-scope-and-pwa-client/reports/IV1.log` |

### Code Review

按 `roles/reviewer.md`。分层：MU1 只审合同资产与文档口径（重点：`snapshotDigest` 与 `chunkCount` 一致性、Node Link 侧未受影响、`node-link-protocol` DTO 未变）；MU2 审 Rust 实现（重点：行级 diff 的边界输入、路径规范化前缀判定、节点级事件的 DDL CHECK 自洽、ACP 原文三要素未变）；MU3 审前端（重点：页面是否直接触碰平台能力、协议 DTO 是否泄漏进视图模型、私钥是否可导出、`no-content-cache` 是否被绕过）。每个 WP 的 Reviewer 非空且不等于 Owner；候选只审 rebase 新冲突解决，内容指纹未变且干净 rebase 时可复用既有 review 结论，须记录依据与原报告 ID。**每个 WP 的独立 review 任务见 tasks，不允许自审。**

### Main E2E

```yaml
mode: not-applicable
reason: "本次变更含完整前端工程，而前端所需的真实入口尚不存在：server::sync 未落地、/ui 静态资源托管未实现、仓库没有浏览器 E2E 驱动入口（openspec/agentic.yaml 的 e2e.command 为空）。以 fixtures/sync/v1/ 为数据源运行前端属于原生 mock，按 schema 判据不得替代场景。"
basis: "仓库既有约定（openspec/config.yaml 的 E2E 条目）：项目级 e2e.enabled 为 true 且 command 未配置时，逐变更记录降级批准；实测 openspec-agentic e2e --json 返回 enabled=true、command=\"\"。MU2 的 core 生产者部分本可跑真实 Daemon 入口（参见 crates/app/tests/node_link_e2e.rs 的既有做法），但本次以替代检查覆盖，不单列为 Main E2E。"
alternative_checks:
  - AC1
  - AC2
  - AC3
downgrade_approval: "用户 2026-10-03 在本次会话的 E2E 模式提问中，选择标注为「not-applicable（推荐）」的选项，其说明为「理由：前端无真实入口（server::sync 未落地、无浏览器驱动、fixtures 属 mock）。core 生产者部分改用跑真 Daemon 的集成测试作为替代检查」。来源：本会话 /opsx-propose 规划对话。该记录仅适用于本次变更。"
```

替代检查：

| ID | 覆盖的风险 | 验证方式 |
| --- | --- | --- |
| AC1 | 派生事件在真实 Daemon 上**可持久化并可读回**（不含广播，见 Risks 的 F5 条） | `cargo test --locked -p app --test node_link_e2e`（真实 Daemon 进程、真实 SQLite、fake ACP Agent），扩展为断言：`file.changed`（会话级）经 owned 路径落库并按 origin 顺序回放；`agent.connected`（节点级）落库后会话标识为空、可按事件库读回，且不被会话级投递路径误收；会话标题在 `session_info_update` 后更新。**不验证广播**——节点级事件在 `server::sync` 落地前没有投递通道（`crates/server/src/node_link/resource.rs` 的会话级投递路径会丢弃会话标识为空的事件）；证据 `reports/AC1.log` |
| AC2 | 合同门禁与存储契约未漂移 | `npm run verify`（PV1 全量），含 `check:drift` 对 `docs/CORE_PORTS_AND_STORAGE.md §5` 与 `crates/core/src/ports.rs` 的逐字比对、`check:schemas` 对 fixtures 的双向门禁；证据 `reports/AC2.log` |
| AC3 | 前端协议语义与 Rust 侧一致 | `clients/app` 的契约测试读同一份 `fixtures/sync/v1/manifest.json` 与 `schemas/sync/v1/**`，并对 `fixtures/sync/v1/transcripts/device-proof.json` 做跨语言字节级对拍；证据 `reports/AC3.log` |

## E2E Execution Plan（apply 阶段生成，执行前冻结）

本变更 Main E2E 为 `not-applicable`，本节在 apply 阶段不产出最终主分支 E2E 计划；上述 AC1–AC3 的执行证据按各自的 Check ID 登记在 verification 的 `## Checks`。

## Failure and Recovery

- **修复归属**：失败包的修复使用原 WP ID、新实例，经 `openspec-agentic dispatch --reopen --retry-kind <implementation|contract|environment|runtime>` 重新派发；四类各限 3 次。
- **下游失效范围**：MU1 契约变更使 MU2/MU3 的字段假设失效 → MU2、MU3 全量重跑 PV1/PV2/PV3；MU2 的 core 端口签名变更仅使 MU3 的 `protocol/` 与状态机编译面失效 → 重跑 PV3，不需重跑 MU2。范围按 Dependency Handoffs 的「Invalidation」列执行。
- **资源异常**：浏览器分片实例未清理时，由 provisioner 回收该 profile 目录并记入 verification 的 `## Runtime Resources`；`CARGO_TARGET_DIR` 污染时清理该包目录后重跑。
- **回滚条件与条件**：(a) 若 D1（快照收窄）导致 `command.uncertain` 或 imported 语义需要重新裁决 → 按能力边界回滚 `sync-snapshot-scope`，MU3 的目录页与对话页骨架不受影响；(b) 若 D4 的行级 diff 在实测中出现无法在本变更内收敛的病态输入 → 保留 `file.changed` 字段与生产者，省略行数统计（specs 场景「判定不出时省略而非填零」已覆盖该分支），不整体回滚；(c) 若前端工程无法在无 `server::sync` 的条件下通过 PV3 → 保留 MU1、MU2，回滚 MU3，前端作为独立变更重做。
- 回滚一律另需用户授权；MU1a–MU3e 的本地合入按本流程已约定直接执行。

## Completion Criteria

- 最终目标：`refs/heads/main` 的固定提交，全部八个交付单元（MU1a / MU1b / MU2 / MU3a / MU3b / MU3c / MU3d / MU3e）已合入（Round 8 的 F30：原量词「四个」与同句枚举的八个单元矛盾）。
- verification 证据：`## Checks` 表中 PV1/PV2/PV3/IV1 与 AC1/AC2/AC3 全部 PASS 且报告可读；`## Premerge History` 中 MU1a/MU1b/MU2/MU3a–MU3e 各有对应 PASS；`## Dependency Declaration Review` 为 PASS 且绑定当前 planningDigest；`## Worktree Handoff` 与 `## Runtime Resources` 已登记。
- 阻断清零：无未解释的 FAIL、无缺失的报告、无过期的审查绑定。
- 最终验收入口：`.agents/skills/agentic-verify/SKILL.md`；按 `openspec/schemas/agentic/procedures/acceptance.md` 核对，并在 verification 的 `## Final Assessment` 记录 target_commit 与 PASS/FAIL/BLOCKED。`all_done` 只表示任务复选框完成，不代替该验收。
- 交付状态分别说明：本地合入（MU1a–MU3e 已合入主分支）、远端交付（PR 与必需 CI 检查，按 `AGENTS.md` §8 走，本变更未授权推送）、部署/发布（未执行）、归档（未执行）。

## User Deliverables

| Deliverable | Location / Version | Usage / Configuration | Recipient | Completion Standard | Authorization / Prerequisites |
| --- | --- | --- | --- | --- | --- |
| 机器合同资产 | `schemas/sync/v1/**`、`compatibility/**`、`fixtures/sync/v1/**` | 由 `npm run check` 十道脚本校验；Rust 与 TypeScript 两侧共用 | 仓库维护者、前端工程 | `npm run check` 全绿，含 `check:schemas` 与 `check:fixtures` 双向门禁 | 本变更范围内 |
| 权威文档更新 | `docs/SYNC_PROTOCOL.md`、`docs/FRONTEND_DESIGN.md`、`docs/ACP_COMPATIBILITY_MATRIX.md`、`docs/CORE_PORTS_AND_STORAGE.md` | 与代码同提交，供实现与评审引用 | 仓库维护者 | 与代码一致，`check:docs` 全绿 | 本变更范围内 |
| 前端工程源码 | `clients/app/` | 未配置：缺 `server::sync` 与 `/ui` 托管，无法在浏览器中连接真实主机；可对其自带 fixture 运行器启动 | 前端开发者 | `clients/app` 的类型检查、契约测试与浏览器用例（PV3）全绿 | 本变更范围内 |
| 静态托管入口 | — | **不产出**：`/ui` 托管属独立变更 | — | — | 未授权，不执行 |