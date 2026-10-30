# WP3 平台所有权包：独立验收

日期：2026-10-30。结论：**BLOCKED — PLAT-T001，高：合法继承环境隐藏驱动器键导致正常spawn全部失败。** 原环境保留，没有删除隐藏键、修改产品或通过换前置把失败掩盖为通过。修复后需同版独立重测，当前不集成通过。

独立tester，只读冻结平台worktree和开发测试断言；独立build/test及外部probe输出均在自有`.tmp/platform-owner-independent`。范围仅平台API；不签共享ACP实例/Harness/Web/真实审批writer或最终CL。

## 冻结版本

候选worktree：`C:/Users/jack/.codex/worktrees/codex-session-platform/acp-remote`；base HEAD `65b46c01d6b95fc1f1a8223b1c92a729a885c185`（只有旧144库测试基线，不含主D WP2存储）。manifest `.tmp/platform-delivery.json` SHA-256 `3C3F5E56B03530D2C3F7F67115E3DBDCD38D375C53FDB9E7A1C21FB29CF6F366`。

5文件运行前后均逐项匹配，独立清单位于`.tmp/platform-owner-independent/frozen-before.sha256`及`frozen-after.sha256`，SHA-256 `757BC302C02B28040403ACC901B6FD51C62F56BED36810F5EBDF486890094713`：

| 文件 | SHA-256 |
| --- | --- |
| app/Cargo.toml | C48046E404E8D10B036EEC48163295A1386AA2CBE4BAEAFBE479FE823638045D |
| app/DEPENDENCIES.md | CF64D162A12E5F0ED740A8E9F46284134098054F82B3816EF552DAFE652F2EEC |
| app/src/platform/mod.rs | 38224819070BF82A2616BD96C8C52210ACD261F9E71614AC0AA61678609BB37F |
| app/src/platform/process_owner.rs | D7231D13F4C4BC02C9B2B58E5B306D7C0ACC96CCD312DFB9B67F96D0A8F40AF0 |
| app/tests/platform_owner.rs | 44F72407A48C91B84B40D1B7C0ED7680876C637B7E8E6061E1597509D958A7A9 |

未把候选其他基线文件合入/覆盖主D，未修改其源码、索引、执行位或锁文件。

## PLAT-T001 复现与依据

位置：`app/src/platform/process_owner.rs`的`environment()`（约505–519行）。实现将`std::env::vars_os()`与用户overrides合并遍历，对所有键统一拒绝含`=`字符的名称。Windows合法继承的隐藏驱动器当前目录变量不等同用户非法override，需要单独处理；本实现将它也拒绝。

独立实际观测仅输出键名和分类，不输出任何环境值或完整清单：

`inherited_invalid_for_current_validator_name="=C:"; matches_system_hidden_drive_form=true`

相同进程内，以绝对系统PowerShell路径、绝对专用cwd、参数`-NoProfile -NonInteractive -Command "exit 0"`且`ProcessSpec.env=[]`调用`OwnedProcess::spawn`，返回：

`InvalidSpec("invalid environment name")`

这是当前程序在正常可继承环境下拒绝合法启动，不是用户传入非法环境键，也不是owner/DPAPI或PowerShell运行失败；错误发生在建立子进程前。cmd启动链自然携带该键，本tester没有人为注入非法键或清理它。最小诊断源码为`.tmp/platform-owner-independent/src/bin/env-probe.rs`。建议原coder区分合法系统hidden-drive条目和用户override的严格校验，不能普遍放宽任意含等号键；修改后同时回归原非法override拒绝及合法继承场景。本tester不改产品。

## 实际执行结果

Windows真实owner环境、rustc/cargo 1.98.1；每次CI owner前置成功。CARGO_TARGET_DIR、TEMP/TMP均只在当前进程设到独占D根/target；外部probe离线依赖解析，不安装用户配置或改系统代理。

| 命令 | 实际结果 |
| --- | --- |
| 冻结worktree：`cargo test --manifest-path app/Cargo.toml --locked --test platform_owner -- --nocapture` | 独立完整编译成功；退出101，9 tests中2 passed/7 failed。正常spawn的7项均在同环境错误处失败，0.00秒测试运行；不能记Job树功能通过。 |
| 自有工程：`cargo test --manifest-path D:/Project/acp-remote/.tmp/platform-owner-independent/Cargo.toml --offline -- --nocapture` | 独立编译成功；退出101，3 tests中1 passed/2 failed，同一正常spawn错误。 |
| 自有诊断：`cargo run --manifest-path D:/Project/acp-remote/.tmp/platform-owner-independent/Cargo.toml --offline --bin env-probe` | 进程退出0仅表示诊断运行完成；输出合法hidden-drive键名`=C:`和空override正常spawn错误，不计功能测试通过。 |

