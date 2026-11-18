2026-11-18 4.2 双层闭环验收与最终交付收口（20/20，`testing/runs/deliver-session-capacity/tester/2026-11-18-l6-control-plane-e2e-v1/ASSESSMENT.md`、`testing/runs/deliver-session-capacity/reviewer/2026-11-18-final-delivery-review.md`）：
经用户确认，4.2 按双层边界完成收口。(1) **真实 Codex 宿主与物理单调时钟层**：沿用 `0caf8143` 与 `cp026` 68 分钟物理运行（29 分钟重放刷新 `1740010ms`，纯缓存 141 读 `1830071ms` 超时释放，重放后 `1799008ms` 存活并于 `1828811ms` 释放，116 Job 槽位/12 任务槽位/端口 `51651` 完全清理）；(2) **50 会话全量闭环与控制面状态机层**：在同步 `capacity_automatic_tests.rs`（`automatic_boundary_pending_rechecks_and_first_accept_keeps_original_freeze` 与 `0caf8143` 去伪刷时钟一致）后，统一测试二进制 `acd38a1dc11b2dd2685d4776514984e2880fccab5a048f301a16a7057f7bf606` 在真实 Windows Node（TLS WSS + SQLite + Windows Jobs）及标准 ACP V2 受控对端上通过状态机/存储 **96/96**、`::rt_` **29/29** + `race_acceptance::rt01` **6/6**（1 ignored）、RB-001 节点/WSS **33/33**、非编排 `capacity_l6_` **35/35**（+ 4 真实 `#[ignore]`）与 4 项重型 L6 控制面编排 **4/4**（`310.77s` / `358.79s` / `70.53s` / `63.57s`），全部临时目录 `category=removed`。历史 `r3_wss_*`（`3204b4fe` 产物，因 `e3760615` 引入 `SessionTreeUnverified` 而预期不兼容且不在 `npm run check` 或容量套件内）维持不变；外部 OpenAI/Codex 账号配额限制（至 `2026-12-10 11:44`）及模型自发能力独立声明，整体任务推进至 **20/20**。

2026-11-18 CP-012 独立安全及交互差分检视（CP-012差分报告（`testing/runs/deliver-session-capacity/reviewer/2026-11-18-cp012-differential-review.md`））：
独立审阅确认本次 ACP V2 快照与时钟修复代码无安全绕过、无新增外部依赖、敏感信息脱敏与资源清理彻底（Job槽位完全归零，端口关闭）；单元与静态检查全绿。5.1与5.2完成签署；4.2因外部Codex配额限额保持OPEN。当前整体任务推进至19/20。

2026-11-18 L6 单调时钟与重放死锁修复评估（CP026修复评估（`testing/runs/deliver-session-capacity/tester/2026-11-18-l6-cp026-remediation-v1/ASSESSMENT.md`））：
针对 ACP V2 重放快照死锁与自动回收候选扫描空闲时钟污染两大核心问题完成生产代码精确修复（reader.rs 移除无 marker 快照当前性过度限制，capacity_automatic.rs 移除候选比对中误调 activity.touch() 的时钟污染）。在隔离数据根下执行了完整的 68 分钟真实单调时钟物理测试 capacity_l6_cp026_real_replay_clock_and_pressure，真实物理事实表明：29 分钟近超时真实重放成功刷新活动时钟（actual_replay_received_near_timeout passed），未重放缓存会话在 1830071ms 准时超时释放（cache_only_actual_1800_no_keepalive passed），重放会话在 1799 秒存活并在 1828811ms 准时超时（after_last_receive_1799_then_1800_and_scan passed），专属 Windows Job 进程树与端口 100% 干净关闭并脱敏敏感信息（complete_owned_job_cleanup passed）。任务项 4.3 达成完成判据并已勾选；4.2 因上游 Codex/OpenAI 外部账户限额（至 2026-12-10 11:44）阻断外部真实 50 模型根请求，严格保持未勾选；5.1/5.2 保持未勾选。当前整体任务推进至 17/20。

## 当前续作记录（2026-11-18）

> v9的`real_replay_accepted_last_slot_owned_by_create`已查明不是`admit_replay_capacity`的source-only或child快速路径绕过。只读身份历史显示`2026-11-18 00:02–00:06`的真实模型轮次持续返回额度上限至`2026-12-10 11:44`，另有一次`gpt-6.1-sol`不受当前ChatGPT账户支持；49个所谓保护根在最后名额阶段已不保持真实已知活动，自动回收按规则腾出名额后重放成功。v9仍为`blocked/fullCpPassed=false`，清理为`joined=true`、`metadataConfirmed=true`、`portClosed=true`、活动进程0；原始证据及关键约束见v9诊断包（`testing/runs/deliver-session-capacity/tester/2026-11-18-l6-cp011-v9-diagnostic/ASSESSMENT.md`）。

> v10最后名额编排在Create预留前和重放前重新核对49个原树均为已知活动且状态未变，并监听`AutomaticReleaseBeforeBegin`；保护根若进入自动回收候选，直接报告`real_protected_root_became_auto_release_candidate`，不再混同为容量拒绝或放行。三feature测试库实际编译通过，纯内存节流回归1/1通过。新程序`c3c82989f1dd5da8e71c30526e35f66ef540062aac23e82713f445ec3586e177`绑定458项源码清单`e05fb389009c8e1c9396939627eb86e056c74809d0807b4145349488eaed3d93`与配置`c66b1419a7e170f19c3a147e10af1c5a6b73455326047fff2e73b62d6201083e`；runner Validate及四入口List均实际exit0，源码清单458/458逐项重算一致。

> v10受控admission未取得行为结果：非沙箱自动审批审查连续因Codex后端TLS中断未执行命令，沙箱内重跑在`actual_workspace_registration_failed`停止，仅证明该环境不能提供Windows命名管道。真实CP011也未重跑，因为最近身份历史仍显示同一额度上限和不支持模型；重复执行只会再次阻塞在首个真实会话。`4.2`、`4.3`、`5.1`、`5.2`继续未勾选，不以编译、Validate/List或受控历史结果替代真实Harness、真实DOM及独立审查。

## 当前续作记录（2026-11-17）

> L6 续试 v2：v6/v7 在沙箱内分别得到 `runId=58819961cd7d4ee689e17064c4711b2c`、`865ef851dea3476b8524394a88455bea`，均因 `actual_workspace_registration_failed` 阻塞；同一固定程序的受控 workspace 用例稳定返回命名管道 `os error 5（拒绝访问）`，证明这是沙箱边界，不是产品、认证或真实外部阻塞结论，两个结果原样保留且不得计入 4.2。新 v8 配置的 Validate/List 实际 exit0，但非沙箱真实运行在审批审查阶段连续两次因 TLS 连接故障未创建进程，v8 运行根仍不存在、无结果。证据见L6真实执行续试v2（`testing/runs/deliver-session-capacity/tester/2026-11-17-l6-real-execution-v2/ASSESSMENT.md`）（SHA256 `62a45c912a20b573932610ce05975de61917b235e8527ed96ac9d4272201461e`）。整体仍为16/20，`4.2`、`4.3`、`5.1`、`5.2`不勾选。

续作工程检查已闭环：v10 检查记录（`testing/runs/deliver-session-capacity/2026-11-17-continuation/PROJECT-CHECKS-v10.md`）（SHA256 `a7a148f6b2b9f2a1b3a7d2fa2fdd43adb369d9160a19659043b2500b755348a8`）记录`npm run check`实际exit0（`CARGO_BUILD_JOBS=2`、`RUST_TEST_THREADS=4`，仅降低并行度），含规范18+2、Web 40文件/1018测试及生产构建、Rust默认all-targets严格Clippy与全量测试；主lib 1088/1088通过，`codex_acp_probe`按显式本地依赖门槛ignored 1项，其余集成目标全部通过。`npm run build:rust`实际exit0，生产Web构建及Rust dev构建通过。此前v8主lib 1087/1088失败原样保留为`shared_view_transfer001_detach_after_next_flush_cancels_unreleased_result`的夹具`Some(Io)`收尾异常；该exact测试单跑1/1、连续30次及`shared_view_transfer001_`组连续30次均通过，v10全量未复现，不声称已修复底层竞态。v9因Windows页面文件耗尽在编译阶段exit101、未进入产品测试，原始日志保留。以上只证明当前工作区工程检查，不改变真实L6、CP-012或16/20任务状态。

最新主区证据为主区集成v8（`testing/runs/deliver-session-capacity/2026-11-17-continuation/l6-main-integration-v8/results.json`）：测试程序`bf4e7e32fd414ea72c33e8b9f6d9620fdc4f46868f53aa8844db3fc3d19bc43a`，绑定458项源码；其裸数组清单`f405c9027eda4cc9e5beaafb510f23672a1ecdc615428d18f62d9e285c6db637`与真实执行使用的runner包装清单`820d4447970bc2668fb12348637076fa88b95ffd1c9d4b4bd09c8aa98855f51b`逐路径哈希比较为458/458相同、差异0，两份摘要不是不同源码快照。真实四例已在真实执行v1（`testing/runs/deliver-session-capacity/tester/2026-11-17-l6-real-execution-v1/ASSESSMENT.md`）固定：`runId=282553e3bfd9484687bb45a5e5a990d9`，均保留`blocked`/`fullCpPassed=false`；`cp011=real_replay_did_not_refuse_last_slot_owned_by_create`且清理未join，`cp023=child_actual_received_persisted_relationship_missing`，`cp026=view_snapshot_deadline_elapsed`，`cp032=required_real_capability_transition_not_observed`。四项均确认`jobActiveProcesses=0`、端口关闭；`cp011`的实际响应码尚未单列，不能据此直接判定产品放行或测试编排错误。

真实执行还暴露三项独立外部条件：宿主机上游依赖本地代理而runner白名单拒绝代理变量；专用认证账号返回`-32603`额度耗尽；已安装`codex-acp 2.5.0`不产生真实`state_update.background`。因此`cp023`/`cp026`/`cp032`的模型依赖分支在未获代理、额度或替代厂商能力授权前不重跑，失败保持具体`blocked`。`L6-CLOSE-001`停机死锁修复的Red/Green证据位于`l6-main-integration-v8/red-history/l6-close-001/`，`closed=false`仍如实保留。整体继续为16/20，`4.2`、`4.3`、`5.1`、`5.2`均不勾选。

CP011已增加只读管理错误码分类并完成固定受控回归：诊断包（`testing/runs/deliver-session-capacity/tester/2026-11-17-l6-cp011-diagnostic-v1/ASSESSMENT.md`）记录`capacity_l6_orchestration_last_slot_client_uses_three_actual_ingresses`实际1/1通过、313.47秒，清理join/metadata/端口全部确认；首次缺少固定对端的无效运行原样保留。随后以新程序`9eea9fde...`只重跑真实CP011，首个会话即返回`real_new_not_confirmed_auth_or_capability_required`，未进入49根与最后名额窗口，故先前“复用已释放根”的怀疑仍未被证实，未改写编排前提。该真实运行`runId=8453064fa81a432c9d7725898a23f0d1`，清理四项全部通过，结果保持`blocked/fullCpPassed=false`；9项体无用证据清单SHA256为`232d3bcba20b7a8ffb5dbaedf2dfed160404fac6b0f5b4201980ca74fece32ed`。

