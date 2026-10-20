# `conversation-area-complete` 单节点真实 Daemon/浏览器验收证据

本文件只登记**实际执行并观察到的结果**。第 1 轮（下文的第 1–10 节）是**修复前二进制**的历史记录，保持原样；
修复后重跑见第 11–16 节。第 1 轮结论先行：

- **E1、E2、E3 未通过**：被组合根装配缺口阻塞（`InteractionAuthority` 未注入），不是断言失败。
- **E4 未通过**：附件发送路径被客户端/服务端的 `manifest_sealed` 协议错位阻塞。
- **E6 部分通过**：文件查看往返与草稿、窄屏文件往返、执行中可编辑不可发送、取消后草稿保留均有真实浏览器证据；
  长输出/上读锚点/媒体加载保持位置因正文读取被阻塞而**未执行**。
- 两个阻塞均已由主 Agent 确认并在主检出修复；本文只记录**修复前二进制**的实测结果，修复后须重跑 E1–E4。
- 未勾选 `tasks.md` 的 6.7/7.1；映射与建议见第 7 节。

## 1. 环境与分层

| 项目 | 实际值 |
| --- | --- |
| 操作系统 | Windows `10.0.26300.0`（win32 x64） |
| 被测二进制 | 主检出 `target/debug/acp-remote.exe`，构建于 2026-10-20 08:55（+0800），SHA-256 `a5ba4d8fce0be43c6f4f99956ed980027a98a23cbfc4669d02a2ff8eb7112d3c`；`uiBuildVersion = 0.0.0+71badcd-dirty` |
| 源码基线 | `71badcd`（隔离工作区与测试时主检出一致；修复发生在本轮测试之后） |
| Daemon | 真实本机 Daemon，`tls.mode = "direct"`、`public_origin = https://127.0.0.1:18771`、`bind = 127.0.0.1`、独立数据目录 `D:/tmp/acpr-wp7single/data`（未与其他 tester 共用端口/目录） |
| 浏览器 | 真实 Chromium 内核 Microsoft Edge `155.0.4283.62`，`--headless=new --ignore-certificate-errors`，CDP 驱动真实 PWA 路由与真实 DOM |
| ACP 子进程 | **可控测试夹具**（`agent.mjs`，LF-JSON ACP，按 argv 选择场景），**不是**真实 Codex/Oh My Pi；E7 不在本文件范围 |
| 证据形态 | DOM 文本/`data-*` 属性、WebSocket 帧、Daemon JSON 日志、SQLite 查询；**未采集截图**（headless CDP） |
| 配对 | 经本地管理通道 `device.pair.begin/status/confirm`（真实 named pipe，真实密码学流程）；配对码/密钥不进入本文件 |

“真实 Daemon/浏览器”指上述链路；“假主机/单测覆盖”本文件未使用。E1–E4/E6 的 Agent 事件由可控 ACP 子进程真实产生，
经 Daemon 的 ACP 适配器、Broker、SQLite、Sync 后才进入页面；它不替代真实 Agent 的能力结论（E7）。

原始运行材料（浏览器 WS 帧、Daemon 日志、ACP dump）位于本机临时目录
`D:/tmp/acpr-wp7single/`，不在仓库内；本文件内联关键原文。

## 2. E1：全部内容类型与刷新恢复

### 2.1 实际执行

真实 Daemon + 真实 Edge，创建会话后发送一条文本 prompt，ACP 夹具依次发出：
`agent_thought_chunk`、两段 `agent_message_chunk`、带 `diff` 的 `tool_call`、真实 `terminal/create`
（`terminal/output` 产生 stdout/stderr 增量）、`plan`、`agent.message.completed`、`turn.completed`。

页面观察（原始返回值，节选）：

```text
cards = ["turn","file-changed","plan","terminal-output","terminal-output","degraded","degraded"]
messages = []
text 包含：internal.unavailable：the command cannot be served（可重试）
```

即：轮次、文件改动、计划、终端输出、降级卡片**按真实事件出现**；但文本/思考/媒体消息不出现。
刷新（`Page.reload`）后同一会话的时间线为**空**（`timelineCount = 0`、`cards = []`），未恢复任何消息或结构化卡片。

### 2.2 根因（真实可观察的读取失败）

对同一会话直接经已认证 WebSocket 发送 `session.read`，只改变 `include`：

```json
{"command":"session.read","payload":{"include":["messages"]}}                         -> completed，返回 "普通回复"
{"command":"session.read","payload":{"include":["config_options"]}}                   -> completed
{"command":"session.read","payload":{"include":["history"]}}                         -> completed
{"command":"session.read","payload":{"include":["pending_interactions"]}}            -> rejected internal.unavailable
{"command":"session.read","payload":{"include":["messages","config_options"]}}       -> completed
{"command":"session.read","payload":{"include":["messages","config_options","history","pending_interactions"]}} -> rejected internal.unavailable
```

客户端首屏固定请求 `messages + config_options + history + pending_interactions`，因此**整条首屏读取失败**：
正文、配置、历史一起没有，页面只剩实时结构化卡片。刷新时页面在连接建立前调用 `loadSession`，读取未被派发，
时间线为空。

### 2.3 装配缺口

测试时基线的生产组合根没有注入有效交互端口（只在测试支持里注入）：`crates/app/src/compose.rs` 构造
`Broker::new(...)` 后未调用 `Broker::with_interaction_authority(...)`。`Broker::effective_interactions`
在端口缺席时对**任何** `pending_interactions` 读取返回 `Unavailable(BackendUnsupported)`，即使当前没有待处理请求。

- 现象：`session.read` 带 `pending_interactions` 恒为 `internal.unavailable`（含空请求集）。
- 影响：E1 文本/思考/媒体与刷新恢复、E2 最终一致性、E3 刷新重新取得请求，全部无法进入可判定状态。
- 处置：已在主检出修复（本轮测试后），本文不计为通过；需重跑。

### 2.4 结论

**E1 未通过（被 compose 装配缺口阻塞）**。可确认的是工具/计划/终端/文件改动卡片确实由真实事件驱动出现；
文本、思考、媒体与刷新恢复没有取得可观察的成功证据。

## 3. E2：逐块流式、WSS 中断、停止部分正文

### 3.1 实际执行

ACP 夹具逐块发送（间隔 900 ms）未闭合 Markdown 与后续闭合文本，页面在运行中可见易失正文：

```text
Agent
这是停止前的第一段，
流式增量（尚未持久化）：正文以会话读取的结果为准。
```

执行中继续发送、页面 `turn.state = running`、`cancel` 按钮出现。点击停止后：

```text
composerState = editable, draft = "执行中编辑的下一条草稿",
submitEnabled = true, turnStates = ["cancelled"],
partials = [], messages = []
```

即服务端进入 `cancelled`，但页面**没有**保留可核对的部分正文（`messages` 为空，无 `data-message-partial`）。
原因同 2.2：`turn.delta_compacted` 移除易失增量后，权威正文只能经 `session.read` 取回，而该读取失败。

### 3.2 未执行/未取得证据

- **主动中断 WSS**：未执行。断线重放/补回无重复/历史与最终一致必须依赖成功的 `session.read` 才能断言，
  在当前构建上执行只会得到“空时间线”，不构成有效验收。
- **流式原位更新且无重复**：只观察到运行中的易失正文出现；未取得完成后的权威正文与 DOM 节点稳定性证据。
- **失败/停止保留部分正文**：停止后 `partials = []`、`messages = []`，未通过。

### 3.3 观察到的正文丢失线索（未单独定性）

在 E1 场景中，同一消息的第二段正文在两次合并窗口之后才到达；SQLite 中事件齐全，但
`agent.message.completed.content` 只含第一段：

```text
events: ... agent.message.delta deltaIndex=0 text="开始处理：…"
        agent.message.delta deltaIndex=1 text="处理完成。"
        turn.completed
        agent.message.completed content=[{"type":"text","text":"开始处理：…"}]
owned_message.content_json=[{"text":"开始处理：…","type":"text"}]
```

这可能使 `session.read` 的权威正文丢掉后到的增量（即使 2.3 修复后仍影响“最终一致”）。本轮没有与单测对照，
**列为待确认缺陷线索，不作为已确认失败或通过**。

### 3.4 结论

**E2 未通过（被 compose 装配缺口阻塞）**；WSS 主动中断未执行。

## 4. E3：双设备、首个有效回答、刷新重新取得、无效输入不消费

**未执行，被 2.3 阻塞**。原因：

- 客户端与服务端都协商了 `core.pending-interactions.v1`，但 `pending_interactions` 读取恒失败；
- 刷新/换设备后“重新取得当前有效请求”只能走该读取；
- 授权/表单卡片的 `requestVersion` 因此不可得，页面只可能呈现“版本未知/不可应答”，无法验证首个有效回答胜出、
  无效输入不消费请求。

没有启动第二个浏览器设备，也没有伪造这两条断言的替代证据。

## 5. E4：附件、失败注入、取消、不支持、清理

### 5.1 选中不上传（通过）

真实 Chromium 选择 `pixel.png` 后，页面状态：

```text
data-attachment-kind=image, data-attachment-status=selected
文本：“图片 · 70 字节 · 待上传（点击发送后上传）”
发送按钮 data-submit-enabled=true
```

未点击发送前没有上传；点击后才进入上传状态。这是可直接观察的消费者行为。

### 5.2 点击发送：上传成功但清单不封存，业务消息不派发（阻塞）

同一附件点击发送，捕获真实附件连接帧：

```text
sent:  attachment.begin, attachment.chunk, attachment.seal,
       attachment.manifest_begin, attachment.manifest_page(final=true)
recv:  attachment.chunk_ack(nextOffset="70"), attachment.sealed(length="70", sha256=…)
```

随后**没有** `attachment.manifest_sealed` 回复，也没有 `command`/`session.prompt` 帧：

```text
UI:      data-attachment-status=sealed；草稿仍在；提交按钮再次可用
ACP dump: session/prompt 计数 = 0（发送前后都为 0）
```

根因：客户端 `sealManifest` 在最后一页后等待 `manifest_sealed`，但它**不发送** `attachment.manifest_sealed`
请求；服务端 `manifest_page` 只登记 `final=true`，只有 `Request::ManifestSealed` 才回 `Reply::ManifestSealed`。
这是一处客户端/服务端协议错位，与 2.3 独立。已在主检出修复（本轮测试后），E4 发送路径须重跑。

### 5.3 Agent 不支持时不静默删附件（部分观察）

使用未宣告图片能力的 ACP 夹具（`promptCapabilities.image=false`）重复 5.2：附件上传完成后保持
`data-attachment-status=sealed`，草稿保留，未派发 prompt。由于清单未封存，**没有走到服务端能力校验**，
因此不能据此断言“不支持时整条拒绝且不删附件”已通过；只能确认页面没有静默删除附件或擅自改成纯文本发送。

### 5.4 未执行/未取得证据

- **部分失败不执行**（一个附件失败、另一个成功）：未执行。
- **失败注入**：未取得可判定的失败态（清单阻塞先于业务受理）。
- **取消不执行**：未执行到可观测的“上传中 → 取消 → 无 prompt”完整链路。
- **闲置 24 小时清理不误删引用**：未执行。需要推进系统时间或直接改写持久时间；本轮没有对真实库做时间穿越，
  不能把 Rust 清理用例的结果当作浏览器验收。
- **已有音频**：未选择/未发送（音频能力声明与上传路径同图片，但未执行即不计通过）。

### 5.5 结论

**E4 未通过（附件清单封存协议错位）**；“选中不上传”和“失败时不删附件”有部分真实证据，其余未执行。

## 6. E6：长输出、上读、文件查看、主题与窄屏

### 6.1 通过项（真实浏览器，原始返回值）

1. **文件查看往返保留草稿**（1280×900）：
   - 打开 `note.txt` 后查看器内容 `17 字节 / hello attachment`，关闭后草稿值仍为
     `草稿保留：文件往返后应还在`；`draftPreserved.equal = true`。
2. **窄屏文件查看往返**（600×800）：
   - 切到「文件」档后 `filesPanel=true, rows=3`；打开 `note.txt` 查看器正常；
   - 关闭后 `filesPanel=true, viewer=false, rows=3`，草稿仍为原值。
3. **执行中可编辑、不可发送**（slow ACP 场景）：
   ```text
   running:  composerState=in_progress, editable=true, submitEnabled=false, cancel=false(首帧)
   edited:   value="执行中编辑的下一条草稿", submitEnabled=false,
             turnStates=["running"], cancel=true
   after:    composerState=editable, draft 保留, submitEnabled=true, turnStates=["cancelled"]
   ```
   即执行中草稿可编辑、发送保持禁用，取消后草稿仍在。
4. **主题偏好切换**：控件 `data-theme-preference` 从 `system` 变为 `light`。未额外断言像素级配色与刷新后的持久化，
   因此只按“偏好切换可观察”计，不作为完整主题验收。

### 6.2 未执行/未通过

- **长输出不强制滚底、上读保持锚点、展开/媒体加载保持位置**：未执行。正文与结构化历史读取被 2.3 阻塞，
  当前时间线无法加载长消息，无法建立可判定的滚动锚点。
- **窄屏“对话 / 会话 / 文件”三档**：已观察到三档文字与切换控件；文件档往返通过（见上）。未执行滚动位置跨断点保留。
- **媒体加载保持位置**：未执行（E1 媒体消息未加载）。

### 6.3 结论

**E6 部分通过**：文件查看/草稿往返、窄屏文件往返、执行中编辑与发送禁用、取消后草稿保留有真实证据；
长输出与滚动锚点部分被阻塞，未通过。

## 7. 与 6.7 / 7.1 的映射及建议

| 任务 | 可引用的本轮证据 | 建议 |
| --- | --- | --- |
| 6.7 上读保持锚点、新内容/回到底部、历史加载、会话/文件往返位置 | 第 6.1 节 1/2/3/4：文件查看往返保留草稿、窄屏文件往返、执行中编辑/禁用发送、取消后草稿；**无**长输出/上读锚点/媒体加载位置证据 | **不勾选**。文件往返部分可作为 6.7 的子证据，但 6.7 的完成判据包含持续输出与媒体加载位置，未取得 |
| 7.1 合入后 PV7 + 单节点真实 Daemon E1–E4/E6 | 第 2/3/4/5/6 节；E1/E2/E3/E4 均被两个装配/协议缺陷阻塞，E6 部分通过 | **不勾选**。修复后的主检出新构建须重跑 E1–E4 与 E6 的未执行项，再据新证据勾选 |

## 8. 缺陷与复现步骤

### 8.1 生产组合根未注入 `InteractionAuthority`（已由主 Agent 修复）

复现：

1. 用 `tls.mode=direct` + 独立数据目录启动真实 Daemon，配置一个可应答的 ACP Agent。
2. 配对真实浏览器并创建会话。
3. 在已认证业务 WebSocket 上发送
   `{"command":"session.read","sessionId":"…","payload":{"include":["pending_interactions"]}}`。
4. 观察：`command.result.status = "rejected"`，`error.code = "internal.unavailable"`；
   `include:["messages"]` / `["history"]` / `["config_options"]` 均 `completed`。

定位（测试时基线）：`crates/app/src/compose.rs` 构造 `Broker::new(...)` 后未调用
`Broker::with_interaction_authority(...)`；`crates/core/src/broker.rs::effective_interactions` 在端口缺席时
`Unavailable(BackendUnsupported)`。影响：首屏读取整体失败、刷新空白、E3 无法判定。

### 8.2 附件清单封存协议错位（已由主 Agent 修复）

复现：

1. 在同一真实页面上选择一个图片附件（状态 `selected`，未上传）。
2. 点击发送；在附件 WebSocket 上观察帧。
3. 实际：`attachment.begin/chunk/seal` 得到 `chunk_ack`/`sealed`；
   `attachment.manifest_begin` + `manifest_page(final=true)` 无回复；无 `session.prompt`；UI 停在 `sealed`。
4. 根因：客户端 `clients/app/src/sync-client/attachment-transfer.ts::sealManifest` 不发送
   `attachment.manifest_sealed`；服务端 `crates/server/src/attachment_transfer.rs::manifest_page` 返回
   `Ok(None)`，仅 `manifest_sealed` 请求回 `Reply::ManifestSealed`。

### 8.3 待确认：跨合并窗口的正文可能丢失

见 3.3。事件表有两条 delta，`agent.message.completed.content` 与 `owned_message.content_json` 只有第一条；
本轮没有用单测隔离，修复 8.1 后应优先复核 E2 的“历史与最终一致”。

## 9. 未执行项汇总（不得计为通过）

1. E3 双设备全部场景（首个有效回答、刷新重新取得、无效输入不消费）。
2. E2 主动中断 WSS、断线重放无重复、完成后的权威正文/最终一致。
3. E4 部分失败不执行、取消不执行、可判定的上传失败注入、已有音频、闲置 24 小时清理不误删引用。
4. E6 长输出/上读锚点/新内容与回到底部/媒体加载保持位置、跨断点滚动位置保留。
5. 主题的像素级配色与刷新持久化（只观察到偏好切换）。
6. 截图证据（本轮为 headless CDP，使用 DOM 文本与 WS 帧；未采集截图）。
7. 真实 Codex / Oh My Pi（E7）与 Windows 终端进程树（E8），不属于本单节点工作包。
8. 任何 `cargo test` / `npm run check` / PV7：隔离工作区没有 `target/` 与 `node_modules/`，未运行，也不以本轮浏览器证据替代。

## 10. 进程与现场清理

- Daemon 以 `daemon stop` 正常停止，单实例锁与 `daemon.instance.json` 已释放（最后一步实测输出
  `daemon 已停止（单实例锁已释放）`，随后实例文件不存在）。
- 每个浏览器由 CDP `Browser.close` 关闭；未留下常驻 headless Edge 进程。
- 所有测试数据位于 `D:/tmp/acpr-wp7single/`（独立于主检出与其他 tester）；未修改主检出、未提交、未勾选 `tasks.md`。

---

# 第 2 轮：修复后重跑（E1–E4 / E6）

> 第 1 轮（上文）是**修复前二进制**的记录，按「历史」保留，不作改动。本节只登记第 2 轮**实际执行并观察到的结果**。

## 11. 第 2 轮结论先行

- **两个已知阻塞已解除并有真实证据**：
  1. `session.read` 带 `pending_interactions` 全部 `completed`（第 12.2 节）；
  2. 客户端在清单最后一页后发送 `attachment.manifest_sealed`，服务端回 `attachment.manifest_sealed`（第 12.5 节）。
- 编号约定：D1 = 组合根未注入 `InteractionAuthority`（第 8.1 节，已修，本轮验证通过）；
  D2 = 附件清单封存协议错位（第 8.2 节，已修，本轮验证通过）；
  D3 = `turn.delta_compacted` 后不重读权威正文（第 8.3 节的线索，主 Agent 已在主线修复，
  **本轮二进制不含该修复**）；D4–D7 为本轮新发现，见第 13 节。
- **E1 仍未通过**：刷新/直接打开会话 URL 后**没有任何 `session.read` 被派发**（0 帧），时间线为空（第 12.2 节，新缺陷 D4）。
- **E2 仍未通过**：单消息原位流式成立（同一 `messageId`，15→51→58 字符），但
  `agent.message.completed.content` 与 `owned_message.content_json` **只含第一段**，最后一段
  未落盘；同一 turn 的合并窗口记录 `daemon.merge_window_failed invalid_request`（第 12.3 节，新缺陷 D5）。
  另：压缩后正文从页面消失属已由主 Agent 在主线修复的缺陷（D3），本轮二进制**不含**该修复，故不计通过、也不计失败。
- **E3 未通过**：单设备授权/表单链路成立（刷新取得、无效输入不消费、有效值原样提交），但
  **第二设备直接打开会话不读取任何内容**（同 D4），且第一台设备答完后第二台仍显示可填写的表单、
  再次提交是静默无操作（第 12.4 节，新缺陷 D6）。
- **E4 未通过**：点击发送才上传成立，上传与清单封存全链路成功，但随后的 `session.prompt`
  被服务端以 `attachment.binding_mismatch` 拒绝——清单绑定的是附件内容 id，客户端只能拿到
  transfer id（第 12.5 节，新缺陷 D7）。部分失败/取消/闲置清理未执行。
- **E6 部分通过**：上读时持续输出不把视口拉回底部（`scrollTop` 保持 0，`scrollHeight` 846→2046）、
  文件查看往返保留草稿、窄屏三档与文件往返、主题偏好切换有真实证据；长输出完成后正文消失
  （同 D3）、媒体加载与跨断点滚动位置未执行。

## 12. 第 2 轮实际执行

### 12.1 环境与构建

