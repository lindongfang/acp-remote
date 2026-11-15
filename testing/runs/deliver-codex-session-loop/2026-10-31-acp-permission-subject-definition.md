# §8A 公共 ACP 权限请求投影：独立 L3 子用例

2026-10-31。仅定义公共ACP客户端的标准权限请求投影及协议响应回归，不改生产代码、不改统一tests。依据permission-approvals规格和implementation-contract §8/8A；不继承coder实现对话。候选定义完成，测试尚未执行，编码门禁由独立reviewer判断。

## 固定依据与范围

已实际读取本地`testing/probes/codex-session-loop/node_modules/@agentclientprotocol/sdk/package.json`，版本为2.1.0；SDK版本不是ACP wire版本。使用其`schema/v1/schema.json`和`schema/v2/schema.json`，不把unstable文件混入依据：

| 固定输入 | SHA256 |
| --- | --- |
| v1/schema.json | 1c57095acf9c0844054dd1fdc3624c121b3fdc24209ecc987fd6748c705fdef5 |
| v2/schema.json | 28c7251e820511da257cdb55d30e2c30bbbe59226305b04e3b751dc655e0778d |
| 读取时implementation-contract.md（含§8A） | ef14c21b0fbdcbae1bc305486dfd6b528ffa32df8ddd8d4b9e81eddda5c232b1 |

schema中v2 RequestPermissionRequest必需sessionId/title/options，subject可省略或null；CommandPermissionSubject必需command/cwd字符串，toolCallId可省略或null；ToolCallPermissionSubject仍必需toolCall，ToolCallUpdate必需字符串toolCallId。v1 RequestPermissionRequest必需sessionId/toolCall/options，不引入v2 subject/title必需语义。选项kind允许未来扩展；“可解析并保留”与“本平台理解且可以selected”分开。

本任务明确要求bool/数组/错type不能冒充省略；known command字段错类型为协议错误。即使SDK可选字段带`x-deserialize-default-on-error`注记，也不把该注记当作本阶段静默降级错误输入的授权。未知扩展对象/字符串保留原始结构并明确unsupported，不能转成已理解的通用授权主题。

全部用例级别L3，使用真实公共Connection/Strategy API及内存双工协议对端；初始化分别合法协商v1/v2，然后发送合成permission帧，观察Incoming/投影/raw及对端实际响应字节。禁止只测手写预期模型或只改开发测试断言。通道无真实Harness、SQL、WSS、Web、后台工作证明或用户秘密。

以下AP标签是阶段子断言，关联已有CL-008/009/013–016/065–068/078等权限用例适用子范围，不新增CL ID、不替代它们的完整L5/L6。协议层允许构造selected响应只表示对原广告的合法编码，不代表产品已持久裁决或有权批准。

## 子用例矩阵

