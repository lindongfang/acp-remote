# 旧正文与强刷接口的构建隔离候选

本文件细化implementation-contract §1/4的旧路径隔离；77521442方案及dea0fe2b独立定义已通过阶段门槛，当前实现已通过独立交付验收及检视。实际数据保留与新路径不写聊天正文是既定规格；本子阶段不新增产品行为，不删除旧表/行、原生会话或私人文件。

## 范围

先前部分实现的application/history、application/approvals和旧InputRepository/InputService，以及session_storage的旧SessionStore/history/approvals，只在cfg(test)编译以保留历史回归。正常cargo build及作为依赖编译的库不再提供这些正文持久化/强刷/必须Run审批入口；不增加可在生产配置或Cargo feature重新开启旧接口的开关。

新product_commands、domain/native/activity/requests/views、schema及product_schema保持生产可用；它们不得因隔离改用旧InputService或数据库正文。EntityId/ActorContext、根纯数据值和既有通用错误保持；旧domain/history与domain/approvals仅测试编译，不再给生产新路径提供旧存储/强刷关联模型。保留数据类型本身不产生正文SQL或原生动作，也不作为新的网络DTO或授权对象。

## 具体改动边界

- application/mod.rs：仅将旧模块、旧输入trait/service及其对应use置于cfg(test)，新product_commands声明保持。
- session_storage/mod.rs：生产仅保留当前schema/product_schema/product_commands声明；原旧实现按原文移到session_storage/legacy.rs，旧history/approvals通过显式path保持原文件位置，legacy及其兼容re-export都为cfg(test)。旧tests.rs继续原断言，路径必要适配不改变业务逻辑。
- Storage::sessions/histories/approvals三个旧工厂为cfg(test)；product_commands工厂及共享故障状态、身份/信任/目录锁/迁移接口不变。
- session_storage/history/tests.rs仅机械更新crash_after_reservation_child子进程的--exact全限定路径，实际新命名空间为session_storage::legacy::history::tests；继续断言child exit93及VISIBLE_POSITION=1，不接受因零测试匹配而退出0。其他测试断言不改。
- session_storage/history.rs、approvals.rs仅为原cfg(test)子模块各增加显式history/tests.rs、approvals/tests.rs相对path，防止父模块显式path搬移后误加载父tests.rs；domain/mod.rs仅给两个旧domain模块增加cfg(test)，新模块/纯基本值不变。不得用allow(dead_code)保留生产旧模型或改业务方法来掩盖编译问题。
- 对应开发说明标明历史回归用途。CORE/SESSION旧DDL及schema4管理DDL不改，数据库迁移、读取身份/撤销与目录所有权均不变。

主执行串行修改上述文件，独立tester只维护编译边界资产/报告，reviewer只读。此前管理rev3与schema4冻结证据保留版本归属；最终新组合重新编译和回归，不用移动前测试日志代签移动后结果。

## 验收

独立外部小crate以普通依赖方式编译（不是把整个依赖强制cfg(test)），实际确认旧工厂/应用接口/存储模块不能被导入调用，同时当前EntityId及新产品接口仍能编译。记录编译错误类型和逐个入口，不把任意语法错误或依赖缺失当隔离成功。

在实际仓库正常cargo build、fmt/clippy/all-targets测试均通过；旧业务断言仍由cfg(test)运行，不能删除/ignore旧用例以掩盖路径问题。对比迁移与product_commands生产源码/已运行SQL资产，确认隔离没有改变管理事实、签名、内存输入消费或引入新正文写入。迁移24项/输入31项的实际组合回归及新输入标记未进入旧表/SQLite分别保留；不读取用户实际数据库取证。

此子阶段限定为任务2.3的构建隔离与旧数据保留边界，不签真实Harness/WSS/Web或整个change完成。
