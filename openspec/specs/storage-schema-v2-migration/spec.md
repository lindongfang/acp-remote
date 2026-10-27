# storage-schema-v2-migration Specification

## Purpose

本能力定义 SQLite 存储从文件格式 v1 逐版升级到 v4（v1 → v2 → v3 → v4）的升级契约：版本常量、升级幂等与数据保留、过新拒绝、损坏或权限不符时的失败关闭，以及管理表纳入保留与容量后的 imported 无正文不变式。

## Requirements

### Requirement: 版本常量、migration 执行时序与连续升级

系统 SHALL 把文件格式版本与 owned 家族表结构版本推进到 9，imported 家族表结构版本保持 3。

migration MUST 在单实例锁之后、开始监听之前执行，单事务、失败整体回滚、可重复打开；v1/v2/v3/v4/v5/v6/v7/v8 库 MUST 经对应段连续升级到 v9，序号只回填一次、中间版本不单独落盘。

连续两次打开后 `user_version`、两族版本键与库内每条 DDL 文本 MUST 逐字节相同；升级库与新建库的 owned 家族列清单 MUST 逐项相等。

#### Scenario: 连续两次打开 schema 文本不变

- **WHEN** 用同一个数据目录连续打开并关闭存储两次
- **THEN** 文件格式版本与 owned 家族表结构版本都为 9（imported 家族为 3），且库内每条表与索引的 SQL 文本逐字节相同

#### Scenario: 升级中途失败整体回滚

- **WHEN** 一次 v8 到 v9（或更早版本连续升级）在事务中途失败
- **THEN** 库保持升级前的版本与内容不变，重开后可再次尝试升级，且不产生半张已改结构的表

#### Scenario: 升级库与新建库的 owned 列清单相等

- **WHEN** 分别打开升级库与新建库并读取 owned 家族的列清单
- **THEN** 两边的列名、顺序、类型与 `NOT NULL`/默认值逐项相等（`export_ids_json`、恢复两列与目录归属列按各自追加顺序位于末尾）

#### Scenario: v2 到 v3 升级保留审计并扩展词表

- **WHEN** 一个含既有审计行的 v2 库升级到 v3（并继续升级到 v9）
- **THEN** 全部审计行与 `audit_id`/序列原样保留，升级后 `actor_kind = 'pairing_claimant'` 与 `action = 'node.authenticated'`/`'node.auth_failed'` 可写入，旧词表之外的取值仍被拒绝，`owned_command` 的 `actor_kind` CHECK 保持三值不变

### Requirement: v3/v4/v5 段升级的列追加与既有行保留

v5 相对 v4 的差异（`owned_session` 末尾新增 `agent_session_id` 与 `workspace_cwd` 两列）与既有行为保持不变：这两列只在会话创建与恢复流程中写入，既有会话行 MUST 保持 `NULL`，`NULL` MUST 被解释为「该会话没有可用于恢复的标识或目录」，MUST NOT 被推导或替换为别名解析结果。

v4 相对 v3 的差异（`owned_node` 末尾新增 `export_ids_json`）与既有行为保持不变：既有节点行由默认值得到空清单，即**不得默认放权**。

#### Scenario: v4 到 v5 升级保留既有会话行且新列为空

- **WHEN** 一个含 owned 会话行（v4 形状，没有恢复所需的两列）的库升级到 v5（并继续升级到 v6）
- **THEN** 每行其余列逐字节不变，`agent_session_id` 与 `workspace_cwd` 为 `NULL`，且这些会话不被任何路径当作可恢复会话

#### Scenario: v3 到 v4 升级给既有节点行写空清单

- **WHEN** 一个含已配对节点行（v3 形状，没有清单列）的库升级到 v4（并继续升级到 v6）
- **THEN** 每行的 `export_ids_json` 为 `'[]'`、其余列逐字节不变，节点仍可完成握手，但看不到任何 Export，直到本机重新确认配对

### Requirement: v1 数据保留与 Import 归属迁移

