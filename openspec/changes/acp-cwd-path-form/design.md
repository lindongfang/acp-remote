# Design

## Context

core 存的规范化路径是 `std::fs::canonicalize` 的结果，在 Windows 上带 verbatim 前缀（`\\?\D:\…`、`\\?\UNC\server\share`）；它同时是内部比对路径一致性的基准。`agent-host` 目前把这个字符串原样填进 ACP 的 `session/new` 与 `session/resume` 的 `cwd`。上游 ACP 快照对该字段只要求「绝对路径」，两种形式都合法（`schemas/acp/v1/upstream/schema.json` 的 `NewSessionRequest.cwd`）。仓库里已有一个剥前缀的实现（`core/src/derive.rs` 的 `strip_verbatim`），但它只服务 `file.changed` 的展示路径。

## Goals / Non-Goals

**Goals:**
- 发给 Agent 的工作目录是普通绝对路径，使把 `cwd` 当路径片段使用（拼目录名、做前缀判断）的 Agent 不被 `?` 这类非法字符打坏。
- 这条转换规则在仓库里只有一处定义。
- 持久化取值、core 解析结果与恢复时的复校验**一行都不改**，因此不需要数据迁移。

**Non-Goals:**
- 不改 `docs/ACP_COMPATIBILITY_MATRIX.md` §6 的 `session/new.cwd` 行（那一行管 Access 侧 `acp_facade` 的映射，与本变更不是同一条路径）。
- 不做「只在路径过长时保留前缀」的分支。
- 不改本地 CLI 的 `rootPath` 显示，不改数据库列。

## Decisions

**D1 只在出站参数构造处转换，不动权威值。** 备选是「在 core 的解析处统一剥」（会让已落库的取值与新取值形式不一致，恢复时"持久化取值 vs 重新 `canonicalize`"的比对立刻失配，必须迁移）与「两处都剥 + 迁移」。选前者：`workspace-resolution` 定义的权威值口径不变，边界转换只影响对外的字节。

**D2 复用 core 里那份 `strip_verbatim`，不复制。** 备选：在 `agent-host` 内自写一份（改动面最小，但会让同一前缀规则在仓库里有两处实现——这正是本次事故的教训）；或提到叶子 crate（为一个 8 行函数新建 crate，按仓库口径过重）。做法是让它对 `agent-host` 可见。

**D3 总是剥，不做长度分支。** 备选：剥完超过经典上限就保留 verbatim。那条路会迫使同一个 Agent 同时应付两种形式，且长路径那一路照样会炸——复杂度换不来安全。已知代价：>260 字符的目录在个别 Agent 侧可能打不开；这是如实接受的代价。

**D4 把规则写成一条能力要求，而不是给两个函数各打一个补丁。** 上游 ACP 快照里还有 4 处「工作目录」字段（`terminal/create`、`session/list` 的过滤、`session/load`、`SessionInfo` 回报）；今天本仓库不构造它们（DTO 集合与宣告的客户端能力都不含），但将来实现时必须沿用同一条规则。

## Risks / Trade-offs

- **修好前缀不等于端到端可用**：主判据是「经 Daemon 用真实 Agent 建成一个会话」，这条从未跑通；剥前缀后仍可能撞上能力协商、`session/update` 处理或 `configOptions` 形状等第二个坑。tasks 里必须把「若仍失败则继续定位」写成显式判据。
- **UNC 还原必须与既有实现一致**：`\\?\UNC\server\share` → `\\server\share`，不能只处理 `\\?\` 一种形态。
- **非 Windows 平台**：转换必须是直通，避免把类 Unix 的合法路径改坏。
