<!-- 已有能力（workspace-resolution）的增量：省略 ## Purpose，只写需求变化。 -->

## ADDED Requirements

### Requirement: 恢复时的目录复校验

恢复用例 SHALL 在调用会话后端之前，对持久化的创建时工作目录执行与创建时同口径的校验：必须是绝对路径、必须存在且为目录，且 `canonicalize` 的结果 MUST 与持久化取值相同。持久化取值为 `NULL`（该会话本就没有可恢复数据）时 MUST 与「能力不支持」走**同一条路径**（`nodelink.command.unsupported`）返回；其余校验失败（目录被删除、被替换为文件、规范化结果不同、无权限读取）时 MUST 返回服务端不可用类错误。两类失败都 MUST NOT 回退到按别名重新解析，也 MUST NOT 改用一个「最接近」的目录。

「它曾经合法」MUST NOT 被当作跳过校验的理由；每次恢复都重新校验一次。

#### Scenario: 目录已被删除时返回不可用且不启动 Agent

- **WHEN** 一条会话的持久化创建时目录在恢复前被删除（或不可访问）
- **THEN** 恢复在调用后端之前失败并返回服务端不可用类错误，不启动 Agent 进程、不产生新会话，也不写入任何目录取值

#### Scenario: 规范化结果变化时拒绝恢复

- **WHEN** 持久化路径仍然存在，但 `canonicalize` 的结果与持久化取值不同（例如被换成指向别处的符号链接）
- **THEN** 恢复返回服务端不可用类错误，不使用新解析出的路径发送 `session/resume`

#### Scenario: 目录仍然有效时使用持久化取值

- **WHEN** 持久化目录仍然存在、是目录，且规范化结果与持久化取值一致
- **THEN** 恢复使用该持久化取值作为 `session/resume` 的 `cwd`，不按 workspace 别名重新解析
