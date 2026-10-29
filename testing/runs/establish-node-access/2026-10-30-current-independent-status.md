# 当前33例独立验收汇总（正式嵌入Web证据合并）

本表取代本tester旧报告中的“当前累计”用法；旧文件仍保留各自执行版本和历史事实。依据当前NA001–033有限Steps/Expected，逐例采纳独立原始执行/已核读独立报告，不按报告标题或主方期望自动签PASS。

**当前：33 PASS / 0 PARTIAL / 0 FAIL / 0 NOT-RUN，共33例。** NA012已核读原平台tester固定R20跨SID补验及追加清理证据后合并PASS；NA007-CANCEL-001与NA-E2E-001原提出者关闭结论保持。旧失败和PARTIAL报告保留历史。tester验收已无剩余阻断；最终reviewer与归档流程不由本表代签。

## 目标与证据复用

最终本轮动态目标：R17普通嵌入binary **38AA9D02F334E3C81FDC900ED4F8943BB78339650981BC0046A609381D03E6C2**；冻结83项manifest **5074890C209A702335F13042A26DBA64278EFD7A7B0A70A4E42E2F698BA3E26F**，本tester前后83/83。实际管理/Chromium执行与清理详见 `2026-10-30-r17-management-independent-acceptance.md`。

追加R19独立cfg(test)执行：冻结83项manifest **298EA007D4E1270E15D37DE93FC680A76A1121D452ECF2B96AD8F2E60B1B32C1**，前后83/83，writer先赢测试1 passed/143 filtered/exit0；见 `2026-10-30-r19-na008-independent-acceptance.md`。R19ordinary binary377C780C...6C38A7仅核对hash、未运行；不将本次库测试说成新普通exe或Web重跑。

新增正式Web链合并详见 `2026-10-30-web-evidence-merge.md`（该合并当时NA007仍FAIL，历史范围保留）。最新R20 ordinary **561A6024E983DD5239C09D1388CA5D256625F690EE9345E64B9654BC43AAFBE3**、manifest **73727BCF617F3F96BCC24C4CBB7B38BA044A56856C6CB4D628869832C8B368A4**，本次独立核对冻结83/83及binary hash；当前工作区仅plan/tasks/review三份新metadata不同，其余80项相同。原NA007三分支和同key重启恢复已采纳，见 `2026-10-30-approval-cancel-r20-retest.md` 及本tester `2026-10-30-component-final-assets-check.md`。未把本次核读说成本tester亲自运行R20 TTY，也不为CLI变更重跑未改Web/后端。

既有完整Rust/Web工程检查按同固定版本实际日志复用；本tester独立运行量另列在各定向报告。旧debug未启debug-embed的exe hash不能冻结网页，故不把旧网页说成R17网页。独立协议、存储、DPAPI、native pipe、生命周期等可分离机制不因旧Web资源嵌入问题全部作废；按负责生产路径、后续修改影响和实际断言复用。R15 SC003原reviewer已动态关闭，R17主方完整143库+4/Web50及build和独立review为当前工程交接证据，本轮另执行OpenSpec strict exit0。

## 唯一逐例表