整体为16/20（2026-11-17 完成 WP4 Task 4.1 独立复核与勾选；4.2、4.3、5.1、5.2 仍未完成）。节点与Web已完成项沿用下方固定版本及限定范围；1.3 独立规划门已于2026-11-17以当前11份规划文件摘要通过，报告见WP1任务1.3独立复核（`testing/runs/deliver-session-capacity/reviewer/wp1-task-1-3-planning-review-2026-11-17.md`），SHA256 `f0d362b630f06dc4dd6afa0c4a45df3c7c3c44f8d4b4de548b1b010081d5f561`。四个真实L6入口、场景说明及受控编排动态交付已合入主区并通过 Task 4.1 入口独立复核；真实Harness长时运行（50主树/1800秒/同节点真实DOM）、独立全量验收及CP-012/最终交付仍未完成。不修改上游，不使用默认认证，不将受控对端或工具检查当真实Harness通过。

已完成结果汇合工具：新format2执行保留原退出码、运行ID、源码清单/程序/配置摘要及原节点身份；真实节点断言与清理全部完成后仍只返回待独立审查。独立页面报告核对原证据后才能新建汇总，不修改原execution。旧format1不能绕过审查直接通过；页面与清理必须绑定原Node/source/url，空DOM及自相矛盾的缺口列表被拒绝。运行说明见[验收工具](../../../../testing/tools/capacity-l6/README.md)。

开发检查实际通过：汇合器39项、runner34项；四处缺陷各保留修复前失败日志。固定源码和结果在汇合工具v1（`testing/runs/deliver-session-capacity/2026-11-17-continuation/l6-finalizer-v1/results.json`），清单SHA256 `4c7a712601b445f10a2458dda01f12d2cb3421aa7e11bdcf2c88e18171e79c9b`。这些用例只用明确标记的惰性数据和无害PowerShell进程，没有运行真实Harness、DOM或模型；中间Red的完整旧源码未另存快照，原日志保留。独立固定工具复核（`testing/runs/deliver-session-capacity/reviewer/l6-finalizer-fixed-static-review-2026-11-17.md`）关闭L6-FIN001–004，SHA256 `34ba3718356b57631d4790e499f5348e51b41a1f5ae9a67fb898d893c6fe4d95`，不签4.1或4.2。

隔离分工继续：主执行者维护runner/汇合器、CP026真实计时、view-wire观察器及原Close测试故障钩子；tester在原私有工作区维护主调度、CP011、CP032票据/必要提交分支；queue维护CP023子读取辅助源，并独占CP032能力分支及实际Current观察的只读collector；reviewer只读固定包并写独占报告。collector只在测试feature中注册，记录原处理路径已接收的来源/代际、时序与当前或历史分类，不修改活动、ACK或IO。主串行集成，角色不并发写同一文件，各自使用原私有target。CP026观察器修正为连续确认接收原流直到真实读取Complete，再切换一次缓存流，不能用SnapshotEnd或重复rebind假设原读取完成；当前修订在私有候选中已编译，修复后动态重测尚未执行。

剩余问题分开记录：CP011/032的具体分支与观察链已写入，新增发送前复核、原票据故障、必要提交及重启编排尚未动态执行；CP023和缓存观察器修复后的Windows受控回归也尚待执行。相关自动审批调用发生TLS连接故障而未执行，不能记作产品失败或认证失败，也不重复同一受阻操作。真实L6另缺专属已授权认证及对应真实场景；既有空认证探针只获得AuthRequired，不再重跑或借默认凭据绕过。上述限制均不能由39/34工具检查或入口编译代替。

原Close故障设施已完成本地开发验证，固定包为关闭写入故障v1（`testing/runs/deliver-session-capacity/2026-11-17-continuation/l6-close-fault-v1/results.json`），结果SHA256 `dab25e865bb15ffe8f09a4143d280fef872798745568fda53055723ca781d2ab`。9项新增测试与11项已有observed回归实际20/20通过；受控退化把一次实际写入上限从1改为全帧，原测试实际以Written不等于Unknown(accepted=1)失败，随后精确恢复并重验。专项Clippy首轮10项诊断与旧源保留，最终修正只涉及嵌套可见性、等价布尔表达式、参数及原来源Arc克隆后提前释放列表锁；最终20/20、格式、默认all-targets严格Clippy和native feature的lib/tests严格Clippy均exit0。证据范围仅内存duplex及编译/静态检查，不含Windows真实pipe、真实Node故障名额或Harness；三feature的历史approval诊断也不由native-only通过覆盖。

独立hook v3固定复核（`testing/runs/deliver-session-capacity/reviewer/l6-close-ticket-fault-hook-v3-review-2026-11-17.md`），SHA256 `e454093b2af806d0844525c86ff1515ec487fc683be0c1a23ec8596a2c6fb0b9`，确认上述限定范围通过，并核对38项归档摘要一致；没有签署真实L6或整体任务完成。

CP026客户端的L6-WIRE002已修复：服务端原读取完成可以在大记录分片之间发送合法换代状态；客户端现在仅允许同stream、连续packet、完整单帧且原stream身份匹配的换代通知丢弃未完成旧片，普通交错和错流仍拒绝。内存验证固定包（`testing/runs/deliver-session-capacity/2026-11-17-continuation/l6-wire-rebound-v1/results.json`），SHA256 `7ae910b56393120522a2531ea2724dcad7878845330c2b61140bff026627da44`，保存原实现3通过/1失败和修后4通过。driver使用候选原重组器/测试、当前wire源码及摘要绑定的既有依赖库，没有执行异步socket或Node；独立修复检视（`testing/runs/deliver-session-capacity/reviewer/l6-wire002-fix-review-2026-11-17.md`）仅关闭这一客户端问题，完整crate及真实长时验收仍待集成执行。

CP032能力候选v1及Current只读collector已固定在候选证据（`testing/runs/deliver-session-capacity/2026-11-17-continuation/l6-capabilities-candidate-v1/results.json`），SHA256 `e197c015de77ca3896de42822a4224defb41b0bf1bc4f246985fa24f524c3f11`。2项解析与1项collector纯内存测试通过，History误记Current的受控退化实际失败后精确恢复；未运行真实Node挂点、Harness或重启。该候选Clippy实际命令含`-D warnings -A dead_code`且exit101，不是严格全局通过。独立检视（`testing/runs/deliver-session-capacity/reviewer/l6-cap032-current-and-restart-static-review-2026-11-17.md`）发现L6-CAP001：事件与较晚状态可能被拼合为“缺字段仍保护”的假阳性，v1保留为修复前证据。

L6-CAP001修复v2在原活动锁内保存同receipt处理后的前台、后台及active工具身份；缺字段和工具结束均绑定该事件自身状态，不能借后续start补证。固定包（`testing/runs/deliver-session-capacity/2026-11-17-continuation/l6-capabilities-candidate-v2/results.json`） SHA256 `4e22eb4641f9dfd5f9e3c515bba697ebbeb69a8a0e52c501aae204df14beec29`，保留受控恢复v1匹配谓词的实际失败，以及最终同程序6项入口和2项collector纯内存测试通过。19项原源/补丁/日志归档摘要均匹配；该包Clippy仍带`-A dead_code`且exit101/73，不能称未放宽的严格检查通过。独立修复复核（`testing/runs/deliver-session-capacity/reviewer/l6-cap001-v2-fix-review-2026-11-17.md`） SHA256 `8d4cc5ac519fe69a90a956dd531646e8d49707caa68a4863e959121bf0437021`，仅关闭此验收假阳性问题。整入口集成、真实Node观察泵、Harness、50树、重启及DOM仍须各自验证。

L6-IN001输入合同修复支持项目外的非默认专属认证目录，仅runRoot必须归专属测试区；身份目录仍禁止默认位置、reparse和运行目录重叠。runner保存替换子环境前的原默认目录字符串并传给Rust，未读取认证文件；固定工具证据（`testing/runs/deliver-session-capacity/2026-11-17-continuation/l6-input-identity-v1/results.json`），SHA256 `8e08af425d54701f4674840f141889ffbb116a31b34ad3b0ddc85f324b4b5e12`，包含原真实无害子进程Red及修后4项、runner34项、finalizer39项通过。独立runner半边检视（`testing/runs/deliver-session-capacity/reviewer/l6-input-identity-runner-half-review-2026-11-17.md`）与后续Rust输入及入口复核（`testing/runs/deliver-session-capacity/reviewer/l6-input-rust-and-entry-scope-review-2026-11-17.md`）（SHA256 `487fae6a3d7ed06e96e9be2bddce0e386237cef02e5857ffd049a519bf3740e2`）已关闭该输入合同问题；后者核对提取driver受控Red和修后7项，以及私有v12同版整crate7项通过，不代表真实认证成功。

同一入口复核确认四CP共40项requiredBranches与固定矩阵逐项一致，并发现L6-ENTRY001：发送前活动令原关闭请求撤回后，入口误用要求“历史关闭操作为零”的初始资格检查，必然被刚保留的Withdrawn记录阻断。现已采用专用重新候选检查，保留原operation、reservation、成员、来源/代际和新活动结束后的计时；通用首次资格检查不放宽。受控旧限制实际Red、恢复后两项谓词及原SQL内存数据库回归通过，独立修复复核（`testing/runs/deliver-session-capacity/reviewer/l6-entry001-fix-review-2026-11-17.md`） SHA256 `bdcfafa5b787bf1a8e8f8ad84621302b1348a69870894cb9576f7597754e2bb0` 已关闭该入口逻辑问题，不签未执行的完整编排。

最终18项入口候选已按patch `490fe89f811c993af7d2130efc87c958b6d6649751f6a60091c0284a558fd3ee` 串行合入，合入前检查及全部文件归一化摘要匹配；主仅将node.rs新增14个feature属性按rustfmt折行，原native-only cut与request-runtime路径保留。独立交付说明（`testing/runs/deliver-session-capacity/tester/2026-11-17-l6-entry-v1/ASSESSMENT.md`）及164项证据清单（`testing/runs/deliver-session-capacity/tester/2026-11-17-l6-entry-v1/index.json`）已逐项核验后永久保存，index SHA256 `cbfaf346b4b72a6d0ef14224b5cfec2644407a9bebf7cdb53d49cf02b440b8cb`。私有最终同版42项通过和四个exact List属于纯内存、自有空目录与入口注册验证；中间误选旧程序导致0项的失败保留且明确无效。场景字段、实际管理命令、身份输入及各phase见[场景运行合同](../../../../testing/tools/capacity-l6/SCENARIOS.md)。

