# Tasks

## 1. 主规格改写（WP1）

- [x] 1.1 `openspec/specs/scope-expansion/spec.md` 删除整条「设备授权包 `pack.create-session`」（含它的两个场景）——判据：`grep -c "### Requirement: 设备授权包 pack.create-session" openspec/specs/scope-expansion/spec.md` = 0，且该文件的 `### Requirement:` 由 5 条降为 4 条
- [x] 1.2 同文件按 design「逐字改写文本」第 1 条替换 `## Purpose`——判据：`grep -c "设备授权包\|配对预设" openspec/specs/scope-expansion/spec.md` = 0
- [x] 1.3 同文件按第 2 条改写「本地管理能力永不远程授予」：要求本身保留，正文去掉「设备 scope」这一面，第一个场景改名并改写、第二个场景原样保留——判据：正文与场景逐字等于给定文本，且要求标题仍存在
- [x] 1.4 同文件按第 3 条改写「展开表的唯一机器来源与漂移可见」——判据：正文与两个场景逐字等于给定文本，且不再出现「授权包/预设」字样
- [x] 1.5 `openspec/specs/sync-session-create/spec.md` 按第 5 条改写「引用失败与解析失败的分类」——判据：`grep -c "authorization.scope_denied" openspec/specs/sync-session-create/spec.md` = 0；新增场景「未配置的 Agent 在受理之后失败」存在且写明 `failed` 终态与零会话行
- [x] 1.6 对照清点与门禁：`openspec validate --specs --strict` 通过；两份主规格的要求条数与 design「逐字改写文本」的清单逐条一致（`scope-expansion` 4 条、`sync-session-create` 5 条）；`npm run check` 十项全绿——判据：三条命令的输出，以及逐条点名的清点记录（删了哪条、改了哪几条、各留几个场景）
