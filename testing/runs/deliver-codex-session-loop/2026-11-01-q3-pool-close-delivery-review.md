# STORAGE-POOL-CLOSE-001 最终窄交付复核

结论：**STORAGE-POOL-CLOSE-001 Closed（本候选组件范围）；允许 Root 窄合固定两路径 patch。** 同池晚归还的物理关闭缺陷已由实际 Storage 同测试逆生产 hunk 的红绿对照闭合，PC01–05 满足。Root尚未应用及完成组合检查，本报告不提前签 Root 集成、Native80、Shared83、完整 Node/WSS 或本变更完成。

本人本轮只读源码、差异、固定证据，未改 Root/作者/独立资产，也未重复执行长矩阵。此前编码门槛 `abf705580074a71158511f8ea96121cfadf55853c2cd25bceb37ce081bd2c45f`、原最小红 `4421c8876069f4c31ee335aff7847f531f33ab200d4eec5c4baed1098adc627c` 保留。

## 固定版本与范围实核

- Root候选 `session-loop-q003-root-pool-close-probe/candidate-v1/freeze.json`：`9c9ea8d0890893a28b15ace6f84c7b864d223f4a5453535515c265949c27fa26`；**322源、28证据逐hash全部相等**。
- patch `b028cb10d3501e7f46502f095c5f673ffdf9476d910d3a50006c443f9a87283a`。本人在当前真实 Root 只读 `git apply --check` 实际exit0。
- 产品 `app/src/storage.rs`：`2598a79e8d944d4cf1042bc24a6e6c89a5be9eeecfb7a9458d2286cc231d673f`；新 cfg(test) `app/src/storage/pool_close_tests.rs`：`a9829a58ed758fa474b0a5037664d37db3b070bbca7b26c82dbc93e99a9aa00f`。
- 对原完整321输入精确比较，只上述2路径是候选增量；当前 Root321也逐项原hash，无提前合入。原锁 `87f27b619281b37015c79ceae8d95471c8a4de50f202f4dd8a6d99fa0d27e324` 不变，无新依赖/外网/用户数据库操作。

生产差异仅：公开open委托私有open_with_pool_options并仍传原max_connections(1)，原preflight/连接选项/同pool Store与Registry/初始化及初始化错误分支均原顺序；原close完成queue supervisor与专用ConnectionOwner后，对**原同pool**追加第二次close及两行因果注释。没有替换pool/伪造构造、调整授权/期限/容量/返回值、放弃clones或改Node关闭顺序。

第二次close的依据仍为已核SQLx0.8.6：归还先发布idle再退permit，首轮等完permit后的晚idle由第二轮排空；不是sleep/重试删除/无界循环。未新增SQLx异常、等待取消或runtime终止情况下的全局worker-joined保证，初始化失败分支也未由本修复自动改善。

## PC01–04：真实 Storage 与同断言红绿

独立包 `session-loop-q003-native-dispatch-tester/pool-close-v1/freeze.json` 为 `12edebef82fa25596893a4b39cea6b7a3ae889e7c1390b5e8301be08e66d9922`，**64成员实算相等**；独立末审/实际workspace的322源全恢复候选。报告 `1ae2f96c16728a6f0643d0a9fcd6de5af03d56d8b978c90a3b7edee465a37a75` 与原日志并读。

测试实际通过私有open建完整 Storage，原同pool真实SELECT后才单次arm/drop；原Store/Registry和observer clone保持存活。共同poll实际Storage.close与同池close_event，确认is_closed且close仍Pending后才放开真实after_release。不是构造另一个pool.close来制造到达，也不依赖首poll假设。

首次close结果在任何额外清理调用之前采样：size与DB/仍存在WAL/SHM的share_mode(0)独占结果被保存；后面的直接pool.close只是失败清理，不能修改该Snapshot。没有num_idle=0断言、丢弃clones或删除重试掩盖未闭句柄。清理验证canonical parent/唯一marker/无reparse，仅删除本nonce已知文件与空目录。普通归还及重复实际Storage.close是第二真实对照函数。

