# ACP最终Root组合独立闭环复核

2026-11-01。结论：**本次固定Root组合限定PASS，三个Open问题Closed。** 这是公共ACP runtime/既有注册和SQL组件/旧测试迁移的实际组合交付，不是NativeDispatch、Node/WSS或整个session-loop通过。

本报告只读复核源、规范、原始日志、诊断、精确资产及索引/差异；没有改Root源码/规格/独立资产/历史证据，没有提交或推送，也没有把reviewer读日志写成另跑完整测试。

## 固定Root源与证据

均位于 `D:/Project/acp-remote/testing/runs/deliver-codex-session-loop`：

| 文件 | SHA256 |
| --- | --- |
| 2026-11-01-q3-acp-final2-source-before.json | fa004a9042c0ade9ba835a3f061f0568707c40d354f2f83b4d472f4464aeed88 |
| 2026-11-01-q3-acp-final2-source-after.json | f01458de3d6aef7d96caafeee73e77094d518f6fddcac0609f4c58365052e8b0 |
| 2026-11-01-q3-acp-final2-root-evidence.json | 84722eed258ba11718e3733cdeaca6c12ea3f4e2e89114c48b6a6cf3ff99d348 |
| q3-acp-final2-root/integration.json | 33b700f802ac733689df751bcbefaac6935bab0a172854985f28d479e98b4a00 |
| q3-acp-final2-root/full-check.txt | 74eb329fb6beb05d2ef399f10f75095ebda3790e5b8ec5024d4f6ee7c597cecc |
| q3-acp-final2-root/exit.json | 50a67f471bc0d0195d0598fb86ea2359ecfa05bf19d84ca0a6db4f404825f3b1 |

reviewer实算before/after清单摘要及306项实际Root文件：全部相等、前后0漂移；35份根归档证据逐项实算全匹配。以批准的port-v2 source248覆盖旧3测试路径再加36包重算核心合集281项，Root全匹配；核心281外的25份权威/配置输入也由306清单核验。36包每件与Root一致，故此前5d374d2e逐断言/源码审查可准确复用，不借新Root版本扩大原证明范围。

另用rg列实际编译来源app/src、app/tests、clients/web/src及两组ACP probe的rs/ts/css，共221项，均在306清单，无manifest外来源。本人只读git diff --check及git diff --cached --quiet实际均0，未动索引。integration保五旧文件前像、两patch先check再apply0、31probe精确新增及7项仅换行差异后的批准字节恢复；最终281逐字节相等再次排除了隐含格式改写。

## 六阶段实际结果

原执行脚本、toolchain-owner及各原日志已亲读。实际Rust/Cargo1.98.1，Root cwd和真实Windows owner上下文，离线/locked；不是外部cwd的默认工具链。script为执行时原路径的字节归档，记录的pipeline结果是实际退出，不按历史scope文本的pending标题推断运行状态。

| 阶段 | 原始证据与实际结果 |
| --- | --- |
| npm run check | check0。Web检查/构建及50/50；Rust fmt、默认all-target严格clippy、all-target测试实际完成。库513/513（39.86s），含旧C两真实阶段的同名取消用例；完整session_loop69/69，含discovery17/permission12及更正P09；WD4/4。其它既有/新默认目标结果保原日志，不把filtered或0test算覆盖。 |
| all-features严格检查 | fixtureClippy0；实际cargo clippy --all-targets --all-features --locked -- -D warnings，包含两opt-in fixture配置。 |
| registration_restart | restart0、1/1。原日志明确registered/begin_committed/applied/settle_committed/settled五个实际子进程边界，各child_exit93。 |
| acp_runtime_owned_pipe | ownedPipe0、显式feature的1/1实际执行；其双版标准close/Closed/IO join/本次Job active0合同与已审同字节资产一致。不是默认0test或仅owner.shutdown冒协议close。 |
| 根Permission编译probe | permissionProbes0；Root新证据目录实际运行正1 check0、负9各101。详见下一节。 |
| npm run build:rust | build0；实际Web构建及默认cargo build --locked完成。默认测试中门控fixture为0test时未冒签，通过上两显式阶段另证真实执行；默认build不声称清除了已在专用target生成的fixture文件。 |

