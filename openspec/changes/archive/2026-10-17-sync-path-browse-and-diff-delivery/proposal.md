<!-- 记录动机、范围、能力与影响；行为写 specs，方案写 design，协作写 plan。 -->

## Why

把前端对齐到原型 `prototypes/acp-remote-pwa.html`（提交 29fd6d7）时，有三处**原型已经在画、而今天的协议层给不出数据**：

1. **宿主目录路径。** 原型在目录页（同名时）、目录详情页、对话页左栏、新建弹层与文件夹浏览器里都要显示主机上的目录路径。但路径**根本不在 wire 上**：快照的 `workspaces` 元素只有 `{alias, displayName}`，`workspaceRef` 是 `additionalProperties: false`，`docs/SYNC_PROTOCOL.md` §9.1 与 `openspec/specs/sync-workspace-catalog` 都明文写「本机规范化路径 MUST NOT 出现在快照、摘要或任何对端可见输出中」。
2. **在主机上浏览目录。** 原型的「选择文件夹」窗口从「此电脑」的根列表逐层浏览、可新建一层。今天的命令词表里**没有**任何目录列举能力：`compatibility/commands/v1/commands.json` 13 条命令里没有，本地管理通道的 25 个方法里也没有，`docs/SECURITY_DESIGN.md` §8 更是把「原始 workspace 路径」明确划为**只属于本地管理**的能力。
3. **逐行差异正文。** 原型的对话页右栏逐行渲染 diff（新增/删除/上下文）。今天 `file.changed` 事件的视图只有 `{changeId, kind, displayPath, summary}` 加可选的 `addedLines`/`deletedLines`；`docs/SYNC_PROTOCOL.md` §10.2 与 §10.3 都写明「**不含**差异正文」，`docs/ACP_COMPATIBILITY_MATRIX.md` 第 181 行把 `tool_content.diff` 的第一阶段交付限定为「行级差异统计与相对化展示路径（不含逐行正文）」。

这三处都不是「实现没跟上」，而是**边界没开**。第四处是语义缺口：原型的「在浏览器里新建一层目录并选中」，其界面文案明确写「创建会话时会先在电脑上建它」——而今天 `sync-session-create` 规定「客户端提供的目录 MUST 存在且为目录」，目录不存在一律是失败。

为什么现在做：`docs/SECURITY_DESIGN.md` §10.3 对这类改动有明文约束——「未来改变这条边界属于产品与安全边界变更，需要更新本文及 `INITIAL_DESIGN.md`，不能只增加一个 command schema」。所以它必须先成为一个有规格、有 ADR、有安全文档更新的变更，而不是夹在前端改动里顺手加字段。

**用户原始要求**：要完整对齐原型，前后端都要对齐；先探索有哪些修改，再新建 change；提案先行。

**硬约束**：

- ACP 兼容性不变量不变：结构化事件不得被文本化，`acp.rawJson` 的字节保真不变，未知字段与扩展 payload 继续往返保真。
- Node Link 面的边界**不动**：Node Link 请求仍不得携带任意 Owner 路径，仍只接受 Export 发布的别名与 template。
- 凭据、MCP 配置、Agent 沙箱权限不因本次进入任何远程命令。
- `local.*` 七项本地管理能力**继续永不远程授予**；`local.workspace.select` 仍是「登记 workspace（别名 + 展示名 + 路径）」的唯一入口，本次新增的浏览**不写入任何登记**。
- 路径校验沿用既有唯一口径（`docs/CORE_PORTS_AND_STORAGE.md` §11.9 / `workspace-resolution`）：绝对路径、存在且为目录、`canonicalize` 为权威值、拒绝相对路径与含 `..` 的输入。
- 只对协商了新 feature 的客户端下发新字段与新命令，未协商的旧客户端行为逐字节不变。
- 不新增可下调配置键：新限额按 `docs/CONFIG_REFERENCE.md` §2 的口径作为固定 v1 常量写进 `docs/SYNC_PROTOCOL.md` §14。

**非目标**：

- 不做 Node Link 面的目录浏览或路径下发。
- 不做「远端目录的后台索引 / 全文搜索 / 文件内容读取」——浏览只给一层子目录的名字与绝对路径，不给文件内容、大小、时间戳。
- 不做 Drive 卷标（原型 `.da` 里的「系统盘 / 本地磁盘 / 移动磁盘」是 mock 装饰数据）：那是平台专有查询，收益不抵新增的平台接口面。
- 不改 `session.create` 的授权前提、幂等语义与终态分类；不改设备配对与认证。
- 不新增审计动作取值（审计动作是封闭词表，扩它会触发两张审计表的 12-step 重建）；浏览不产生审计记录，理由写进 `design.md`。

