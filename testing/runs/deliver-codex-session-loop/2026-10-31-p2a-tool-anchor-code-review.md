# P2A有限工具锚点：固定候选静态审查

2026-10-31。结论：本次只读源码检视未发现阻断；最终阶段结论等待原tester按A–H对同版公开接口的独立执行。不得据本报告合并后免验，或关闭完整VPROJ-PLAN-002、任务3.2。

## 固定来源

外部目录 deliver-codex-session-loop-p2a-implementation/workspace；没有使用root正在变更的Domain作为依赖。
- HANDOFF.md：7828d2fd3768615459115f3dd9d3907e999b83a8a66d24d5f5fa468ef38e4d8c
- implementation-source.frozen.json：9181f209916771ecc59e2b3b7f0f8703beaab59266234b8f95b2ee769f7637b6
- baseline-source.json：5ddc610e9c4d3c6b898616e2e4c324e5ab11a7ca3a51e82d5d6618e6de1b28c4
- p2a-candidate.patch：ea8bac49e74c436482ca4457095732d9ca1d9e8e6436bb0b6257b5263d73ac49
- tool_projection.rs：4cf2b6d36a8a3095bed73de496d926cec871e97bc0addf431256be5a8d54b503
- 开发tests.rs：f9cbf51ac8ff4be8aded29121e0c961820f24d6393e2939ae881da79ee788433

自取摘要核对103个workspace来源全部匹配manifest；其中100项与原baseline摘要相同，acp/mod.rs仅增加模块声明，另两项新增。baseline目录只保存需差异比较的mod文件，不是101文件完整备份；未将不存在的完整baseline副本声称已逐文件验证。依据root合同81d0b6ef…及tester定义faa72a83…；外部Domain固定a22bb6e5…/d2a2a6ec…、P1固定0164d462…，不同于root新的借用revision实现。

## 实际代码判断

完整读取新增生产实现和15项开发测试，审阅patch、mod差异及开发日志。P1 observe只调用一次，原raw结果独立保留；先完成整帧字段形状/本版枚举验证，再修改锚点。已保留原Tool/id位置的线性查找和消费lease替换；只有真实MissingItem新建partial，Capacity、损坏、旧scope等不转为新位置。原位更新不依赖256项索引或大raw分段。

固定512字节编解码校验长度、版本、namespace、保留区、序号、来源、知识槽、丢失水位及status/kind联合码。最终§4A的赋值/保持/null区别、v1初始_meta:null、v2未知枚举保守显示、chunk仅content槽、R保持历史loss/D更新当前loss、flags汇总均有对应实现。未知任意字符串仅知识事实，未暴露伪造字符串读取；RawEvidence只有Unverified/NotRecorded，Recorded不是当前仍保留的承诺。原raw可被同帧锚点插入再次淘汰。

费用复用现有Domain端口：持有P1 read之外追加1024工作区，固定512 scratch编码，旧lease/CAS及候选双份峰值沿用原端口，没有完整raw/Value/String/id clone或动态索引；唯一控制身份clone不携正文。开发实际layout列ToolProjectionRead552、嵌入ObservationRead304，额外248<=512；不把size_of视为全heap/RSS证明。未引入依赖、unsafe、协议写入或权限裁决。

开发15/ACP60/原Domain20/公开69及fmt/clippy日志仅作为作者证据读取；没有将作者测试代作独立A–H。补充四个旧公开二进制只有作者明确汇总，未捏造完整原始stream。

## 保留范围

等待原tester同版独立证据后才能给本小包最终限定结论。内部不可达CAS/u64极值等不得冒称普通API动态覆盖。完整current/scanner/可读取任意字段、满4096首次订阅读取、真实来源owner/授权/撤销、审批、WSS/Web、可信轮次及真实Harness仍未验。后续与root新Domain组合必须重新固定摘要、审逐hunk并运行组合回归。原VPROJ-PLAN-002完整义务保持开放。

## 独立证据到齐后的最终限定结论

2026-10-31追加。已亲读原tester p2a-tool-anchor-acceptance.md（SHA256 e536ea79f5e1a5e2390a7e67d5fd6fe0a8a9ca9d753da1050b1e372f711c7d63）、A–H实际/未执行表、p2a-tests-final.log、p2a-regression-final.log、p2a-final-exits.json、negative结果与严格clippy/fmt、锁比较及前后来源记录。另实际重算candidate workspace及独立p2a-sut各103来源合计206个摘要，全部等于固定expected；11件验收资产实际摘要也全部匹配。source-after SHA256 3573e74f1ba69b40829397027a251191566bb8498ac3a5d41b0716d4f71c652a。

独立普通依赖19/19、原资产112/112全部通过；8192个联合码组合、真实双版Connection/257工具/长raw后晚完成原位、R/D四项精确转移、Recorded后同帧真实淘汰、仅真MissingItem新建、2369精确峰值成功/2368失败、缩限4引用满账本无fallback等原断言已核读。6项负编译及5个Copy正控制有确切目标诊断，实际退出记录与日志一致；237个共享依赖精确匹配产品锁，没有新增registry包。PowerShell NativeCommandError包装文字不当作Cargo失败；0项doc/lib不计功能用例。初始资产API误写、缺source夹具及风格lint均在原报告/日志明确归属，未改产品掩盖失败。

**最终结论：外部冻结P2A有限工具锚点子范围通过独立源码审查及已列公开API验收，可进入按三路径最小patch的集成准备。** 不需要为本小包再裁决产品规则。此结论不能直接签root组合：root新的Domain/revision两源不同，必须保留root现有实现，只添加新模块/开发测试及必要mod声明，重新固定组合摘要并运行本19、原112、root DBR与适用工程检查。

P2A内部CAS/u64极值、其中途bulk null/short-write等仍未独立动态执行；同版Domain原20+6只证明内层已列路径，不扩大成P2A每一步注入。全字段current/scanner/可读定位、满4096首次交付、输出/授权/owner/WSS/实际Harness未完成。原VPROJ-PLAN-002完整义务及任务3.2保持开放。本阶段没有新的稳定问题ID。


