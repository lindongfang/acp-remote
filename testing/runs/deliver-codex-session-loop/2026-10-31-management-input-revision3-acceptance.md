# 管理候选 revision3 独立复验

结论：在冻结的外部revision3候选上，原29项管理测试及MGT-CODE-003新增2项测试共31项全部通过，0失败、0忽略；同版21项原公开模型回归通过，外部类型负向编译11项拒绝符合预期，生产lib clippy通过。MGT-CODE-001–003的独立执行失败均已复验关闭。本结论仅涵盖已执行的L3/L4子范围，不签主区schema4集成、真实产品接线或完整L5/L6。

## 固定输入与资产一致性

实际读取HANDOFF.revision3.md、修订源码及冻结manifest，SHA256=`0e116d1f4a191d336298be79137921007beac25743317931016b3b9b1419dcae`；delta=`0d628e843583334be5f1f9d9e89dd26b33a76bdba53fe5e7197c3bd22ace9734`，full patch=`29bf3f49ad34883728efe05d34ba2ad0141d630781d453acbfaa7ad8a17aa492`。独立逐一核验78份实际源，执行前后均匹配；见`management-revision3-source-initial.json`和`management-revision3-source-final.json`。

新副本为本目录`management-revision3-copy/`，原1/2/3冻结候选及失败日志保持原归属。副本对候选源码唯一差异仍是SQL模块末尾三行cfg(test)include，见`management-revision3-test-overlay.diff`；生产逻辑未改。验收源从revision2独立资产逐字复制，预期不改：

| 独立验收源 | SHA256 |
| --- | --- |
| management_independent.rs（原29项及child声明） | f36d97eda8e326a6dbf7bd29c7c3a07ed0e8b08709f432470e6a2d202d1438e9 |
| management_deadline_semantics.rs（003两项） | 124890c8423b9537d6927ce3622c26d5b99bdf892bd98170b9f66d263f0ed4aa |

两个版本的相同字节证据在`management-test-unchanged-revision3.json`，最终资产清单在`management-revision3-test-assets.json`。原001的500ms剩余/700ms真实等待、002的shared fault暂停、003的10.3秒操作预算及500ms对象TTL预期均未为实现迁就。

## 实际环境、命令与结果

Windows，rustc/cargo1.98.1；使用独占`target-management`，原产品Cargo.lock（SHA256=`87f27b619281b37015c79ceae8d95471c8a4de50f202f4dd8a6d99fa0d27e324`），offline执行。正常测试/检查带--locked；故意失败的外部crate从产品锁种子offline解析仅本地根包，无新增registry依赖。测试为自有真实SQLite及合成ES256、actor/owner/结构校验/时间/writer端口，不读取用户数据库、身份或秘密，不运行Harness/WSS/Web，不外发Web审计。

```powershell
cargo test --offline --locked --manifest-path <本目录>/management-revision3-copy/app/Cargo.toml --lib independent_acceptance -- --nocapture
cargo test --offline --locked --manifest-path <本目录>/management-revision3-copy/app/Cargo.toml --test session_loop_acceptance -- --nocapture
cargo check --offline --manifest-path <本目录>/management-negative-revision3/Cargo.toml
cargo clippy --offline --locked --manifest-path <本目录>/management-revision3-copy/app/Cargo.toml --lib -- -D warnings
```

| 检查 | 实际结果 | 原始证据 |
| --- | --- | --- |
| 独立管理矩阵 | **31/31 pass**，exit0，15.78秒；297个非本目标库测试过滤，不计执行 | management-independent-revision3.log |
| 原公开活动/请求模型回归 | **21/21 pass**，exit0 | management-revision3-model-regression.log |
| 外部类型负向编译 | cargo exit101为预期：10处E0277、1处E0423；无Clone/Serialize/Deserialize、无裸ActorContext转换及私有构造器 | management-revision3-proof-negative.log、management-revision3-negative-classification.json |
| 生产库clippy | exit0，仅--lib，不声称all-target或独立注入测试已过clippy | management-revision3-lib-clippy.log |

退出值汇总`management-revision3-final-exit.json`。故意失败的负向crate不是运行故障，也不计为11个Rust单测。PowerShell对正常cargo stderr的格式标签不替代真实退出值。

## 问题关闭依据

| ID | 原失败 | revision3同预期结果 |
| --- | --- | --- |
| MGT-CODE-001 P2 | 保留ticket跨请求exp后正文55字节/1对象仍被保留 | 原用例通过：retained=(0,0)，对象真实deadline与supervisor清理生效；没有通过丢ticket才清理的方式规避原断言。 |
| MGT-CODE-002 P1 | final_fence COMMIT后共享fault再恢复，writer=1且dispatch成功 | 原用例通过：writer=0、dispatch拒绝，共享fault不清除；同矩阵中可能已写后的失败仍unknown且至多一次。 |
| MGT-CODE-003 P2，操作预算分支 | 有效24h签名因操作10秒预算耗尽被标command expired | 原用例通过：同步最终采样延迟10.3秒后，原签名仍可验证，writer=0、正文0/0、command failed，返回值不再是产品请求Expired。 |
| MGT-CODE-003 P2，对象TTL分支 | 短body TTL到期的failed命令错误返回产品请求Expired | 原用例通过：最后owner刷新暂停跨500ms对象TTL后，原签名仍可验证，writer=0、正文0/0、command failed，返回值不再是产品请求Expired；代码将其作为独立InputExpired对象错误。 |

实际通过的29项仍逐一覆盖原报告中的事务/floor CAS、两pool并发、必需COMMIT失败与成功未知、单次ticket/claim/传输、当前actor/资源/原生活动、10000合法命令、缩限audit12、不可覆写slot/90天边界、旧正文与旧管理事实隔离、取消/Drop/有界等待、启动unknown等已执行分支。31项逐例名称及ok在原日志中，不以开发37项作为独立证据。reviewer对源码及原提出问题的正式关闭另有独立记录。

## 保持未验证的边界

所有随机/内存分配故障、受理COMMIT后owner关联失败等未逐一独立注入的细分范围仍按`management-acceptance.md`记录，不由31通过补签。默认100000审计自然累积本阶段不可达：仅checked算术及合法缩限SQLite已验，完整CL-083仍待后续。正式清理、恢复新request epoch、最终reconciliation/ready及故障安全控制、SQL审批、真实认证/资源owner/实际ACP writer/WSS/Web、真实Windows节点/进程树及完整L5/L6均未执行。

本外部候选使用其固定schema3来源；主执行随后schema4布局及Storage常量的集成属于不同快照，必须在集成后重验，不复用本报告声明同版通过。未合入主区、未提交/推送/发布。自有pool、task及supervisor均结束或显式关闭；合成数据库和构建资产保留在独占目录用于证据复核，没有用户资源待清理。
