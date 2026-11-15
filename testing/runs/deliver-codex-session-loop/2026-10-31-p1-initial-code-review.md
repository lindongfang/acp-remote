# P1原始观察记录独立交付检视（初始冻结版）

2026-10-31。reviewer `/root/baseline_reviewer`，只读生产及开发测试，无产品修改。实际完整读取新模块、其开发测试和ACP README，重新核根`2026-10-31-p1-raw-observations-source.json`全部10项均一致：主模块`1e38ef176b4c2d505d05cc84aa506817952a91148e79b7e50d6b2574926f0dec`、开发测试8ee000b6、ACP mod b517c733、README7929e581；既有strategy72ad、transport93bd、Domain a22/d2、锁87f及a071合同均不变。独立定义8f8e不变。

**当前不通过交付：P1-CODE-001 Open，独立定向执行待补。** 其余初审未见具体阻断；开发8绿/严格clippy只作输入，不代替原R1–R8独立验收。

## P1-CODE-001：P2，先做形状校验破坏分流及Stale优先级

位置：`app/src/adapters/acp/view_observations.rs:153–159`，`context`从259行起。

observe在确认Incoming::Update后立即`context(event,raw)?`。context先检查jsonrpc/method/id、params/sessionId/update/sessionUpdate及coarse event一致性；之后observe才根据目标/状态Ignored和window_status检查。实际顺序与a071合同及8f8e最终定义规定的“先枚举/目标/v2 State分流，匹配显示Update先current handle再形状，合格才seq”不一致。

可由公开Incoming字段误用构造的确定分支：

1. event属于别的session，raw与其不一致：契约先Ignored、零统计，现会先InvalidObservation。
2. 本协商v2的目标State，raw不一致：契约先交State owner/本模块Ignored，现先InvalidObservation。
3. read已release并有新窗口，匹配旧event但raw不一致：契约先Stale、旧seq不动/新窗口不受影响，现先InvalidObservation。

正常由Connection返回的一致事件不会触发此误用组合，但R7明确包含公开字段误用、陈旧scope以及错误分类；不能用正常帧全绿代替该确定优先级。当前未发现它造成新窗口写入、计费绕过或控制写，问题级别为P2合同/边界语义，不能描述成已证实授权绕过。

窄修建议：借用event已有session和typed State先分流，不增加String/Value分配；匹配显示事件再window_status验证原handle，然后context核形状一致性，再checked seq与复制。v1同名未知state_update仍保留为显示观察，不能借此吞v1未知结构。仅修本模块和回归，不改变既定产品规则或增加真实来源authority。

已交原独立tester在原1e38快照按同预期定向复现，原主执行接受该顺序差异并将保留初版。待读取原失败源/日志后补动态证据；修复必须另冻结rev2，由本问题原提出者复核关闭。

动态补证：已实际读取request-tester独占`p1-probe/src/ordering.rs`三项完整源、`p1-first-test.log`和`p1-first-exit.json`（实际101）。真实Incoming后仅修改公开event，三个分支均实际Err(InvalidObservation)，分别预期Ok(Ignored)、Ok(Ignored)、Err(View(Stale))，全部失败。是原1e38产品顺序缺口的独立反证，不能把正常帧通过代签。该首次总运行6过5失败，其中另外两项是新夹具缺v2 permission.title、对已Incomplete执行finish误unwrap，tester按既定契约修夹具；不能把两项夹具错误混算成产品问题或改P1-CODE-001预期。原失败与exit保留，待新版本同断言复查。

## 其余静态检视

parse_header先取32字节再访问固定offset，magic/version/wire/origin/kind、seq>=1、raw非空、reserved0、usize转换/32+len精确checked均严格，不截尾，不解析JSON/授予来源资格；RawObservation仅借输入raw，Debug仅输出长度不泄露正文。无正常unwrap/expect、unsafe、新依赖、I/O或writer入口。

read只复制Strategy.version与handle，不克隆初始化raw/Value/长SessionId；StageWorkspace声明在ObservationRead末尾，固定状态先销毁才退read费用。每帧先收费768，32头写在scratch中，完整raw写已预留候选，单次stage Insert/publish，不能延伸buffer或隐存第二份raw。工作区/候选/旧cell的预算及实际分配边界复用已验Domain，新增Read304/Stats56/Raw32当前为开发证据，独立布局及峰值还待验。

序号和drop计数checked，CounterExhausted只关闭本read显示登记，不改Connection/pending/deadline或取消工作；close_counter用旧handle调用fail不会污染新窗口。显示Capacity/Structure保留首因/边界并后续可继续，真实已FIFO淘汰事实仍由window_status取得。Stale出现在store前后正常路径没有异步mutate窗口间隙；未发现新资源任务或无限队列。

本阶段Incoming没有连接proof且同read只应由实际owner组合一个ObservationRead，源码明确不是授权token。这些真实来源/唯一读取所有权尚未装配，不因context字段比较或非Clone就签成实际安全准入。

## 范围与待证

等待原testerR1–R8同版实际公开API/真实wire、长流/超限恢复、source前后/锁/退出码及原回归。私有极值注入、选定真实分配故障与可达/不可达分支要分别记录，不能复用开发8项代签。

保留后续delivery风险：原Domain的4096条数同时覆盖对象及snapshot引用，4096个小None-key raw对象可让full snapshot恒Capacity，P1又不能按None key item_lease。纯保存/publication/status尾位置不等于首次订阅读出保留内容；不改本包预算或扩大Domain，后续必须有有界读取/引用余量等方案与“已满且无后续输出”独立用例，不能永久取消完整3.2首开/有权读取义务。

P1只保留原始观察，不解释typed current、工具锚点/原位更新、轮次/分页，不关闭VPROJ-PLAN-002或完整CL/L5/L6；003在观察记录层的定义通过也不代签P2聚合来源。无真实Harness/owner/WSS/Web结论。
