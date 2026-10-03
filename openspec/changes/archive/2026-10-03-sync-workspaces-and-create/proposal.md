<!-- 记录动机、范围、能力与影响；行为写 specs，方案写 design，协作写 plan。 -->

## Why

设计原型 `prototypes/acp-remote-pwa.html` 被用户确立为 PWA 第一版的验收基准，但它画的三件事与当前权威合同相反：**目录**（本机注册的 workspace，独立于会话存在，会话归属其中）、**Agent 目录**（`⊕ 新建会话` 的选择器），以及从设备侧发起的 **`session.create`**。权威文档此前把这三项明确排除在 Sync v1 之外（`FRONTEND_DESIGN §9` 第 1 条「PWA v1 无法通过 UI 或构造普通命令创建会话」、`SECURITY_DESIGN §9`「Sync 首版不暴露该入口」、`SYNC_PROTOCOL §11.3` 把 `session.create` 列在「Sync v1 尚未定义」清单），且 `SessionSummary` 至今没有任何目录字段。用户已决定以原型为准，把此前明确不做的项补入范围。本次只交付**合同与行为规范**（wire、词表、授权语义、存储列），`server::sync` 的实现属后续变更。

## What Changes

- **Sync 目录能力（新）**：`SessionSummary` 新增设备可见的 workspace 引用 `{ alias, displayName } | null`（`null` = 原型的「未分组」）；快照新增两个资源：已登记 workspace 目录与已配置 Agent 目录；两者按设备 scope 过滤，使原型在离线态仍能渲染目录页。
- **Sync 侧 `session.create`（新）**：`commands.json` 的 `session.create` 增加 `sync` transport；payload 为 `{ workspaceAlias, agentId }`（不含 Node Link 的 `exportId`/`templateParams`）；新增 `pack.create-session`；新增 feature ID（`core.local-catalog.v1`、`core.session-create.v1`）。
- **授权语义**：`session.create` 对设备按**设备级 scope** 生效——授予后覆盖当时及此后全部已登记的 workspace 与已配置的 Agent，这条副作用 MUST 显式写入 `SECURITY_DESIGN`，不得隐含在 scope 名义下。
- **目录标识口径**：目录在 wire 上**只**以 `{ alias, displayName }` 表示；`normalize` 后的规范化路径继续不得进入任何对端可见输出（`SECURITY_DESIGN §12.3`、`workspace-resolution`）。会话在创建时持久化其所用别名，使目录归属不随 workspace 重指向而漂移。
- **存储**：`owned_session` 追加可空列 `workspace_alias`；文件格式与 owned 家族版本推进到 v6，只做追加、不触发表重建。
- **词汇与合同同步**：`compatibility/commands/v1/commands.json`、`compatibility/features/v1/features.json`、`schemas/sync/v1/**`、`fixtures/sync/v1/**`，以及 `docs/SYNC_PROTOCOL.md`、`docs/SECURITY_DESIGN.md`、`docs/FRONTEND_DESIGN.md`、`docs/CORE_PORTS_AND_STORAGE.md` 的相应章节。
- **本次不包含**：`server::sync` 与 `/ui` 托管的实现、前端工程、`node-link-client` 与 `server::acp_facade`、把规范化路径下发（原型详情页显示真实路径的方案）、`sessions.queue_policy` 的口径决定（原型对「进行中拒绝发送」与 `queued` 状态自相矛盾）。

## Intent and Constraints

