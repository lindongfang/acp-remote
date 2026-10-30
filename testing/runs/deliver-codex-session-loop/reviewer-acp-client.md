# 公共 ACP 第一候选独立审查

2026-10-30。结论：**BLOCKED（ACP-R001/002），仅公共客户端与版本策略候选，非真实Harness验收。** 独立reviewer只读源码/固定标准schema/日志，未继承coder自评，未改产品/plan/任务。

根：`C:/Users/jack/.codex/worktrees/codex-session-wp2/acp-remote`。冻结 `.tmp/wp3-acp-logs/frozen-files.json` SHA256=`C38150F250C0BC2DFBC73717B7397F89F801BE9D43FDEA9C0E886425A70C1A09`，实际首尾8/8文件匹配；原WP2 2.3清单14/14保持。核读5份日志并逐摘要5/5匹配，全目标开发217库+4JWS通过及owner/fmt/clippy成功，未把它们代替协议正确性。8文件仅adapters目录及lib模块声明，未带入Cargo/platform/storage/node/policy/trust修改。

本reviewer实际运行已有编译testbin `.tmp/wp2-target/debug/deps/acp_remote-266e97892ace37dd.exe adapters::acp::tests:: --test-threads=1`，exit0，17/17通过，1.62秒。它们是duplex fixture，不是实际Codex/DeepSeek、资源owner/gate/writer或正式CL验收。未独立重编译217项；编译物与冻结源码关联采用对应实际all/contract日志，结束源码摘要不变。

## ACP-R001 — P1：未知v1 stopReason被直接提升为完成

位置：app/src/adapters/acp/strategy.rs prompt_result（约200行）。v1分支只要求stopReason是字符串，随后直接构造PromptResult::Completed；因此`{"stopReason":"future_or_invalid"}`甚至空字符串都会被编码为完成。固定官方SDK v1 schema的StopReason只定义end_turn/max_tokens/max_turn_requests/refusal/cancelled五种；本次已实际读取该schema。未知结构可保留原文，但不能把未知的控制语义当可靠终态，导致上层提前释放Run前台互斥。

最小修复：按固定版本显式验证已实现终态，未知值返回明确不支持/InvalidMessage或专门未知控制事实，绝不构造Completed；raw仍可保留用于安全诊断，不拼正文错误。v2 state/stopReason由后续owner解释时同样不得因任意字符串解除Run。新增未知/空值/非字符串以及五种合法值合同测试，断言未知不成为完成且无副作用重发。现有17fixture只覆盖已知cancelled，未涵盖该分支。

## ACP-R002 — P2：提交Request未绑定当前已协商策略，跨连接可绕过能力门控

位置：strategy.rs Request/Strategy请求构造与transport.rs Connection::request/submit。Request字段私有但对象可从任意已协商Strategy公开方法构造；Connection::request仅检查phase Ready，submit直接用request.method/params/kind发送，不检查request来源版本或当前连接capabilities。

可达调用：连接A已广告image/resume，使用A.strategy().prompt(image)/resume生成Request；将其交给连接B.request，B即使未广告image/resume（或selected不同版本）仍会实际写出该操作。Connection冻结策略没有在实际发送边界保持，调用者拿错Strategy能跨过本应由版本/能力决定的标准语义。权限请求已使用私有连接tag防借用，但通用Request没有对应约束。

最小修复：Request携带可验证的版本/能力构造范围，并在目标连接发送前按目标Strategy验证操作/params，或将请求构造封装为Connection自身的方法；允许同版本同能力复用不必按厂商分支，但目标不满足能力/版本时必须零写入。新增两个不同能力连接及不同selected版本的迁移Request测试；拒绝时不污染无副作用的正常目标连接，也不自动回退重发。此项为内部API正确性，不能用未来应用调用者“保证不拿错”替代公共端口合同。

## 其他已核边界

