<!-- 已有能力（scope-expansion）的增量：省略 ## Purpose，只写需求变化。 -->

## ADDED Requirements

### Requirement: 设备授权包 pack.create-session

系统 SHALL 登记设备授权包 `pack.create-session`，其成员为命令名 `session.create`。该包 SHALL 与其他授权包同口径展开为命令级 scope，其成员集合 MUST 与 `compatibility/commands/v1/commands.json` 中该包列出的成员逐项相等；任何一侧单独变化 MUST 让合同门禁失败。`pack.create-session` MUST NOT 进入任何配对预设，配对时只能被显式请求。

#### Scenario: 展开得到命令级 scope

- **WHEN** 一次设备配对显式请求 `pack.create-session`
- **THEN** 展开结果为命令名 `session.create`，状态查询与设备记录中不出现包名

#### Scenario: 包成员漂移被门禁发现

- **WHEN** `commands.json` 中 `pack.create-session` 的成员变化而代码侧展开表未同步（或反向）
- **THEN** 合同检查失败并指出不一致项