主区集成检查固定包（`testing/runs/deliver-session-capacity/2026-11-17-continuation/l6-main-integration-v1/results.json`），SHA256 `34131618727303b226a89f13a42a7759107859b12df84185389ea6ac5e5346b5`，记录格式、默认all-targets严格Clippy、三feature编译、42项实际本地回归及四入口exact List全部exit0。额外三feature严格Clippy不放宽规则，实际exit101/66；逐路径、类别及文本与独立最终候选对比新增0、删除0，不能记整体严格检查通过。相对旧approval基线的65项完全相同，另一项只是原未用字段列表缩窄，详见独立说明和原分类。当前Web工程实际类型检查、40文件1018项测试及生产构建通过，包中明确这是实际终端结果记录。以上均未执行新版完整Node/TLS编排、真实Harness或浏览器，4.1、4.2、4.3、5.1、5.2及整体任务勾选保持不变。

独立主集成及归档复核（`testing/runs/deliver-session-capacity/reviewer/l6-main-integration-and-archive-review-2026-11-17.md`），SHA256 `1730eb06788cb844f158867192d165d31fac91cc6e3061d2428badabbd1f61fb`，核对独立包164项、主包42项及当前源码摘要全部一致，接受三个问题修复在主集成中的限定有效性。Clippy对比统一了路径分隔符，原日志并非逐字节相同；66项涉及既有审批及其他测试设施，不将失败改为通过。增量扫描v7（`testing/runs/deliver-session-capacity/2026-11-16-request-boundary/security-tail-v7/result.json`）实际317项、exit1/6条；同报告逐项确认均为源码副本里的公开RFC6455握手向量，原配置和退出码保留。新增报告及最终文档的尾扫另见v8结果（`testing/runs/deliver-session-capacity/2026-11-16-request-boundary/security-tail-v8/result.json`）；差异与规格结构检查（`testing/runs/deliver-session-capacity/2026-11-17-continuation/l6-delivery-checks-v2/results.json`）通过，只证明相应范围，不代签完整交付。

独立 tester 受控编排归档固定在编排清单（`testing/runs/deliver-session-capacity/2026-11-17-continuation/tester-orchestration-v1/manifest.json`），SHA256 `f76b1729922b6583625eb7668930b638d02593ccf248f1adfbbc3353058a0a26`（235项副本、3,654,090字节、0禁止文件）；入口准入评估v4（`testing/runs/deliver-session-capacity/2026-11-17-continuation/tester-orchestration-v1/evidence/l6-entry-readiness-v4.json`） SHA256 `7fb8392e60b81b176a12b8dd4bb03723897e5599c54ea583680557c452a22e69` 固定32个CP（均 `fullCpPass: false`）与40个 `requiredBranches`（`fullRealExecution` 均为 `"not_run"`，`testsSha256` 匹配 `tests.md`，LF归一化 `branchMatrixSha256` 匹配 `branch-matrix.v1.json`）。私有候选 `v32`（`3507073e3c0e822805b30c85a9b32a0d3ba5901f594f86fa31e6e9c0801c7193`）记录9组17/17受控编排通过，完整保留历史失败 ENTRY002–006；其评估文本第35行对66条既有诊断的简称仍以分类JSON为准（63条approval + 3条 `tasks.rs` / `registry_connection_owner.rs` / `schema.rs`，0新增）。主区串行合入12文件编排补丁，并修复 [runtime_child_activity_clock_probe.rs](../../../../app/src/application/native_dispatch/runtime_child_activity_clock_probe.rs) 中 `l6_known_work_for_test` 缺失的 `#[cfg(feature = "approval-owner-acceptance-fixtures")]` 门禁；`v3` `native-clippy` `E0425` 红日志与 `v5` `orch-children`（误注入 `l6-settled-normal-peer-v2` 拒绝 `"source-create-replay-state-verified"` 返回 `invalid_configuration`）红日志均保留于 `l6-main-integration-v6/red-history/`。

主区最终入口集成固定在主区集成v6结果（`testing/runs/deliver-session-capacity/2026-11-17-continuation/l6-main-integration-v6/results.json`）（SHA256 `5027fc042c55c6d6f7d36a8ac67a987b23bafd9e716051d067f8ff0558d17a`）与归档清单（`testing/runs/deliver-session-capacity/2026-11-17-continuation/l6-main-integration-v6/manifest.json`）（SHA256 `82a3ac82b323d72be639fae69aadcefcc9045cb711374367c593cf8f1a8a7b77`，58项副本、0禁止文件）；`source-manifest.json`（458项，SHA256 `2acd6d42c09142ad7336f8ad7870648cdebeebfaf666e2abe0a5d51718615916`）与 `binding.json`（SHA256 `1287d24e8f886c6ccc0a9594a22994ffa5e07279fe287e5b5254c30d6f0be0e1`，主区测试程序 `1cc6e55f4850cfe666f33c24c506a55e7ef3775b6497d7b2a4f59a60069751e3`、`native_dispatch_dev_child.exe` `464c776341ad3be3f96a90025e8518c3f6dcd0d58e1045cf75e735302d1614d8`、`realHarnessExecuted: false`）全部绑定一致。实测 `--no-default-features` 检查、`fmt`、`default-clippy`、`native-clippy`、`build-dev-child`、`expanded-no-run` 均 exit 0；6组42项本地回归（42/42）、4项 exact ignored `--list`（各1项）、9组17项受控 Node/TLS 编排（17/17 passed、`children`/`cache`/`cut` 沿用主区 `native_dispatch_dev_child.exe`、`admission`/`restart`/`ticket`/`pacing`/`web`/`boundary` 使用受控对端，0 `R3_WSS_FIXTURE_PRESERVED` / `L6_CONTROLLED_FIXTURE_PRESERVED`，`WindowsJob { active_processes: 0 }`）全部通过；3-feature `expanded-clippy`（SHA256 `2bf4ee77f8139ab370a2a015c7b6dd7df9d043ec05ff2d61632646d7a591df00`）实际 exit 101、66条既有诊断，相对 `v2` 分类基线新增0/删除0，不计作通过；372项 Web 资产相对 `l6-main-integration-v1/l6-web-recheck-v1/result.json`（SHA256 `6097258fc0d97a83c9a6752d35e85c6563c2c7cd96e2c8cc4677dbf878ce00fb`）保持 `mismatches: []`。增量扫描v9（`testing/runs/deliver-session-capacity/2026-11-16-request-boundary/security-tail-v9/result.json`）（SHA256 `be971723ab68da440189ab2c35ce5b714238671a5ad04f7a85ee262192c0a957`，329项、exit 1）命中3条均为归档 `node.rs` 副本中公开 RFC6455 `Sec-WebSocket-Key` 测试向量。独立Task 4.1 入口交付复核（`testing/runs/deliver-session-capacity/reviewer/l6-task-4-1-entry-delivery-review-2026-11-17.md`）（SHA256 `ce2e5b25ab675a2de003a498c2698efe491525ad6bbc93db49423da4a8ae11ca`）仅签通过 **Task 4.1**，并明确 **4.2、4.3、5.1（CP-012）、5.2、CAP-R020 与真实 Harness** 仍未完成/OPEN；其中 4.2/4.3 受外部前置条件阻塞，需用户显式提供非默认已认证 `dedicatedHome` / `dedicatedCodexHome` 路径及真实模型/Harness 授权。最终文档及报告的增量尾扫见v10结果（`testing/runs/deliver-session-capacity/2026-11-16-request-boundary/security-tail-v10/result.json`），差异与结构检查见交付结构检查v3（`testing/runs/deliver-session-capacity/2026-11-17-continuation/l6-delivery-checks-v3/results.json`）。

## RB-001 当前执行计划（2026-11-16）

用户已批准按请求/执行职责边界更新规格和验收，继续完整实现且禁止修改上游。当前行为以新版spec/design/implementation-contract为准；下方第一阶段及旧安全门报告保留历史范围，不再把完整隐藏活动/原子窗口证明作为新实现门。

最新集成进度：接线前实际源码与证据固定在 internal-integration-v2（`testing/runs/deliver-session-capacity/2026-11-16-request-boundary/internal-integration-v2/manifest.json`），清单 SHA-256 `08019068a5c44fc1d6cd05addfe9c7bd10aaa37ae920becc5c2d5149143a490d`，覆盖550份源码/配置。主执行者实际运行自动回收46/46、child读取3/3、共享读取8/8、ACP重放30/30、OT001–010共10/10、排序单测1/1；origin单独6项包含在OT10内，错误模块过滤的0项日志不计通过。旧排序实现和显式关闭漏封另一child分别有行为断言Red，新实现均Green。独立 接线前集成检视（`testing/runs/deliver-session-capacity/reviewer/2026-11-16-prelaunch-integration-v2-review.md`） SHA-256 `097864da6bb6462cea10dc7c3e9cdd0a1300407e5691cdccf1518bd09c243551` 确认有限PASS并关闭CAP-R022原排序缺陷；CAP-R020整体仍OPEN。

后续接线：reviewer已批准在上述内部回归后进入正常launch接线包，不要求尚未启用功能先通过最终验收。主执行者已将自有扫描接到旧运行协调和Running状态转换成功之后；启动失败使用fail_all收敛。新启动断言在旧launch实际失败（0通过/1失败），接线后46/46通过。独立复查发现CAP-R023：启动占位标记早于实际任务登记，可能短暂发布不准确的启用状态。该问题已通过真实TaskRegistry拒绝登记获得Red0/1，并改为成功取得原TaskSlot后发布凭据；最新自动47/47、child3/3、OT10/10通过。最新固定输入为 launch-integration-v2（`testing/runs/deliver-session-capacity/2026-11-16-request-boundary/launch-integration-v2/manifest.json`），SHA-256 `c6be0d42f89606e7ae8577f2553905241b989a0eaa371657829d3a8f2dd3384a`，580份输入含tester补充的21份非Rust编译fixture。CAP-R023已由独立 固定版本复核（`testing/runs/deliver-session-capacity/reviewer/2026-11-16-scan-registration-final-review.md`） 关闭，报告SHA-256 `38d68413752b99fdeacc6c436cd8deefdeddf2ca75bfd79581e2fd9ed5d66fa4`；当前文档启用范围冲突DOC-RB-LAUNCH-01也已独立关闭。

当前验证：完整 `npm run check` v4 已实际 exit0，Web1018/类型/构建、默认Rust格式及严格Clippy全部通过；库1085/1085（458.73秒）、main2/2及所有实际运行的默认集成目标通过。27个Rust summary包含子进程重叠结果，不按简单加和宣称独立总数；真实Codex probe仍ignored1，feature未启用的0项不计验收。输入相对launch-v2仅两份旧V2夹具不同，578项其余输入逐项相同，证据与差异副本固定在工程检查v4（`testing/runs/deliver-session-capacity/2026-11-16-request-boundary/project-checks-v4/results.json`）。原六个MissingSnapshot由缺少replay_complete后当前状态的对端序列引起；补齐序列并提取重复marker helper，未放宽生产reader、负向断言或lint规则，当前两个目标分别11/11和57/57通过。此前默认库1085/1085（553.45秒）的缓慢队列用例自行完成，未终止进程；独立诊断另有单项11次和队列44项并发组通过，不据此推断延迟根因。此前失败和相同生产runtime的正式 `npm run build:rust` 通过原日志保留在工程检查v2（`testing/runs/deliver-session-capacity/2026-11-16-request-boundary/project-checks-v2/results.json`）。

