# Module 的显式通知交付精化 v2

本页与本包input-snapshot/prior-v1/contract.md组成完整有效补充，只精确其“ModuleTransfer先报告新stream metadata”，不增加公开proof/Scope构造或改变72字节wire。v1十成员原字节和freeze2f0c全部保留；没有实现或执行。

ReadRebind.metadata()仅是注册成功后的准备数据，不能据此声称peer收到。ModuleTransfer仍使用B7原next_record()/credit()/detach()：先向调用方报告旧记录Stale并结束旧部分record；新流真实initial credit后，**第一条实际peer重组的记录为既有RecordKind::Status**，其payload为平台自有小JSON对象：

~~~json
{"type":"stream_rebound","previousStreamId":"<platform id>","streamId":"<platform id>","viewEpoch":"<platform id>","readEpoch":"<platform id>","snapshotRevision":"<canonical unsigned decimal>","sourceBindingId":"<platform id>","sourceId":"<configured entity id>"}
~~~

所有id为既有EntityId的固定canonical数据编码，snapshotRevision用十进制字符串，不能引入JS数值精度损失；不得放native ID、正文、配置或凭据。Status帧仍用新的真实streamId/packet1、kind=Status、recordId=1（由kind区分平台通知与OriginalRaw身份）；它不推进任何原始record字节位置、不标SnapshotEnd或ReplayComplete。peer真正收到后才确认该Status包；匹配真实flush+credit后才能准备第一块新snapshot正文。随后平台record地址仍按其既有kind/原来源解释，不以通知的recordId冒native身份。

这是ModuleLocal受监督两端的同client装配通知；新流初始metadata由私有ReadRebind原物交装配，peer实际编码写initial credit，通知本身则必须走真实duplex接收，不能从registry合成已交付CollectedRecord。next_record()交出的Status/新地址明确可见，原始opaque正文不经JSON处理。旧stream尚可能完成的固定已开始块仍按旧stream事实收束，不交给新record重组器、不自动回旧credit。

WSS以后须使用原真实Attachment的control结果/通知先交新stream metadata，再由该socket真实initial credit开新body；仍保原当前验权、bounded outbound及writer围栏，未实现/未验。此处Module Status不充当SocketFlush证据，也不预签WSS通知schema或其可用性。