| 标签 | 操作步骤 | 预期结果 |
| --- | --- | --- |
| AP-v2-subject-optional | 合法v2请求分别完全省略subject、subject:null；提供title与一个或多个已理解的原广告选项，不提供Run/Turn/工具关联；分别选择广告选项及独立请求cancelled | 两者均为合法已理解请求，不因没有结构主题/工具/Run变unsupported；tool投影为None，真实session/RPC ID/connection scope及原raw保留。selected编码匹配原optionId；另请求可单次cancelled。不伪造tool/Run。 |
| AP-v2-command-optional-tool | subject={type:command,command:字符串,cwd:字符串}，toolCallId分别缺省、null、合法字符串；保留terminalId/_meta和未知扩展；使用至少两个原广告选项 | 三种command均supported；前两者tool=None，后者仅投影真实tool ID。command/cwd保留原字符串及raw，未知显示字段不当执行授权。缺工具关联不能被旧限制误判unsupported；原广告选项可合法selected。 |
| AP-subject-shape-error | v2 subject分别bool、数组、数字、字符串，以及对象缺type、type:null/bool/数组/数字；对照未知字符串type的对象 | 前述错误为协议错误，不产生可批准的Permission投影，不把错误形状当省略/null。只有具有未知字符串type的对象按扩展保留并unsupported，不能把错type归入“无主题可允许”。 |
| AP-command-known-types | 对type:command的command和cwd分别缺失/null/bool/数组/对象/数字；toolCallId分别bool/数组/对象/数字；对照缺省/null合法情况 | known必需字段缺失或错类型、toolCallId非字符串且非null均协议错误，不能通过as_str/get失败降为None或unknown允许主题；合法缺省/null仍通过。具体cwd路径权限/存在性在owner层，本阶段不执行命令或读文件。 |
| AP-v2-tool-call | 合法type:tool_call包含toolCall:{toolCallId:字符串}及原结构扩展；另toolCall缺失/null/非对象、toolCallId缺失/null/错类型 | 合法投影为Some原tool ID，必要原字段完整保留；非法必需关联为协议错误，不能因command的可空规则放宽tool_call。tool ID只作为可选投影事实，不自动证明Run归属。 |
| AP-v1-regression | 合法v1 toolCall/toolCallId及原选项；省略v2 title/subject；对照省略/null/错类型toolCall或toolCallId，以及仅发送v2 subject替代toolCall | 合法v1保持支持且可选原广告；v2规则不污染v1，缺v1必需结构失败。不会仅因没有v2 title拒绝v1，也不会靠v2 subject绕过v1必需toolCall。 |
| AP-required-envelope | v2 title缺失/null/非字符串；两个版本sessionId错类型、options非数组、option.name/kind/optionId缺失或错类型；v2 options空数组对照v1 schema的空数组 | 必需字段错误为协议错误；v2至少一个option。v1空options不伪造默认允许选项、不能selected未广告值，可按既有协议取消；不把v2 minItems新增为v1标准要求。保持现有raw/结构预算，不借修正放宽RPC/帧解析。 |
| AP-original-advertisement | 收到真实Incoming::Permission后，只修改调用者可见投影（supported_subject、tool、options增删/ID/kind）而保留原raw；再尝试选择伪造选项。另改raw中广告/ID后提交，及未由Connection收到的手造请求 | 发送前以所属连接保存的原raw与广告重新核对；伪造投影不能增加可选项/改变不支持判定，raw被换拒绝。合法原广告仍可响应；完整raw字节、扩展、顺序/显式null不因投影修正丢失。正文不作为授权来源。 |
| AP-unsupported-safe-cancel | v2 unknown与_自定义subject type对象；v1/v2 unknown或_自定义option kind；广告含已知+未知kind混合；重复optionId（同kind及不同kind）；选择不在广告列表的ID | 原unknown payload/options保留，显示unsupported；unknown subject、任何unknown option kind、重复ID或未广告ID都拒绝selected，实际selected帧为0。失败选择不耗掉安全取消资格，可随后对该请求单次cancelled；第二次cancelled/selected均无新响应。不会自动选第一个/同名近似选项。 |
| AP-rpc-scalar | 同一连接依次挂起RPC整数7与字符串"7"，另字符串"007"；分别响应并解析实际出站id；回归null/bool/数组/对象/小数及超安全整数的RPC id | 不混淆或字符串化RPC身份，返回id标量类型及值逐一相同；整数和字符串是独立请求。非法RPC id沿既有安全合同拒绝，不能借subject修正扩展接受范围。 |
| AP-scope-duplicate-once | 两个同版连接用同RPC id/session发请求，交叉使用PermissionRequest；同连接重发已见RPC id（未答复和已答复两窗口）；回答后再答复；对照正常独立连接合法响应 | 跨连接零写拒绝，即使raw完全相同；同连接重复RPC id按既有已见身份规则拒绝，不覆盖原广告/复活请求。每个真实请求至多一帧结果；未知/未请求对象不能应答。新连接只能处理自己新建真实请求，旧对象不能迁移。 |
| AP-cancel-unknown-write | 对合法无subject/无tool command分别cancelled；对该request实际完成cancelled响应后尝试旧selected；在实际response writer开始后注入失败/超时/future取消，再重试原响应 | cancelled保持原RPC/session/connection关联且只一次；不得向已取消/已回复请求发送selected。可能已写出的交付保持未知，连接沿既有不可安全复用规则处理，不自动再写selected或cancelled、不得声称对端执行ACK。v1/v2会话取消范围由后续真实协议owner分别处理，本阶段不把session/cancel通知写成功当作已自动回答所有请求或证明后台停止。 |

## 覆盖说明与交付判据

当前12组均未执行。编码后先冻结strategy/model/transport/公共测试及本定义摘要，再独立通过公共API执行。报告每组实际测试数、输入分类、错误/支持/不支持区别、raw比较及实际出站响应帧计数，不把仅返回Ok或截图当完成。结果日志和源码清单保存独占目录；开发回归可补独立证据，但开发自评不能代签。

此修正不提供真实审批来源owner、SQL唯一裁决/决定提交围栏、故障安全控制、后台权限生命周期或前台idle后的应用准入，不替代既有CL的完整L5及真实Harness L6。没有新的产品审批/WSS/Web入口；尤其不读取或外发Web审计元数据。管理SQL外部候选尚未冻结，仍须按management-stage-definition.md另行独立验收。
