# NET-I001 关闭机制编码前独立复核

2026-10-30。**设计可实施，未发现需NET-R阻断的方案问题；仅允许在既定隔离owner范围实现，不签修复成功。** 本reviewer只读主D现有node/management_ws、plan及任务提供机制，未改产品/归档报告，未运行网络复测。输入不是冻结实现，后续必须对实际源码与红绿证据重新检视。

主D依据：node.rs SHA256=`8C0A09EC141B76A1949A73AF827ADBAEFDC1B8AB005FC4485D4ED5087C4AF516`，management_ws.rs=`F5C8006E3BEEA09B92367EA8ECAB7A75C515F616C7CC68F1C853D624D994595E`。现有连接JoinSet拥有网络task、HTTP upgrade把TLS流移交WS任务、management独立reader/writer split，因此只能在原连接owner层收敛真实底层释放；不能把HTTP upgrade future返回当TLS/WS已drop，也不能在fused的WS错误流里假装仍能drain。没有独立重读axum私有源码，本次该API限制以主执行已提供的固定本地调查作为实现动机，不当成已测试产品结论。

## 可实施边界

- 接收TCP时建立安全标准库副句柄，owner仍在原网络连接task生命周期内；TLS/HTTP/WS独占数据主handle，包装Drop只发完成信号，不能在Drop启动未拥有task/阻塞读。网络task等待**最终主包装真实Drop**，不是reader退出/HTTP future结束/发close成功；此后才允许控制副句柄shutdown(Write)及有界排空未读密文。正常协议期副句柄不得读取、探测正文或抢占TLS字节。
- 副句柄只做关闭和丢弃密文，不建立应用请求旁路、不TLS解密、不执行新命令、不写应用帧。排空至EOF/显式字节上限/已有deadline先到即终止，固定小缓冲，不积累整个超帧或为了获得4429无限读。达到上限/超时要保留实际未完整关闭事实，不能承诺对无限攻击流必然优雅关闭。
- management首次开始关闭时，通过私有服务Extension发布绝对deadline，writer发送Close、等待主Drop及transport drain共用该deadline；若有重复关闭来源只能取最早deadline，不能被之后通知续期。所有关闭出口都必须纳入：普通协议close、容量/格式/认证错误、close_immediately、撤销/存储错误writer分支、写失败/超时、节点stop。不能只修超帧测试路径而让其他出口再各等2秒。
- 原节点15秒关闭仍是上限；原JoinSet中连接task负责wait/drain及副句柄，节点取消/超时abort令owner Drop执行shutdown(Both)并关闭副句柄，解除主handle仍在等待的网络IO。等待Drop信号和排空也必须可取消，错误/不发信号不导致隐藏连接task泄漏。不得持client_gate或数据库事务等待transport排空。
- close开始后原业务writer停止新增应用帧、清理/失效队列继续受现有撤销gate协调；副句柄shutdown/排空不发送正文，因此不会扩展撤销COMMIT后应用发送权限。已交网络栈数据不可收回的原边界不变。禁止以获得TCP EOF作为Run结束、资源释放或授权成功事实。
- nonblocking副句柄不得形成busy spin。采用已有Tokio/标准库可取消等待，重用绝对截止时间；如果从标准socket转异步控制IO，必须在主包装drop后才注册/读，避免同时在两套数据reader间争抢字节。不新增依赖、固定sleep延长关闭预算或放大现有WS帧缓冲。

上述均为所述方案落地时必须保持的所有权/预算语义，不需要新增产品或安全授权。允许写范围为隔离Cnet-close的node.rs连接所有权/相关开发测试、management_ws.rs关闭边界及必要私有node/transport.rs；禁止改32KiB/深度8/认证/队列、2秒单次关闭和15秒节点预算，不写其他并行包或主D产品。后续WP3/WP4共享文件接管需同步修复冻结基线，原owner串行交接。

## 实现后必需证据

保留普通客户端原串行第9次红及原并发失败，不换自定义客户端规避。修复版在同一真实普通TLS/WS客户端重复高复现红绿，确认对端可观察4429而非仅服务端测试钩子值；同时验证客户端不读/不结束、持续发送直至字节上限、等待最终主Drop、HTTP正常关闭/upgrade后关闭、写失败与取消分支无资源残留。收集实际耗时/计数证明writer与排空没有叠加预算，不只凭无超时测试通过。

重跑原认证/配对/撤销及撤销COMMIT后零新增应用发送、旧帧/深度/出站队列/连接上限、slow-client和节点15秒关闭检查。排空只统计固定元数据，不打印密文或正文。实现源码须固定完整受影响文件摘要，独立tester/reviewer再判断NET-I001关闭；本报告不以可实施设计代关原失败，也不改变已有归档验收结论。
