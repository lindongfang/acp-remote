# NativeDispatch WIP02独立准备包窄审

结论：**本固定准备包的资产/执行证据可追溯，允许后续复用夹具源码和明确范围的历史自检证据；不签NativeDispatch最终交付或正式80项。** 15个准备函数确已通过，正1负6编译结果与目标诊断匹配。它们证明低层provider、真实observed ACP、真实SQLite迁移与合成Windows进程夹具路径，不证明完整factory、R3可靠授权、Loaded/control/SQL proof业务链。Root无需重复本固定WIP准备矩阵。

## 固定来源和实际核验

独立tester `implementation-v3/freeze.json`：`e0bb2501df81c1c86df97fddf44802a3efbd6f7aae16a7deb6a20edba5b2296c`，85成员；额外交接 `implementation-v3-handoff.md`：`41de08cb09d9d62fe8654d3395da4e6da2c8cba75f055bb1334c5a68a50fbf5a`。准备报告：`1178872a2b4021ca7bd971fa8ed374b5c75e9fbfd7f7d4bb25543e678c727b72`。

输入作者WIP02 freeze `8c3b0204eea0dbddc8308f91d566dd81503c258bfcf7d3e22ef035eec882ecc1`的328项，**不是正式candidate**。tester workspace-final-source.json：`f3934d9a7e74f5568f3364fa4bce646a20a55b9b2f9c3692cb722d5097acf38c`，340项；portable overlay：`e305c768b0497ef47754291924f7f6697ee849fc770a510fad1937d1a6ff7940`。

亲算85成员、作者WIP02实际328源、tester实际340源均匹配；对比仅3个原路径变化：Cargo.toml增加默认关闭fixture feature及required-features child bin，lib.rs/schema.rs增加cfg(test)挂载；另12份新测试/fixture/合成wire文件。除此325个WIP原路径相同，没有独立tester改产品行为。对作者固定WIP02 source实际只读apply --check exit0，没有apply或写作者/Root。

原cc00准备65成员、7ed550新版SM资产16成员、NDT v5的11成员及Root128保护输入亲算均原字节。原SM94755三文件/六常量由作者WIP输入已合入，本overlay未重复放宽旧断言。case-preparation-map的80个ID/标题与最终NDT v5实际提取逐项相同，全部formal_status=not-run。准备函数/编译探针不能据此计为80项部分完成。

## 实际15函数的范围

| 组 | 实际最终日志及结果 | 可证明与不能外推 |
| --- | --- | --- |
| provider | fixture_selfcheck-final：3/3、0失败/忽略 | 真AsyncRead/Write自检；无放行不接受，部分/Err0/Ok0与flush独立，分段LF/EOF及wake实际发生。不是产品授权或原生来源证明。 |
| observed ACP | observed_preparation-final：5/5 | 真prepare_observed/ObservedStart/Connection/Lease/Ticket/IoOwner；启动不开门零provider poll；实际held LF及未ack阻Prompt真实poll，真实无RPC native通知可写；foreign gate错误返原Reservation/Frame；本地取消Zero不发原生通知、Err0/flush错保持Unknown；实际owner.shutdown.joined。 |
| SQLite | schema_preparation-final：5/5 | 真实自有pin0..5经Storage到6；旧DDL/所有原行/身份key子集保持，重开全集保持；精确11表3索引14对象；真实私有step5独立提交及第二连接验5，再升6；旧5故障回滚、CommitUnknown第二连接见6；半布局/future77只读拒绝。不是全部仓储proof及逐14DDL故障覆盖。 |
| Windows/stdio | process_preparation-final-owner：2/2 | 一函数内两种NoScope路径（未take IO、take IO未prepare）：根exit0但后代仍活，真实Job Forced active0且独立OpenProcess句柄显示后代已退出；另一函数双版实际stdio init/resume/prompt、v2无RPC cancel通知及真实cancelled更新，IO/stderr/Job清理。不是实际Harness或完整Native授权链。 |

最终四组日志分别为`7f4ee299…`、`f659d9dc…`、`f65cdcdf…`、`fe6beb7c…`，均在85成员中核原字节；亲读实际test/result行，确为3+5+5+2，0忽略。日志实际为UTF-16LE，按BOM解码检视，未转换/重写原证据。WIP未使用字段/缓存警告仍保留，准备通过不等于strict all-target lint或完整工程检查通过。

controlled_io只控制实际provider接受字节、Pending、Err与flush动作，不返回伪WriteReport。Scope/Received/Lease来自实际ACP，初始化RPC取原Reservation.id，未构造假Scope/Ready/Loaded。Forward fence转同PollIo一次，仅用于低层协议准备，不声称已验证R3控制授权/每poll业务范围。held LF使用真实AfterReadCapturedBeforeRoute钩子并看实际writer计数，不拿feed/peer.write当reader已经capture。

进程peer使用真实stdio、实际LF和原typed ID捕获；合成脚本的Value只解析自有有限内容，不证明未知大数/原始词法全保真。v2脚本显式要求cancel无id，实际测试亦核原Reservation.id=None；本地Ticket.request_cancel和真session/cancel分开。后代PID marker与spawn记录相符后才OpenProcess持独立句柄，259→非259与真实shutdown.active_processes=0共同核收敛；marker本身、根exit或Drop都不当Job0。两种NoScope分支没有伪IOjoin/Scope证据；合成单层后代不外推真实Harness/任意多层树验收。

