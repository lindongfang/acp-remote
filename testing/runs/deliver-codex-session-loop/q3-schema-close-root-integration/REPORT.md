# Root 测试连接关闭维护交付

经独立维护门槛和原 tester v22 复验批准后，仅在两份 product_schema 旧测试中合入七处显式 `close().await`。原断言、helper、DDL 和产品源码未变。

实际 Root 固定334项，另外332项原字节；先 git apply --check，再 apply。Git 自动换行转换另留证并恢复批准的精确原字节。Git 索引前后一致。

实际当前用户 Windows owner 同 shell、Rust锁定离线、专属 target/TEMP：原schema11例全部通过（12.22秒），原物理池关闭2例全部通过（0.06秒），fmt和git diff --check均退出0。三个检查临时目录及本次构建进程检查为空。

仅证明这些正常测试路径的维护修复；不追认原残留的唯一原因，不证明panic/cancel或Native80/Shared83。后续Native整合须保留两份文件的七处关闭，另保留此前12个Web TS输入。
