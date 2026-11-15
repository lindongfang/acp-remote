# v2精化：唯一receipt上下文，不是native消息因果

本页与原字节prior-v1/contract.md、cases.md共同生效；只覆盖其§1/§5及RF08中可能把窗口说成RPC因果归属的表述，其他失败段/可读入口/FIFO责任不改。原a424包全字节保留。本分支仍设计、执行0、未批准编码。

ReceivedReadWindow只能证明：该真实Received的原ReceiptCut下，这个SID在原State里有唯一可验证accepted read匹配。它不是通知在native因果上由RPC产生的证明。origin也只沿原ND§8 receipt-window含义；不得改旧reader origin/accepted/ack/receipt语义以支持新view。

## 私有ABI的精确覆盖

~~~rust
pub(crate) enum ViewReadContextKind { OutsideReadWindow, UniqueReceiptWindow, AmbiguousReceiptWindow }
impl Received {
    pub(crate) fn view_read_context(&self)->ViewReadContextKind;
    pub(crate) fn view_read_window(&self)->Option<&ReceivedReadWindow>;
}
~~~

原getter只在UniqueReceiptWindow返回Some。新增kind由原reader同一个已验证cut/pending现场记录，不从当前worker/candidate/DTO事后恢复。所有字段私有，不给构造/Default/Deserialize/setter。泛型P等价端口同样必须表达歧义，不能用first或HashMap迭代顺序“选一个”。

新增投影独立计算匹配集合：同真实scope、同SID，cut.windows中的原cell key确实仍匹配accepted pending，且kind为Resume/Replay；按原cell identity去除重复同一原物。零个为OutsideReadWindow，一个为UniqueReceiptWindow并取该原cell/RPC，多个为AmbiguousReceiptWindow且window=None。它不改变旧reader::route目前find选origin的结果，也不修改任何accepted/ack状态。

当前ACP合法不可达说明：State::reserve_request在建新read cell前，对所有pending的Resume/Replay同SessionId执行SessionReadBusy检查。因此通过当前安全Connection API，同一scope+SID不能同时有两个合法accepted read；不同SID可以。该约束来自原协议state，不是新增view quota或本地“单op”自报。仍必须保留多匹配分支，防御后续协议实现/演化；不能用测试setter捏pending来冒真实并发。生产A9单op约束只是额外保证，不能代替原reader集合检查。

## 观察范围与完整性

prior-v1“同真实Replay窗口原RPC归属”精确解释为“同唯一receipt窗口的观察范围”。这是分段/隔离的必要条件，不是native消息因果或跨load条目身份的充分证明。旧failed/Live段绝不吸入新标准重放；唯一窗口中的已知ID也只能在该独立段按原版本规则关联，不推native轮次/因果。

每个view_segment Status再含：

~~~json
{"readContext":"outside_read_window|unique_receipt_window|ambiguous_receipt_window",
 "relation":"connection_observation_only",
 "limitation":"<static limitation or null>"}
~~~

Ambiguous、端口不支持/没有原物、或不能归入本次原窗口的同target raw，仍经合法UnassociatedSegments路径实际可读，不能长期source-only。它们不改变原匹配响应是否真实，也不造capture gap；raw保全但重建关联不足是单独的view限制。该情况不得仅因候选为空、收到Replayed或全部raw保全，就把新bundle说成完整历史重建。

真实标准Replayed+无gap prefix的ReadCompletion事实保留，transport另轴；view重建范围若含未关联/歧义，则aggregate保守ReplayIncomplete、JoinState UnassociatedSegments，Status limitation=ambiguous_receipt_window或unassociated_observation_range。不得把这个view限制写成原生保存失败/撤回响应/重发输入。旧已证明完成段仍保Complete；新空candidate旁存在这些不能证明归本次窗口的raw时，不能用空candidate假Complete覆盖它们。

## RF08保留ID，补实际可执行子断言

1. 同真实Connection先reserve/enqueue并实际接受A的Resume/Replay，保原Ticket/pending；对同SID再分别尝试Resume/Replay，原API实际返回SessionReadBusy，不能有第二accepted窗口；不同SID正对照可受理。输出引用原reader state检查及真实wire/probe，不能以作者内存flag代此事实。
2. 真wire在当前唯一窗口内给同SID更新，Received私有context是Unique且原RPC/scope匹配；更早已排队Live及另一SID不被归入该窗口。None不靠最近read补造。
3. 多匹配分支对原私有纯选择函数做普通数据/枚举单元覆盖可以，但只签算法分支，不签“当前ACP真实并发窗口已发生”；不得mint Received、accepted bool或完成proof。若未来协议合法可达多个窗口，必须追加同连接真实反例。
4. 对可合法到达的Unassociated场景用原window缺失/来源不匹配对照：原文经真auth/peer可读，真实Response事实与view重建限制分轴；不能把空candidate称完整。这些断言映射原SV006/029/041/046/055，原83步和期待不改。
