# Scope Expansion 增量

## MODIFIED Requirements

### Requirement: 本地管理能力永不远程授予

系统 SHALL 让本地管理能力不参与任何展开，也 MUST NOT 出现在远程授权面中；它们只能由本地管理入口在操作系统用户边界校验后使用。本变更后远程授权面不再有名称展开：已认证的节点即为执行前提，但 `local.*` 能力仍然永不远程可用。

#### Scenario: 本地能力不能被请求覆盖

- **WHEN** 请求方尝试通过任何输入形式把某项本地管理能力纳入远程可用集合
- **THEN** 请求失败或该项被拒绝，远程侧不出现本地能力

#### Scenario: 远程命令不接受本地能力

- **WHEN** 一次远程连接携带包含本地管理能力的授权集合
- **THEN** 该集合被拒绝或这些项被忽略且不产生任何授权效果，本地管理面不因远程请求而改变

## REMOVED Requirements

### Requirement: 导出授权的展开与拒绝

**Reason**: 跨节点导出授权（`grant.*`）整族删除；没有任何授权词汇需要展开，未知名称的拒绝也随之失去对象。

**Migration**: 有效权限不再由「Export scopes ∩ Import grants ∩ 能力」计算；已认证节点即可执行本节点命令（`local.*` 除外），见威胁模型。

### Requirement: 展开表的唯一机器来源与漂移可见

**Reason**: `compatibility/commands/v1/commands.json` 不再有 `grants` 块，代码侧也不再有展开表，因此"两侧一致"的门禁对象消失。

**Migration**: 命令目录的机器来源仍是 `commands.json`；门禁继续校验命令名与本地能力，但不再校验 grant 成员。

### Requirement: 展开只影响授权输入，不产生授权结论

**Reason**: 不再有展开结果，也就不存在"展开结果是否构成授权"的问题。

**Migration**: 授权判定只剩连接级边界（已认证、未撤销、连接上限）与 `local.*` 的本地限制，见 `docs/SECURITY_DESIGN.md`。