```agentic-intent
sources:
  - "用户 2026-10-02 本会话原话：「以原型为准，原来明确不做的，现在都要按照原型来补充」——确立原型为 PWA v1 的范围基准"
  - "用户 2026-10-02 本会话原话：「那就B」——目录标识口径选 B：wire 上只投影 { alias, displayName }，不投影规范化路径（方案 A/C 均被否决）"
  - "用户 2026-10-02 本会话原话：「那就设备级」——session.create 对设备按设备级 scope 生效"
  - "原型文件：prototypes/acp-remote-pwa.html（桌面端）与 prototypes/acp-remote-pwa-mobile.html（移动端）；目录数据 DIRS/EXTRA_DIRS（`:731`）、创建弹层（`:1909`）、目录页与详情页（`:1335`、`:1422`）、scopes 列表含 session.create 默认 off（`:723`）"
  - "既有权威约束：docs/FRONTEND_DESIGN.md §9（第 1 条验收标准）、docs/SECURITY_DESIGN.md §9/§12.3、docs/SYNC_PROTOCOL.md §11.3/§16.2、docs/CORE_PORTS_AND_STORAGE.md §3.6（workspace_cwd 不得进入可投影形状）"
constraints:
  - "目录在 wire 上只以 { alias, displayName } 表示；规范化路径 MUST 继续不出现在 catalog、事件、错误 details、快照、审计前像与任何对端可见输出中"
  - "session.create 的设备级授权语义 MUST 被显式记录，包括「此后新登记的 workspace 与新建的 Agent profile 自动进入该设备可启动范围」这一副作用"
  - "会话创建 MUST 使用本机已登记的 workspace 别名与已配置的 Agent profile；不接受客户端提交的绝对路径、目录追加或 Provider/MCP 凭据"
  - "本次为合同与规范变更，MUST NOT 在本次实现 server::sync、/ui 或任何前端代码"
  - "协议合同变更必须同步 schemas/、fixtures/、compatibility/ 与权威文档，并让 npm run check 全绿"
  - "新增能力不得削弱既有不变量：先持久化后广播、每会话单 active turn、稳定 requestId 幂等、崩溃窗口进 uncertain"
non_goals:
  - "不实现 server::sync 入站适配层、/ui 静态托管与前端工程（后续独立变更）"
  - "不实现 node-link-client 与 server::acp_facade（切片 6，非 PWA 硬前置）"
  - "不把规范化路径或其派生片段下发给设备（否决方案 A 与 C）"
  - "不裁决 sessions.queue_policy（原型'进行中拒绝发送'与 queued 状态自相矛盾，记为 design 的 Open Question）"
  - "不为移动端原型缺失的门控（canCreate、revoked/incompatible/replaced 阻断态）单独定义语义，一律以桌面端为准"
success_criteria:
  - "命令目录、feature 登记、两份协议 schema 与 SYNC §11.5 / SECURITY §10.2 表格逐项一致，npm run check 全绿"
  - "SessionSummary 携带 workspace 引用，且任何对端可见输出都不含规范化路径（由规格场景与固定向量断言）"
  - "设备 scope 词表包含 session.create，且新 pack 在展开表（commands.json ↔ identity-auth 镜像）中一致"
  - "旧库（v1–v5）可升级到 v6，升级后既有会话行的 workspace_alias 为 NULL 且被解释为'未分组'，连续两次打开 DDL 逐字节相同"
decision_bounds:
  - "可自主决定：字段与类型命名、feature 的粒度与命名、快照资源的组织方式、pack 命名、payload 校验顺序、spec 场景与用例命名"
  - "需用户决策：改动安全模型或信任边界、改变设备级授权的语义、新增或重命名超出本次登记的封闭词表条目、扩展范围到 server::sync/前端实现"
assumptions:
  - "原型在离线态仍渲染目录页与'离线 · 2 分钟前同步'，因此目录数据必须离线可用 → 以快照资源承载，而不是仅提供在线查询命令"
  - "preset.remote-control 不纳入 session.create（原型把该 scope 默认设为关闭、由用户在配对时单独勾选）"
  - "displayName 是同名目录的唯一消歧依据（原型仅在重名时才显示额外路径信息，B 口径下该信息由用户命名承担）"
  - "桌面端原型语义优先于移动端（移动端缺 canCreate 门控与三个阻断态）"
```

## Capabilities

### New Capabilities

- `sync-workspace-catalog`: Sync 面可观察的本机 workspace 目录与 Agent 目录、`SessionSummary` 的 workspace 引用（`{alias, displayName} | null`）、两个新增快照资源及其按 scope 过滤与 feature 协商规则、离线可用性与路径不泄漏。
- `sync-session-create`: Sync 面的 `session.create` 命令契约：设备级 scope 与 `grant.remote-work`、payload 只允许 `workspaceAlias`/`agentId`、accepted 与终态语义、幂等与 `uncertain`、被拒字段与失败分类。

### Modified Capabilities

- `workspace-resolution`: 新增「会话在创建时持久化所用别名」与「会话投影只暴露别名/展示名、规范化路径继续不投影」的要求。
- `scope-expansion`: 新增设备授权包 `pack.create-session` 及其成员 `session.create`，纳入 `commands.json` 唯一机器来源与漂移门禁。
- `storage-schema-v2-migration`: 文件格式与 owned 家族版本推进到 v6，`owned_session` 追加可空列 `workspace_alias`（只追加、不重建），并定义 NULL 的语义。

## Impact

- **机器合同与固定向量**：`compatibility/commands/v1/commands.json`（`session.create` 的 `transport` 增加 `sync`、新增 `pack.create-session`）、`compatibility/features/v1/features.json`（新增 `core.local-catalog.v1`、`core.session-create.v1`）、`schemas/sync/v1/{common,command,sync}.schema.json`、`fixtures/sync/v1/**`。
- **Rust**：`crates/sync-protocol`（`SessionSummary` 新字段、`SnapshotResource` 与 `SnapshotItems` 两个新变体、`CommandName` 与 payload 形状）、`crates/core`（`SessionSummary` 投影字段、`SessionStore` 别名读写、`create_session` 写入别名、`broker::required_grant` 镜像）、`crates/storage-sqlite`（v6 migration 与会话读写）、`crates/identity-auth`（授权包展开镜像及其验收测试）。
- **权威文档**：`docs/SYNC_PROTOCOL.md`（§5.2 feature、§9.4 快照资源与 item、§10.3 值对象、§11.3/§11.5 命令、§16.2）、`docs/SECURITY_DESIGN.md`（§9 设备 create 规则与副作用、§10.2 命令/scope/pack/grant 表）、`docs/FRONTEND_DESIGN.md`（§4.1 必须实现、§9 第 1 条验收标准、§7 本地缓存、§2.2 产品边界）、`docs/CORE_PORTS_AND_STORAGE.md`（§3.6/§5/§7 会话行与投影形状、版本常量）。
- **存储与数据**：单次 `ALTER TABLE ... ADD COLUMN` 追加一个可空列，不做 12-step 表重建；旧行该列为 `NULL`，语义为「未分组」，不得按 cwd 反查补齐。
- **依赖**：不新增任何 crate 或第三方依赖。
- **安全**：不改变威胁模型与信任边界；不新增 `local.*` 能力；新增一个设备授权包与两个 feature ID。设备级授权的副作用必须写入 `SECURITY_DESIGN`。规范化路径的出网禁令保持不变。
