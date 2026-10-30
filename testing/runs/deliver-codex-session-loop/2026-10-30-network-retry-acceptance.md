# WP1 网络恢复后的独立证据核对

日期：2026-10-30。独立 tester，只读核对主执行提供的真实探针结果及探针源码；没有重新运行 Harness，没有读取、复制或输出 auth 凭据。产品与规划均未修改。本报告不签 CL-005、CL-030 或完整产品验收通过。

结论：本轮原始结果支持 v1/v2 均完成真实创建、回复、关闭后跨新适配器进程恢复同一原生ID并正确召回上下文、可靠取消终态及权限请求 cancelled。此前“Authentication required/直连超时”不再能代表这两份新证据的结果。允许审批后的真实工具成功执行和 Windows 完整进程树清理仍缺独立证据。

## 固定证据

| 文件 | SHA-256 |
| --- | --- |
| `.tmp/cl-wp1-proxy-retry/result-v1.json` | 12857A90B89048C41F33EC5AE16BC8CFCE521C9DFF264EE7D044BE1C6BB681E0 |
| `.tmp/cl-wp1-proxy-retry/result-v2.json` | 50B67B9F01B944C07840F1C0BB12E6751429DFB1F13CA78FC3E851E3DE7A7CF9 |
| `.tmp/cl-wp1-proxy-retry/retry.mjs` | 9D2524AEC8D0C57CDB08B2A15F85061CFFFEF13521D0CFF1A7D258F061CF18E3 |

结果initialize标识适配器 `@agentclientprotocol/codex-acp` 2.5.0；探针指向隔离依赖目录下的真实Windows Codex exe及独立CODEX_HOME。仅进程代理环境启用由主执行报告，本结果未保存代理环境快照，故本 tester不独立证明代理设置与成功的因果关系，也不宣称持续网络可用性。程序发布摘要及Codex版本继续引用WP1固定发布证据，不能仅从此结果推断。

## 逐项核对

| 项目 | v1实际结果 | v2实际结果 |
| --- | --- | --- |
| 创建与同一原生ID | `01a189ff-9c73-7d02-9a27-6f456058740a` | `01a189fe-df07-7870-a9d1-b52b3f7ab275` |
| 首次回复 | 将相同messageId的agent_message_chunk拼接，精确等于8字符 `ACK_63F9` | 同左 |
| 关闭并换进程恢复 | 第一适配器PID 23572退出0，第二PID 42268恢复同ID成功 | 第一PID 33284退出0，第二PID 33440恢复同ID成功 |
| 恢复后上下文 | 新请求仅问先前marker，不携带答案；相同messageId拼接精确等于23字符 `WP1_NATIVE_CONTEXT_63F9` | 同左 |
| 恢复时不重放旧回复 | resume响应前记录的updates为0，没有旧agent chunk | updates为session_info_update、usage_update、state_update，共3条；无旧agent chunk；idle为当前状态快照，无stopReason，不能算新的执行完成 |
| 回复完成 | marker及recall各prompt响应stopReason=end_turn | prompt响应只有messageId；两轮各有之后running→idle且stopReason=end_turn，不将messageId当完成 |
| 主动取消 | cancel轮prompt响应stopReason=cancelled | cancel轮有running→idle、stopReason=cancelled；与恢复时idle快照明确分开 |
| 权限请求 | 1项request_permission，广告选项allow_once/allow_always/reject_once；客户端返回cancelled，prompt最终cancelled | 同样1项及相同选项；requires_action→running→idle，终态cancelled |
| 最终关闭 | session/close返回result={}；第二适配器退出0 | 同左 |

v1共55条session/update事件、v2共70条；每份结果均9个步骤、无failure字段。所有事件各自始终关联其创建的原生ID。两份独立native测试目录均存在该ID对应的真实rollout文件，但本次没有读取其正文，也没有将其存在等同平台数据库持久化。

## 资源与证据限制

- 四个适配器进程均记录`stdio exited`、exitCode=0、stderrBytes=0；未触发探针的taskkill回退。探针close先结束stdin再等待最多5秒。此记录证明探针观察到的适配器正常退出，**不证明其全部后代进程已经结束**；结果未保存Job Object、子孙PID清单或退出后完整进程树检查。不能签完整Windows资源清理通过。
- 专用workspace下`approval-probe.txt`读取时不存在，与权限cancelled相符；两版工具更新最终failed，本轮没有allow_once决定成功及写入成功证据。工具请求/取消轨迹不等于真实工具成功执行；60秒等待任务在取消前实际开始也缺操作系统级证据。
- 探针的resume-same-native-id只在响应到达时截取updates，结合其后完整事件流未发现重放ACK；这支持本次省略replayFrom的观测行为，不证明所有时序/重放配置及异常恢复均无重放，也不证明平台已实现历史去重。
- 本轮没有平台节点/Web、SQLite会话事务、唯一审批竞态、可信身份撤销、进程Job Object或故障保存链路。两版官方规范状态及完整兼容矩阵由其他独立报告核对；本证据不将Draft规范变为稳定标准。
- 临时auth副本及原生会话仍由主执行拥有，本tester未删除或扩大其权限；最终脱敏归档与凭据副本清理需要主执行记录。结果变更后应重新固定摘要，不能把本结论无条件沿用到追加探测。