升级 SHALL 保留 `server_epoch`、会话 origin 与事件序号、幂等行、命令终态与全部既有审计，序号 MUST 不重新编号。v1 的单 Export Import 行 SHALL 迁移为一条 Import 管理行加一条关联行；没有可信来源的 grants MUST 不被凭空补齐或默认放权，这类 Import 保持不可用。

#### Scenario: 升级后重放与幂等仍一致

- **WHEN** 一个含会话、事件、cursor、幂等行与审计的 v1 库升级到 v2
- **THEN** 原有事件序号与 origin cursor 不变，重放结果与升级前一致，重复提交既有 requestId 仍命中幂等行

#### Scenario: 缺少可信来源的 Import 保持不可用

- **WHEN** 升级遇到一条无法从可信来源确定 grants 的 v1 Import 行
- **THEN** 该 Import 不被补齐权限、不承担授权、保持不可用状态等待本地重新授权

### Requirement: 过新版本拒绝打开

文件格式版本或任一表结构版本高于本二进制已知版本时，系统 MUST 拒绝打开并返回具名错误，MUST 不得降级写入或修改库内容。

#### Scenario: 过新库零写入

- **WHEN** 打开一个版本高于已知版本的数据库文件
- **THEN** 返回明确的版本过高错误，库内容与打开前逐字节相同

### Requirement: 损坏与权限不符时的失败关闭

存储 SHALL 在启动检查失败（完整性检查不通过）或正式模式权限判定为宽松时进入只读失败关闭：写路径 MUST 全部返回明确错误，只读查询 MUST 仍然可用。Windows 上无法读取 ACL 时 SHALL 记录一次结构化警告且不失败关闭，Unix 上仍按真实模式位失败关闭。

#### Scenario: 损坏库的写路径全部被拒

- **WHEN** 用一个损坏的数据库文件或完整性检查失败的状态打开存储
- **THEN** 所有管理与会话写路径返回损坏类错误且库不被修改，只读查询仍能返回既有数据

#### Scenario: 权限过宽在 Unix 上失败关闭

- **WHEN** 在 Unix 上数据目录或数据库文件对组或其他用户可写
- **THEN** 启动判定为宽松并进入只读失败关闭，写路径返回明确错误而不是继续写入

### Requirement: 管理表纳入保留与容量

总容量度量 SHALL 包含管理表（含身份、配对、Export/Import、本地配置与审计）的文本列与附件字节；空间不足时 MUST 拒绝新写入，MUST 不得删除活动信任、未到期审计或撤销记录来腾出空间。清理顺序 SHALL 保持既有约定的相对次序，且不得破坏承诺期内的重放。

#### Scenario: 超限时拒绝新写入而不是删信任

- **WHEN** 库已达到总容量上限且其中存在活动信任与撤回记录
- **THEN** 新的管理写入返回存储满错误，活动信任、撤销记录与未到期审计仍然存在

#### Scenario: 度量包含管理表分量

- **WHEN** 只写入管理状态而不写入任何会话或附件数据
- **THEN** 容量度量随管理表内容增长，清理与超限判定都基于该度量而不是恒为零

### Requirement: 恢复所需列的读写边界

存储层 SHALL 在 owned 会话的创建与恢复流程中读写 `agent_session_id` 与 `workspace_cwd`：`agent_session_id` 只在会话创建取得 ACP 会话标识之后写入，`workspace_cwd` 只在创建时解析出规范化目录时写入；两者一旦写入 MUST NOT 被恢复流程改写（恢复只读它们）。

#### Scenario: 恢复流程不覆写持久化取值

- **WHEN** 对一条已带有 `agent_session_id` 与 `workspace_cwd` 的会话执行恢复
- **THEN** 恢复成功或失败后，这两列的字节都与恢复前相同，不出现用别名、新解析路径或新会话标识覆盖已存取值

### Requirement: 会话目录归属列的写入与解释（目录标识）

