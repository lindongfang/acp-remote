# `conversation-area-complete` 验收证据索引

本文件汇总 `conversation-area-complete` 的实际执行证据。原始帧、SQLite 查询、浏览器 CDP 输出与脚本保留在
各分项证据中；未执行项明确列出，不写成通过。

## 门禁

| 命令 | 结果 |
|---|---|
| 根目录 `npm run verify` | 通过：`cargo fmt`、`cargo clippy --workspace --all-targets --all-features -D warnings`、workspace 全量测试 |
| 根目录 `npm run check` | 通过：schema/fixture/命令/错误/feature/ACP/文档链接/边界/合同漂移/OpenSpec |
| `clients/app && npm run check` | 通过：typecheck、E2E typecheck、vitest（含附件/媒体/表单/滚动用例）、Web 构建 |

## Main E2E

| 用例 | 结果 | 证据 |
|---|---|---|
| E1 单节点时间线与媒体 | 通过：文本/思考/工具/计划/终端/文件改动；图片/音频 `blob:` 渲染；刷新后媒体经就绪重试恢复，多块读取单代、无并发 offset=0 | [`single.md`](conversation-area-complete-single.md) 第 73–82 节、第 1–27 节 |
| E2 流式、WSS 中断、停止部分正文 | 通过：原位累加、重连无重复、压缩后权威回填、停止保留 `cancelled` 部分正文 | 同上第 28–39 节 |
| E3 双设备交互/表单 | 通过：首个有效回答胜出、刷新重新取得、无效输入不消费、另一设备 `handled_elsewhere` | 同上第 28–50 节 |
| E4 附件上传/失败/取消/能力拒绝 | 通过：点击发送才上传；第二个附件断开时 0 prompt、完整草稿/选择保留、可重试；取消不上传；不支持图片在受理前 `command.unsupported` 且保留草稿；闲置 GC 保留已引用附件（Rust 用例） | 同上第 51–58 节、第 83–88 节 |
| E6 阅读锚点/媒体/窄屏 | 通过：上读保持锚点、新内容/回到底部、历史与媒体加载补偿、文件查看往返、窄屏 600×800 媒体与切档位置恢复 | 同上第 73–82 节 |
| E5 双节点 | **部分通过**：真实配对、imported 在线读取/历史、Owner 离线 `resource.remote_unavailable`/`owner_offline`、事件重放、撤销、Access 无正文磁盘检查 | [`dual.md`](conversation-area-complete-dual.md) |
| E7 真实 Codex / Oh My Pi | 通过：初始化能力、文本、工具、停止、历史；D3/D4/D8/D9 回归 | [`agents.md`](conversation-area-complete-agents.md) |
| E8 Windows 终端/进程树 | 通过：output/release、kill、Agent 崩溃、Daemon 关闭无孤儿；重启后历史可读 | 同上 |

## 真实 E2E 中修复的缺陷

生产组合根注入 `InteractionAuthority`；客户端补发 `attachment.manifest_sealed`；`turn.delta_compacted` 触发
`refresh_messages`；认证后补发首次 `session.read`；agent-host 响应/通知 FIFO；delta 指标使用输出事件下标；
`attachment.sealed` 回传内容 `attachmentId`；不支持媒体在受理前预检；媒体读取按代次串行/跨重建去重；
imported 事件进入 Sync 扇出、catalog 携带 `origin_epoch`、撤销写回本地信任；Owner Node Link
`session.read` 与 Access 回源接线。

## 未完成（不计通过）

- **E5 远程 mutation/附件/审批表单**：Owner 的 `server::acp_facade` 仍未落地，Sync 的远程
  `session.create`/`session.prompt`/`cancel`/有效交互应答/附件消费没有入站消费者；因此 7.2 不勾选。
- 真实 Agent 的图片/音频上传未执行（仅初始化能力与文本/工具/停止/历史）。
- 7.5 的最终独立 reviewer 与归档未执行（7.2 仍未闭环）。

## 已知限制

- OMP 的终端命令成功输出受本机 WSL 存根限制；Codex 走内部 exec，未观察到 ACP `terminal/create`。
- Access 新建会话的 catalog 增量推送有延迟，需重新订阅后才出现；已记录，未作为 E5 通过项。
