# schema4纯布局/迁移独立验收

2026-10-31。结论：限定的 CL-004 数据库子范围独立验收通过，新 schema4 K组 **11/11**，原 schema3 意图适配后的回归 **13/13**，合计 **24 passed / 0 failed / 0 ignored / 274 filtered out**。最终 lib/tests clippy `-D warnings`、三处资产 rustfmt 检查及lib差异空白检查通过。未发现本轮范围产品缺陷。

**这不是完整CL-004、审批owner或审批产品验收。** Observation、人工决定、持久claim资格、ReplyCell、实际响应writer、安全cancel、attempt/投影/result的应用事务一致性、权限/生命周期/WSS/真实Harness全部未接，仍not-run；数据库raw SQL事务不代签应用实现。A–J剩余前置及owner整体编码门禁不由本报告解除。

## 版本与输入

Git HEAD `f6d1f8df63673eeebd6109d4ddb3e95e5b66b906` 加当前未提交集成文件；使用逐文件SHA固定版本，不以HEAD替代工作区内容。依据 approval-owner-definition.md K组、schema4-layout-definition.md，以及合同fe621e48版§3A已通过的纯布局/固定hook范围。实际执行时合同完整文件SHA为 `40b3201436331e848dd8b0b70e50e90367b68e888bf9bfa4d31c964b79bad4fa`（含限定放行状态说明）。

执行前后冻结生产文件和依赖锁完全一致：

| 文件 | SHA256 |
| --- | --- |
| session_storage/schema.rs | fcec6f7e929056bdf7809534c7f9566c7bbe77f5ebb0c2a47be1a00e06564d18 |
| session_storage/product_schema.rs | e3d2d32c0e3ec32f1c6ef4ed4a971f31c4f203bd8ca6d67063f81b4f14aa6c00 |
| storage.rs | e837ffd3f8a4754ccb4be279aede3ff2483089aec40a2785fa89583a5eaa5be6 |
| product_schema/tests.rs | 5474fa85089a02d630ed8ffdea95a08f9d2a6131b690f97d598e94f32ae57e32 |
| product_schema/delivery_tests.rs | 34822fbb82488901297734709410cf4b9e888644d31bc665efd09b3c7f19fc17 |
| Cargo.lock | 87f27b619281b37015c79ceae8d95471c8a4de50f202f4dd8a6d99fa0d27e324 |

完整前后清单分别 `schema4-implementation-before.json` 与 `schema4-acceptance-source.json`。tester没有修改生产业务源码、产品DDL或开发测试。只按授权新增 `app/tests/acceptance/schema4_migration.rs`、lib.rs的cfg(test)模块声明，及窄适配原schema3验收资产。

## 独立测试资产和原3证据归属

- 最终新资产 `app/tests/acceptance/schema4_migration.rs` SHA256 `9c82a6e1964d0648d93ec0cdbc6397eacb915fe8fd0a0af9ee598a561078a56c`；外部同名文件为逐字副本。最初候选447506...保留为schema4-pre-execution-candidate.rs，不重跑旧生成器覆盖最终资产。
- 原3资产窄适配后 `app/tests/acceptance/schema3_migration.rs` SHA256 `501c45a597b8a423ab4936e4dc4271e508eeb8c8e79d8fa1a02ead3908e173a5`。正向目标使用SCHEMA_VERSION，当前非control管理表9→10、STRICT产品表10→11；“schema3缺meta/control/布局”负例改为直接构造pinned真3，避免被新版Storage.open偷偷换成4。其余13项原有意图保留，没有删WAL/FK/只读/正文隔离义务。
- lib.rs本轮只追加新cfg(test)模块声明，最终SHA `f13a23576f46c510e60644c7de56ba6fa865738f1d1ce0f64fc01aff6cb2069f`。
- 原3测试逐字快照 `schema3-before-schema4-test.rs` SHA `2fb2e326088eb107bcd05ae6a949238c9a073803e64a2f6357886926a5d9dc91`，其原13项日志、R003首轮失败/最终报告不改，不将新版本回归结果回填旧证据。
- schema1/2使用原f6d1f8df CORE/SESSION字面量；真3在其上直接执行固定原3DEFINITIONS并插入双version=3和原控制行/管理记录。固定原3product文件重构SHA精确为 `6243339225616835236c2348d357c432620c9d1d15195e40b06f6d73d84755ba`，保存 `schema3-pinned-product-original.rs`。没有用新Storage.open4再伪降版本造夹具。

## 实际命令、环境与日志

Windows NT 10.0.26300.0；rustc 1.98.1 (48a229cea 2026-09-01)，cargo 1.98.1。工作目录D:/Project/acp-remote，独占tester target。运行以下owner前置包装（只影响本次子PowerShell执行策略，不改机器/用户策略）：

```powershell
powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File 'C:/Users/jack/.codex/visualizations/2026/10/30/01a18dc1-45b2-7f82-981e-964264221cef/deliver-codex-session-loop-tester/run-schema4-owner-final.ps1'
```

脚本实际dot-source仓库testing/ci/windows-owner.ps1，成功输出`CI new-object ownership matches the process user.`，然后设置独占CARGO_TARGET_DIR并运行：