| 独立阶段 | 实际结果 | 精确依据 |
| --- | --- | --- |
| 候选首次绿 | 2/2，exit0 | candidate-first-green日志/退出记录。 |
| 仅逆第二close及两注释 | 1红1绿，真实Rust exit101 | storage精确变为`22bd174d573caf41ca342ac0db99dfdae8d1258966972624d2c253b228e5e9fe`，测试始终a982；本人从候选机械撤hunk得到同hash。 |
| 逆改首次Snapshot | size1，DB/WAL/SHM均Win32 32 | 原stderr `9727d191a4bee25fdb3042ce3c002d8165f872068fe23ca6f377af0cdaa337b2`；正常/重复close对照仍绿。 |
| 恢复全部322原字节 | 2/2，exit0，0.08秒 | stdout `8f912b40e258fda34c165953661a6376a51469ee48e7999888cae5b755950a66`；actual result JSON `104a1ffdbdc0da424b6f5f116f2a48cc6871c877241335608925287d74a885ca`，host_error=null。 |

两项执行只覆盖该target过滤，**513项未运行**。开发红至最终测试只有observed→close_result局部重命名，本人机械反算与固定红测试全文相等，原oracle无改。开发最终2绿、fmt/clippy日志与退出记录均已实读核hash；独立格式0，不把开发strict记录冒充独立重跑。Root合入后适用组合定向和机器配置检查另行负责。

独立首次PowerShell `*>`被owner脚本ErrorActionPreference=Stop截断，只running2的宿主中断日志不计有效红。随后Node捕获真实exe stdout/stderr/exit101，宿主自身预期匹配exit0分开记录。中断nonce先独占检查再按canonical/marker/非reparse精确清理，日志保留；最终两phase目录本轮实读均为空，无数据库残留。没有将该包装错误归产品bug。

## PC05：原 Shared 关联的实际同源复测

追加入口 `pool-close-shared-v1-delivery.json`：`5e4e8ea888a2ea05cb2b42fcd0b274d3ef5f12cd291143190790b04601687d34`。

- freeze `ca1c3a522b615f5ebeee5cab06de04c0398e9f3e475d374de64996301e3ac21b`，**104成员全hash相等**。
- Shared作者固定component-wip2b manifest：`bf6f256f57fb8145def564633c6cf546da6a081111ef9e877112e794da59b603`。独立336源全部与输入/实际副本相符，91原日志及3Storage输入实算相等；storage2598/testa982/锁87与候选相同，未修改其断言或生产源。
- report `7e60389e2dcf63b3d0841bacfa91f457377b9267b1a983edc2c898191f87b900` 必须连读不可回写的勘误 `9d228e6bcf99d947bcc679fd001578442057fe1c0fe8241c634ba162356891ee`：原文filtered503错误，**原stdout实际515**。
- 原stdout `126fd9f60f3d8a21b7ee77d857a03a3e14134120288aca891a8f7e8ffde8469b` 实际 **17/17，0失败/忽略，515 filtered，2.84秒**；actual results `3f4b3c5c29e7fb4ee7b9a743e17d8d7ac885db45c2f98e95e5789551b7c200bd` 为exit0/host_error=null。自有temporary为空。

亲读关联fixture实际NodeRuntime::start、真实IPC PairClaim/AuthProof/current policy路径、持原pool clones的关闭断言：请求stop并await原Node task成功join，pool.is_closed/size0，再canonical/marker/递归无reparse核验，并对文件share_mode(0)独占成功后首次清理。有关执行函数包括real_pairing_authentication_revoke、local_issuer、actual_local_peer_credit_flush_seals_and_revocation及formal_auth_fixture等，确在该17函数原日志内。没有用第二次清理调用或删掉文件锁断言变绿，Shared关联活动源没有新增DEBUG诊断输出；旧ACP DEBUG canary测试不属于该诊断。

这证明原关联在本同Storage修复上实际通过，满足PC05；**没有在Shared副本逆改hunk，不额外推断所有OS32均同根因**。其余Shared facade、正式83、Native80、生产WSS/Native资源工厂仍不在本执行范围。

## 合入及后续边界

允许Root只应用上述固定两路径patch：合前核原321前像与patch --check，合后核322批准字节，并按其实际组合运行定向/适用检查，保原红与更正证据。Native作者正改Storage factory，之后只按最终单hunk串行重基准，不能覆盖其构造/关闭所有权。

STORAGE-POOL-CLOSE-001以此候选的静态与独立PC01–05证据关闭；Root组合尚待实际证据，不因组件Closed记整个change完成。无须重复本轮已固定长矩阵，也不将17或2个函数总数混充80/83正式验收。
