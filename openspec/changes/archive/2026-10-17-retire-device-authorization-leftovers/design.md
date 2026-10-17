# Design

## Context

- 主规格是行为合同的唯一权威，但两道机器门禁都不核查「陈述与实现是否一致」：`check:openspec` 只做结构与格式校验（每个要求至少一个场景、长度上限等），`scripts/check-contract-assets.mjs` 的「已删除词汇不得回潮」扫描只覆盖 `compatibility/`、`schemas/`、`fixtures/` 与协议文档，**不覆盖 `openspec/specs/**`**。这两件事实合起来解释了这几处欠账为何能长期存活：结构合法、门禁不查。
- 四份权威文档在 `pairing-model` 里已按实绩改写，今天读到的都是「设备授权包/配对预设已随设备权限层删除」这类**正确表述**；欠账只在 `openspec/specs/**` 的两处。
- 「设备授权包不存在」有机器证据：`compatibility/commands/v1/commands.json` 里已无 `packs` 字段。
- 两处改写的措辞由实现归口决定（不是我的措辞偏好）：
  - **未登记别名**：`UseCases::create_session` 在**受理提交之前**完成别名→规范化路径的解析，因此它是接受前的参数类失败；Sync 面经 `port_rejection` 的 `InvalidRequest` 臂落到 `protocol.schema_invalid`。
  - **Agent 未在已配置集合内**：`agent-host` 的 `HostError::UnknownProfile` → `to_establishment_error` → `Unavailable(SessionNotEstablished)` → `create_failure_status` 判 `failed`、`create_failure_error` 取 `internal.unavailable`。它发生在受理之后（profile 解析在被调用的后端里，不在受理提交之前）。
- 改写前基线：`openspec/specs/scope-expansion/spec.md` 5 条要求 / 77 行；`openspec/specs/sync-session-create/spec.md` 5 条要求 / 91 行。

## Goals / Non-Goals

**Goals：**

- 让两份主规格里每一句话都能对上已实现行为：没有无实现支撑的要求、没有已删除词汇。
- 改动面最小：整删 1 条、改写 3 条 + 1 处 Purpose，不多改一个字。
- 改动可逐条复核：`npm run check` 全绿，且能用 grep 判据证明词汇确已消失。

**Non-Goals：**

- 不新增、不收紧、不放松任何要求（本变更不改行为）。
- 不重写 `pairing-model` 的归档 delta，也不改 `docs/`（那里已是正确表述）。
- 不给 `check-contract-assets` 新增扫描（见 Open Questions）。

## Decisions

**D1 用 `skip_specs: true` + 直接改主规格，而不产 delta。**

理由：本变更改的是「规范文本与已实现行为的不一致」，不是行为本身；而 delta 的语义是行为的增删改，且按 sync 规则 delta **无法**表达既有主规格的 Purpose 修正（Purpose 只在新增能力时随 delta 作为种子）。若改用 delta：apply 阶段无事可做（delta 的合并发生在归档），Purpose 仍要绕道处理。

- 备选①：用 `REMOVED` delta 删掉 pack 要求 + 两条 `MODIFIED` delta。代价：apply 阶段空转；Purpose 仍无从表达。
- 备选②：把「引用失败与解析失败的分类」整条删除，由 `pairing-model` 新增的「目录校验与失败分类」承担。代价：会丢掉「Agent 未在已配置集合内」这一条**仍在实现里成立**的行为（`agent-host` 的 `UnknownProfile`）。因此只改写、不删除。

**D2 「Agent 未配置」写成受理之后的 `failed` 终态。**

依据见 Context 的第二条归口。改写时**交叉引用**同文件的「终态与幂等」，不重复它的判据全文——避免同一判据在两处各写一份、未来又要同步两次。

**D3 词汇清理的边界只到「已删除词汇」。**

清掉：`pack.*`、`preset.*`、「设备授权包」、「配对预设」，以及把「设备 scope」列为远程授权面的表述。
保留：`grant.*`、命令 scope、节点方向的 scope/grant 表述（节点仍在用），以及「本地管理能力永不远程授予」这条要求本身（它约束的 7 项本地能力仍然存在，只是不再有「设备 scope」这一面）。

**D4 Purpose 与要求正文的改写文本逐字给出（见本文件末节「逐字改写文本」）。**

原因：本变更没有 delta 记录，若把措辞留给实现阶段临场创作，复核时就无从判断「是否只删词汇、没有夹带语义变化」。逐字给出后，reviewer 只需比对给定文本与落盘文本。

## Risks / Trade-offs