完整检查/构建原文里的增量缓存hard-link退回复制警告为工具缓存输出，保留；没有把它伪成代码lint错误或隐去。原513/69/WD及各默认目标均真实0失败，检查与显式fixture的并发/串行参数以脚本为准，不将一次有意串行的child阶段称默认并发覆盖。

## 根正1反9的真实诊断

Root portable runner确实从 `D:/Project/acp-remote` cwd执行，工具链日志1.98.1。reviewer重新解析九份原JSON诊断：supported/tool/session/options_clear/options_push/raw_swap/id/kind为E0616；Choice literal为E0451；各预期主span均在该probe main.rs第7行。正对照0且无error，九负例实际101；与root results摘要一致，不以任意非0当证明。

原输入main/Cargo/lock与批准36包的Root字节一致；package/bin独立；没有因为旧私字段攻击不可构造而加假品牌/生产兼容口。未知主题/kind的真实人工原ID选择沿现spec，未重新禁选；无品牌/foreign/错ID/duplicate/spent仍拒绝。该层仍不等于持久化人工批准或Node授权接线。

## 问题关闭

| 稳定ID | 最终处置与依据 |
| --- | --- |
| ACP-IMPL-WRITER-DEADLINE-001 | **Closed。** 生产c737仅attempt优先；same红cfdd/support fa528不变转绿及独立WD01–06已在5d374d2e核验。Root同字节实际WD4、库/全入口/严格检查和构建全部通过，Zero/Unknown/Written、早Received、实际join/Weak责任未改。不是调大timer或等待方造结果。 |
| ACP-INTEG-LEGACY29-001（TARGETS-001为同别名） | **Closed。** 原遗漏两个模块17+12均保身份，21机械/8明确规则迁移已逐断言审过；Root完整69真实通过、原120实际入口及显式child另通过，正1反9和全target编译/严格检查完成。没有删原模块或生产兼容API掩盖编译失败。 |
| ACP-INTEG-LEGACY-CANCEL-001 | **Closed。** cfg(test)真实COMMIT前/后通知端口a686与独立C ff592固定字节已在4a0cfee1审过；原faulted/count0或1/重新BEGIN/ROLLBACK/close断言保持。独立1/1及512、现Root513实际同名函数通过，完整入口/build补齐；旧25ms不足以证阶段的oracle修正不改真实提交规则。原512失败确切取消时点不事后编造。 |
| ACP-INTEG-LEGACY29-ORACLE-001 | **Closed（定义纠正）。** 双方c188门禁漏核新审批spec的错误已在28728795公开纠正；新P09在独立12/69及Root69实过。TEMP ACP-IMPL-PERMISSION-OPTIONS-001产品归类仍撤回，不能记“修生产后通过”。 |

此前ACP组件八个IMPL问题及开发瞬时断言TEST问题的Closed保持原限定范围，不重新计算本次问题数量。旧248首次编译失败、旧C失败/受控反例、P09首红、writer最小124及中间资产工程失败、原gate与组件报告均保留；本报告是新最终证据，未回写历史PASS/未执行状态。

## 结论边界

固定Root ACP组合的真实编译、协议/IO及既有注册/SQL回归已闭环。**尚未签署** actual R3 owner与NativeDispatch真实IO派发/新资源Loaded mint链、Node/WSS/真实认证页面、持久审批唯一决定、真实Harness/多层descendant树、完整无预算共享视图或完整WP2/WP3。NativeDispatch rev4仍有RESOURCE004工程门槛，不能以本306/六0通过替代。

后续源变化需另版影响及同源证据；306和本Closed仅对应当前固定结果，不因实现仍有下一阶段任务而伪报整个项目完成。