| 项目 | 实际值 |
| --- | --- |
| 操作系统 | Windows `10.0.26300.0`（win32 x64） |
| 源码树 | 隔离工作区 `D:/Data/omp-wt/tf8f54854c/m`（含 `compose.rs` 注入 `InteractionAuthority` 与 `attachment-transfer.ts` 发送 `attachment.manifest_sealed`；**不含**主 Agent 之后在主线加入的 `turn.delta_compacted` 刷新修复） |
| 前端产物 | 由该树 `npx expo export --platform web --clear` 重建，`build-version.json` = `0.0.0+71badcd-dirty`，`builtAt = 2026-10-20T01:16:46.563Z`；包内含 `attachment.manifest_sealed` 共 5 处（含发送点） |
| 被测二进制 | `D:/tmp/acpr-wp7single2/target/debug/acp-remote.exe`，2026-10-20 09:17（+0800），SHA-256 `ba2e818f57554641861424e289e9d5efcf42102be8c85fc31b1b6c8f34f248b1`；`cargo build -p app` 从该树构建，二进制内嵌上述新产物 |
| Daemon | 真实本机 Daemon，`tls.mode = "direct"`、`public_origin = https://127.0.0.1:18793`、独立数据目录 `D:/tmp/acpr-wp7single2/data`、独立实例 `daemon.instance.json` |
| 浏览器 | 真实 Microsoft Edge `155.0.4283.62`，`--headless=new --ignore-certificate-errors`，CDP 驱动真实 PWA 路由与 DOM |
| ACP 子进程 | 同一可控夹具 `agent.mjs`（场景 `full` / `stream` / `stream-cancel` / `interact` / `long`），不是真实 Codex/Oh My Pi；E7 不在本节范围 |
| 证据形态 | DOM `data-*` 属性与文本、WebSocket 帧类型序列、Daemon JSON 日志、SQLite 只读查询；未采集截图 |
| 原始材料 | `D:/tmp/acpr-wp7single2/`（`evidence/e1.json`、`e2.json`、`e3.json`、`e4.json`、`e6.json`、`daemon.log`、`agent-*.log`） |

### 12.2 E1：内容类型、`session.read`、刷新恢复

**通过的部分**（真实事件驱动的结构化卡片与流式正文）：

- 流式期间采样 6 次，**3 次**出现 `data-timeline-entry="message"`（`thought` + `agent`），正文长度随时间增长；
- 压缩后时间线剩 7 条：`["turn","file-changed","tool-call","plan","terminal-output","terminal-output","degraded"]`，
  `data-message-*` 为 0（见 D3：主线已修复，本轮二进制未含该修复）。

**`session.read` 五种 include 全部成功**（真实返回，按 `body.requestId` 匹配）：

```text
["pending_interactions"]                                        -> completed, pending=0
["messages"]                                                    -> completed, messages=2,
   agentTexts=["先分析请求，再检查文件。","开始处理：E1 全部内容。"]
["history"]                                                     -> completed, history=11,
   types=["turn.queued","turn.started","file.changed","tool.call.started","session.plan.changed",
          "tool.call.started","terminal.output","terminal.output","tool.call.updated",
          "tool.call.completed","turn.completed"]
["config_options"]                                              -> completed
["messages","config_options","history","pending_interactions"]  -> completed
```

即第 1 轮的第 2.2 节阻塞**已解除**。但 `messages` 只有两段（思考 + 回复首段），**缺**「处理完成。」与「全部完成。」（见 D5）。

**刷新恢复未通过**：`Page.reload` 后 6 s，时间线 `count = 0`、无任何卡片；同页面统计
`session.read` 出站帧 `beforeReload = 6`、`afterReload = 0`：

```text
CASE afterReload {"count":0,"kinds":[],"readDispatch":{"beforeReload":6,"afterReload":0}}
```

同一现象在“已配对页面直接导航到 `/session/<id>`”上稳定复现（等待 10 s），见 D4。

**媒体**：未取得证据——附件 prompt 被 D7 拒绝，Agent 收不到图片，不产生 `image_ref`。

### 12.3 E2：逐块流式、WSS 中断、停止部分正文

**流式原位更新（通过）**：`stream` 场景逐块（900 ms）发送未闭合 Markdown，页面采样 24 次：

```text
agentMessageIds = ["655486c0-0ee9-4208-b291-a81c8b6ae41a"]     // 全程一个消息节点
lengths = [[],[15],[15],[15],[15],[15],[15],[15],[51],[58],[58],[58],[58],[58],[],[],...]
```

同一 `messageId`、正文在原节点上增长，未出现重复节点。

**主动中断 WSS（部分）**：流式中调用 `sock.close(4001)`，客户端重连成功（页面内 `sockets = 2`），
未观察到重复消息节点；但最终可见正文为空（D3），无法在本轮二进制上判定“补回无重复 + 历史与最终一致”。

**权威正文（通过）**：中断后 `session.read include:["messages"]` 返回完整三段拼接：

```text
"流式第一段：未闭合的 **强调在此继续，仍然未闭合。\n\n```ts\nconst value = 1;\n```\n\n最后的收尾。"
```

**停止保留部分正文（服务端通过 / 页面未通过）**：`stream-cancel` 场景中点击停止后：

```text
turn = ["cancelled"]；页面 messages = 0
session.read messages = [
  {status:"cancelled", partial:{state:"cancelled",reason:"turn_cancelled"}, text:"先想一下这一段。"},
  {status:"cancelled", partial:{state:"cancelled",reason:"turn_cancelled"}, text:"这是停止前的第一段，"}
]
```

服务端确实保留了已收到的部分正文；页面在压缩后未保留（D3，主线已修复）。

**跨合并窗口的正文丢失（第 1 轮 3.3 的线索，已确认为缺陷 D5）**：`full` 场景连续两次重跑，
逐字一致：

```text
events:  agent.message.delta deltaIndex=0 text="开始处理：E1 全部内容。"
         agent.message.delta deltaIndex=1 text="处理完成。"
         ...（无 deltaIndex=2："全部完成。"）
         turn.completed / agent.message.completed / turn.delta_compacted deltaCount=1
completed.content   = [{"type":"text","text":"开始处理：E1 全部内容。"}]
owned_message.content_json = [{"text":"开始处理：E1 全部内容。","type":"text"}]
daemon.log: {"error":"invalid_request","event":"daemon.merge_window_failed", ... "session_id":"114f2fa9-…",
             "ticks":1762,"ts":"2026-10-20T01:25:19.173Z"}
```

同一次 turn 的 `turn.delta_compacted.deltaCount = 1`，而实际已落盘 delta 有 2 条；缺失的最后一段
在 ACP 流里紧随 prompt 响应之前发出，却从未落到事件表。两轮复现（`114f2fa9…`、`ee4fa276…`）。

### 12.4 E3：双设备、首个有效回答、刷新重新取得、无效输入

**单设备链路（通过）**：

```text
授权卡片初始： availability = not_answerable（request_version_unknown，无按钮）
点击「重新取得当前有效请求」后：
              availability = answerable, options = ["allow-once","reject-once"]
点击「允许一次」后：
              授权卡片 = handled_here（“已由本设备处理”）；夹具日志 permission allow-once
表单卡片：    点击刷新后 form = supported, fields = ["count","note"]
无效输入：    count=0 → formError「不能小于 1。」/「表单里有未通过校验的字段。」，
              卡片仍 answerable；数据库命令表**没有**新增 elicitation.respond
有效输入：    count=3,note="ok" → handled_here；
              夹具日志 elicitation {"action":"accept","values":{"count":3,"note":"ok"}}
```

`owned_command` 佐证无效输入不消费请求：该会话只有一条 `elicitation.respond`（`completed`）。

**第二设备未通过**：
- 直接在设备 B 打开同一会话：`entries = 0`、`[data-interaction-id] = 0`（同 D4：无 `session.read`，
  但实时事件仍能到达 B——授权应答后 B 收到了 elicitation 卡片）；
- 设备 A 提交有效表单后，B 的卡片仍为 `availability = answerable`、`form = supported`；
  B 再次填写并提交：无可见提示、无新命令（`owned_command` 无第二条 `elicitation.respond`），
  即“已被其它设备处理”状态没有传播到 B（D6）。

### 12.5 E4：点击发送才上传、封存、派发、不支持

**点击发送才上传（通过）**：选择 `pixel.png`（图片）、`note.txt`（文件）、`audio.wav`（音频）后：

```text
rows = [image:selected, file:selected, audio:selected]
文本 = “待上传（点击发送后上传）”
附件连接只有 auth.client_hello / auth.client_proof，没有任何 attachment.* 帧
```

**封存协议已修复（通过）**：点击发送后的真实帧序列：

```text
sent: attachment.begin, attachment.chunk, attachment.seal  ×3
      attachment.manifest_begin, attachment.manifest_page, attachment.manifest_sealed
recv: attachment.chunk_ack, attachment.sealed ×3, attachment.manifest_sealed
```

第 1 轮 5.2 的“清单不封存”已解除。

**业务派发未通过（D7）**：封存后页面出现

```text
data-submission = failed
“提交失败：attachment.binding_mismatch：the attachment reference is not bound to this submission”
```

数据库对照（同一会话）：

```text
owned_attachment_transfer.transfer_id = 4ec642e4-29de-4221-b89c-8fe0d0d59c71  (red.png, sealed)
owned_attachment_submission_item.attachment_id = 9b44d98b-df14-4545-85be-dcb0f3a48c7f
```

清单绑定的是**附件内容 id**（按 sha256 去重后的 `owned_attachment.attachment_id`），客户端只能
拿到自己的 `transferId`，`session.prompt` 携带 `attachmentId = transferId` 因此必然不匹配。
换一个从未上传过的新文件后仍然 `binding_mismatch`，所以不是去重偶发。

**Agent 不支持图片 / 取消 / 部分失败 / 闲置清理**：未取得可判定证据。上述 binding_mismatch 发生在
服务端派发 Agent 之前，`files`（未宣告图片能力）场景同样先被它拦住；取消按钮在 100 ms 采样窗内
未出现（上传过快）；闲置 24 小时清理未做时间穿越。

### 12.6 E6：长输出/上读、文件往返、主题、窄屏

```text
上读前： scrollTop=365, scrollHeight=846, clientHeight=481, following=true,  entries=2
滚到顶部后继续输出 3 s：
        scrollTop=0,   scrollHeight=2046, following=false, unseen=false,
        [data-action="scroll-to-bottom"] 可点击（jump=true）
完成后：entries=1, messages=0, degraded=[], turn=["completed"]     // 正文随后被压缩移除（D3）
```

即“持续输出不强制滚底、上读锚点保持”成立；但长输出完成后的正文可见性未通过。

```text
文件往返（1280×900）：rows=[audio.wav,note.txt,pixel.png,red.png]
  查看器内容 = "17 字节\nhello attachment\n\n下载文件"
  关闭后：viewer=false，draft 仍为 "草稿保留：文件往返后应还在"
主题： preference system → light（html data-theme 仍为 null，body 背景为透明，未取得配色证据）
窄屏 600×800：segments=[all*, running, pending, timeline*], filesPanel rows=4，
  打开 note.txt → 查看器正常，segment=all；关闭后 panel=true, viewer=false, draft 保留
```

媒体加载位置、跨断点滚动位置恢复：未执行（媒体消息被 D7 阻断）。

## 13. 第 2 轮发现的新缺陷（D4–D7）

### D4 刷新/直接打开会话 URL 不派发 `session.read`（E1 刷新恢复、E3 换设备）

复现：

1. 真实 Daemon + 已配对真实 Edge，创建并完成一条 turn（首屏链路正常，页面累计派发 6 条 `session.read`）。
2. `Page.reload`（或新设备直接导航 `/session/<id>`）。
3. 观察：6–10 s 内该连接只有 `sync.subscribe` / `sync.snapshot_request` 两条出站，`session.read` 为 **0**；
   时间线 `[data-timeline-entry-at]` 计数为 0。

定位方向（只读代码，未改）：`app/session/[id].tsx` 的 effect 在挂载时调用 `store.loadSession`，
而 `client-store.#dispatchIntent` 在 `identity.connectionId === null` 时**直接返回 null 且不登记待重试**；
`authenticated`/`online` 事件到达后没有对“当前活动会话”补一次首屏读取（`setActiveConversationSession`
只丢弃旧文件读取）。因此直连/刷新落在“页面已挂载、连接尚未认证”的窗口里时永远不读。

### D5 合并窗口提交失败后丢失最后一段流式正文

复现：

1. 会话用 `full` 场景发送 prompt（`agent_thought` → `agent_message` 两段 → `diff` → 真实终端 → `plan` → 收尾文本）。
2. 观察 `daemon.log`：该 turn 最后一次 `daemon.merge_window` 记录
   `{"error":"invalid_request","event":"daemon.merge_window_failed",...}`，随后是“本轮无活动会话”，
   即缓冲未被重试。
3. 观察 `owned_event`：`agent.message.delta` 只有 2 条；`turn.delta_compacted.deltaCount = 1`；
   `agent.message.completed.content` 与 `owned_message.content_json` 只有第一段。

影响：`session.read` 的权威正文缺段，即使客户端在压缩后重读（主线 D3 修复）也只能看到第一段。
执行路径上 `flush_locked` 先 `std::mem::take` 整个缓冲，`commit_chunk` 对非 `Unavailable` 错误
直接上抛、既不回填也不放弃该 turn，缓冲中的事件因此丢失。

### D6 跨设备交互解析状态不传播，第二设备仍可填写并静默无效果

复现：

1. 设备 A 创建会话并触发授权请求；设备 B 直接打开同一会话（实时事件可达，卡片经“重新取得”变为可填写）。
2. 设备 A 填写合法值并提交（`owned_command` 出现一条 `elicitation.respond=completed`；夹具收到
   `{"count":3,"note":"ok"}`）。
3. 设备 B 的卡片仍 `availability=answerable`、`form=supported`；B 再次填写提交：
   页面无任何提示，`owned_command` 无第二条 `elicitation.respond`。

### D7 附件 prompt 绑定要求“附件内容 id”，客户端只有 transfer id

复现：

1. 支持图片的 Agent、真实浏览器，选择任意图片。
2. 点击发送：`attachment.*` 与 `attachment.manifest_sealed` 正常完成。
3. 观察：`data-submission = failed`，
   `attachment.binding_mismatch：the attachment reference is not bound to this submission`；
   `session.prompt` 未派发到 Agent（`owned_command` 无该命令，ACP dump 无 prompt）。
4. 数据库对照见 12.5：`submission_item.attachment_id ≠ transfer_id`。

`crates/server/src/attachment_transfer.rs::verify_prompt_bindings` 用 `manifest.attachments`
（`AttachmentId`）比对 prompt 的 `attachmentId`；而附件连接的 `attachment.sealed` /
`manifest_sealed` 回包都不携带 `AttachmentId`。客户端 `client-store.#dispatchPrompt` 因此只能回填
`transferId`。契约文档（`SECURITY_DESIGN.md` §prompt 绑定、`NODE_LINK_PROTOCOL.md` §12.7）
要求“逐块比对清单里的 `attachmentId`”，但当前帧族没有把该值交给客户端。

## 14. 与 6.7 / 7.1 的映射（第 2 轮）

| 任务 | 第 2 轮可引用的证据 | 是否可勾选 |
| --- | --- | --- |
| 6.7 上读保持锚点、新内容/回到底部、历史加载、会话/文件往返位置 | 第 12.6 节：上读时 `scrollTop` 保持 0、输出继续增长；文件查看往返保留草稿；窄屏文件往返；主题偏好切换。**缺**长输出完成后的正文（D3/D5）、媒体加载位置、跨断点滚动位置 | **不可勾选** |
| 7.1 合入后 PV7 + 单节点真实 Daemon E1–E4/E6 | E1 首屏读取已修复但刷新不恢复（D4）；E2 流式与部分正文服务端成立、最终一致性缺段（D5）；E3 单设备成立、双设备不传播（D6）；E4 封存成立、派发被拒（D7）；E6 部分通过 | **不可勾选** |

## 15. 第 2 轮未执行项（不得计为通过）

1. E1 媒体消息显示与加载位置（被 D7 阻断）。
2. E4 部分失败不执行、取消不执行、可判定的上传失败注入、闲置 24 小时清理不误删引用。
3. E3 “首个有效回答胜出”在**双设备都可填写**前提下的完整判定（B 端没有可判定的第二设备答完后的拒绝态，D6）。
4. E6 跨断点滚动位置恢复、窄屏媒体加载位置。
5. 主题的像素级配色与刷新后持久化（只观察到偏好值切换，`html[data-theme]` 仍为 `null`）。
6. PV7（`npm run verify` + `clients/app npm run check`）与任何 `cargo test`：本节在隔离工作区执行，
   未将其结果冒充浏览器验收。
7. 真实 Codex / Oh My Pi（E7）与双真实节点（E5）、Windows 进程树（E8），不属于本节范围。

## 16. 第 2 轮现场清理

- Daemon 以 `daemon stop` 停止，输出 `daemon 已停止（单实例锁已释放）`；本轮最后一次停止后不再启动。
- 所有浏览器由 CDP `Browser.close` 关闭。
- 数据、日志、原始证据均在 `D:/tmp/acpr-wp7single2/`；隔离工作区只新增了本文件的这一节，
  未改产品代码、未改 `openspec` 任务、未提交。

---

# 第 3 轮：修复后重跑（E1–E4 / E6，验证 D3–D7）

> 前两轮按「历史」保留，本文不作改动。本节只登记第 3 轮**实际执行并观察到的结果**；通过 / 未通过 /
> 未执行分开写。原始材料在 `D:/tmp/acpr-wp7single3/`（`evidence/e1r3.json`、`e2r3.json`、`e3r3.json`、
> `e4r3.json`、`e4media.json`、`e6r3.json`、`e6b.json`、`daemon.log`、`agent-*.log`）。

## 17. 第 3 轮结论先行

| 项 | 结果 | 依据 |
| --- | --- | --- |
| D3 `turn.delta_compacted` 后回填 | **通过** | E2 流式完成后页面正文含「最后的收尾。」；E6 长输出完成后正文 2450 字符含「第 60 段」 |
| D4 刷新 / 直接打开会话派发 `session.read` | **通过** | 刷新后出站 `session.read = 1`、时间线 9 条恢复；新配对浏览器直接打开 `/session/<id>` 同样 `reads = 1`、9 条恢复 |
| D5 尾段 delta 进入 `agent.message.completed.content` / `owned_message.content_json` | **未通过** | 含工具调用的 `full` 场景稳定丢最后一段；`turn.delta_compacted.deltaCount` 与正文同时缺段（第 24.3 节） |
| D6 第二设备 resolved / `handled_elsewhere` | **未通过** | `owned_event` 中**没有任何** `permission.resolved` / `elicitation.resolved`；B 仍显示可填表单，提交只得到一条 `failed` 命令且页面无提示 |
| D7 `attachment.sealed.attachmentId` 绑定 | **通过** | `session.prompt` 携带封存内容 id，无 `binding_mismatch`，命令 `completed`，ACP 侧收到 `image/resource/audio` 原生块与字节 |
| E1 | **部分通过** | `session.read` 五种 include 全部 `completed`；首屏/刷新/直开恢复通过；正文缺尾段（D5） |
| E2 | **通过** | 原位流式、WSS 中断后权威正文完整、停止保留部分正文、页面回填通过 |
| E3 | **部分通过** | 单设备授权/表单、无效输入不消费通过；B 直开可读通过；跨设备解析状态传播失败（D6） |
| E4 | **部分通过** | 点击发送才上传、封存、派发、取消上传通过；Agent 不支持图片时整条失败且客户端清空附件（新缺陷 D8） |
| E6 | **部分通过** | 上读锚点、完成后正文、回到底部、文件往返、窄屏三档、主题偏好持久化通过；图片字节加载位置、跨断点滚动位置未取得 |

## 18. 环境与构建（第 3 轮）

| 项目 | 实际值 |
| --- | --- |
| 操作系统 | Windows `10.0.26300.0`（win32 x64） |
| 源码树 | 隔离工作区 `D:/Data/omp-wt/t4f9e2f155/m`（与主检出同源；`client-store.ts`、`broker.rs` 与 `D:/Project/acp-remote` 的 md5 一致） |
| 前端产物 | 该树 `cd clients/app && npm run export:web`，`build-version.json` = `0.0.0+71badcd-dirty`，`builtAt = 2026-10-20T01:43:17.453Z`；bundle 内含 `refresh_messages` 与 `deferredReads`（D3/D4 标记） |
| 被测二进制 | `target/debug/acp-remote.exe`，2026-10-20 09:43（+0800），SHA-256 `2064c2b599ab88d0315f074825bb184eac56673293194b218b3fe913df0c3dac`；`cargo build -p app` 从该树构建，构建时 `clients/app/dist` 已存在，静态资源按真实产物嵌入 |
| Daemon | 真实本机 Daemon，`tls.mode = "direct"`、`public_origin = https://127.0.0.1:18804`、独立数据目录 `D:/tmp/acpr-wp7single3/data`、独立实例文件 |
| 浏览器 | 真实 Microsoft Edge `155.0.4283.62`，`--headless=new --ignore-certificate-errors`，CDP 驱动真实 PWA 路由与 DOM |
| ACP 子进程 | 可控夹具 `agent.mjs`（场景 `full` / `stream` / `stream-cancel` / `interact` / `long` / `files`），不是真实 Codex/Oh My Pi；E7 不在本节范围 |
| 证据形态 | DOM `data-*` 属性与文本、WebSocket 帧（出站 command / 入站 command.result）、Daemon JSON 日志、SQLite 只读查询；未采集截图 |

说明：本轮固定使用**自己构建**的二进制与产物（与主检出同源），未使用第 2 轮的旧 dist。

## 19. E1：内容类型、`session.read`、压缩后回填、刷新/直开恢复

### 19.1 `session.read` 五种 include 全部成功（与第 2 轮一致）