- 删掉 pack 要求后，规范不再提醒「存在设备级授权包」这一概念。取舍：该概念已无实现与机器资产支撑，真正的护栏在 `commands.json` 与 `check-contract-assets` 的词汇扫描——任何未登记取值都会在门禁处失败。
- 改写后的「引用失败与解析失败的分类」与 `pairing-model` 新增的「目录校验与失败分类」语义相邻，可能被读成重复。处理方式：在改写里显式交叉引用（前者指向后者的目录校验，后者不重复 Agent 归口）。
- 本变更没有 delta 记录，改动只体现在主规格 diff 与变更目录的 proposal/design/tasks 里。补偿：grep 判据 + `npm run check` 都可复核，且变更目录本身入库留痕。

## 逐字改写文本

落盘时必须与本节逐字一致（包括 `SHALL`/`MUST` 两侧的空格与四个井号的场景标题）。

`openspec/specs/scope-expansion/spec.md`：

1. `## Purpose` 整段替换为：

> 定义跨节点导出授权到命令级 scope 的展开行为，使 wire 与持久记录上只出现可独立判定的最小授权单位，未知名称显式失败，且本地管理能力永不进入远程授权面。

2. `### Requirement: 本地管理能力永不远程授予`：保留要求本身，正文去掉「设备 scope」这一面，第一个场景改名并改写，第二个场景原样保留（它不含设备词汇）。

> 系统 SHALL 让 7 项本地管理能力不参与任何展开，也 MUST NOT 出现在远程授权面（节点 grant、导出授权或远程命令授权）中；它们只能由本地管理入口在操作系统用户边界校验后使用。
>
> #### Scenario: 本地能力不能被请求覆盖
>
> - **WHEN** 请求方尝试通过任何输入形式把某项本地管理能力纳入远程授权集合
> - **THEN** 请求失败或该项被拒绝，远程授权集合中不出现本地能力

3. `### Requirement: 展开表的唯一机器来源与漂移可见`：正文与两个场景改为只校验导出授权。

> 展开所依据的名称与成员关系 SHALL 以 `compatibility/commands/v1/commands.json` 为唯一机器来源；代码侧的展开表与它逐条相等，任何一侧单独变化都必须让合同门禁失败。
>
> #### Scenario: 表与合同资产不一致时门禁失败
>
> - **WHEN** 合同资产新增或删除一个导出授权名称或成员，而代码侧展开表未同步
> - **THEN** 合同检查失败并指出不一致项
>
> #### Scenario: 展开结果与合同成员一致
>
> - **WHEN** 合同资产中的每个导出授权被逐一展开
> - **THEN** 展开结果与合同中该名称列出的成员集合逐项相等

4. **删除整条** `### Requirement: 设备授权包 pack.create-session`（含它的两个场景）。

`openspec/specs/sync-session-create/spec.md`：

5. `### Requirement: 引用失败与解析失败的分类`：正文改写（别名未登记仍是接受前的参数类错误；Agent 无 profile 改为受理之后的 `failed`），第一个场景去掉设备授权码，第三个场景为新增。

> 引用的别名不在本机登记集合内时，系统 MUST 以参数类错误拒绝并 MUST NOT 触碰文件系统；引用的 Agent 在本机没有对应 profile 时，失败发生在**受理之后**，以 `failed` 终态收尾（判据见本文件「终态与幂等」）。别名已登记但本机解析失败（目录被删除、被替换为文件、规范化失败或权限不足）时 MUST 以服务端不可用类错误拒绝。三类失败都 MUST NOT 创建会话、MUST NOT 启动进程、MUST NOT 回退到任何替代目录或替代 Agent。
>
> #### Scenario: 未登记别名是参数类错误
>
> - **WHEN** payload 引用一个本机从未登记的 workspace 别名
> - **THEN** 返回参数类错误（Sync 面为 `protocol.schema_invalid`），且不访问文件系统
>
> #### Scenario: 已登记但解析失败是服务端错误
>
> - **WHEN** 别名已登记，但其目录已被删除或已不是目录
> - **THEN** 返回服务端不可用类错误（`internal.unavailable`），不创建会话也不启动进程
>
> #### Scenario: 未配置的 Agent 在受理之后失败
>
> - **WHEN** payload 的 `agentId` 在本机没有对应的 profile
> - **THEN** 该命令先被受理，随后以 `failed` 终态收尾（错误码复用既有 `internal.unavailable`），本机不产生会话行

## Open Questions

- 要不要给 `check-contract-assets.mjs` 加一条「`openspec/specs/**` 不得出现已删除词汇」的扫描，让同类欠账在下一次发生时被门禁拦住？可安全延后；本次不动门禁。