| ID | 状态 | 已采纳依据 / 准确剩余 |
|---|---|---|
| NA001 | PASS | 本tester历轮真实工程命令、当前同版主方完整check/build日志及reviewer核读复用；本轮OpenSpec strict、范围diff检查。未把定向1测试说成独立全量143项。 |
| NA002 | PASS | 原独立供应链/密码库合同审查与implementation-r15-review：官方cargo-audit/RustSec、根/Web npm、许可证/平台、Gitleaks正反控制及真实commitlint；SC003普通debug离开/修改自有dist仍返回嵌入原三资源的独立动态闭环。锁文件未改，R17仅Vault协作及cfg(test)增补，不要求全部依赖形式化证明。新增报告/metadata不冒称属于此前扫描文件集。 |
| NA003 | PASS | 本testerjws-challenge报告：真实Chrome固定7+负向25+额外13、Rust真实DER/有效压缩点/双向新签名与原始ASCII控制；负责JWS字节未变。 |
| NA004 | PASS | 本tester独立22挑战场景＋14产品挑战测试，绑定/用途/失败消费/重取/回拨/熵失败；r6再次22通过。 |
| NA005 | PASS | r6独立12配对测试、真实binary非交互拒绝、SQL秘密不落盘；R12新增5终态claim封闭独立复验，pending幂等仍成立。 |
| NA006 | PASS | budget-acceptance最新追加已核读：R10 1024桶/299–300秒/固定窗口组件、真实混合计数、128/129且原128存活、R14 IPC32/33及原32存活/释放恢复、实际300秒pair流、R17真实WSS慢writer字节/帧先到4429。保留clock、SecretProtector、writer暂停/帧分支刷新窗口的明确适配器边界，不继续引用其上文历史NOT-RUN。 |
| NA007 | PASS | 原提出tester R20真实TTY control/precommit/committed三分支各一次Ctrl-C、退出均0；COMMIT已发生但approved未交付时，CLI先给诚实条件提示，真实owner list按相同指纹确认实际ID及完整revoke命令；未提交分支不把另一trusted记录误判本次成功。DB原信任保留、同key真实页面重启恢复原ID。30份证据摘要本tester独立重算全匹配，NA007-CANCEL-001由原testerClosed；与原SQL语句/实际COMMIT故障回滚及响应丢失证据合并，整例PASS。 |
| NA008 | PASS | 既有RV001/002受理/普通与缓存结果入队、撤销先于writer及队列栅栏，加R17双B订阅/失败提交/自撤；R19本tester独立writer持gate栅栏，实际poll撤销50ms未提交、DB trusted且写计数不增，释放后send成功计数+1再COMMIT，实际原结果后4410。最后有限缺口已补，不靠客户端时戳推断。 |
| NA009 | PASS | 本testerr6实际DPAPI子进程partial/pre-flush abort，长度/abortmarker/正式key/meta无/marker保留/首建重开稳定/close-cleanup；原rename/meta/损坏/未知schema全部独立重跑。 |
| NA010 | PASS | r4/r5a普通产品ApplicationPolicy真实挑战/ES256不可伪造actor及DB共享gate；无/过期/未来未验证审批均dispatcher0；后续Harness明确mock。 |
| NA011 | PASS | 本tester实际origin/TLS/不可写SQLite/占端口失败后修复重试，正常stop及锁/端口释放、同身份保持。 |
| NA012 | PASS | 原platform负责字节复用链覆盖1004 SID status/list/revoke、文件/pipe DACL、服务端token SID及独占；R8五点模拟错误与真实Drop Revert恢复、SMB同UNC正反控制、正式E2E owner无Web资格恢复均已核读。新cross-sid-r20报告真实abc SID1006 pair/stop各exit1/os error5且TTY有效；DB逻辑摘要不变，owner status/list/stop0，pipe/监听释放且锁可独占重开。原final watchdog=true保留；追加两次absent确认最终清理。详见final-independent-merge报告，不声称两新命令到达服务端token校验或历史命令为R20重跑。 |
| NA013 | PASS | E2E最新正式R16实际listen loopback/Tailscale切换，同data/key、同origin及原A profile；远端IP分别127.0.0.1/100.119.123.57，均secure，三操作未认证拒绝/可信成功、ID/key/node连续。最初代理失败/hash检查未启动历史不计通过，新实测已补齐。 |
| NA014 | PASS | runtime-secret独立8项认证拒绝/56条网络与实际日志审计历史存储检查；claim-origin报告r6→R9负责路径影响分析。R17本轮实际输出/审计/code不入URL/local/session/IDB和无第三方资源补充。旧Chrome观察不说成R17重跑，未变封闭错误/JWS验证和元数据边界按范围采纳。 |
| NA015 | PASS | E2E最新正式R16再次实际未完成TTY pair＋多订阅＋真实SQL在途操作stop、事务真实收敛、无孤儿及重启恢复，补足旧debug网页固定性边界。4500/未收result/该次10秒热重连未观察事实保留，不要求每连接成功响应。R14新增IPC背压可取消、R17本tester正常stop收尾亦有独立证据。 |
| NA016 | PASS | 本testerr5a默认deadline真实DPAPI/NodeRuntime/owner IPC＋Storage.close阻塞，15000配置/14000清理预算，observed14010ms真实失败报告、未删data伪造成功。不是声称实际等满15秒，也不加无穷故障组合。 |
| NA017 | PASS | r6-e2e实际A有效/B撤销重启、key损坏/独占不可读/meta不符三副本启动失败且不替换/清库；历史真实跨SID DPAPI仅作未变CurrentUser/固定entropy平台子路径复用，不签当前NA012完整跨用户矩阵。 |
| NA018 | PASS | 本tester实际device pair TTY＋真实Web输入/指纹、绑定名称/公钥指纹/到期、尚未批准管理三类拒绝。 |
| NA019 | PASS | 实际终端明确批准后Web新WSS持有证明/稳定ID/list，批准本身不替代认证；提交前失败证据独立存储路径采纳。 |
| NA020 | PASS | 原NA-PROTO001由本tester修复后真实TTY Ctrl-C无需额外回车进程退出；y/yes/n/Enter/退格、IPC断开/网络无批准入口有实际证据。 |
| NA021 | PASS | claim-origin独立报告两真实独立profile同码9ms竞争、原proof重试相同结果、改proof冲突、其他key不能覆盖、有效/撤销key新码proof401。R12终态guard不改变pending重试控制，本tester已重跑pending原proof幂等及全终态拒绝。新WebLock是各profile内部协调，不把旧网页资源误称R17冻结。 |
| NA022 | PASS | 本testerR12新binary原9行HTTP红灯全绿、批准结果丢弃后留存内新proof原ID；3项冻结精确clock测试覆盖自然300秒及max300/300/359，NA022001原提出tester关闭。 |
| NA023 | PASS | budget报告最新真实各HTTP/WSS阈值/混合计数/128连接及1MiB/256真实Outbound至4429的有限调度路径完成；不把初始仅组件容量当真实网络，不继续写旧“未跑128/300秒”。 |
| NA024 | PASS | claim-origin实际既有Web pin下替代key/错nodeId/origin/坏hostProof均0签名/0proof、原绑定不变；40项真实Origin/Host表及正控制。原Steps未指定另一个“全新无pin接受自洽替代节点”实验，该情形不能凭空期待客户端识别未知真key；不将它额外加成验收门。已覆盖明确字段错误、失效签名和既有绑定替换。 |
| NA025 | PASS | 本tester真实5秒nohello、15秒无proof、错key/重放、提前管理、8+1第9关闭且原8继续info、一次挑战消费。 |
| NA026 | PASS | E2E最新R17真实批准事务trigger失败：TTY storage_unavailable、DB无半信任、浏览器cancelled/clientId=null且无AUTHENTICATED、失败key新鲜auth4401；移除trigger后同key真实再批准、刷新/重启原ID有效。新鲜且pair/status刚成功200的配对proof用于长期auth仍4401，不拿过期claim替代。 |
| NA027 | PASS | 本轮r17-management报告：真实WSS读计数不增、实际失败撤销/两B仍有效、重放/冲突/新request；R7 SQLITE_FULL与R12满10000精确24小时子项均原tester独立执行，原storage实际COMMIT失败/规范化摘要复用。 |
| NA028 | PASS | 本轮两B真实订阅并发info/list、4410、原key4401；A自撤无success但DB/list重连确认，owner重新批准恢复。精确提交后的入队/发送由原reviewer门/队列栅栏证据＋当前多连接集成收敛支持，不把客户端时戳作内核commit测量。 |
| NA029 | PASS | 本轮R17真实Chromium70集合32/32/6无重漏；真实跨actor/node/篡改/59.1成功60.2秒失败、真实新信任/撤销旧cursor失效、三次变更最终73及Web最终75、三字段通知、静态安全头、审计输出/IDB清理。精度与合成67记录边界见正式报告，不说70次真实批准。 |
| NA030 | PASS | 同NA010普通产品真实actor→审批gate/dispatcher0动态证据；无Harness实装声明，未来时间也不能伪造审批。 |
| NA031 | PASS | web-evidence-merge逐项表合并：真实IDB/不可导出/读写故障/坏key/私密/清站点提示/新origin，正式R16/R17普通profile全进程退出原key/ID认证；同origin重连/4401/4410再批准、错误pin保护、真实页面受控生命周期与终止调度。原NA-E2E-001正式包原testerClosed，R17双页一次rotate、R18真实晚到旧4410遇新consumed record保持新key/ID；未变client请求超时不重发本次又独立1 pass/14 skipped，fake socket/timer边界明确。不因一个问题Closed直接签整例，也不另加私密/导航无穷矩阵。 |
| NA032 | PASS | E2E最新正式R16嵌入三资源字节/83项/独立A-B普通profile，真实TTY批准/认证/list/订阅/两B4410/原key4401/同data重启A有效B拒绝/owner正常关闭，实际secure TS地址；R17保存/旋转、R18晚到分支新测，其他未变机制按范围复用。不是将旧r9裸exehash当固定网页，也不只凭本tester单页R17管理签全链。 |
| NA033 | PASS | 用户在2026-10-30-user-acceptance.md原始主观理解结论已通过；不复活旧NA034/旧安全签署或要求用户重复Harness技术验收。 |

