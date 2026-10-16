# Design

## Context

见 `proposal.md` — Why。三处边界缺口与今天的实况：

- **路径**：`schemas/sync/v1/common.schema.json#workspaceRef` 是 `{alias, displayName}` 且 `additionalProperties: false`；`docs/SYNC_PROTOCOL.md` §9.1、`openspec/specs/sync-workspace-catalog`、`docs/SECURITY_DESIGN.md` §12.3 三处都写「规范化路径不得出现在任何对端可见输出中」。因为 `workspaceRef` 是封闭对象，给已协商 `core.local-catalog.v1` 的旧客户端多塞一个键会让它按 `protocol.schema_invalid` 拒收整条消息——所以这条路只能走新的 feature 协商（口径与 `core.event-origin.v1` 相同）。
- **浏览**：`compatibility/commands/v1/commands.json` 的 13 条命令与本地管理通道的 25 个方法里都没有目录列举；`crates/server/src/sync/command/` 只有查询与 mutation 两条管线，加一条查询命令的落点是固定的（见 `plan.md` 的工作包写范围）。
- **差异行**：ACP 的类型化 Diff 是 `{path, oldText, newText}`（`crates/acp-protocol/src/content.rs:219-235`，上游 schema 也没有行级模型），core 已经用它按行级 Myers diff 算统计（`crates/core/src/derive.rs:8-9,145-146`），但 `file.changed` 视图只投影统计与展示路径（`derive.rs:531-550`）。
- **存储**：事件正文落在通用列 `payload_json`（`docs/CORE_PORTS_AND_STORAGE.md:1082-1107`），所以新增视图字段**不需要 DDL migration**；浏览是只读查询，不落盘。
- **core 依赖**：`std::fs` 已在 core 内被直接使用（`use_cases.rs:2055-2115`、`broker.rs:4358-4376`），浏览与建目录都不需要新 crate 依赖，`check:boundaries` 的 allow-list 不动。

## Goals / Non-Goals

**Goals:**

- 让客户端能拿到「显示用的目录路径」「逐层浏览并选定目录」「逐行差异」三样数据，且全部走 feature 协商实现加性扩展。
- 把「路径可见」这件事收在**一处**（目录资源条目），并且只覆盖**已登记**目录；其余所有输出继续不含路径。
- 复用既有唯一口径：路径校验五步、`canonicalize` 权威值、`strip_verbatim`、行级 diff、`rawUnavailable` 式的显式省略、命令族限流、`result_too_large`。
- 不新增审计动作、不新增可下调配置键、不新增 crate 依赖。

**Non-Goals:**

- 不做 Node Link 面（请求仍不得携带任意 Owner 路径）。
- 不做文件内容读取、目录索引、文件大小/时间/权限下发。
- 不做 Drive 卷标等平台专有元数据。
- 不把 `workspace.browse` 变成授权边界——设备通过认证即可用，与其它业务命令同前提。

## Decisions

**D1 路径只出现在目录资源条目上，且由新 feature 门控。**

`workspaces` 条目在协商 `core.workspace-path.v1` 时多一个路径键；会话摘要的 `workspace` 引用**保持两个键不变**，需要路径的客户端按引用里的别名在同一份快照内查表。

- 备选一：把路径加进 `workspaceRef`。否——同一目录会在每条会话摘要里重复携带，且别名被重指向后「这条摘要里的路径该是创建时还是当前的」变成一个必须额外规定的语义坑。
- 备选二：只在 `sessionSummary` 顶层加。否——重复面更大，且不解决目录页（它只需要目录资源）。

**D2 下发形式是「规范化结果剥掉平台 verbatim 前缀」。**

