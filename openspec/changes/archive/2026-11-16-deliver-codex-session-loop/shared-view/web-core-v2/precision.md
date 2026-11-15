# Web内核 v2：控制帧与解析失败精度

本页加上`prior-v1/CONTRACT.md`构成有效工程设计；v1原freeze及47成员均保原字节。仍是未编码/未门禁，不改变83预期或72字节wire。只修正Web v1草稿尚未实施的歧义。捕获的活copy wire.rs仅作研究输入，不称最终已验协议实现。

1. **Data packetSequence必须1..u64MAX。** 解码器当场拒绝0，不只依赖后续assembler拒绝；Credit允许0..u64MAX。有效B4明确要求Detach.packetSequence及其它非stream字段必须0，Detach不是确认。有效接口为`encodeViewDetach(streamId: string): Uint8Array`，`ViewDetachFrame.packetSequence`类型为`0n`。原v1允许Detach任意u64的候选作废；不修改其它header字段或提供新frame类型。未来独立双端向量验证必须包含非零Detach拒绝，不拿研究Rust源码当动态通过。

2. assembler接受Data时先验证runtime字段、stream、packet、record和byte范围、复制payload，再原子推进自身状态；错误不得先报告接受包/产生完成record。结构错误封该本地assembler且返回静态错误，分配失败也保持局部失败，不取消节点工作或丢节点缓存。错误后不重用该assembler；seal保持幂等、丢弃本地半record。任何错误不产生自动Credit/Detach/网络动作。

3. 严格UTF8验证可分固定CPU处理块，用同一个`TextDecoder("utf-8",{fatal:true})`的stream模式并在末尾flush，丢弃临时验证字符串；不能逐网络chunk重置decoder，也不必为了验证把整raw再转换成一个巨大字符串。块长只为本地调度/复制，不是正文长度上限。词法扫描仍对完整拥有的bytes迭代，不将此称作页面增量展示。

4. JSON字符串语法按JSON转义文法验证，允许原文保留反斜线u加四位hex的转义，不提前替换或规范化数字/转义。只有已验证string token的显式stringValue才JSON.parse单literal，原tokenLexeme始终可读原转义。重复键保留原顺序，scanner不返回普通key/value执行对象。字节和token数据归该document独立私有所有权，公开token表及记录不可改；调用者改输入/返回copy都不改原文。

5. 原开发检查只证明非视觉模块，不声明DOM安全、实际Socket来源、权限撤销、Status可见通知或新stream绑定。源码检查禁止直接/间接管理动作依赖；即使原数据包含`__proto__`、constructor、script、URI或审批指令，模块也只产平面token/原字节。UI/流连接装配保原后续门槛。
