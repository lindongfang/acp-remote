# 环境继承修复：独立限域验收

## 结论

指定测试全部通过：process_owner 13/13、native_release 7/7、platform_owner 13/13；另独立运行 raw hidden fixture 1/1。合计 33 个不同顶层测试（其中 fixture_process 为无选择器时空操作），另重复一项关键 fixture。四组均退出码 0、无忽略测试。这是环境继承修复的独立限域结果，不是完整 WP4 或整个 change 验收；不修改任何任务复选框，不代其他角色关闭问题。

## 固定版本

候选：`testing/runs/deliver-codex-session-loop/2026-11-13-environment-inheritance-fix-v1/source`。`INPUTS.json` 保存清单文件 `SOURCE-FINAL-v2.json` 的自身 SHA-256、全部实际文件的 path/bytes/SHA-256、运行前 PE 哈希和 Git index 哈希。

独立对 514 项逐叶重算并核对字节数，514/514 全部吻合；实际目录递归文件数也为 514。相对 BASELINE 恰有三处变化：`app/src/platform/process_owner.rs`、新增 `app/src/platform/process_owner/environment_tests.rs`、`app/tests/platform_owner.rs`。基线无删除。

清单声明聚合摘要为 `243e288d6d79b31686c34aa618c5d7de645a9d89b9aad0fc25b9f897aa68daf4`。本轮不声称重现该声明值。曾按清单顺序试算 UTF-8 无 BOM 的 path + NUL + 小写实际 SHA-256 + LF，得到 `46beb34a4d8e67e28edf7fc83424cb9723b852b7c5f97131a89522b5fe6e1368`；主执行者随后撤回该拼接规则，故仅作为非规范探算记录，不作为不匹配缺陷或规范版本锚。客观版本锚为清单自身摘要与全部逐叶吻合。

主执行者后续新增非覆盖身份补件，原清单保持不变。已独立确认清单自身 SHA-256 为 `f1f96636ed110b6786523ccd2f7a0ff50f2ebb442c5465fb5e6b305308beb90a`。本验收基于独立算出的实际 path/bytes/hash，将 path 按 StringComparer.Ordinal 升序，每行 path + TAB + InvariantCulture 十进制 bytes + TAB + 小写 SHA-256 + LF，UTF-8 无 BOM 且保留末尾 LF，生成 `CANONICAL-INDEPENDENT.tsv`，其 SHA-256 为 `3d1a45f714b38e91d83912ae958093fc36cdc7d3843cfed66dedfbdc2029bff1`，与身份补件的 CANONICAL-IDENTITY.tsv 一致。此 canonical 值和原清单文件自身摘要可作为已复算的明确版本锚；不追认旧 243e 声明。

三个运行前 PE 哈希均与 FINAL-VALIDATION 一致；运行后逐项未改变：

| PE | SHA-256 |
| --- | --- |
| acp_remote-e75360741b1915fc.exe | `ad37f392c6c76cf804d7433cade09e2dacb76b1ca68c0b875f059573fa56170c` |
| platform_owner-b7dedd8b2e7b6a61.exe | `f97e1806467952d17c828d9c6c9b45e2b710ac45a52a37b691c3d989f83e5b40` |
| native_dispatch_dev_child.exe | `2bcf2efe3eb8bd1b95e42cb6b821cca208b0a2fb0fac74a0e904d898654ef978` |

Git index 前后均为 `ac76c7ad84c1d4333fc64a710647e60aeb96015eb4407f996a4bf6adc91fbff4`。候选源码、产品、预期、已有结果及 index 均未被本验收修改；新写入仅位于本独占输出目录。

## 环境、命令与结果

Windows `10.0.26300.0`；Windows PowerShell `5.1.26100.9818`。实际调用：

```powershell
powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File testing/runs/deliver-codex-session-loop/2026-11-13-environment-independent-v1/run.ps1
```

脚本同 shell dot-source `testing/ci/windows-owner.ps1`，实际所有者探针通过；随后直接运行既有 PE，没有构建、没有调用 Cargo、未占用或写入 target。工作目录为固定候选 `source/app`。四组分别自建 TEMP/TMP、P1_DATA_ROOT、RPC_FIXTURE_ROOT，并设置给定的 NATIVE_DISPATCH_DEV_CHILD。所有测试均 `--test-threads=1 --nocapture`，没有改动测试期望。

| 组 / 选择器 | 实际结果 | 时间（+08:00） | 证据 |
| --- | --- | --- | --- |
| process-owner / platform::process_owner | 13 passed / 0 failed，exit 0 | 09:42:11–09:42:12 | process-owner/RESULT.json、stdout.txt、stderr.txt |
| native-release / native_release | 7 passed / 0 failed，exit 0 | 09:42:12–09:42:14 | native-release/RESULT.json、stdout.txt、stderr.txt |
| platform-owner / 全测试 | 13 passed / 0 failed，exit 0 | 09:42:14–09:42:15 | platform-owner/RESULT.json、stdout.txt、stderr.txt |
| raw-hidden-focus / --exact raw_hidden_environment_reaches_tracked_child_and_closes_original_tree | 1 passed / 0 failed，exit 0 | 09:42:16 | raw-hidden-focus/RESULT.json、stdout.txt、stderr.txt |

