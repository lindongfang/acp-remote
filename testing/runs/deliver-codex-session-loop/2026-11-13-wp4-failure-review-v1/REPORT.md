# WP4 native release 失败独立复核

日期：2026-11-13。角色：独立 reviewer。结论：**阻断仍存在；已把当前失败定位到进程创建前的继承环境名称校验，具体被拒绝名称尚未确认。** 本次只读产品和测试定义，未构建、未运行测试、未改 Git 索引、未关闭其他人的 finding。写入范围仅为本报告目录。

## 版本与证据边界

- Git HEAD：`65ec7815bb0280f4519989d93a7b4ed3b4b37994`。Root 存在大量已有未提交修改，记录于 `ROOT-TRACKED-STATUS.txt`，不能把 HEAD 当作本次完整受审版本。
- 固定源码清单：`../2026-11-13-wp4-owner-rerun-v2/SOURCE.json`，513 个文件，摘要重新计算为 `2309968f6a85b92d9fa1cb5691ab9a75adc958301bc061446175724b8d5d306f`，与声明一致；逐文件核对无缺失。
- 审查时副本有且只有两处与原清单不同：`app/src/application/native_dispatch/read_source.rs` 和 `app/src/platform/native_source.rs`。移除各自单条固定类型诊断后，内存中重算的字节哈希均恢复原清单值。详见 `DIAGNOSTIC-COMPARISON.json`。未把这些诊断改动写回 Root。
- 已读取的 52 份规则、设计、源码和日志的实际 SHA-256 固定在 `REVIEW-INPUTS.json`。它也记录上述两处变化前后哈希、清单重算结果、Git 索引摘要。
- `SOURCE-AFTER.json` 是原始运行结束时的历史快照，记录当时没有变化；它不能描述后续诊断副本。现有 `DIAGNOSTIC-DELTA.json` 仅说明第一处诊断。本报告额外固定第二处诊断的实际差异。
- 本人未执行构建或 native release 测试。以下动态结论来自实际读取主执行保存的 stdout、stderr、RESULT 和 ENVIRONMENT，属于**复核原始证据**，不是“当前独立运行通过”。

## 已确认的失败链

1. 原始 `native-release.stdout.txt` 显示 `native_release` 过滤器实际执行 7 项，6 passed、1 failed，exit 101。唯一失败为 `session_management_routes_approval_and_native_release_services`，`queue_read_development_tests.rs:3981` 期待 `view.opened`，收到 `result/error.code=protocol_failure`。durable actor 绑定登记测试通过；后续 release 执行与关闭断言尚未到达，不能说此场景 release 部分已验证。
2. 同一原始 PE 在 `runtime-unrestricted` 的另一真实用户身份、对应 temp owner 下重跑仍为 6/7、同一失败。ENVIRONMENT 固定 test PE SHA-256 为 `9be66adfa6046b91453536ae5dd24d1a14b35aa56f3ec10ebfd84db6e48b3203`。这说明仅换身份没有消除失败；它不等价于独立安全审查通过。
3. 第一处隔离诊断 `runtime-diagnostic/native-release.stderr.txt` 记录 `native_read_failure=InvalidConfiguration`。
4. 第二处隔离诊断 `runtime-diagnostic2/native-release.stderr.txt` 进一步记录 `prepare_tracked_failure=InvalidSpec("invalid environment name")`，紧接同一个 `InvalidConfiguration` 和 `view.open` 失败。该诊断运行 PE 哈希为 `f6367ae74e2ee1b6986e437f4c5b7b7d46124b3a0feb9ba53a064a994cc27fdc`，不能冒称它是原始 PE 的执行。
5. 调用链为：`node/native_views.rs:208–212` 获取异步读取 → `read_source.rs:288` → `source.rs:94–107` 解析读取配置并确保来源 → `platform/native_source.rs:210` 调用 `OwnedProcess::prepare_tracked` → `process_owner/tracked.rs:540` 构造环境 → `process_owner.rs:524–529` 名称校验失败 → `native_source.rs:137` 转为 `DispatchError::InvalidConfiguration` → `read_source.rs:203` 保留为 `ViewError::NativeFailure(error)` → `management_ws/session_commands.rs:475` 对所有 NativeFailure 返回 `protocol_failure`。
6. 这里还没有调用 CreateProcess、设置子进程 Job、恢复子线程或发起 ACP 初始化。`NeverEntered` 与该链一致。不要修改 ACL、放宽管道 owner 校验、重试降版或延长等待来处理本次已定位故障。
7. 早先清理日志的 `view_failure=SourceReleased` 不能单独作为原始失败原因。`view_attachment.rs:87` 对 `AttachmentState::NotInstalled` 就返回 SourceReleased；环境准备失败使视图从未安装，也会出现它。

## Findings

### WP4-FR-001 — P1 / open：继承环境名称校验阻止合法配置进入原生启动

位置：固定副本 `app/src/platform/process_owner.rs:513–579`；调用点 `app/src/platform/process_owner/tracked.rs:530–541`。

已确认：第二处诊断将本次失败定位到名称校验。该分支拒绝空名、超长名称，或者名称含 `=` 且不满足继承保留名例外。`queue_read_development_tests.rs:3503–3511` 生成的 profile 明确 `environment:[]`；所以该场景的触发输入来自继承环境，不能归咎于用户显式环境覆盖。规格 `harness-management/spec.md:158–163` 要求默认继承、允许显式覆盖，不增加环境白名单作为基础准入，并禁止诊断泄漏凭据值。