## 负编译及失败历史

亲读runner、7份源码和最终原始compiler JSON：先正例build-finished=true、exit0；六反例均exit101、build-finished=false，主span确在目标源码：control作Prompt为E0277；收据字段构造E0451；from_actual私有E0624；收据Clone E0277；SessionBinding.scope私有E0616；NativeCancel添rpc字段E0559。不是错误import或“产品没编译”造成的假负例。正例只是泛型只读API合法使用，**没有动态构造控制授权或factory正链**。

runner只在独占workspace/app/examples精确新建当前一个probe，校验内容SHA后删除；当前无nd_临时文件遗留。负源码归档于外部compile-probes，不挂正常all-target。正式候选仍须重跑实际相同imports正对照和逐错误原因，不能复用旧stderr就签新visibility。

失败历史原样保留且分类正确：

- 最初sandbox运行1函数在`open stdio pipe` Windows5失败；实际process_owner先建管道后CreateProcess，未到child/Job断言。不能把它归为已启动泄漏，亦无证据认定和旧owner故障同因。
- 交接记录相同首版已编译目标在escalated、无owner脚本时即1/1通过；后来两函数版在owner脚本+escalated最终2/2通过。两运行条件不同，不能把首红→首绿单归因于owner脚本；最终成功也不证明默认sandbox相同权限可运行。
- 新stdio夹具首编译E0308是String传给Known<Arc<str>>；只在自有资产补.into()，最终编译0。未改产品/预期，不冒称Native产品缺陷已修。

现有进程测试成功路径明确join；未来扩大正式故障矩阵时仍须逐失败/超时/取消验证真实清理，不能以assert panic触发最佳努力Drop替正式cleanup证据。自有Area只在独占workspace.tmp新child内创建、canonical父子核对后删除，不涉及用户目录。本reviewer本轮未运行进程或删除文件。

## 仍然打开的覆盖边界与复用条件

PREP-HOOK001保持：WIP02 NativeDdl在每个循环使用同fault标记，首次即命中第1条；成功路径确实执行并比对14定义，但失败路径不能据此称14条逐项回滚已测。正式candidate须提供test-only ordinal或等价真实SQL入口并实测每个点、坏14对象矩阵及完整proof往返。

准备SQL的DeferredCommit实际走真实deferred FK插入/COMMIT代码，不是CommitUnknown或简单返回Err的替身；但本新资产断言主要是is_err+全snapshot/版本回滚，没有单独保留该次787错误码断言。正式故障类型验收应保精确数据库错误类别/码及到达点，而不能仅从函数名或静态代码推导所有失败原因。CommitUnknown则已用独立连接验证真实提交6，不能把Err称回滚。

可复用：这些夹具源码/离线SDK/合成脚本、固定80定义和本次明确WIP自检历史。最终源冻结后须重新核接口、3挂载与全部新资产差异，走实际factory/合法Pre→Bound→Loaded及原R3 D/E、控制intent/receipt/Zero重试/必要仓储/关闭链，再在**同版最终候选**执行对应80全部原oracle和适用回归。低层15绿不替代无target unsupported、unknown权限不自动否决、source/config/共享目标/CallerDrop、每poll因果围栏与传输/前台双轴。Root无须现在再跑这份WIP准备；新候选的正式结果另记，不回填本历史。

## 补充：306 sparse输入范围更正

本轮另亲核Root新增`2026-11-01-q3-additional-baseline-inputs.json` SHA `213122282554af634cef2c12b95f12ce0b9b30e60c19487dcf96871d3e9726f2`：15原件及15字节快照都匹配；对15路径实际`git diff --quiet HEAD -- ...` exit0，HEAD为65ec7815bb0280f4519989d93a7b4ed3b4b37994。它们包括2个旧Rust examples、8个Web测试TS输入，以及browser HTML/index/tsconfig/vite/DEPENDENCIES等原输入。

旧Root完整npm check是在完整checkout实际运行，既有结果不因清单遗漏自动失效；但306 sparse复制**没有冻结全部Web测试/配置或Rust examples**，旧限定目录rg检查不能外推为完整app/Web所有构建输入均在306。本轮准备只按原328/340范围核验，不事后修改旧WIP/历史gate或冒称15项当时已冻结；这些遗漏与当前目标--lib/指定probe的准备结果分开。

未来Native/Shared最终baseline、workspace及source/build-inputs manifest须纳入这15项原相对路径/字节，并对完整app、clients/web相关输入做库存；至少306+15为321项原基线，再加实际增量。正式all-target/Web/build从该完整副本执行并保前后hash。不能复制丢失测试的sparse树后以“测试全过”代Root完整入口，也无需因此无目的重跑已固定库准备矩阵。此为证据范围修正，不是新产品规则或Native实现缺陷。

本报告不批准将准备overlay直接合入Root，不签最终Native source、正式80、Node/WSS/Web/真实Harness或整体目标；后续固定candidate交接到达后再执行原同源审查与独立验收流程。
