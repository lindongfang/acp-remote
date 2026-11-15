# 独立验收可达性与门禁交接 v1

本文只提供工程风险及可达入口，**不是tester的独立用例定义**，没有测试执行或独立通过结论。独立tester须按当前tests.md稳定ID自行完善新阶段定义/资产，reviewer核方案与用例后才可编码。现完整tracked仍由root管理，本文件不勾任务。

## 1. 当前输入与正式证据边界

当前ACP+R3的306项根清单为已独立接受基础；原根完整检查、构建、513函数等证据只覆盖其当时范围，不证明本候选view/Node/WSS/UI可用。原P1/P2/预算测试保护历史模块，不证明新无预算路径。ND revision4有工程阻断、revision5未冻结；NativeHistoryPort、capture tap、可信target/view绑定是新增明确依赖，未实现不算产品能力。

产品验收门槛分开：模块可先按真实ACP duplex/子进程证明source/raw/worker语义；L5必须真实node认证/撤销/网络writer/存储，L6真实Windows节点/Web/SQLite/Harness原生来源/进程树。缺后者保未执行，不能把协议夹具、UI截图、traits编译或多个数值相同的计数代签。实际新动态数量由tester结果列明，不以本表行数计测试数。

## 2. 应由独立tester覆盖的风险组

| 主tests关联 | 必须能观察的断言 | 实际可达/隔离安排 |
| --- | --- | --- |
| CL-003/004/025–027/042 | 真实当前输出已capture、没有后续输出首开仍见；view失败不触发保存/取消；旧body库原字节保留且未读作恢复 | 自有ACP pair+capture hooks；自有SQLite旧表canary，禁止用户DB |
| CL-034/049/095 | >32MiB一个raw、>16MiB一个target、节点总>128MiB、>4096/32768条、>64 target、长tool/message ID；snapshot引用及版本持续增长不预算拒绝/驱逐 | 每项总数/全字节SHA只在自有fixture，不日志原正文；内存不足记实际故障，不降样仍称跨阈值 |
| CL-034/049/050 | 未知巨大整数/小数词法、原键序/数组序、unknown深结构、null/省略/patch、工具晚完成原位；同形文本重复不合并 | 真实Received源；前后byte compare+anchor/version事实，不能自造Received |
| CL-051/064 | 同source多个启动别名同key；不同source同短ID隔离；actualscope换代旧capture拒绝；缺跨load稳定ID重建，旧响应不能覆盖 | 同可信source fixture/两真实ConnectionScope，各自reader实际发；反序延迟响应 |
| CL-047/052/053/106 | CurrentOnly能力独立，恢复/读取/Run完成分开；无重放明显限制；重连丢重放不复活审批/历史 | 模块真实ACP合同对端只证明通用语义；实际Node/Web/真实Harness分支另列 |
| CL-048/049/050/051/052 | 同目标同时open/refresh只一个在途真实Replay RPC；配置MCP/额外根/未知raw不改空；原RPC期望匹配 | 新NativeHistoryPort实现后真实same Connection wire；无端口则受阻，不fake config/scope |
| CL-050/052 | 匹配Replay完成响应早于flush；flush挂起或Err之后仍保已得ReadCompletion；错误RPC/Resumed/snapshot-end绝不冒Complete | 真实WriteFence/provider边界及SourceSlot operation registry；先匹配response再flush error真实窗口 |
| CL-049/050 | Complete前丢capture/结构错误/EOF标未完整；Complete后断开只标stale不编已完成段缺失；边界后capture故障只影响后续范围 | 完成proof真实seq/receipt order，worker barrier pause，原response raw保留 |
| CL-051/064 | 首开cut前后、H前后、快照首块/末块/tail切换都不漏不重；无下一事件也唤醒；stale cursor拒绝 | cfg(test)只暂停真实边界，实际Received到capture到snapshot；不把peer.write当reader receipt |
| CL-049/050/064 | 慢投影/慢reader/大量raw/busy browser时，控制Received、输入、审批/取消仍进展；capture失败不进ND gap/ack/write gate | pause worker锁外/只detach边界；真实ND observed后核control ack与writer次数，standalone只证明较窄capture不await |
| CL-025/026/050 | fallible分配注入：原文保留失败sticky incomplete，projection失败raw仍可读，read buffer失败只该读失败；没有执行cancel调用 | 注入显式try_reserve wrapper仅cfg(test)，用真实raw事件；真实OOM不冒已验 |
| CL-031/064/092 | A撤销后新open/next/read/send拒绝；A已受理输入/保存审批继续由node负责；B仍有权可读同节点view | L5真实认证+TrustStore撤销事务+实际socket writer；不能用固定true authority |
| CL-031/064 | 撤销先于copy、copy后queued未发送、writer已拿gate发送中三个窗口；只承认已开始chunk不可撤回，之后零chunk | 复用真实write_locked/queued pause hooks、现writer_holding_actor_gate/revoke_commit_discards_queued_result语义；新binary同路径 |
| CL-049/095 | transfer超过120个chunks不触发120管理命令限制；credit错stream/跨socket/超ack/重复ack不多读；断线节点raw保留 | 新72字节binary实际WSS；一原文跨>120片，全字节核对，慢socket不取消原生 |
| CL-031/034/105 | raw大数不经JS Number，未知深树不递归爆栈，HTML/SVG/ANSI/双向控制安全；未知结构可惰性读原文 | Web实际unit/check+浏览器交互；字节保真和无执行/无网络请求分别观察 |
| CL-031/105 | 原生URL/附件不自动抓，私网/重定向/越根不能借chunk API获取；审批提示不能触发管理动作 | 请求计数隔离server+真实browser；无资源入口应明确unsupported，不测试mock假能力 |
| CL-006/007/027/064 | 断浏览器保view/work；取消读取不发session/cancel；释放/换代/重启使旧epoch失效；raw不持久化 | 生命周期event实际slot签名；旧cache正常失效不是永久保存保证 |
| CL-007/050/064 | shutdown seal/取消/join不遗弃任务；未终态read Ticket留原native owner；超时报告未确认而非虚join | cfg(test)真实任务暂停+handle计数+关闭返回；Windows Job关闭证据仍归ND单独真实测试 |

