# Windows 进程 owner 冻结包独立检视

2026-10-30。**PASS（仅所列5文件平台包及已观测Windows测试范围），未发现需要PLAT-R阻断修复的代码问题。** 不签CL最终、真实Harness/Web、共享实例或审批安全接线。独立reviewer只读代码/日志，未继承开发自评，未改产品/plan/任务。

## 固定输入与验证

根：`C:/Users/jack/.codex/worktrees/codex-session-platform/acp-remote`；基线HEAD为65b46c01d6b95fc1f1a8223b1c92a729a885c185。实际首尾核对 `.tmp/platform-delivery.json` 内 **5/5文件**摘要一致，4/4日志摘要一致。交付只限 app/Cargo.toml、DEPENDENCIES.md、src/platform/mod.rs、src/platform/process_owner.rs、tests/platform_owner.rs。完整摘要以冻结清单为准；**不得将旧工作区lib/storage/domain等覆盖主D已集成WP2**。

实际核读fmt/check/clippy/test日志，已有开发全目标结果144库+4JWS+9平台测试；这是旧平台工作区基线，不是主D200库的存储回归。fmt为空日志、退出0来自交付执行记录，未冒称本reviewer重跑全工程。Cargo.lock与主D均为`87F27B619281B37015C79CEAE8D95471C8A4DE50F202F4DD8A6D99FA0D27E324`；无新增/升级crate。

冻结清单SHA256=`3C3F5E56B03530D2C3F7F67115E3DBDCD38D375C53FDB9E7A1C21FB29CF6F366`；独立运行testbin SHA256=`9AFB6646F03D2B83C43485689DDE42D611CB8D05F41B10E2B481DA0F4F6D05B7`。

本reviewer另以真实owner实际运行冻结testbin：`.tmp/target-platform/debug/deps/platform_owner-1c9e6b8f28d038f0.exe --test-threads=1`，exit0，**9通过/0失败，3.96秒**。包含8项行为测试及1个通常无操作、作为子进程入口的fixture_process，不称9个独立端到端能力。测试使用自有PowerShell/fixture子树、随机临时根和稳定进程句柄，无用户真实Harness/数据库/凭据。独立tester的正式平台报告未作为本源码结论的替代；本报告仅承担自身实际静态与重跑范围。

## 代码与资源检视

- **启动与Job**：CreateProcessW显式绝对程序路径、独立cwd、UTF-16参数，不经shell；使用CREATE_SUSPENDED和STARTUPINFOEX句柄列表。Job为无名、不可继承、KILL_ON_JOB_CLOSE，无breakaway；Assign成功后才Resume。SuspendedProcess RAII覆盖创建后失败，已归Job路径另有最终Job关闭保证；未发现用户代码在归属之前执行的顺序窗口。同步创建段无await，不会在持裸结构指针时被异步取消。
- **unsafe及句柄**：OwnedHandle独占成功句柄，属性列表使用pointer-aligned Vec、按初始化状态调用Delete；结构尺寸检查有语义错误。父overlapped管道与Job不可继承，只列三项同步子stdio；子句柄在创建完成后由局部owner关闭。私有pipe复用owner-only DACL、随机192bit名字、first-instance、拒远端；已有windows.rs/console.rs实际与主D相同，未改DPAPI/pipe安全实现。
- **async IO/取消**：ProcessIo直接给Tokio NamedPipeServer，未新增不可取消的阻塞read线程；take_io仅一次且不转移Job。spawn外层deadline覆盖管道connect等待，await间局部owner析构；wait_exit取消不丢owner。deadline已过启动拒绝已实际测；活跃spawn中各await取消、系统调用超长阻塞没有故障注入，不声称全面时限证明。
- **关闭**：shutdown用绝对deadline，最多预留100ms强杀观察，不重设总预算；活动数是Job树计数而非根退出。Incomplete保留owner/句柄，可再次调用；Terminate/Query失败返回固定错误且owner保留。Drop尽力TerminateJobObject再由kill-on-close兜底，不创建清理线程。taken IO由上层任务owner取消/收敛，Job kill用于解除阻塞，不要求读写任务先join才允许kill，避免循环等待。独立重跑父先退出/系统PowerShell孙进程仍活、阻塞双输出/stdin、Drop、嵌套Job owner异常退出、拒breakaway与拒继承sentinel均通过。
- **参数、环境与秘密**：Windows argv转义覆盖空参数、引号、尾反斜杠及中文，显式环境块按CompareStringOrdinal去重排序；继承父环境后按Windows名称规则覆盖，属于该端口明示行为，上层须限制可暴露配置。ProcessSpec无Debug，错误仅固定操作名/数字码，不拼参数、路径、环境值；invalid输入测试不泄合成秘密。没有将子进程stderr自动当日志输出。argv/env参数真实性由上层Harness配置边界负责，不把此owner当任意远程执行授权。
- **依赖feature**：实际新增JobObjects和Globalization两个现有windows-sys0.61.2 feature，后者用于Windows ordinal环境名比较。DEPENDENCIES说明必要性、许可/维护复用及unsafe边界，锁不变；不是新crate，也不是新供应链扫描。plan先前只写JobObjects，主执行应同步为实际两feature范围，属于事实记录细化，不要求新产品授权，不阻断本限定代码结论。