```text
["pending_interactions"]                                        -> completed, pending=0
["messages"]                                                    -> completed, messages=2,
   agentTexts=["先分析请求，再检查文件。","开始处理：E1 全部内容。"]
["history"]                                                     -> completed, history=11,
   types=[turn.queued,turn.started,file.changed,tool.call.started,session.plan.changed,
          tool.call.started,terminal.output,terminal.output,tool.call.updated,
          tool.call.completed,turn.completed]
["config_options"]                                              -> completed
["messages","config_options","history","pending_interactions"]  -> completed
```

### 19.2 D3：压缩后确实发生了一次消息页重读

```text
出站 session.read 计数：首屏前 1 → 压缩窗口后 2 → 轮次结束后 2
```

即 `turn.delta_compacted` 触发了一次 `refresh_messages`（`include:["messages"]`），页面据此显示权威正文。
（本条只证明**回填机制**成立；本次正文仍缺尾段，属于 D5，见 24.3。）

### 19.3 D4：刷新与直接打开都恢复（通过）

```text
reload:      afterReload.count = 9,
             kinds = [turn,message,message,file-changed,tool-call,plan,terminal-output,terminal-output,degraded],
             readDispatch = { beforeReload: 7, afterReload: 1 }
直接打开:    （新配对浏览器导航 /session/<id>）
             count = 9, reads = 1, messageTexts = ["先分析请求，再检查文件。","开始处理：E1 全部内容。"]
```

刷新前连接上累计 7 条 `session.read`，刷新后新连接补发 1 条并恢复 9 条时间线条目；新设备直开同样补发 1 条。

## 20. E2：逐块流式、WSS 中断、停止部分正文

```text
原位流式（stream 场景，200 ms 采样 30 次，第 4 次采样后 close(4001)）：
  agent 消息节点长度序列 = [..,15,15,51,51,58,58,..]（同一消息 id），重连后 sockets = 2
  同屏出现两条消息 id（thought 与 agent），agent 正文在原节点增长，未出现重复正文节点

完成后（压缩 + 回填）页面正文：
  "流式第一段：未闭合的 **强调在此继续，仍然未闭合。 ts 复制 1 const value = 1; 最后的收尾。"
session.read 权威正文：
  "流式第一段：未闭合的 **强调在此继续，仍然未闭合。\n\n```ts\nconst value = 1;\n```\n\n最后的收尾。"
  turn = ["completed"]，degraded = []
```

D5 在本场景（无工具调用）**通过**：三段 delta 全部进入 completed 正文。

停止：

```text
点击停止前：turn=["running"]，正文 "这是停止前的第一段，"
停止后：    turn=["cancelled"]，页面消息正文保留 "先想一下这一段。" 与 "这是停止前的第一段，"
session.read：status="cancelled"，partial={"state":"cancelled","reason":"turn_cancelled"}，正文同上
草稿：本场景未预置草稿（draft=""），提交按钮保持禁用
```

服务端与页面都保留了已收到的部分正文（页面不再出现第 1 轮的 `messages=[]`）。

## 21. E3：双设备、刷新重新取得、无效输入不消费、D6

单设备（通过，与第 2 轮一致）：

```text
初始：authorization card = not_answerable（request_version_unknown，无按钮）
刷新后：answerable，options=["allow-once","reject-once"]
允许一次后：handled_here，夹具日志 permission allow-once
表单：刷新后 form=supported，fields=[count,note]
无效输入 count=0：错误「不能小于 1。」/「表单里有未通过校验的字段。」，卡片仍 answerable，
                 owned_command 里 elicitation.respond 计数仍为 0
有效输入 count=3,note="ok"：handled_here；elicitation.respond=completed（计数 1）
```

设备 B 直接打开（D4）：

```text
entries = 2, kinds = [turn, permission-requested],
授权卡片 availability = answerable，options = [allow-once, reject-once]
```

设备 A 处理完表单后，设备 B（D6，未通过）：

```text
B 的 elicitation 卡片：availability = answerable, form = supported,
                      formFields = [count,count,note,note], submit 按钮存在
B 再次提交：owned_command 新增一条 elicitation.respond，status = failed
            页面无任何「已被其它设备处理」提示；卡片仍为 answerable
owned_event 中该会话的事件类型只有：
  command.completed, turn.queued, turn.started, permission.requested, elicitation.requested,
  agent.message.delta, turn.completed, agent.message.completed, turn.delta_compacted, command.failed
  —— 没有 permission.resolved / elicitation.resolved
```

## 22. E4：点击发送才上传、封存、D7 派发、取消、不支持

### 22.1 选择不上传（通过）

```text
rows = [image:selected, file:selected, audio:selected]，文本「待上传（点击发送后上传）」
附件连接出站帧：只有 auth.client_hello / auth.client_proof
```

### 22.2 封存与 D7 派发（通过）

```text
sent: attachment.begin, attachment.chunk, attachment.seal ×3
      attachment.manifest_begin, attachment.manifest_page, attachment.manifest_sealed
recv: attachment.chunk_ack, attachment.sealed ×3, attachment.manifest_sealed
UI:   data-submission = completed，附件列表清空（发送成功后的正常清理）
session.prompt 命令计数：0 → 1（completed），无 binding_mismatch
```

出站 `session.prompt` 原文（节选）：

```json
{"content":[
  {"type":"attachment_ref","attachmentId":"0ba92de8-5f95-46dd-bf51-3d8e09ca34bf","mediaType":"image/png","byteLength":"70","sha256":"xBTNDiBN…"},
  {"type":"attachment_ref","attachmentId":"5c7c0d82-cbe0-4316-bf2f-bbb441c74768","mediaType":"text/plain","byteLength":"17","sha256":"KFLXJi8f…"},
  {"type":"attachment_ref","attachmentId":"1dc32632-cc5a-41ce-830f-5ccdb949d11b","mediaType":"audio/wav","byteLength":"44","sha256":"hXWbOBH_…"},
  {"type":"text","text":"E4 附件发送"}
],"submissionId":"e2a77268-83ba-4279-b005-ab3071b829c4"}
```

ACP 侧实际收到的 prompt 块（夹具新增日志）：

```text
prompt {"id":5,"text":"E4 附件发送","attachments":0,
        "blockTypes":["image","resource","audio","text"],"blockBytes":[96,0,60,0],
        "resourceUris":["urn:acp-remote:attachment:5c7c0d82-cbe0-4316-bf2f-bbb441c74768"]}
```

即 `attachment_ref` 被适配成 ACP 原生 `image`（base64 96 字符 = 70 字节）、`resource`（带本机 urn）、
`audio` 块；第 2 轮的 `attachment.binding_mismatch` 不再出现，prompt 真正到达 Agent。

### 22.3 取消上传（通过）

```text
发送后 15×20 ms 内出现取消按钮并点击
sent: attachment.begin, attachment.chunk, attachment.abort
recv: attachment.chunk_ack, attachment.status
附件状态：audio:cancelled；草稿仍为 "E4 取消上传"；retry 按钮出现
session.prompt 计数：0 → 0（未派发）
```

### 22.4 Agent 不支持图片（未通过，新缺陷 D8）

```text
文件（images=false）Agent，选择 pixel.png 并发送：
  上传与清单封存照常完成
  data-submission = failed
  session.prompt：accepted → turn.queued → turn.started → turn.failed
  command.result: status=failed, error={code:"internal.unavailable", message:"请求不合法"}
  页面：附件列表 []、草稿 ""（提交前被清空），只剩「提交失败：internal.unavailable：请求不合法」
  ACP 侧：files Agent 的 dump 为空（能力判定在适配器构造 prompt 时失败，未发出 session/prompt）
```

## 23. E6：长输出/上读、完成后正文、回到底部、文件往返、主题、窄屏

### 23.1 上读锚点与完成后正文（通过）

```text
上读前： scrollTop=365, scrollHeight=846, clientHeight=481, following=true
滚到顶部后继续输出 3 s：
        scrollTop=0, scrollHeight=2046, following=false, unseen=false
轮次完成并压缩后（回填）：
        entries=2, turn=["completed"], 正文 2450 字符，含 "第 60 段：…"
点击回到底部：scrollTop=2573, scrollHeight=3096, clientHeight=523, following=true
```

即持续输出不把视口拉回底部，上读锚点保持；`turn.delta_compacted` 后长正文完整回到页面（D3）。

### 23.2 文件查看往返保留草稿（通过）

```text
rows = [audio.wav, note.txt, pixel.png, red.png]
viewer 内容 = "17 字节\nhello attachment\n\n下载文件"
关闭后：viewer=false，draft 仍为 "草稿保留：文件往返后应还在"
```

### 23.3 主题（部分）：偏好切换并跨刷新保持

```text
before: preference=system, html[data-theme]=null, body bg=rgba(0,0,0,0)
after:  preference=light,  html[data-theme]=null, body bg=rgba(0,0,0,0)
reload: preference=light（持久化成立）
```

只观察到偏好值与持久化；`html[data-theme]` 仍为 `null`、背景透明，**未取得像素级配色证据**。

### 23.4 窄屏三档与文件往返（通过）

```text
600×800：tabs = [timeline(selected), sessions, files]
切到 files：panel=true, rows=4
打开 note.txt：查看器内容 "17 字节\nhello attachment\n\n下载文件"，draft 仍为 "窄屏草稿保留"
关闭：panel=true, viewer=false, draft 保留
```

### 23.5 媒体（部分）

`full` 场景下 Agent 回显媒体：

```text
prompt blockTypes=["resource","text"]，resource uri = urn:acp-remote:attachment:5c7c0d82-…
页面媒体块："文件：note.txt（text/plain，18 字节） 下载文件"（不自动拉取字节，需点击下载）
上读后等待 2.5 s：scrollTop 保持 0，scrollHeight 不变（该会话滚动容器 clientHeight=0，位置断言不具代表性）
```

图片方向：Agent 收到的 ACP `image` 块不携带附件 id，夹具只能回 `image_ref` 且无 `attachmentId`，
页面因此无法按 id 拉取图片字节（`data-media-state` 为空）。**图片字节加载位置本轮未取得证据**，
不计通过；同时说明 ACP 侧 `image` 块没有把 `attachmentId` 交给 Agent，Agent 无法引用回原附件。

## 24. D3–D7 判定与复现（第 3 轮）

### 24.1 D3 已修复

`turn.delta_compacted` 之后会派发 `refresh_messages`（E1 `session.read` 计数 1→2；E2/E6 完成后正文完整）。
复现：发送一条回复→等 `turn.delta_compacted`→页面仍显示完整正文。

### 24.2 D4 已修复

刷新与新设备直接打开 `/session/<id>` 都补发一次首屏读取并恢复时间线（19.3 节）。
复现：已配对页面 `Page.reload`，或新配对设备导航到 `/session/<id>`；观察 `session.read` 出站与时间线条目。

### 24.3 D5 未修复：尾段正文仍丢，且有两个独立成因

本轮树包含主线的「终态与尾段同批」修复（`completed_events_for` 的 `in_chunk`），但 `full` 场景稳定复现：

```text
owned_event（session c7e0d975-…）：
  5 agent.message.delta deltaIndex=0 "开始处理：E1 全部内容。"
  9 agent.message.delta deltaIndex=1 "处理完成。"
  （没有 deltaIndex=2 "全部完成。"）
 15 turn.completed
 16 agent.message.completed content=[{"type":"text","text":"开始处理：E1 全部内容。"}]
 18 turn.delta_compacted deltaCount=1
owned_message.content_json=[{"text":"开始处理：E1 全部内容。","type":"text"}]
daemon.log:
 {"error":"invalid_request","event":"daemon.merge_window_failed",
  "message":"驱动会话的合并窗口失败：本轮跳过该会话，下轮重试","session_id":"c7e0d975-…","ticks":613}
```

第二处证据（`full` 场景，媒体回显会话 `e99fcd89-…`）：

```text
events: deltaIndex=0 "开始处理：E4 媒体回显。"，deltaIndex=1 "处理完成。"，无 deltaIndex=2 "全部完成。"
agent.message.completed content = "开始处理：E4 媒体回显。处理完成。"（缺最后一段）
turn.delta_compacted deltaCount = 2（实际应含 3 段）
```

两轮 E1 会话（`de2ee660…`、`c7e0d975…`）逐字一致：`agent.message.delta` 只有 2 条、completed 只有第一段、
`deltaCount` 少计。定位（只读代码，未改）：

1. **响应与通知的处理顺序倒置**：`crates/agent-host/src/process.rs::handle_line`（622–682）对
   `MessageClass::Response` **就地**结算 oneshot（662–681），而 `session/update` 通知经 mpsc 交给路由任务
   异步处理。于是 `session/prompt` 的响应对应的 `turn.completed` 可能先于刚刚读到的最后一段 delta 落盘；
   随后的 delta 批次被 core 以 `invalid_request` 拒绝，`Broker::flush_locked`（2658–2686）已
   `std::mem::take` 了缓冲并在 `?` 处直接返回 → 该批事件永久丢失，`daemon.merge_window_failed` 只记录不重试。
2. **delta 登记用错下标**：`commit_chunk` 里 `delta_plan.push((event_index, …))`（2791）记录的是**输入
   chunk** 的下标，而合并成功后用 `kinds`/`outcome.appended.get(event_index)`（2976、3007–3021）按**输出
   events** 的下标回查；`file.changed` 等派生事件会在循环中额外 push，导致其后 delta 的下标错位、未被并入
   `slot.deltas`。这使 `agent.message.completed` 与 `deltaCount` 少算已落盘的 delta（E1 中
   `处理完成。` 已入库却不在 completed 正文里）。

复现命令：`node e1r3.mjs`（`full` 场景）后查 `owned_event`/`owned_message` 与 `daemon.log` 的
`daemon.merge_window_failed`。

### 24.4 D6 未修复：服务端不产生 `*.resolved` 事件

设备 A 提交合法表单后：

```text
owned_interaction：permission 与 elicitation 都 state=resolved（resolved_by_id=设备 A）
owned_event：没有任何 permission.resolved / elicitation.resolved
设备 B：elicitation 卡片仍 availability=answerable、form=supported；
        再提交 → owned_command 多一条 elicitation.respond(status=failed)，页面无提示
```

客户端具备 `handled_elsewhere` 判定（`interaction-model.ts` 处理 `*.resolved`），但事件源不存在，
因此「首个有效回答胜出」在页面上没有可观察的跨设备传播。

复现：`node e3r3.mjs`；A 提交合法表单后观察 B 的 `data-interaction-availability` 与 `owned_event`。

### 24.5 D7 已修复

见 22.2：客户端用 `attachment.sealed.attachmentId` 组装 `attachment_ref`，服务端绑定校验通过，命令
`completed`，ACP 收到原生媒体块。复现：选择任一附件发送，检查 `session.prompt` 出站正文、
`owned_command` 与夹具日志的 `blockTypes`。

### 24.6 D8（本轮新发现）：能力不支持时错误不具体且清空附件

Agent 未宣告图片能力时，服务端受理 prompt 后让 turn 失败，错误是泛化的
`internal.unavailable：请求不合法`（`command.result.error` 亦然），且客户端在派发前已清空附件与草稿，
用户无法重试或改用其它内容。第 1 轮 5.3 的「不静默删附件」在本轮**未通过**。
复现：`files` Agent + 图片 → 发送；观察 `turn.failed`、页面「提交失败：internal.unavailable：请求不合法」
与附件列表清空。

## 25. 与 6.7 / 7.1 的映射（第 3 轮）

| 任务 | 本轮可引用证据 | 是否可勾选 |
| --- | --- | --- |
| 6.7 上读保持锚点、新内容/回到底部、历史加载、会话/文件往返位置 | 23.1 上读时 `scrollTop` 保持 0、输出继续增长、完成后正文完整、回到底部生效；23.2/23.4 文件查看往返保留草稿、窄屏三档与文件往返；23.3 主题偏好持久化。**缺**图片字节加载位置、跨断点滚动位置恢复、历史的 UI 翻页 | **不可勾选**（缺项仍在） |
| 7.1 合入后 PV7 + 单节点真实 Daemon E1–E4/E6 | E1 部分通过（D5）、E2 通过、E3 部分通过（D6）、E4 部分通过（D8）、E6 部分通过（图片媒体/跨断点） | **不可勾选** |

## 26. 第 3 轮未执行 / 未取得证据（不得计为通过）

1. 图片/音频字节在页面上的实际加载与加载时位置保持（Agent 侧 ACP `image` 块无 `attachmentId`，无法回引）。
2. 跨断点（1280×900 ↔ 600×800）滚动位置恢复。
3. 历史消息的 UI 翻页加载（本轮只验证 `session.read include:["history"]` 返回 11 条）。
4. 主题的像素级配色证据（只观察到 `data-theme-preference` 与刷新持久化）。
5. E4 的失败注入（可判定的上传失败）、部分失败（一个成功一个失败）、闲置 24 小时清理不误删引用。
6. PV7（`npm run verify` / `clients/app npm run check`）与任何 `cargo test`：本节在隔离工作区用自建二进制
   做浏览器验收，不以单测结果替代；合入后的验证由主检出负责。
7. 真实 Codex / Oh My Pi（E7）、双真实节点（E5）、Windows 终端进程树（E8）。

## 27. 第 3 轮现场清理

- Daemon 以 `daemon stop` 停止，输出 `daemon 已停止（单实例锁已释放）`；最后一次停止后不再启动。
- 所有浏览器由 CDP `Browser.close` 关闭；未留下常驻 headless Edge 进程。
- 数据、日志、原始证据均在 `D:/tmp/acpr-wp7single3/`；隔离工作区只新增了本文件的这一节，
  未改产品代码、未改 `openspec` 任务、未提交。

---

# 第 4 轮（最终）：修复后重跑（E1–E4 / E6，验证 D5/D6/D8/D9 并回归 D3/D4/D7）

> 前三轮按「历史」保留，本节只登记第 4 轮**实际执行并观察到的结果**。原始材料在
> `D:/tmp/acpr-wp7single4/`（`evidence/e1r3.json`、`e2r3.json`、`e3r3.json`、`e3b.json`、`e4r3.json`、
> `e4media.json`、`e6r3.json`、`e6b.json`、`e6c.json`、`d5mix.json`、`d8-prepare.json`、
> `d8-restart.json`、`d9-titles.json`、`daemon.log`、`agent-*.log`）。本节从第 28 节起编号。

## 28. 第 4 轮结论先行

| 项 | 结果 | 依据 |
| --- | --- | --- |
| D5 尾段 delta 进入 `agent.message.completed.content` / `owned_message.content_json`；无 `merge_window_failed` | **部分通过** | 普通 `full` / `stream` 的尾段（含与 `turn.completed` 同批）完整落盘且 `deltaCount = 3`；但**同批插入派生 `file.changed` 时仍丢第二段**（`deltaCount = 1`），见 31.3 |
| D6 跨设备 `permission.resolved` / `elicitation.resolved` 与 `handled_elsewhere` | **通过** | `owned_event` 出现两条 resolved；B 实时、刷新、新设备直开均为 `handled_elsewhere`，无表单输入/提交按钮（33） |
| D8 Daemon 重启后首读 `session.read`（含 `config_options`）成功 | **通过** | 重启后无活动会话（日志持续「本轮无活动会话」），首读 `include` 四项 `status = completed`，9 条时间线恢复（35） |
| D9 工具卡片标题取最新非空 title | **通过** | started「编辑 src/main.rs」→ updated「编译 src/main.rs」→ completed 空串，页面标题仍为「编译 src/main.rs」，无 degraded（36） |
| D3 压缩后回填 / D4 刷新与直开 / D7 附件绑定 | **通过（回归）** | E1 刷新/直开 9 条 + `session.read` 各 1；E2 压缩后正文完整；E4 `session.prompt` 携带封存内容 id 且 ACP 收到原生块（30–32） |
| E1 | **部分通过** | 五种 include 全 completed、刷新/直开恢复、正文与尾段完整；图片字节显示仍未取得（`image_ref` 无 `attachmentId`） |
| E2 | **通过** | 原位流式、WSS 中断后权威正文完整、停止保留部分正文、压缩后回填 |
| E3 | **通过** | 单设备授权/表单、无效输入不消费；B 实时/刷新/直开均显示已处理且不可填 |
| E4 | **部分通过** | 选择不上传、上传/封存/派发/取消、音频与原生块通过；**Agent 不支持图片仍清空附件与草稿并给泛化错误**（34.4）；部分失败/失败注入/闲置清理未执行 |
| E6 | **部分通过** | 长输出/上读/回到底部、完稿正文、文件往返+滚动位置、窄屏三档+切档锚点、跨断点位置、主题偏好持久化通过；图片媒体加载位置未取得；像素级配色未取得 |

明确失败：31.3 的 D5 残留（同批派生事件导致尾段未并入 completed；`deltaCount` 少计），以及 34.4 的
E4「Agent 不支持图片时清空附件与草稿、错误泛化」。两者都有可复现命令与原始输出。

## 29. 环境与构建（第 4 轮）

