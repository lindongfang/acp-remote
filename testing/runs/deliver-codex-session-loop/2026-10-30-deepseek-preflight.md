# DeepSeek Harness 独立前置核验（2026-10-30）

结论：本机已有可用的官方 Windows DeepSeek Harness 0.2.1-rc.3；独立存储根下真实 ACP v1 initialize 成功。CL-032 的完整双 Harness 输入、结构输出、审批、取消、恢复尚未执行；不能将本次握手当作完整兼容通过。DeepSeek 仅提供 v1 不阻断 Codex v1 + DeepSeek v1 对照，也不代替 Codex v2 组合证据。

## 范围与依据

执行者为独立前置核验角色。读取仓库 AGENTS.md、deliver-codex-session-loop design/session-contract/tests 的 CL-032、037、040、041，重设计 §9。检查 Git 状态后保护既有产品、规划及未提交文件；只写本报告与 `.tmp/deepseek-preflight/`。未接管真实会话，未读取、复制或输出用户凭据正文，未安装、升级或修改用户配置。

## 本机定位与固定版本

PATH 中 Get-Command deepseek*/dsh*/pnpm*/node/npm/uv 无命中，不意味着未安装。定向检查常见 npm/uv/Programs/.local/bin，以及 D:/Project/deepseek-harness（不存在）；进一步只读卸载注册表发现：

- DisplayName `DeepSeek Harness 0.2.1-rc.3`；InstallLocation `D:/Application/DeepSeek Harness`。
- 官方自带入口 `resources/runtime/cli/bin/dsh.cmd`；真实 `--version` 返回 `0.2.1-rc.3`。
- 入口以 `ELECTRON_RUN_AS_NODE=1` 运行同安装的 Electron executable，加载 `resources/app.asar/dsh/node_modules/@deepseek-ai/dsh-desktop-host/lib/cli.js`。没有启动桌面 UI、共享 daemon 或原生已有会话。
- 自带 runtime.json：win32/x64，Node 24.21.0、pnpm 11.28.5；payloadDigest `e2cef445c262ce6e8d28480b5ebecc8b0c54c06bea7183374debfcf02f30eb7d`。

SHA256（现有安装固定证据，未与签名发布清单完成独立校验）：

| 文件 | SHA256 |
|---|---|
| DeepSeek Harness.exe | FD7D298F4BA03EE9BAB1719A892AFEAEF5C09BF50C574A592A64C85F64574C0C |
| resources/app.asar | 04B32E3A872826BA15CFB2081C6A929A8D4ECAA65E09A2543E5A6B30E2EFF1F0 |
| resources/runtime/primary-runtime/runtime.json | AD225DF11DEFE4D80AD6FCC3F32ABD142730CB976E0D013010D9E3EEFA697DA6 |
| resources/runtime/cli/bin/dsh.cmd | C6C38BE7F9F66A7BFB96D5AC9490618F9713DF5FFB552CB7E66B831AFFB20AB9 |

## 官方发行与协议依据

