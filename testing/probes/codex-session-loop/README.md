# WP1 隔离协议探测

此目录是前置探测工具，不是产品 ACP 客户端或 CL-030 验收实现。

依赖固定为官方 `@agentclientprotocol/codex-acp` 2.5.0；锁文件固定 SDK 2.1.0 和配套 Codex 0.165.0。适配器和 SDK 为 Apache-2.0，Codex 平台包为 Apache-2.0；探测只支持 Windows x64、Node >=22.12。依赖保存在本目录，不添加到产品依赖。2026-10-30 安装审计为 0 vulnerabilities，不能替代独立供应链审查。

在仓库根执行：

```powershell
Push-Location testing/probes/codex-session-loop
npm.cmd ci --ignore-scripts
Pop-Location
node testing/probes/codex-session-loop/initialize.mjs .tmp/cl-wp1-initialize
```

每次使用新的输出目录；工具拒绝复用已有 native-v1/native-v2。隔离 `CODEX_HOME`，移除 API key 和已有应用会话引用，不执行登录、不读取用户原生会话。分别使用两版真实初始化结构；成功后尝试创建会话。无认证时预期得到 -32000，而不是执行成功。协商能力声明不证明执行、审批、取消或原生恢复成功。结果只包含公开能力、错误和本次 PID，不保存原始 stderr；正常结束关闭标准输入并等待，超时只终止本次启动 PID 的进程树。

工具结果写到输出目录的 `initialize-result.json`。初始化限时20秒，退出等待3秒。复测后还需只读核对该隔离路径的子进程已全部退出；不能仅凭父进程退出宣称进程树验收通过。

## 已授权身份的真实链路

`authenticated.mjs`保留实际协议请求与单命令审批流程，参数为协议版本及仓库`.tmp`下的测试根；仅使用已准备好的`native-v1/auth.json`或`native-v2/auth.json`，不自动复制用户登录。测试根必须预先设置当前owner-only受保护DACL，登录材料复用必须有用户授权，结束后清理本次副本。不能将凭据或私有运行日志提交。脚本不启动浏览器登录，不加载用户既有原生会话。

```powershell
# 在已授权并准备好临时身份的情况下运行；仅当前进程使用既有代理
$env:HTTPS_PROXY = 'http://127.0.0.1:10808'
$env:HTTP_PROXY = $env:HTTPS_PROXY
$env:ALL_PROXY = $env:HTTPS_PROXY
node testing/probes/codex-session-loop/authenticated.mjs 2 .tmp/my-owned-probe
```

代理地址只是本次机器已启用设置的示例，不能假设其他机器相同。Windows系统代理不保证独立CLI自动继承；先核实可达性，不修改全局设置。

该脚本创建新会话、测试合成marker、关闭适配器后按同一原生ID恢复召回、取消及一次性审批。只批准专用cwd内精确`Set-Content -LiteralPath 'approval-probe.txt' -Value 'WP1_APPROVAL_PROBE' -NoNewline`命令，且仅选择对端广告的allow_once；其他请求取消。输出原始合成测试结果到临时根。完整产品的多客户端唯一裁决、撤销与Job Object仍需真实产品验收，不能用此探针代签。

2026-10-30实际使用的原型及结果摘要见`testing/runs/deliver-codex-session-loop/2026-10-30-network-retry-acceptance.md`。此处将原型路径改为可移植参数、添加临时目录/认证前置检查及更严格的精确命令白名单；路径调整后的脚本仅作语法核对，真实证据仍归原型摘要，不冒称改写后的完整脚本已重跑。

首次实际运行使用 `.tmp/cl-wp1-probe/initialize.mjs`，当时依赖与数据均在该自有目录。此处保留相同请求及清理逻辑并将依赖位置改为脚本目录、输出目录改为参数且增加拒绝复用检查；原始运行结果与摘要见 `testing/runs/deliver-codex-session-loop/2026-10-30-wp1-capabilities.md`。产品级 Job Object 清理尚未实现或验收。