| 项目 | 实际值 |
| --- | --- |
| 操作系统 | Windows `10.0.26300.0`（win32 x64） |
| 源码树 | 主检出 `D:/Project/acp-remote`（含本轮 D5/D6/D8/D9 修复后的工作区；隔离工作区只写本文件） |
| 前端产物 | 主检出 `clients/app/dist/build-version.json`：`version = 0.0.0+71badcd-dirty`，`builtAt = 2026-10-20T02:02:53.754Z` |
| 被测二进制 | `D:/Project/acp-remote/target/debug/acp-remote.exe`，2026-10-20 10:03:12（+0800），SHA-256 `22c8a3e3179a0682e6f4616deffdbfb7fdf21b210ddf9f0aa6607d1a957d73e2`；`daemon status.uiBuildVersion = 0.0.0+71badcd-dirty`（与 dist 一致） |
| Daemon | 真实本机 Daemon，`tls.mode = "direct"`、`public_origin = https://127.0.0.1:18815`、数据目录 `D:/tmp/acpr-wp7single4/data`、独立实例 |
| 浏览器 | 真实 Microsoft Edge `155.0.4283.62`，`--headless=new --ignore-certificate-errors`，CDP 驱动真实 PWA 路由与 DOM |
| ACP 子进程 | 可控夹具 `agent.mjs`（`full` / `files` / `stream` / `stream-cancel` / `interact` / `long`，另为 D5/D9 新增 `d5mix`、`titles`），不是真实 Codex/Oh My Pi；E7 不在本节范围 |
| 证据形态 | DOM `data-*` 属性与文本、WebSocket 帧（出站 command / 入站 command.result）、Daemon JSON 日志、SQLite 只读查询；未采集截图 |
| 合并窗口失败计数 | 本轮 `daemon.log` 中 `daemon.merge_window_failed` 出现 **0** 次（E1–E6、D5 混合场景均无） |

## 30. E1：内容类型、`session.read`、压缩后回填、刷新/直开恢复

会话 `ad7f78b3-05b8-42e4-9585-2281178af312`（`full` 场景）。

五种 `include` 全部 `completed`（真实返回值）：

```text
["pending_interactions"]                                        -> completed, pending=0
["messages"]                                                    -> completed, messages=2,
   agentTexts=["先分析请求，再检查文件。","开始处理：E1 全部内容。处理完成。全部完成。"]
["history"]                                                     -> completed, history=13,
   types=[turn.queued,turn.started,file.changed,tool.call.started,session.plan.changed,
          tool.call.started,terminal.output,terminal.output,tool.call.updated,
          tool.call.completed,tool.call.updated,tool.call.completed,turn.completed]
["config_options"]                                              -> completed
["messages","config_options","history","pending_interactions"]  -> completed
```

时间线与刷新/直开（D3/D4 回归）：

```text
完成后：count = 9,
  kinds = ["turn","message","message","file-changed","tool-call","plan","tool-call","terminal-output","terminal-output"],
  messageTexts = ["先分析请求，再检查文件。","开始处理：E1 全部内容。处理完成。全部完成。"],
  degraded = []
Page.reload：count = 9，readDispatch = { beforeReload: 7, afterReload: 1 }，正文同上
新配对浏览器直开 /session/<id>：count = 9，reads = 1，正文同上
```

即 E1 的文本/思考/工具/计划/终端/文件改动与刷新恢复均成立；图片字节显示见 37（未取得）。

## 31. E2：逐块流式、WSS 中断、停止部分正文、D5 尾段

### 31.1 原位流式与中断（通过）

```text
stream 会话 fad5035d-…：30 次采样（200 ms），第 4 次采样后 close(4001)
  agent 正文长度序列 = [..,15,15,51,51,58,58,..]（同一消息 id），重连后 sockets = 2
  未出现重复正文节点
完成后页面正文 = "流式第一段：未闭合的 **强调在此继续，仍然未闭合。 ts 复制 1 const value = 1; 最后的收尾。"
session.read 权威正文 =
  "流式第一段：未闭合的 **强调在此继续，仍然未闭合。\n\n```ts\nconst value = 1;\n```\n\n最后的收尾。"
turn = ["completed"]，degraded = []
```

`owned_event`：`agent.message.delta` 0/1/2 三条，`agent.message.completed.content` 为三段拼接，
`turn.delta_compacted.deltaCount = 3`，`owned_message.reply.content_json` 同值。

### 31.2 停止保留部分正文（通过）

```text
点击停止前：turn = ["running"]，正文 "这是停止前的第一段，"
停止后：turn = ["cancelled"]，页面保留 "先想一下这一段。" 与 "这是停止前的第一段，"
session.read：status = "cancelled"，
  partial = {"state":"cancelled","reason":"turn_cancelled"}，正文同上
degraded = []
```

### 31.3 D5 残留失败：同一合并窗口内派生事件导致后段 delta 未并入 completed

`full` / `stream` 场景（尾段与 `turn.completed` 同批）本轮已通过：`deltaIndex=2` 落盘、
completed 正文三段、`deltaCount=3`、`owned_message` 三段，且无 `merge_window_failed`。

但用可控 ACP 在**同一合并窗口**内按 `delta → 派生 file.changed → delta → turn.completed` 顺序发送
（`d5mix` 场景）时稳定复现后段丢失：

```text
会话 3897deb4-6a88-40b4-981e-17e49c85570e
owned_event:
  4 agent.message.delta deltaIndex=0 text="同批第一段。"
  5 file.changed（同一 tool_call 的 diff 派生）
  6 tool.call.started
  7 agent.message.delta deltaIndex=1 text="同批第二段。"
  8 turn.completed
  9 agent.message.completed content=[{"type":"text","text":"同批第一段。"}]
 11 turn.delta_compacted deltaCount=1
owned_message.reply.content_json = [{"text":"同批第一段。","type":"text"}]
页面消息正文 = "同批第一段。"（第二段从未进入页面）
daemon.merge_window_failed 计数 = 0（本次不是 FIFO/缓冲丢失，而是合并登记下标错位）
```

复现：`cd D:/tmp/acpr-wp7single4/driver && D:/Application/Node/nodejs/node.exe d5mix.mjs`；完成后查询
`owned_event` / `owned_message` 与页面 `[data-message-text]`。两轮复现（`3897deb4-…` 与
`529fd615-…`）逐字一致：页面都只有「同批第一段。」、completed 只有第一段、`deltaCount=1`、
`deltaIndex=1` 已落盘却未被并入。

与第 24.3 节定位一致：`commit_chunk` 用**输入 chunk 下标**登记 `delta_plan`，而合并成功后按**输出
events 下标**回查；`file.changed` 在循环中额外 push 使后续 delta 的下标错位，导致已落盘的
`deltaIndex=1` 未并入 `slot.deltas`，于是 completed 正文与 `deltaCount` 同时少计。第 24.3 节的
「响应/通知 FIFO」成因已修复（本轮 0 次 `merge_window_failed`），此下标成因**仍在**。

影响：真实 Agent 若在同一 250 ms 窗口内交错发送工具/文件事件与消息增量，消费者的权威正文会缺段；
`session.read` 与页面都只能看到第一段。

## 32. E3：双设备、首个有效回答、刷新重新取得、无效输入、D6

会话 `89f1d95c-6f63-4ca4-815f-231737127ae5`（`interact` 场景）。

单设备链路（通过）：

```text
初始：authorization card = not_answerable（request_version_unknown，无按钮）
刷新后：answerable，options = ["allow-once","reject-once"]
允许一次后：handled_here（"已由本设备处理：allow-once"）；夹具日志 permission allow-once
表单：刷新后 form=supported，fields=[count,note]
无效输入 count=0：错误「不能小于 1。」/「表单里有未通过校验的字段。」，卡片仍 answerable，
                  owned_command 的 elicitation.respond 计数仍为 0
有效输入 count=3,note="ok"：handled_here；elicitation.respond=completed（计数 1）
```

`owned_event`（D6 修复的直接证据）：

```text
5 permission.resolved {"interactionId":"c380f7ec-…","resolution":"allow-once","resolvedByDeviceId":null}
8 elicitation.resolved {"action":"submit","interactionId":"dbf7896d-…","resolvedByDeviceId":null}
owned_interaction：两条均 state=resolved，resolved_by_id=设备 A
```

设备 B（同一浏览器会话）在 A 提交后：

```text
permission 卡片 = handled_elsewhere（"已被其它设备处理（不是本设备所为）：allow-once"）
elicitation 卡片 = handled_elsewhere，form = null，canFill = false，submit 按钮不存在
再次提交尝试：canFill = false（没有产生第二条 elicitation.respond）
```

设备 B 刷新与**新设备直开**（`e3b.mjs`）：

```text
直开：permission/elicitation 均 handled_elsewhere，
      form = null，inputs = []，submit = false
刷新后：同上（同一输出）
```

即「首个有效回答胜出」在 B 端有可观察的拒绝态；D6 的实时传播、刷新重放、新设备直开三条路径均通过。

## 33. E4：点击发送才上传、封存、D7 派发、取消、Agent 不支持

### 33.1 选择不上传（通过）

```text
rows = [image:selected, file:selected, audio:selected]
文本 = "待上传（点击发送后上传）"
附件连接出站帧只有 auth.client_hello / auth.client_proof
```

### 33.2 封存与派发（通过，D7 回归）

```text
sent: attachment.begin, attachment.chunk, attachment.seal ×3
      attachment.manifest_begin, attachment.manifest_page, attachment.manifest_sealed
recv: attachment.chunk_ack, attachment.sealed ×3, attachment.manifest_sealed
UI:   data-submission = completed，附件列表清空
session.prompt 计数：0 → 1（completed），无 binding_mismatch
```

出站 prompt 携带三个封存内容 id（原文节选）：

```json
{"content":[
  {"type":"attachment_ref","attachmentId":"59ef08d9-df28-41c5-9b7c-7897edea20ef","mediaType":"image/png",...},
  {"type":"attachment_ref","attachmentId":"94e73ed9-792a-4b71-b704-86cd7495498f","mediaType":"text/plain",...},
  {"type":"attachment_ref","attachmentId":"e9821f0e-b78d-4b95-bda6-dbe4cd3ef86e","mediaType":"audio/wav",...},
  {"type":"text","text":"E4 附件发送"}
],"submissionId":"165abd9b-72d1-45db-abc2-d87dadb21f43"}
```

ACP 侧实际收到：

```text
prompt {"id":5,"text":"E4 附件发送","attachments":0,
        "blockTypes":["image","resource","audio","text"],"blockBytes":[96,0,60,0],
        "resourceUris":["urn:acp-remote:attachment:94e73ed9-792a-4b71-b704-86cd7495498f"]}
```

### 33.3 取消上传（通过）

```text
发送后 15×20 ms 内出现取消按钮并点击
sent: attachment.begin, attachment.chunk, attachment.abort
recv: attachment.chunk_ack, attachment.status
附件状态：audio:cancelled；草稿仍为 "E4 取消上传"；retry 按钮出现
session.prompt 计数：0 → 0（未派发）
```

### 33.4 Agent 不支持图片（未通过，本轮残留）

`files` Agent（`promptCapabilities.image=false`），选择 `pixel.png` 后发送：

```text
会话 48a436d3-fd0c-43f1-bee4-c8fe3e44c640
上传与清单封存完成；session.prompt 受理（命令 1 条）
UI: data-submission = failed，附件列表 []，草稿 ""
    "提交失败：internal.unavailable：请求不合法"
owned_event: 4 turn.failed {"turnId":"607f8714-…","state":"failed","error":{}}
owned_command: session.prompt status=failed, error_code=protocol.schema_invalid,
               error_message="请求不合法"
ACP dump: []（未发出 session/prompt）
```

即：服务端确实在派发 Agent 前整条拒绝（Agent 一个字节也没收到），但**客户端已清空附件与草稿**，
用户无法重试或改用其它内容；且错误是泛化的 `internal.unavailable：请求不合法`，既没有
`capability.unsupported_by_agent`，`turn.failed` 事件里的 `error` 还是空对象 `{}`。
第 1 轮 5.3 的「不静默删附件」仍是失败。复现：`node e4r3.mjs` 的第 2 步（`files` + 图片）。

### 33.5 未执行/未取得证据（不得计为通过）

- 部分失败不执行（一个附件失败、另一个成功）。
- 可判定的上传失败注入。
- 闲置 24 小时清理不误删引用（未做时间穿越）。

## 34. E6：长输出/上读、回到底部、文件往返与位置、窄屏、主题

### 34.1 长输出与上读（通过）

```text
session f0e0850b-…：
上读前： scrollTop=413, scrollHeight=894, clientHeight=481, following=true
滚到顶部后继续输出 3 s：
        scrollTop=0, scrollHeight=1998, following=false, unseen=false
轮次完成并压缩后（回填）：
        entries=2, turn=["completed"], degraded=[]，正文 2450 字符，含 "第 60 段：…"
点击回到底部：scrollTop=2573, scrollHeight=3096, clientHeight=523, following=true
```

### 34.2 文件查看往返保留草稿与滚动位置（通过）

```text
rows = [audio.wav, note.txt, pixel.png, red.png]
viewer 内容 = "17 字节\nhello attachment\n\n下载文件"
关闭后：viewer=false，draft 仍为 "草稿保留：文件往返后应还在"
单列宽度 1280×900：打开前 scrollTop=300；打开 note.txt → 关闭后 scrollTop=300（preserved=true）
```

### 34.3 窄屏三档、文件往返、切档与跨断点锚点（通过）

```text
600×800：segments = [timeline(selected), sessions, files]
切到 files：panel=true, rows=4
打开 note.txt：viewer 内容 "17 字节\nhello attachment\n\n下载文件"，draft 仍为 "窄屏草稿保留"
关闭：panel=true, viewer=false, draft 保留
真实上读路径（先到底 following=true，再滚到 200）：before={scrollTop:200, following:false}
切到 files 再切回 timeline：after={scrollTop:200, following:false}（preserved=true）
再切回 1280×900：scrollTop=200（跨断点保持）
```

### 34.4 主题（部分）

```text
before: preference=system, html[data-theme]=null, body bg=rgba(0,0,0,0)
after:  preference=light,  html[data-theme]=null, body bg=rgba(0,0,0,0)
reload: preference=light（持久化成立）
```

只观察到偏好值与刷新持久化；`html[data-theme]` 仍为 `null`、背景透明，**未取得像素级配色证据**。

### 34.5 媒体加载位置（未取得）

`full` 场景把 `note.txt` 以 `resource` 块发给 Agent 后，页面出现文件卡片
`"文件：note.txt（text/plain，18 字节） 下载文件"`，但不自动拉取字节；`clientHeight = 0`，
滚动位置断言不具代表性。图片方向：ACP `image` 块不携带 `attachmentId`，夹具只能回无 id 的
`image_ref`，页面 `data-media-state` 为空，**图片字节加载与加载时位置保持未取得证据**。

## 35. D8：Daemon 重启后首读 `session.read`

重启前创建并完成会话 `b4ca887c-2056-4c35-a34a-bb7a9d54df65`（`full`，页面 9 条、正文
"开始处理：D8 重启前。全部完成。"）。`daemon stop` 后重新 `daemon start`（实例 id 改变），
日志在重启后持续记录 `daemon.merge_window`「本轮无活动会话」，说明**没有活动 endpoint**。

新配对浏览器直开该会话 URL 后：

```text
firstRead.include = ["messages","config_options","history","pending_interactions"]
firstRead.status  = completed
firstRead.error   = null
firstRead.resourceKeys = ["messages","pending_interactions","config_options","history"]
firstRead.messages = [
  {role:"agent", status:"completed", text:"开始处理：D8 重启前。全部完成。"},
  {role:"agent", status:"completed", text:"先分析请求，再检查文件。"}
]
firstRead.historyCount = 13
firstRead.configOptions = []
页面：count=9, kinds=[turn,message,message,file-changed,tool-call,plan,tool-call,terminal-output,terminal-output],
      turn=["completed"], degraded=[]
显式 include 四项：status=completed，messageCount=2，historyCount=13
显式 include=["config_options"]：status=completed，configOptions=[]
```

页面配置区显示「模型 未提供 / 模式 未提供」，即 `config_options` 按无活动 endpoint 的实际可用情况返回空表，
而不再让整页读取失败。D8 通过。

## 36. D9：工具卡片标题取最新非空 title

`titles` 场景的事件顺序与页面输出：

```text
owned_event（会话 9cad570f-…）：
  5 tool.call.started   title="编辑 src/main.rs"
  6 tool.call.updated   title="编译 src/main.rs"
  7 tool.call.updated   title=""
  8 tool.call.completed title=""
页面：
  [data-tool-call-id="tool-title-1"] state=completed
  [data-tool-title] = "编译 src/main.rs"
  timelineCards=["turn","tool-call"]，degraded=[]
```

若取 started 标题会得到「编辑 src/main.rs」，若取 latest 值会因空串降级/不渲染；实际取组内最新非空值，
D9 通过。复现：`node d9-titles.mjs`。

## 37. 与 6.7 / 7.1 的映射（第 4 轮）

| 任务 | 本轮可引用证据 | 是否可勾选 |
| --- | --- | --- |
| 6.7 上读保持锚点、新内容/回到底部、历史加载、会话/文件往返位置 | 34.1 上读 `scrollTop` 保持 0、输出继续增长、完成后 2450 字符完整、回到底部生效；34.2 文件往返保留草稿与 `scrollTop=300`；34.3 窄屏三档、文件往返、切档与跨断点保持 `scrollTop=200`；35 历史读取 13 条（UI 翻页未覆盖）。**缺**图片媒体加载位置（34.5）与历史 UI 翻页 | **不可勾选**（缺项仍在） |
| 7.1 合入后 PV7 + 单节点真实 Daemon E1–E4/E6 | E1 部分（图片字节未取得）、E2 通过、E3 通过、E4 部分（33.4 不支持能力仍删附件/泛化错误）、E6 部分（媒体位置/像素配色）；D5 有残留（31.3） | **不可勾选** |

## 38. 第 4 轮未执行 / 未取得证据（不得计为通过）

1. 图片字节在页面上的实际加载与加载时位置保持（ACP `image` 块无 `attachmentId`，无法回引）。
2. 主题的像素级配色证据（只观察到 `data-theme-preference` 与刷新持久化）。
3. E4 的失败注入、部分失败（一个成功一个失败）、闲置 24 小时清理不误删引用。
4. 历史消息的 UI 翻页加载（本轮只验证 `session.read include:["history"]` 返回 13 条）。
5. PV7（`npm run verify` / `clients/app npm run check`）与任何 `cargo test`：本节在隔离工作区用主检出
   自建二进制做浏览器验收，不以单测结果替代；合入后的验证由主检出负责。
6. 真实 Codex / Oh My Pi（E7）、双真实节点（E5）、Windows 终端进程树（E8）。

## 39. 第 4 轮现场清理

- Daemon 以 `daemon stop` 停止，输出 `daemon 已停止（单实例锁已释放）`；最后一次停止后不再启动。
- 所有测试浏览器由 CDP `Browser.close` 关闭；未留下使用 `acpr-wp7-*` profile 的常驻 headless Edge 进程。
- 数据、日志、原始证据均在 `D:/tmp/acpr-wp7single4/`；隔离工作区只新增了本文件的这一节，
  未改产品代码、未改 `openspec` tasks、未提交。

# 第 5 轮（针对性复测）：D5 混批下标、E4 预检拒绝、D6/D8/D9 回归，及 E2/E3 快速回归
…

## 40. 第 5 轮结论先行

| 项 | 结果 | 依据 |
| --- | --- | --- |
| D5 同一合并窗口 `delta → 派生 file.changed → delta → turn.completed` | **通过** | 两段正文都并入 `agent.message.completed.content` / `owned_message.content_json`，`deltaCount = 2`，页面与 `session.read` 均为两段，`merge_window_failed = 0`（42） |
| E4 Agent 未宣告 image 时的 `session.prompt` 预检 | **部分通过（含未通过项）** | 服务端在受理前回 `rejected`（`acceptedAt = null`、无 `owned_command`、无 turn 事件、ACP 无 prompt）；但页面仍**清空附件与草稿**，错误仍是泛化的 `protocol.schema_invalid`，没有说明是图片能力缺失（43） |
| D6 跨设备 `handled_elsewhere` | **通过（回归）** | 设备 A 处理权限与表单后，B 实时呈现 `handled_elsewhere`、无表单/提交按钮，重放提交没有产生第二条 `elicitation.respond`；服务端两条 `*.resolved` 事件与 `resolved_by_id` 均在（44） |
| D8 Daemon 重启后首读 `session.read` | **通过（回归）** | 重启后（实例 id 改变）首读四项资源 `status = completed`，2 条消息、13 条历史、`config_options = []`，页面 9 条时间线、无降级（45） |
| D9 工具卡片标题取最新非空 title | **通过（回归）** | started「编辑 src/main.rs」→ updated「编译 src/main.rs」→ 空 title，页面标题仍为「编译 src/main.rs」，无 degraded（46） |
| E2 逐块流式、断线重连、停止部分正文 | **通过（快速回归）** | 30 次采样原位增长、重连后 2 个 socket、完成后正文完整；停止后 `cancelled` 且保留部分正文（47） |
| E3 首个有效回答、无效输入不消费 | **通过（快速回归）** | 无效 `count=0` 被本地校验拦下且 `elicitation.respond` 仍为 0/1；有效提交只有一次；B 端见 `handled_elsewhere`（44、47） |
| E1 / E6 | **本轮未执行** | 见 49；第 4 轮结论保持不变 |