每个原始 stdout 保留逐测试名字与结果。process-owner 的 stderr 是被 catch_unwind 捕获的四个 `synthetic exact mutex poison`，与实际测试源码的故障注入一致，不是未处理的测试失败。native-release stderr 含测试诊断，记录 joined=true、metadata_confirmed=true、physical_owner WindowsJob active_processes:0；platform-owner 与 raw-hidden-focus stderr 为空。Windows PowerShell 将原生 stderr 包装为 NativeCommandError 文本，判断采用实际退出码及 Rust 测试结果，而不把包装文本当失败。没有读取或写出未知环境变量值。

## 断言核对与实际 Job 证据

依据：harness-management 的“默认继承节点环境”、FOLLOWUP-20261113-ENVIRONMENT 的批准范围；本会话自行读取候选产品和真实测试，不以开发自评代签。

- 三个 environment_tests：核对 `=::`、drive、ExitCode、合成未知隐藏名；ordinal-ignore-case 去重/显式覆盖、排序，值带等号/空值，原始 UTF-16（含未配对代理单元），双零终止。显式隐藏名、空后缀、第二个等号、普通中间等号、嵌入 NUL、名称长度 32766/32767 边界明确断言。畸形名仅为 helper 受控输入，不宣称 OS 实际枚举过畸形名。
- 新 raw_hidden 测试使用直接 CreateProcessW 原始 Unicode 环境块注入 `=::`、未知隐藏名及混合大小写 ExitCode；先创建挂起的父 fixture，分配专属 KILL_ON_JOB_CLOSE Job，再恢复。父 fixture 实际枚举 vars_os 并核对常量，使用 prepare_tracked 启动子 fixture；子再次核对常量并写入固定 marker。tracked owner 的真实 shutdown 必须返回 AssignedJob，且 active_processes=0、root_exit_code=Some(0)、disposition=Graceful；外层检查父进程退出码 0 和 marker。这些断言在完整 platform-owner 与专项各真实执行一次。
- Job/树回归不仅看 root：parent_exit 测试先确认 root 已退出但后代句柄仍存活，再 shutdown 并等待后代句柄 signaled，断言 active_processes=0；nested Job 异常 owner 退出测试持有稳定后代句柄并确认其结束；drop、阻塞 I/O、breakaway/句柄继承等原有测试均通过。tracked 的十项 poison/disposal 回归核对实际句柄清单与原始所有者证据。
- native_release 七项包含原先失败的 view.opened 完整服务路由测试、注册 durable actor/generation 事实、SQLite 重开及幂等/冲突/故障重试、schema9→10 与提交前回滚和提交报告丢失。服务测试没有修改 opened 断言；此次越过该点完成全测试，实际诊断确认物理 Job 活跃进程数 0。

## 限制与问题归属

1. 新 raw_hidden fixture 的实际链是受控父 fixture → tracked 子 fixture。该子不再创建第三层孙进程；它证明原始 tracked Job 的真实零活跃关闭，不独立证明隐藏环境在任意深度派生树中的传播。更深层后代关闭由既有非隐藏环境树测试提供，不能混为同一个场景。
2. 外层 RawFixtureJob 在成功路径会 Drop/TerminateJobObject，但测试没有在 Drop 后再查询该外层 Job 的活跃数；它只等待父 fixture 句柄结束。关键 tracked 原始 Job 的零活跃断言则实际执行。测试后 Get-Process 按 fixture PE 名没有匹配进程；额外 Win32_Process/CIM 全进程查询因权限被拒绝，不能把该查询记为全机无残留的证明。
3. 本次复用既有最终 PE，不独立重编，因此只通过逐叶清单与开发 FINAL-VALIDATION 的 PE 哈希绑定，不能声称独立证明源码到二进制的可复现构建关系。旧实现 Red Check 未重跑，本轮不独立验证其失败，只执行新候选回归。
4. 未运行全库、完整 E2E、真实外部 Harness、npm audit、安全专项或供应链验收；这些不能由本报告替代。FR-002 typed socket 错误映射继续 open；FR-001 由其原独立提出者阅读本证据后自行决定，本测试者不关闭他人问题。
5. 输入源码在运行前逐叶核对，PE 在运行前后核对；本轮未生成或修改产品源码。大范围 Git 状态读取遇到旧历史数据目录权限警告，与新验收目录无关。

运行脚本最初将 binariesUnchanged 用未加括号的 PowerShell -join 表达式求值，输出了错误字符串 False；读回立即发现，以三个逐项前后哈希核对修正为布尔 true。原 stdout/stderr 与各组 RESULT 未改。聚合探算首次使用当前 .NET 不支持的 Convert.ToHexString 而失败，后用 BitConverter 计算；该探算仍明确不具规范效力。辅助脚本/汇总纠错均未重跑或改变测试事实。

`RESULT.json` 为机器可读汇总，`INPUTS.json` 为 514 文件和 PE 锚；`SHA256SUMS.json` 固定本目录报告、脚本、结果及原始日志文件哈希（自身不纳入）。