独立CP012定向检视（`testing/runs/deliver-session-capacity/reviewer/2026-11-16-rb-cp012-differential-review.md`）未发现系统关闭权限、原集合/票据、child来源归属及任务所有权的新静态阻断，不代签完整CP012或5.1。当前源码与永久证据2900文件的密钥扫描保留实际exit1/两项发现；reviewer逐项确认均为源码归档副本路径下的公开RFC6455测试向量误报，原配置不变，精确范围见扫描结果（`testing/runs/deliver-session-capacity/2026-11-16-request-boundary/security-v1/result.json`）。扫描后新增记录另需尾扫。

本轮新增/变化的138项已执行增量尾扫v1（`testing/runs/deliver-session-capacity/2026-11-16-request-boundary/security-tail-v1/result.json`），实际exit1/6项；独立逐项分类（`testing/runs/deliver-session-capacity/reviewer/2026-11-16-rb-security-tail-v1-classification.md`）（SHA-256 `fc511a11d060e0fc518341ced961c5383b52738a67ea7e8c14cdc95997e4ed19`）确认其中4项是manifest内可重算且相符的源码SHA-256，2项是归档副本内相同RFC6455公开向量。未改扫描配置或将exit1改写为0；该报告及后续增量仍须后续尾扫。

新增v3/race源码及v7证据另执行增量尾扫v2（`testing/runs/deliver-session-capacity/2026-11-16-request-boundary/security-tail-v2/result.json`），96项输入、实际exit1/4项；独立v7与扫描检视（`testing/runs/deliver-session-capacity/reviewer/2026-11-16-rb-v7-race-and-security-tail-v2-review.md`）逐项重算确认两条为源码摘要、另两条为相同公开握手向量。该检视同时核对v7的47个归档文件摘要、竞态退化失败/恢复及测试钩子范围，未发现内部交付新阻断；不代签tester、完整CP012或真实Harness。

当前任务为14/20：独立任务证据归属报告（`testing/runs/deliver-session-capacity/reviewer/2026-11-16-task-evidence-attribution.md`）（SHA-256 `de9bd34366839882440537116d222233949dd0211524cc8f73e64c0c47a0060d`）确认1.1合同冻结与1.2入口准备；本轮 Web交付独立检视（`testing/runs/deliver-session-capacity/reviewer/2026-11-16-rb-web-delivery-and-test-overlay-review.md`）（SHA-256 `724f08b80f60deedfea79ccb146ffc9ee9b3bf3d076f68b9bfbb5218cbc22b51`）确认3.1/3.2达到各自Web判据；节点WP2完成检视（`testing/runs/deliver-session-capacity/reviewer/2026-11-16-rb-node-wp2-completion-review.md`）（SHA-256 `64fd4f75a844ab0cebfb49902161c9b91fd8811195d48caba536869c8d2ff6f3`）按逐CP节点断言确认2.1/2.2完成，不将真实L6义务混入本地实现判据。完整CP012及真实Harness仍未签。共享写入由主串行执行，tester/reviewer/fixture协作者使用隔离目录。

实际浏览器及后端固定在 browser-live-v3（`testing/runs/deliver-session-capacity/2026-11-16-request-boundary/browser-live-v3/delivery.md`），58项文件清单SHA-256为 `5090c4b176a7e22a71876e4c2f6ef9d665386225b74cf4602620c0df168beaff`。专属真实Node/TLS/WSS/SQLite与受控ACP对端验证：原根Closed且原Cleanup未join时页面50/50，同原操作/票据经正常扫描完成后49/50，重开同根的新stream并收到真实replay正文后50/50；child-A/B正文互斥、CurrentOnly和历史不完整提示、撤销后“原配对已失效”及管理表单消失均有实际DOM。fixture实际1/1、511.97秒；最终joined/metadata_confirmed为true、Windows Job活动进程0、端口关闭、专属临时目录移除。压力原调用是read_failed，不能冒称本轮浏览器Full错误通过；Full组件断言依据相同Web字节及工程v4复用。截图超时无图片，未签视觉检查。v1/v2观察超时与原清理结果保留；测试等待时长调整没有改变默认30分钟策略或30秒扫描。

独立tester的稳定测试包已合入：新增本轮request-runtime路径而保留第一阶段原档案，Cargo受控peer路径同步；主合入仅另有两处rustfmt折行。默认格式和严格Clippy实际通过，三测试feature联合编译通过；feature编译保留既有测试hook告警，不冒签expanded严格Clippy。主同版重验current33实际33/33、20项RT子集19/20，RT003执行加载在ensure_target处返回SessionCapacityFull，保留专属失败现场并交独立tester诊断，不记整组通过；原始结果固定在工程检查v5（`testing/runs/deliver-session-capacity/2026-11-16-request-boundary/project-checks-v5/results.json`）。独立tester继续审批Zero/必要终态故障、归属失败、迟到child与shutdown交错分支。真实Harness认证、50树、实际30分钟和真实child仍未验收。

额外三feature严格Clippy实际exit101，66项均与原有approval测试设施的诊断按路径、类别、文本逐条对应；本轮新增测试原6项诊断已消失。默认项目严格Clippy通过不覆盖这个额外组合，原始失败及只读分类保留在feature检查记录（`testing/runs/deliver-session-capacity/2026-11-16-request-boundary/feature-lint-review-v1/expanded-feature-clippy-current-addendum.md`），未为本change改写无关历史hook或放宽全局规则。

后续测试增量v2的7文件已串行合入，新增审批原回复Zero/必要终态SQL故障、直接子目标正文、无可信关系拒绝及持有child读取期间shutdown。主格式、三feature编译与peer构建实际通过，五例重验5/5，固定在工程检查v6（`testing/runs/deliver-session-capacity/2026-11-16-request-boundary/project-checks-v6/results.json`）；独立测试增量检视（`testing/runs/deliver-session-capacity/reviewer/2026-11-16-rb-test-increment-v2-review.md`）明确真实注入和组合证据范围。RT017实际持有Reader任务，新登记的NativeRead被拒；两者不混写。该增量没有修复或覆盖v5的RT003失败；RT012本包只验缺关系，未知根及迟到活动/登记交错仍单独补验。

测试增量v3及RT015/016竞态包已串行合入，主区本版整组实际66/66（27项RT、6项竞态、33项既有节点回归），默认严格Clippy、格式及三测试feature编译均通过；固定在工程检查v7（`testing/runs/deliver-session-capacity/2026-11-16-request-boundary/project-checks-v7/results.json`），源码清单SHA-256 `9c0e058ee7bc6c413057b40f759b823af1c2875a911b8e0fd756bed7113689f5`。RT003独立诊断确认：原测试在已受理队列启动加载后，又额外调用ensure_target；额外请求得到Full时，原目标尚无预留或必要binding。保持节点存活后，原输入实际resume、建立新预留并完成原关闭操作。失败后的Withdrawn来自测试退出清理，不能反推原Full之前撤回。修订保留原accepted输入的实际resume、durable loaded/Written、prompt、Close一次、Finished和50占额断言，再验证已有资源的ensure_target复用且无第二次resume；独立v3检视（`testing/runs/deliver-session-capacity/reviewer/2026-11-16-rb-rt003-v3-and-node-mapping-review.md`）已接受该修订，不将旧失败记作产品Red。原失败和诊断分别保留。

v3还补齐child reader关闭后跨配置重开时原owner/instance/capacity精确复用，以及根协议拒绝、缺当前快照两条真实失败。竞态包覆盖晚到child在Close首字节前后的保护，以及真实任务adopt前后与根关闭/source shutdown的四种交错；开发六项通过并有受控退化失败证据，独立复验仍进行中。测试钩子只在专属测试feature下编译，未修改上游。真实L6 Rust入口与PowerShell runner正在隔离分工补齐；专属认证缺失仍是实际真实验收的阻断，入口编译/参数检查不代签真实通过。

v4新增EOF/请求者取消两项已独立通过，并经独立复核（`testing/runs/deliver-session-capacity/reviewer/2026-11-16-rb-v4-eof-cancel-and-independent-race-review.md`）接受；同一报告核对竞态独立6/6和清理。主合后格式、三feature编译及新增2/2通过，记录在工程检查v8（`testing/runs/deliver-session-capacity/2026-11-16-request-boundary/project-checks-v8/results.json`），源码清单SHA-256 `fbd51d05484ffedc045337e98c34cec04a24592d61ad759c491f27cb8df38c0c`。与v7逐项比较只有新测试文件及node include变化，既有66项按同范围源码复用，不称两个程序为同一二进制。

独立内部验收的正式报告（`testing/runs/deliver-session-capacity/tester/2026-11-16-request-runtime/internal-final-v1/runtime-child-internal-assessment.md`）、逐CP节点矩阵（`testing/runs/deliver-session-capacity/tester/2026-11-16-request-runtime/internal-final-v1/cp-node-evidence-map.json`）及232项原始证据已逐项校验后固定；清单（`testing/runs/deliver-session-capacity/tester/2026-11-16-request-runtime/internal-final-v1/manifest.json`） SHA-256 `518251a8062da3c5953c5796854e3e29b21e2432b47d74b41fdd9dd8327fd764` 保留原路径到永久路径映射，不改写旧日志/失败。20项RT为分层内部结论，32项CP逐例保留真实Harness和独立安全义务；主v8之后native-only严格Clippy也实际通过，额外审批feature组合仍维持原独立失败记录。

WP4当前精确分工：tester在私有快照写真实L6 Rust验收入口；runner协作者只写私有 `workspace/l6-runner`，以patch交付 `testing/tools/capacity-l6/`，主执行者检查后串行合入，独立tester/reviewer验收。runner七文件已合入，首版主配置/结果映射检查28/28，开发惰性进程的30秒超时及父子退出检查通过；固定在runner首版记录（`testing/runs/deliver-session-capacity/2026-11-16-request-boundary/l6-runner-v1/results.json`）。独立发现的 L6-RUN-001（空 Cases 集合误报通过）已修复并复核关闭，主新增检查后34/34；真实CLI的Red/Green、修复patch和最终工具源码另存runner第二版记录（`testing/runs/deliver-session-capacity/2026-11-16-request-boundary/l6-runner-v2/results.json`），不覆盖首版。默认Validate/List不签验收，Run核固定必需分支、专属环境、程序摘要及清理结果；不下载、登录或读取默认认证。实际L6未执行，Rust仍有具体必需分支及同Node真实Web交接的入口实现缺口，与认证受阻分别记录，4.1/4.2不因此勾选。

