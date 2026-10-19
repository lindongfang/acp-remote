# Design

## Context

动机与范围见 [proposal.md](proposal.md)。本变更以用户已确认的 [讨论记录](../../../docs/file-panel-discussion.md) 为需求来源。

已核实：`FileChangesPanel.tsx` 当前消费逐条事件；`buildFileChangeViews` 尚未按文件归并。`workspace.browse` / `DirectoryBrowser` 只列子目录，禁止返回文件及内容，且 Windows 默认过滤隐藏/系统属性；不能修改它来承载这次全部文件视图。`UseCases`、`server::sync::command` 与前端 `client-store` 已有查询接线，配置画像中关于 Sync/前端尚未实现的描述已过时，以实际代码为准。

已有 `conversation-page-prototype-parity` 尚未归档，且大量 UI/wire 在途修改存在；本变更依赖其最终检视结果。实施前固定基线，在归档时先同步该变更，再同步本变更；本次不修改旧变更、主规范或产品代码。

## Goals / Non-Goals

**Goals:** 在既有 Sync query 路径内完成受会话根约束的文件读取，不新增 HTTP 内容端点或 bearer 凭据；客户端仅维护互斥的对话/文件查看状态，事件与 Agent 生命周期继续独立运作。

**Non-Goals:** 不复用或扩张 Node Link，不改 ACP 文件能力声明；不改现有目录浏览口径；不增加设备 scope、数据库表、配置开关、Git 写操作或持久正文缓存。

## Decisions

### D1：新增独立查询，不改变目录选择器合同

新增协商 feature `core.session-files.v1`（仅 Sync），四个命令均为 query，要求 body.sessionId，禁止 expectedVersion。命令登记只加入 Sync 支持面，不加入 Node Link 或本地管理方法集。旧客户端未协商时保持原有行为，新客户端对旧主机保留会话变更并说明其他来源不可用。feature_required 在所有 I/O 之前判定。查询重复沿用连接期去重；后续主动刷新使用新 requestId，不能复用旧结果。

拟定的 wire 合同（实现时同时写入登记、schema、DTO 和 fixture）：

| 命令 | payload | completed result |
| --- | --- | --- |
| `session.files.list` | `{ path, hideIgnored, after? }`；根用空字符串 | `{ path, gitState, entries, next? }`；条目含根内路径、名称、类型、可打开性与受限原因 |
| `session.files.read` | `{ path, purpose, offset, version? }`；purpose 为 preview/download | `{ path, kind, sizeBytes, version, offset, dataBase64, eof, previewTruncated }` |
| `session.git.status` | `{ after? }` | `{ gitState, entries, next? }`；每项含路径、状态、可用的旧路径及冲突/二进制标识 |
| `session.git.diff` | `{ path }` | `{ path, base, kind, diffLines?, unavailable?, truncated }`；base 为 head/empty，kind 区分文本、二进制、冲突 |

offset、sizeBytes 等字节数以规范十进制字符串编码，版本为不透明连接期标识；客户端不解析其结构。after/next 是只用于翻页的短期位置标识，不授予读取权限；分页变化须明确重试，不能声称提供目录事务快照。枚举按目录优先、名称 Unicode 码点、路径打破同名并列的确定顺序；过滤在分页前完成。所有字符串/数组有界且封闭对象拒绝未知键。

替代方案：扩张 `workspace.browse` 会破坏既有只列子目录合同；无认证下载 URL 会绕过逐连接认证；因此均不采用。

### D2：core 拥有规则，原生适配器负责 I/O

在 `core::model` 增加根内相对路径、枚举页、字节块与 Git 只读视图，`core::ports` 增加会话文件读取端口；`UseCases` 负责设备执行前提、查找 owned 会话实际目录、来源判定、限制与错误语义。根不能从客户端路径、目录名或 workspace 登记是否存在推断。

原生实现放在 `app` 的专属 `session_files` 适配模块，由组合根注入，类似现有 clock 等端口实现；它只处理文件句柄、目录枚举、Git 子进程和取消，不承载授权、会话选择或来源业务规则。不新增 crate，不将进程、Tokio、Git 库或 wire DTO 放入 core。现有 `DirectoryBrowser` 不变。`server::sync` 只调用用例与转换 DTO，不直接读取磁盘或调用其他入站适配器。

Git 通过已有运行时的受管理子进程调用本机 Git 可执行文件，不新增 Rust 依赖。Git 缺失只影响 Git 视图和忽略过滤，全部文件仍可浏览并明确提示过滤不可用。部署文档说明这项运行时能力，不要求终端用户安装 Rust。

### D3：路径边界必须落实到实际打开对象

请求仅接收根内相对路径；校验拒绝绝对路径、盘符、UNC、父引用、NUL、Windows alternate data stream 等不安全形状。隐藏 `.git` 目录及 worktree 的 `.git` 管理文件，直接访问也拒绝。名称含非 UTF-8 字节或不能安全表达时，返回不可打开/不完整说明，不替换成有歧义的路径。

