# 旧正文/强刷接口构建隔离独立静态检视

2026-10-31，reviewer `/root/baseline_reviewer`。只读实际冻结实现、独立before快照及已验root4终态清单；未修改产品/测试、运行数据库或真实Harness。

当前结论：**7752合同/dea0独立定义范围内的静态检视通过，未发现阻断。** 旧运行接口的正常依赖不可达、旧56及真实child、迁移24/管理31和完整新组合检查仍待独立执行证据；开发56、正常check和clippy不代签交付。

## 实际冻结摘要

reviewer逐一计算，与交接一致：

| 文件 | SHA256 |
| --- | --- |
| application/mod.rs | 2a47d4fdf0e1c5740ff7628fe3f10852bfc06025c19fbc01001fa68a94f41651 |
| session_storage/mod.rs | 638996f3b2ef85a342e3d9677979c9dc789a5c08dec062095bd9243f2c346f7d |
| session_storage/legacy.rs | b77c0c0d62fccdeef9b504033af4080b05bc05937777d95a95e290eaa253ce00 |
| session_storage/history.rs | f428f5f3d85b6c27c67ecace678f3d8daf6df8c9aac33c69b2bad105d497d6dc |
| session_storage/approvals.rs | 82ad5a9bd709f76f7a88391549dbcd2ebc47562cc35437086b671c6cb9c6da11 |
| session_storage/history/tests.rs | dba91167488a438d8b67cb236fbd946adbc9db7bd2880d33e89ce8fba4a2b403 |
| domain/mod.rs | d42675796ef54f73642a564d0a65eebaf9829685d64c987b4b439de934743c3b |
| storage.rs | cfaf7e77610f57de91ffb0cddc8df6ee48a0b4474adb49c5a00126d8045c11bf |

合同为77521442，独立13入口定义为dea0fe2b；此前ff8b/78247与首次机械编译失败保留原归属。此报告不把初编译路径/旧私有字段错误当成业务断言失败，也不把修复属性自动当测试已经执行。

## 搬迁与门控核对

- 将旧session_storage/mod.rs与legacy.rs实际全行diff，仅模块头、显式approvals/history path、父schema别名及tests物理path有机械差异。reviewer另提取两份从`use crate::{`至最终cfg(test) testmod前的15554字符主体作严格原文比较，完全相同；旧SQL、常量、trait实现、辅助函数与业务方法无变动。父schema别名只保持原引用，不复制或改用另一迁移实现。
- 新session_storage/mod.rs正常只声明product_commands/product_schema/schema；legacy模块与兼容re-export都由cfg(test)隔离，普通依赖不能通过新legacy路径绕回。没有Cargo feature/环境开关或额外production别名。
- application只对旧history/approvals、InputRepository/InputService及对应use/impl加cfg(test)，新product_commands生产声明不变。Storage只对3个旧工厂加cfg(test)，共享ProductStore/fault、身份/信任/目录锁/迁移接口和SCHEMA_VERSION=4保持。
- 两旧SQL文件仅给原testmod加真实history/tests.rs与approvals/tests.rs属性，避免误加载父tests.rs。domain/mod仅旧history/approvals声明门控；根EntityId/ActorContext/纯值与新native/activity/requests/product_commands/views不变。没有allow(dead_code)或方法改写压住原编译问题。
- 唯一旧测试内容变动是child --exact路径从原命名空间改为legacy真实命名空间；exit93、VISIBLE_POSITION=1及数据库区间断言原文未变。旧app/SQL测试主体及名称未删/ignore，运行清单与真实child仍需后续证据。

## 保护的新组合源码

初始legacy before/source.json较早记录过管理验收风格修正/SQL测试声明排序前版本，不能把这两项早于隔离、已在root4审过的差异误算成隔离修改。reviewer已改以实际验过的root4终态78清单逐一对照：只有7个原有白名单文件变动，加新legacy.rs，共8源；当前SQL模块e4d1f446和管理验收f9359628均保持root4终态。

新application product_commands、SQL ProductStore、签名/请求、原生活动/视图模型、schema fcec6f7e、product_schema e3d2d32c、Cargo.toml/lock及已验SQL资产未被本轮修改。迁移仍保留CORE/SESSION旧DDL/行；此隔离只排除旧运行DML/强刷入口，不声称正常二进制里连旧兼容DDL也消失，不删除数据库或原生数据。

