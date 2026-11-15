# Doc 七叶隔离实现初读

2026-11-13，独立 reviewer。**当前七叶静态差异未发现阻断；有限API compile Red成立。交付结论待七项真实Green回归及受影响检查，不代签独立动态验收。** reviewer未修改产品、构建、list或运行测试。

实际读取来源为 `.tmp/q3-doc-slice-20261113/source`，基线canonical `b9de84e397f31d3f7882c807b07ef28b47fe4774f93c6819cacce035498951aa`；HEAD `65ec7815bb0280f4519989d93a7b4ed3b4b37994`，index `ac76c7ad84c1d4333fc64a710647e60aeb96015eb4407f996a4bf6adc91fbff4`。逐核当前Root516叶全部仍匹配R1冻结，副本仅六个既有叶变化加一个新payload叶，未触R1六叶。所读七叶实际SHA、Red输入清单及原始编译日志SHA见 `REVIEW-R2-DOC-READ.json`，SHA256 `cb74a1b418d747e08cbe93e76f52fd65089f35fed1e8870ad2d10c7f579359b4`。

七叶差异符合原窄门：`QueuedBodySnapshot<B,P=B>`的core/owner保持B，payload独立P；sealed实现同边界；snapshot只在原Queue锁/品牌校验后clone().into()，原scope/current/epoch/撤销及Storage唯一登记逻辑不变。Node使用唯一 `DocumentDisposalOwner<NodeDocumentPayload>`，新enum只有真实Queued(PromptDocument)，RetainedRaw直接委派原uploaded_bytes，没有Source占位、重序列化或第二owner。cfg(test) reserve helper直接取同生产owner的slot。

固定新测试走真实Node/上传/提交/Queue，inspect原PromptDocument clone后into核原allocation ptr及原bytes，随后用同唯一owner的slot走snapshot并核scope/current/bytes、retire与idle。Red与当前Green该测试及Node三叶SHA相同；Red输入的三个API叶则均精确等于R1原基线字节。

已直接解析 `red-compile.stdout.txt` 原Cargo JSON：恰两个compiler error，均为E0308，分别在生产open destination与固定新测试destination，旧API要求 `DocumentPayloadSlot<PromptDocument>` 而实参为 `DocumentPayloadSlot<NodeDocumentPayload>`；没有缺module、E0107或无关error。Cargo101，原Job首尾仅PID42960/vctip、Stop=true/natural=false，owned cleanup active0；保持编译helper异常，不冒称自然退出或运行时Red。首相对Spec启动失败独立保留且Cargo未启动，不计Red。

当前原Green Cargo JSON已build-finished success、零compiler error，原Job首尾仅PID41612/vctip、Cargo0、Stop=true/natural=false、cleanup0。此刻尚未据七项实际测试或格式/受影响检查给出结论。待Root固定最终源、PE与逐项原日志后，只做同源增量后验；不扩大Source或端到端矩阵，也不把开发自测代签完整R2。

## 七项动态后验与窄集成结论

已随后读取固定 `SOURCE-GREEN.json`：517叶canonical `21137d04eb1cfcdce87342d363d29ccaa9734b2f75f716aaec760fe2cd8061f1`；逐叶读取当前副本零漂移。实际Green PE SHA `7d8789785a2728360850daa858140b7beaeaa7948da66a0c2b806fcd0af57fc1`与GREEN-PE及本次Cargo唯一artifact一致，已读取磁盘实核。原 `--list` 与7个完整selector逐项原stdout/stderr/result已读：均恰1 passed/0 failed/0 ignored，全部原Job natural=true、Stop=false、active0、non-timeout。新same-owner snapshot/From ptr/bytes测试实际执行通过；旧6例继续覆盖原Queue raw/socket/final credit、edit/cancel/seal、同actor不同socket、descriptor、dispose reaping及publication failure。六个真实Node用例原日志均有shutdown joined/metadata_confirmed及自有area removed。

**当前无新增阻断，允许此固定七叶窄集成，不需等待整Source完成。** targeted格式/diff、all-targets/all-features check、default build及实际集成源的受影响验证仍是待完成项，不由这7例代替。若格式化产生新SHA，须记录纯格式差异与最终身份；本次动态只绑定上述21137源。机器后验 `REVIEW-R2-DOC-DYNAMIC.json` SHA256 `6ad646892b081b7ea28c9b8684871a36ebd621cd011d4da8ecda719d299a2cfb`。

执行者为Root开发者；reviewer对实际开发Red/Green作独立只读复核，不将其重标为另一独立tester已验收。不代签Source result/producer、R2全部合同或完整WP4。Root窄集成仍不得改R1六叶或tester matrix target/冻结原证据；后续Source真实payload必须按既定串行交接接入此同唯一owner。