后续L6实现继续按互不重叠的新helper隔离：tester负责主调度、CP011/CP032及只读probe；主执行者负责CP026重放/时钟helper；runner协作者负责Web交接helper及其开发测试，交付后负责CP023真实子路径helper。各helper在私有目录完成后由tester汇成固定候选包，主检查patch后串行合入；共享调度/probe仅由tester修改，主集成后重跑适用编译与新增回归。独立reviewer审查具体路径及分支证据，不将入口编译或受控peer检查签为真实L6。

以下分包进展保留各自记录时的状态；“生产未启用”指接线前快照，以以上最新进度为准。

WP1：主执行者串行更新规格、总体设计及实施合同；独立tester在专属目录生成验收候选，主执行者集成后由原独立reviewer检视。现有阶段8项、本轮准备2项、节点2项和Web2项已完成，其余6项须按修订验收实际闭环，未完成不勾。
WP2：runtime内统一三入口的容量压力触发、单调时钟、已知活动保护、请求前复核和关闭所有权；存储增加系统自动回收必要操作合同，精确绑定原reservation及代际，禁止借用户actor或伪造target。复用原关闭确认和失败保额。父子名额依标准已知关系实现，未知拒绝。
WP3：既有容量DTO/Web文案与结果展示按新边界同步，无新会话状态。
WP4/WP5：独立验收和检视同一固定源码摘要；默认50及实际30分钟、真实Harness和Windows进程树清理仍须实际执行。

并行评估：tester/reviewer只读分析可与主执行者规划并行；所有共享规划与runtime写入由主执行者串行完成。需要独立实现的存储包在.tmp专属副本生成patch，先检查后串行合入，重跑SQLite/迁移/释放合同。独立tests候选同样隔离生成；不允许并发写共享工作区。tester/reviewer角色依据openspec/role，不继承coder对话。Pi规划改动不属本任务。

本轮新检查已开始，不能复用旧“全部未知必须禁止请求”的预期。领域策略19项、标准活动投影7项开发测试已通过；新增未报告范围、异常child state及未知工具状态均保留实际Red/Green日志。它们还未接生产扫描，不等于自动关闭产品验收。产品检查入口为npm run check、npm run build:rust；OpenSpec strict与diff仅文档验证。证据写testing/runs/deliver-session-capacity/2026-11-16-request-boundary及独立角色目录。

独立分包门见 RB-001检视报告（`testing/runs/deliver-session-capacity/reviewer/2026-11-16-request-boundary-planning.md`）：领域/投影/系统存储三内部包已PASS，后续 [运行时R1合同](automatic-release-runtime.md) 的队列及协议首字节保护、单主目标内部运行时亦获PASS。父子完整资源集合、生产启用与完整验收仍待完成，CAP-R020未关闭。存储coder只写 `.tmp/capacity-request-storage/workspace`，私有target及jobs2；主执行者写共享领域/协议，待patch检查后串行集成存储。

独立真实小样本已运行：隔离身份、已安装codex-acp2.5.0，initialize成功而list/new均AuthRequired(-32000)，无会话ID，close未执行。探针及adapter退出、Job活动进程归零已验证，不代表关闭正向通过。真实50/30及child仍缺专属认证和实际运行证据；不修改上游、不读取或复制用户默认会话来绕过此限制。

### RB-001 内部分包集成进展

后续进展：全树自动关闭包及最终修正已串行合入，补丁 SHA-256 分别为 `42ebf35f2307b971b5d837b6d258a4a02b66e0ce62d057157f17ae2e65f139a9`、`36dbb74a46d7bf25f002ae9a429d1dc3e5e2441db8ff945f5a2e5124cdf1bc06`，隔离开发测试44/44通过。独立来源/主树消费者验收53/53通过，报告摘要 `9a767d42f263b7f9af8485d5cb7532a385426903ad9b191318e90b9cf30fb280`；仅签固定内部快照。独立审查新发现 CAP-R022：压力候选在 SQL 等待后收到重放可能不再最旧，发送前须复核排序且不能仅因重放反复撤回唯一候选。修复在隔离 runtime 包执行，主暂不写其文件。child 实际任务所有权包仍独立完成；生产扫描仍未启用，最终工程/真实 Harness 验收不由上述数字代签。

以下记录对应当前未提交实现，不能替代完整 change 验收；任务仍为8/20，生产扫描启用点尚未接入。

- schema13 系统关闭存储已串行合入，固定原 reservation 集合、Requested/Failed/Unknown/Withdrawn 与原票据协调；内部独立检视范围见 reviewer 的 `2026-11-16-rb-internal-v1-review.md`。
- CAP-R021 历史/当前状态来源修复已合入，补丁 SHA-256 `a76864fd6cec3130bb313d2a1f56c1311a97a9bb6ce31e5b7f22c19fb6ba85bf`。开发实际旧 reader Red、155项定点 ACP Green；此前30项身份认证前提失败仍未通过，该定点运行明确排除此组，不能写成全 ACP 通过。
- schema14 主树持久关系已合入，补丁 SHA-256 `4452899c99d0682e5a1fdc80f937e0a15581b73ef04006c6bb0de683673428d8`。开发实际存储回归291/291，包含18项新增树测试；旧 schema13 行为 Red 和 resolver 受控退化 Red 分开记录。子读取与活动完整集成仍待验证。
- 队列首字节冻结和单主目标自动关闭中间包已合入，后一补丁 SHA-256 `5902aec42e3294a726c74f2ee5dbcb81542b1e23c739bb7a8e9574b8ca687c71`。主执行者接入三入口统一准入、原关闭票据周期协调、当前活动及关系保存；审批正常回复的保护解除同步刷新树级计时。当前主库 `cargo check --lib --locked --offline -j 2` 实际通过，尚有未启用扫描和未接树候选的 dead_code 告警，不等于严格 Clippy 或产品通过。
- 当前隔离包：`capacity_queue` 在 `.tmp/capacity-request-runtime/workspace` 完成全树候选/冻结和运行时开发验证；`capacity_child_read` 在 `.tmp/capacity-child-read/workspace` 完成 `RootResource`/`ChildAutoDelivered` 读取。后者写读取模块、source 的读取分支、view attachment 窄绑定端口及 ACP native views，主执行者不并行改这些文件。所有补丁先检查再串行合入，各用私有 Rust target。
- 独立 reviewer 正在复核固定 origin 和树存储内部包；CAP-R020 完整集成仍 OPEN。真实 Harness、默认50树/实际30分钟、child正文正向及最终工程检查尚未完成。

## 第一阶段及接口调研状态（RB-001之前的历史快照）（2026-11-16）

用户随后同意先核实现有Harness接口与运行方式。该核验已完成一轮：固定codex-acp 2.5.0实际关闭入口会中断任务；本机Codex 0.169.0正式及实验schema已隔离导出并分别核对109/184个请求方法，未找到公开主树条件安全卸载接口。退订与卸载分开，现有活动保护不能直接当完整原子窗口。详细事实、原始接口证据、独立源码复核及本机下载审批服务故障见 现有Harness核验（`testing/runs/deliver-session-capacity/2026-11-16-existing-harness-verification.md`）。这不是上游新功能已实现，也不预设所有可行方案都必须修改核心；生产自动回收继续受CAP-R001约束。

最新用户要求：“继续吧，完成这个change”。现已恢复完整范围的前置核对；第一阶段完成事实保留，整体仍为8/20。当前ACP v2公开文档具备后台及子会话观测信息，但普通关闭会取消工作，尚未取得检查至实际关闭期间不误取消新工作的可靠路径。CAP-R001继续阻断完整实施门；本轮核对和待决定的上游能力工作见 完整范围恢复核对（`testing/runs/deliver-session-capacity/2026-11-16-full-resume-assessment.md`）。继续实施授权不等于允许放宽安全语义或新增私有桥接；不以历史第一阶段范围阻止前置研究，也不把缺口记为已完成。

> 最新用户确认已批准第一阶段。当前范围、写集合、串行原因及阶段验证由 [phase1.md](phase1.md) 定义，阶段任务使用 tasks.md 的6.x；原完整12项仍保持待办。下文“阶段待批准”是上一轮门禁快照，不覆盖本段当前状态。

第一阶段方案检视已通过，报告见 阶段方案门禁（`testing/runs/deliver-session-capacity/reviewer/2026-11-16-phase1-planning-gate.md`）。WP2/WP3 已实现必要容量管理、全加载入口准入、原所有者路由、显式释放协调、认证查询及 Web 展示。独立tester与reviewer已对同版阶段产品签署通过，最终完整工程检查及构建实际exit0；见 最终独立验收（`testing/runs/deliver-session-capacity/tester/2026-11-16-phase1/final-v14-assessment.md`）和 最终独立检视（`testing/runs/deliver-session-capacity/reviewer/2026-11-16-phase1-final-review.md`）。第一阶段交付完成，6.x为8/8，整体为8/20；原完整12项继续待办。自动回收、完整活动及安全关闭窗口证明和完整真实 L6 仍未完成，整个 change 不归档。

实现说明见 [节点](../../../../docs/implementation/session-capacity.md)与 [Web](../../../../docs/implementation/session-capacity-web.md)。受验收 v13 固定973文件，manifest SHA-256 为 `75152ba10534e54a7f3455fddf4c23aab1c19eb0fefa793494ccb3c79761e7bb`；独立真实节点33/33、开发回归28/28和四项既有fixture各1/1均通过，见 阶段集成记录（`testing/runs/deliver-session-capacity/2026-11-16-phase1-integration.md`）。自动节点33分支与完整32个CP定义不同，不能互相替代。真实浏览器已完成49→50、满额拒绝、显式释放49、重新新建50和重连读取50，见脱敏记录（`testing/runs/deliver-session-capacity/2026-11-16-phase1-browser.json`）。其完整Web资源与当前字节一致，独立核对见复用证据（`testing/runs/deliver-session-capacity/tester/2026-11-16-phase1/browser-asset-reuse.json`）；它是较早Rust版本的实际DOM运行，新版释放竞态由v13节点测试覆盖。受控ACP对端不代表厂商模型或自动回收。

必要释放固定原操作预留集合及原资源代际；新增三项旧行为Red与Owned查询后闭源的第33项Red均保留，后者修复后定点和整组Green通过。v8两项跨PID日志下标观察错误没有手改签通过，修正观察器后已由v13整组实际重验。历史完整工程失败、旧fixture修正和此前v6完整检查成功均保留原始报告；v6早于新竞态修复，不能代替本轮v13最终工程结果。4097分块测试曾独立通过并耗时数分钟，不因静默等待判死锁。最终完整检查和构建按项目默认配置执行，不使用试验性优化签收。

最终 `npm run check` v7 实际exit0，明确绑定v13产品快照；Web类型、40文件/1016测试及构建通过，Rust格式、Clippy和全部实际运行目标通过，主lib为981/0（861.80秒）。27个打印summary合计1229通过、0失败、1ignored，不将子程序的重叠summary当作唯一测试总数。ignored真实Codex ACP集成例需要本机node/codex/codex-acp，本轮未执行，不能据工程通过冒签真实Harness。原结果见 v7结果（`testing/runs/deliver-session-capacity/2026-11-16-phase1-root-check-v7.result.json`）及同名log。