存储层 SHALL 在 owned 会话创建流程中写入 `workspace_directory_id`：只在本次创建解析出目录标识时写入，且 MUST 写入该标识原文，MUST NOT 写入路径或由路径派生的其它文本。恢复流程 MUST NOT 读取或改写该列。该列为 `NULL` 时 MUST 被解释为「该会话没有目录归属」，MUST NOT 被任何读取路径按 `workspace_cwd` 反查目录表补齐，也 MUST NOT 用空串或占位值代替。

#### Scenario: 写入目录标识原文

- **WHEN** 一个会话创建请求携带已记录的目录标识并解析成功
- **THEN** 该会话行的 `workspace_directory_id` 与请求中的标识逐字节相同

#### Scenario: 恢复不改写目录归属

- **WHEN** 对一条已带 `workspace_directory_id` 的会话执行恢复
- **THEN** 恢复前后该列的字节相同，不出现用目录表或新创建结果覆盖已存取值

#### Scenario: NULL 不被路径反查补齐

- **WHEN** 一条会话行的 `workspace_directory_id` 为 `NULL`，而其 `workspace_cwd` 与某个已记录目录的规范化路径相同
- **THEN** 读取该会话得到的目录归属仍为「未分组」，不产生任何目录归属

### Requirement: v9 的目录表与目录归属列

v9 相对 v8 的差异是：新增 `owned_directory(directory_id PRIMARY KEY, canonical_path UNIQUE NOT NULL, created_at NOT NULL, last_used_at NOT NULL)`，并用 `ALTER TABLE ... RENAME COLUMN` 把 `owned_session.workspace_alias` 改名为 `workspace_directory_id`（不触发 12-step 表重建）。`owned_workspace` 表在 v9 删除。

迁移 MUST 从既有 `owned_workspace` 行与既有会话行的 `workspace_cwd` 生成目录行，使升级后不存在「会话引用了目录表里没有的 id」的行；目录标识由规范化路径派生，MUST 与运行期派生规则一致。

#### Scenario: v8 升级保留历史会话并归组

- **WHEN** 一个含已登记目录与既有会话行的 v8 库升级到 v9
- **THEN** 每条会话行的其余列逐字节不变，`workspace_directory_id` 等于由该会话原目录路径派生的标识

#### Scenario: 原先未分组的会话被归组

- **WHEN** 一条 v8 会话行的 `workspace_alias` 为 `NULL`，但 `workspace_cwd` 非空
- **THEN** 升级后该目录被记录，且该会话的 `workspace_directory_id` 非空

#### Scenario: 标识由规范化路径派生

- **WHEN** 两个既有目录的 `canonical_path` 只有大小写差异（或指向同一目录的链接写法）
- **THEN** 升级后它们合并为同一条目录行，且没有任何会话引用一个不存在的标识

### Requirement: v10 删除 Agent 与 Provider 管理表

`owned_schema_version` MUST 从 v9 推进到 v10，并在同一事务内删除 `owned_agent_profile` 与 `owned_provider_ref`；既有审计行 MUST 保留。迁移 MUST 在删除前记录 profile 行数（供启动日志 `daemon.agents_discarded`），失败时整体回滚且版本不前进。

#### Scenario: v9 升级到 v10

- **WHEN** 一个含 `owned_agent_profile` 行的 v9 库打开
- **THEN** 两张表被删除、审计行不变、版本变为 v10，且记录被删除的 profile 条数

#### Scenario: 升级失败回滚

- **WHEN** v10 迁移在中途失败
- **THEN** 两张表与版本保持 v9，不留下半升级状态

#### Scenario: 连续两次打开

- **WHEN** 已升级到 v10 的库再次打开
- **THEN** 不再执行迁移，schema 与行逐字节不变

### Requirement: v11 删除 Export/Import 与节点授权列