```powershell
$env:CARGO_TARGET_DIR='C:/Users/jack/.codex/visualizations/2026/10/30/01a18dc1-45b2-7f82-981e-964264221cef/deliver-codex-session-loop-tester/activity-target'
cargo test --manifest-path app/Cargo.toml --locked --offline --lib cl004_ -- --nocapture
cargo clippy --manifest-path app/Cargo.toml --lib --tests --locked --offline -- -D warnings
rustfmt --check --edition 2024 --config skip_children=true app/src/lib.rs app/tests/acceptance/schema3_migration.rs app/tests/acceptance/schema4_migration.rs
git diff --check -- app/src/lib.rs
```

最终 `schema4-owner-final.log`：24/24，退出0，用例执行20.71秒，274filtered（不是忽略必需项）。最终 `schema4-clippy-final-after-history.log`：退出0。原始 `schema4-owner-attempt1.log` 首轮24/24保留；首轮测试自身8项clippy风格告警（数字分隔、相同match、明确SQL矩阵helper参数与分组长度）保留在schema4-clippy-attempt1.log。仅整理测试写法/有说明的窄lint允许，不改产品或判据；后续增补“仅新attempt事实缺meta”分支，再跑最终24/24及clippy。中间clippy成功日志schema4-clippy-final.log也保留。

PowerShell将cargo stderr进度行包装成NativeCommandError外观，cargo实际exit及测试统计均为0。git仅提示未来换行转换，没有空白错误；未改Git配置/索引。全部数据为外部tester根下合成副本；两个fixture目录最终剩余子项均0，独占测试连接/目录锁随owner Drop关闭；没有用户库/密钥/配置访问，没有真实审批/Harness/浏览器/Web audit或网络外发。

## 新K 11组实际覆盖

| 组 | 实际结论 |
| --- | --- |
| 1/2/3非空升级与4重开 | 三种原始副本均到双version4，所有既有定义与行保留（版本更新除外），完整integrity/FK检查通过。真3含2输入命令+Run、4审批决定命令、6有/无Run审批、5类delivery状态、24个原stage槽；epoch/floor完全保留，新attempt为空、不从旧未知/已写行猜actual；重复重开不变 |
| 空bootstrap | 无meta空库控制行只生成一次合法epoch/floor0，4重开不制造身份、不换epoch |
| 固定故障 | 1/2/3各跑Ddl/Product3Ready/DeliveryIndex/DeliveryTable/Metadata/Pragma/Validation/Commit，共24个错误边界；所有旧事实/定义/双version完整回滚，无新对象残留，可正常重迁 |
| 真COMMIT拒绝 | 1/2/3各跑固定DeferredCommit，SQLite真实deferred FK错误787，全部回滚；不把precommit hook当引擎COMMIT失败 |
| 真COMMIT成功但响应未知 | 1/2/3各跑CommitUnknown，初始化返回错误，但故障pool尚未关闭时独立连接确认完整durable4与全部旧事实；重开4成功，不错误声称回滚 |
| 只读拒绝 | 真3夹带部分4索引/表、矛盾/未来version、4缺新索引/表/旧索引/control、无CORE事实仅product_source、仅floor>0、仅新attempt历史缺meta均拒绝；DELETE对照主DB/key逐字不变，WAL另独立覆盖 |
| NULL/格式/类型 | 72组合planned/option/actual/option/transport真值矩阵与期望一致；显式NULL漏过负例被拒，lowerhex64/长度/枚举/安全整数边界与STRICT错误类型验证通过。原13回归继续检查旧元数据/审计shape及无Run FK |
| FK/唯一/长度边界 | 真实两审批/两个delivery claim跨配、无claim/无approval、重复approval/claim拒绝；21/23字符拒绝；22个非canonical字符在SQL可存且领域EntityId解码拒绝，明确此处SQL未做canonical强校验 |
| raw SQL事务 | 同事务attempt+delivery更新后发生约束错误，显式ROLLBACK恢复全部快照。只证明数据库事务，不证明未来应用attempt/投影/result编排 |
| Windows WAL拒绝侧文件 | 实际新增WAL为0字节，主DB/key不变，只允许SHM/WAL非权威侧文件；owner-only ACL/SHM继承实测，第二目录锁取得拒绝，原owner释放后可取得 |
| Windows未checkpoint WAL | 固定DB+非空已提交WAL副本中的version77与marker独立read-only真实可读，preflight本身UnknownSchema77，原DB/WAL精确字节保持；ACL/独占锁实测，不用immutable忽略WAL |

固定hook证据边界保持：Metadata/Pragma在对应写前；Pragma不是PRAGMA执行时故障。Commit是调用前；DeferredCommit才是真引擎失败。CommitUnknown真实提交不能记回滚。SQL claim_id只验证length22/非NULL/唯一与关联，规范128bit随机发行/字母表解码属于后续domain/API。

## 未验证与交付限制

新表仅提供metadata布局，本轮未发行Observation、未执行用户审批、未创建真正危险permit、未让selected/safe cancelled共用ReplyCell，也未测试真实writer的planned→actual回执。未签全内存1/8/32MiB账本、actor撤销/逐块读围栏、请求exp与原生有效期、两pool应用裁决竞争、10000命令/审计容量、应用result更新故障及最终reconciliation；这些仍在审批owner/管理后续阶段独立验收。真实Windows进程树/Harness/Web端到端另验，不因本轮数据库通过改为完成。

持久摘要可复制至plan：固定schema4生产清单如上，CL-004纯布局K11及原意图回归13共24/24独立通过；真实COMMIT失败与成功未知、WAL/ACL/lock已执行；只改tester两文件和cfg(test)入口；原3证据保留；应用owner及完整CL/L5/L6仍not-run。
