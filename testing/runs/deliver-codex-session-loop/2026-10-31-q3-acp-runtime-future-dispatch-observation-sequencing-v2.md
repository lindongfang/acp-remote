# 后续actual R3观察处理屏障：范围精化版

2026-10-31。保留前版SHA256 9e617329508ba0651189300cb19538c3b5efd3f934272a2eaa530712c3e021ef 的原字节；本版仅补充小事实确认与显示/持久化分离边界。**确认未来接线缺口；不判当前公共runtime scope失败，不修订64cc，不定义新public API。** 仅只读工程盘点，后续必要账本+actual R3 owner+IO精确合同须闭合。

## 64cc当前确实提供与未提供的边界

真实reader按poll_read/LF捕获ReadReceiptCut，锁外parse、短锁route后排Received；Received提供只读sequence/receipt_order。EventReceiver只有async next。外部WriteFence拿WriteIdentity/WriteStep/PollIo，在应用自己的短锁内验证状态并调用实际poll。它看不到协议Core尚未被应用处理的Received、已经取得但尚未route的完整LF集合或可信“已追上”证明。

因此以下真实时序可达：reader已经route失败/活动观察并排入队列 → App事件pump尚未更新target/queue状态 → App fence仍见旧Ready → 下一输入实际poll。Received品牌证明来源，不证明观察已应用到目标状态。EventReceiver.next已pop也不等于应用已完成；单独有last_seen/receipt_order不能证明追上当前frontier。

只在fence开始时读取一个新watermark也不够：检查watermark与调用PollIo之间，reader仍能取得或route新的完整观察。外部App锁不阻reader，因为reader只持ProtocolState锁。若真实W.poll时不再同锁验证，就存在TOCTOU（检查时与使用时状态已变化）。

## 最小后续合同应明确的能力

1. 一个受信观察处理端口或等效私有装配边界，记录应用已**连续**同步应用的真实Received序及scope/generation。确认发生在目标原生活动/queue状态短步已处理之后，而非仅pop、排内部job或补审计；不得按后来一条确认跳过前面尚未处理项。旧scope/旧generation或重复确认不能清新观察。
   连续确认只表示可信应用已经吸收影响派发的协议、活动和审批小事实；正文原文与显示工作可独立交给其owner。不能等待旧P1/P2投影、慢客户端、正文分页、附加审计SQL或审批选择长await完成才确认，不能把展示变成输入控制门槛。必要状态持久化尚不可靠时，由该目标独立RequiredStateUnavailable围栏表达，不以显示frontier代替它。
2. 真实写poll在同ProtocolState临界区核：没有未覆盖的已route观察，也没有已取得完整LF而尚未route的held集合。核完到真正W.poll及accepted记账之间不可让reader插入新cut。完整LF边界沿64cc；不能把reader未poll出的OS/peer缓冲或半帧冒称已处理。
3. 未追上时保原active/armed lease/cell/attempt和正文，只注册可靠唤醒并Pending暂停；应用完成处理或held集合route完成后再核。不能伪造Zero/Written、重做D、换attempt、自动重发或以旧Ready继续。协议reader/控制接收必须仍可推进，不能让App锁内等待SQL、大parse或审批而阻其处理。
4. 仅新危险派发的观察barrier与安全cancel/close分开定义；不能无条件让安全收敛也等待同一新输入barrier。控制仍须按可靠目标/资源归属及现有安全规则处理，不借此恢复派发再验原actor。

可以保守按整连接观察frontier处理以避免parse前猜target，或在可靠route之后按target细分；这里不替作者选择新类型/签名。关键是最后实际poll的原子比较，不能只增加可分离getter。公开当前WriteFence的零次poll会InvalidFence、FenceRejected一般是终态，所以后续还须精确约定由哪个受信边界提供“暂缓而非终态”的语义，不暗改现合同。

这些序列/确认仅证明本地事实已经由应用处理，不证明原生执行归因、Agent已接收、终态、原生Ready或网络中没有更晚活动。不能把本地serial当ACP未提供的因果标识；真实native证据及后续reducer仍另验。

## 后续独立验收的最小反例

- 真实Received已route但故意暂停App处理：禁止下一危险写；处理对应失败/活动后结果反映新目标状态。
- App已pop但尚未应用：同样不能解除屏障；批内多观察不能确认后项跳过前项。
- 已取得完整LF、尚未route：不得按旧Ready穿越；不能用仅peer写入代替真实取得。
- watermark检查之后、真正poll之前有新观察：实际poll同锁核能阻断该TOCTOU；短无await缝隙以锁序静态证明加合法并发回归分列，不加生产yield伪命中。
- 等待中取消/资源换代/旧scope确认、并发close及安全cancel均保原所有权和正确唤醒，无换attempt或晚首写。

本报告不要求当前公共runtime实现这一未来App接线端口，也不撤销其既有编码门禁。后续精确合同须绑定同源真实SQL/R3 owner/实际IO验证，不能以单独watermark模型代签。