**成功判据**：

- 已认证且协商新 feature 的客户端可以从快照取得目录路径、可以用新命令逐层浏览并在界面里选定一层、可以在改动事件里拿到有界的结构化差异行。
- 未协商的旧客户端收不到任何新字段、新命令返回明确的 unsupported，且既有 fixture 与 schema 校验逐字节不变。
- `npm run check` 与 `npm run verify` 全绿；新增夹具在 `fixtures/sync/v1/manifest.json` 登记并通过 `check:schema-fixtures`；`crates/sync-protocol/tests/envelope_fixtures.rs` 的计数常量同步。
- `docs/SECURITY_DESIGN.md`、`docs/INITIAL_DESIGN.md`、`docs/SYNC_PROTOCOL.md`、`docs/ACP_COMPATIBILITY_MATRIX.md`、`docs/CORE_PORTS_AND_STORAGE.md` 与新增 ADR 一致描述新边界，无一处仍写「路径永不出网」。

## What Changes

**一、目录路径随目录资源下发（新 feature `core.workspace-path.v1`）**

- 协商该 feature 时，快照 `workspaces` 元素与会话摘要的 `workspace` 引用各多一个规范化路径字段；未协商时该字段**缺席**（不是 `null`，也不是空串），旧客户端因此逐字节不受影响。
- 路径取值是服务端已登记的规范化权威值；客户端 MUST NOT 拼接、规范化或反查它。
- 同步改写「路径不泄漏」口径：从「本机规范化路径永不出网」改为「**已登记**目录的规范化路径只下发给协商该能力的已认证设备；客户端提供的目录仍不得被回显；审计、日志与错误 `details` 仍不得含路径」。

**二、目录浏览（新命令 `workspace.browse` + 新 feature `core.workspace-browse.v1`）**

- 新增一条 Sync 查询命令：请求给一个父路径（`null` 表示要根列表），结果给该层的**直接子目录**（名字 + 绝对路径 + 是否已登记）。不返回文件、不返回大小与时间戳、不返回文件内容。
- 新增固定 v1 限额：单层条目数上限、路径长度上限（复用既有 4096）、命令速率沿用既有族分组；超限时明确截断并给出标记，MUST NOT 静默丢条目。
- 失败分类沿用既有口径：形状非法（相对路径、含 `..`、非目录）是参数类错误且**不触碰文件系统**；形式合法但不可达是服务端不可用类错误。
- 未协商该 feature 的客户端提交该命令时返回既有 `protocol.feature_required`。

**三、创建会话时允许先建目录（`session.create` 的显式开关）**

- `session.create` 的目录引用允许追加一个**显式**的「目录不存在则先创建」开关；缺省为假时行为完全不变（不存在即失败）。
- 开关为真且目录不存在时，服务端先创建该目录（含必要的父层）再启动 Agent；该目录**不会被登记成 workspace 别名**，因此该会话的目录归属是「未分组」。
- 父层不存在或没有写权限时以服务端不可用类错误失败，且不启动 Agent、不留会话行。

**四、结构化差异行随改动事件下发（新 feature `core.file-diff.v1`）**

- 从 ACP 工具调用内容里的**类型化 Diff**（`path` / `oldText` / `newText`）派生有界的结构化差异行（新增/删除/上下文），随 `file.changed` 事件视图下发；既有行数统计与展示路径口径不变。
- 有界：单处改动有行数与字节上限；超出时按既有 `rawUnavailable` 的口径显式标记为未下发并给出原因，MUST NOT 下发截断内容、MUST NOT 让客户端以为它就是全部。
- 客户端只有在收到结构化差异行时才逐行渲染；没有时呈现明确说明，不解析 ACP 原文来补。
- `docs/ACP_COMPATIBILITY_MATRIX.md` 与机器矩阵里 `tool_content.diff` 的第一阶段交付从「统计 + 路径」扩展为「统计 + 路径 + 有界逐行差异」，Layers 不变（PWA 仍为 `view_only`）。

**五、修正两处已过期的登记**