最终 `npm run build:rust` v2 实际exit0，采用项目默认配置，Rust构建14.27秒，前置Web生产构建同时通过；原结果（`testing/runs/deliver-session-capacity/2026-11-16-phase1-root-build-v2.result.json`）明确绑定同一v13快照。后续仅文档、任务和报告更新，须由最终清单与独立比较确认产品及测试字节未变。

CP-002的根/Web依赖审计均exit0、所有严重度报告为0；Gitleaks 8.30.1按原配置扫描固定源码、全部当时永久证据（含ignored日志）1286文件及340次本地提交历史，最终均无发现。绝对路径调用曾因原精确允许路径未匹配而报告3项既有公开向量，失败脱敏记录保留，未改配置；从副本根目录按相对路径重扫通过。扫描后新增的报告分别尾扫，不将首次1123文件扫描写成全部永久证据覆盖。精确范围及结果见最终独立验收。

### 第一阶段批准前的门禁快照

用户已授权开始本 change，并先更新旧合同、用例及实施计划。当前处于 WP1，tracked 进度为0/12；不是缺少一般实施授权。完整编码门因 CAP-R001 的真实安全依据缺失受阻，主树归属及多入口加载责任同时需落实。没有产品代码、迁移或真实自动回收结果。

当前源码入口与接口责任以 [implementation-contract.md](implementation-contract.md) 为准；当前验收定义由独立 tester 维护 [tests.md](tests.md)。完整范围没有缩减，CP-011 的真实50主会话、真实30分钟及成功安全回收仍为必需。先交付受限分支的分阶段方案尚待用户决定，未当作已批准范围。

独立源码审计见 reviewer报告（`testing/runs/deliver-session-capacity/reviewer/2026-11-16-native-resource-audit.md`）。原始 proposal/design/plan/tasks/review 字节及摘要见 本轮基线（`testing/runs/deliver-session-capacity/2026-11-16-implementation-gate/baseline/manifest.json`），原 review.md 继续保留历史归属。下方带日期的旧轮次记录仅描述当时状态；“本轮不实施/上游未交付”不覆盖2026-11-16授权和实际代码，不得据旧结论放行。

> Q-001（2026-10-31）当前修订：本change复用通用闭环当前八份specs。缺历史重放不再整体禁用；输入受理授权、共享队列、无期限/不跨重启正文、附加审计及正文/输出容量取舍以通用规格为准。下方与旧RP/R2冲突的规划或用例预期不再有效，原ID/标题和证据保留历史归属；本次关联修订须独立tester/reviewer重新检查，不表示已实现或已验收。

## R2-002 当前修订范围（2026-10-31）

用户同意修复管理存储故障下限。本 change 复用故障安全控制，但资源已关闭且管理结果未可靠提交时保留名额，核验和持久化协调成功后才返还；新增 Scenario 待独立 tester 映射及 reviewer 检视。主执行者串行修订相关文档，不改代码、独立 tests/review、原任务或历史结果；完整合同和工作包仍待重基准，实现不恢复。

---

## ARCH-001 当前修订范围（2026-10-30）

本轮按用户授权明确容量计数、活动覆盖、显式释放和自动安全回收的独立边界；当前依据为本 change proposal/specs/design 及总体设计。主执行者串行维护共享文档，不改代码、独立 tests/review、历史报告或任务勾选；实现继续暂停，旧结论不覆盖本轮。

恢复实施前由独立 tester 重基准场景：前台 idle 但后台及子工作未知仍计数和保护；Run 结束不清除有效后台审批；检查至关闭之间的原生自发及外部活动不能由平台锁排除；完整证据时正常回收；全部目标受保护或未知时满额拒绝；重连失去依据后重新保护。独立 reviewer 检视后才更新详细合同、工作包及实际能力验证安排。不把规格中的安全条件当作真实 Harness 已提供对应能力，也不签本轮独立或产品通过。

---

## DC-001 独立文档结论（2026-10-30）

独立 tester `/root/spec_tester` 编写本轮 CP-024–026，独立 reviewer `/root/spec_reviewer` 限定文档复核通过，无未关闭阻断；DC-R001（P2，重放反复撤销满额候选）已由原 reviewer 复核设计及 CP-025 关闭。详见[本轮 review](review.md)顶部。全量 OpenSpec strict 11/11、`git diff --check` exit 0；7条新增产品用例均 not-run，不签产品、全面架构或完整重基准通过，实现仍暂停。本记录仅转录独立结论，下方规则及计划原文不变；29文件固定摘要属于本记录追加前快照。

---

## DC-001 三项取舍当前范围（2026-10-30）

用户确认实际重放可刷新30分钟无活动计时，但不形成执行、有效审批或后台任务保护，满额仍可安全淘汰；默认继承节点环境并允许显式覆盖，不新增白名单或环境指纹准入；原生身份以显式接入来源+原生会话ID协调，同来源多入口复用，不自动证明不同来源背后物理存储等价，也不以缺少此证明阻断已配置来源。

本轮独立 tester 负责必要用例定义，拟验证 CP-024–026：实际重放刷新与缓存访问对照、容量淘汰及真实回收。 产品用例本轮全部不执行；恢复实施前须固定新版合同、实现和环境，由独立 tester 在 Windows 专用数据根/配置、节点与 Web 环境验证，真实与替身边界分别报告，证据归 testing/runs/deliver-session-capacity/。本轮规格、设计和规划由主执行者串行修订，避免同文件重叠写；tester 仅写三份 tests，reviewer 只读，两者按 role 文件保持独立。

当前规则以更新后的规格及总体设计为准。下方 RP/SR 审查和摘要属于各自历史快照，冲突的物理存储证明、一概查看不保活断言不再适用。详细合同、旧用例及执行工作包全面重基准尚未完成，本轮只同步文档，独立复核结论另行记录；不恢复实现、不修改任务勾选、不删除实际数据，不把结构校验当产品验收或其他架构风险已解决。

---

## RP-001 独立文档结论（2026-10-30）

独立 reviewer `/root/spec_reviewer` 对用户已确认的重放准入及兼容修订作限定文档复核：通过，本轮无新增阻断；本 change 仅补充影响范围，不机械新增重复用例。详见[本轮 review](review.md)顶部。本结论只覆盖本轮规则与用例定义，不代表产品、安全实测、全面架构或执行工作包通过。全部产品用例 not-run；旧合同、旧用例及工作包全面重基准和其他架构整改尚未完成，实现继续暂停。以下方案、规格、用例和计划的固定快照是本报告追加前值，不能冒充报告追加后的文件摘要。

---

## RP-001 独立文档结论（2026-10-30）

独立 reviewer `/root/spec_reviewer` 对用户已确认的重放准入及兼容修订作限定文档复核：通过，本轮无新增阻断；本 change 仅补充影响范围，不机械新增重复用例。详见[本轮 review](review.md)顶部。本结论只覆盖本轮规则与用例定义，不代表产品、安全实测、全面架构或执行工作包通过。全部产品用例 not-run；旧合同、旧用例及工作包全面重基准和其他架构整改尚未完成，实现继续暂停。以下方案、规格、用例和计划的固定快照是本报告追加前值，不能冒充报告追加后的文件摘要。

---

## RP-001 重放兼容当前范围（2026-10-30）

本轮未机械新增本 change 的产品用例，仅标明重放政策影响范围。主接入门禁由通用闭环验证；资源、删除或备份本身仍按对应规格独立检查，已有相关用例只有确认未受影响后才能复用。尤其平台-only离线恢复不因原生重放不可用而失败。旧计划及产品结果仍须全面重基准，不在本轮执行。

Q-001当前引用：无重放也可按实际能力接入，历史与其他能力分别判断；当前行为统一遵循通用 harness-management、structured-history 和 acp-platform-constraints。下方原版本方案及独立报告仅作历史追踪，与新规格冲突的预期不再有效。

本轮只修订文档；独立 tester 增补本轮用例，独立 reviewer 复核本次范围，产品用例仍未执行。详细合同、全套旧用例与工作包全面重基准及其他架构底座整改未完成，不恢复实现、不修改 checkbox，不删除原生数据或改写旧证据。平台身份、审计及离线平台备份不因 Harness 未准入而失效。

---

## 本轮文档修订独立复核（2026-10-30）

本 change 的 SR-003 文档修订和新增用例 CP-019–023 经独立 tester `/root/spec_tester` 编写、独立 reviewer `/root/spec_reviewer` 限定复核通过。对应的受审查快照、范围和问题关闭记录见 [本轮 review](review.md) 顶部。50 文件报告追加前聚合 SHA-256 为 `7e77ec113fd48b6796139cfb4465d2949dc3a788c7ee033e23ddc84136d77d2d`；追加本记录不改动规格、设计或用例。

本次只完成文档检查：全量 OpenSpec strict 11/11 与文档 `git diff --check` 通过；R-SR-DOC-001 和 R-SR-TEST-001 经独立复核关闭。新增产品用例全部未执行；旧合同、旧用例及执行工作包仍待全面重基准，不恢复实现、不勾产品任务、不把下方旧阶段记录当新版通过。下方 Project Verify 已登记本轮用例及未来执行安排，须在恢复实施前落实到重基准后的工作包。

---

> 五项审查修订补充（2026-10-30）：跨入口原生身份、非 Run 操作审批、节点共享连接视图、能力准入分层及按能力独立交付已写入当前 specs/design。下方旧用例与工作包尚未覆盖这些新场景和依赖调整，保持原 ID 供独立重基准，不据此开工或声明覆盖。

> 2026-10-30 规格重基准门禁：实现暂停。会话及历史以 Harness 为权威，见本 change 当前 specs 与 design。下方为原方案的plan.md内容及证据，凡依赖平台聊天持久化、入库版本、强刷、导入批次、平台删除或历史备份的部分均已失效；不可据旧任务勾选、旧检查或旧审查恢复实现。待独立 tester 修订用例、详细合同与工作包重排、reviewer 重新检视后再决定恢复。历史证据保留原范围，不改写旧结论。

## Execution Model

本节以下工作包、并行评估与合入表保留WP1开始时的完整change计划；第一阶段实际写范围、隔离存储交接和串行合入依据为 [phase1.md](phase1.md)，当前结果以本文开头及 Findings / Verification Results 为准。未执行的完整自动回收计划不是本阶段已实现能力。

2026-11-16进入WP1。主执行者维护实施合同、设计依据及plan/tasks；独立tester重基准验收预期，独立reviewer查阅实际代码并复核同一规划快照。产品行为以当前spec为准，旧文档检视不能签本轮完整编码门通过。

依赖已归档通用闭环的实际资源所有权、活动及释放接口；完整子树浏览、历史分页、Pi/OMP不作硬前置。原生主树身份、当前已知活动与请求/结果合同仍需真实依据，缺失时不把相邻能力归档当作满足。本轮基线为65d734f638bcb951e237b69faa592917b3a6d2c7，开始时tracked clean。角色按openspec/role分工，tester/reviewer不继承开发对话；不提交、推送或发布。