只规范化后做字符串前缀比较不够：适配器必须以实际打开句柄验证最终对象位于规范根内，并验证对象类型。Windows 覆盖 junction、reparse point、盘符大小写与 verbatim 路径；Unix 覆盖 symlink 与根句柄相对打开。无法安全证明时明确拒绝，不以猜测放行；根外链接仍列名称但不泄漏目标绝对路径。内部链接允许逐层浏览，遍历保持根约束并检测循环。预览、Git 内容和下载都复用同一校验器，不能存在只在 UI 检查的路径。

Git 仓库可位于会话根的父目录，但只有 Git 元数据查询能定位仓库；任何下发路径与内容必须投影回会话根。Git status 的越界改名按根内删除/新增表达，不下发根外旧路径。

### D4：Git 结果与会话事件严格分源

Git status 使用可保真分隔的机器格式和根内过滤，禁止 shell 拼接；固定参数并使用路径分隔符和 literal pathspec，防止文件名被当作 Git 选项。禁用 external diff、textconv、hooks、fsmonitor、pager 与可触发仓库命令的配置，关闭可选索引写入；不调用 checkout、clean、add 或 reset。

差异直接比较 HEAD 与当前工作树（新仓库空基准）；已暂存后又撤销成 HEAD 的文件仍可在状态中出现，但详情如实显示相对 HEAD 无净差异，不把暂存/未暂存补丁拼接。未跟踪文件按新增；冲突保留冲突标识；二进制不伪造文本。rename 仅在安全确认时显示旧路径，否则保持新增/删除原意。子模块仅显示状态，不递归越过会话根查询其内容。

会话变更仍消费已持久化 `file.changed`，按可靠的根内完整相对路径归并，按 sequence 排列每次记录，保留 eventId 去重与诊断入口。根外事件仅有 basename 时不能按同名合并；按事件保留并说明不可读取目录外文件。只展示已加载历史，有更多历史时沿用加载入口并说明范围。列表计数改为文件数，单次修改统计在详情保留，不显示误导性的累计净差异。

### D5：分块传输、资源预算与内容一致性

不增加 HTTP 路由：预览与下载都复用已认证 Sync query。固定 v1 初始预算：每页最多 128 项、相对路径最多 4096 字符；单块原始字节最多 32 KiB（base64 后仍远小于 1 MiB 消息预算）；文本预览最多 256 KiB；栅格图片预览最多 4 MiB；一次下载最多 64 MiB。Git 文本差异沿用 400 行/64 KiB/单行 4096 字符限制，超限明确 truncated 或 unavailable；整体仍按编码后实际字节检查 `maxMessageBytes`。

首次读取创建连接绑定、内存有界的只读内容快照，后续 version+offset 从同一快照取块；快照建立前后检查文件身份与变化，变化则拒绝重试。后续每块发出前重新检查源文件身份与变更标识，发现变化即废弃快照并明确要求重新读取，不续传旧版本。快照上限为下载预算，仅保留当前操作；每连接最多一个，每节点最多两个，空闲 60 秒释放，连接断开/取消/撤销立即释放，不写临时正文文件。对预览只取得该预览预算内容，明确 original size 与截断标记；超下载预算明确 `resource.result_too_large`，不能悄悄只下载前一段。快照版本绑定连接、设备、会话与相对路径，不能跨连接当 bearer 使用。

客户端通过平台下载端口流式保存；浏览器不支持流式落盘时允许有明确 64 MiB 上限的 Blob 回退，取消即释放，下载完成撤销对象 URL。数据块调度遵守既有设备命令族限流并处理 retryAfterMs，不为下载提高通用速率。每设备只保留一个主动读取与一个列表刷新；Git 每节点最多两个任务、单次 10 秒，超时/断连取消并清理整棵进程树，Windows 验证 Job Object。分页、字节读取和 diff 分族登记服务端限流覆盖，不能漏入无限制路径。

替代方案：一次内嵌完整文件会突破消息上限；只靠 mtime+size 拼接块不能保证同版本，因此采用有上限的内存快照。超大文件本期明确不支持完整下载，不暗示无限容量。

### D6：互斥查看状态与交互恢复

客户端领域数据、查询 DTO 和 view model 分开。feature 状态持有 `conversation | file` 互斥内容态；file 内区分 loading/ready/error、来源、相对路径、请求代次与返回上下文。右栏持有来源、展开目录、过滤、选中与滚动位置。首次会话默认 session；来源切换不清理其他来源已完成列表。

宽屏点击替换中间正文（时间线与输入区暂不呈现），会话头和必要状态仍可见，右上角关闭可键盘触发，关闭后焦点回到触发文件行。输入草稿、时间线滚动锚点、展开覆盖与事件消费由上层保留，不因组件隐藏销毁；不强制滚到底。另点文件直接替换，迟到响应通过会话+来源+路径+请求代次丢弃；关闭或切换会话取消读取、释放资源，不取消 Agent。