超阈值、未知结构、深层销毁、原文跨度、snapshot+tail、source伪造、撤销和完成响应时序都要有否定断言；只看“得到一个success”不足。传输帧数量和view条目总数分别记录，不能将chunk数当原生消息/轮次数。

## 3. 测试插桩要求

只允许cfg(test)观察/暂停真实生产路径：AfterCaptureEnqueue{actualscope,sequence}、BeforeWorkerDetach、AfterWorkerCutProcessed{cut}、AfterSnapshotH{epoch,H}、BeforeSnapshotEnd、BeforeChunkCopy、AfterChunkQueuedBeforeWriterGate、WriterGateHeld、BeforeViewWorkerJoin。hook不带正文、不改Received/Scope、不mintproof，不在生产加yield/可写状态setter。

控制进展测试应把worker停在锁外、浏览器停在自己的credit/发送边界，同时实际发送permission/state或其它control，看同ND ack/当前native writer真实次数进展。不能把capture完全不调用与“慢view隔离”混为一谈。handoff锁仅用于O(1)batch detach及引用入队的静态边界需reviewer逐项检查，动态额外用接收/投影压力证明没有view/client锁循环；不声称动态测试证明硬实时分配上界。

分配注入只针对新fallible路径，不改raw_json基础层或全局allocator假造错误；一次故障不得被后续成功掩盖丢失范围。Debug canary涵盖未知数字、native ID、正文、配置、工具提示，不把原文写快照/日志。

编译负例需在外部client probe证明：Received/Scope/SourceViewRegistration/VerifiedViewTarget/ReadCompletionProof不能从public字段/Deserialize伪造；合法真实SourceScope可以只读clone但不获得reader/ack/权限。只检查private文字不够，仍需真实异scope/异target动态拒绝。

## 4. 工程检查与资源

编码后从正式README/package配置执行实际检查，独占target建议 `.tmp/q3-shared-view-coder-target`；tester必须另用自有路径，不能争用ND build target。既有依赖已可用时保持--locked，不跑安装/外网审计。Node/WSS的自有数据根、fixture端口、客户端身份和原生会话由独立tester建立并清理，只删除其所有资源。

实际集成后完整npm run check、npm run build:rust、前端check/build与含新fixture的strict all-target检查必须真实执行；已有同306范围结果可作基线回归参考，不能代新源码。命令、退出码、实际函数数量、前后源SHA、源/端口/进程清理分别输出独立报告；已通过的限定阶段不得覆盖缺L5/L6真实链。

## 5. 编码前尚须独立完成

tester生成本阶段精确稳定用例，reviewer核当前全部来源和可达性；ND最终正式接口gate后，重新固定capture tap、原Replay binding、response proof的具体生产实现与重叠文件owner。当前只是冻结设计候选，未有可据以开始产品编码的独立PASS。没有新产品/安全/数据权威待用户决策；剩余是已授权范围内工程/验证门槛，由相应角色完成，不把此作为向用户重复索要权限的理由。