## component任务可独立收口

**tasks2.5可完成**：完成判据NA011/015–017已PASS，不因NA007/008/012/013/026/031/032未闭锁住该component。已核读R14独立review与budget最终ordinary binary容量/停止恢复，覆盖新增backpressure停止路径；本报告只给判定，未修改任务勾选。

**tasks2.4现也可完成**：NA006/008/023/027–030原判据全部PASS，R19已补唯一writer先赢栅栏。不得将其扩成NA028“每个并发命令必须成功”或NA015“所有连接只能1001”的新要求。其余component按自己的原用例集合判断；本表未修改任务勾选或代reviewer签署。

**tasks3.2、4.1已满足**：3.2原NA024/025/031及真实普通profile完整退出后原身份有效已满足；4.1原NA001/002/015/032及正式同包资源/受影响集成检查已满足。R20仅CLI修复不否定这些未变Web/后端PASS。NA007与NA012现已完成独立证据闭环；tester全例通过不代替reviewer最终交付检视。

**tasks2.3、3.1已满足，主方已勾选**：2.3原NA005/007/018–026全部PASS，NA007现已由原testerR20真实复查闭合；3.1原NA018–026及码不入URL/日志已全部独立满足，本来不包含NA007。主方回报apply实际11/15。NA012补验现已采纳，2.1与4.2的tester验收条件已满足，详见最终合并报告。

