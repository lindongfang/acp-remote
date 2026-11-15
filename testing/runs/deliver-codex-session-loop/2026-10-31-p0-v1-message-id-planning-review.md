# P0 v1可空messageId编码前独立复核

2026-10-31，reviewer `/root/baseline_reviewer`。只读合同、实际策略/模型及固定本地SDK，没有产品修改或新测试执行。

**P0限定编码门槛通过，未见设计或用例阻断。** 只允许现有Strategy消息分支将协商v1的messageId:null解释为None，以及独占开发测试/必要mod声明；不放行P1/P2、投影索引/scanner、Domain新增端口或node/WSS装配。

实际完整读取并重算：

- 根`p0-v1-message-id-contract.md`：`833c1124b7257cfaf796f045fa3b7e72bceb7d343ae1204e6836beccec7e1981`。
- 原tester七组定义`p0-v1-message-id-definition.md`：`4a16aa62b77d0c933bed614911ab770b6470e2278e2cbed81b5706c39bf5bed1`。
- 实际Strategy仍冻结`cb91ed1d4395ac70af35fb422a8c97e1b2edf93f76feb39b10f638f3cf43debf`。
- 本地固定SDK2.1.0 v1 schema：`1c57095acf9c0844054dd1fdc3624c121b3fdc24209ecc987fd6748c705fdef5`。

实际读schema的ContentChunk：required仅content，messageId可省略且anyOf MessageId/null，合法null并非未知结构。当前strategy.rs:374对任何存在值调用MessageId::parse；model的parse只接字符串、限制UTF8长度≤4096且不禁空串，所以null被错误拒绝。按实际selected Version::V1增加null分支，省略/正常字符串/空串及其他错误边界沿用原逻辑，是既定协议兼容修正，不需要新用户产品裁决。

七组覆盖三个合法v1 chunk×省略/null/string/空串、UTF8 4096/4097、错误类型独立连接、offer2 selected1、原raw逐字/接收序/origin与null后继续读取、v2现有粗投影不变、原17发现/12权限及开发回归。全部要求真实initialize和Connection::next，未以直接Strategy调用代替入口；异常连接独立且有截止/受监督peer。预期有明确固定来源，执行安排可行，当前全部not-run。

本包保留v2现有省略ID粗投影None不等于签完整v2必需字段符合性；完整校验仍留P2，不能借本包放宽null/错误类型或顺便改变v2。未知字段/raw/origin与连接错误处理不变，不新增聚合/Run/Turn/审批或重试。修改后需固定实际源码和测试摘要，同版独立七组及原17+12回归并检视真实diff；未执行前不能签交付。

VPROJ-PLAN-002/003仍属独立投影设计阻断，不是该合法null修正的前置。Domain预算端口交付结论与新入仓测试资产样式/根组合回归另记，均不由本报告覆盖。
