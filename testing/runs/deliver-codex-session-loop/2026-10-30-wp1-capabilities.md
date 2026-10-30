# WP1 上游与真实 ACP 能力核验

> 当前状态入口：本报告保留首次无登录/直连超时结果。随后显式使用本机既有进程代理，两版真实执行/取消/恢复及一次性批准已有证据，见[网络重试](2026-10-30-network-retry.md)及独立readiness-review；不能将下文旧阶段BLOCKED当作最新WP2可行性结论。

日期：2026-10-30。执行者：主执行 coder；本报告不代签独立 tester/reviewer。结论：上游已交付，真实 v1/v2 初始化成功；两版在复用已授权登录后均创建成功，但模型流超时，执行/审批/取消/释放恢复门槛未满足。尚不开始 WP2–WP4 产品实现，不声明 CL-005/030/032–042 通过。

## 上游与范围

初始 HEAD `65b46c01d6b95fc1f1a8223b1c92a729a885c185`，Git clean。读取归档 access-contract、长期 node-access/client-trust、当前 README 和实际 Rust 模块；上游最终独立33/33及归档完成结论见 `testing/runs/establish-node-access/2026-10-30-ready-metadata-review.md`、`2026-10-30-archive-review.md`。历史较早 BLOCKED 不覆盖最终结论。

本次重基准明确复用现有 identity/trust/access_policy/management_ws/storage/node/lib：认证 actor 不由客户端自报构造；受理、出站入队与实际发送均沿用 client_gate/身份代数；撤销 COMMIT 后禁止新受理/新发送；可信身份不代替运行审批；schema1升级保留身份、撤销与审计。详细设计候选见 change/session-contract.md，仍须独立复核，未实施迁移。

三款探测矩阵范围冻结为 Harness × 实际协议版本 × 必需能力（创建、结构输出、审批、可靠取消、释放、原生恢复）。本change以Codex首验；ZCode/DeepSeek配置及真实兼容验收归各自change，不推断它们同时提供两版。CL-032两款真实同版能力组合仍为未执行，不用改厂商名称的fixture冒充。

## 固定版本与供应链边界

使用 npm 注册表的精确发布包，不使用main/preview推断运行版本：

- `@agentclientprotocol/codex-acp` 2.5.0，Apache-2.0；tarball SHA-1 `bafab6cf524d84dc8661ee7943c598f9c8873c2b`，npm integrity `sha512-7e/4OhJwpvSTqsjiokoVYRV7O1XGem1CAsQgO1opmGYbRD5LvmydwDlJrldzQYO5TcPWueGzQ2Jdvyvp6l3qDw==`。
- 配套 `@openai/codex` 0.165.0 Windows x64，Apache-2.0。实际执行 `--version` 为 `codex-cli 0.165.0`；未使用本机已有0.163.0代替配套版本。
- ACP SDK 2.1.0，Apache-2.0；包内同时有 v1/v2 schema，SDK号不是wire版本证据。
- 仅探测依赖新增于 `testing/probes/codex-session-loop/package.json` 与独立锁文件，未改产品Cargo/npm依赖；`npm ci --ignore-scripts` 实际26 packages added/28 audited、0 vulnerabilities。许可证/Windows平台元数据与官方仓库读取过，仍不代替独立供应链及运行时秘密检视。

| 资产 | SHA256 |
|---|---|
| 实际适配器 dist/index.js | `15DE6B2DD5203876A48B12861378BCF87040D623AB2159E05052DC9D54C32B16` |
| 配套 codex.exe | `D4B2E6A054EA1DFE53217F9D8942C5F8A9F4B76171A1E0BB76CED5759915300A` |
| 首次临时目录 package-lock.json（历史探测） | `5A9711784FB86BDBD17D67B95EC607174EC61D41E1CD1932F2E82F4F1B76466F` |
| SDK v1 schema.json | `1C57095ACF9C0844054DD1FDC3624C121B3FDC24209ECC987FD6748C705FDEF5` |
| SDK v2 schema.json | `28C7251E820511DA257CDB55D30E2C30BBBE59226305B04E3B751DC655E0778D` |
| 首次 initialize 原始公开结果（本目录2026-10-30-acp-initialize.json） | `9A5FC7AA9F7BF60EAB156E58DB9FF0D648AD01DA4ECE81FE48DAFDBC1BA86C7E` |
| 首次认证探针 .tmp/cl-wp1-probe/authenticated.mjs | `8225337D907193DAD312156EBED0DB4C9613EC6DA08386C37F576C1AC5045B82` |
| 首次v2私有结果 .tmp/cl-wp1-authenticated/result-v2.json | `B74AD18812C0D061E585D1877C8A2F733A3334D91747CFAF3F3ED4B455CC89B0` |

首次路径位于 `.tmp/cl-wp1-probe/node_modules/`：适配器 `@agentclientprotocol/codex-acp/dist/index.js`，程序 `@openai/codex-win32-x64/vendor/x86_64-pc-windows-msvc/bin/codex.exe`。可重复的初始化探针及锁已保存 testing/probes；重新安装后独立新目录复测两版相同结果。认证探针为任务自有原型，不是产品公共协议客户端。

## 官方规范核验