建议下一份真实探测补allow_once唯一选项的决定与实际工具输出/文件摘要，再建立操作系统级完整进程树退出证据。当前可将本轮已观测能力记为WP1有限兼容证据，不改变CL-005/CL-030尚未产品验收的状态。

## 同日 allow_once 与资源补验（追加，不改写前轮结论）

收到主执行临时auth副本清理信号后，只读核验新增证据。以下新证据补充前节“尚缺allow_once成功”状态；原始cancelled探测及其限制仍保留各自版本归属。

| 新文件 | SHA-256 |
| --- | --- |
| `.tmp/cl-wp1-proxy-retry/result-approved-v1.json` | 2CF52A5C0AF81775321A8CB3AB5A5B4EE5D975B07D49DD1A06306412D2987B55 |
| `.tmp/cl-wp1-proxy-retry/result-approved-v2.json` | 83E5E66384CDC9210B5B0CCBD647154DAC12F369EC409B0101C2901A7F74596A |
| `.tmp/cl-wp1-proxy-retry/approved.mjs` | 18242CDF234DCAD85D0332C5E94C849C7D963E3135211161022AF91721F1635F |

两版各1次权限请求，实际记录的决定为`{outcome:selected, optionId:allow_once}`，该optionId在本请求广告选项中对应kind=allow_once。探针按请求中的实际命令和工作目录筛选选项，并向真实请求返回广告的optionId，没有凭空注入新选项。

- v1：真实tool_call `exec-08ca56c2-9ec0-4a3a-af15-08a96da47136`从in_progress更新至completed；rawInput为专用workspace下执行`Set-Content -LiteralPath 'approval-probe.txt' -Value 'WP1_APPROVAL_PROBE' -NoNewline`。permission轮最终prompt stopReason=end_turn，结果读取文件内容匹配true。
- v2：对应tool_call `exec-718dfa8c-e65e-45fb-be7d-5dba99ccc499`亦完成；terminal_update记录exitCode=0、signal=null，随后tool_call_update completed，permission轮最终idle/end_turn。结果读取文件内容匹配true。
- 独立读取当前文件原始字节，长度18，精确为无BOM、无换行的ASCII/UTF-8 `WP1_APPROVAL_PROBE`；SHA-256 `DF09D7F317D6BEBD1DD7C05FD79ACE8C52D56A00F325619F00A0102AD5A5F581`。探针的approvedFileMatches只是UTF-8字符串比较，原始固定字节判断来自本次独立读取。

两版共用workspace，因此当前文件字节不能分别归因于每次运行。主执行报告v2前文件不存在，本tester没有在运行前独立观察该删除时刻；v1的fileMatches=true也不能单独证明v1写入。但各自真实授权请求、指定命令及同ID工具completed轨迹，结合各自运行后匹配记录，已支持“本轮两版allow_once真实工具执行完成”的有限兼容结论。若要签每版独立、可归因的新文件落盘证明，应再用不同文件名并记录运行前不存在、运行后原始字节及摘要；当前不将该更强结论写成通过。

新探针共4个适配器进程均stdio exited、exitCode=0。2026-10-30T07:34:00+08:00，本tester使用require_escalated只读CIM按ExecutablePath前缀`D:/Project/acp-remote/.tmp/cl-wp1-probe/`过滤，匹配进程数量为0。此观测支持专用程序路径下当时无残留，不涵盖系统路径PowerShell等后代，也不证明产品Windows Job Object或完整进程树关闭合同。

独立Test-Path确认以下4个临时凭据副本路径均不存在，仅检查存在性，没有读取凭据：`.tmp/cl-wp1-proxy-retry/native-v1/auth.json`、`native-v2/auth.json`以及`.tmp/cl-wp1-network-retry/native-v1/auth.json`、`native-v2/auth.json`。未清理用户真实凭据、数据库或原生会话。

本次仅追加兼容证据，不签CL-005/CL-030或平台唯一审批/撤销/Job Object验收。产品完整进程树证据和按版本独立落盘归因仍有上述限制。