明确失败仍是 E4 的**客户端行为**：受理前拒绝已经成立，但拒绝后用户失去附件与草稿，且错误文本无法区分
「Agent 不支持图片」与「请求形状不合法」。这不是本轮测试手法造成的（见 43.2 对一次无效尝试的说明）。

## 41. 环境与构建（第 5 轮）

| 项目 | 实际值 |
| --- | --- |
| 源码树 | 主检出 `D:/Project/acp-remote`（含 D5 `delta_plan` 下标修复与 E4 `validate_prompt` 预检修复后的工作区；隔离工作区只写本文件） |
| 前端产物 | `clients/app/dist/build-version.json`：`version = 0.0.0+71badcd-dirty`，`builtAt = 2026-10-20T02:18:02.209Z` |
| 被测二进制 | `D:/Project/acp-remote/target/debug/acp-remote.exe`，2026-10-20 10:18:17（+0800），SHA-256 `68379009ce0e25816c23090ae0be66ff7a9c293fcca1de493b12757b8746cb67` |
| Daemon 版本一致性 | `daemon status.uiBuildVersion = 0.0.0+71badcd-dirty`，与 dist 一致；`version = 0.0.0` |
| Daemon | 真实本机 Daemon，`tls.mode = "direct"`、`public_origin = https://127.0.0.1:18815`、数据目录 `D:/tmp/acpr-wp7single4/data`、独立实例 |
| Daemon 实例 id | 启动时 `c61bb0c08bee4f4e`；D8 重启后 `37d376445c8d47d9` |
| 浏览器 | 真实 Microsoft Edge，`--headless=new --ignore-certificate-errors`，CDP 驱动真实 PWA 路由与 DOM |
| ACP 子进程 | 可控夹具 `agent.mjs`（`d5mix` / `files` / `interact` / `titles` / `stream` / `stream-cancel` / `full`），不是真实 Codex/Oh My Pi |
| 证据形态 | DOM `data-*` 与文本、WebSocket 出站/入站帧、Daemon JSON 日志、SQLite 只读查询 |
| 合并窗口失败计数 | 整个 `daemon.log` 中 `daemon.merge_window_failed` 共 **0** 次（含本轮 D5 混合场景） |

本轮脚本：`D:/tmp/acpr-wp7single4/driver/r5-*.mjs`；原始 JSON 在
`D:/tmp/acpr-wp7single4/evidence/r5-*.json`（不覆盖第 1–4 轮的旧文件）。

## 42. D5：同批派生 `file.changed` 后，尾段必须进入 completed

会话 `0e2d5b6b-1a6e-4f74-8712-49868d793610`（`d5mix`，复现脚本 `node r5-d5mix.mjs`）。

真实 `owned_event`（按 `session_sequence`）：

```text
4  agent.message.delta deltaIndex="0" text="同批第一段。"
5  file.changed        （同一 tool_call 的 diff 派生）
6  tool.call.started
7  agent.message.delta deltaIndex="1" text="同批第二段。"
8  turn.completed
9  agent.message.completed content=[{"type":"text","text":"同批第一段。同批第二段。"}]
11 turn.delta_compacted  deltaCount=2
```

对应消费者可见结果：

```text
页面 [data-message-text] = "同批第一段。同批第二段。"（单条 agent 消息）
页面 turn = ["completed"]，cards = ["turn","file-changed","tool-call"]，degraded = []
owned_message.reply.state = completed，
  content_json = [{"text":"同批第一段。同批第二段。","type":"text"}]
session.read messages = [{role:"agent", status:"completed", text:"同批第一段。同批第二段。"}]
deltaCount = 2
daemon.merge_window_failed：复测前 0 → 复测后 0
```

即派生事件插入后 `deltaIndex=1` 指向**输出事件**而非输入 chunk，第二段不再丢失；第 4 轮 31.3 的
残留（completed 只有第一段、`deltaCount=1`、页面只有第一段）已不再出现。D5 通过。

## 43. E4：Agent 不支持图片时，`session.prompt` 必须在受理前被拒绝

### 43.1 通过的服务端部分

会话 `f098b97f-8947-49c5-b1ac-303725a05f42`（`files` 夹具，`promptCapabilities.image=false`）。
选择 `pixel.png`（70 字节）并输入草稿「E4 不支持图片（第 5 轮）」后发送，出站帧为：

```json
{"type":"command","body":{"command":"session.prompt","requestId":"2aadf586-f1ca-493f-ac4c-2423a711fd2f",
 "payload":{"submissionId":"5f52dd64-cc07-4c98-9de3-bf1f77fea4e3",
            "content":[{"type":"attachment_ref",...},{"type":"text",...}]}}}
```

服务端回执（真实入站帧）：

```text
type = command.result
status = rejected
acceptedAt = null
result = null
error = {code:"protocol.schema_invalid", message:"the request does not match the v1 schema", retryable:false, details:{}}
```

服务端副作用检查：

```text
owned_command：仅 session.create status=completed；**没有** session.prompt 记录
owned_event：仅 sequence 1 = command.completed；没有 turn.queued / turn.started / turn.failed
daemon.log：sync.command_rejected request_id=2aadf586-…，
            message="the mutation was rejected without any side effect"
ACP dump：D:/tmp/acpr-wp7single4/agent-files.log 发送前后均不存在（无 prompt 行）
```

即「受理前拒绝、无持久副作用、Agent 一个字节也没收到」这三条成立，第 4 轮 33.4 中
`accepted → turn.failed` 的路径已消失。

### 43.2 未通过：拒绝后清空附件与草稿，且原因不明确

同一会话发送前后（真实 DOM）：

```text
发送前：草稿 = "E4 不支持图片（第 5 轮）"
        附件 = image · 70 字节 · 待上传（点击发送后上传），attachmentCount=1
        submitEnabled=true
        提示 = "尚未取得这个 Agent 的输入能力声明；发送时服务端会再校验，可能整条被拒绝。"
发送后：draft = ""
        attachmentRows = []，attachmentCount = null
        [data-submission] = failed
        文本 = "提交失败：protocol.schema_invalid：the request does not match the v1 schema"
```

失败点：

1. **附件与草稿仍被清空**。客户端在 `#uploadAndSubmit` 派发命令后**立即**清空草稿与附件，没有等待
   `accepted`；被 `rejected` 时也没有回滚。用户既看不到原附件，也无法直接重试或改用文本重发。
   这与 spec「完整草稿保留」以及工作包判据「页面保留附件与草稿」不符。
2. **原因不明确**。错误码是 `protocol.schema_invalid`（形状错误），消息只说请求不匹配 v1 schema；
   既没有 `capability.unsupported_by_agent`，也没有指出缺的是 `image` 能力。用户无法据此知道
   该换掉图片还是改请求形状。

复现：`cd D:/tmp/acpr-wp7single4/driver && node r5-e4unsup.mjs`。

说明：本轮第一次 E4 尝试（会话 `11ee60bc-…`）在发送前用了注入式 `session.read` 手动递增
`connectionSequence`，导致随后的 `session.prompt` 被服务端以 `protocol.sequence_invalid` 拒绝
（daemon.log 中该现象仅 1 次）。那是**测试手法的副作用**，已从 E4 脚本中移除；上面 43.1/43.2 的
结论来自不带注入读取的干净运行，不应把第一次的 `pending_confirmation` 当作产品行为。

## 44. D6：设备 A 答后，设备 B 必须 `handled_elsewhere`（回归）

会话 `d3418759-9445-43c7-bd91-29a31cc8c02b`（`interact`）。

服务端事实：

```text
owned_event 5 permission.resolved  {interactionId:"c71be371-…", resolution:"allow-once", resolvedByDeviceId:null}
owned_event 8 elicitation.resolved {interactionId:"f994a0c9-…", action:"submit", resolvedByDeviceId:null}
owned_interaction：两条均 state=resolved，resolved_by_id=设备 A（25d33d91-…）
owned_command：permission.resolve completed；elicitation.respond completed（仅 1 条）
```

设备 A/B 页面：

```text
A 初始 permission：availability=not_answerable（无 requestVersion，无按钮）
A 刷新后：answerable，options=["allow-once","reject-once"]
A 允许后：handled_here（"已由本设备处理：allow-once"）
表单无效输入 count=0：错误「不能小于 1。」/「表单里有未通过校验的字段。」，elicitation.respond 未增加
有效输入 count=3,note=ok：handled_here，elicitation.respond 仍为 1
B 在 A 作答后（实时）：permission/elicitation 均 handled_elsewhere，form=null，canFill=false，无 submit 按钮
B 尝试再次提交：canFill=false，elicitation.respond 仍为 1
B 直接打开会话（A 作答前）：entries=2，kinds=["turn","permission-requested"]，卡片 answerable（证明首读自己取回）
```

D6 的「首个有效回答胜出、其它设备看到明确拒绝态且不能再填」成立。

## 45. D8：Daemon 重启后首读 `session.read`（回归）

重启前会话 `5552f759-eaaf-4c53-9cd6-7da8fb4f7411`（`full`）：页面 9 条，正文
"先分析请求，再检查文件。" 与 "开始处理：D8 重启前。处理完成。全部完成。"，turn=completed。

`daemon stop`（`daemon 已停止（单实例锁已释放）`）后重新 `daemon start`，实例 id 由
`c61bb0c08bee4f4e` 变为 `37d376445c8d47d9`。新配对浏览器直开会话后：

```text
firstRead.include = ["messages","config_options","history","pending_interactions"]
firstRead.status  = completed，error = null，readCount = 1
firstRead.resourceKeys = ["messages","pending_interactions","config_options","history"]
firstRead.messages = [
  {role:"agent", status:"completed", text:"先分析请求，再检查文件。"},
  {role:"agent", status:"completed", text:"开始处理：D8 重启前。处理完成。全部完成。"}
]
firstRead.historyCount = 13，configOptions = []
显式 include 四项：status=completed，messageCount=2，historyCount=13，configOptions=[]
显式 include=["config_options"]：status=completed，configOptions=[]
页面：count=9，
  kinds=[turn,message,message,file-changed,tool-call,plan,tool-call,terminal-output,terminal-output]，
  turn=["completed"]，degraded=[]，配置区仍为「模型 未提供 / 模式 未提供」
```

D8 通过：没有活动 endpoint 时首读仍成功，`config_options` 按实际可用返回空表而不是让整页失败。

## 46. D9：工具卡片标题取最新非空 title（回归）

会话 `2aa381b5-70d1-46dc-8d92-fd4d865f96ef`（`titles`，`node r5-d9.mjs`）：

```text
owned_event：
  5 tool.call.started   title="编辑 src/main.rs"  state=in_progress
  6 tool.call.updated   title="编译 src/main.rs" state=in_progress
  7 tool.call.updated   title=""                 state=completed
  8 tool.call.completed title=""                 state=completed
页面：
  [data-tool-call-id="tool-title-1"] state=completed
  [data-tool-title] = "编译 src/main.rs"
  timelineCards=["turn","tool-call"]，degraded=[]
```

若取 started 会得到「编辑 src/main.rs」；若取 latest 原始值会因空串降级。实际取组内最新非空值，D9 通过。

## 47. E2 / E3 快速回归

### 47.1 E2（`node r5-e2.mjs`）

```text
stream 会话 237fef55-…：30 次采样（200 ms），第 5 次采样前 close(4001)
  agent 正文长度序列 = [15,15,15,15,15,15,51,51,58,58,58,58,58,58,…]（原位增长）
  重连后 sockets = 2，未出现第三份重复正文
完成后页面正文 =
  "流式第一段：未闭合的 **强调在此继续，仍然未闭合。 ts 复制 1 const value = 1; 最后的收尾。"
session.read 权威正文 = 同文本（含 ```ts 代码块换行），status=completed
turn = ["completed"]，degraded = []

slow 会话 63490c11-…：
停止前 turn=["running"]，正文 "这是停止前的第一段，"
停止后 turn=["cancelled"]，页面保留 "先想一下这一段。" 与 "这是停止前的第一段，"
session.read：status="cancelled"，partial={"state":"cancelled","reason":"turn_cancelled"}
degraded = []
```

### 47.2 E3（`node r5-e3.mjs`）

```text
无效输入 count=0：错误「不能小于 1。」/「表单里有未通过校验的字段。」；elicitation.respond 未增加
有效输入 count=3,note=ok：elicitation.respond=completed（共 1 条）
B 端随后：permission/elicitation 均 handled_elsewhere，form=null，无输入框/提交按钮
```

与 44 的 D6 证据同源；E2/E3 的既有结论未被本轮修复破坏。

## 48. 与 6.7 / 7.1 的映射（第 5 轮）

| 任务 | 本轮可引用证据 | 是否可勾选 |
| --- | --- | --- |
| 6.7 上读保持锚点、新内容/回到底部、历史加载、会话/文件往返位置 | 本轮**未重跑**滚动/历史翻页（42–47 不覆盖滚动位置）；仍只能引用第 4 轮 34.1–34.3 的 `scrollTop` 证据，且图片媒体加载位置与历史 UI 翻页仍缺 | **不可勾选** |
| 7.1 合入后 PV7 + 单节点真实 Daemon E1–E4/E6 | D5 通过（42）、E2 通过（47.1）、E3 通过（47.2）、D6/D8/D9 通过（44–46）；但 E4 仍失败于「保留附件与草稿 + 明确原因」（43.2），E1/E6 本轮未重跑 | **不可勾选** |

## 49. 第 5 轮未执行 / 未取得证据（不得计为通过）

1. E1 内容类型全套（尤其图片字节在页面上的实际加载）与 E6 滚动/主题/媒体位置：本轮只做针对性复测，未重跑。
2. E4 的失败注入、部分失败（一个附件成功一个失败）、闲置 24 小时清理不误删引用。
3. E4 在「客户端已取得 `capabilities` 资源」时的前端闸门（本轮观察到会话首读没有申请 `capabilities`，
   因此页面确实处于「能力未知」分支；该分支下服务端拒绝成立，但客户端保留/提示不合格）。
4. 历史消息的 UI 翻页加载。
5. PV7（`npm run verify` / `clients/app npm run check`）与 `cargo test`：本轮在隔离工作区用主检出
   二进制做浏览器验收，不以单测结果替代；合入后的验证由主检出负责。
6. 真实 Codex / Oh My Pi（E7）、双真实节点（E5）、Windows 终端进程树（E8）。

## 50. 第 5 轮现场清理

- Daemon 以 `daemon stop` 停止，输出 `daemon 已停止（单实例锁已释放）`；停止后端口 18815 无 LISTENING。
- 所有测试浏览器由 CDP `Browser.close` 关闭；检查 `msedge.exe` 命令行含 `acpr-wp7-` 的进程数为 0。
- 数据、日志、原始证据均在 `D:/tmp/acpr-wp7single4/`（`driver/r5-*.mjs`、`evidence/r5-*.json`）；
  隔离工作区只新增了本文件的第 5 轮，未改产品代码、未改 `openspec` tasks、未提交。

# 第 6 轮（E4 定向复测）：拒绝后草稿/附件保留与错误文案、D5 混批、E2 回归

## 51. 第 6 轮结论先行

| 项 | 结果 | 依据 |
| --- | --- | --- |
| E4 受理前拒绝（无 `acceptedAt`、无 turn、ACP 零 prompt） | **通过** | `session.prompt` 回 `rejected`、`acceptedAt = null`；`owned_command` 无 prompt 行、`owned_event` 只有 `command.completed`；ACP dump 的 prompt 数 0 → 0（53.1） |
| E4 拒绝后页面保留草稿与附件 | **通过（已修复）** | 失败后 `draft` 恢复为「E4 不支持图片（第 6 轮）」、`attachmentCount = "1"`、图片行回到 `sealed`、发送仍可用（53.2） |
| E4 错误文案明确为不支持/能力缺失 | **未通过** | 实际错误是 `internal.unavailable` / `the command cannot be served`（`retryable = true`），既不是 `command.unsupported`，也不是 `capability.unsupported_by_agent`，且没有提到 `image` 能力；只是不再是 `protocol.schema_invalid`（53.3） |
| D5 同批 `delta → file.changed → delta → turn.completed` | **通过（回归）** | 页面、`agent.message.completed`、`owned_message`、`session.read` 均为两段；`deltaCount = 2`；`merge_window_failed = 0`（54） |
| E2 流式/断线重连/停止部分正文 | **通过（回归）** | 30 次采样原位增长（15 → 51 → 58）、重连后 2 个 socket、完成后正文完整；停止后 `cancelled` 且保留部分正文（55） |
| E1 / E3 / E6 / D6 / D8 / D9 | **本轮未执行** | 见 57；第 4、5 轮结论保持不变 |

第 5 轮 E4 的两个失败点中，**客户端保留草稿与附件已修复**；**错误语义仍不合格**。根因不在客户端：
`crates/agent-host/src/error.rs` 已把 `CapabilityNotDeclared` 映射为
`PortError::Unavailable(BackendUnsupported)`，但 `crates/server/src/sync/command/mod.rs` 的
`port_rejection`（约 2175–2205 行）没有 `Unavailable(BackendUnsupported)` 分支，落进 `_ =>
(ErrorCode::InternalUnavailable, "the command cannot be served")`。因此 wire 上是
`internal.unavailable`，并被标记为可重试（53.1 给出原始帧）。

## 52. 环境与构建（第 6 轮）

| 项目 | 实际值 |
| --- | --- |
| 源码树 | 主检出 `D:/Project/acp-remote`（含客户端草稿/附件快照恢复与 agent-host `BackendUnsupported` 映射后的工作区）；隔离工作区只写本文件 |
| 前端产物 | `clients/app/dist/build-version.json`：`version = 0.0.0+71badcd-dirty`，`builtAt = 2026-10-20T02:26:17.902Z`；`GET /build-version.json` 返回同一 `builtAt`（证明托管的是本轮产物） |
| 被测二进制 | `D:/Project/acp-remote/target/debug/acp-remote.exe`，2026-10-20 10:26:24（+0800），SHA-256 `a36aecc63024559cd2f1c3999d81524844b69d46b7a7f907a4ab1745b92e8fdf`（晚于 dist，嵌入的是本轮前端） |
| Daemon 版本一致性 | `daemon status.uiBuildVersion = 0.0.0+71badcd-dirty`；`version = 0.0.0` |
| Daemon | 真实本机 Daemon，`tls.mode = "direct"`、`public_origin = https://127.0.0.1:18815`、数据目录 `D:/tmp/acpr-wp7single4/data`、独立实例 |
| Daemon 实例 id | `ddfb00745dac45ee`（`startedAt = 2026-10-20T02:27:29.164Z`） |
| 浏览器 | 真实 Microsoft Edge，`--headless=new --ignore-certificate-errors`，CDP 驱动真实 PWA 路由与 DOM |
| ACP 子进程 | 可控夹具 `agent.mjs`（`files` / `d5mix` / `stream` / `stream-cancel`），不是真实 Codex/Oh My Pi |
| 证据形态 | DOM `data-*` 与文本、WebSocket 出站/入站帧、Daemon JSON 日志、SQLite 只读查询、ACP dump |
| 合并窗口失败计数 | 整个 `daemon.log` 中 `daemon.merge_window_failed` 共 **0** 次 |

本轮脚本：`D:/tmp/acpr-wp7single4/driver/r6-*.mjs`；原始 JSON 在
`D:/tmp/acpr-wp7single4/evidence/r6-*.json`（不覆盖第 1–5 轮的旧文件）。

## 53. E4：不支持图片时受理前拒绝（定向复测）

会话 `e350f874-cb89-48df-ba66-64b68eb54326`（`files` 夹具，`promptCapabilities.image=false`）。

### 53.1 受理前拒绝与零副作用

发送前页面处于「能力未知」分支（提交按钮可用），因此这次确实走到了服务端预检：

```text
draft = "E4 不支持图片（第 6 轮）"
attachmentCount = "1"，图片 · 70 字节 · 待上传
submitEnabled = true
提示 = "尚未取得这个 Agent 的输入能力声明；发送时服务端会再校验，可能整条被拒绝。"
```

出站帧（`session.prompt`，`content = ["attachment_ref","text"]`，
`submissionId = 3a19c2cf-d035-4063-bd27-8ba4d53aec9a`）之后，真实入站帧：

```text
type          = command.result
command       = session.prompt
status        = rejected
acceptedAt    = null
result        = null
error.code    = internal.unavailable
error.message = "the command cannot be served"
retryable     = true
details       = {}
```

服务端副作用检查（同一会话）：

```text
owned_command：仅 session.create status=completed；没有 session.prompt 记录
owned_event：仅 sequence 1 = command.completed；没有 turn.queued / turn.started / turn.failed
daemon.log：sync.command_rejected request_id=fee8ed39-d16c-4d0b-b185-1975423a6ad7
            code=internal.unavailable
ACP dump（agent-files.log）：发送前后均不存在（dump 大小 0，prompt 行 0 → 0，即未收到任何 prompt）
```

「受理前拒绝、无持久副作用、Agent 一个字节也没收到」三条成立；第 4 轮 33.4 的
`accepted → turn.failed` 路径没有出现。

### 53.2 拒绝后草稿与附件已恢复（修复生效）

同一会话在收到 `rejected` 后的真实 DOM：

```text
draft = "E4 不支持图片（第 6 轮）"
[data-submission] = failed
文本 = "提交失败：internal.unavailable：the command cannot be served"
attachmentCount = "1"
附件行 = image · 70 字节 · sealed（"已封存（整条消息发送前不会执行）"）
submitEnabled = true（可以改文案重发，也可以换掉图片）
```

