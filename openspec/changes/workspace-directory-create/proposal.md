# Proposal

## Why

用户原始要求：「新建文件夹 电脑上并没有新建文件夹。取消新建并选择，保留新建，允许新建多级目录后，选择。」当前 `ClientStore.createFolderInBrowse` 只构造 `willCreate: true` 的路径并关闭选择器，实际创建延迟到 `session.create`；这使「新建」的可见行为与用户预期不符，也无法继续进入新目录创建下一层。

## What Changes

- 「新建并选择」改为「新建」：确认成功后在主机磁盘上已有目录，选择器继续打开，不自动选择工作目录。
- 用户可以进入新目录继续新建下一层，例如依次建立 `project`、`src`、`api`，最后单独点「选择此文件夹」。新建一次只接受一个目录名，不接受手写任意路径。
- 新增 Sync-only mutation `workspace.mkdir`，受新 feature `core.workspace-mkdir.v1` 门控；先持久化受理，再执行文件系统操作，再持久化命令终态。它不创建会话、不启动 Agent，也不登记 workspace。
- 新建中防重复提交，失败保留输入与当前层；不确定结果显式保留，不静默重发。选择器关闭或会话创建取消不会删除已经成功创建的目录。
- 以新 ADR 明确取代 ADR-0009 对「独立建目录 mutation」的否决，保持只读 `workspace.browse` 和现有 `session.create(createIfMissing)` 兼容。

## Capabilities

### New Capabilities

- `sync-workspace-mkdir`: 已认证设备独立创建主机目录的命令、输入校验、受理/终态、重放与失败边界。

### Modified Capabilities

- `pwa-web-client`: 目录选择器由「新建并选择候选路径」改为「实际新建、继续逐层浏览、最后单独选择」。

## Impact

- 待确认的协议与决策变更：新增命令与 feature，不扩展 Node Link 的任意 Owner 路径访问，不新增设备 scope，不改变现有身份认证。旧服务端不支持新 feature 时，客户端明确禁用独立新建，保留浏览和选择现有目录。
- 代码范围：`core` 的用例/领域输入与持久受理、`storage-sqlite` 的命令受理/终态路径、`sync-protocol`、`server::sync`、`app` 的启动恢复与接线、前端 store/模型/选择器及测试。不得把文件系统副作用塞进 React 组件或只读查询。
- 合同资产：commands/features 机器登记、Sync schemas/fixtures、Rust/TypeScript 共用 fixture manifest、Sync 与安全/存储/模块/前端权威文档、新 ADR；不新增密码学 transcript，不引入依赖，不要求新建账号或云服务。
- 验收：点击「新建」后可在主机磁盘看到目录；可连续建立三级再选择；关闭选择器或取消新建会话不删目录；无会话/Agent 副作用；断线、重复请求、崩溃恢复、权限不足与非法名字均有可区分结果。

本变更目前仅为待确认的规划，不声明代码或新接口已经存在。
