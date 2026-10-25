# omp ACP 工具开始与审批发送顺序

状态：源头补丁已完成；151 项相关回归通过，独立二进制自检及真实模型 ACP 顺序探针通过。2026-10-24 已安装到开发 Daemon 专用目录，并启动 Daemon 使用新路径。系统原版 omp 未替换，既有历史事件不改写。

## 真实记录

2026-10-24 对用户截图对应的本地数据库进行只读检查。只取事件类型、序号、时间和工具关联标识，不导出 prompt、认证资料或工具输入输出。

| 会话内序号 | 事件 | 时间（UTC） |
|---|---|---|
| 246 | permission.requested | 05:11:39.148 |
| 247 | tool.call.started（搜索） | 05:11:39.148 |
| 248 | tool.call.started（被审批命令） | 05:11:39.148 |
| 249–250 | 搜索工具 updated/completed | 05:11:39.148 |

审批请求与第 248 条开始通知带同一 toolCallId。存储里已经是审批在前，单改前端排序无法修复源头。

## 源头与修复

本机版本：omp/18.12.6。读取对应 [v18.12.6 源码](https://github.com/can1357/oh-my-pi/tree/v18.12.6)，提交 `45c35e40c46166dc726ccc52f5d27f40f2449f99`。

- `packages/agent/src/agent-loop.ts` 的 `runTool` 先调用 `stream.push(tool_execution_start)`，随后进入 `tool.execute`。
- `EventStream.push` 只把事件交给异步队列，不代表 `Agent` 的消费循环已经向会话订阅方转发事件。
- `SessionTools` 的工具执行包装层可立即经 `requestPermission` 发 ACP 请求；这条路径无需等待上述队列，因而能越过已经排队的工具开始事件。
- ACP transport 内部请求和通知共用有序写队列。问题出在它们进入发送队列之前。

补丁：[18.12.6-tool-start-delivery.patch](../../patches/omp/18.12.6-tool-start-delivery.patch)。在 `Agent` 组合的调度配置中加入交付确认，`runTool` 在开始事件排队后等待该事件的同步订阅通知完成，再进入工具执行包装层。确认按工具标识匹配，取消会解除等待并移除监听；原有执行路径在取消后不执行工具。直接使用 `agentLoop` 的消费方未安装此确认函数，保持原行为。补丁不修改 ACP Remote 的事件序号、前端排序或协议资产。

## 验证与限制

诊断命令（当前本地工作区）：

```powershell
node .diagnostics/omp-tool-order-repro.cjs
```

诊断程序实际执行上游 `runTool` 的开始发布代码段、上游 `EventStream`，以及补丁中的交付确认函数；消费者启动前预先排队思考与搜索开始事件，再模拟命令执行包装层的即时审批副作用。

- 修复前断言失败：`permission → thought → search → bash`。
- 修复后通过：`thought → search → bash → permission`。
- 取消且无消费者的等待能够结束，不发送审批，不残留监听。
- 上游补丁通过 `git diff --check` 与逆向 `git apply --check`。

上面的片段诊断现已由完整模块回归与真实 ACP 验证补足：

| 检查 | 结果 |
|---|---|
| 未修改的 18.12.6 发布模块运行新增 Agent 回归 | 稳定失败，审批先于两个工具开始事件 |
| 应用补丁后的新增 Agent 回归 | 通过，搜索开始 → 命令开始 → 审批 |
| 工具开始通知期间取消 | 通过，工具未执行，运行状态正常结束 |
| 双向 NDJSON / JSON-RPC ACP 连接回归 | 通过，客户端收到两个工具开始通知后才收到审批请求 |
| 上游原有 `packages/agent/test/agent.test.ts` | 52 项通过 |
| 上游原有 `packages/coding-agent/test/acp-agent.test.ts` | 96 项通过 |
| 上述四个测试文件合跑 | 151 pass，0 fail，632 次断言 |
| 编译后 `--version` / `--smoke-test` | `omp/18.12.6` / `smoke-test: ok` |
| 独立真实模型 ACP 探针 | 安装前、安装路径复验均为 `tool_call → permission`，1 次审批；探针自动选择拒绝，不执行命令 |
| 开发 Daemon 状态与 `agent list` | 就绪，omp 指向修复版可执行文件 |

正式测试直接使用同版本 npm 发布包的真实模块，测试文件和夹具取自 v18.12.6；没有使用简化版 Agent 替代生产实现。新增 Agent 测试位于补丁的 `packages/agent/test/tool-start-delivery.test.ts`，连接测试位于 `packages/coding-agent/test/acp-tool-order-wire.test.ts`。使用工作区独立下载的 Bun 1.4.4，不修改系统工具链。未运行上游整个多语言仓库的 `bun check`；这里验证的是本次涉及的 Agent、ACP 与最终可执行文件。

后续在完整 v18.12.6 源码和上游工具链环境中应用补丁，至少运行：

```powershell
git apply --check <补丁绝对路径>
git apply <补丁绝对路径>
bun test packages/agent/test/tool-start-delivery.test.ts
bun test packages/agent/test/agent.test.ts
bun test packages/coding-agent/test/acp-tool-order-wire.test.ts
bun test packages/coding-agent/test/acp-agent.test.ts
bun check
```

## 本机构建与部署

完整 Git blob 下载失败后，使用 npm 的 18.12.6 发布源码包与其依赖完成测试和构建。构建沿用上游 JSON 插件、legacy Pi 模块注册插件、原生 addon 压缩嵌入与独立二进制参数；复用同版本发布包的 docs index 和已构建 stats 页面资源。npm 源码中的 stats archive 是空占位，初次构建的统计页面自检失败；补齐同包 `dist/client` 的压缩归档后重新构建，完整自检通过，失败候选未部署。

- 构建工作区：`.worktrees/omp-runtime/`；本地构建脚本 `build-fixed.ts`，依赖版本由该目录 `bun.lock` 固定。
- 安装路径：`D:/Data/acp-remote/dev/tools/omp-order-fixed.exe`。
- SHA-256：`4893712B2F21574A874AB4A72009E3852E32E405864F22A3643667366B8CDD99`。
- 配置：`D:/Data/acp-remote/dev/config.toml` 的 `omp` argv 改为上述可执行文件和 `acp`，没有添加自动批准选项。
- 配置备份：`D:/Data/acp-remote/dev/config.toml.before-omp-order-fix`。
- 原版 `C:/Users/jack/AppData/Local/omp/omp.exe` 保留，仍可单独使用。
- 开发 Daemon 启动前已确认未运行，启动后管理接口确认 Agent 启动路径生效。

回退时只把开发配置中的 omp 命令恢复为 `["omp", "acp"]`，再按正常流程重启 Daemon；不要覆盖之后对其他配置键的修改。新启动的 Agent 使用修复版。已有数据库事件的历史序号不会因发送端补丁而被改写。