## 问题/限制与合入条件

冻结代码未发现需创建PLAT-R阻断项的问题。尚未做AssignProcessToJobObject/ResumeThread/Terminate/Query失败注入；测试不保证所有系统API在deadline内返回，也没有确定性注入Incomplete及随后恢复观察。失败路径清理本次为静态证据，不能把通过的正常/强杀测试改写成这些故障已测。Job自身不实现共享实例引用策略、Run/审批失效或writer安全临界区；最终装配须防止单会话关闭共享Job。

整包仅5文件逐摘要集成，manifest feature合入后重跑主D/协议包的完整适用编译、测试、历史/身份撤销回归；旧工作区144库不替代当前200库。正式产品CL-030/031仍待同版真实集成证据。若后续补故障注入或修改平台实现，应更新冻结清单后由tester/reviewer重新判断受影响范围，不能无条件沿用本PASS。

## 同日追加：独立 tester 环境发现 PLAT-T001，当前集成 BLOCKED

已实际读取主D `tester-platform-owner.md`。同一冻结清单3C3F5E56...的独立tester真实owner环境中，9项开发测试2过/7失败，正常spawn在创建子进程之前即因继承环境合法隐藏驱动器键`=C:`被environment统一名称校验拒绝。tester外部空override/绝对程序cwd probe亦复现，未清环境绕过，未输出环境值。证据与问题归属详见该独立报告PLAT-T001及其固定日志。

**当前平台包集成结论改为 BLOCKED，等待PLAT-T001修复及原tester关闭。** 本reviewer此前9/9实际执行结果保持原环境、原快照归属，不回写成未运行或失败，也不据自己的环境通过否定tester失败。此前静态审查未覆盖Windows继承hidden-drive环境键，这一限制现已明确；原PASS不能作为跨环境无条件放行依据。

修复应区分系统继承hidden-drive变量和用户override，保留严格拒绝任意含等号/非法override，不以清除tester原环境或放宽所有键代替。新manifest冻结后本reviewer应核受影响env合并/排序/编码、原错误隐私及新增回归，原tester在原环境重跑后独立关闭PLAT-T001。Globalization feature已由主执行同步plan事实范围，此文档差异不再作为待同步事项。

## 同日追加：PLAT-T001 修复候选独立代码复查

新冻结 `.tmp/platform-plat-t001/delivery.json` SHA256=`E5F3896FA6FC9E6A9A2EA7D589790717899069B612DF662244006279C434E54E`。首尾实际核5/5文件匹配，8/8证据摘要匹配；旧manifest仍3C3F5E56...保留。实际读取原代码副本、修复环境循环、测试新增fixture分支、DEPENDENCIES，以及red/green/all日志，不以修复者自评代替这些依据。

**修复候选静态/本reviewer环境回归PASS；PLAT-T001仍由原tester重测关闭，集成阻断尚不由本节单独解除。** environment将系统继承和显式override按来源分流，仅继承项可使用精确UTF-16 `=[A-Za-z]:` 加终止NUL形状，其他含等号名称仍拒绝；用户override即使是=C:或小写也拒绝。合法原键值原样保留并进入原有Windows ordinal大小写去重/排序、UTF-16环境块，不清环境绕过问题，不普遍放宽任意等号键。

新测试真实cmd执行cd /d建立隐藏条目，fixture确认该键存在；空override正常spawn和嵌套OwnedProcess读取相同原值均有断言，只比较不输出原值。显式=C:/=c:/畸形键及A=B/空名仍报固定InvalidSpec且不泄SECRET。开发red原始代码确因invalid environment name失败，green和真实cmd全量144+4/平台11日志成功，其余启动/Job/async IO代码未改。

本reviewer实际以真实owner运行新编译 `platform_owner-1c9e6b8f28d038f0.exe --test-threads=1`，exit0，11/11通过，5.38秒（包含10行为测试与1个fixture入口）；没有清除当前环境或仿造tester原环境。新增hidden-drive场景由测试内真实cmd建立。未重编译144库或重跑全部旧基线，未把本环境PASS改写原tester失败。新代码SHA256为清单所列`17ABD5A4023874E0870B74947A23AF9529BB4DCC37A35781CA3133B1A98DC436`；新增测试为`BC24DF836BD805697B28B34244C9FB53034A274DA2D3315708729246E5588A25`。

原故障注入/Incomplete/共享实例/实际Harness及最终同版集成限制继续保留；等待原tester在其未清理的环境验证新快照、关闭PLAT-T001，再由主执行核对同版两份报告决定集成。仅允许5文件更新，不得覆盖主D存储基线。
