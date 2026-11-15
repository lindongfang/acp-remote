# 旧正文/强刷路径构建隔离独立验收

2026-10-31。限定独立范围通过：**13个旧入口普通依赖负编译符合预期，15次新API正向控制成功；旧56项逐名保留、0ignore且实际全部通过；child父用例精确运行1/1通过；schema迁移24/24、管理输入31/31通过；普通cargo build成功。** 没有发现本轮隔离范围产品缺陷。全仓最终check由主执行者在本组合随后执行，本报告不代签其尚未提供的结果。

这是生产构建可达性隔离与回归结论，不是完整CL-001–004、整个change、实际WSS/Web/Harness或审批owner通过。旧业务仅为cfg(test)历史回归，其强刷/必须Run语义未重新变成新产品规则。

## 依据、冻结与独立性

依据13入口版legacy-isolation-definition.md SHA256 `dea0fe2b883e5a5f958ad0de0a656de3a101c29e49316907f80e997f4f79150c`、合同77521442版机械白名单；执行时合同完整文件SHA为 `ed47c648264e4f0e2f926ddd60e02a63cb04b246344d8face6c93fc4e166059e`（含后续阶段状态说明）。原11入口定义文件与原SHA78247c55...另行保留，不回改其归属。测试前后8个冻结源码摘要一致：

| 路径（app/src/） | SHA256 |
| --- | --- |
| application/mod.rs | 2a47d4fdf0e1c5740ff7628fe3f10852bfc06025c19fbc01001fa68a94f41651 |
| session_storage/mod.rs | 638996f3b2ef85a342e3d9677979c9dc789a5c08dec062095bd9243f2c346f7d |
| session_storage/legacy.rs | b77c0c0d62fccdeef9b504033af4080b05bc05937777d95a95e290eaa253ce00 |
| session_storage/history.rs | f428f5f3d85b6c27c67ecace678f3d8daf6df8c9aac33c69b2bad105d497d6dc |
| session_storage/approvals.rs | 82ad5a9bd709f76f7a88391549dbcd2ebc47562cc35437086b671c6cb9c6da11 |
| session_storage/history/tests.rs | dba91167488a438d8b67cb236fbd946adbc9db7bd2880d33e89ce8fba4a2b403 |
| domain/mod.rs | d42675796ef54f73642a564d0a65eebaf9829685d64c987b4b439de934743c3b |
| storage.rs | cfaf7e77610f57de91ffb0cddc8df6ee48a0b4474adb49c5a00126d8045c11bf |

完整清单为legacy-isolation-frozen.json与legacy-isolation-acceptance-source.json。schema.rs仍fcec6f7e...，product_schema.rs仍e3d2d32c...，Cargo.lock仍87f27b61...；本tester未改仓库产品/测试/配置/锁/索引，只在独占外部目录创建probe、脚本与证据。

## 13项普通依赖负编译与正控制

外部crate `legacy-boundary-probe` 是正常path依赖acp-remote；独立workspace、无额外直接依赖/feature/patch。实际命令由run-legacy-boundary.mjs逐项运行并完整登记legacy-boundary-results.json：

```powershell
node 'C:/Users/jack/.codex/visualizations/2026/10/30/01a18dc1-45b2-7f82-981e-964264221cef/deliver-codex-session-loop-tester/run-legacy-boundary.mjs'
```

每次cargo check使用`--manifest-path <probe>/Cargo.toml --offline --lib --message-format=json`，首次完成自身锁解析后全部加`--locked`。没有`--cfg test`、include源码、把依赖作为test模块或重新启用旧API的feature；正向probe还用cfg(test) compile_error防止伪测试构建。独占target为原tester activity-target。

| 编号 | 被拒绝的旧入口 | 实际诊断 |
| --- | --- | --- |
| N01 | Storage.sessions() | E0599 |
| N02 | Storage.histories() | E0599 |
| N03 | Storage.approvals() | E0599 |
| N04 | application::InputRepository | E0432 |
| N05 | application::InputService | E0432 |
| N06 | application::history::HistoryCoordinator | E0432 |
| N07 | application::approvals::ApprovalService | E0432 |
| N08 | session_storage::SessionStore | E0432 |
| N09 | session_storage::history::HistoryStore | E0432 |
| N10 | session_storage::approvals::ApprovalStore | E0432 |
| N11 | session_storage::legacy::SessionStore | E0432 |
| N12 | domain::history::HistoryItem | E0432 |
| N13 | domain::approvals::ApprovalBinding | E0432 |

各负向cargo exit101，每个恰有目标API错误，primary span落在本probe src/lib.rs，message匹配该目标；没有语法/缺依赖/无关类型/lint/链接错误。不是以任意非零退出码当成功。每项之后立即恢复正向源，13次均exit0，另初始/最终各一次成功，共15次正控制（28次cargo check）。

正控制实际导入EntityId/ActorContext、新native/activity/requests/views类型、InputOwner/MemoryInputs/ProductInputService/ProductRepository，并编译`Storage.product_commands() -> ProductStore`工厂和EntityId构造。证明新正常库仍可使用，不是把全库cfg(test)掉。每个source SHA/完整编译JSON/标准错误保留于probe内对应Nxx.rs/jsonl/stderr.log；活动src/lib.rs最后恢复为正向源。

