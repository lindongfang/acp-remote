# Root639→Native+A789 精确增量审查

结论：本版精确patch暂不给Root安装门。发现1个稳定P2：ROOT789-INSTALL-REV001，当前schema8生产入口与7个仍挂载测试叶的成功打开后版本预期不一致。修复范围只涉及测试当前版本oracle；其余47 hunk未发现丢Root有效修改/断言或来源越界，193个受影响after与已冻结783逐SHA一致，另4处分别是已审Node维护及A两测试叶/挂载。无需重做Native业务或重跑同版Trust/Node/Web作为这7个oracle修复的证明。

## 静态阻断及最小修复

目标 storage.rs:18 SCHEMA_VERSION=8；schema.rs 的同一 initialize_inner 在 current<6/7/8 时依次迁移，并最后validate到当前版本。下列成功调用Storage::open/open_with_pool_options或schema::initialize的测试仍断言5/6/7，必然与目标当前入口矛盾。不是把这些测试算成已实跑红；本review只静态发现，未执行它们。

|路径|固定目标位置|错误成功后预期|本提案位置|
|---|---:|---|---|
|app/tests/registration_acceptance/c/management_independent.rs|240及246|SCHEMA_VERSION和PRAGMA均6|47修改内|
|app/tests/registration_acceptance/c/schema4_migration.rs|312、407|双头6/6及PRAGMA6|47修改内|
|app/src/session_storage/request_registry/tests.rs|211|成功重开后preserved(c,6)|47修改内|
|app/tests/registration_acceptance/m/schema5_registration_acceptance.rs|163|成功migrate后版本7|47修改内|
|app/src/session_storage/native_dispatch_schema/tests.rs|17|真实initialize后版本7|150新增内|
|app/src/storage/pool_close_tests/size_fence_tests.rs|163|真实Storage工厂两连接后版本5|592原字节保持内|
|app/src/storage/pool_close_tests/independent_size_fence_tests.rs|199|真实Storage工厂取连接后version==5|592原字节保持内|

这些叶均有实际cfg(test)挂载；后两者Windows单测在本机适用。management fixture直接比较crate::storage::SCHEMA_VERSION与6，无需猜SQL执行细节即可判定矛盾。后两条是此前Root有效的pool-close资产：原字节保留不代表其对schema5的成功后前置断言在schema8入口下仍成立。修复它们须给新精确测试维护增量，不能默默声称592全部仍可运行。

建议最小7叶修复：仅把“当前成功打开/迁移后”的版本预期固定为本目标8（需要双头的仍同时核两头）。保留原0..4/5/6/7历史夹具、故障回滚旧版本、伪头/未来版本拒绝、旧表逐字节与FK完整性断言；不要全局替换数字。保留pool-close无写PRAGMA、原max2、原owner、同关闭、首次物理尾、Weak与首错不重试规则；不得改生产工厂去强制旧头5、删断言、换假fixture或改变过滤范围隐藏失败。

修后应固定新789清单和精确Root639增量：原47内4叶修正、150新增内1叶修正，另把2个原592测试叶纳入显式修改，故成为49修改+150新增、其余590原字节同。文件总数仍789。所有其他目标SHA可机械核同本包，不需重开已审业务实现。对7叶覆盖到的原测试模块做真实定向检查（可先list核挂载，再执行相关原测试），保持首次失败和物理尾证据；执行结果不预签。新patch再核原639前像/150不存在/index及后像，届时可窄复查这些修正及实际结果。

## 已核的保护与来源

实核251冻结成员、639实际Root前像、789固定目标、197条before/after快照与patch路径一致，47修改/150新增/0删除/592相同。实际只读git apply --check --whitespace=error退出0，未apply；index仍ac76c7ad…。

Trust BEGIN IMMEDIATE、revoke测试与Web3五叶均与目标相同且不在patch。node.rs目标仍为已审b8cc43a3…：Root的原250ms超时cleanup helper及所有断言保留，Native两条shutdown观察断言保留；关闭错误处理、原取消/等待顺序不被观察器替换。A产品/两验收叶及lib挂载与已审资产一致。历史request-identity从集成测试入口迁到product_requests的cfg单测并改crate路径，正文断言保留。

逐47文件记录见HUNK-REVIEW.json。原owner handoff/Drop、actual ACK与join、同Storage audit、原runtime claim、原schema1..5迁移均在对应有限Native/Audit/Approval增量中延续。本结论不声称这些已变成默认D/完整原生上传能力；C stage3、F、schema9、WSS/页面和真实Harness均不在该patch。

strict603仍失败；STARTUP001/CLEANUP001保持Open。先前同版A12、Trust11、Node46、Web558/check/build有限证据继续有效，不能覆盖此次发现的历史测试oracle矛盾，也不能借本门关闭物理清理首红。