窄屏关闭返回文件列表原位置；之后切回对话仍恢复原对话位置与草稿。窗口宽度改变保留当前选中文件与源，不丢内容。中间错误页仍能关闭。时间线文件联动只切到会话变更并选中，不自动遮住聊天。

刷新采用进入视图、手动操作与文件事件三种触发，突发事件合并；查看期间文件改变先显示过时提示并允许重新加载，不擅自用不同版本覆盖正在阅读/下载内容。无磁盘后台 watcher，无正文持久缓存。

会话变更手动刷新复用协商后的 `session.read` + `include: ["history"]`，客户端以独立的最新历史刷新意图处理返回页，仅按 eventId 合并，不执行「首次读取」的消息页替换，也不执行「向前翻页」的不得重叠断言。保留已加载的历史与翻页边界；首次取得历史时使用返回的范围标记。在途读取不重复派发；UI 显示刷新中、失败或不可用原因。不新增 wire 字段、查询方法或 Agent 副作用。

### D7：内容与错误呈现

文本按纯文本/代码显示，UTF-8 无法解码时按二进制处理；不注入 HTML/SVG，不加载其中引用的网络资源。图片仅 PNG/JPEG/GIF/WebP，校验实际格式与解码尺寸（最多 16 百万像素）；静态资源 CSP 如需允许本机 Blob 图片，仅允许 `img-src blob:`，不放宽 script-src，不允许远端 URL。下载文件名只取安全 basename。

错误复用已有码：非法路径 `protocol.schema_invalid`、会话缺失 `session.not_found`、来源不支持 `capability.unsupported_by_broker`、文件 I/O/安全打开失败 `internal.unavailable`、整体过大 `resource.result_too_large`、限流 `resource.rate_limited`。非仓库/二进制/冲突是成功结果的显式状态；Git 缺失/超时是不支持或不可用原因，不回空列表。读取快照过期返回明确不可用并要求重开，不挪用会话 version conflict。文件内容、版本令牌、Git stderr 原文不得写日志或协议错误。

### D8：文档和合同随所属工作包维护

Sync 字段、限额与兼容策略唯一落 `SYNC_PROTOCOL`，core 端口与内存生命周期落 `CORE_PORTS_AND_STORAGE` §3/§5（§7 声明不新增持久字段），模块职责落 `MODULE_ARCHITECTURE` §4.1/§4.7，原生适配归属落组合根职责；安全内容落 `SECURITY_DESIGN`，产品范围落 `INITIAL_DESIGN`，交互落 `FRONTEND_DESIGN`。新 feature 在 Sync 登记，不增加 Node Link feature；检查两份协议 feature 表与机器登记各自一致。签名 transcript 仅自然承载协商后的 features，不新增域/tag；共享向量按实际影响更新，不改 ACP 矩阵或本地管理方法。

### D9：文件树视觉参照

按用户提供的目录树/变更树截图，三来源使用无卡片边框的单行列表、文件夹/文件类型字形、目录引导线、文件名省略与右侧标记。已知单分支目录压缩显示为 `a/b`，不丢完整路径；全部文件不为压缩展示额外读磁盘，只使用成功完整加载的目录页。会话文件打开中栏后不再同时展开右栏详情，未提供中栏打开能力时保留原内联路径。Git 没有下发行数，不补造统计；多次会话修改不累计。样式与截图证据见 `docs/evidence/file-tree-visual.md`。

## Risks / Trade-offs

- [Windows 链接替换可能绕过路径检查] → WP2 必须有真实句柄、junction 与替换竞态证据，单纯 canonicalize 测试不构成通过。
- [本机 Git 缺失或危险配置] → 明确降级，仅固定只读参数，恶意仓库配置测试验证不执行外部命令。
- [下载内存与限流影响吞吐] → 64 MiB 操作预算、节点并发上限、可取消与速率退避；大于预算明确拒绝，后续可独立设计大附件协议。
- [现有 file.changed 缺乏完整文件身份或历史未加载] → 不猜测归并、不新增原文解析，保留按事件降级并标注历史范围。
- [旧变更未归档/在途改动被覆盖] → 固定基线、串行合入、归档先旧后新，主检出只接收已检视 patch。

## Migration Plan

1. 先固定并检视 `conversation-page-prototype-parity` 的最终基线；归档同步顺序为旧变更在先、本变更在后。
2. 先落合同与后端，再落客户端；只有实现完整时才宣告 feature，新客户端在未协商时显式降级。
3. 无 DDL 或正文迁移，不新增配置与 HTTP 路由。部署将 Git 查询依赖说明纳入运行文档。
4. 回滚可停止宣告 feature 或回滚客户端；保留原有会话事件、已提交命令与正文不变。断开连接释放所有内容快照，不能靠数据库清理完成回滚。
