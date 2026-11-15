# v3运行时与Drop补充

绑定已过编码门槛的d9b1cf3f补充，不改其原字节。尚未分发的candidate-v3/source-manifest.json a8130585…保留首实现原字节，因root指出并实际复现无Tokio上下文Drop的panic，不作为最终交付。正式修复候选使用candidate-v3-r2。

真实红：新增developer例将真实begin_consume Future轮询到BeforeCommit已Pending，再由std::thread无Tokio上下文drop。原产品连接关闭模块e59ffffb…的ambient tokio::spawn真实panic“there is no reactor running”，0/1、0.08秒；日志connection-owner-v3-thread-drop-red.txt保留。不是假Future或只丢channel。

修复：ConnectionOwner构造时保存Handle::try_current().ok()；无受信runtime Handle时lease在acquire之前typed StorageUnavailable，未构造连接、不spawn、不unwrap。合法ConnectionLease携带该捕获Handle，handoff用Handle.spawn，Drop不依赖当前线程context。不改变COMMIT/取消/active计数或关闭slot语义；runtime若已结束导致任务取消，真实join得到Unconfirmed，不能伪闭。

Storage::Drop另同步seal新registry lease，保留现有close job责任、不await不伪join；原Storage.close仍先queue stop/join允许收敛SQL，之后seal/drain再pool.close。遗留Repo clone在Storage drop后不能新acquire。

实现中的RegistryConnection.close是私有async方法：未首次poll时Future仍持RAII guard，若被丢弃由guard Drop同步handoff；首次poll立即同步handoff并在第一个await前已把conn放进tracked slot。无未计数或未登记窗口；waiter取消不取消CloseSlot中的consuming close Future。这个具体签名与d9b1同步close描述的责任/原子边界相同，未暴露新的产品API。

测试hook类型路径为crate::session_storage::request_registry::connection_close_test_hooks；新私有模块通过pub(crate)别名重导出，生产字段与构造器仍私有。ConnectionCloseStatus路径为crate::session_storage::ConnectionCloseStatus。原R3/v4 hook签名、业务错误、schema和正文策略不变。
