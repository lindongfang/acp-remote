# FR-002 独立限域验收报告

日期：2026-11-13。结论：冻结候选的服务端 NativeFailure 错误分类及本轮适用回归通过。真实执行为专项 4/4、session_commands 6/6、native_release 7/7，测试子进程均 exit 0；合计17次测试执行、13个不同顶层测试（四项专项在 session_commands 中重复执行）。本报告不签完整 WP4，不代原 reviewer 关闭 FR002。

## 版本与范围

- 固定源：2026-11-13-native-view-error-mapping-v1/source；SOURCE-GREEN.json 自身 SHA-256：`22c7f058f416aca2d48680d49387c5619bdf050ceade940f7c18135198a16472`。
- 独立运行前后逐叶 path/bytes/SHA-256 验证均515/515，实际文件树也是515，无缺叶、额外叶或删除；canonical：`5f076a551d81c3b3ed28b18a0c6bc3d66772969170178962ec3b745c9aaf9942`。算法为 StringComparer.Ordinal 排序，path + TAB + InvariantCulture十进制bytes + TAB + lowercase SHA-256 + LF，UTF-8无BOM并保留尾LF。
- 对 BASELINE.json（文件SHA f1f96636ed110b6786523ccd2f7a0ff50f2ebb442c5465fb5e6b305308beb90a）的变化只有 session_commands.rs 和新增 session_commands/view_error_development.rs。
- 冻结 lib PE：`d06e61c8839492fbc539fc8165f8a0fbb66d1fe63745b54970242b81721d0a11`（66087424 bytes）；dev child：`2bcf2efe3eb8bd1b95e42cb6b821cca208b0a2fb0fac74a0e904d898654ef978`（690688 bytes）。运行前后相同，均吻合冻结通知与 DEVELOPMENT-RESULT；未构建、未调用 Cargo、未占共享 target。源码到PE关系按冻结清单和开发构建记录绑定，没有独立可复现构建声明。
- Git index 前后 SHA：`ac76c7ad84c1d4333fc64a710647e60aeb96015eb4407f996a4bf6adc91fbff4`；未改产品、用例预期、候选或已有结果。PREPARATION 的预读状态保留原样，正式状态由本报告补充。

## 实际执行

独立脚本以 Windows PowerShell 5.1.26100.9818、-NoProfile -ExecutionPolicy Bypass 启动，OS Microsoft Windows NT 10.0.26300.0。自有 TEMP/TMP、P1_DATA_ROOT、RPC_FIXTURE_ROOT 都在 execution-v1 下；冻结 NATIVE_DISPATCH_DEV_CHILD 指向 artifacts。启动测试的同一个 shell dot-source testing/ci/windows-owner.ps1，实际 owner probe 通过。测试工作目录是冻结 source/app，直接调用 artifacts/acp_remote-e75360741b1915fc.exe。

每个 selector 先 --list，随后 --test-threads=1 --nocapture：

| selector | 实际通过 | 退出码 | 原始日志 |
| --- | ---: | ---: | --- |
| management_ws::session_commands::view_error_development | 4/4 | 0 | execution-v1/view-error.run.stdout.txt |
| management_ws::session_commands | 6/6 | 0 | execution-v1/session-command.run.stdout.txt |
| native_release | 7/7 | 0 | execution-v1/native-release.run.stdout.txt / stderr.txt |

真实执行窗口：2026-11-13T10:07:25.4841915+08:00 至 10:07:27.3330724+08:00；三次 list 和三次 run 的 PID、起止时间、参数、退出码与逐测试名称见 RESULT.json。名单与通过行、test result计数均独立重新核对，无零用例或忽略用例冒充。

## 逐项判定