对比第 5 轮 43.2（`draft = ""`、`attachmentCount = null`），客户端的
`#promptDrafts` 快照恢复在 `rejected` 路径上确实执行了：草稿、图片与路径引用都回到派发前，
且同一会话没有出现重复附件（`data-attachment-count = "1"`）。

### 53.3 未通过：错误文案仍不表明「不支持 / 能力缺失」

用户看到的整句是：

```text
提交失败：internal.unavailable：the command cannot be served
```

- 不是 `protocol.schema_invalid`（第 5 轮的失败点已消除），但也不是 `command.unsupported`
  或 `capability.unsupported_by_agent`；
- 文案没有说明是「Agent 未宣告 `image` 能力」，只说「命令无法被服务」；
- `retryable = true` 会诱导用户用同一张图片重试，而结果必然再次被拒。

来源与 wire 一致：agent-host 的 `CapabilityNotDeclared` 已映射为 `BackendUnsupported`，
但 Sync 拒绝装配的 `port_rejection` 没有把 `Unavailable(BackendUnsupported)` 翻译成
`command.unsupported`，而是落入 `_` 兜底 `internal.unavailable`。因此判据
「错误文案明确为不支持/能力缺失」**未通过**。

（附：本轮 E2 第一次采样脚本把「首条 agent 消息」误当成流式消息，导致两条断言误报；
修正断言后按第二次运行记录 55。E4 与 D5 均一次运行成功。）

## 54. D5：同批派生 `file.changed` 后尾段进入 completed（回归）

会话 `21437eb8-27c9-474a-b3ae-61fc56bade59`（`d5mix`）。真实 `owned_event`：

```text
2  turn.queued
3  turn.started
4  agent.message.delta deltaIndex="0" text="同批第一段。"
5  file.changed        （同一 tool_call 的 diff 派生）
6  tool.call.started
7  agent.message.delta deltaIndex="1" text="同批第二段。"
8  turn.completed
9  agent.message.completed content=[{"type":"text","text":"同批第一段。同批第二段。"}]
11 turn.delta_compacted  deltaCount=2
```

消费者可见结果：

```text
页面 [data-message-text] = "同批第一段。同批第二段。"（单条 agent 消息）
页面 turn = ["completed"]，cards = ["turn","file-changed","tool-call"]，degraded = []
owned_message.reply.state = completed，
  content_json = [{"text":"同批第一段。同批第二段。","type":"text"}]
session.read messages = [{role:"agent", status:"completed", text:"同批第一段。同批第二段。"}]
deltaCount = 2
daemon.merge_window_failed：本轮前 0 → 本轮后 0
```

脚本 `r6-d5mix.mjs` 的布尔检查（两段 delta、`deltaIndexes = ["0","1"]`、
completed/owned_message/页面/read 均含两段、`deltaCount = 2`、无合并窗口失败、无 degraded）
全部为 `true`。第 4 轮 31.3 的
残留与第 5 轮的修复结论保持一致。

## 55. E2：流式、断线重连、停止部分正文（快速回归）

### 55.1 流式与 WSS 中断（会话 `604a6dc7-8b41-45ed-a3b9-90237ac8b3e3`）

```text
30 次采样（200 ms），第 5 次采样前 close(4001)
同一 messageId 的正文长度序列（只取已出现的采样）= 15,15,15,15,15,15,51,51,51,58,58,…
重连后 sync socket 数 = 2；流式 messageId 在采样中始终至多 1 条（无重复正文）
完成后页面正文 =
  "流式第一段：未闭合的 **强调在此继续，仍然未闭合。 ts 复制 1 const value = 1; 最后的收尾。"
session.read 权威正文 = 同文本（含 ```ts 代码块与换行），status = completed
turn = ["completed"]，degraded = []
```

（完成时夹具另发一条 thought 正文「先想一下这一段。」，页面按 `thought` 渠道展示，不与
流式 agent 消息混为一条。）

### 55.2 停止保留部分正文（会话 `11610c0f-875c-4ce9-b80a-46a5e2e579b4`）

```text
停止前 turn = ["running"]，正文 "这是停止前的第一段，"
停止后 turn = ["cancelled"]，页面保留 "先想一下这一段。" 与 "这是停止前的第一段，"
session.read：status = "cancelled"，partial = {"state":"cancelled","reason":"turn_cancelled"}
degraded = []
```

脚本 `r6-e2.mjs` 的 12 项检查（采样数、原位单调增长、流式消息唯一、重连 socket 数、
完成/取消终态、尾段落地、权威正文完整、无降级）全部为 `true`。

## 56. 与 6.7 / 7.1 的映射（第 6 轮）

| 任务 | 本轮可引用证据 | 是否可勾选 |
| --- | --- | --- |
| 6.7 上读保持锚点、新内容/回到底部、历史加载、会话/文件往返位置 | 本轮**未重跑**滚动/历史翻页；仍只能引用第 4 轮 34.1–34.3 的 `scrollTop` 证据，且图片媒体加载位置与历史 UI 翻页仍缺 | **不可勾选** |
| 7.1 合入后 PV7 + 单节点真实 Daemon E1–E4/E6 | D5 通过（54）、E2 通过（55）；E4 的服务端受理前拒绝与客户端保留草稿/附件均通过（53.1、53.2），但错误文案仍为泛化的 `internal.unavailable` 且可重试（53.3），E1/E6 本轮未重跑 | **不可勾选** |

## 57. 第 6 轮未执行 / 未取得证据（不得计为通过）

1. E1 内容类型全套（尤其图片字节在页面上的实际加载）与 E6 滚动/主题/媒体位置：本轮只做针对性复测。
2. E3、D6、D8、D9：第 5 轮已回归通过，本轮未重跑。
3. E4 的失败注入、部分失败（一个附件成功一个失败）、闲置 24 小时清理不误删引用。
4. E4 在客户端已取得 `capabilities` 资源时的前端闸门（本轮仍走「能力未知 + 服务端预检」分支）。
5. 历史消息的 UI 翻页加载。
6. PV7（`npm run verify` / `clients/app npm run check`）与 `cargo test`：本轮在隔离工作区用主检出
   二进制做浏览器验收，不以单测结果替代；合入后的验证由主检出负责。
7. 真实 Codex / Oh My Pi（E7）、双真实节点（E5）、Windows 终端进程树（E8）。

## 58. 第 6 轮现场清理

- Daemon 以 `daemon stop` 停止，输出 `daemon 已停止（单实例锁已释放）`；停止后端口 18815 无 LISTENING
  （仅 TIME_WAIT），`data/daemon.instance.json` 已移除。
- 所有测试浏览器由 CDP `Browser.close` 关闭；检查 `msedge.exe` 命令行含 `acpr-wp7-` 的进程数为 0。
- 数据、日志、原始证据均在 `D:/tmp/acpr-wp7single4/`（`driver/r6-*.mjs`、`evidence/r6-*.json`）；
  隔离工作区只新增了本文件的第 6 轮，未改产品代码、未改 `openspec` tasks、未提交。

# 第 7 轮（E4 错误码定向）：`command.unsupported`、受理前拒绝与草稿保留

## 59. 第 7 轮结论先行

| 项 | 结果 | 依据 |
| --- | --- | --- |
| 错误码为明确的「Agent 不支持」 | **通过（本轮修复生效）** | `command.result.status = rejected`，`error.code = "command.unsupported"`、`retryable = false`、`message = "the agent does not support this command"`；不是 `internal.unavailable`，也不是 `protocol.schema_invalid`（61.2） |
| 用户在页面上看到该原因 | **通过** | 提交失败行原文 `提交失败：command.unsupported：the agent does not support this command`，同时带出码与原因（61.3） |
| 受理前拒绝、零副作用 | **通过（回归）** | `acceptedAt = null`、`result = null`；`owned_command` 只有 `session.create`；`owned_event` 只有 `command.completed`；`daemon.log` 的 `sync.command_rejected` 记录 `code=command.unsupported` 且注明「无副作用」（61.2） |
| 拒绝后草稿与附件保留 | **通过（回归）** | `draft = "E4 不支持图片（第 7 轮）"`、`attachmentCount = "1"`、图片行回到 `sealed`、`submitEnabled = true`（61.3） |
| ACP 侧零 prompt | **通过（回归）** | `agent-files.log` 不存在：dump 大小 0 → 0、prompt 行 0 → 0（61.2） |
| 其他场景 | **本轮未执行** | 本轮只复测 E4 错误码/文案；D5、E2、E1、E3、E6、D6、D8、D9 的第 4–6 轮结论保持不变 |

第 6 轮 53.3 的唯一失败点（`internal.unavailable`）在本轮的新构建上已消除；四组检查一次运行全绿。

## 60. 环境与构建（第 7 轮）

| 项目 | 实际值 |
| --- | --- |
| 源码树 | 主检出 `D:/Project/acp-remote`（含 `server::sync::command::port_rejection` 的 `Unavailable(BackendUnsupported) → command.unsupported` 映射后的工作区）；隔离工作区只写本文件 |
| 被测二进制 | `D:/Project/acp-remote/target/debug/acp-remote.exe`，2026-10-20 10:31（+0800），SHA-256 `ebd71a6f1edd12fc0a52e3cb87ec2d68947538b9dca208a6a4079497657967ae` |
| 前端产物 | `clients/app/dist/build-version.json`：`version = 0.0.0+71badcd-dirty`，`builtAt = 2026-10-20T02:26:17.902Z`（本轮前端未改） |
| Daemon 版本一致性 | `daemon status.uiBuildVersion = 0.0.0+71badcd-dirty`，与 dist 一致；`version = 0.0.0` |
| Daemon | 真实本机 Daemon，`tls.mode = "direct"`、`public_origin = https://127.0.0.1:18815`、数据目录 `D:/tmp/acpr-wp7single4/data`；`instanceId = e08d61b95c8b498b`，`startedAt = 2026-10-20T02:33:00.996Z` |
| 浏览器 | 真实 Microsoft Edge，`--headless=new --ignore-certificate-errors`，CDP 驱动真实 PWA 路由与 DOM（profile 标签 `e4pre7`） |
| ACP 子进程 | 可控夹具 `agent.mjs --scenario files`（`promptCapabilities.image = false`），不是真实 Codex/Oh My Pi |
| 脚本 / 原始证据 | `driver/r7-e4unsup.mjs`；`evidence/r7-e4unsup.json`、`evidence/r7-e4unsup.stdout.txt` |

本轮没有构建动作：命令路径的改动由主检出完成 `cargo build -p app`，复测直接使用上表的二进制。

## 61. E4：图片附件在受理前被 `command.unsupported` 拒绝

会话 `e57f90fb-b9c0-4b34-a837-62afba93410d`（`files` 夹具，`promptCapabilities.image=false`），
复现：`cd D:/tmp/acpr-wp7single4/driver && D:/Application/Node/nodejs/node.exe r7-e4unsup.mjs`。

### 61.1 发送前的页面状态（服务端预检路径确实被走到）

```text
draft          = "E4 不支持图片（第 7 轮）"
attachmentCount = "1"（图片 · 70 字节 · 待上传）
submitEnabled  = true
提示            = "尚未取得这个 Agent 的输入能力声明；发送时服务端会再校验，可能整条被拒绝。"
```

客户端没有 Agent 能力声明，因此提交按钮可用、请求真的发往服务端；下面拒绝来自服务端预检，而不是页面本地的能力拦截。

### 61.2 Wire、持久层与 ACP：拒绝且零副作用

出站帧（`session.prompt`，`content = ["attachment_ref","text"]`，
`submissionId = 675d9567-18f9-41b5-8e04-aa463192da11`，`requestId = 327cc929-1273-4a77-86fb-b93ed3f5d94f`）
的真实入站帧：

```text
type          = command.result
command       = session.prompt
status        = rejected
acceptedAt    = null
result        = null
error.code    = command.unsupported
error.message = "the agent does not support this command"
retryable     = false
details       = {}
```

同一会话的服务端事实：

```text
owned_command：仅 session.create status=completed；没有 session.prompt 记录
owned_event  ：仅 sequence 1 = command.completed；没有 turn.queued / turn.started / turn.failed
daemon.log   ：sync.command_rejected request_id=327cc929-1273-4a77-86fb-b93ed3f5d94f
               code=command.unsupported，"the mutation was rejected without any side effect"
ACP dump     ：agent-files.log 不存在（dump 大小 0 → 0，prompt 行 0 → 0）
```

拒绝发生在任何派发之前：没有命令记录、没有 turn 事件、Agent 没有收到 prompt。
registry（`compatibility/errors/v1/errors.json`）对 `command.unsupported` 的语义是
`retryable = false`，与入站帧一致——「Agent 不支持图片」是永久失败，不再诱导用户用同一张图片重试。
（`capability.unsupported_by_agent` 也是 registry 中的既有码，但本轮实现选择并返回的是
`command.unsupported`；按本轮判据「`command.unsupported` 或等价明确码」，判为通过。）

### 61.3 拒绝后页面保留草稿与附件，并显示明确原因

同一会话收到 `rejected` 之后的真实 DOM：

```text
draft            = "E4 不支持图片（第 7 轮）"
[data-submission] = failed
文本              = "提交失败：command.unsupported：the agent does not support this command"
attachmentCount   = "1"
附件行            = image · 70 字节 · sealed（"已封存（整条消息发送前不会执行）"）
submitEnabled     = true
```

整句同时包含明确的码与原因，且不含 `internal.unavailable` 或 `schema_invalid`；
草稿与附件都保留，用户可以改文案或换掉图片后重发，不会丢掉已选文件。

### 61.4 检查清单（脚本输出 `checks`，全部为 true）

```text
dispatchedBecauseCapabilityUnknown, rejectedNotAccepted, rejectedHasNoAcceptedAt,
rejectedErrorCodeUnsupported, rejectedErrorIsPermanent,
rejectedErrorMessageNamesAgentUnsupported, rejectedErrorNotUnavailable,
rejectedErrorNotSchemaInvalid, submissionTextShowsUnsupportedCode,
submissionTextShowsReason, submissionTextNotUnavailable,
submissionTextNotSchemaInvalid, noPromptCommandRow, noTurnQueuedEvent,
noTurnStartedEvent, noTurnFailedEvent, attachmentRetained,
attachmentCountRetained, draftRetained, submissionShowsFailure,
composerStillSubmittable, acpSawNoPrompt
```

判据映射：7.1「Agent 不支持的能力必须在受理前被拒绝，并给出明确原因」——本轮通过。

## 62. 第 7 轮现场清理

- Daemon 以 `daemon stop` 停止，输出 `daemon 已停止（单实例锁已释放）`；停止后端口 18815 无 LISTENING
  （仅 TIME_WAIT），`data/daemon.instance.json` 已移除。
- 测试浏览器由 CDP `Browser.close` 关闭；检查 `msedge.exe` 命令行含 `acpr-wp7-` 的进程数为 0。
- 数据、日志、原始证据均在 `D:/tmp/acpr-wp7single4/`（`driver/r7-e4unsup.mjs`、
  `evidence/r7-e4unsup.json`、`evidence/r7-e4unsup.stdout.txt`）；
  隔离工作区只新增了本文件的第 7 轮，未改产品代码、未改 `openspec` tasks、未提交。

# 第 8 轮（E1 图片/音频定向）：多块图片读取失败，E1/E6 未取得通过证据

## 63. 第 8 轮结论先行

| 项 | 结果 | 依据 |
| --- | --- | --- |
| 图片/音频附件上传、封存与 ACP 派发 | **通过（回归）** | `attachment.begin` 的 `mediaType` 分别为 `image/png`、`audio/wav`；`attachment.sealed` 给出 `0a3aad85-…`、`f3cb5492-…`；Agent 收到 `image`/`audio`/`text` 原生块（63.2） |
| 历史读取保留媒体引用 | **通过** | `session.read` 的 Agent 消息含 `image_ref {attachmentId=0a3aad85-…, displayState=available, byteLength=219661}` 与 `audio_ref {attachmentId=f3cb5492-…, byteLength=4044}`，`status=completed` |
| 附件连接为独立已认证 WSS | **通过** | `/sync/v1/attachments` 上 `auth.client_hello → auth.server_challenge（selectedFeatures 含 core.attachment-transfer.v1）→ auth.client_proof → auth.authenticated` |
| 单块音频渲染 | **通过（部分）** | 音频块 `data-media-state=ready`，状态机不再报「未就绪」 |
| 多块图片渲染（`<img>` + 正确字节 + blob:） | **失败** | 图片块 `data-media-state=failed`，文案「读取失败：读取偏移超过附件总长度」；有序帧显示同一 attachmentId 出现两次 offset=0 的并发读取，客户端把重复块再次累加，第 7 个 32 KiB 块后 offset=229376 > totalLength=219661（63.3） |
| 刷新恢复媒体 / 加载时位置保持 / Blob 回收 / 历史翻页 / 回到底部 | **未取得** | E1 脚本在「等待两个媒体块 ready」处中止；E6 分页脚本未运行 |
| 6.7 可勾选 | **不可勾选** | 图片字节加载仍未成立，媒体加载位置与历史 UI 翻页无成功证据 |

## 64. 环境与构建（第 8 轮）

| 项目 | 实际值 |
| --- | --- |
| 源码树 | 只读使用主检出 `D:/Project/acp-remote` 的二进制与前端产物；隔离工作区只写本文件 |
| 被测二进制 | `D:/Project/acp-remote/target/debug/acp-remote.exe`，2026-10-20 10:31（+0800），SHA-256 `ebd71a6f1edd12fc0a52e3cb87ec2d68947538b9dca208a6a4079497657967ae` |
| 前端产物 | `clients/app/dist/build-version.json`：`version = 0.0.0+71badcd-dirty`，`builtAt = 2026-10-20T02:26:17.902Z`；Daemon `status.uiBuildVersion` 与之一致 |
| Daemon | 真实本机 Daemon，`tls.mode=direct`、`public_origin=https://127.0.0.1:18847`、数据目录 `D:/tmp/acpr-wp7e1e6/data`，`instanceId=132ec66f23334fbd` |
| 浏览器 | 真实 Microsoft Edge，`--headless=new --ignore-certificate-errors`，CDP 驱动真实 PWA 路由与 DOM |
| ACP 子进程 | 可控夹具 `D:/tmp/acpr-wp7e1e6/agent/agent.mjs --scenario media`（声明 image/audio/embeddedContext），不是真实 Codex/Oh My Pi；媒体场景在收到 prompt 后读取驱动写下的附件标识，再回显 `image_ref`/`audio_ref` |
| 媒体文件 | `tall.png` 219,661 字节 SHA-256 `7b141002…6bbd55`（多块失败用例）；`tall-small.png` 160×400、842 字节 SHA-256 `179dccdf…d69ea9`；`tone.wav` 4,044 字节 SHA-256 `f8a07604…c84669` |

会话 `014ba358-a064-4b1e-92a1-21aaa1871c57`（`media` 夹具）；附件 `0a3aad85-9d22-4974-93e7-b393d02e15c4`（image/png）、
`f3cb5492-a222-4c98-85b0-d3ab129e043c`（audio/wav）。

## 65. 已取得的具体事实

### 65.1 附件连接认证与上传封存

```text
socket wss://127.0.0.1:18847/sync/v1/attachments
sent  auth.client_hello, auth.client_proof
recv  auth.server_challenge, auth.authenticated
挑战 selectedFeatures 含 core.attachment-transfer.v1
attachment.begin  mediaType=image/png, totalLength=219661
attachment.begin  mediaType=audio/wav, totalLength=4044
attachment.sealed attachmentId=0a3aad85-9d22-4974-93e7-b393d02e15c4 length=219661
attachment.sealed attachmentId=f3cb5492-a222-4c98-85b0-d3ab129e043c length=4044
```

### 65.2 历史读取中的类型化媒体

`session.read`（`include=["messages"]`）对同一会话返回 `status=completed`，媒体块为：

```text
{"role":"agent","type":"image_ref","attachmentId":"0a3aad85-9d22-4974-93e7-b393d02e15c4",
 "displayState":"available","mimeType":"image/png","byteLength":"219661"}
{"role":"agent","type":"audio_ref","attachmentId":"f3cb5492-a222-4c98-85b0-d3ab129e043c",
 "displayState":"available","mimeType":"audio/wav","byteLength":"4044"}
```

### 65.3 图片读取失败的有序帧（复现核心）

复现：`cd D:/tmp/acpr-wp7e1e6/driver && node probe-chunks.mjs`。附件 socket 上的真实帧序列（节选，保持到达顺序）：

```text
sent attachment.read  image offset=0     maxLength=32768
sent attachment.read  audio offset=0     maxLength=32768
sent attachment.read  image offset=0     maxLength=32768     ← 同一 attachment 的第二次并发读取
recv read_chunk image offset=0     length=32768 totalLength=219661 eof=false
sent attachment.read  image offset=32768
recv read_chunk audio offset=0     length=4044  totalLength=4044   eof=true
recv read_chunk image offset=0     length=32768 totalLength=219661 eof=false   ← 重复块被第二次计入
sent attachment.read  image offset=65536
...
sent attachment.read  image offset=196608
recv read_chunk image offset=196608 length=23053 totalLength=219661 eof=true
```

页面结果：

```text
image: state=failed  text="读取失败：读取偏移超过附件总长度"
audio: state=ready
```