固定安装对应的 [0.2.1-rc.3 ACP 契约](https://raw.githubusercontent.com/deepseek-ai/deepseek-harness/dsh-v0.2.1-rc.3/packages/acp/acp/README.md) 描述稳定 ACP v1，命令 `pnpm dsh --profile acp`。支持 list/resume/close、单次允许/拒绝审批、取消；resume 不重放历史，session/load 不支持；断线走所属 Agent 关闭路径。上述生命周期行为是文档合同，本次没有实测。

[官方发布列表](https://github.com/deepseek-ai/deepseek-harness/releases) 当前可见 v0.2.2-alpha.2，tag `dsh-v0.2.2-alpha.2`、短修订 f3803c9；[附件](https://github.com/deepseek-ai/deepseek-harness/releases/expanded_assets/dsh-v0.2.2-alpha.2) 为源码 zip/tar 和 attestation，没有 Windows CLI 二进制附件。本次没有升级到该版本。

[固定较新版本 CLI 元数据](https://raw.githubusercontent.com/deepseek-ai/deepseek-harness/dsh-v0.2.2-alpha.2/apps/cli/package.json) 指明 npm 包 @deepseek-ai/dsh、bin dsh、Node >=24.9.0；不得将包号、JSON-RPC 2.0 或 SDK 号当作 ACP v2 证据。DeepSeek v2 未发现官方支持依据，矩阵不登记此组合通过。

## 真实握手

环境为 Windows；DSH_HOME 仅设为 `D:/Project/acp-remote/.tmp/deepseek-preflight/home`。使用上述官方 CLI 等价入口加 `--profile acp`，向 stdin 发送一行 JSON-RPC：

```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1,"clientCapabilities":{},"clientInfo":{"name":"acp-remote-independent-preflight","version":"0"}}}
```

真实响应落 `.tmp/deepseek-preflight/initialize-v1.jsonl`：protocolVersion=1；agentInfo 为 deepseek-harness-acp/0.0.1；sessionCapabilities 为 close/list/resume；MCP http=true；image/audio/embeddedContext=false；authMethods=[]。未广告 load 能力。authMethods 为空仅表示 ACP 控制通道不要求登录，不说明模型调用已有认证。

资源所有者为本次 .NET Process 探针：读响应预算20秒，关闭stdin后等待5秒，超时仅终止 owned PID 完整进程树。实际 PID49620 关闭stdin后正常退出，HasExited=True；stderr 空。未创建 session，未发 prompt，无模型网络调用证据。

官方 `--dump-default-config` 与 `--dump-config-schema` 在独立根执行，分别保存在 `.tmp/deepseek-preflight/default-config.yml`、`config-schema.json`。真实配置将 sessions/storages 根绑定 DSH_HOME；ACP 默认 provider=deepseek-official、model=deepseek-v4-flash。模型 API 插件 schema 的 apiKeyEnv 默认为 DEEPSEEK_API_KEY，标注 credential-ref；credentials-local 的 path/dshHome/watch 可配置。尚未验证凭据查找优先级及只读保证，不直接挂载真实凭据库。

## 登录引用与下一步

仅文件名检查发现 `C:/Users/jack/.dsh/.credentials.yaml`，以及 desktop profile 的 cordis.yml/cordis.patch.yml/package.json；已有 sessions/storages/skills 目录未遍历。未读取 credential 文件内容、provider 列表或任何密钥；未确认其中有 DeepSeek 官方可用认证。进程环境中 DSH*/DEEPSEEK* 名称未发现。沙箱会重定向 LOCALAPPDATA，因此真实安装检查使用精确 require_escalated 只读操作。

[官方模型配置说明](https://raw.githubusercontent.com/deepseek-ai/deepseek-harness/dsh-v0.2.2-alpha.2/docs/user/guide/providers.md) 说明凭据保存在 DSH_HOME/.credentials.yaml，profile 存引用。此页对应较新版本，实际0.2.1-rc.3字段以本机schema为证，不能据此假定账号票据与API key可互换。

主执行下一步需统一向用户确认：允许隔离验收使用哪一个既有 DeepSeek 官方认证引用（优先用户指定受控 DEEPSEEK_API_KEY 环境变量名；或者明确授权从既有 .credentials.yaml 中仅取所需 DeepSeek 条目至进程，不复制整个库），及实际模型/provider。无需用户在聊天粘贴密钥。若选择既有凭据，须先核实该固定版本官方解析与只读方式，防止挂载整个凭据库/真实会话根。

## 依赖、安全及限制

必要性：第二真实 Harness 用于 CL-032 同版对照；复用已有固定安装即可，新增全局依赖没有必要。本轮未增加仓库依赖或锁文件。官方项目 [MIT](https://raw.githubusercontent.com/deepseek-ai/deepseek-harness/dsh-v0.2.2-alpha.2/LICENSE)，自带 Electron/Chromium 许可证文件存在；完整第三方许可证与漏洞审查未完成，不能标供应链审查通过。Windows 自带运行时与真实initialize已验证；PowerShell工具及完整进程树取消/释放仍待真实执行。

[官方安全说明](https://raw.githubusercontent.com/deepseek-ai/deepseek-harness/dsh-v0.2.2-alpha.2/SAFETY.md) 将软件列为开发预览、未安全审计，能够执行命令并访问可用凭据/文件；后续验收应继续独立工作目录、认证最小暴露和审批，不挂载真实sessions。默认配置含遥测设置，执行真实输入前应按官方支持方式在独立profile明确关闭并验证，避免测试正文外发至非模型端点。

网络：尝试通过既有127.0.0.1:10808代理下载官方发布元数据/文档，curl出现证书撤销离线（CRYPT_E_REVOCATION_OFFLINE）；仅将撤销离线作为best-effort后再次尝试，仍超时，无下载产物。web工具可读取上述官方固定tag文档。没有以无法下载推断软件能力失败；没有弱化HTTPS证书校验或改全局代理。

状态：CL-032完整对照 not-run；CL-037/041仅取得版本依据与真实握手局部证据；CL-040没有引入私有桥接或文本降级。创建/真实输出/审批/取消/恢复/关闭/断线/Windows进程树验收均待认证与独立用例执行。报告不代签产品验收或安全审查。
## 追加：用户授权复用当前桌面模型与凭据后的真实探测

本节取代上文“等待认证引用”的阶段状态；原记录保留。用户明确授权复用 DeepSeek 桌面当前模型及凭据配置后，已完成独立真实 ACP v1 能力链局部验证，15/15 探针断言成立。脱敏结果为 [2026-10-30-deepseek-real-probe.json](2026-10-30-deepseek-real-probe.json)。这是固定真实 Harness 的直接协议证据，不是产品 CL-032 通过声明，也没有代签 Web、公共产品适配器或 Windows Job Object 验收。

### 最小认证隔离及原配置保护

在内存解析用户 desktop 的 YAML，确认当前默认 provider=`deepseek-account`、model=`deepseek-flash`。它使用 DeepSeek 官方账户授权 grant，不能按 DEEPSEEK_API_KEY 处理。只筛选 `deepseek-account-platform/default` 的单一 grant 至独立 DSH_HOME/.credentials.yaml；未复制浏览器连接凭据、device 身份、其他 provider、desktop profile、sessions、storages 或 skills。

读取固定安装中的官方实现核实：

- credentials-local：托管文件优先于继承进程环境，再依次回退 invocation cwd/.env、DSH_HOME/.env；显式 config.path 优先。启动可迁移旧版布局，set/unset 可写文件；它没有只读挂载保证。
- deepseek-account-platform：读取 `deepseek-account-platform/default`，校验 issuer/目标 inference origin；错误 issuer 或401可能删除本地记录。故没有让测试进程访问原始凭据库，只让其访问筛选副本；没有交换为私有模型网关。
- 自有根ACL移除继承，仅当前用户 SID 完整访问；临时凭据继承该单一用户ACL。测试后原始用户.credentials.yaml的SHA256与读取前相同，筛选副本已精确删除。未将其值或原始文件纳入版本控制证据。

原始私有事件、stderr及新建原生测试状态仅保留在受限 `.tmp/deepseek-preflight/`；本报告及脱敏JSON无token、apiKey或账户资料。凭据副本删除后重跑需重新按授权筛选，不能悄悄改用真实DSH_HOME。

### 遥测关闭的实际检查

独立 profile 将 session-telemetry-otel config 设为 `{mode: DISABLED}`，并设置该测试进程 `DSH_TELEMETRY_MODE=DISABLED`。实际 `--profile acp --dump-config` 输出已保存 effective-config.yml，确认有效 mode=DISABLED，ACP/默认模型为 deepseek-account/deepseek-flash。读取同一固定安装官方实现，DISABLED 分支不构造 SDK/provider/exporter，仅保留本地feedback警告监听。真实会话使用的就是该 profile；未改全局环境或用户配置。本轮未做网络拦截，所以不将配置/代码检查扩大为所有网络流量审计。

### 实际步骤和结果

所有探针都是 protocolVersion=1，真实 Windows 官方程序与模型；进程代理仅设置在测试子进程环境，未改全局。无mock/fixture。第一次先行运行的 marker 为8E24，取消、恢复均完成，但探针没有把 permission中的toolCallId关联到先前结构化tool_call，导致allow阶段fail-closed取消，没有误批。修正探针关联后，重新创建全新目录 workspace-real-v1-second、marker9F35和新会话，保留第一次结果，不混用归因。

第二次实际链路：

1. initialize成功、session/new取得新原生ID；marker输入返回精确ACK_DS_9F35，prompt终态end_turn。
2. 只读session中请求写ds-deny-9F35.txt；权限回答cancelled，工具结构事件记录取消失败，磁盘文件不存在。
3. 请求精确单一PowerShell命令 `Set-Content -LiteralPath 'D:/Project/acp-remote/.tmp/deepseek-preflight/workspace-real-v1-second/ds-allow-9F35.txt' -Value 'DS_ALLOW_9F35' -NoNewline`。先按同会话toolCallId关联已观察tool_call.rawInput，严格字符串相等才选广告的allow_once选项，未永久允许。真实工具结构状态completed，磁盘内容精确匹配。
4. 发起长推理prompt，2秒后session/cancel；真实prompt返回stopReason=cancelled。这里验证活动模型请求的取消，不等同于已启动外部长进程的进程树取消。
5. session/close成功，关闭stdin，旧进程PID46232 exit0；启动新进程PID49304，重新initialize，session/resume相同原生ID成功，该响应前未收到历史update重放；后续prompt精确召回DS_NATIVE_CONTEXT_9F35，end_turn。
6. 最终session/close成功，新进程exit0、stderr零字节。第一次两进程PID38144/47260亦exit0。结束后CIM快照确认上述owned PID、其直接子进程及本次probe命令行无残留。未杀已有桌面进程，未删除用户状态；本次没有调用强杀fallback。

15项断言：v1协商、创建ID、marker匹配、结构tool_call、结构completed、取消审批、拒绝写未落盘、精确allow_once、允许写内容匹配、prompt取消终态、resume RPC成功、无历史重放、marker召回、两进程exit0、最终close成功，均成立。每项布尔值和断言数已写脱敏JSON，不以截图或启动成功代替。

原始第二次证据SHA256：`214DB3078676660CC40D1D80495966B7B42982FBF795FE17830A80A7396354E8`；探针摘要见脱敏JSON。原始schema与配置仍在自有根；未新增产品依赖、未全局安装、未更新用户程序，未修改产品/规划/验收定义。

剩余限制：本次为前置直接ACP探针；CL-032要求公共产品路径对两款真实Harness运行相同合同并核调用轨迹，仍须主执行及独立tester在集成产品上完成。DeepSeek v2不宣称支持；图像能力随账户/route/附件存储组合发生变化（本次账户配置initialize广告image=true，早先无认证默认route为false），不能把前一次capability布尔值外推为所有组合。完整Windows子树故障取消、用户界面、持久化故障、独立安全/供应链审查仍未完成。

## 追加：无凭据 v2 形状 initialize offer（WP3协商边界）

使用固定DeepSeek0.2.1-rc.3，独立新DSH_HOME=`.tmp/deepseek-preflight/v2-offer-home`、独立cwd与owned CLI进程；未读用户配置/凭据，没有session/new、session/prompt或任何其他请求。测试环境移除DEEPSEEK*/CODEX*/OPENAI*，遥测环境设DISABLED。完整公开请求/响应与清理结果保存 [2026-10-30-deepseek-v2-offer.json](2026-10-30-deepseek-v2-offer.json)。

请求严格采用官方v2形状，没有夹带v1字段：

```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":2,"capabilities":{},"info":{"name":"acp-remote-preflight","version":"0.0.0"}}}
```

实际响应成功选择 `protocolVersion:1`，并使用v1响应形状 `agentInfo`、`agentCapabilities`、`authMethods:[]`；没有返回v2的info/capabilities。agentInfo为deepseek-harness-acp/0.0.1；标准能力仍为session close/list/resume、MCP http、prompt image/audio/embeddedContext=false。完整原始响应见上述JSON，不改写为v2支持。

[官方v2 initialization规范](https://agentclientprotocol.com/protocol/v2/initialization)（2026-10-30核对，页面标v2 Draft）的版本协商要求：选择不高于请求的最高已支持版本，响应必须采用所选版本形状；当请求版本较新时忽略所选版本无法解析的参数。客户端必须确认自身支持返回版本。这个实际观察符合“v2 offer→v1 selected/version形状”的协商边界，不能推导该DeepSeek实现支持v2业务协议。

本次没有尝试重复initialize或开启第二种引导形式，也没有修改程序、产品代码或增加私有fallback。它为WP3通用协商实现提供真实对端证据，不是产品CL-036验收通过。owned PID34164关闭stdin后exit0，stderr空、未强杀，独立home不存在.credentials.yaml。完整公开原始证据SHA256：`a5c9c0d195f79e91bd34f90c24c9b1d3b71cbcd443557647246846569e7a7c02`（针对.tmp/deepseek-preflight/v2-offer-public.json，脱敏JSON附相同摘要）。
