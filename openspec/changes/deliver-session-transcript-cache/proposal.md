# 交付独立持久化展示快照库与混合自适应历史恢复

## Why

各款 Harness 及其 ACP 适配器对标准历史重放的支持差异显著（部分仅支持无重放的 `session/resume`、部分重放存在缺陷、长会话全量重放耗时长），结合平台 30 分钟空闲自动回收机制，纯内存视图会导致无重放会话在回收或节点重启后白屏失忆，也让支持重放的长会话在打开时陷入重放白屏等待（或在满额、异常时无法查阅已有历史）。需要解耦“模型上下文恢复权威”与“界面历史展示快照”，在物理隔离的独立存储中建立有期限、有容量上限的持久化展示快照库，实现“先秒开本地快照、再后台自动拉取补齐外部更新”及无重放历史兜底。

## Success Criteria

### 用户原始要求

> 第2点是，acp-remote默认不保存会话历史数据，依赖harness层面的会话恢复，但是harness层面对这个的支持层面不一而同，这个看看怎么解决。能否调研一下竞品是怎么解决的
> 1. 采用混合自适应模式
> 2. 本地快照，设置一个期限吧，默认保留多久
> 同意
> 2G是不是太小了，调整成10G吧
> 同意
> 用户绕过acp-remote，往同一个底层会话追加了内容，我希望在acp-remote打来会话时，自动拉取一次，追加补充的会话，而不是手动点击再获取
> 同意

### 硬约束与非目标

- 以 [总体设计](../../../docs/architecture/overview.md) 与 `openspec/specs/` 长期规格为依据；**模型执行上下文恢复始终以 Harness 原生（`session/resume` 或 `session/load`）为唯一权威**，严禁拼接平台展示快照文本重发给模型伪造上下文，严禁解析各厂商 CLI 私有磁盘目录文件。
- 展示快照库必须与节点核心控制管理库（`node.sqlite3`）**物理隔离存储**；快照库故障、满额淘汰或过期清理不得阻塞设备信任、请求去重、严格权限审批及会话容量账本。
- 本地展示快照保留策略默认为**按会话快照最后写入/同步时间滑动保留 30 天**，总磁盘安全上限默认为 **10 GiB**（均支持在 `acp-remote.toml` 中配置）；活跃已加载会话受保护不清理，原生会话显式删除时立即同步擦除对应快照。
- 首期限定 Windows 节点与同源 Web，基于 [refactor-web-workbench](../archive/2026-11-20-refactor-web-workbench/proposal.md) 模块化基线实施，严格保持 Web 子模块单文件行数上限及单向无环依赖。

### 本 change 的成功判据

- SC1：建立与控制主库物理隔离的独立持久化展示快照库，忠实保存经标准 ACP 协议（实时 `session/update` 及标准重放）观测到的结构化轮次、思考与工具调用原位更新状态，复用既有不可信内容安全边界。
- SC2：实现三档混合自适应读取与恢复：① 打开未在节点活跃加载的会话时，若命中本地有效快照则立即秒开渲染（零白屏等待）；若对端支持标准重放，节点同时在后台按容量规则自动拉取一次最新原生历史，更新本地快照并在前端保持阅读位置无感补齐外部新增对话（50 名额被受保护会话占满或原生暂不可达时优雅降级保留本地快照视图并轻提示，同时保留手动重试入口；已处于活跃加载态的会话直接读取内存共享视图，不重复拉取）；② 接管恢复时支持 `session/resume` 轻量恢复上下文并与本地快照/已同步视图衔接；③ 对无重放或纯内存 Harness 用本地快照兜底界面历史并诚实标注来源与可继续性边界。
- SC3：实现默认 30 天滑动保留期与默认 10 GiB 容量上限自动清理器（超限时优先淘汰最久未更新的非活跃会话快照），保护当前已加载活跃会话，并与原生删除联动擦除。
- SC4：正式开工后通过编码前门禁、真实实现与独立集成交付验收。

## What Changes

- **BREAKING** 修正“平台绝不落盘任何聊天或工具展示内容”的限制：在坚守“不伪造模型上下文、不解析厂商私有文件、不混入核心控制库”的前提下，引入物理隔离的节点持久化【展示快照库】。
- 实现冷会话“先秒开本地快照 + 后台自动拉取补齐外部更新（Stale-While-Revalidate）”、满额/异常降级快照直读、接管恢复时的 `session/resume` 衔接，以及无重放会话历史留存兜底。
- 实现展示快照的默认 30 天滑动过期清理、默认 10 GiB 总容量淘汰、活跃会话保护及配置项解析。