| 独立用例 | 结果与证据含义 |
| --- | --- |
| FR002-AC-001 | passed。29种 DispatchError 与生产 helper 显式 match 一致，无裸 _ 兜底；30项非 ProtocolFault 表输入（28种+SqlClose和SourcePlatform各多一项），通过真实 view_error_code(ViewError::NativeFailure(...))。Stopped→source_released，InvalidResource/StaleSelection/wrapped SequenceExhausted→read_failed；配置/目标/来源/范围/节点停止/长度固定码保留。 |
| FR002-AC-002 | passed。17种 NativeProtocolFailure 全部实际走 ProtocolFault 包装，含Io/Allocation，均 protocol_failure。 |
| FR002-AC-003 | passed。30个直接 ViewError 固定码通过；同冻结 BASELINE 对比，除 NativeFailure 调用外的 mapper 完全相同，view_failure 认证关闭函数逐字相同。纯 mapper 的认证错误测试不代表异步认证关闭已动态覆盖。 |
| FR002-AC-004 | passed。5组完整 serde_json 序列化/反序列化 Value 等值断言，包含InvalidConfiguration、两个不同 SourcePlatform operation/code 和SqlClose false/true；仅既有v/type/requestId/ok/result/error且error仅code，故payload及敏感sentinel不在公开JSON。 |
| FR002-AC-005 | passed。6项命令与7项释放回归完整达到末尾；读取实际路由用例，view.opened、actor绑定、幂等/冲突、结果存储故障→releasing→重试released和状态一致均为真实断言。 |

四项专项每套表内共82次等值检查（30+17+30+5），与顶层测试数分开，不称82个测试。两套 SourcePlatform 映射和序列化 payload 取值不同；bool两态在两层均覆盖。

## Red与开发证据的复用边界

只读复核既有 red-v2/RESULT、stdout、stderr、SOURCE-RED-V2与RED-V2-RECEIPT，未独立重跑Red。Red/Green测试叶字节相同。旧实现实际4项中2通过2失败、exit101；category 与 envelope 均在首项 InvalidConfiguration 处看到 protocol_failure 而预期 invalid_configuration；后续payload未在Red遍历，不作相反声明。开发Green/fmt真实日志只作同版本参考，不能替代本轮独立PE实跑。

## 关闭证据与限制

本次 native-release stderr 中实际诊断为 joined=true、metadata_confirmed=true；有源路径记录 WindowsJob active_processes:0、metadata_close:Ok、view_failure:None，测试临时area first_remove 均 removed。运行后独立 temp/p1-data/rpc-data 无残留文件；Get-Process未发现native_dispatch_dev_child。以上只证明本次fixture及其诊断到达的关闭状态。CIM Win32_Process枚举返回0x80041003拒绝访问，因此没有独立的完整Job后代清单；不以单个child消失替代完整进程树实证。

专项是服务端内存mapper及JSON序列化测试，路由回归也不是把每种错误注入真实WebSocket/浏览器。Web ManagementErrorCode/knownManagementError闭集仍缺服务端既有码（含invalid_configuration/invalid_source/invalid_scope/source_released/length_unrepresentable/protocol_failure）；完整Web错误呈现仍有既有合同缺口。本轮不改Web，不放开任意字符串。

未运行：重编/可复现构建、全库Rust、strict clippy、专门认证关闭故障、浏览器/Web闭集、真实外部Harness、npm audit。前两类构建/fmt证据仅复核开发记录；与本轮限域判据不同，不签完整WP4、不勾tracked任务、不代reviewer关闭finding。

## 执行包装错误与交付

所有测试通过、前后身份落盘后，独立PowerShell包装脚本最后汇总报OrderedDictionary/Measure-Object属性兼容错误，包装exit1。原execution-v1/RESULT.json保持running且日志不改；随后finalize-independent.mjs仅从既有日志与退出码重算，形成顶层RESULT.json，未重跑测试。详见EXECUTION-ERROR.md。该错误属于证据汇总工具，不是受测程序失败；保留原始记录以便复核。

主要文件：REPORT.md、RESULT.json、INPUTS.json、COVERAGE.json、EXECUTION-ERROR.md；原始证据在execution-v1；FILES-SHA256.json/tsv固定本目录所有交付证据（不把哈希清单自身纳入，避免循环）。