Windows 上 `canonicalize` 必然带 `\\?\` 前缀；原型界面显示的是 `D:\work\api`。core 已有 `strip_verbatim`（`derive.rs:486-505`，`agent-host` 出站参数也在用），复用它即可。客户端回传该形式时服务端**重新规范化**并以其为权威值，因此显示形式与权威值不会分叉。

**D3 浏览做成一条 Sync 查询命令 `workspace.browse`，请求一层、返回一层。**

请求只有父路径（`null` = 根列表），结果只有直接子目录的「名称 + 绝对路径 + 是否已登记」与一个截断标记。归入既有 `grant.observe` 分组（`grant` 在设备方向只是登记副本，不产生判定）。

- 备选一：把浏览并进 `session.create`。否——查询与 mutation 的终态语义完全不同，会让「查询命令同步完成」这条契约出现例外。
- 备选二：新开 `fs.*` 命令族。否——它的用途就是选 workspace，`workspace-resolution` 已经拥有「目录来源」这个概念；另立一族会让同一个语义有两个名字。
- 备选三：一次返回整棵子树。否——目录树可能极大，且一次性下发等于把整台机器的结构塞进一条消息。逐层是唯一同时满足「有界」和「够用」的形状。

**D4 不新增建目录命令；建目录做成 `session.create` 的显式开关。**

原型界面自己的文案就是「创建会话时会先在电脑上建它」，因此建目录不是一个独立步骤。开关取显式布尔值而不是「路径不存在就默默创建」，因为后者会把一次笔误变成一棵被建出来的目录树。开关与别名互斥（别名解析出的目录必然存在）。

- 备选：独立 `workspace.create` mutation。否——多一条 mutation 的幂等/终态/崩溃窗口面，而原型并不需要「先建目录」这个可观察步骤。
- 已建目录在后续失败时**不删除**：删除要区分「哪些层是这次建的」，而误删用户目录的代价远高于留下一个空目录。这条写进规格，避免实现随意回滚。

**D5 差异行在 core 派生，不在客户端算。**

core 已经为了统计做过行级 diff，差异行与统计同源；让客户端从 `oldText`/`newText` 自己算会引出第二份实现，而且要把整份文件的新旧文本都发到浏览器。

- 量：单处改动的行数与字节上限取固定 v1 常量（写入 `docs/SYNC_PROTOCOL.md` §14），超限按既有 `size_limit` 口径显式省略。
- 差异行落在既有 `payload_json`，不改 DDL。
- `docs/ACP_COMPATIBILITY_MATRIX.md` 里 `tool_content.diff` 的第一阶段交付从「统计 + 路径」扩为「统计 + 路径 + 有界逐行差异」，Layers 不变（PWA 仍 `view_only`）。

**D6 不新增审计动作。**

审计动作是封闭词表，两张审计表的 `action` 是 CHECK 约束，扩它会触发 12-step 表重建（`docs/CORE_PORTS_AND_STORAGE.md` §11.8）。浏览是只读查询，纳入既有的**命令级日志**（命令名、主体标识、资源标识、请求标识、终态类别）即可，日志字段里不写路径（`SECURITY_DESIGN.md` §14.1 已禁止）。

**D7 三个新 feature 而不是扩既有 feature。**

`core.workspace-path.v1`（目录资源携带路径）、`core.workspace-browse.v1`（浏览命令）、`core.file-diff.v1`（差异行）。理由同 `core.event-origin.v1`：既有对象是封闭的，加键会打断已协商的旧客户端的 schema 校验，因此必须让「多出来的键」只在协商后才出现。

**D8 新限额是固定 v1 常量。**

`docs/CONFIG_REFERENCE.md` §2 的口径是「只有三项 Sync 键可下调，其余上限都是固定常量」，因此单层条目上限、差异行行数/字节上限都写进 `docs/SYNC_PROTOCOL.md` §14，不新增配置键。

## Risks / Trade-offs

| 风险 | 缓解 |
|---|---|
| 路径对新客户端可见面扩大：任一被配对设备都能拿到本机全部已登记目录的路径 | feature 门控（未协商一律不下发）+ 只限已登记目录 + 日志/审计/错误 `details` 仍不含路径 + 新增 ADR 记录残余风险与理由 |
| 浏览成为目录结构探测面 | 只读、只返回一层、不返回文件与元数据、单层条目上限、命令族限流复用既有 120/分钟、不产生审计动作（避免第二条“谁看了什么”的隐私面） |
| 差异行放大事件体积与存储占用 | 行数/字节上限 + 超限显式省略 + 不改既有统计口径 + 保留期沿用既有事件保留策略 |
| 旧客户端被新字段打断 | 三处全部走 feature 协商；封闭对象只在协商后才多键；未协商时新命令返回 `protocol.feature_required` |
| `strip_verbatim` 后的路径回传导致规范化结果与显示值不一致 | `session.create` 与浏览的校验都**重新** `canonicalize` 并把结果作为权威值；显示形式只在 wire 上存在 |
| Windows 上 `canonicalize` 不归一化 `..` | 沿用 `canonical_workspace_path` 的「先拒绝 `..` 再 `canonicalize`」五步顺序，不依赖 `canonicalize` 做这件事 |
| 新增端口触发合同漂移门禁 | 端口签名同时改 `docs/CORE_PORTS_AND_STORAGE.md` §5 与 `crates/core/src/ports.rs`，并在同一工作包内完成 |

## Migration Plan

- **无 DDL migration**：差异行写入既有 `payload_json`；浏览不落盘；无新表、无新列。
- **无数据回填**：路径是投影期从既有 `owned_workspace.canonical_path` 读出的，不需要回填。
- **部署顺序**：服务端先升级即可——新 feature 只在协商后生效，旧客户端连上来仍按未协商处理。
- **回滚**：停发新字段/新命令即可；客户端侧无需迁移。已创建目录留在主机上（D4），不因回滚被删。
- **文档同步**：`SECURITY_DESIGN.md` §10.3 的「未来改变这条边界属于产品与安全边界变更，需要更新本文及 `INITIAL_DESIGN.md`」就是本次的执行项，两处必须在同一变更内改完。

## Open Questions

- 差异行的行数与字节上限取什么具体值（例如 400 行 / 64 KiB）——实现时按既有 `payload_json` 容量与单条消息 1 MiB 反推，取值写进 §14 即可，不影响规格与任务拆分。
- 浏览结果的排序规则用「Unicode 码点升序」还是「平台本地化排序」——前者可判定、跨平台一致，倾向前者；实现时若发现本地化排序对中文目录名体验明显更好再单独决定，不改变命令形状。