## Capabilities

### New Capabilities

- `session-transcript-cache`: 物理隔离的节点本地持久化展示快照库、秒开快照结合后台自动原生校准的三档混合自适应读取/恢复衔接，以及默认 30 天滑动保留与 10 GiB 容量上限管理。对应文件为 `specs/session-transcript-cache/spec.md`。

### Modified Capabilities

- `structured-history`: 解耦原生内容权威与平台持久化展示快照，支持冷会话秒开本地快照并后台自动拉取补齐外部更新、无重放会话历史兜底及满额/异常降级。对应增量文件为 `specs/structured-history/spec.md`。
- `native-session-recovery`: 支持原生 `session/resume` 无重放上下文恢复与本地展示快照协同衔接，消除回收或重启后的界面白屏。对应增量文件为 `specs/native-session-recovery/spec.md`。
- `session-resource-policy`: 明确首屏快照直读与满额降级浏览不占名额、不刷新空闲计时，打开支持重放的未加载会话触发后台自动同步时按可安全淘汰的重放候选分配名额与刷新计时。对应增量文件为 `specs/session-resource-policy/spec.md`。

## Impact

- 依赖：[deliver-codex-session-loop](../archive/2026-11-16-deliver-codex-session-loop/proposal.md)、[deliver-session-capacity](../archive/2026-11-19-deliver-session-capacity/proposal.md)、[refactor-web-workbench](../archive/2026-11-20-refactor-web-workbench/proposal.md)，并为下游 [deliver-history-navigation](../deliver-history-navigation/proposal.md)、[deliver-agent-session-tree](../deliver-agent-session-tree/proposal.md)、[deliver-session-data-lifecycle](../deliver-session-data-lifecycle/proposal.md) 提供持久化展示快照底座；跨变更关系见 [能力地图](../../../docs/architecture/capability-map.md)。
- 影响范围：独立展示快照存储引擎（`transcript-cache.sqlite3`）与生命周期清理任务、节点配置（`acp-remote.toml`）、共享视图与后台自动重放协调端口、WSS 历史秒开与同步状态推送协议及 Web 前缀保持无感补齐与来源状态提示；具体工作包划分与写范围见 [plan.md](plan.md)。

## Exploration

- **竞品调研与混合自适应定位（用户确认 1）**：对比 Zed（纯 `session/load` 重放导致无重放适配器白屏及大线程首屏重放卡顿）、Happy Agent（平台 SQLite 持久化记录 + 原生 `resume` 恢复）与私有 `.jsonl` 扫描器后，确认采用“原生上下文权威 + 节点独立持久化展示快照库”混合自适应方案，兼顾跨 Harness 一致体验、零私有文件耦合与外部变更可同步性。
- **默认 30 天滑动保留期（用户确认 2）**：与 `management-audit` 默认 30 天保留期对齐，按会话快照最后写入/同步时间滑动计算，避免短期休假后无重放会话丢失界面历史。
- **默认 10 GiB 容量安全上限（用户确认 3）**：考虑到编码会话包含完整原始 JSON、多文件 Diff 与构建/测试工具输出，将默认总磁盘上限由初拟 `2 GiB` 上调至 `10 GiB`（支持配置调整），避免 30 天窗口内正常重度使用触发提前淘汰。
- **打开冷会话先秒开快照再后台自动拉取补齐（用户确认 4）**：针对外部 CLI 绕过 `acp-remote` 向同一底层会话追加对话的场景，确认打开未加载会话时无需手动点击刷新——先秒开渲染本地展示快照，同时对支持标准重放的 Harness 自动在后台拉取一次最新原生历史，前缀对齐保持阅读位置无感补齐外部新增对话（若原生发生压缩/删改则同步为最新权威视图）；50 名额被受保护会话占满或原生暂不可达时优雅降级保留本地快照展示并轻提示，同时保留手动重试兜底。
- **术语与备份边界复核**：在 [领域术语表](../../GLOSSARY.md) 新增“展示快照库”定义，并与 [deliver-backup-recovery](../deliver-backup-recovery/proposal.md) 对齐（展示快照仍属可过期淘汰的展示缓存层，不混入核心管理事实备份包），无新增独立 ADR。
