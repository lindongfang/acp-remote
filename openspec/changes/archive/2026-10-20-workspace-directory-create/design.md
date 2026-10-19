# Design

## Context

动机和原始要求见 `proposal.md`。已只读核对当前代码：`ClientStore.createFolderInBrowse` 调 `buildCreateFolderPath` 后将选择器关闭，写入 `selection.willCreate = true`，不发命令。真正的 `std::fs::create_dir_all` 位于 `core::use_cases::create_client_workspace`，仅由 `session.create` 的 `WorkspaceSource::PathWithCreate` 触发。`workspace.browse` 是只读 query，ADR-0009 明确否决独立建目录 mutation。现有协议没有立即建目录的入口，不能仅改按钮文案实现需求。

`SessionStore::commit(OwnedCommit)` 已支持 `session = None` 的全局事件、请求受理及命令终态；现有 `session.create` 受理/结算路径有命令专用限制，不能把建目录伪装成创建会话。当前工作区还有本轮用户授权的未提交修改，实施必须保留它们。

## Goals / Non-Goals

**Goals:**

- 每次「新建」得到主机的真实持久结果；逐层新建与最后选择分离。
- 复用连接认证、命令状态、持久幂等与无会话全局事件，命令数据不携带可广播的路径。
- 不新增 crate/依赖，不破坏 core、wire、SQLite 和前端领域状态的边界。

**Non-Goals:**

- 不提供删除、移动、重命名或通用远程文件管理；不放宽 Node Link 路径边界。
- 不新增设备 scope，不修改签名或认证协议，不在 `workspace.browse` 内加入写操作。
- 不支持在一个名字输入框里填写 `a/b/c`；多级通过「新建一层 → 进入 → 再新建」完成。

## Decisions

### 1. 新增显式 mutation 和协商项

拟新增 `workspace.mkdir` / `core.workspace-mkdir.v1`，仅 Sync，载荷 `{ parentPath, name }`；成功结果 `{ created: true }`。`parentPath` 是当前浏览层的服务端回显值，`name` 是单组件名字，长度分别复用浏览路径 4096 字符与目录名 255 字符上限；最终目标路径仍须不超过 4096 字符。父层先做绝对路径/父引用形状校验，再规范化；验证单组件名字后只创建这一层，不补建消失的父层。名字 `.`/`..`、分隔符、冒号、NUL 及平台非法名字在副作用前拒绝。

新 ADR 提议取代 ADR-0009 中「不需要先建目录可观察步骤」的假设。保留 `session.create(createIfMissing)` 对旧客户端的原有行为，不移除既有 fixture。未协商新 feature 时 UI 明确显示不支持独立新建，不发送必然失败的命令。

不把它做成 query：文件系统写入需要稳定请求标识、持久受理、可重放终态与崩溃恢复。也不借用 `session.create` 后再丢弃会话：那会启动不需要的 Agent 并产生错误的会话副作用。

### 2. core 拥有副作用与受理顺序

core 增加窄的 `DirectoryCreator` 出站端口（只创建既有父层下的一层目录），默认实现与现有本机目录实现放在同一语义边界；`DirectoryBrowser` 继续只读。用例负责授权/撤销判定、形状校验、受理去重和结果分类，server 只映射 wire 与 core。

建目录用例通过现有 `SessionStore` 和 `OwnedCommit(session=None)` 单独实现受理/结算，不调用命令专用的 `settle_session_create`，也不造假会话行：

1. 查相同主体/requestId 的指纹与记录；冲突拒绝，已受理/已终态返回原状态。
2. 持久化受理后才能调用 `DirectoryCreator`。实际执行任务必须由 Sync 命令执行所有者持有并纳入关闭，不留下 detached task。
3. 成功/失败的无会话 `command.*` 事件与终态记录单次事务落账，然后投递；结果仅 `{ created: true }`，错误不回显路径或名字。
4. 重启发现未终结受理时复用现有 `unsettled_commands` 恢复为 `uncertain`，不自动重做或凭目录存在推断原命令成功。

SQLite 的接受记录、全局事件与无会话终态能力已有基础；实施需扩展命令归属校验并断言不写 `owned_session`/`owned_workspace`。本方案不计划新增表；如现有接受/结算校验不能表达该无会话命令，先修订本 design 与合同，不绕过持久化。

### 3. 成功后浏览，获取真实条目

成功结果不新增路径可见面。收到确认的成功终态后重新 `workspace.browse` 当前父层，子目录路径只从该结果取得。新建成功但浏览刷新失败时分开显示两件事实，允许重新刷新，不重新发 mkdir。目录被截断导致新条目不可见时显示「目录已创建，列表被截断」，不得从客户端候选路径伪造服务端条目。

成功后从父层真实条目取得新目录路径并自动进入，再输入下一层名字；同一次创建最多自动进入一次，不自动确认选择或关闭选择器。刷新失败或截断未找到条目时保留父层，不拼接路径假装进入。最后的「选择此文件夹」从当前真实层创建 `DirectorySelection(kind=path, willCreate=false)`，会话提交不再为该路径补 `createIfMissing`。

### 4. 前端状态有单一来源

store 的单份目录浏览状态增加独立建目录请求关联，命令生命周期继续使用命令状态机；输入框折叠和名字仍是呈现状态。UI 显示「新建 / 新建中」，失败保留名字，只有已确认成功才清空；`accepted` 不等同完成，不能刷新出假目录。关闭弹层只解除 UI 观察，不能取消已经受理的副作用或丢弃不确定记录。

同名目标在一个新的 requestId 下明确失败，不覆盖既有对象；同一请求的重放继续返回原结果。错误沿用现有参数/不可用/幂等冲突词表，不新增包含宿主路径的错误详情。

## Risks / Trade-offs

- 目录创建与 SQLite 终态不可能成为一个原子事务 → 明确 `uncertain` 窗口、持久受理、稳定 requestId，不通过自动重试掩盖。
- 关闭会话创建后磁盘已有目录 → UI 在点击前说明「立即在电脑上创建，取消会话不会删除」，不做危险的自动清理。
- 父层在浏览后被删除或变更 → 重新规范化与文件系统创建结果是权威；失败不回退。延续既有可信主机模型，不宣称对恶意本机文件系统具备新沙箱保证。
- 隐藏/系统目录、大小写排序、128 条截断 → 保留现有浏览行为；新建成功和刷新可见性分开呈现。
- 服务端旧版本 → 独立 feature 协商，旧功能保持可用，不用延迟创建伪装成功。

## Migration Plan

批准后先登记命令/feature/schema/fixture 与新 ADR，再实现 core/storage、server/app 和前端。无破坏式数据库迁移；现有命令、设备信任和会话数据保持兼容。发布时更新 Daemon 与其托管的前端，已打开页面刷新以获得新 feature。

回滚时新前端连接旧服务端将禁止独立新建，已有目录不删除；回滚前必须完成在途 mkdir 或保留已持久化的不确定命令，不能让旧版恢复器把未识别命令当成功。相关路径与关闭/重启窗口纳入真实 Daemon E2E。

## Approval Boundary

本方案待用户确认的是新增独立文件系统 mutation 及取代 ADR-0009 的决定。实施方案中的协议与产品选择已明确，不留给实现阶段自行变更；以下计划与任务仅描述批准后的执行安排。
