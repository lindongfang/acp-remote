# 管理输入在主区schema4组合的独立复核

2026-10-31，reviewer `/root/baseline_reviewer`。只读已合入源码、独立原始证据及外部revision3冻结，未修改产品/测试或执行真实Harness。

结论：**主区schema4与管理输入组合的约定独立L3/L4子范围限定通过。** 这是一份新的实际集成结论，不将外部schema3通过直接套用为主区4。仍在运行/收尾的全工程完整入口由主执行另行记录，本报告不提前签署；实际认证、资源/重放发行者、原生writer、审批owner、WSS/Web与L5/L6均未签。

## 独立读取与版本核对

- 实际读取request_tester `management-root4-acceptance.md`，重新计算SHA256 `104922b7ac348440207107270651c727fdabfcf23718fa60ebe87e9bcdd03aac`。
- 已读initial/final来源清单、source-changes、normalized-compare、三份声明/Storage/overlay全行diff，以及test-adaptation完整diff。reviewer对当前终态78文件再逐一计算，全部匹配final；源变化仅独立资产等价风格修正和测试声明排序，无其他生产逻辑变动。
- reviewer亦自行从外部revision3的11候选源读原字符串，以CRLF→LF统一行尾、仅去掉明确新增views声明/把Storage4还原3/去掉3行cfg(test)include，逐一严格比较，11份全部相等。没有用广泛空白/注释归一化掩盖逻辑变化。
- 当前重点生产/组合摘要：Storage `d4bf0e624ffa20b9b73cc167f2a346557f02a23dd0a54a240cfb4caf51ef7bb4`（保留4），SQL模块含测试overlay `e4d1f446e7e3f1ac4f2496f0b2d7511298a57d29b5d2cb087e742900c8c27333`，schema `fcec6f7e929056bdf7809534c7f9566c7bbe77f5ebb0c2a47be1a00e06564d18`、product_schema `e3d2d32c0e3ec32f1c6ef4ed4a971f31c4f203bd8ca6d67063f81b4f14aa6c00`。锁未改。

## 实际执行证据

已实际读取以下原始日志及exit汇总，而非只依据报告/开发结果：

- `management-root4-tests-final.log`：31通过、0失败/忽略、335过滤，15.72秒；包含原001/002/003及正文不写旧表/SQLite分支。每个fixture明确检查SCHEMA_VERSION=4、PRAGMA=4及attempt新表存在，不将open4误作旧3。
- `management-root4-model-regression.log`：69通过、0失败/忽略/过滤，覆盖原21活动/请求、ACP发现重放/权限投影及视图合同。
- `management-root4-proof-negative.log`与negative-classification：10项E0277、1项E0423，外部crate因预期私有/非Clone/Serde/裸ActorContext转换边界拒绝；exit101是预期负向，不计为11个运行单测。
- `management-root4-clippy-final.log`：lib/tests严格clippy最终exit0；初次仅新验收资产风格lint失败保留，未降低产品警告或改预期。
- `management-root4-verified-exit.json`：matrix/model/clippy均0，negative101且expected=true；`management-root4-fmt-exit.json`记最终全工程fmtExit0。报告曾列fmt-final.log但该静默命令未留下该文件，已通知原tester修正引用；不把不存在文件称为已读日志，也不把此引用问题当产品失败。

独立主测试资产`f93596287f840e5352499ce4b47a0d26c9bed49cec6802d076cdca15166d0261`，003 child仍`124890c8423b9537d6927ce3622c26d5b99bdf892bd98170b9f66d263f0ed4aa`。主源适配只改自有fixture位置至系统临时目录、加schema4前提、等价命名/数值/格式和有reason的窄lint；业务31断言未弱化。两个即时可信callback保留async，在future真正poll时读owner/计writer，未用提前执行的ready future改变取消时序。

31项最后执行以后仅将cfg(test)模块声明按rustfmt排序。全行diff已核只有声明顺序，无产品/函数/断言变更；该无行为变化按已执行结果复用，最终fmt另有实际exit记录。后续完整入口仍由主执行在最终物理快照重新运行，不能把这里的局部检查改写成完整入口已经成功。

## 边界和未验义务

原001/002/003在新4组合原断言均保持通过，新旧管理/正文事实隔离有真实SQL；旧schema3及外部revision1/2失败证据继续保留，不回填为通过。schema4的K11+原13迁移实证按其独立报告归属，本轮没有冒充再跑迁移24；之后构建隔离移动会再要求24/31新组合回归。

各M组随机/分配故障、COMMIT后owner关联失败等未逐项独立注入仍not-run；默认100000 audit自然路径不可达，缩限SQL/算术不等于完整CL-083。当前事务subphase不清理，完整change仍须按spec完成安全清理/依赖保护及清理后不重派，不能采用曾经错误的“全change不清理”句子缩范围。

管理输入没有装配真实外部认证/目标来源/transport、故障安全控制、完整恢复reconciliation/ready、审批owner或WSS/真实Harness。完整任务2.2/2.4/2.5及CL-080–084/L5/L6不关闭。完整npm check/build的格式中断或PowerShell包装问题按实际原始退出另记，不算本报告已通过/产品失败。未运行Web npm audit，未提交/推送/发布。