**tasks4.4最终metadata交reviewer核验，不由本表预签**：此前本tester实际长期定义checker33例/28场景/2能力通过，增量与长期规格正文/用例一致，plan已有正确旧NA033合并删除、旧NA034→当前NA033映射，R20冻结及检查/扫描证据已核读。早于本次表更新的plan/review曾保留31/1/1与NA007待复查文字；后续由各维护者和reviewer按本表33/0/0、原R20 Closed及新metadata独立摘要核验最新资产，不将早先文字当新阻断。NA012已合并PASS，最终资产及归档仍需reviewer独立核验；本tester不修改plan/review/tasks，不预签整个change。

### NA008有限栅栏逐项校准

本节已按R19实际执行更新：有限行全部通过，NA008整例PASS。不是重开原reviewer已关闭的NA-RV001/002，也不要求调度无限排列。

| 有限位置/次序 | 已有实际证据 | 判定 |
|---|---|---|
| 撤销COMMIT先于受理 | 原reviewer独立revoked_actor_cannot_be_admitted_or_enqueue_a_completed_result的Admission栅栏：暂停→owner revoke提交→释放，admitted增量0、enqueued不增、4410 | 子项PASS |
| 受理/动作完成后，COMMIT先于普通结果入队 | 同测试Enqueue普通结果分支：admitted增量1，撤销提交后enqueued不增、4410；不承诺撤回已完成动作 | 子项PASS |
| 真实幂等缓存命中后，COMMIT先于入队 | 同测试cached分支先实际revoke产出缓存，再重放命中→栅栏→撤销actor→释放，无结果入队、4410 | 子项PASS |
| 撤销事务失败，不伪造COMMIT事实 | 原独立语句/实际COMMIT失败回滚；本轮R17真实WSS失败撤销后状态/generation/revision/audit/result不变，两条B仍成功操作 | 子项PASS |
| 已入队/出队后，撤销COMMIT先于writer资格门/socket提交 | 原reviewer独立revoke_commit_discards_an_already_queued_management_result：pause_next_write在writer gate之前暂停，owner revoke提交后释放，只见4410；本轮两B订阅同时收敛及新证明4401 | 子项PASS |
| **writer先取得actor gate并通过资格检查，再有撤销竞争同gate，先完成sink.send后才允许撤销COMMIT** | R19独立执行writer_holding_the_actor_gate_finishes_before_a_competing_revoke_commits：WriteLocked位于gate/资格检查后、send前；实际poll撤销future50ms未提交，DB trusted、protectedWrites不增；释放后计数+1再COMMIT，原结果后4410 | **子项PASS；NA008整体PASS** |