开始实现先记录基线、未提交文件及删除项SHA-256、真实工具版本和隔离数据根。目录沿上游单Rust应用app/与TypeScript clients/web/；以下为未来具体写范围，WP1须在真实上游树核对路径，不存在的检查入口先建立再执行。可控时钟只验证时间边界，不能替代真实50会话和真实30分钟策略证据；记录内存/进程数而非预宣称硬件容量。

## 本轮回收资格澄清

WP2只对主会话计数、扫描和回收，子生命周期交由父/Harness管理，子仍执行则保护父会话。待授权同时豁免超时与满额淘汰，授权取消只重评资格，不直接关闭；其他授权或执行工作尚存则继续保护。沿用既有安全空闲计时与释放确认规则，容量满则在安全候选中按静默时间选择。WP3展示保护及候选/释放事实，不把取消授权展示成已空闲；WP4补充CP-013–018并更新CP-011真实闭环，WP5重新检视当前文本。旧review仅对应旧快照，不用于签当前修订通过；新增用例均待实施，不勾任务完成。

## 测试流程修订

2026-11-16独立tester将当前用例重基准为32例（CP-001–CP-032），保留原26例全部编号/标题，修正失效预期并新增6例；逐名映射当前10条Requirement/26条Scenario。CP-012为L5独立reviewer工具检视，不新增人工门禁；工具/环境/权限或入口缺失记受阻/未执行，同版本同范围有效证据可复用。本文的执行安排仍须独立复核，旧review不能签本轮通过。

## Parallel Assessment

WP1的tester与reviewer可并行：tester仅写tests.md及自有报告，reviewer只读产品并写自有报告，主执行者串行维护其他规划。产品实现尚未放行。放行后，WP2负责节点/管理协议，WP3在独立工作区实现冻结DTO上的Web展示，独占数据、端口及CARGO_TARGET_DIR；共享管理入口由WP2先合入再集成WP3，不能并发写同一工作区。WP4/WP5对固定同版分别验收/检视，可以并行只读但最终须等全部必需结果。当前未创建产品开发工作区、启动独立构建或安装运行Harness，不把并行安排写成已执行。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 上游源码映射、真实安全依据及实施门禁 | 实际通用闭环；无完整子树浏览或Pi/OMP前置 | 主执行者、独立tester及reviewer | 本change proposal/design/implementation-contract/plan/tasks；tests仅tester维护；testing/runs/deliver-session-capacity/各自目录 | 覆盖Create/执行加载/Replay的合同、场景映射、真实取证路径、固定摘要；CAP-R001等阻断未解不编码 | CP-001/002的入口核对及全部场景的独立方案检视 |
| WP2 | 节点级50名额、30分钟安全策略、活动保护、管理协调及原始安全端口 | WP1完整编码门通过 | coder | 拟建app/src/domain/session_capacity.rs及同名测试目录；app/src/application/native_dispatch/capacity.rs；runtime/source/source_sessions/read_access/resource_close/observation/shutdown的精确接线；app/src/domain/mod.rs；app/src/application/native_dispatch/mod.rs；app/src/node/session_services.rs；app/src/management_ws/；app/src/session_storage/容量合同及必要注册；app/tests/session_capacity_contract.rs；docs/implementation/session-capacity.md | 全入口单一账本、按已知事实发起的自动回收及开发/故障测试；复用普通关闭但不绕过节点已知活动保护 | CP-003/004及CP-005–010、CP-013–032节点断言、适用Red Check；新迁移与DTO须先冻结并独立检视 |
| WP3 | 既有认证管理入口中的容量、保护/受限及错误反馈 | WP1门通过，WP2 DTO冻结 | coder | 拟建clients/web/src/management/session-capacity.ts；既有clients/web/src/management/session-protocol.ts、session-view-status.ts、source-session-view.ts及clients/web/src/main.ts、clients/web/src/styles.css必要集成；clients/web/tests/session-capacity.test.ts；docs/implementation/session-capacity-web.md | Web展示及交互测试，不新建平行厂商流程 | CP-005–010、CP-013–032页面范围；Web check/test/build |
| WP4 | 独立验收程序、故障注入与真实闭环 | WP2/3集成版本冻结 | 独立tester | 拟建app/tests/session_capacity_acceptance.rs；clients/web/tests/acceptance/session-capacity/；testing/runs/deliver-session-capacity/tester/；本change tests.md | 逐CP结果、真实50/30与清理证据，fixture不代真实Harness | CP-001–032，CP-012由WP5执行；全部必需分支保持原级别 |
| WP5 | 最终集成、独立交付检视及摘要 | 同版WP4全部必需结果闭环 | 独立reviewer与主执行者 | testing/runs/deliver-session-capacity/reviewer/；本change plan/tasks结果段；README及能力地图交付状态 | 同版本独立结论、原提出者复查及受限事实；未完成不归档 | CP-012及CP-001–032证据核验、strict/diff、最终完整检查 |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 上游合同/真实入口/独立方案门禁先确定 | CP-001、CP-002，核对差异不覆盖用户改动 |
| 2 | WP2 | 节点合同供集成调用；与其他change共享注册按依赖串行 | CP-003、CP-004及上游执行/历史/认证回归 |
| 3 | WP3 | 在WP2最终接口上集成；如接口变化必须回改本包 | CP-005–010、CP-013–032对应页面分支，节点合同及真实Web构建/类型检查 |
| 4 | WP4 | 独立验收只针对完整集成版本 | CP-001–032中tester负责部分（不含CP-012），明确CP-011/023/026/032真实L6及其余各级边界、逐例断言和清理 |
| 5 | WP5 | 结果摘要仅本包写，避免与tester报告冲突 | 重核同一受测文件清单、全部检查及问题关闭证据 |

表内产品路径是拟建/接线范围，不声称已经实现；确切DDL、错误DTO、注册和进程所有权改动须在WP1补齐独立检视后才能进入WP2。当前不新增依赖，依赖必要时先审查并调整锁文件写范围。WP2先合入共享协议/注册，WP3后合入Web并重跑管理合同和Web全套；WP5串行更新README/能力地图，其他change不写这些文件。本轮只修改规划及报告。

## Project Verify

2026-11-16独立tester已按RB-001修订32例，保留稳定ID和标题。第一阶段结果保留原范围；本轮新增行为尚待执行。CP-011验证按已知事实发起请求、Harness响应及必要管理协调，不要求隐藏活动或原子关闭窗口证明；仍不得用受限拒绝、可控时钟或fixture替代真实50/30路径。

CP-013–032纳入WP2/3对应节点与交互合同、WP4全量验收及WP5证据检视；产品执行均为not-run。CP-011/023/026/032保持真实L6，分别覆盖主闭环、直接子加载、重放真实计时及安全能力/故障协调，不以可控时钟或模拟组件代签真实链路。

工作目录统一D:/Project/acp-remote，实际构建/测试子目录与完整命令由WP1从README和工程配置确认并写入报告；现有上游工程命令按README核对，本change专用入口缺失记not-run，不以其他目标成功替代。环境为Windows、可信HTTPS、Chromium、隔离数据库/附件/原生根及所属进程；Rust工具链只读实际配置，Harness/适配器/协议版本及二进制摘要固定。证据写testing/runs/deliver-session-capacity/，报告需含断言数量与具体结果，不能只有截图。

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | 独立tester：CP-001、CP-002；根目录 npx.cmd openspec validate deliver-session-capacity --strict 与 git diff --check，按WP1核定工程真实入口执行L1/L2 | 规划校验只证明文档；真实Rust/Web构建、合同、格式/静态与供应链检查实际执行，零目标/缺入口不通过 |
| PV2 | 独立tester：CP-003、CP-004；app/与clients/web/内WP1确定的领域/合同/故障测试入口 | 验证事务、并发、取消、失败与资源所有权；fixture不冒充真实兼容性 |
| PV3 | 独立tester：CP-005–010、CP-013–022、CP-024–025、CP-027–031；实际Web/节点管理及各例指定故障/边界入口按tests.md逐项操作 | L5对应场景结果可观察，局部时钟/竞态注入单独标范围；覆盖主树、重放及未知保护，不因能力缺失改预期 |
| PV4 | 独立tester：CP-011为Main E2E，CP-023/026/032为专项真实L6；Windows+Web+真实Harness/适配器+SQLite实际产品入口 | 默认真实50主树和实际30分钟、直接子加载归属、实际重放刷新及RB-001请求/结果/必要管理故障协调；纯页面/缓存不保活，重放不赋工作保护；关闭确认且管理协调后才返额，新请求再竞争预留，无候选拒绝，必需失败/未执行阻断 |
| PV5 | 独立reviewer（工具验证L5）：CP-012；同版本源码/配置/证据 | 安全、交互、协议语义、依赖与敏感数据独立结论，不以主执行自查代签独立结论 |
| PV6 | 主执行者核对完整待提交集成快照，独立reviewer复核 | 新修改触发影响分析与相关重验；不能以旧HEAD证据覆盖未提交实现变化，归档路径复核另行执行 |

## Code Review

编码前reviewer独立读取用户原话、AGENTS、config、proposal/specs/design/tests/plan/tasks，人工逐条核对真实Requirement/Scenario、WP关联、Main E2E及可执行入口；阻断未知和独立问题未关闭不可开工。文档齐备、方案检视通过、产品验收通过是三种不同结果。

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | 独立reviewer（不同于coder/tester） | 上游证据、能力门禁、方案和用例、写范围 | 必需决策已解决且问题ID有独立关闭依据 |
| WP2、WP3 | 独立reviewer | 真实diff、协议/事务/资源、安全和页面反馈 | 对同一集成版本完成检视，相关测试实际通过 |
| WP4、WP5 | 独立reviewer（含L5安全及交互检视） | 全用例结果、证据范围及复用、问题重测复查 | 全部必需测试通过，无未验证项被写成通过；未满足保持未完成 |

## Main E2E

```yaml
mode: required
```

由独立tester按CP-011在真实Windows节点与Web执行，真实Harness和存储参与；使用默认50主树及实际30分钟，长任务/审批/子后台受保护，纯页面/心跳/缓存访问不保活，节点实际接收重放刷新计时但不赋工作保护；候选新活动复核、关闭失败/未知或管理未协调不返额，无候选拒绝不排队。CP-023/026/032按PV4另行完成真实专项L6，不被CP-011汇总替代。fixture仅限tests标明的局部边界；记录成功/失败/取消后的完整进程树清理和隔离数据保护，L6不能以启动成功、截图或受限负向分支代替。

## Risks

通用闭环已有代码并已归档，但不证明已知活动、主树及自动请求处理能力。CAP-R001原隐藏活动/窗口门由RB-001取代，新请求合同仍须独立检视及实现验收；Create/Replay遗漏计数、猜测主树或把SQL拒重当跨入口复用都会破坏50名额语义。安全依据缺失时按规格保护，保持其他已准入能力，不新增厂商私有探测。真实50/30验证可能受能力及资源条件限制，明确记录而不降低判据；不删除用户数据或未经授权提交。

