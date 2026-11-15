# WP3 §6A纯协议发现/重放阶段方案独立检视

2026-10-31，reviewer `/root/baseline_reviewer`，独立于coder及baseline_tester，只读项目，未参与产品修改。

结论：**限定方案与用例定义通过，允许在现有公共ACP transport/strategy/model目录增量实现list、标准replay、preferred初始化及同目标resume/replay窗口互斥。** 无新增编码前阻断或需用户再次裁决的产品分支。不批准Node、SQLite、资源/审批发行者、Web或真实Harness接线，不签完整重放视图/历史分页或产品准入。

## 固定依据

- implementation-contract.md含§6A的SHA-256 `9ea803b6ee4a0ac7c139b865cd717b7a7aed15f8c5fa5a2c339be358cc3f33a0`。
- 独立tester `acp-discovery-replay-definition.md` SHA-256 `0d2897bd651000351bc35d21256023ae0ee0e686306129fea264267707e92129`，已完整读取A–H及稳定用例关联。
- 已实际读取本地SDK package.json确为2.1.0，以及稳定schema中的AgentCapabilities、SessionCapabilities、ListSessionsRequest/Response、SessionInfo、v1 LoadSession、两版ResumeSession及相关身份定义。v1 schema SHA-256 `1c57095acf9c0844054dd1fdc3624c121b3fdc24209ecc987fd6748c705fdef5`，v2 `28c7251e820511da257cdb55d30e2c30bbbe59226305b04e3b751dc655e0778d`，与既有README一致；未使用unstable schema代替。
- 已独立浏览官方[v1 session setup](https://agentclientprotocol.com/protocol/v1/session-setup)及[v2 session setup](https://agentclientprotocol.com/protocol/v2/session-setup)。v1 load能力与v2 start重放/当前状态同步的区别与候选一致；SDK版本不冒充协议号，草案状态不冒充真实互操作通过。

## 核对

| 独立定义 | 核对结果 |
| --- | --- |
| A / H：preferred与连接能力 | 默认只发一次v2 offer，合法selected v1按v1结构解析；当前连接不热切，新连接不沿旧低版缓存，不因网络/业务错误降版重发。v1 loadSession/list/resume独立，v2 session对象为基线能力依据，无厂商分支。 |
| B：list | 标准可选参数省略、UTF-8预算、必需字段、原顺序及游标语义、4096项容量与完整raw均可执行验证；拒绝需检查对端零字节，不只错误返回。 |
| C：replay与resume | v1声明loadSession后session/load，v2 session/resume带start；无重放resume另保留。缺能力零写，不代new或私有读取；读取完成不生成Run/批准。 |
| D：互斥与scope | 同连接同session的resume/replay四种组合在RPC登记/任何写之前拒绝，保留首请求；另一session可并行。取消通知不提前解除窗口；远端error、超时、关闭及跨Strategy零写有明确定义。 |
| E：来源 | DuringReplay只表示同目标窗口，不证明每条更新是历史；权限保持独立真实请求路径，不复用旧决定。重复正文不猜合并，raw顺序保留。 |
| F：v2 snapshot | 当前窗口同目标state_update须先于成功响应；错目标/窗口前/响应后不补通过，缺失MissingSnapshot并关连接。未知状态保持未知，idle snapshot不生成新完成。不能为伪边界禁止快照后的所有合法实时更新。 |
| G：失败/预算/取消 | 原32MiB、深度96、token和关联预算保持；非法/未知响应、EOF、deadline及write future取消不自动重发。逐帧消费不在客户端累积整段历史，不冒称全平台视图有界。 |
| 稳定ID映射 | CL-032–041/047–053/058–059仅L3/L4子范围；产品准入、旧资源/命令/审批责任、真实升级和L5/L6义务仍保留，不用纯层结构取代实际能力。 |

原生sessionId继续不透明字符串，不得误套平台22位ID或UUID格式。cwd的绝对路径语义保留，本层不声称验证本机存在性/权限；所属workspace/资源入口以后独立校验。协议scope互斥只处理连接内session，跨连接显式source+native身份协调仍由应用层负责。

Negotiated可补充此次真实offer字段，与selected分开作为可观察协商元数据；offer必须取本次唯一initialize关联事实，不从厂商或旧缓存推导，也不能凭低版选择自行编造对端支持集合或回退原因。这属于本阶段实现细节，独立定义A/H的实际帧断言可直接核验。

## 文档与独立性边界

此阶段README应同步移除停用session-contract权威、历史强刷和必Run关联的过时接线前提；以真实目标/连接/当前请求、可选Run及当前实施合同解释边界。不通过文档宣称应用owner、缓存、审批发行者已交付。

另已静态检查本轮trust.rs变动完全位于cfg(test) insert_client helper：先建立合成合法node_meta再制造信任历史，未改变生产信任或缺身份拒绝。该fixture调整与R002拒绝规则一致；开发报告的9项运行不在本方案审查中代签。

并行写范围仅公共ACP目录及对应开发测试/README，不与外部SQL管理候选写同一模块声明/装配；合入公共文件仍按plan串行处理。不得新增依赖或读取凭据作为本阶段默认步骤。

本次未执行新协议测试、构建或真实Harness；独立定义A–H全部not-run。未来固定候选源码后需同版独立测试及代码检视，不能以方案通过或旧ACP测试完成勾产品任务。
