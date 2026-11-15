# CL-004 / 任务2.3 独立迁移与布局验收

2026-10-31。当前结论：**阶段受阻，不能签通过**。独立真实SQLite验收已运行；初轮10测试中7通过、3失败，失败均为WAL模式拒绝路径新增WAL/SHM文件。随后建立固定DELETE-journal对照并补WAL主库字节检查，11项通过；原WAL清单义务另外保留为主动测试，单独运行仍失败。独立reviewer将其登记为SCHEMA-R003 P2阶段阻断，待原提出者复核关闭，不能以DELETE对照或主库字节不变代替WAL完整预检义务。

## 快照与资产

测试资产：主仓库 `app/tests/acceptance/schema3_migration.rs`，由父执行的 `lib.rs` cfg(test)路径模块编入库单元测试；不暴露生产破坏入口。所有SQLite副本在tester专属外部 `sql-fixtures/` 下创建，每分支独占子目录，连接关闭后清理；未触及用户数据库/凭据，未启动节点或Harness。

fixture旧CORE/SESSION完整DDL从 `git show f6d1f8df:app/src/session_storage/schema.rs` 固定并内嵌测试，未引用当前产品DDL常量，也未先建schema3再伪降版本。原抽取来源存 `schema2-original.rs`。schema1与schema2分别从原始DDL建立合成完整副本；schema2填入非空旧正文/未知结构/Run/审批及交付/审计关系。类型保真的有序SQL quote/BLOB投影与旧对象定义前后比较，身份哨兵字节单独核对。

首次执行已使用SCHEMA-R002修复后的schema.rs `9a80c9e01cb67396eef4bf53eff7d7b3c0b4b1e86373c5b4da4da19dd1c33b2c`；其他业务文件对应父冻结product_schema `624333...`、mod `0420ba...`、storage `e1d66e...`、error `3554d9...`、node `f11a4a...`。lib仅增加测试路径模块。详见 `schema3-acceptance-run-start.json` 及后续快照；此前 `schema3-acceptance-inputs-before.json` 的lib摘要在父新增路径模块之前，不冒充正式执行快照。

未在SCHEMA-R002修复前的旧schema `25572e...` 上执行独立负例；该旧版本的缺metadata缺口来自独立reviewer静态检视，不编造旧失败日志。

## 实际命令及结果

设置 CARGO_TARGET_DIR 为tester独占 `activity-target/`，使用产品锁与离线依赖：

`cargo test --manifest-path app/Cargo.toml --locked --offline --lib cl004_ -- --nocapture`

| 执行 | 实际结果 | 日志 |
| --- | --- | --- |
| 初次完整运行 | 10实际测试：7 passed、3 failed；退出101 | schema3-acceptance-attempt1.log |
| 增加DELETE对照及WAL主库字节检查后 | 11实际测试：11 passed、0 failed；退出0 | schema3-acceptance-test.log |
| 原WAL文件清单义务单独保留主动测试运行 | 1实际测试：0 passed、1 failed；退出101 | schema3-wal-inventory.log |
| 最终完整资产再次运行 | 12实际测试：11 passed、1 failed、0 ignored；退出101，仍是WAL文件清单负例失败 | schema3-acceptance-final.log |

WAL单例命令过滤为 `cl004_wal_readonly_rejection_preserves_file_inventory`，其他参数相同。最后资产共12个主动测试，不把11项结果误称12/12通过。最终测试资产SHA-256为 `c85d5a632b54efab0ab8fd4416f2ff72948be29c7a0f220384509c52996b584c`；最终运行后schema仍为上述9a80c9...快照，其他输入见 `schema3-acceptance-current-source.json`。PowerShell将cargo stderr进度装饰成NativeCommandError，以上状态均取真实进程退出码及Rust测试摘要。

