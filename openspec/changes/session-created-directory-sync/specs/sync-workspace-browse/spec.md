## MODIFIED Requirements

### Requirement: 目录浏览不产生任何登记或持久化副作用

`workspace.browse` SHALL 是只读查询：它 MUST NOT 写入任何登记、缓存或会话状态，MUST NOT 改变本机目录登记表，也 MUST NOT 让被查看的目录因此具备任何后续权限。经浏览选定的新目录在会话成功创建时 SHALL 与该会话在同一事务中登记，并提供稳定的目录归属；客户端 SHALL 通过服务端快照取得该目录的标识和路径。

#### Scenario: 浏览不登记

- **WHEN** 一台设备浏览了若干层目录
- **THEN** 本机目录登记表不因这些调用发生任何变化

#### Scenario: 浏览不产生审计记录

- **WHEN** 一次 workspace.browse 完成
- **THEN** 系统不写入审计记录

#### Scenario: 新目录随成功会话登记

- **WHEN** 用户经浏览选定一个未登记的目录并据此创建会话
- **THEN** 成功落账时登记该目录及会话归属，之后的快照包含该目录；仅浏览或独立创建空文件夹不产生这项登记