实际浏览读取 [v1初始化](https://agentclientprotocol.com/protocol/v1/initialization)、[v1执行](https://agentclientprotocol.com/protocol/v1/prompt-turn)、[v1会话](https://agentclientprotocol.com/protocol/v1/session-setup)、[v2初始化](https://agentclientprotocol.com/protocol/v2/initialization)、[v2执行](https://agentclientprotocol.com/protocol/v2/prompt-lifecycle)、[v2取消](https://agentclientprotocol.com/protocol/v2/cancellation)、[v2恢复](https://agentclientprotocol.com/protocol/v2/session-setup)。官网导航明确 v1 Latest、v2 Draft；实际wire整数分别为1、2。规范主分支revision未取得（GitHub git连接重置），不伪填commit；本次按读取日期、公开页面内容摘要及精确SDK schema固定依据，草案变化后重新分析并复测。

| 语义 | v1 | v2 Draft |
|---|---|---|
| 初始化 | clientCapabilities/clientInfo；响应agentCapabilities/agentInfo | capabilities/info；响应capabilities/info |
| prompt | 原请求响应携带stopReason，不能将cancel通知当结束 | 响应messageId仅确认输入插入；由state_update idle及所属工作事实协调终态，不能将插入当完成 |
| 取消 | 按原prompt完成与权限cancelled处理 | session/cancel后相关权限回复cancelled；工作完成后idle/cancelled；请求取消与整轮取消分开 |
| 恢复 | 使用实际广告的sessionCapabilities.resume；不默认load无重放 | 省replayFrom恢复上下文且不重放历史；仍须当前state快照，快照不计作新的完成 |

公开页面缓存 `.tmp/cl-wp1-probe/official-1.json` 至7（不含登录材料），v2初始化页面SHA256 `D0E1934330389E351DDC2E87C5D6C1548DDE3E5BFE1D4D8E7809C6B6BA17D80C`；v2执行 `8423ACB9AF9E2D290EE2B0F577F3F0BF87CC076ED0931E90CDB9316583729CEF`；v2取消 `62A46280EC3214E7456C62697D61E34FEE0A53C894EC75CCA4300B4C2711F964`；v2恢复 `331D09544617B579C221F47D3254CA4D0837D4C7A83203AD59487A06E7D41E09`。页面快照不是已批准稳定版声明。

## 真实结果与资源

1. 完全隔离、无登录、移除API key及应用会话环境引用：两版initialize成功；v1广告loadSession/resume/list/close等，v2广告session方法面；两版session/new均明确 -32000 Authentication required，没有假造会话。
2. 用户明确授权“复用本机现有Codex登录，仅在专用测试目录创建会话”。只复制登录材料到 `.tmp/cl-wp1-authenticated/native-v1`、native-v2，父目录设置当前owner-only受保护DACL；没有把凭据放入报告、Git、配置快照或聊天输出，没有复制用户既有会话/数据库。
3. v2认证后session/new成功，新ID仅属于专用测试根。合成marker输入已收到user_message_chunk及state_update running，但90秒窗口内没有模型回复或idle，探针记timeout idle。私有协议日志只统计固定错误词：多次stream disconnected/retrying/timed out；未直接打印日志。无审批请求，恢复和取消步骤尚未执行，不能据此评价其支持或不支持。
4. 未附凭据的 `curl -I https://chatgpt.com/backend-api/codex/responses --max-time 15` 失败，Connection was reset。说明本机独立服务网络路径当时不可用；不推断为适配器协议不兼容，也不擅自切私有网关或厂商桥接。
5. v1复测也成功创建新会话，但原session/prompt在90秒内无响应，记录timeout session/prompt。未运行后续审批/取消/恢复步骤；不将网络超时当作可靠取消。私有结果SHA256 `9A97444F69ABF3677E2AE9395A7FC7BEF64914C1068339898F105E9710B3C157`，位于`.tmp/cl-wp1-authenticated/result-v1.json`。
6. 每个探针关闭stdio，父进程均退出0；随后以实际可执行路径只读查询Win32_Process，首次初始化、两版认证探测及重复初始化路径无遗留进程。未运行产品Job Object，此结果不是CL-030完整进程树验收。两份任务专用auth.json已逐一移除且Test-Path均False；未删除用户原始登录、数据库或原生会话。任务测试原生状态保留用于复验；认证材料不入Git、不保存在运行报告。脱敏持久摘要为本目录`2026-10-30-acp-authenticated-summary.json`。

## 门禁与后续

WP1真实能力门槛仍未通过，任务1.2不勾选；CL-005定义要求已认证Web的产品门控行为，本change界面尚未实现，不能把此原始探针签作CL-005。方案问题由原独立reviewer对最终文档摘要关闭；真实执行需服务网络可用后在相同固定版本重新测试，并另取审批唯一裁决、可靠取消、跨进程原生上下文恢复和资源证据。当前不需要用户发送密钥或重复手工安全签字。

## 探测资产最终收口

最终可重复锁文件SHA256 `B968DB5562536DDDD6F648375C633B6BC9421477D0A1F3283DA58DF9945C61A2`；持久初始化脚本SHA256 `ACEC4F3441172D566AE4A21B9487B5EFEA8CFD5D84914035F503DAC95B043694`。首次使用npm `--prefix`安装时，npm在独立探测清单加入了本仓库file链接；该链接及无用extraneous元数据已从最终探测清单/锁移除，产品清单和锁未改。改为在探测工程目录执行npm ci，最终25 packages added/26 audited、0 vulnerabilities；最终依赖只有官方适配器及其锁定的传递依赖。未将初次含本地链接的锁冒称最终独立锁。

在新输出根重复初始化仍取得相同适配器/程序SHA256、v1/v2成功及未登录新建拒绝。公开官网浏览任务已正常finish；任务临时登录副本清理后没有再次使用用户登录，认证探测原始结果与最终脱敏摘要分别保留其各自版本。未自动重试结果未知的prompt、未触发私有网关或协议降级。