- 单次初始化、响应typed ID关联后读selected版本，v2 offer选v1按v1字段解析；v1可选info/caps省略与v2 baseline session能力区别实际对照固定schema，不假定所有字段必填。没有厂商名称分支。未知字段保留原始字节及结构；错误/重复/无匹配响应/EOF/超时关闭而不重发。
- RpcId区分整数/字符串，session/tool/message/option独立类型；权限原文+哈希+连接私有tag及重解析原广告阻止篡改/跨连接借用，未知subject/option不允许selected，cancelled可关闭未支持请求。这里仅真实连接来源，不是应用审批发行者；Run/actor/代际/gate及工具同域关联仍属后续owner。
- v1 prompt响应与v2插入messageId分开；v2 resume要求同session state_update发生在响应前并标DuringResume，不把快照当新完成。v2具体可靠Run终态仍需owner按协议和所属工作协调，本包不应被当现成Run状态机。
- 读缓冲保留取消时部分帧；写future在首await前先使phase Closed，仅write_all+flush成功后恢复，未知不允许重试。独占read/write和关联表，无独立后台任务；shutdown绝对deadline清关联/关闭writer，完整进程树仍平台owner负责。17测试包含取消读/写、边界、计数容量及关闭，未做真实进程/慢网络。
- ACP独立32MiB帧/深度96/结构token/ID/未决预算显式失败，不放宽管理WSS预算；序列化FrameBuffer在分配中限额。unknown消息/正文Debug避免输出内容；安全错误固定。
- 固定标准依据为主D探测SDK2.1.0 v1/v2 schema，原报告摘要1C57095A...与28C7251E...；完整标准与真实程序行为不由本次fixture穷尽，v2仍Draft，后续升级需复验。未新增依赖。

ACP-R001/002 Open，修复后按新冻结清单由本reviewer复查；独立tester的结果另行承担，不因17通过代关。禁止在问题未闭环时把该候选签为完整3.4或最终任务完成。catalog的§10定义门槛已单独记录通过，不受本包内部问题倒推为未通过；平台PLAT-T001仍另行阻断其集成。

## 同日追加：ACP-R001/002 修复原问题复查

新清单 `.tmp/wp3-acp-r001-r002/frozen-files.json` SHA256=`55FD2951F95A3B420554970FA27F905008860477520F9474F6B86F176B4D875D`。实际首尾8/8文件匹配，5/5验证日志摘要匹配；旧C38150F2...清单保留不变，原WP2 2.3 14/14摘要保持。核读新增源码/回归和已有all-final日志（219库+4JWS，owner/fmt/clippy成功），未独立重编译全目标。

本reviewer实际用新已有testbin执行同一过滤命令 `adapters::acp::tests:: --test-threads=1`：exit0，**19/19通过，1.76秒**。含原17回归及新两组矩阵；不是19项真实Harness能力。运行前后源码保持冻结，不修改产品或计划。

- **ACP-R001 Closed（公共客户端范围）**：prompt_result明确仅接受五种固定v1 StopReason；空、未知、非字符串或缺字段不会产生Completed。next路由语义拒绝后关闭连接、清未决，不自动重发；保存一份有界RawMessage供调用方take一次，原字节保留且Debug仍不打印正文。独立重跑五种合法值、五种错误形状、缺字段、raw逐字节/单次取出、关闭后零重发，以及v2未知state仅作为原始控制事实而非Completed的回归通过。后续owner仍必须验证v2状态与所属Run，不能从原始字符串直接推出完成。
- **ACP-R002 Closed（公共客户端范围）**：每次selected Strategy产生私有Arc scope，clone保留同一协商来源；各业务Request构造携该scope，外部无法设置，initialize独走私有None路径。Connection::request在Ready检查后、submit分配ID/登记pending/序列化写入前核Arc来源，跨连接/旧Strategy返回InvalidRequestScope且target保持正常。测试覆盖v1/v2源×v1/v2目标的能力差异、来源关闭后旧Strategy、零字节/零pending/ID未消耗，以及目标自身合法new继续完成；独立重跑通过。更严格的每连接来源隔离不按厂商分支，不妨碍同协议实现复用。

**两项原问题均Closed，本冻结公共客户端/策略候选限定PASS。** 独立tester新同版外部复测仍由其报告负责；主执行应收齐同版结论后才集成8文件，并协调lib模块声明保留已合平台与存储基线。未签3.4最终任务完成、实际ACP对端、进程启动/关闭、真正审批issuer/actor gate/writer或Web；原非真实fixture及Draft升级限制全部保持。旧平台问题状态以其最新独立报告为准，本ACP复查不代关平台问题。