`owned_schema_version` MUST 从 v10 推进到 v11，并在同一事务内删除 `owned_export`、`imported_import`、`imported_import_export`，删除 `owned_node.grants_json`/`export_ids_json`，并重建 `imported_session`、`imported_delivery_index`、`imported_command_ref`（去掉 `export_id`）。迁移 MUST 按 `(owner_node_id, session_id)` 合并重复行并保留最新 cursor，`local_sequence` MUST NOT 回退；失败整体回滚。

#### Scenario: v10 升级到 v11

- **WHEN** 含 Export/Import 行与重复 imported 会话的 v10 库打开
- **THEN** 相关表/列被删除，重复行合并且 cursor 取最新，版本变为 v11

#### Scenario: 失败回滚

- **WHEN** v11 迁移中途失败
- **THEN** schema 与数据保持 v10

### Requirement: 会话三态升级保留历史并可重复打开

系统 SHALL 通过版本化迁移使当前会话摘要仅含三态，并在迁移及恢复完成后根据有效交互与未结束任务重新计算状态。旧 failed/closed 标签 MUST NOT 继续作为会话不可恢复的依据。旧任务结果、命令幂等结果、事件原文与序号 SHALL 保留；既有引用及序列不得损坏。迁移失败 SHALL 整体回滚，版本不前进；重复打开 MUST NOT 重复迁移或派发任务。

#### Scenario: 旧七态会话升级
- **WHEN** 旧库包含 idle/queued/running/waiting_input/waiting_permission/failed/closed 七类摘要
- **THEN** 升级与恢复后所有摘要均为三态，状态由仍有效的任务和交互决定，不能仅凭旧标签伪造任务结果

#### Scenario: 历史失败与关闭没有未完成工作
- **WHEN** 旧 failed 或 closed 会话没有未结束任务和有效交互
- **THEN** 当前摘要为空闲，原有失败任务、事件及可用历史仍保留，会话不再因旧标签被永久拒绝提交

#### Scenario: 遗留请求所属任务已经结束
- **WHEN** 旧库含仍标记待处理、但所属任务已确认结束的交互
- **THEN** 恢复后该交互失效而非被自动批准，不再导致会话待处理

#### Scenario: 非终态执行记录需要恢复
- **WHEN** 旧库存在崩溃留下的执行记录
- **THEN** 系统先执行停止确认与结果无法确认的既有恢复规则，再重算状态，MUST NOT 只把摘要改为空闲而丢弃该记录

#### Scenario: 迁移失败与再次打开
- **WHEN** 迁移中途失败，随后重新打开数据库
- **THEN** 首次失败不留下半升级数据；成功后再次打开不追加重复结果、不改变既有序号或重复派发

### Requirement: 旧历史重放与新状态合同分离

系统 SHALL 保留旧事件载荷的原文、摘要及序号。旧会话状态在新合同下重放时 SHALL 使用明确版本规则，MUST NOT 让旧七态覆盖当前三态权威摘要。旧 waiting 标签无法唯一确定待处理原因时 MUST NOT 编造请求；访问节点升级 MUST NOT 为转换状态缓存远程正文。

#### Scenario: 旧事件包含失败或关闭标签
- **WHEN** 升级后重放含旧状态标签的历史事件
- **THEN** 历史事实保持可读，客户端不会因此重新进入失败或关闭的会话主状态，ACP 原文不变

#### Scenario: 导入会话缺少最新权威摘要
- **WHEN** 访问节点只有旧交付索引且所属节点离线
- **THEN** 客户端按明确的旧摘要映射与过期提示展示，MUST NOT 声称已重新确认远程状态或读取远程正文

### Requirement: 升级后没有持久远程资源与进度

最新格式 SHALL 无可写入 Owner 资源、命令状态或交付进度的 imported 资源表、替代表或持久视图。封闭存储清单 SHALL 只允许 Access 连接与信任资料及节点自身权威数据。既有远程记录及其日志副本 SHALL 在提供服务前清理；格式与迁移版本 SHALL 前进，不复用旧格式声明新语义。

