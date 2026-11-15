# WP3 恢复/重放配置保真：只读工程盘点

2026-10-31。仅后续合同建议，**不放行编码，不是R3或产品验收结论**。未改root、未访问网络/私人配置/凭据，未运行产品或真实Harness。实际输入SHA在inputs.json。

## 结论与现有缺口

最小改动应是：应用提供有可信来源、已授权且固定版本的完整有效配置；ACP构造器显式接收该配置，并按本连接版本/实际广告编码。未知不能自动变成空集，不能仅把`mcpServers:[]`换成一个可缺省参数就声称CL-102完成。

- strategy.rs:124/165/230 的new_session、replay、resume只接cwd，全部硬写mcpServers=[]，全部省略additionalDirectories；v1 replay用session/load，v2 replay用session/resume+replayFrom start。
- Strategy::select仅保留session/list/replay/resume等及prompt能力；未投影MCP transport或additionalDirectories广告。Negotiated.raw保留原初始化消息，可作为固定解析依据，不能从厂商/SDK号猜能力。
- model::SessionListItem只保session/cwd；optional roots还在Incoming.raw中，当前类型没有配置来源。根产品源未发现MCP/额外根的可信有效配置模型；旧harnesses.config_ref不能自动解释成完整已授权Client配置。
- transport::request已有同连接scope和同session在途resume/replay互斥；参数由Request.params原样装入帧，不会替应用补配置。v2仍须current-state快照才接受恢复/重放完成；配置修复不能去掉此边界。

## 固定primary来源

已亲读本地 `D:/Project/acp-remote/testing/probes/codex-session-loop/node_modules/@agentclientprotocol/sdk/package.json`，版本2.1.0。以下只指这个固定包的**stable** schema文件，SDK号不等于wire版本或真实Harness已通过；未使用unstable schema。

- v1 `schema/v1/schema.json`，SHA `1c57095acf9c0844054dd1fdc3624c121b3fdc24209ecc987fd6748c705fdef5`。
- v2 `schema/v2/schema.json`，SHA `28c7251e820511da257cdb55d30e2c30bbbe59226305b04e3b751dc655e0778d`。

下面名称均为该文件的`$defs.<名称>` JSON指针；不是另外一份协议规则。

| 请求/字段 | 固定v1 | 固定v2 |
| --- | --- | --- |
| NewSessionRequest | session/new；cwd、mcpServers必填；additionalDirectories可选 | session/new；cwd必填；mcpServers、additionalDirectories可选 |
| LoadSessionRequest | session/load；sessionId、cwd、mcpServers必填；additionalDirectories可选；须agentCapabilities.loadSession=true | 此包无LoadSessionRequest，不补造session/load |
| ResumeSessionRequest | session/resume；sessionId、cwd必填，mcpServers、additionalDirectories可选；须agentCapabilities.sessionCapabilities.resume={} | session/resume；sessionId、cwd必填；mcpServers、additionalDirectories、replayFrom可选；capabilities.session={}广告baseline resume |
| 完整历史重放 | 上述load；不得用仅resume冒充历史 | resume加replayFrom:{type:start}；省略/null replayFrom是无历史恢复，不能替代不支持的cursor |
| cwd | string，描述要求绝对路径 | AbsolutePath引用，其定义本身为string，描述要求绝对路径；并非另有JSON路径解析器 |
| additionalDirectories | ordered string[]，各路径绝对；load/resume非空时为完整结果列表，cwd须匹配原会话cwd；省略/空表示不激活额外根 | ordered AbsolutePath[]，同样是完整结果列表；省略/空表示无额外根 |

**额外根能力**：v1为初始化结果`agentCapabilities.sessionCapabilities.additionalDirectories`；v2为`capabilities.session.additionalDirectories`。两版SessionAdditionalDirectoriesCapabilities均以对象（含{}）广告；省略/null不广告。非空根不能因缺能力静默丢弃；已确认空根可按协议省略以兼容无该扩展的连接，因为这里表达“无额外根”，并非“保持旧配置”。

**列表报告不能直接当配置授权**：两版SessionInfo.additionalDirectories描述均把省略/空解释为报告无额外根，v2描述还列null；present非空是完整有序报告。不要把所有省略一概判Unknown，也不要把“Agent报告无根”直接当成本节点已授权空配置。SessionInfo不提供mcpServers，所以它不能单独补齐完整Client配置。v2该字段description提null而schema type仍为array，后续解析合同须显式处理这个差异并保留raw；此处不暗改接收规则。

## MCP的精确版本差别

来源为AgentCapabilities/McpCapabilities/McpServer/McpServerStdio/Http/Sse、EnvVariable、HttpHeader。