legacy-probe-lock-comparison.json：产品锁261 package，probe可达图238 package（未使用的dev依赖被裁减）；唯一新增是probe自身0.0.0/path，无新增第三方name/version/source，不修改产品锁，无网络访问。

## 原文搬迁与两项准备期差异

check-legacy-move.mjs及legacy-move-comparison.json保留全部比较。8个隔离文件符合机械白名单：application旧项加cfg(test)、Storage仅三旧工厂加cfg(test)、新session mod仅生产管理声明与test legacy/re-export、legacy旧业务体完整保留（外加super::schema和模块path适配）、两个SQL文件只增真实tests path、domain两旧声明只加cfg(test)、history子进程只换真实全限定名。比较时仅统一CRLF/LF用于文本块比较，没有泛化忽略空白/注释/SQL。

29项直接比较符合；准备基线后有2项完整文件SHA不同，原始不一致记录保留，没有伪写相同。逐hunk人工复核见legacy-reviewed-baseline-deltas.md：ProductStore只重排cfg(test)两个测试模块声明；管理独立测试为其原tester的clippy命名/窄lint/等价bool转换与or_else整理。没有生产SQL改变、断言削弱或新增ignore。本轮重新运行当前31项，不用准备期或移动前日志代签。其余schema/DDL、新应用输入、签名适配器、新领域/视图、Cargo配置/锁及旧业务断言源码均保持对应固定内容。

首次搬移因物理test路径及生产旧域dead_code的编译失败属于前置机械问题，未记作产品功能失败；本轮在修正并冻结之后独立实际重跑。没有用allow(dead_code)保留生产旧域。

## 旧56项、child、迁移24与管理31实际运行

使用run-legacy-regression.ps1，通过一次子PowerShell `-ExecutionPolicy Bypass`运行（不修改机器/用户策略），先执行仓库windows-owner.ps1，实际输出owner匹配进程用户；随后在独占target执行以下分组。命令/exit/日志全列于legacy-regression-results.json。

| 分组 | 实际数量 | 原始日志 |
| --- | --- | --- |
| application::history::tests | 22 passed / 0 ignored | legacy-old-application22.log |
| session_storage::legacy（store12+history12+approvals10） | 34 passed / 0 ignored | legacy-old-sql34.log |
| child父用例全限定--exact | 1 passed / 0 ignored | legacy-child-exact.log |
| cl004_迁移组合 | 24 passed / 0 ignored | legacy-schema24.log |
| ProductStore独立管理组合 | 31 passed / 0 ignored | legacy-management31.log |
| cargo build正常库与二进制 | exit0 | legacy-normal-build.log |

测试均用`cargo test --manifest-path app/Cargo.toml --locked --offline --lib <filter> -- --nocapture`；child加`--exact`。普通构建用`cargo build --manifest-path app/Cargo.toml --locked --offline`。实际库测试list有366项；这里只签表内独立子范围，不把filtered当已运行。legacy-old-tests-runtime.json将准备期56个原函数逐一映射到实际list：全部56存在、0ignore，没有以新增测试凑数。实际运行旧22+34均通过。schema24和管理31分别为当前隔离后组合，不借用移动前结果。

child父进程未设置ACP_WP2_HISTORY_CRASH_ROOT：包装脚本启动时显式检查，若已设置就失败而不运行。原父用例仅全限定子filter改成legacy路径，exit93和`VISIBLE_POSITION=1`以及next_position65/仅已提交输入等assert逐字保留；实际父用例在--exact下1项通过，说明其真正spawn的child通过上述断言，而不是零测试exit0。child stdout由父用例捕获用于断言，不声称原始父日志单独打印了marker；证据是未削弱的断言源码+实际执行成功+父env无早退条件。父用例10秒超时杀/收尾原逻辑保留。

管理31中`m_new_path_never_changes_legacy_body_or_security_metadata`实际通过：原合成body/旧管理/审计记录保持，Turn/Item/旧session命令/history pin无新增，未知结果不重派，关闭自有库后新BODY marker不出现在SQLite文件。迁移24继续覆盖真1/2/3非空→4、双版本/身份撤销/原管理与旧正文保持、真实COMMIT拒绝/成功未知、完整WAL及私有目录边界。它们不证明实际产品审批或外部Harness行为。

## 资源、未验证与交付摘要

全部crate源/诊断/报告在tester独占外部根；Rust运行是自有临时SQLite/合成密钥和端口/owned child，没有用户实际数据库、配置、私人文件或真实会话读取。各cargo/Node进程已结束，无后台服务；probe与锁/target作为持续验收资产有意保留，活动probe源有效。没有Web audit/浏览器操作/依赖元数据外发；未改全局执行策略/环境/索引。

本报告不重复执行全仓最终all-targets/fmt/clippy和整个产品验收；主执行者随后对同组合完成总检查，须另附证据。未知Harness/Node网络接线、审批owner、权限撤销/引用逐块交付、完整安全清理CL-080–084和真实L5/L6均不由构建隔离替代，仍按各阶段spec验收。

可写plan的持久摘要：隔离8源冻结前后不变；13普通依赖旧入口精确拒绝+15新API正控制，旧56全名0ignore并22+34实际通过、child父exact1通过、schema24/管理31/正常build成功；旧SQL业务与DDL保持，准备期两项测试格式差异单列复核；仅本阶段独立子范围通过，全仓最终check及真实产品未代签。