#### Scenario: 升级后检查完整白名单
- **WHEN** 含远程标题、会话状态、请求引用及 ACK 的旧库完成升级
- **THEN** 对应资源表及记录不存在，其他表和日志没有这些数据的替代副本，连接信任与本机日志保留

#### Scenario: 投递后重开无远程记录
- **WHEN** 在线投递远程正文和结构化事件后关闭并重新打开 Access
- **THEN** 没有可读的远程 eventId、来源位置、摘要或命令记录，必须重新连接 Owner 建立状态

### Requirement: 历史资源清理与本机数据保留分开验收

迁移 SHALL 按原资源归属区分本机权威记录和远程代理记录；本机事件、序号、幂等、连接信任、撤销与必要安全审计 MUST 保留。含远程资源标识、内容或进度的代理审计 MUST 清理，不能援引历史保留规则继续保存。归属不可安全确定时 SHALL 阻止迁移完成，不凭时间或名称猜测。

#### Scenario: 同库两种角色
- **WHEN** 一个库同时含本节点会话和其他 Owner 会话的代理记录
- **THEN** 升级后本节点事件与命令仍可恢复，其他 Owner 记录被清理，既有连接仍可重新认证

#### Scenario: 迁移重复及故障
- **WHEN** 迁移失败后再次启动，或成功后重复打开
- **THEN** 失败时不开始监听，重试可完成；成功后再次打开结构和数据不再变化

### Requirement: v19 移除准备式新建存储

文件格式版本与 owned 家族表结构版本 SHALL 从 v18 推进到 v19。v18 升级 SHALL 删除 `owned_first_submission` 及其索引，并清除未转正的准备附件暂存；正式会话、消息、事件、幂等记录、目录、审计和附件引用 MUST 保留。

#### Scenario: v18 正式数据保留

- **WHEN** 含正式会话、普通用户消息、事件、幂等记录和审计的 v18 库升级
- **THEN** 所有正式行及其原有序号保持不变，准备表和准备暂存不再存在

#### Scenario: 半提交准备数据被清理

- **WHEN** v18 库包含已持久首条但不完整的准备记录，或附件目录存在 `.preparation` 暂存
- **THEN** 升级删除准备记录与暂存，不把它们转成正式消息，也不删除已存在的正式附件

### Requirement: v19 升级原子且可重复打开

数据库结构迁移 SHALL 在单一事务中完成并在失败时整体回滚。成功后重复打开 MUST 不再改变 schema 或数据；v19 二进制打开更高版本 MUST 继续拒绝并零写入。

#### Scenario: 升级失败回滚

- **WHEN** 删除准备表、索引或写入 v19 版本键时发生错误
- **THEN** 数据库保持 v18 结构与数据，Daemon 不开始监听

#### Scenario: 连续两次打开

- **WHEN** v18 库成功升级后再次打开
- **THEN** `user_version`、owned schema 版本和全部表及索引 SQL 与第一次完成后逐字节相同

### Requirement: 空会话删除原子移除权威历史

空会话删除 SHALL 在同一事务中把正式会话转为最小删除墓碑，并移除消息、会话内容事件、任务和交互记录、目录归属及仅被该会话引用的 ACP Remote 附件关联。墓碑 MUST 对所有会话读取、快照、恢复、激活和普通命令不可见，只为删除事实、命令幂等与命令终态保留外键锚点。

事务失败 MUST 恢复删除前的全部记录与引用。

#### Scenario: 消息关联清理

- **WHEN** 删除一个仍有孤立准备关联或普通输入关联的空会话
- **THEN** 该会话的关联记录被一并清除，其他会话共享的附件内容不受影响

#### Scenario: 删除回滚

- **WHEN** 事务在删除任一从属行后失败
- **THEN** 会话、历史、附件引用和序号全部回到删除前状态，容量占位不被释放

#### Scenario: 墓碑不能重新成为会话

- **WHEN** 删除已提交后客户端读取、列出、恢复或发送普通会话命令
- **THEN** 服务端把该标识视为不存在，墓碑不出现在任何产品投影中