## Handoff

交接用户2026-11-16实施授权、Windows+Web/50主会话/30分钟及当前规格，附AGENTS/config/角色文件、本change完整产物、实施合同、固定版本清单、精确读写范围和真实检查入口。不传开发对话或自评。独立tester/reviewer各自维护稳定问题，原提出者复查关闭；分阶段建议尚未批准，不把它当作交付范围。

## Findings

当前稳定问题由独立 reviewer 逐项维护于 第一阶段问题状态表（`testing/runs/deliver-session-capacity/reviewer/2026-11-16-phase1-findings-status.md`），不以本段自查代签。CAP-R002/004及009–014/017的具体缺陷在阶段范围内闭环；005–008/015/016按报告限定问题闭环。CAP-R003只完成标准new独立根证明和未知守卫，没有可信父子关系映射及共享名额正向。CAP-R017的v12 hook-only实际Red、v13 Green/整组重测及最终工程已获原reviewer复核关闭；CAP-R017-E1及DOC-20261116-01文档问题亦关闭。CAP-R001旧安全承诺变更待原reviewer标记；新自动请求实现与验收仍待完成。

### 开始时的问题快照

下表是开始实施时的状态，不表示本轮实现及重测后仍全部未实现；保留原ID和结论用于追踪。

| ID | Source | Severity | Target Version | Finding | Fix Version | Status | Retest / Re-review |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 方案/用例/计划/任务检视 | 独立reviewer | 文档结论 | 当前review快照 | 旧快照文档曾检视通过，不覆盖本轮资格澄清或产品验收 | 旧快照 | 本轮待复核 | 逐文件摘要及独立依据见review.md |
| CAP-R001 | 独立reviewer | P1 | 65d734f | 后台/主树完整活动与检查至关闭窗口无依据，真实CP-011无可执行安全路径 | 未修复 | 编码门阻断 | reviewer/2026-11-16-native-resource-audit.md；不由coder关闭 |
| CAP-R002 | 独立reviewer | P1 | 65d734f | Source Create和SourceOnly Replay均须在原生副作用前纳入容量 | 合同记录，未实现 | open | implementation-contract实际入口映射；待实现及原提出者复查 |
| CAP-R003 | 独立reviewer | P1 | 65d734f | 目标身份不等于可信主树关系，未知不能猜根 | 合同记录，真实依据待取得 | open | 同报告；不得用完整子树浏览硬前置掩盖接口缺口 |
| CAP-R004 | 独立reviewer | P2 | 65d734f | 身份拒重不等于跨配置复用同一原生owner | 合同记录，未实现 | open | 同报告；统一ledger和入口协调须纳入详细设计 |
| CAP-R005 | 独立reviewer | P2 | 首轮manifest规划 | Project Verify遗漏CP-019–032真实分级安排，18例及一概查看不保活描述过时 | 当前plan已补PV3/PV4、Merge Order、32例及重放语义 | 修复已提供，独立复查见报告 | reviewer/2026-11-16-planning-gate.md；不据此关闭CAP-R001 |
| CAP-R006 | 独立reviewer | P3 | 首轮manifest规划 | WP3样式文件写为style.css，实际为styles.css | 当前plan明确clients/web/src/styles.css | 修复已提供，独立复查见报告 | 同报告；拟建文件与已有集成文件分别表述 |

旧快照独立检视已完成，实际问题ID与关闭依据保存在review.md；后续实现或行为文件变化须按影响重新检视，不能沿用旧结论。

## Verification Results

> **2026-11-18 最终交付收口结论（20/20）**：本变更已按 4.2 双层边界完成收口验收。统一测试二进制 `acd38a1dc11b2dd2685d4776514984e2880fccab5a048f301a16a7057f7bf606` 的控制面 L6 编排与状态机/存储验证记录、工程门禁日志与 SHA256 清单归档于 `testing/runs/deliver-session-capacity/tester/2026-11-18-l6-control-plane-e2e-v1/ASSESSMENT.md`；最终独立交付检视报告见 `testing/runs/deliver-session-capacity/reviewer/2026-11-18-final-delivery-review.md`；68 分钟真实 Codex 宿主物理单调时钟证据延续自 `testing/runs/deliver-session-capacity/tester/2026-11-18-l6-cp026-remediation-v1/ASSESSMENT.md`（外部上游配额至 `2026-12-10 11:44` 限额已声明）。

当前受测产品为v13未提交文件快照，版本及命令见 集成记录（`testing/runs/deliver-session-capacity/2026-11-16-phase1-integration.md`）。独立测试使用真实Windows Node/TLS/WSS/SQLite和受控ACP进程，实际33节点、28开发及四项既有fixture通过。完整CP的真实自动回收、父子关系正向、厂商50树与实际30分钟及L6仍未执行。领域可控时间和受控对端不能代替这些结果。

正式阶段逐分支映射由独立tester维护于 阶段证据矩阵（`testing/runs/deliver-session-capacity/tester/2026-11-16-phase1/phase1-evidence-matrix.md`）；真实浏览器复用限于同Web字节与已核范围，不能宣称新版Rust已重跑DOM。Socket开发fixture的SQLite worker join仍未观察、专属目录保留；新节点用例的joined/metadata/Job及目录结果按各自真实日志记录，不把失败目录保留写成已移除。

最终独立阶段验收及CP-012限定交付检视均PASS，原始报告见本文开头。v14相对实际运行的v13仅7份Markdown变化；阶段完成后的结果文档及checkbox另固定最终清单并由两角色复核，不复用摘要掩盖产品变化。完整工程、依赖审计、密钥扫描与各层功能结果分别成立，不互相代签；“SQL已commit至运行时record发布与source收集交错”仍只有静态所有权论证，没有定向实测。

### 开始时的验证快照

以下基线和CP表保留开工时未执行状态；当前阶段结论以上述矩阵、固定版本结果和独立报告为准，不覆写历史失败或将完整CP补签为阶段通过。

- 受测版本：2026-11-16开始于65d734f638bcb951e237b69faa592917b3a6d2c7；本change尚无产品实现。规划修改前基线及本轮固定文件摘要分别保存，不能仅用HEAD代替未提交集合。
- 摘要排除范围：只允许plan结果段、tasks勾选和报告自身；实现、配置、测试及用例变化不得排除。
- 执行者与环境：主执行者核对源码与启动npm run check工程基线；独立tester维护用例，独立reviewer审计真实代码。没有执行本change真实产品验收。
- 验证命令及目录：现有入口为根目录npm run check、npm run build:rust及clients/web的check/test/build，命令以package.json为准。专项容量目标尚未实现；基线检查完成结果见本轮implementation-gate报告，不将其写成CP-001或产品通过。
- 2026-11-16工程基线：Web类型/39文件1002测试/构建、Rust格式及clippy通过；全量Rust测试进程以STATUS_INVALID_HANDLE退出，npm run check最终exit1。沙箱外串行复跑一个相关既有节点用例1/1通过，不能外推其余失败或归因为权限。准确命令、未执行项及范围见 baseline-checks.md（`testing/runs/deliver-session-capacity/2026-11-16-implementation-gate/baseline-checks.md`）。

| Case ID | Executor | Target Version | Status | Evidence / Limitations |
| --- | --- | --- | --- | --- |
| CP-001 | 独立tester待安排 | 产品快照未建立 | not-run | 当前无产品入口；定义不等于执行证据 |
| CP-002 | 独立tester待安排 | 产品快照未建立 | not-run | 当前无产品入口；定义不等于执行证据 |
| CP-003 | 独立tester待安排 | 产品快照未建立 | not-run | 当前无产品入口；定义不等于执行证据 |
| CP-004 | 独立tester待安排 | 产品快照未建立 | not-run | 当前无产品入口；定义不等于执行证据 |
| CP-005 | 独立tester待安排 | 产品快照未建立 | not-run | 当前无产品入口；定义不等于执行证据 |
| CP-006 | 独立tester待安排 | 产品快照未建立 | not-run | 当前无产品入口；定义不等于执行证据 |
| CP-007 | 独立tester待安排 | 产品快照未建立 | not-run | 当前无产品入口；定义不等于执行证据 |
| CP-008 | 独立tester待安排 | 产品快照未建立 | not-run | 当前无产品入口；定义不等于执行证据 |
| CP-009 | 独立tester待安排 | 产品快照未建立 | not-run | 当前无产品入口；定义不等于执行证据 |
| CP-010 | 独立tester待安排 | 产品快照未建立 | not-run | 当前无产品入口；定义不等于执行证据 |
| CP-011 | 独立tester待安排 | 产品快照未建立 | not-run | 当前无产品入口；定义不等于执行证据 |
| CP-012 | 独立reviewer待安排 | 产品快照未建立 | not-run | 当前无产品入口；定义不等于执行证据 |
| CP-013 | 独立tester待安排 | 本change产品快照未建立 | not-run | 用例定义不是执行证据；缺入口或能力保持未执行 |
| CP-014 | 独立tester待安排 | 本change产品快照未建立 | not-run | 用例定义不是执行证据；缺入口或能力保持未执行 |
| CP-015 | 独立tester待安排 | 本change产品快照未建立 | not-run | 用例定义不是执行证据；缺入口或能力保持未执行 |
| CP-016 | 独立tester待安排 | 本change产品快照未建立 | not-run | 用例定义不是执行证据；缺入口或能力保持未执行 |
| CP-017 | 独立tester待安排 | 本change产品快照未建立 | not-run | 用例定义不是执行证据；缺入口或能力保持未执行 |
| CP-018 | 独立tester待安排 | 本change产品快照未建立 | not-run | 用例定义不是执行证据；缺入口或能力保持未执行 |
| CP-019–026 | 独立tester已重基准定义，未执行产品 | 本change产品快照未建立 | not-run（各ID） | 主树加载、来源及重放真实证据尚未取得；保留原稳定ID与标题 |
| CP-027–032 | 独立tester新增定义，未执行产品 | 本change产品快照未建立 | not-run（各ID） | 完整活动、安全窗口、受限管理、R2账本及全部加载入口；真实正向能力未证，不能以受限负向分支代签 |

- 未执行项与已知限制：本change产品构建、合同、真实Harness、Main E2E及独立交付检视均未执行；既有工程基线分步结果见上表和报告。完整编码门CAP-R001受阻，不把发现缺口或用例定义完成写成产品任务完成；规划校验结果另记。
- 问题关闭依据：待独立提出者提供固定版本复验，不由coder关闭。
- 新版本证据判断：后续改动逐项分析影响，由独立tester/reviewer确认重验或同版本证据复用。
- 归档后复核：尚未归档，未来检查移动映射、规范/长期suite及摘要保真；不能提前签通过。

## Workflow follow-up

产品实现、全部tracked任务证据与独立结论齐备后再同步长期规格和用例并归档，归档后复核真实路径映射及受影响检查。本轮不归档、不提交或发布。
