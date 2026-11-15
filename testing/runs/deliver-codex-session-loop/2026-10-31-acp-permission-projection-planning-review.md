# 公共ACP权限投影§8A独立方案预检

2026-10-31，reviewer `/root/baseline_reviewer`，只读项目，未参与产品修改。受审implementation-contract.md SHA-256 `713af98e75608df77c5a727700ef1c5c6114f94b4add88a5b813de70fad076d5`的§8A。

初步结论：**候选方向与固定协议及现行spec一致，未发现新产品决策或设计阻断；独立子用例未交接，暂不放行编码。** 范围仅公共权限请求投影/选项编码，不含实际审批、SQL、actor/资源owner或WSS。

## 兼容性依据

已实际读取固定SDK2.1.0稳定v1/v2 schema的RequestPermissionRequest、RequestPermissionSubject、CommandPermissionSubject、ToolCallPermissionSubject、PermissionOption及Outcome；两稳定schema仍为已固定`1c57095a...fdef5`和`28c7251e...e0778d`。

v2 subject省略/null与command.toolCallId省略/null均合法；v2必需sessionId/title/options，已知command仍需command/cwd，tool_call仍需toolCall.toolCallId。v1必需toolCall/toolCallId，不能把v2缺省规则套到v1。官方同样区分可选结构主题与实际权限请求，工具/终端ID只关联显示状态，不是请求来源凭证。[ACP v2权限请求](https://agentclientprotocol.com/protocol/v2/tool-calls#requesting-permission)

现行permission-approvals要求可靠会话/连接/真实请求归属即可，不以缺Run/精确前后台分类额外拒绝。旧strategy对v2无subject或无tool的command标unsupported，是遗留工具必关联前提；解除这一限制是协议/新spec对齐，不是放宽真实来源或批准门禁。

## 必须保留的安全边界

- 仅省略和JSON null是合法无结构主题；bool/数组/错误type或已知主题必需字段错误不得被当作省略主题开放。未知主题保留raw并明确unsupported，不能悄悄转成一般允许。
- 支持的选项仍来自原广告；未知kind、重复optionId、未广告所选项都拒绝selected。取消仍可按协议一次性回应，不能因unsupported自动允许。
- sessionId、类型化RPC ID、当前private connection scope和完整raw/原广告绑定不变。Connection.permission_response须继续重新解析原raw，不信任调用者篡改的公开投影字段。
- 无subject合法只意味着能将有效用户裁决按协议编码，不是默认批准；不能从title/description或kind猜操作范围、记住的平台授权或建立伪Run。人工裁决、持久唯一决定、当前actor及资源围栏留给后续owner。
- 重放窗口内权限仍是独立当前请求，不标成可自动批准的历史；连接失效、已答复、取消或未知交付继续按原安全规则处理，不自动重发。

## 所需独立子定义

1. v1合法toolCall及缺/错toolCallId的对照；v2合法subject省略/null、command带/不带/null toolCallId、tool_call，各分支投影/raw与合法人工selected编码。
2. v2缺/错title、空options、错command/cwd、错tool_call结构，以及subject primitive/无type/未知type；按规定拒绝或unsupported，零不合法selected。
3. 四种已知选项、未知kind混入、重复/未知所选ID；unsupported可cancelled且一次回应，拒绝不能意外消费合法未决请求或伪造响应。
4. 公开投影字段与raw篡改、跨连接相同RPC ID、数字与字符串RPC ID、无请求/重复/关闭后响应、写失败/取消未知不重发；原有预算及Debug不泄露raw。
5. 同目标replay窗口内收到合法无Run/无tool权限仍独立Incoming，不造Run/前台终态或复用旧授权；该测试仅协议夹具，不签真实来源发行或后台审批生命周期。

本次未执行代码/测试或真实Harness；独立定义到齐并复核前保持未开工，完整CL-008/013–016及ARCH审批L5/L6不因本候选关闭。

## 最终阶段门槛复核

2026-10-31，已完整读取独立request_tester的12组AP子定义，SHA-256 `180b42e5c80fce85e2a18a7367a642d9da6b5037fa0cd95c5023a239916ff526`；当前含明确错误形状/人工编码边界的合同SHA-256 `ef14c21b0fbdcbae1bc305486dfd6b528ffa32df8ddd8d4b9e81eddda5c232b1`，与tester输入一致。

最终结论：**§8A纯公共ACP权限投影方案和独立L3定义通过，可以开始该限定阶段编码。** 没有需用户新增裁决的产品规则或未关闭编码前阻断。

12组实际覆盖合法v2省略/null主题及command缺省/null工具、错误subject/type与已知字段协议错误、v2 tool_call与v1必需结构、信封/选项schema、原raw广告重验、未知/重复ID安全取消、RPC标量身份、跨连接及重复一次答复、writer失败/取消未知不重发。未知变体只接受object且type为string后标unsupported，不能让缺/错type走合法无主题；取消通知本身不代签owner已处理全部权限。

可修改公共strategy投影、必要类型/注释及原错误开发断言并新增开发边界测试，不放宽传输私有来源/原广告校验或上层持久审批门禁。新独立资产应保持与coder测试区分。所有本阶段执行仍not-run，实际后台生命周期、唯一裁决、SQL/WSS/真实Harness及完整L5/L6继续另验。