| transport | v1 wire及准入 | v2 wire及准入 |
| --- | --- | --- |
| stdio | 结构字段name/command/args/env全必填；**没有v2要求的type discriminator**；McpServer分支明确所有Agent必须支持 | type:"stdio"必填；name/command必填，args/env可选；须capabilities.session.mcp.stdio对象广告 |
| HTTP | type:"http"、name/url/headers必填；须agentCapabilities.mcpCapabilities.http=true | type:"http"、name/url必填，headers可选；须capabilities.session.mcp.http对象广告 |
| SSE | type:"sse"、name/url/headers必填；须agentCapabilities.mcpCapabilities.sse=true | 无标准SSE广告位/独立SSE分支；不能按v1规则猜支持或改成HTTP。v2 other transport允许保留未知原结构，但不代表已广告可用 |

args是string[]；env与headers均为有序{name:string,value:string}[]，不能转为会去重/丢顺序的map。command要求绝对路径。保留必要_meta/未知原始字段；无法无损编码的结构明确限制受影响路径，不删字段后发送缩水配置。v1描述中出现mcp_capabilities字样不改变实际JSON property的camelCase `mcpCapabilities`。

特别注意v2 session={}只广告baseline session方法，**不自动广告stdio MCP**。缺MCP能力且配置为非空时不能改发[]；已确认Client MCP为空时可发[]而无需任何transport广告。这里的空仅指本次Client提供的列表，不宣称Agent内置工具/服务也为空。

## 最小API/参数建议（待新合同冻结）

1. 应用层只有一个小型有效配置快照：绑定NativeSessionKey、来源引用/修订、已验证cwd、完整Client MCP列表和完整additionalDirectories。来源可以是本节点已明确授权配置或明确的新授权；Agent报告只提供它实际报告的字段事实。字段Unknown与Known(empty list)分开，禁止Default把Unknown变空。外部目标MCP未知就限制这次load/resume/replay或取得所需授权，不扫私有文件、不整体禁用无关操作。
2. ACP层参数改为明确的`SessionSetup`值/借用，必有cwd、显式完整mcp_servers、显式完整additional_directories；它是协议数据，不是授权grant。`new_session(&SessionSetup)`、`resume(session,&SessionSetup)`、`replay(session,&SessionSetup)`三方法统一取参，删除旧无配置cwd-only入口，避免继续硬编码空。应用已完成可信来源/权限判断后才构建该完整值；不需要通用配置框架或厂商读取器。
3. 仅补实际需要的配置能力投影：additionalDirectories、MCP stdio/http/sse（按上述版本规则），从本连接固定initialize raw解析。缺省/非法广告不虚报支持；不替换现有operation准入。未知配置来源写前明确失败；不支持的已配置transport不能筛掉后发送剩余列表。v2 other transport保留原结构，是否允许同版本受信透传须在准确合同说明，不能据本盘点宣称支持，也不把全部未来扩展永久禁用。
4. 请求固定同一来源修订和本连接scope；真正写前若配置修订/目标或连接换代改变，重新核验，不沿旧参数发出。已写出与未发送分别协调；反复恢复/重放/翻页都复用已授权完整快照或明确变更，不能因“只是看历史”默发空MCP/空根。

快照和协议请求不可Debug输出env/header值、凭据或敏感配置；审计仅记录配置引用/版本及真实结果。结构_meta/未知数字等若当前Value编码无法保证保真，保留原输入并明确未支持，不宣称字节级转发已实现。不要引入配置的隐含正文预算；现存ACP 32MiB帧/解析预算也不是固定schema规定的配置大小上限，其整体移除属已有WP3限制对齐义务。

## 后续可执行合同至少固定的验收

- v1 load/resume、v2 resume及全量replay分别捕获实际帧：非空MCP/根、已确认空、Unknown来源；Unknown/无授权/缺所需广告时零请求/零写，不静默清空。v1 stdio与v2 stdio准入差异必须对照。
- 精确字段、数组顺序、env/header重复项、版本discriminator/必填数组、必要_meta与未知结构处理；不把未知transport当已支持。
- 同目标重放/翻页复用配置；配置修订在构建后或在途改变的行为明确；cwd不匹配、外部参数未知只限制所依赖路径。SessionInfo报告和授权事实分开，不补造MCP列表。
- 配置错误不能取消原输入、虚报历史完整、放松v2 snapshot完成要求或自动降版重发。敏感canary不进日志/错误/审计。
- 独立tester按CL-102建立字段/能力/未知/并发矩阵；真实程序/原生配置来源及Web授权入口另有实际证据。当前未写这些测试、未运行它们，不能把字段盘点当CL-102通过。

后续仍须冻结准确快照构造来源、协议类型/错误、变化围栏及独立定义再预审；本报告不给下一阶段产品编码授权，也不改变当前R1/R2/R3的独立实施/验收范围。