此前要求的最小after-gate测试测量入口已由R19 cfg(test)提供，并由本tester实际执行通过；当时普通外部probe拿不到私有gate的问题不再保留为当前缺口。仍保留已交网络栈字节不可追回的合同语义，不要求每个并发命令有成功结果。此次为明确FakeProtector和writer栅栏的真实TLS/WSS节点测试，不冒充ordinary binary内置故障入口。

### R18交接范围补记

用户交接R18普通binary摘要328D7CEBCA708A300D48AA673204E4E52F1131E6E05951F85DA4601DA7AA2800，仅main晚到旧拒绝遇已消费新record的恢复分支变化、后端完全R17。本tester未亲自运行该ordinary binary；现已核读原E2E tester同hash、冻结83/83及原生CloseEvent延迟释放真实页证据，按来源采纳，不再列等待。R19仅cfg(test)新增writer测量，管理027–029不需重跑。plan/task新metadata不冒称由旧83项manifest覆盖。

## 已采纳外部独立报告快照

| 文件 | 本次读回SHA256 |
|---|---|
| 2026-10-30-budget-acceptance.md（含R17 writer、300秒、R14容量与SMB追加） | FE59AB5D5CC7AD6E7223E548E23E40C1EC5D0AF662D6544DB81D7AEDCF9EEABC |
| 2026-10-30-implementation-r15-review.md | 64D1AA00B7CC588AD2CBFA729188D73791ACFE25A5251E68A005F38DEBBC24C0 |
| 2026-10-30-runtime-secret-acceptance.md | 4208FEA37729516E99061BE1F7F3832F0F6D5E8624D037C14ADBFBEF5220B97F |
| 2026-10-30-claim-origin-acceptance.md | ACD945480FA7AC08A0100190B58122838A2D08805EC49A0D58E903C474C13864 |
| 2026-10-30-r6-e2e-acceptance.md（最新正式R16/R17/R18追加） | F5DAE2EAA0041D2574F9ED45B932367BB29FFA8EDF8525661C8EF3E0389E17AF |
| 2026-10-30-browser-storage-acceptance.md（私密/故障及STORAGE001关闭） | BAF2744599735B07898950395BFCDBCF5EDE2EBD6B62735DD3903E0BBD789AD6 |
| 2026-10-30-approval-cancel-boundary-acceptance.md（保留原R17实际FAIL历史，现已由R20复测关闭） | ED608964CF78CB61E8FC6C7C314F7FA00F9EA47943EFEEFDDEB5A44241E85358 |
| 2026-10-30-approval-cancel-r20-retest.md（原问题提出者真实复测Closed） | 477D8CD47F8FA662B950FD39EA65C82B896CF372D588E08F2E1E31A877D1A2AB |

后续新报告增加不自动套用此hash；需要按新增实际结果更新本汇总。当前自有管理节点、pipe、锁、敏感临时配对材料、ego空间13已清理，未提交、推送或归档。

## 最终跨SID证据合并

原平台tester正式补验已采纳，当前33/33 PASS。新最终报告：2026-10-30-final-independent-merge.md。tasks2.1原NA009/011/012/017均PASS；tasks4.2独立tester逐例执行/证据、清理与限制记录已齐全，用户NA033原通过保留。两任务可由主方按原判据收口；reviewer职责与tasks4.3不代签。本轮仅只读核证和更新自己的两份报告，未启动产品或重跑无关验收。
