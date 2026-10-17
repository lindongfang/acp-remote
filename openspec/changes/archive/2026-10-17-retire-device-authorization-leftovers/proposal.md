# Proposal

## Why

**用户原始要求**：把 `pairing-model` 归档时查出的「delta 没声明要删、但主规格里仍是旧口径」的几处收干净（本变更只做规格文本的收口，不改任何行为）。

**现状**：`pairing-model`（#140）删掉了设备权限层、设备授权包/配对预设两类词汇与它们的展开规则，实现与 wire 都已生效；但那次变更的 delta 只声明了「加什么、改什么」，没有声明这几处要删/要改，于是主规格里留下了**读起来像规范、实际没有实现支撑**的文本：

- `openspec/specs/scope-expansion/spec.md` 的 `### Requirement: 设备授权包 pack.create-session` **整条仍在**：它要求系统「登记设备授权包 `pack.create-session`」、其成员 MUST 与 `compatibility/commands/v1/commands.json` 中该包列出的成员逐项相等。而 `commands.json` 里**已经没有 `packs`**，代码里也没有对应的展开表——这条要求今天既不可能实现也不可能被判红。
- 同文件的 `## Purpose` 仍写「授权词汇（设备授权包、配对预设、跨节点导出授权）」。
- 同文件另两条要求仍在用不存在的输入面描述约束：「本地管理能力永不远程授予」把「设备 scope」列为远程授权面之一（设备方向已无授权词汇），「展开表的唯一机器来源与漂移可见」的两个场景仍逐一展开「授权包/预设/导出授权」。
- `openspec/specs/sync-session-create/spec.md` 的「引用失败与解析失败的分类」：未登记别名的场景仍要求返回 `authorization.scope_denied`（设备侧授权词汇，已随设备权限层删除）；且正文把「Agent 不在本机已配置集合内」写成**接受前**的参数类错误——实现里它发生在受理之后，以 `failed` 终态（`internal.unavailable`）收尾（见同文件「终态与幂等」）。

**为什么现在做**：主规格是行为合同的唯一权威。它如果继续包含与实现相反、且不可能被门禁发现的要求，下一个变更的智能合并与归档就没有可信基线；而这几处的存在只是「上一次变更没声明」的历史欠账。

## What Changes

一、`openspec/specs/scope-expansion/spec.md`

- **删除**整条 `### Requirement: 设备授权包 pack.create-session` 及它的两个场景（包本身已随设备权限层删除，`commands.json` 不再有 `packs`）。
- **改写 Purpose**：授权词汇只剩「跨节点导出授权」一类，删除设备授权包与配对预设。
- **改写**「本地管理能力永不远程授予」：远程授权面的清单去掉「设备 scope」（设备方向没有授权词汇），保留节点 grant、导出授权与远程命令授权。
- **改写**「展开表的唯一机器来源与漂移可见」：唯一机器来源仍是 `commands.json`，但被校验的名称与成员只剩导出授权（去掉「授权包/预设」）。

二、`openspec/specs/sync-session-create/spec.md`

- **改写**「引用失败与解析失败的分类」：
  - 未登记别名 / 形状非法的目录 ⇒ 参数类错误（Sync 面即 `protocol.schema_invalid`），**不再**写 `authorization.scope_denied`；
  - Agent 不在本机已配置集合内 ⇒ 受理之后以 `failed` 终结（错误码复用既有 `internal.unavailable`），指向「终态与幂等」；
  - 「不触碰文件系统」「不创建会话」「不回退到替代目录或替代 Agent」三条不变式原样保留。

三、范围形态

- 本变更**不新增能力、不改变任何要求的语义**，只把主规格的陈述改写到位；因此按 schema 约定在 `.openspec.yaml` 声明 `skip_specs: true`，不产出 delta 文件。受影响的主规格文件见 Impact。

## Capabilities

### New Capabilities

（无）

### Modified Capabilities

（无 delta 文件：本变更不改变任何要求的语义，只改写主规格的陈述；按 schema 约定声明 `skip_specs: true`。受影响的主规格文件在 Impact 里逐一列出。）

## Impact

- **只改两份主规格**：`openspec/specs/scope-expansion/spec.md`、`openspec/specs/sync-session-create/spec.md`。
- **不改**：任何 `crates/**` 代码、`schemas/`、`fixtures/`、`compatibility/`、`docs/`、已归档的 `openspec/changes/archive/**`（含 `pairing-model` 的原 delta）。
- **硬约束**：主规格仍是唯一权威；改写后的每一条都必须与已实现行为一致；`npm run check`（十项门禁，含 `check:openspec` 的 `--strict`）必须全绿。
- **非目标**：不重写上一次变更的归档产物；不新增或收紧任何要求；不顺手改动与此无关的措辞。
- **成功判据**：
  1. `openspec/specs/scope-expansion/spec.md` 里不再出现 `pack.create-session`／`pack.`／`preset.`，且「设备授权包」「配对预设」两类词汇不再出现；
  2. `openspec/specs/sync-session-create/spec.md` 里不再出现 `authorization.scope_denied`，且「Agent 不在配置集合内」的失败被写成受理之后的 `failed` 终态；
  3. 两份主规格的要求条数与场景数与改写计划逐条一致（不多不少）；
  4. `npm run check` 全绿。
