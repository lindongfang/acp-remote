# ADR-0004：本地管理通道

- 状态：Accepted
- 日期：2026-09-18
- 影响范围：CLI 与 Daemon 之间的高权限操作、本地 IPC 边界、配对与 Export 管理

## 背景

`SECURITY_DESIGN.md` §8 把 Provider/MCP 配置、原始 workspace 路径、Node/Device 管理、Export 管理定义为 Owner Node 本地管理能力，并在 §20 把"Windows Named Pipe、Unix socket 或其他本地管理 IPC 的最终方案"列为编码前必须收口的选择。

同时，Node Link 首个纵向切片的第一步就是节点配对：`acp-remote node pair`、`acp-remote export create`、`device pair`、`node|device list|revoke` 都必须在 Daemon 运行时执行。没有本地通道，这些命令只能靠停掉 Daemon 独占数据库来完成，既不支持撤销类热操作，也和 `SECURITY_DESIGN.md` §9.5"撤销必须提交持久状态后立即关闭连接"冲突。

## 决策

1. **进程内调用为第一优先**：CLI 自己负责的命令（`daemon start|stop|status`、`doctor`）在 CLI 进程内直接完成，不经过任何 IPC。
2. **运行中 Daemon 的管理操作走平台本地 IPC**：Windows 使用 Named Pipe（默认名 `\\.\pipe\acp-remote-<userSidHash>` 之类的用户专属名字），Unix 使用 Unix domain socket（目录 `0700`、socket 文件 `0600`，位于当前 OS 用户的受保护配置目录）。
3. **两端共用同一组管理 use case**：IPC 上传递的是与 `core::use_cases` 同一套 `DeviceManagement`、`ExportManagement`、`RemoteCatalogQueries` 请求，不新增一套"管理专用"业务语义；wire 编码由本地管理模块自己拥有，不复用 Sync/Node Link DTO。
4. **访问控制只按 OS 用户**：IPC endpoint 只允许启动 Daemon 的 OS 用户连接（Named Pipe 的 SDDL / socket 文件权限）；调用方身份就是 Daemon 进程的 OS 用户，不引入本地认证 token、不复用设备身份。
5. **`acp-stdio` 走同一通道，但不内嵌核心**：ACP facade 常驻 Daemon（会话状态、幂等与事件提交必须发生在拥有该会话的进程里），`acp-remote acp-stdio` 只是 stdin/stdout 与本地通道之间的字节泵；Daemon 未运行时以明确错误退出，不得自行打开数据库或启动第二套核心。
6. **本地通道有两类载荷**：管理请求/响应，以及长期双向的 ACP 会话流；两者各有自己的编码与生命周期规则，都不复用 Sync 或 Node Link 的 DTO。
7. **不使用 loopback HTTP 管理面**：`SECURITY_DESIGN.md` §8 的优先级 3 保持未实现状态，除非将来为它单独设计本地认证、Origin/CSRF 与权限模型并新增 ADR。
8. **失败关闭**：IPC endpoint 创建失败（权限不符、路径被占用、socket 被替换为符号链接）时，正式模式拒绝启动而不是降级为"无管理通道"或临时暴露无认证 endpoint。

## 结果

- 配对、撤销、Export 管理可以在 Daemon 运行期间完成，且不需要新增长期凭据。
- 本地通道不进入远程攻击面：它不监听 TCP，不受 Origin、Host、scope 或设备身份影响。
- CLI 与 Daemon 之间需要一条本地的请求/响应编码与版本标识；它与 Sync、Node Link 是三条互不复用的边界，改动其一不影响另外两条。
- `local.*` 能力（`SECURITY_DESIGN.md` §10.3）的授权判定落在"连接方是启动 Daemon 的同一 OS 用户"这一事实上，不需要新增 scope。
- 跨平台差异被限制在一个 adapter 内（Named Pipe / Unix socket），不影响 `core` 或在其他平台上复用同一组 use case。