日志使用cmd原生重定向以避免PowerShell将cargo普通stderr误判终止。开发manifest中的144+4+9和check/clippy只是其开发证据，不替代本次失败；没有复跑其余144/4以制造无关通过结果。

## 断言与未执行范围

已读9项开发测试源码：真实PowerShell后代且父先退出、stdin/out/err阻塞、takeIO一次、Drop树终止、stdin EOF、异常owner exit23与嵌套Job、argv/大小写环境回传、无关可继承event不得继承及breakaway失败。`fixture_process`普通父运行会直接返回，不能将它的PASS当实际异常/逃逸执行；这些路径本次均被PLAT-T001阻断，未到达真实子进程断言。

本次有效通过的低风险边界：开发invalid-spec/已过spawn deadline组，以及外部NUL/长度组（args、env值、env名、空env名、cwd/executable NUL、过长命令行），错误不回显固定SECRET值。但因正常继承环境在先被拒，有些外部无效输入也可能被更早环境错误挡住，因此不能把整个外部分支都归因于各自专用校验器已命中。

外部原拟补验过期总shutdown deadline、Incomplete保留owner与二次shutdown、取消wait/shutdown future后仍持有实际进程；均未到spawn成功，明确not-run。实际spawn取消中间窗口、AssignProcessToJobObject/ResumeThread失败注入、15秒全关闭预算亦未执行，不能凭源码推断通过。

本次失败发生在CreateProcessW之前，测试未获得或记录任何实际子PID，没有按名称kill、枚举后随意结束进程或触碰用户进程。后续真实进程测试必须继续使用测试生成并打开稳定同步handle的PID，不得用专用exe路径空列表替代PowerShell孙进程清理证据。

## 原始证据

| 自有日志 | SHA-256 |
| --- | --- |
| platform-tests.log | CAFB8FD99CB95EF13AC31673329BB6BFF64F78BE72DABE071EB4BE8E1EEADCED |
| external-tests.log | 68746FC22DFBA92EE3F874EE042F788DBC26430A0045E9EFD08ACC6E08E33D82 |
| env-diagnostic.log | 92C9B5369661E5ABD7297AB1D9C7B66EE3CADD5AFA7E2EA9A253A10101297AEC |

保留自有外部工程及日志；所有5候选文件前后摘要一致。PLAT-T001待原coder修复、新快照冻结后由本tester复测关闭；reviewer另行审unsafe/ownership，不由本报告代签。

## PLAT-T001 修复版独立复测（最新状态，保留上文旧失败）

2026-10-30：**PLAT-T001由原提出tester复测关闭。** 保留原cmd隐藏驱动器环境，既没有删除`=C:`也没有修改产品或测试预期。修复版11项平台测试、外部4项均实际通过；整体结论仅是下述已执行平台API范围的有限证据，不签完整产品/ACP/shared-instance/Harness/Web。

修复候选manifest为C平台worktree的`.tmp/platform-plat-t001/delivery.json`，SHA-256 `E5F3896FA6FC9E6A9A2EA7D589790717899069B612DF662244006279C434E54E`。5文件运行前后逐项匹配，独立清单`.tmp/platform-owner-independent/fixed-before.sha256`/`fixed-after.sha256`的SHA-256均为`9BE2A91AC717C424126D5993D1CD6C2456615B61BE6CDC2B588BD281579BB28A`。其中process_owner.rs为`17ABD5A4023874E0870B74947A23AF9529BB4DCC37A35781CA3133B1A98DC436`，platform_owner.rs为`BC24DF836BD805697B28B34244C9FB53034A274DA2D3315708729246E5588A25`；Cargo.toml及platform/mod.rs与原包相同，DEPENDENCIES为`8CF0CA9C009CCC799E56FD7F5823A5F02DEBBE717756381251E937E6097FAD28`。

源码区分继承条目与显式override，仅继承且精确`=<ASCII字母>:`形式可保留。显式`=C:`/`=c:`及其他空名、含等号键仍拒绝，不是普遍放宽非法环境变量。独立实际诊断仍输出同一键名`=C:`及hidden-drive=true，原空override的PowerShell正常spawn改为ok；只输出键名/分类、结果和本次PID2728，不输出环境值。新增开发回归实际验证隐藏原值能穿过嵌套OwnedProcess保留，显式override无法伪造。

