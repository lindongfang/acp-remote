# WP2 输入事务开发阶段记录

日期：2026-10-30。范围为任务2.1的领域/SQLite基础，不是CL-017–022的Web产品验收，任务复选框暂不勾选。

## 实际实现与集成

在app管理的隔离工作区`C:/Users/jack/.codex/worktrees/codex-session-wp2/acp-remote`基于`65b46c01d6b95fc1f1a8223b1c92a729a885c185`实现并冻结七文件：app/src/lib.rs、storage.rs、domain/mod.rs、application/mod.rs、session_storage/mod.rs、schema.rs、tests.rs。没有新增产品依赖、修改现有安全模块、Web或既有测试，没有提交或操作Git索引。

候选建立schema1→2迁移基础，保留既有节点身份、客户端信任/撤销和审计；以真实SQLite写事务原子保存Command、用户输入/Turn/Run及审计，按actor/request去重或报告冲突，保持单会话前台互斥。派发仅可一次领取并先持久化dispatching；未决恢复标记unknown/interrupted而不自动重派。业务对象、应用入口和存储模块分开。

独立静态复核见[review](2026-10-30-wp2-input-review.md)，独立测试及追加边界见[acceptance](2026-10-30-wp2-input-acceptance.md)。两者均未发现限定层级阻断缺陷；只签当前层级，不代签最终产品。

主执行复制前检查主目录两份原有产品文件与上游固定清单一致，新文件目标尚不存在；复制后七文件分别与冻结候选SHA256一致。主目录现在包含这批七文件；同一固定版本的`npm.cmd run check`实际exit0，Web50、Rust156库+4向量、类型/格式/全目标Clippy/生产Web构建通过；随后`npm.cmd run build:rust`实际exit0，普通Windows exe成功构建。日志`.tmp/wp2-integrated/check.log`（SHA256 `AEBAFEB29F242A17418AD9EF870AD2DAE80EEF0A7338330C9BF1604985A896AC`）、build.log及各自.exit文件，target为同目录target。使用现有CI owner进程前置，不启动用户节点或迁移用户数据库。

build.log SHA256为`88944CD6837EDC9915799FFF85172523CA6441D3E8B64DBA70E522F538940149`。独立tester随后用实际schema2受理库的一致性副本执行旧普通exe，确认明确拒绝版本2且主库/身份fixture摘要不变；只关闭旧程序拒绝子项，不扩写为真实DPAPI升级或可复现构建签名，详见其追加报告。

## 已有验证

- coder在隔离工作区实际报告fmt、差异、全目标Clippy及完整Rust测试exit0：156库测试、4签名向量（新增12项SQLite开发测试）。原始工具输出在该coder会话，未保存stdout文件；本记录不伪称有落盘原始日志。
- 独立tester实际重跑12项开发测试，另有2项独占外部probe通过；检查原子写/真实deferred-FK COMMIT故障、schema升级故障、独立连接竞争、100000容量、16MiB边界、引用篡改和入库后撤销。正式日志、固定摘要和边界在其独立报告。
- 原候选首次完整Rust测试有两项旧trust测试回归，原因是历史检查范围过宽；修复后完整重跑通过。没有修改旧测试来迁就新实现，失败版本不得代作最终证据。

## 保留的实施与验证责任

ActorContext是内部传值，不是可接受的网络授权；WP3仍须由真实opaque actor及client_gate接入。recover_interrupted目前显式调用，启动ready前的接线与真实进程崩溃不重派仍待实施/验证。重开测试不冒称强杀或断电。原生Harness派发尚无实现，测试计数使用明确替身。DPAPI保留以opaque fixture检查，不签真实密钥升级；旧程序拒绝新schema的真实子项另由独立tester补验或保留未执行。

保存、审批和管理注册当前仅有schema基础，任务2.2/2.3继续在隔离工作区实现，主目录本阶段不与其并行修改产品。完整会话交付、L5/L6和任务完成判据仍须后续实现及独立验收。