- `compatibility/features/v1/features.json` 里 `core.session-create.v1` 的描述仍写 `{ workspaceAlias, agentId }`，与今天的载荷（别名或客户端提供的目录，二者取一）不符；同步修正。
- `docs/SECURITY_DESIGN.md` §10.3 的正文仍写「Sync 面只能引用本机当前已登记的 workspace 别名」，与 §10.2 已放开的 `workspacePath` 自相矛盾；按 §10.2 收敛。

**六、安全与权威文档**

- 新增 ADR：为什么把「已登记目录路径」与「节点目录浏览」纳入已认证设备的能力面，以及它的限制（只读目录结构、无内容、无登记副作用、按连接与命令族限流）。
- 更新 `docs/SECURITY_DESIGN.md`（§2.1 / §3 / §8 / §10.2 / §10.3 / §12.3 / §14.1 相关段落）、`docs/INITIAL_DESIGN.md`（同步策略与产品边界里与路径相关处）、`docs/SYNC_PROTOCOL.md`（§5.2 feature 表、§9.1 快照资源、§10.2/§10.3 事件视图、§11.5 命令表与载荷、§14 固定限额）、`docs/ACP_COMPATIBILITY_MATRIX.md`、`docs/CORE_PORTS_AND_STORAGE.md`（新增端口与用例面）、`docs/MODULE_ARCHITECTURE.md`（职责描述）。
- 机器合同：`compatibility/commands/v1/commands.json`、`compatibility/features/v1/features.json`、`compatibility/acp/v1/matrix.json`、`schemas/sync/v1/**`、`fixtures/sync/v1/**`。

## Capabilities

### New Capabilities

- `sync-workspace-browse`: 已认证设备在受限范围内浏览节点上的目录（根列表与逐层子目录），用于在创建会话时选定一个工作目录；只读、不登记、不返回文件内容。

### Modified Capabilities

- `sync-workspace-catalog`: 目录资源与会话摘要的目录引用在协商新 feature 时携带规范化路径；「目录相关输出不泄漏规范化路径」条改口径。
- `workspace-resolution`: 「路径不泄漏」条由「任何对端可见输出都不含规范化路径」改为「已登记目录的规范化路径只下发给协商该能力的设备；客户端提供的目录不得被回显；日志、审计与错误 `details` 仍不含路径」。
- `sync-session-create`: 目录引用新增显式的「不存在则先创建」开关，及其失败分类与「不登记为别名」的口径。
- `core-derived-events`: 从类型化 Diff 派生有界结构化差异行（与既有行数统计同源），并按上限显式省略。
- `sync-event-projection`: `file.changed` 视图携带结构化差异行与省略标记；未协商时不携带。

## Impact

- **Rust**：`crates/sync-protocol`（命令名枚举与载荷/结果 DTO）、`crates/core`（新端口、用例面、差异行派生、目录浏览用例）、`crates/server/src/sync`（新查询命令处理、命令族归类、限额）、`crates/app`（组合根装配新端口）。
- **机器合同**：`compatibility/commands/v1/commands.json`（新命令 + 归入 `grant.observe` 分组）、`compatibility/features/v1/features.json`（3 条新 feature + 1 处描述修正）、`compatibility/acp/v1/matrix.json`（`tool_content.diff` 的 delivery/layers 描述）、`schemas/sync/v1/{command,common,sync,event-views}.schema.json`、`fixtures/sync/v1/**` 与 `manifest.json`。
- **门禁**：`check:commands`（命令 enum、`grant` 分组、`required_grant`、两份文档表格）、`check:features`、`check:error-registry`（若不新增错误码则不动）、`check:acp`、`check:schema-fixtures`、`check:drift`（新增端口即触发）、`check:boundaries`（不新增 crate 依赖则不动）、`cargo test`（`envelope_fixtures.rs` 计数、`CommandName::ALL` 计数、`CommandFamily::of` 穷尽 match）。
- **权威文档**：`docs/SECURITY_DESIGN.md`、`docs/INITIAL_DESIGN.md`、`docs/SYNC_PROTOCOL.md`、`docs/ACP_COMPATIBILITY_MATRIX.md`、`docs/CORE_PORTS_AND_STORAGE.md`、`docs/MODULE_ARCHITECTURE.md`、`docs/adr/`（新增一条）。
- **前端**：本变更只提供合同；呈现由变更 `pwa-prototype-parity` 承担。
- **兼容性**：全部新增项都走 feature 协商，未协商的连接收不到新字段、新命令被明确拒绝，因此 v1 内不需要 major version 升级（口径同 `core.event-origin.v1`）。