`clients/app/src/features/client-store.ts` 的读取循环按 `length` 累加 `offset`，只按 `attachmentId` 等待响应；
同一附件存在两条并发读取时，重复的 `offset=0` 块会再次累加，第 7 个 32 KiB 块后
`offset = 229376 > totalLength = 219661`，触发「读取偏移超过附件总长度」。[INFERENCE] 两条并发读取来自
同一媒体块被多个响应式变体/重复挂载各调用一次 `onLoad`；本轮的 DOM 观测到同一会话页面存在多份
`[data-conversation-scroll="timeline"]` 与重复的 `[data-media-block]`（另一轮诊断中一条消息对应 4 个媒体块），
但未逐一定位到具体节点。

单块音频（4,044 字节，一次读取即 `eof=true`）不受该缺陷影响，状态到 `ready`。
多块图片的 `<img>`、字节摘要、Blob 生命周期与加载位置因此都无法在本轮取得。

## 66. 第 8 轮未执行 / 未取得证据（不得计为通过）

1. 图片 `<img src="blob:…">`、图片字节 SHA-256/尺寸、`blob:` 前缀与附件读取字节的重放摘要（被 65.3 阻断）。
2. 刷新后媒体恢复、离开会话后的 Blob 回收（脚本已就绪，未通过前置条件）。
3. E6 媒体加载时上读位置保持、跨断点位置、历史消息/结构化历史翻页、新内容不强制滚底与回到底部
   （`e6-paging.mjs` 未运行）。
4. E2/E3/E4/E7/E8 与 PV7：本轮未重跑，沿用既有轮次结论。
5. 真实 Codex / Oh My Pi（E7）、双真实节点（E5）。

## 67. 第 8 轮现场清理

- Daemon 以 `daemon stop` 停止，输出 `daemon 已停止（单实例锁已释放）`；端口 18847 无 LISTENING，
  `data/daemon.instance.json` 已移除。
- 所有测试浏览器由 CDP `Browser.close` 关闭；`msedge.exe` 命令行含 `acpr-wp7` 的进程数为 0。
- 数据、日志、脚本与原始证据在 `D:/tmp/acpr-wp7e1e6/`（`driver/probe-chunks.mjs`、`driver/e1-media.mjs`、
  `driver/e6-paging.mjs`、`evidence/probe-chunks.stdout.txt`、`evidence/e1-media.json`）；
  隔离工作区只新增本文件的第 8 轮，未改产品代码、未改 `openspec` tasks、未提交。

# 第 9 轮（E1 多块图片定向复测）：并发读取消失，图片/音频字节正确

## 68. 第 9 轮结论先行

| 项 | 结果 | 依据 |
| --- | --- | --- |
| 多块图片（`tall.png`，219,661 字节）读取无并发 `offset=0` | **通过** | 两代读取的有序帧各自只有一次 `offset=0`；任一时刻 `in-flight` 均为 1（§70.2） |
| 图片 `<img>`、字节数与偏移 | **通过** | 块状态 `ready`，`src` 为 `blob:`，Blob 为 219,661 字节；重建字节 SHA-256 与源文件逐字一致；偏移 0→196608、块长之和 219,661（§70.3） |
| 音频仍正常 | **通过（读取与字节）** | `tone.wav` 4,044 字节，块状态 `ready`、`blob:`，重建 SHA-256 一致；未断言实际播放（截图时 `readyState=0`，见 §70.4） |
| 刷新后媒体恢复 | **未通过（新观察）** | 刷新后媒体块立即进入 `failed`：「附件连接尚未就绪（可能未协商 … 或正在重连）」；本项不属于本轮去重目标，未计入通过（§70.5） |
| 回归用例 | **已新增，未在本隔离工作区执行** | `attachment-upload.test.ts` 的代次串行用例；执行由主检出负责（§71） |
| 6.7 / 7.1 | 仍不可勾选 | 媒体加载位置、历史翻页、刷新媒体未取得通过证据 |

## 69. 环境与构建（第 9 轮）

| 项目 | 实际值 |
| --- | --- |
| 源码树 | 执行使用主检出 `D:/Project/acp-remote` 的已构建二进制与前端产物；隔离工作区只写测试与证据 |
| 被测二进制 | `D:/Project/acp-remote/target/debug/acp-remote.exe`，2026-10-20 11:48（+0800），SHA-256 `7a8f3533c4b3d32598890e3149dca73245968f019683c13eb822046d2224ca83` |
| 前端产物 | `clients/app/dist/build-version.json`：`version = 0.0.0+71badcd-dirty`，`builtAt = 2026-10-20T03:47:57.571Z`；Daemon `status.uiBuildVersion` 与之一致 |
| Daemon | 真实本机 Daemon，端口 18857，数据目录 `D:/tmp/acpr-wp7final/e1/data`，`instanceId=f9fcdbea10c44ec8`，`nodeId=dd3c9b20-513d-8097-807c-50e3c4c51621` |
| 浏览器 | 真实 Microsoft Edge（`--headless=new --ignore-certificate-errors`），CDP 驱动 PWA 路由与 DOM |
| ACP 子进程 | 可控夹具 `D:/tmp/acpr-wp7final/e1/agent/agent.mjs --scenario media`（声明 image/audio），不是真实 Codex/Oh My Pi |
| 媒体文件 | `tall.png` 219,661 字节 SHA-256 `7b141002…6bbd55`；`tone.wav` 4,044 字节 SHA-256 `f8a07604…c84669` |
| 会话 / 附件 | 会话 `73b023dc-a6a8-460c-b4f3-323dd2d2a300`；图片 `03ed2c81-7b08-4a93-8543-9c0dc2a10df4`，音频 `58622720-fd29-4c42-917f-df5f6ab1a238` |
| 驱动与原始证据 | `D:/tmp/acpr-wp7final/e1/driver/e1-final.mjs`、`evidence/e1-final.json`、`evidence/e1-final.stdout.txt` |

## 70. E1：多块图片读取的具体事实

### 70.1 上传、封存与媒体引用

```text
attachment.begin  mediaType=image/png  totalLength=219661
attachment.begin  mediaType=audio/wav  totalLength=4044
attachment.sealed attachmentId=03ed2c81-7b08-4a93-8543-9c0dc2a10df4 length=219661
attachment.sealed attachmentId=58622720-fd29-4c42-917f-df5f6ab1a238 length=4044
```

页面消息：`媒体回显：E1 多块图片去重复测（第 9 轮） 媒体回显完成。`

### 70.2 有序读取帧：两代各自串行，没有并发 offset=0

同一附件 socket 上的真实有序帧显示两次完整读取；第二次在第一代最后一次 `read_chunk` 之后才发出
（两代开始时间相差 96 ms，第一代结束到第二代开始仅 1 ms，但没有任何重叠）：

```text
generation 1 (image): sent offsets 0,32768,65536,98304,131072,163840,196608
generation 2 (image): sent offsets 0,32768,65536,98304,131072,163840,196608
每一代 maxInFlight=1；任一代 sentOffsets 中 offset=0 恰好 1 次
```

与第 8 轮（§65.3）的对比：第 8 轮在收到第一个 `offset=0` 块前后又发出第二个 `offset=0`，重复块被
第二次累加，`offset=229376 > totalLength=219661` 后报「读取偏移超过附件总长度」。第 9 轮不再出现
并发读取，也没有偏移溢出。

### 70.3 图片字节与 `<img>`

```text
image block:  state=ready  mimeType=image/png  attachmentId=03ed2c81-7b08-4a93-8543-9c0dc2a10df4
image element: src=blob:https://127.0.0.1:18857/ec2a6738-c849-4f60-ae67-e81d4c720db2
               naturalWidth=160 naturalHeight=400 complete=true
blob 记录:     size=219661 type=image/png
重建字节:      byteLength=219661 sha256=7b141002ff4493e5f067131388dcbeb63c29e4edc339de0502fc0989de6bbd55
块长之和:      219661（等于 totalLength，无重复/遗漏）
```

### 70.4 音频

```text
audio block:  state=ready  mimeType=audio/wav
audio element: src=blob:https://127.0.0.1:18857/71cccfa5-1853-434e-bb8c-aa38bae26e25
blob 记录:     size=4044 type=audio/wav
重建字节:      byteLength=4044 sha256=f8a076046b00a3a9af9cbdda9b5d41b5f9e3254af7d68e40a9f7f73488c84669
maxInFlight:   1
```

说明：抓取时 `<audio>` 的 `readyState=0`、`duration=null`（headless 环境未触发播放/元数据加载）；
本项只证明附件读取、Blob 字节与块状态，不宣称实际播放。

### 70.5 刷新后媒体未恢复（新观察，非本轮去重目标）

同一会话执行 `Page.reload()` 后等待 8 秒，图片与音频块均为：

```text
state=failed
text=读取失败：附件连接尚未就绪（可能未协商 core.attachment-transfer.v1 或正在重连）
```

页面正文仍能显示「媒体回显完成」，说明历史与消息正文已恢复，只有附件读取在连接就绪前失败且没有重试。
本轮没有修改产品代码，因此不把该现象归因到本轮修复；E1 的「刷新后恢复」仍未通过。

## 71. 回归测试（已写入，未在隔离工作区执行）

新增前端用例（`clients/app/src/features/attachment-upload.test.ts`）：

- 「释放后重建等待旧读取结束：多块图片任何时刻只有一个 offset=0 在途，重建字节完整」：
  用可控 219,661 字节 / 32 KiB 分块替身，在第一代读取挂起时释放并重建；断言重建读取在第一代完整
  结束前不发出任何帧（仍只有一个 `offset=0` 待放行）、两代偏移均为
  `0,32768,…,196608`、`maxInFlightReads=1`、最终 `byteLength=219661` 且 Blob 字节逐字节等于源。

执行命令（主检出）：

```text
cd clients/app
npm run test -- --run src/features/attachment-upload.test.ts
```

本隔离工作区没有 `node_modules/`，未执行该命令；用例是否通过由主检出验证。

## 72. 第 9 轮未执行 / 现场清理

未执行（不得计为通过）：

1. 刷新后媒体恢复（本轮直接观察到失败，见 §70.5）；
2. E6 的媒体加载位置、历史翻页、跨断点位置；
3. E2/E3/E4/E7/E8 与 PV7：本轮未重跑，沿用既有轮次结论；
4. 真实 Codex / Oh My Pi（E7）、双真实节点（E5 的本轮证据见 dual 文件）。

现场清理：

- Daemon 以 `daemon stop` 停止，输出 `daemon 已停止（单实例锁已释放）`；端口 18857 无 LISTENING；
- 所有 headless Edge 由 CDP `Browser.close` 关闭；
- 数据、日志、脚本与原始证据保留在 `D:/tmp/acpr-wp7final/e1/`；
- 隔离工作区只新增/修改测试与证据文件，未改产品代码、未改 `openspec` tasks、未提交。

# 第 10 轮（E1 刷新媒体定向复测 + E6 媒体/历史加载锚点）：两项均通过

## 73. 第 10 轮结论先行

| 项 | 结果 | 依据 |
| --- | --- | --- |
| E1 刷新后媒体自动恢复 | **通过** | 刷新后两块媒体 `loading`（318 ms）→ `ready`（419–447 ms），从未出现 `failed`；`<img src="blob:…">` 160×400，字节 219,661 与源 SHA-256 一致；音频 4,044 字节一致（§75.2） |
| 附件连接「尚未就绪」时有界重试 | **通过** | 把附件 socket 首帧推迟 1,200 ms（等价于重连未就绪）：1,200 ms 内媒体保持 `loading`，期间记录 16 次 150 ms 等待（2 个附件各约 8 次），1,535 ms `ready`，无 `failed`，字节仍逐字一致（§75.3） |
| 刷新后不出现并发 `offset=0` | **通过** | 刷新后图片只发生 1 代读取，偏移严格为 `0,32768,…,196608`，任一时刻 `maxInFlight=1`，`offset=0` 恰好 1 次；音频同样只有 1 次 `offset=0`（§75.2） |
| E6 媒体加载保持上读锚点 | **通过** | 上读后锚点条目偏移 `7 px → 7 px`；媒体加载使高度 +478 px、`scrollTop` +478 px，`following=false` 保持（§76.1） |
| E6 历史加载保持上读锚点 | **通过** | 条目 30→36，锚点条目偏移 `7 px → 7 px`，锚点上方实际插入 178 px、`scrollTop` +178 px，`following=false` 保持（§76.2） |
| E6 新内容提示与回到底部 | **通过** | 新内容到达时 `scrollTop` 不变、`data-scroll-unseen=true`、提示文案「↓ 有新内容 · 回到底部」；点击后 `following=true` 且回到底部（§76.3） |
| 6.7 / 7.1 | 仍不可勾选 | 6.7 还需窄屏媒体加载、前端文档/原型差异与 PV1/PV6；7.1 需要 E1–E4/E6 全量复跑（§77） |

与第 9 轮（§70.5、§72）的直接对照：同一构造下，第 9 轮刷新后媒体立即 `failed`，本轮变为
`loading → ready` 且字节正确；第 9 轮未执行的 E6 媒体加载位置与历史翻页，本轮取得通过证据。

## 74. 环境与构建（第 10 轮）

| 项目 | 实际值 |
| --- | --- |
| 源码树 | 执行使用主检出 `D:/Project/acp-remote` 的已构建二进制与前端产物；隔离工作区只写证据 |
| 被测二进制 | `D:/Project/acp-remote/target/debug/acp-remote.exe`，38,414,336 字节，2026-10-20 12:15（+0800），SHA-256 `fa40afd541c36a3e482a4f7a4a4051f7f5f929214a609e97dcff044556b781ca` |
| 前端产物 | `clients/app/dist/build-version.json`：`version = 0.0.0+71badcd-dirty`，`builtAt = 2026-10-20T04:15:04.010Z`；Daemon `status.uiBuildVersion` 与之一致 |
| Daemon | 真实本机 Daemon，端口 18867，数据目录 `D:/tmp/acpr-wp7final/e1r10/data`，`instanceId=b53f246ad5f64376`，`nodeId=fe7e4922-9189-81f8-b53e-056065b7d02f` |
| 浏览器 | 真实 Microsoft Edge `155.0.4283.62`（`--headless=new --ignore-certificate-errors`），CDP 驱动真实 PWA 路由与 DOM；未采集截图 |
| ACP 子进程 | 可控夹具 `D:/tmp/acpr-wp7final/e1r10/agent/agent.mjs --scenario mediafull`（声明 image/audio；媒体回显 + full 的结构化卡片），不是真实 Codex/Oh My Pi |
| 媒体文件 | `tall.png` 219,661 字节 SHA-256 `7b141002…6bbd55`；`tone.wav` 4,044 字节 SHA-256 `f8a07604…c84669` |
| 会话 / 附件 | 会话 `3a7d5a2e-50c4-4f62-8d39-9b5aca9dcc9a`；图片 `240e4de7-60d5-4029-967b-1a21de7cddef`，音频 `7b12cbff-3ee7-48bb-b03a-b7713156aaad` |
| 驱动与原始证据 | `D:/tmp/acpr-wp7final/e1r10/driver/e1r10.mjs`、`evidence/e1r10.json`、`evidence/e1r10.stdout.txt`、`agent.log`、`daemon.log` |

说明：同一驱动的前两次运行因**用例自身缺陷**作废（第一次上读仍停在底部、历史增长断言把总高度当
成锚点上方高度）。最终结果以 `e1r10.json`（第 3 次运行）为准；前两次原始输出保留为
`e1r10-run1.*`、`e1r10-run2.*`，不计入结论。

## 75. E1：刷新后媒体恢复与读取去重

### 75.1 上传、封存与首轮

```text
attachment.begin  mediaType=image/png  totalLength=219661
attachment.begin  mediaType=audio/wav  totalLength=4044
图片块：state=ready  mimeType=image/png  attachmentId=240e4de7-60d5-4029-967b-1a21de7cddef
        <img src="blob:https://127.0.0.1:18867/73de9925-…" naturalWidth=160 naturalHeight=400 complete=true>
        块字节 219661，SHA-256 7b141002…6bbd55（与源文件一致）
音频块：state=ready  mimeType=audio/wav
        <audio src="blob:https://127.0.0.1:18867/f92f8f17-…">
        块字节 4044，SHA-256 f8a07604…c84669（与源文件一致）
```

首轮图片发生了两代读取（同一组件挂载/重建），两代**串行**：每代偏移均为
`0,32768,65536,98304,131072,163840,196608`、`maxInFlight=1`、`offset=0` 恰好 1 次、块长之和
219,661 = `totalLength`。音频两代各一次 `offset=0`、4,044 字节。没有出现两个并发 `offset=0`。

### 75.2 自然刷新

```text
刷新后 t=318 ms：states=["loading","loading"]，页面无 failed
刷新后 t=419 ms：states=["ready","ready"]
图片块：state=ready，src="blob:https://127.0.0.1:18867/…"（与刷新前不同的新 URL）
        字节 219661，SHA-256 7b141002…6bbd55；<img> naturalWidth=160 naturalHeight=400 complete=true
音频块：state=ready，新 blob URL，字节 4044，SHA-256 f8a07604…c84669
图片读取帧（刷新后）：1 代，sent offsets 0,32768,65536,98304,131072,163840,196608
                      recv offsets 同上；maxInFlight=1；dupOffset0=1；sum=219661=totalLength
音频读取帧（刷新后）：1 代，sent/recv offset 0；maxInFlight=1；sum=4044=totalLength
刷新页面记录到的 150 ms 定时器：2 次（两块媒体各一次重试等待）
```

音频元素抓取时仍为 `readyState=0`、`duration=null`（headless 未触发播放/元数据加载）；
本项只证明附件读取、Blob 字节、块状态与刷新恢复，不宣称实际播放。

### 75.3 附件连接未就绪时的有界重试（故障注入）

为证明「并非首读失败即放弃」，在刷新页把附件 socket 的**首帧**推迟 1,200 ms（附件连接在
`hello/auth` 完成前保持未就绪，与真实重连窗口同形）：

```text
注入：__attachDelayMs=1200，作用于 wss://127.0.0.1:18867/sync/v1/attachments 的首个 send
t=312 ms：states=["loading","loading"]（连接尚未就绪，媒体保持等待，没有 failed）
t=1506 ms：states=["ready","ready"]
期间 150 ms 等待计数：16（= 2 个附件 × 约 8 次，与 1200/150 相符）
最终图片 219661 字节 SHA-256 7b141002…6bbd55；音频 4044 字节 SHA-256 f8a07604…c84669
图片读取：1 代，offsets 0…196608，maxInFlight=1，dupOffset0=1
```

即：在首次读取必然遇到「附件连接尚未就绪」的条件下，实现按 150 ms 有界重试，连接就绪后
自动完成读取；没有把瞬时未就绪呈现成 `failed`，也没有产生重复 `offset=0`。

## 76. E6：媒体加载、历史加载与上读提示

准备：同一会话发送 1 条带图片/音频的 prompt 与 6 条纯文本 prompt（共 7 轮），时间线
52 个条目；刷新后首屏历史页为 30 个条目且 `historyHasEarlier=true`。为避免宿主活动终端上限
（8）干扰，未再追加 prompt 扩大历史。

### 76.1 媒体加载保持锚点（通过）

刷新前把附件首帧推迟 1,500 ms，使时间线渲染时媒体仍为 `loading`；随后上读：

```text
上读后（媒体仍 loading）：
  scrollTop=1188  scrollHeight=2432  clientHeight=544  following=false
  锚点：message:21c62507-…（index 12）相对容器顶边 offset=7 px
  媒体块：state=loading（位于 index 0，在锚点上方）
媒体 ready 后：
  scrollTop=1666  scrollHeight=2910  following=false
  同一锚点：message:21c62507-…（index 12）offset=7 px
  高度增量 +478 px，scrollTop 增量 +478 px
  媒体块：state=ready
```

媒体加载发生在锚点上方时，滚动位置按新增高度精确补偿，锚点条目在视口中的偏移不变，且没有
因为内容变化被拉回底部。`<img>` 字节数仍为 219,661。

### 76.2 加载更早结构化历史保持锚点（通过）

在上一步的上读位置点击「加载更早的结构化条目」（`data-paging-disabled=false`）：

```text
before：entries=30  scrollTop=1666  scrollHeight=2910  following=false
        锚点 message:21c62507-…（index 12，offset=7 px）
after ：entries=36  scrollTop=1844  scrollHeight=3350  following=false
        同一锚点 message:21c62507-…（index 16，offset=7 px）
        锚点上方实际插入高度 = (1844+7) - (1666+7) = 178 px；scrollTop 增量 = 178 px
        总高度增量 = 440 px（其余 262 px 落在锚点下方，属条目按时间插回原位，不是丢失）
        无 paging error；新内容提示变为 unseen=true（见 76.3 复位）
```

判定锚点保持的是锚点条目相对容器顶边的偏移（7 px 不变），而不是总高度；新增历史插入锚点上方
的 178 px 被 `scrollTop` 精确补偿。

### 76.3 新内容提示与回到底部（通过）

