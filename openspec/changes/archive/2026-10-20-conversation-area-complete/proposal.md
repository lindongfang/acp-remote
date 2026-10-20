# Proposal

## Why

对话页已有布局和基础卡片，但附件输入、可填写提问、真实终端、思考历史和 Access 侧远程链路仍不完整。用户要求对话区域前后端完整支持，并明确要求真实流式输出，而不是仅补视觉或模拟事件。

## What Changes

- 交付消息、回复、Agent 明确提供的思考、工具及结果、审批、提问、计划、终端、文件改动、轮次结果、多媒体、异常与降级的统一时间线，按用户轮次组织并保持发生顺序。
- 回复、思考、工具与终端按实际增量原位更新；重连补回且去重，停止或失败保留已收到内容，历史与实时结果一致。上读不强制滚底，支持未完成 Markdown。
- **BREAKING（交付边界调整）**：从仅文本输入扩为图片、文件、已有音频及工作目录路径引用。取消此前建议的附件数量/大小产品额度，保留分块、并发、速率、内存与磁盘保护及 Agent 真实能力限制；不以分块绕过 Agent 限额。
- 点击发送后上传，所有附件完成后才整体提交；失败保留完整草稿。已提交附件随消息保留，未提交上传闲置 24 小时清理；不做刷新续传。工作目录文件只传路径，不保存版本或追踪变化；链接不自动下载。
- **BREAKING（交付边界调整）**：允许重新获取并确认待处理请求后在刷新/换设备后应答；实现真实表单、服务端校验与首个有效应答胜出。执行中草稿改为可编辑，仍不发送、不排队。
- 补齐 Agent 驱动的真实终端服务，以及本地与 Node Link 远程会话的完整路径；Owner 保存正文，Access 不落远程正文。验证 Codex 和 Oh My Pi 的真实能力，缺能力时如实说明。
- 非目标：录音、录像、视频专用预览、用户手动输入终端、消息排队、页面刷新后的草稿恢复和上传断点续传、任意主机路径读取、自动下载外部链接、云服务。

## Capabilities

### New Capabilities

- `conversation-attachments`：附件选择、上传、提交绑定、Owner 保留、媒体读取、路径引用及资源保护。
- `agent-terminal-service`：Agent 请求的终端生命周期、真实输出、审批与进程清理。
- `node-link-access-session`：Access 出站连接、远程会话命令/读取/事件/附件转发及无正文持久化。

### Modified Capabilities

- `pwa-web-client`：轮次时间线、流式呈现、附件输入、可编辑草稿、表单与待处理恢复、折叠及安全富文本。
- `sync-session-history`：思考、部分输出与媒体的历史恢复，以及远程历史读取。

## Impact

- 用户逐项确认的 Q1–Q23 及后续修订见 `docs/conversation-area-discussion.md`；该记录用于需求追溯，不替代权威合同。
- 涉及 core、agent-host、storage-sqlite、协议 crate、server 入站、app 组合根、新增既定边界 node-link-client 与 clients/app；沿用层级依赖，不让入站适配器相互调用。
- 需要更新 INITIAL_DESIGN、FRONTEND_DESIGN、SYNC_PROTOCOL、NODE_LINK_PROTOCOL、SECURITY_DESIGN、CORE_PORTS_AND_STORAGE、MODULE_ARCHITECTURE、ACP_COMPATIBILITY_MATRIX；按实际端口/配置/CLI 变化同步 CONFIG_REFERENCE、LOCAL_ADMIN_PROTOCOL 等。不改变设备认证、Origin 或密钥规则。
- 协议加性能力经 feature 协商；旧客户端继续文本路径，新客户端面对旧服务端显式不支持，封闭 schema 不强推新字段。机器命令/feature/error 注册、Rust/TS 共用 fixture、存储迁移和合同漂移检查同时交付。
- 成功判据：本地和双节点真实 Daemon 浏览器验收、可控 ACP 全覆盖、Codex/Oh My Pi 兼容证据；覆盖实时、历史、重连、重复事件、审批竞争、上传失败与资源失败，不能把模拟 UI 测试当成真实贯通。