| 独立命令（同一owner前置/自有target/TEMP） | 实际结果 |
| --- | --- |
| `cargo test --manifest-path app/Cargo.toml --locked --test platform_owner -- --nocapture` | 独立重新编译，退出0；11 passed/0 failed/0 ignored，1.29秒。原9项中的fixture_process普通空跑仍不计额外功能证明，其分支由对应父测试真实调用。 |
| 自有工程`cargo test --manifest-path D:/Project/acp-remote/.tmp/platform-owner-independent/Cargo.toml --offline --lib -- --nocapture` | 最终退出0；4 passed/0 failed/0 ignored，1.00秒。新增首poll取消probe最初漏导入Poll，编译失败后仅修正自有源码并重编译，无产品修改或运行失败被隐去。 |
| `cargo run ... --offline --bin env-probe` | 退出0，原环境最小复现正常spawn；诊断本身不算功能测试。 |

### 已执行平台边界

- 真实系统PowerShell后代：root先exit7时后代仍活，随后shutdown确保后代稳定同步handle已退出；重复shutdown保留Forced/active0/root7。Drop在IO已take时终止root及后代。不是只按专用exe路径查询空列表。
- 故意不读stdout/stderr、同时大块写stdin造成堵塞，shutdown终止Job并使writer任务有界退出。stdin EOF、取消pending read、root exit9及Graceful均实际通过。take_io第二次AlreadyTaken实际通过。
- 测试fixture在外层Job内另建OwnedProcess嵌套Job，owner用process::exit23绕开Rust析构；在外层Job仍开着时内层PowerShell稳定handle已退出。无关可继承event未进allowlist，breakaway真实尝试失败；这里只操作fixture自己创建的句柄和PID。
- argv空字符串/空格/引号/反斜杠/中文路径及大小写环境override roundtrip实际相等。原环境问题消除后，外部7类NUL/空名/命令行长度分支不再被先前hidden-drive错误遮挡，均InvalidSpec且不回显合成SECRET值。
- 外部记录本次自有PID（最终一轮）：40520、34400、45868、51892、32316、11272、31552、50292、48272、51260、45696。均从OwnedProcess.id或其专用stdout取得；在Drop/shutdown前打开同步handle，逐个等待确认退出。没有全局按名称kill。
- 外部取消wait_exit及shutdown future后，已记录进程仍活且owner保留；再次shutdown后active0并由稳定handle确认退出。过期总deadline调用立即返回（本次0ms），后二次shutdown继续可用，树PID51260/45696最终都退出。

### 仍未覆盖的分支，不冒充通过

过期shutdown本次实际返回`Forced, active_processes=0, root_exit_code=None`，没有观测到Incomplete分支；之后二次shutdown及句柄退出有实际证据，但不能说本轮“已测到Incomplete后再清理”。首poll取消probe8次实际全部同步Ready后Drop（8个owner PID均退出），`SPAWN_FIRST_POLL_CANCELLED=0`；因此真实spawn future处于Pending时的取消分支本次仍未触发。没有为命中分支改产品或用替身伪造。

AssignProcessToJobObject/ResumeThread系统调用失败注入、受限制嵌套Job失败路径、15秒全预算、完整ACP共享实例引用/原生关闭仍未执行，需后续平台故障/装配验收或明确报告限制。已执行普通嵌套Job成功和异常owner退出不能代替失败路径。unsafe/ownership另由独立reviewer审查，本tester不代签。

| 新自有日志 | SHA-256 |
| --- | --- |
| platform-fixed-tests.log | 62695712DFA2C2D569F6CC8A769EEAF010DD8AFDAAE48669B351010606A9C166 |
| external-fixed-final-tests.log | 8D7D9D8E21B424484456B3E9D32D1C67E74D2C037FD9B7B105B376B9BB2DDDBE |
| env-fixed-diagnostic.log | 5281C36C763E3E550FF30ABD724B15C732EEFD856977503E5626EF802A93FFA7 |

外部源码当前SHA-256 `4A98145BBE0CC54B2432E3799B4D6C161F2FA62570D4DFAAE19C797E40004045`；新测试.exit均0。测试专用acp-owner-test目录未残留，日志/外部工程保留自有根；没有清理用户进程/文件。5文件前后摘要完全一致。旧BLOCKED记录仍属旧版本；本节只关闭PLAT-T001，不把其余未执行边界或最终任务标完成。