新来源/身份/permit依赖方向无反向连接legacy。旧根纯数据类型即使仍可导入，也不能制造新的私有CurrentActor/VerifiedProductRequest/DispatchPermit；普通依赖实际类型/工厂存在性仍按13负向+正控制独立验证，不以静态判断代替编译。

## 交付门槛

独立13个逐入口正常依赖负向及新工厂正控制、旧56逐名/ignore/真实执行、父环境无预置的child exact真实退出93/标记与DB断言、迁移24及管理31移动后新组合回归、正常build及适用完整检查齐备后，才可签构建隔离交付。移前366或外部候选绿不能代签移动后。

任务2.3其他义务、完整CL-080–084安全清理/去重保护/故障收敛，审批owner/WSS/真实Harness与L5/L6仍未签；当前事务阶段不清理不是整个change免除清理。未运行Web audit或读取用户凭据/实际库，未提交/推送。

## 独立执行及最终交付闭环

2026-10-31追加：**旧接口构建隔离限定交付通过；按当前tasks.md的具体范围，任务2.3“增量管理布局并隔离旧历史”可以完整勾选。** 此判断结合已签schema3/schema4布局迁移和本轮实际隔离，不代表任务2.2/2.4/2.5、完整CL或整个change完成。上文待执行为静态初审时状态，不覆盖成旧快照已经运行。

已实际读取独立`legacy-isolation-acceptance.md`（重新计算SHA256 `daf72174408dfa76218ae91dc2812d607513822b6b827e88653e80c40cc572bd`）、boundary/regression/runtime/move/锁比较JSON、普通probe Cargo/正向源及runner、回归包装脚本与各原始日志。reviewer对终态18源再逐一计算，全部相符，8实现冻结值仍为本报告表格，schema/新管理/锁保持。

实际证据：

- 13个普通依赖负向each exit101，准确3个E0599和10个E0432，均只有目标入口错误、primary span指向对应probe语句；不含语法/缺库/链接等假阳性。13次逐项之后恢复正控制加首尾共15次均exit0。probe是独立workspace普通path依赖，无include/测试模块/feature，正控制的cfg(test) compile_error保证没有强制测试构建；实际新Storage.product_commands工厂、新服务/领域类型编译成功。锁对比仅探针自身新增，第三方不新增。
- 旧56逐名全部列出、ignore0，实际旧application22与legacy SQL34均通过，没有用新增测试填数量。真实父用例按迁后全限定名`--exact`实际1/1通过；包装脚本显式拒绝父环境已设CHILD_ROOT，原exit93/marker/DB断言未改，所以不能用早退或零匹配假绿。marker由父捕获用于原assert，没有虚称父日志另打印了child stdout。
- 移动后同组合迁移24/24、管理31/31各实际exit0，普通cargo build也exit0。新输入标记不进旧表/SQLite、旧管理/撤销事实不改及未知不重派分支在31中实际执行。迁移24延续真1/2/非空3→4、epoch/floor/既有审批与资源责任保留、未知版本/结构、真实COMMIT拒绝/成功未知及完整WAL/ACL/锁。
- 原文搬迁/白名单静态结果与独立move比较一致。两份较早准备期baseline中的SQL testmod排序和管理验收等价风格差异按root4已审完整diff单列，没有称为原样相同或掩盖生产SQL变化；本轮对应31又重跑。

主执行随后记录最终`npm run check`与`npm run build:rust`实际exit0；reviewer已读`.tmp/session-loop-isolation-check.log`、`.tmp/session-loop-isolation-build.log`的实际命令及结果：Web类型/构建与50测试，Rust fmt/严格all-target clippy、库366/独立合同69/JWS4/平台12测试全部通过，最终Rust及Web构建成功。本轮是移动后组合，不借用移前全绿；早期工具/机械路径编译失败保留，未计为业务失败或通过。

任务2.3判据现已满足：原始副本身份/撤销/管理/旧正文、请求epoch/floor和既有schema3事实保留，1/2→原3布局→4及3→4增量原子，未知/矛盾/残缺识别与故障回滚有独立证据，新产品运行接口不再可达旧正文/强刷/必须Run链路，新路径不永久保存聊天正文。可以更新该任务状态及其证据链接；不能由此填完成管理清理/完整容量/故障恢复协调、真实审批owner、WSS或Harness验收。