代码中的具体缺口：`inherited_reserved_name` 对 `=ExitCode`、`=ExitCodeAscii` 使用逐 UTF-16 单元的精确大小写比较，但同一模块普通名称去重和排序调用 `CompareStringOrdinal(ignoreCase=1)`；驱动器伪变量另有大小写处理。**标准保留名大小写变体是可解释当前现象的候选，还不是已确认的实际拒绝项。** 其他继承项、异常长度仍需同进程上下文分类排除。

最小下一步：在失败测试 PE 的同一环境构造边界，仅记录拒绝类别（空/超长/含等号、是否 inherited、是否与两个固定保留名 ordinal-ignore-case 相等）及必要长度；不记录名字、值、完整环境块、凭据或会话 ID。仅探父 PowerShell/.NET 环境不足以确认 Rust 子进程实际枚举结果。

主执行提出的最小方案评估：**条件通过，前提是分类确认命中两个标准保留名的大小写变体。** 将 `inherited_reserved_name` 改为返回 `Result<bool, ProcessOwnerError>`，对两个常量复用已有 fallible `compare_names`，调用点用 `?` 传播 Windows 比较错误；保留 `inherited && ...` 语义，使显式保留名覆盖仍拒绝。原始 UTF-16 名称和值继续保留，既不 Unicode 大写归一化，也不对比较失败给默认答案。该范围不改变权限策略或产品准入语义。若探针未命中，先定位真实结构，再定修复；不要扩大为接受所有未知 `=` 名称，也不要静默删除未知继承项。

最低回归范围：

- 旧代码 Red Check、新代码 Green Check，覆盖标准名、大小写变体、ordinal 相等的同名覆盖/去重与值保留。建议对抽出的纯环境构造输入测试，不在并发 Rust 测试中修改全局进程环境。
- 真实受控父进程为子 fixture 建立继承环境，经过 **prepare_tracked / tracked 真实启动路径**，验证进入运行、继承值确实到达子进程、完整关闭。现有 `app/tests/platform_owner.rs:540–589` 只以标准拼写验证继承，并使用 `OwnedProcess::spawn`；不能单独覆盖此次 tracked 路径。
- 保留 `platform_owner.rs:509–537` 对显式 `=C:`、`=ExitCode`、大小写变体、`A=B`、空名的拒绝与无敏感值错误检查；新增 malformed 继承项边界若设计允许需先明确依据。
- 运行原失败 `session_management_routes_approval_and_native_release_services` 的完整剩余流程，再运行全部 `native_release` 7 项及相关原生视图/进程 owner 检查。不得修改 `view.opened` 预期。按新源码清单和 PE 哈希保存新证据，由独立 tester/reviewer 复查。

### WP4-FR-002 — P2 / open：所有 NativeFailure 对外归成 protocol_failure，丢失错误类别

位置：固定副本 `app/src/management_ws/session_commands.rs:462–492`，特别是 :475；对照 `app/src/node/native_views.rs:173–183` 与 `app/src/application/native_dispatch/read_source/cause_tests.rs:28–46`。

证据：同一实际错误已保留为 `NativeFailure(InvalidConfiguration)`，但是外部响应只剩 `protocol_failure`。同步 admission 分支会把 InvalidConfiguration 映射为已有的 `ViewError::InvalidConfiguration`，再输出 `invalid_configuration`；异步完成分支却因包裹型错误走全覆盖分支。这也让 stopped、存储和平台错误被错误暗示为 ACP 协议故障。现有 cause 单测只断言内部原值保留，没有验证 socket 错误码。

建议：内部继续保留 typed 原因；在统一出口按稳定 DispatchError 类别映射现有公开码，让同步和异步错误保持一致。先核对相应管理合同，新增类别按合同明确处理；协议错误才返回协议类别，绝不把静态 operation/code 或敏感内容直接塞入 Web 响应。补表驱动出口检查，至少覆盖 InvalidConfiguration、Stopped、NodeStopping、InvalidTarget/Source/Scope、ProtocolFault、SourcePlatform 和存储失败。此修复只改错误呈现，**不能使本次 view.open 成功，也不能替代 FR-001 的启动修复**。本 finding 保持 open，未代表任何既有 finding 的提出者关闭问题。

## 验证状态与限制

| 项目 | 状态 | 含义 |
| --- | --- | --- |
| 规则、设计与关键调用链人工复核 | passed | 已读取指定四份规则，核对相应继承环境规格及实际代码 |
| 513 文件固定清单摘要与当前副本差异核对 | passed | 清单摘要一致；仅两处固定类型诊断差异，无遗漏文件 |
| 两处诊断移除后原哈希复原 | passed | 只在内存重算；未写回源码 |
| 原始 native_release 执行证据复核 | failed | 主执行保存的结果为 6/7，不是本人执行 |
| unrestricted 同 PE 运行证据复核 | failed | 换真实身份仍在同一 view.open 断言失败 |
| 诊断运行证据复核 | failed | 明确命中 invalid environment name，未确认具体被拒绝项 |
| 当前修复编译、测试、Red/Green、新版独立验收 | not-run | 本次授权仅只读诊断；目前无受审修复版本 |
| 当前真实 Codex Harness / 完整 E2E / npm audit | not-run | 本次范围未执行；不得把历史日志计为当前通过 |

原始 `pipe-owner.stdout.txt` 为 1/1 passed，证明那一个真实管道 owner 检查在该记录中通过；它不覆盖继承环境构造。WSS 登记的通过由主执行另行汇总，本报告未读取其独立原始证据，故不为其新增通过签字。

审查结论到此只确认失败边界、两项 open findings 和有条件可执行的最小修复方案。阻断未关闭，WP4 不应因此标记完成。
