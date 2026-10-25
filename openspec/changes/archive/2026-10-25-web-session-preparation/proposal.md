# Proposal

## Why

真实 Codex 复现表明：未发送过消息的会话可以在原进程内使用，却可能没有可跨进程恢复的文件；现行 Web 新建流程已经将这种空会话登记为长期会话，服务重启后只能报告恢复失败。用户确认将 Web 新建改为“临时准备 → 首条消息持久受理 → 派发”，同时保留发送前的真实模式/配置调整、刷新恢复和不变的会话地址。

## What Changes

- 新增本机 Web/PWA 会话准备流程：由 Owner 后端提前分配最终会话 ID，可建立临时 Agent 会话，但 ACP Remote 不保存准备会话、命令收据、正文或准备进度；目录与其他设备看不到尚未提交的会话。
- 准备页直接使用 `/session/<后端分配的ID>`。首条保存成功、失败、响应丢失以及后续派发失败均保持该地址与页面；不得使用浏览器临时 ID 再替换地址。
- 当前标签页保存用于刷新恢复的文字、模式、配置、路径引用和附件；标签页相互隔离。主动离开清除；更换 Agent 或工作目录清空输入、设置和旧准备，后端重新分配 ID。
- 允许后台回收、断线及服务重启后沿用原 ID 重新准备，恢复并验证所选配置；重建只允许用于尚未提交的准备，不能变成旧正式会话恢复失败后的新建兜底。
- 首次发送先原子保存会话、完整用户消息、附件归属、稳定请求记录及派发阶段，再向 Agent 派发；保存不等于已发送。保存后会话进入目录，后续失败仍保留；能够证明未派发时才可显式重试，结果不确定不自动重发。
- 准备阶段允许选择真实 Agent 模式与配置，默认沿用 Agent 默认值；不得仅凭缓存生成可选项，也不得在重建后静默将不兼容选择改回默认值。
- 附件采用用户选定的方案 A：复用现有附件连接和分块帧，在开始上传/清单开始帧中新增准备目标分支；成功路径只上传一次，转正只认领已有字节与元数据，不复制整份文件。附件未完整时不提前保存正式会话。
- 准备恢复增加明确的 `committed` 结果：首条已经保存时，同页读取原正式会话与请求，不再建立准备。删除正式会话及其首条关联记录不在本轮范围内，因此不新增 `consumed` 状态或删除墓碑表。
- 陌生标签页或设备访问尚未保存的 URL，以及用户输入不存在的会话 URL，在认证与权威不存在确认后替换到目录页 `/`；断网、快照未就绪及服务错误不冒充不存在。
- **BREAKING（协商后的 Web 新建体验）**：新 Web 入口不再点击即持久创建会话。新增 feature 与明确降级；未协商时禁用新入口并说明，不能退回已知有问题的旧空会话创建流程。原 `session.create` 的 Sync/Node Link 合同、Zed 流程与旧会话保持不变。

### 用户要求与非目标

需求来源为本轮 Q1–Q20 访谈及 [设计讨论](../../../docs/SESSION_LIFECYCLE_DISCUSSION.md)；以最后确认的选择为准：Q1 的“不持久化”指 ACP Remote 正式会话记录，Q13 允许临时真实 Agent，Q14 要求后端分配且 URL 不变，Q18/Q19 要求更换目录或 Agent 清空并换 ID，Q20 要求附件随刷新恢复。

不迁移、修复或重新解释旧会话；不修改 Zed 与 Node Link 的创建行为，不扩展 imported Agent 创建，不保存 Access 远程内容；不通过隐藏 prompt、历史重发或新建替代已提交的 Agent 身份来伪造恢复。现有 `session-activation-lifecycle` 的 owned 激活/容量/回收实现是前置基线，不在本变更中重做；本变更仅扩展其准备资源与首次提交边界。

成功判据：真实 Codex 空准备跨进程重建后仍可首次发送；第一次提交保存后的故障不丢正文、不重复派发；URL 全程不变；附件/配置刷新恢复、标签页隔离与主动离开清除均经真实页面断言；旧协议与旧会话测试不回归。

## Capabilities

### New Capabilities

- `web-session-preparation`：后端预分配身份、临时 Agent 准备、认证后重建、浏览器准备草稿、首条持久受理与派发收口的完整行为边界。

### Modified Capabilities

- `pwa-web-client`：目录创建入口转入准备；同页同 URL 首次保存、准备模式/配置、准备草稿刷新恢复与不存在地址跳转。
- `conversation-attachments`：首条提交前的浏览器附件草稿与点击发送后的准备上传，保持完整提交、归属校验和资源保留语义。

## Impact

现状接缝：`clients/app/src/features/client-store.ts` 的 `submitCreateSession` 当前立即派发 `session.create`；`AgentHost::create` 的 `session/new` 响应才带真实模式/配置；`Broker::submit_prompt_bound` 受理只保存任务与命令、正文在内存排队；现有附件表引用已存在的 owned 会话。必须增加新的首条路径，不能在旧 `session.create` 或 `session.resume` 上偷改含义。

计划影响 core 模型/端口与 broker、owned SQLite 事务/升级、agent-host 临时绑定、identity-auth 的专用准备证明、Sync 新 feature/控制帧/首次提交命令及 Rust/TypeScript 共享资产、PWA feature/platform/page 与对应测试。核心仍不依赖协议、平台和运行时；app 只装配。计划使用已审查的现有 P-256/长度前缀 transcript 实现，不新增第三方加密算法或云服务。

实施时须同步 `INITIAL_DESIGN`、`FRONTEND_DESIGN`、`SYNC_PROTOCOL`、`CORE_PORTS_AND_STORAGE`、`MODULE_ARCHITECTURE`、`SECURITY_DESIGN`、`IDENTITY_AND_AUTH_CONTRACT`、必要的 `CONFIG_REFERENCE`/ACP 矩阵说明以及 schemas/fixtures/commands/features/errors/transcripts。ADR-0014 记录选型原因；本轮只产规划文档，不提前修改这些机器合同或实现。
