<!-- 记录动机、范围、能力与影响；行为写 specs，方案写 design，协作写 plan。 -->

## Why

真实 Agent 在这台机器上**建不了会话**：`omp acp` 对 `session/new` 回 `ENOENT`，整条会话创建失败。根因不在 Agent，而在我们发出去的 `cwd` 形式——core 存的规范化路径是 Windows verbatim 形式（`\\?\D:\...`），`agent-host` 把它原样当作 ACP `cwd` 发出；Agent 拿 `cwd` 派生自己的会话目录名时撞上 `?`（Windows 文件名非法字符）。

实测（同一清空环境、同一个 `omp acp`、只改 `cwd` 一个变量）：

| 发给 Agent 的 `cwd` | 结果 |
|---|---|
| `\\?\D:\tmp\acpr-e2e\ws`（现状） | 失败：`ENOENT ... mkdir 'C:\Users\…\.omp\agent\sessions\---?-D--tmp-acpr-e2e-ws--'` |
| `D:\tmp\acpr-e2e\ws` | 成功，返回 `sessionId` |

上游 ACP schema 对 `cwd` 只要求「绝对路径」，两种形式都合法——所以剥前缀不是绕开协议，而是一次自由选择。仓库里也已有同款先例：`core/src/derive.rs` 的 `strip_verbatim`（注释给的理由完全相同），只是它只服务 `file.changed` 的展示路径。

**为什么现在做**：它是这次端到端验收里**唯一挡住真实 Agent 的功能性缺陷**；不修，「用真实 Agent 开会话」这条路在本机就是死的。

## What Changes

**一、出站 ACP 路径参数的形式**

- 构造 `session/new` 与 `session/resume` 的 `cwd` 时 SHALL 剥掉 Windows verbatim 前缀（`\\?\` 去掉；`\\?\UNC\server\share` 还原为 `\\server\share`），MUST NOT 原样发给 Agent。
- 该规则 SHALL 只有一份实现：复用 core 现有的 `strip_verbatim`（改为对 `agent-host` 可见），MUST NOT 在 `agent-host` 内复制第二份。
- 一律剥，MUST NOT 依赖路径长度分支。>260 字符的目录在个别 Agent 侧可能因此打不开，这是本变更**如实接受的已知代价**，写进 design。

**二、权威值与持久化不动（因此无需迁移）**

- core 的 `ResolvedWorkspace`、`owned_session.workspace_cwd`、`owned_workspace.canonical_path` 与本地 CLI 的 `rootPath` 显示 SHALL 保持现状（仍带 verbatim 前缀）。
- 恢复时「持久化取值 vs 重新 `canonicalize`」的复校验 SHALL 保持两侧同形式，MUST NOT 因本变更改变。

**三、规格**

- `local-agent-host` SHALL 新增一条要求，钉住「出站 ACP 路径参数的形式」——现状是该规格全篇未出现 `cwd`，创建侧对 `session/new` 出站参数形式**没有任何规定**。
- `local-agent-host` 的「进程不在时的会话恢复入口与执行」与 `workspace-resolution` 的「恢复时的目录复校验」SHALL 各补一句：线缆上的 `cwd` 是剥掉 verbatim 前缀后的形式。

**四、验收**

- 单测：断言两个参数构造函数产出的 `cwd` 不含 verbatim 前缀（含 UNC 形态）。
- 端到端：**经 Daemon 用真实 Agent（`omp acp`）建成一个会话**——这条今天从未跑通过，是本变更的主判据。

**本次不包含**：

- `docs/ACP_COMPATIBILITY_MATRIX.md` §6 的 `session/new.cwd` 行**不改**。那一行管的是 Access Node `acp_facade`（上游 Zed → Node Link）的映射，与 Owner/Daemon 构造出站 `session/new` 是两条不同路径（且 facade 侧在 `openspec/specs/` 下没有专属能力）。
- 超长路径的例外规则（不做「只在装不下时保留前缀」的分支）。
- 本地 CLI 的路径显示；core 权威值；数据库列与迁移。

## Capabilities

### New Capabilities

（无新增能力。）

### Modified Capabilities

- `local-agent-host`: 新增「出站 ACP 路径参数的形式」一条要求；「进程不在时的会话恢复入口与执行」补一句线缆形式。
- `workspace-resolution`: 「恢复时的目录复校验」的场景补一句线缆形式——该能力定义的「权威值」口径本身不变。

## Impact

- **代码**：`crates/agent-host/src/host.rs`（`session_new_params`、`session_resume_params`）；`crates/core/src/derive.rs`（`strip_verbatim` 改为可复用）。
- **规格**：上节两个能力的 delta spec。
- **权威文档**：`docs/MODULE_ARCHITECTURE.md` §4.5（若需登记「出站路径形式」这条实现约束）；`docs/ACP_COMPATIBILITY_MATRIX.md` 不改（理由见上）。
- **机器合同**：无改动（`schemas/`、`fixtures/`、`compatibility/` 不变）。
- **验收边界**：主判据是「经 Daemon 用真实 Agent 建成一个会话」。**风险如实登记**：剥前缀后仍可能撞上第二个坑（能力协商、`session/update` 的处理、`configOptions` 的形状），因此 design 要把「若仍失败则继续定位」写成显式判据，而不是假定一行改动必然通。