```text
加载历史后点击回到底部：scrollTop=2806=max，following=true, atBottom=true
再次上读：scrollTop=2106, following=false, unseen=false
         提示："↓ 回到底部"（data-scroll-unseen=false）
发送新 prompt 后：
  scrollTop=2106（不变），following=false，unseen=true
 新消息可见：包含「E6 上读时的新内容提示（第 10 轮）」
 提示："↓ 有新内容 · 回到底部"（data-scroll-unseen=true）
点击提示后：
  scrollTop=3342，scrollHeight=3865，clientHeight=523（=底部），following=true，unseen=false，提示消失
```

## 77. 与 6.7 / 7.1 的映射（第 10 轮）

- **6.7（上读锚点、新内容/回到底部、历史加载与会话/文件往返位置恢复）**：本轮桌面浏览器补上了
  「媒体加载」与「历史加载」两项此前缺失的证据，且新内容提示/回到底部复测通过；但完成判据还要求
  窄屏浏览器覆盖媒体加载、更新前端文档/原型差异，以及 PV1、PV6 通过。这些本轮未执行，
  **6.7 仍不可勾选**。
- **7.1（合入后运行 PV7 并做单节点 E1–E4/E6）**：本轮只定向复测 E1 的刷新媒体恢复与 E6 的
  媒体/历史锚点，未重跑 PV7、E2、E3、E4；且 7.1 要求在 `docs/evidence/conversation-area-complete.md`
  写入逐项断言，本文件是单节点分文件。**7.1 仍不可勾选**。
- 本轮不修改 `tasks.md`。

## 78. 第 10 轮现场清理

- Daemon 以 `daemon stop` 停止，输出 `daemon 已停止（单实例锁已释放）`；端口 18867 无 LISTENING；
- 所有 headless Edge 由 CDP `Browser.close` 关闭；按 `--user-data-dir=…acpr-wp7-e1r10…` 复查无残留进程；
- 数据、日志、脚本与原始证据保留在 `D:/tmp/acpr-wp7final/e1r10/`；
- 隔离工作区只新增/修改测试与证据文件，未改产品代码、未改 `openspec` tasks、未提交。

# 第 11 轮（窄屏 E6 定向）：600×800 媒体加载保持上读位置、图片 Blob 与切档返回

## 79. 第 11 轮结论先行

| 项 | 结果 | 依据 |
| --- | --- | --- |
| 窄屏布局可断言 | **通过** | 600×800 下切换控件只有 `timeline / sessions / files` 三档（无 `all`），默认选中 `timeline`，中栏高度 707.2 px、时间线滚动容器 `clientHeight=407 px`（非 0），因此位置断言可判（§80.1） |
| 窄屏媒体加载保持上读位置 | **通过** | 附件 socket 首帧推迟 1,500 ms 后刷新，两块媒体先为 `loading`；上读后媒体 ready：锚点条目 `message:47eaf577-…`（index 13）相对容器顶边偏移 `-44 px → -44 px`，内容高度 +428 px、`scrollTop` +428 px，`following=false` 保持（§80.2） |
| 窄屏图片 Blob 正常 | **通过** | `<img src="blob:…">`、`naturalWidth×naturalHeight=160×400`、`complete=true`；Blob 219,661 字节，按附件读取帧重组 SHA-256 `7b141002…6bbd55` 与源文件一致；音频 4,044 字节 SHA-256 `f8a07604…c84669` 一致；图片读取 1 代、`maxInFlight=1`、`offset=0` 恰 1 次（§80.2） |
| 窄屏切档返回后位置恢复 | **通过** | 上读位置 `scrollTop=1788`（max=2488，非底部）；切到「文件」档、再切到「会话」档、最后切回「对话」档后 `scrollTop=1788`，同一锚点偏移 `-44 px`，`following=false`，媒体仍 `ready` 且图片 Blob 仍为 219,661 字节（§80.3） |
| 6.7 窄屏媒体加载项 | **可判通过** | 本轮取得窄屏浏览器下「媒体加载保持锚点 + 图片 Blob 字节 + 切档返回位置」三项可观察证据；6.7 其余完成判据（前端文档/原型差异、PV1/PV6）不在本轮范围（§81） |

## 80. 实际执行

### 80.1 环境与构建（第 11 轮）

| 项目 | 实际值 |
| --- | --- |
| 源码树 | 执行使用主检出 `D:/Project/acp-remote` 的已构建二进制与前端产物；隔离工作区只写证据 |
| 被测二进制 | `D:/Project/acp-remote/target/debug/acp-remote.exe`，38,414,336 字节，2026-10-20 12:15（+0800），SHA-256 `fa40afd541c36a3e482a4f7a4a4051f7f5f929214a609e97dcff044556b781ca`（与第 10 轮同一构建） |
| 前端产物 / Daemon | `build-version.json` 与 `daemon.status.uiBuildVersion` 均为 `0.0.0+71badcd-dirty`（`builtAt=2026-10-20T04:15:04.010Z`）；`instanceId=da187043b92f428d` |
| Daemon | 真实本机 Daemon，端口 18877，数据目录 `D:/tmp/acpr-wp7final/e1r11/data` |
| 浏览器 | 真实 Microsoft Edge `155.0.4283.62`（`--headless=new --ignore-certificate-errors`），CDP `Emulation.setDeviceMetricsOverride` 固定 600×800；未采集截图 |
| ACP 子进程 | 可控夹具 `D:/tmp/acpr-wp7final/e1r11/agent/agent.mjs --scenario mediafull`（声明 image/audio；媒体回显 + 结构化卡片），不是真实 Codex/Oh My Pi |
| 媒体文件 | `tall.png` 219,661 字节 SHA-256 `7b141002…6bbd55`；`tone.wav` 4,044 字节 SHA-256 `f8a07604…c84669` |
| 会话 / 附件 | 会话 `84f5d212-704c-4ed4-8a3d-ef4741720312`；图片 `49cc40c7-9f92-4c04-9076-2eaebd061758`，音频 `7669048d-813b-40d9-86ad-b1769e5d1eec` |
| 驱动与原始证据 | `D:/tmp/acpr-wp7final/e1r11/driver/e1r11.mjs`、`evidence/e1r11.json`、`evidence/e1r11.stdout.txt`、`agent.log`、`daemon.log` |

窄屏布局事实（`narrowLayout`）：

```text
viewport = 600×800
切换控件 options = ["timeline","sessions","files"]，selected = ["timeline"]（无 "all"）
timeline 栏 height = 707.2 px（可见）；时间线滚动容器 clientHeight 在扩展后为 386–407 px
```

### 80.2 窄屏媒体加载保持上读位置（通过）

先发送 1 条带图片/音频的 prompt，并再发送 6 条纯文本 prompt（共 7 轮、52 个时间线条目）。随后把
附件 socket 首帧推迟 1,500 ms 并刷新：刷新后两块媒体先处于 `loading`，在媒体完成前上读。

```text
注入：__attachDelayMs=1500，作用于 wss://127.0.0.1:18877/sync/v1/attachments 的首个 send
上读后（媒体仍 loading）：
  scrollTop=1360  scrollHeight=2467  clientHeight=407  following=false  atBottom=false
  锚点：message:47eaf577-…（index 13）相对容器顶边 offset=-44 px
  媒体块：image=loading、audio=loading（均位于 index 0，在锚点上方）
媒体 ready 后（readyAfterMs=2529）：
  scrollTop=1788  scrollHeight=2895  clientHeight=407  following=false
  同一锚点：message:47eaf577-…（index 13）offset=-44 px
  高度增量 +428 px，scrollTop 增量 +428 px
  媒体块：image=ready、audio=ready
```

图片与字节：

```text
<img src="blob:https://127.0.0.1:18877/…" naturalWidth=160 naturalHeight=400 complete=true>
Blob 大小 219,661 字节；读取帧 offsets 0,32768,65536,98304,131072,163840,196608，
maxInFlight=1，offset=0 恰 1 次；重组 SHA-256 7b141002…6bbd55（与源文件一致）
音频：Blob 4,044 字节，读取 1 次 offset=0，重组 SHA-256 f8a07604…c84669（与源文件一致）
```

即在 600×800 下，媒体加载发生在锚点上方时滚动位置按新增高度精确补偿，锚点条目在视口中的偏移
不变，也没有被拉回底部；图片仍是真实 Blob 且字节与源文件逐字一致。本项不宣称音频实际播放
（headless 下 `audio.readyState=0`）。

### 80.3 窄屏切档返回后位置恢复（通过）

接上一步的上读现场，依次点击切换控件的「文件」档、「会话」档，再切回「对话」档：

```text
切档前：selected=timeline，timeline height=707.2 px，scrollTop=1788
        锚点 message:47eaf577-…（index 13）offset=-44 px，following=false
切到 files：selected=files，timeline 栏 height=0（隐藏）
切到 sessions：selected=sessions，timeline 栏 height=0（隐藏）
切回 timeline：selected=timeline，timeline height=707.2 px
        scrollTop=1788（与切档前相同，且不等于 max=2488）
        同一锚点 message:47eaf577-…（index 13）offset=-44 px
        following=false；image/audio 仍 ready
        图片 <img> 仍为 blob URL，219,661 字节
```

三档切换只隐藏时间线栏、不卸载其 DOM；返回「对话」档后 `scrollTop`、锚点条目与偏移、跟随状态
以及已加载媒体全部保持，没有被重置到顶部或底部。

## 81. 与 6.7 / 7.1 的映射（第 11 轮）

- **6.7**：窄屏浏览器下的「媒体加载保持上读锚点」与「切档返回位置恢复」本轮取得通过证据（§80.2、
  §80.3），至此 6.7 完成判据中的桌面/窄屏媒体加载断言已齐备。6.7 仍要求「更新前端文档/原型差异」
  与 PV1、PV6 通过；这两项不在本轮执行范围，因此本轮只报告**窄屏媒体加载项可判通过**，不自行勾选
  6.7。
- **7.1**：本轮为 E6 窄屏定向复测，未重跑 PV7、E1–E4，也未向
  `docs/evidence/conversation-area-complete.md` 汇总；7.1 仍不可勾选。

本轮不修改 `tasks.md`。

## 82. 第 11 轮现场清理

- Daemon 以 `daemon stop` 停止，端口 18877 无 LISTENING；
- 所有 headless Edge 由 CDP `Browser.close` 关闭；按 `--user-data-dir=…acpr-wp7-e1r11…` 复查无残留进程；
- 数据、日志、脚本与原始证据保留在 `D:/tmp/acpr-wp7final/e1r11/`；
- 隔离工作区只新增/修改测试与证据文件，未改产品代码、未改 `openspec` tasks、未提交。

# 第 12 轮（E4 剩余项：第二个附件中途断开 + 失败重试；取消上传复跑）：浏览器侧全部通过，7.1 仍不可勾选

## 83. 第 12 轮结论先行

| 项 | 结果 | 依据 |
| --- | --- | --- |
| 第二个附件上传中途断开 | **通过** | 第 2 个传输发出 2/4 块后由客户端关闭附件 socket（close code `1006`）；第二项 `failed`、错误文案「附件连接已关闭」，第一项保持 `sealed`（§84.1） |
| 失败时 `session.prompt` 未派发（ACP 0 prompt） | **通过** | 业务连接在失败窗口内 `session.prompt` 0 条、`attachment.manifest_begin` 0 帧、时间线无该文本；ACP 夹具 prompt 行数 `4 → 4`（增量 0）（§84.1） |
| 完整草稿与两个附件选择保留 | **通过** | 输入框草稿逐字等于提交原文；附件列表仍有 2 项（`pixel.png`=sealed、`second.txt`=failed），发送按钮 `data-submit-enabled=false`，重试入口可见（§84.1） |
| 失败项可重试且只重传失败项 | **通过** | 重试后 `pixel.png` 不重传（begin 计数仍为 1）、`second.txt` 换新传输重传一次；清单只封存 1 次；最终恰好 1 条 `session.prompt`（§84.2） |
| 重试后两个附件都进入 prompt | **通过** | prompt `content` 类型序列 `attachment_ref, attachment_ref, text`，两个 `attachmentId`/`sha256` 均为字符串，带 `submissionId`；ACP 收到 1 条 prompt，块类型 `image, resource, text`，text 与草稿一致（§84.2） |
| 取消上传复跑 | **通过** | 附件连接发出 `attachment.abort`（`reason=client_cancelled`）；无清单、无 prompt、ACP prompt 行数不增；草稿逐字保留；附件列表 `sealed/cancelled`，提示「附件不可用：cancel.bin 已取消。」（§85） |
| 闲置清理不误删已引用附件 | **测试已写入，本轮未执行** | 新增 Rust 用例 `idle_gc_keeps_attachments_that_are_still_referenced`：24 小时窗口后同时保留「会话绑定」与「消息媒体引用」两个附件、只回收无引用孤儿（§86） |
| 7.1 是否可勾选 | **仍不可勾选** | 本轮只补 E4 的两项剩余浏览器证据；7.1 还要求 PV7 与 E1–E3/E6 全量复跑并汇总到 `docs/evidence/conversation-area-complete.md`（§87） |

## 84. 环境与构建（第 12 轮）

| 项目 | 实际值 |
| --- | --- |
| 源码树 | 执行使用主检出 `D:/Project/acp-remote` 的已构建二进制与前端产物；隔离工作区只写测试与证据 |
| 被测二进制 | `D:/Project/acp-remote/target/debug/acp-remote.exe`，38,414,336 字节，2026-10-20 12:15（+0800） |
| 前端产物 / Daemon | `daemon.status.uiBuildVersion = 0.0.0+71badcd-dirty`；`instanceId = f441e550cc2c4619` |
| Daemon | 真实本机 Daemon，端口 18897，数据目录 `D:/tmp/acpr-wp7final/e4r12/data` |
| 浏览器 | 真实 Microsoft Edge（`--headless=new --ignore-certificate-errors`），桌面视口；未采集截图 |
| ACP 子进程 | 可控夹具 `D:/tmp/acpr-wp7final/e4r12/agent/agent.mjs --scenario e4fail`（声明 image/audio/embeddedContext；收到 prompt 后立即回一段文本并 `end_turn`），不是真实 Codex/Oh My Pi |
| 附件夹具 | `pixel.png` 70 字节 SHA-256 `c414cd0e…ce77`；`second.txt`（100,000 字节，4 块）SHA-256 `ef91a1ed…af73e`；`cancel.bin` 4,194,304 字节 SHA-256 `053ede97…60aa1` |
| 驱动与原始证据 | `D:/tmp/acpr-wp7final/e4r12/driver/e4r12.mjs`、`evidence/e4r12.json`、`evidence/e4r12.stdout.txt`、`agent.log`、`daemon.log` |

失败注入方式：页面侧替换 `WebSocket`，在附件 socket 上按 `attachment.begin` 的全局次序识别「第 2 个附件」；该传输发出第 2 块（文件共 4 块）后调用 `close(4000)`。重试轮不再注入。

### 84.1 失败轮：中途断开后不派发 prompt，草稿与两项选择保留（通过）

```text
注入：second.txt 的传输发出 2 块后关闭（injectedClose.chunksSent=2）
附件 socket：close code=1006（异常关闭），第 2 个 begin 的 chunkCount=2，无 attachment.sealed
附件列表（2 项）：
  pixel.png  data-attachment-status=sealed
  second.txt data-attachment-status=failed  error="附件连接已关闭"
  data-attachment-count=2，data-action=retry-attachments 存在，data-submit-enabled=false
附件提示行："附件不可用：附件连接已关闭"
业务连接：session.prompt 0 条；attachment.manifest_begin 0 帧；命令序列只有 workspace.browse×6、session.create、session.read
时间线：[data-message-text] 为空（该文本未进入时间线）
草稿：data-composer-input.value === "两个附件：第二个上传中断，不应派发这条文字"（逐字相等）
ACP：agent.log 的 prompt 行数 4 → 4（失败窗口增量为 0）
```

即第二个附件在发送 2 块后被断开时，客户端把该附件标为失败、保留第一项已封存状态与完整草稿，且**没有任何**
`session.prompt` 或清单封存帧离开业务连接；ACP 侧一条 prompt 都没收到。

### 84.2 重试轮：只重传失败项，最终恰好一条 prompt、两个附件都在（通过）

```text
重试前：pixel.png=sealed，second.txt=failed
点击「重试失败的附件」后：pixel.png=sealed（不变），second.txt=selected
传输 begin 次序：pixel.png(1) → second.txt(2, 失败) → second.txt(3, 重试)
  → pixel.png 的 begin 计数仍为 1（未重传），second.txt 为 2（失败一次 + 重试一次）
attachment.manifest_begin 帧数：1
session.prompt 条数：1
  content = [attachment_ref, attachment_ref, text]
  attachment_ref[0] = image/png, byteLength=70, sha256=xBTNDiBN6XT3N1PH4o12OOezaRu4saK6trJbt_7Xznc
  attachment_ref[1] = text/plain, byteLength=100000, sha256=75Gh7Yvxs2-Y7RTDakidZYzBKmQHUSMmvyPbRcda9z4
  text = "两个附件：第二个上传中断，不应派发这条文字"
  submissionId = 4979b642-e9e3-48fb-bdc1-021fef69f4ac（非空字符串）
ACP：prompt 行数 4 → 5（增量 1）
  text="两个附件：第二个上传中断，不应派发这条文字"
  blockTypes=["image","resource","text"]，resourceUris=["urn:acp-remote:attachment:2e134cf9-…"]
派发后：附件列表清空、草稿为空；时间线出现 1 条完成轮次（"处理完成：…"）
```

即重试只重置失败项，已封存的第一项没有重复上传；清单只封存一次，所以 `session.prompt` 只可能派发一次，
且该 prompt 同时携带两个附件与原文。

## 85. 取消上传复跑（通过）

新建会话（空时间线）后选择 `pixel.png` + `cancel.bin`（4 MiB），发送后为附件帧加 80 ms 出站延迟以留出取消窗口；
在第二个附件已发出 2 块时点击「取消上传」：

```text
附件连接：attachment.abort { transferId=cancel.bin 的传输, reason=client_cancelled }
  chunkCount(cancel.bin)=2；无 new attachment.error；manifest_begin=0
附件列表：pixel.png=sealed，cancel.bin=cancelled
  提示行："附件不可用：cancel.bin 已取消。"
草稿：data-composer-input.value === "取消上传：这段草稿必须原样保留"
业务连接：session.prompt 0 条（命令只有 workspace.browse×6、session.create、session.read）
ACP：agent.log prompt 行数 5 → 5（增量为 0）
data-submit-enabled=false；data-action=retry-attachments 存在
```

与 §84 一样：取消只中止本批上传，不产生清单、不派发 prompt；已封存的第一项留在服务端可复用。

## 86. 闲置清理不误删已引用附件（Rust 用例已写入，未在隔离工作区执行）

浏览器侧不等待 24 小时；该不变量由存储层集成测试覆盖。本轮新增
`crates/storage-sqlite/tests/upload_lifecycle.rs::idle_gc_keeps_attachments_that_are_still_referenced`：

- 附件 A 由 `commit_with_submission` 建立 `owned_attachment_link`（会话绑定）；
- 附件 B 由 `owned_message` + `owned_media_ref` 持有消息媒体引用；
- 附件 C 只 `put`、无任何引用，作为「本轮 GC 确实执行」的对照；
- 在三个附件都闲置超过 24 小时后的 `prune` 中：A、B 的行与文件都保留，C 的行与文件被回收，
  `removed_attachments == 1`、`freed_bytes == 5`（只有 C 的 5 字节）。

命令（主检出执行；本隔离工作区没有 `target/`，未执行）：

```text
cargo test -p storage-sqlite --test upload_lifecycle idle_gc_keeps_attachments_that_are_still_referenced
```

另外既有的 `retention_and_gc_respect_shared_message_references`（`tests/messages.rs`）与
`message_retention_reclaims_media_references`、`sealed_manifests_expire_and_stop_pinning_attachments`
（`tests/upload_lifecycle.rs`）分别覆盖共享消息引用、媒体引用与清单过期；本轮的浏览器失败/取消路径也由
`clients/app/src/features/attachment-upload.test.ts` 的三条仓库级用例覆盖（「第二个附件失败」「取消上传」
「重试只重置失败项，重新提交恰好派发一次 prompt」），这些均未在本隔离工作区执行。

## 87. 与 6.7 / 7.1 的映射（第 12 轮）

- **E4 剩余项**：本轮补齐了两项此前未执行的浏览器证据——「第二个附件上传失败不派发 prompt、草稿与选择保留、
  失败项可重试」与「取消上传时 `attachment.abort`、无 prompt、草稿保留」；两项均通过。
- **闲置清理**：24 小时窗口不误删已引用附件改由存储层 Rust 用例断言（未执行，交由主检出验证），浏览器侧
  未等待 24 小时，不据此宣称浏览器证据。
- **7.1**：仍不可勾选。7.1 要求合入后运行 PV7 并做单节点 E1–E4/E6 全量验证，且逐项断言写入
  `docs/evidence/conversation-area-complete.md`；本轮只定向复测 E4，未重跑 PV7、E1–E3、E6，也未汇总到
  该单文件。本轮不修改 `tasks.md`。

## 88. 第 12 轮现场清理

- Daemon 以 `daemon stop` 停止，端口 18897 无 LISTENING；
- 所有 headless Edge 由 CDP `Browser.close` 关闭；
- 数据、日志、脚本与原始证据保留在 `D:/tmp/acpr-wp7final/e4r12/`；
- 隔离工作区只新增/修改测试与证据文件，未改产品代码、未改 `openspec` tasks、未提交。