测试资产首轮clippy发现3个测试自身格式告警（数值分隔/分号），已修复且未改变预期，原输出保留于schema3-clippy.log。最终 `cargo clippy --manifest-path app/Cargo.toml --lib --tests --locked --offline -- -D warnings` 退出0，日志schema3-clippy-final.log；测试单文件rustfmt检查及git diff --check也退出0。静态通过不关闭WAL运行失败。

## 已验证子范围

- CL-004 A/B：原schema1与2实际升级成3，双版本一致；旧对象定义、身份/公钥/密钥引用、客户端信任/撤销代数与时间、旧命令/审计/revision及schema2旧正文/审批完整逻辑内容不变；产品表除唯一control外为空，没有自动转换旧Run/裁决；合法request_epoch、初始floor0及再次重开保留epoch/已存floor。
- CL-004 D：两旧版本各自五个cfg(test)事务检查点实际回滚，使用独立真实连接核对原双版本与完整旧事实、无product半成品，随后显式打开能成功。PRAGMA前hook只证明该事务边界错误回滚，不称引擎PRAGMA语句失败。
- 真实COMMIT失败：两版本自有副本用延迟FK和metadata更新trigger在真实SQLite COMMIT触发引擎错误787，明确匹配FOREIGN KEY失败；不是提交前假错误，重新打开原双版本/旧事实完整且无product结构，去掉自有故障trigger后可正常升级。
- 拒绝与R002：DELETE-journal稳定副本上的未知版本、两方向版本矛盾、缺core表/约束/node_state、部分product对象、缺旧/新索引/新表、control缺失/非法epoch均拒绝且主库字节/身份/文件清单不变。schema1/2/3有历史但缺metadata拒绝；CORE三类事实分别单独存在、仅SESSION harness事实、仅product_sources事实、仅floor>0亦拒绝；合法无身份空bootstrap可重开。
- 布局与SCHEMA-R001：合法同target/load且run=NULL审批实际插入通过；无Run跨目标load与有关联Run跨目标均由真实FK拒绝，不依赖业务伪检查。资源被审批引用时删除拒绝。
- STRICT与审计槽形状：非整数及越界水位/代数/revision/协议值拒绝，非法期限拒绝；10张product表均STRICT。四阶段槽可单独保留未发生空值，非法stage/半填状态/不足90天保留拒绝，90天精确边界通过。仅证明结构，不签四槽受理事务/容量/不可覆盖API。

## SCHEMA-R003待审证据

同一个已关闭且主库头保留WAL模式的自有schema3副本，先删metadata以造成必须拒绝的条件，再调用公开Storage::open：确实返回MissingNodeMeta，主库和合成identity字节不变，但文件清单由 `[identity.key,node.sqlite3]` 变成 `[identity.key,node.sqlite3,node.sqlite3-shm,node.sqlite3-wal]`。原始失败清单见两个失败日志。当前 `read_only(true)` SQLite连接仍产生这类sidecar；是否满足冻结的“只读拒绝且不新增sidecar”必须由修复或明确规范复核解决。

DELETE-journal对照是在测试调用产品之前显式固定自有fixture，不是更改产品预检；原WAL义务没有删除、skip或改为通过，也未使用immutable绕开已提交WAL。新增WAL主库字节检查仅证明有限字节不变，不能证明已提交但未checkpoint WAL内容正确读取。修复复测须新增/覆盖这种真实WAL来源，并遵守真实目录锁/无writer或固定快照前提，不能只看主DB或忽略WAL数据换绿。

## 尚未验证与交付界限

SCHEMA-R003未闭环，任务2.3整个限定阶段不能通过。完整CL-004的受理/单次派发/输入消费、容量10000/100000、四槽事务与最终reconciliation消费、水位CAS与隔离恢复、无Run审批来源/交付，以及新入口运行时从不读写旧正文均未验证。没有新入口时不以空表代签其隔离行为。SQLite哨兵不是DPAPI/ACL及真实认证恢复证据；真实Harness与Windows进程树未执行。
