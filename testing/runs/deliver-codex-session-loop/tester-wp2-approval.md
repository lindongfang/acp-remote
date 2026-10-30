# WP2 2.3 审批事务：独立验收

日期：2026-10-30。独立tester，同一模型与独立上下文持续执行；只读冻结产品及权威合同，独占外部probe/target/合成数据。未改产品、CL预期或tasks，不代签reviewer，也不签真实issuer/client_gate/writer、ACP/Web或完整CL-013–016/033交付通过。

结论：冻结候选审批专项10项、独立外部补验4项、共享历史回归34项均通过。限定应用/真实SQLite范围未发现新阻断产品缺陷；原生可信发行、最终writer前实际锁协调及进程生命周期等仍未执行，必须保留后续门禁。

## 固定版本与权限

- 候选在`C:/Users/jack/.codex/worktrees/codex-session-wp2/acp-remote`，manifest为`.tmp/wp2-2.3-logs/frozen-files.json`，SHA-256 `88A094ABC6682B3F7CA70AEB640725928C36B09A7C6489EFBD9BF68ED24B07AA`，共14文件。主D产品当时仍为2.2，不以主D或HEAD代替候选。
- 本tester逐文件核验运行前后均与manifest一致；独立清单位于`.tmp/wp2-approval-independent/frozen-before.sha256`、`frozen-after.sha256`，SHA-256 `F3169564A4FD5EE12D073DB43CE5534EF64A1AC488B1265A3556BC4261D04B35`。冻结manifest给出每个文件摘要；没有继承coder自评作为独立结果。
- 依据主D最新session-contract：SHA-256 `CBF1BFE389CA3824884918C0BA9147B117C472359D396A7A132F150204B7D071`；tests：`1CEB400EC05147BAEDD1DBF5E02DF371A8B61684FE85A0AF9AB3E3A1A6DFB717`。§9投影维护不改变本次审批事务行为。
- Windows真实owner环境，沿用rustc/cargo 1.98.1。每轮加载现有CI owner脚本且probe成功；CARGO_TARGET_DIR=`D:/Project/acp-remote/.tmp/wp2-approval-independent/target`，TEMP/TMP同属该独占根。审批授权正常，没有切模型或换壳绕审查。外部工程仅离线解析缓存依赖；不改产品依赖/锁文件，不读取凭据或实际用户库。

## 实际执行

| 命令 | 结果 |
| --- | --- |
| 冻结worktree：`cargo test --manifest-path app/Cargo.toml --locked --lib approvals::tests -- --nocapture` | 退出0；10 passed、0 failed、0 ignored、190 filtered，0.84秒。 |
| 自有probe：`cargo test --manifest-path D:/Project/acp-remote/.tmp/wp2-approval-independent/Cargo.toml --offline`（最终） | 退出0；4 passed、0 failed、0 ignored，0.58秒；0项doc-test不计功能覆盖。 |
| 冻结worktree：`cargo test --manifest-path app/Cargo.toml --locked --lib history::tests -- --nocapture` | 退出0；34 passed、0 failed、0 ignored、166 filtered，7.84秒；包含HIST-R001确定性回归。 |
| 主D：`git diff --check` | 退出0；已有LF/CRLF提示不代表本tester改写文件。 |

开发者verification及完整200库+4向量、fmt/clippy日志只作输入与范围参照；本tester没有重跑完整200/4，也不将其自评冒充独立执行。正式独立结果是上表实际命令。

## 与14组准备矩阵的对应

| 组 | 实际证据与结果 | 层级/未执行部分 |
| --- | --- | --- |
| AP-001 正常/强刷顺序 | 真实SQLite保留完整原请求/作用域/tool引用；同一Command裁决后独立claim与TransportWritten，重复相同结果不重复审计。ApprovalService准备/提交检查历史checkpoint，dirty变化使提交失败 | L3应用+L4；无真实发行者或网络执行 |
| AP-002 唯一决定 | 现有双pool相反actor裁决仅一成功；外部probe额外三个参与者Barrier同时释放两个独立pool的相反决定，恰一成功，随后同一决定两pool抢claim恰一成功 | L4真实SQLite独立连接，非单pool排队模拟 |
| AP-003 跨动作防重 | 已受理input requestId用于approval.decide冲突；已裁决requestId用于session.input冲突；同approval原请求replay保留原Command、同ID改choice冲突且审计不增 | L4；错误wire映射未接 |
| AP-004 actor隔离 | 决策唯一性按actor/request；旧代数/撤销actor访问拒绝，claim只原决定actor及代数有效；原decision读取与后续delivery分开 | L4部分；完整目标ACL/command.get网关映射未验 |
| AP-005 强刷故障 | 现有真实输出写失败、准备后dirty变化均使observation/decision/claim失败；外部probe用真实deferred FK令历史COMMIT失败，ApprovalService返回HistoryUnavailable，approval仍Pending、approval.decide Command数量0 | L3+L4；0裁决由DB事实证明，不是模拟端口0计数 |
| AP-006 Run失效 | CancelRequested/Completed/Cancelled/Failed使旧claim不current；晚WriteCompleted记Unknown；取消请求仍为cancelling，不冒称Run已完成 | L4生命周期函数，未发真实ACP取消 |
| AP-007 连接代际/恢复 | generation更新拒绝旧observation/决定；手工recover_interrupted使旧claim失效，原accepted决定replay不重授权 | L4函数；没有本次真实节点重启/跨适配器恢复/物理连接断开 |
| AP-008 未支持事实 | 数字1/字符串1原生请求分离、同ID异结构冲突；未知option与错native session拒绝。外部另5分支：未知request、未知scope、重复option、空option、缺tool关联均unsupported且0决定 | L4；可信private issuer防伪是WP3，内部坐标不是授权 |
| AP-009 事务故障 | Command/approval CAS/security_audit/approval_audit_links写失败回滚；claim及report审计故障；decision/claim/report三阶段真实deferred COMMIT失败，命令/审计/交付状态不虚推进 | L4 SQLite真实触发器/延迟外键；无外部发送 |
| AP-010 delivery未知 | 一次claim后不能再claim；report持久化失败保持原决定与已花费claim，后续claim拒绝；report重复幂等/冲突拒绝；失效后晚成功报告记Unknown | L4；未模拟真实writer崩溃/网络确认丢失或守护进程自动恢复 |
| AP-011 认领后撤销 | 外部两pool竞争claim恰一成功；直接真实SQL撤销原actor/增代数后claim_current=false，晚WriteCompleted→Unknown，不能再claim | L4；不签撤销COMMIT与真实writer共享gate的零发送边界 |
| AP-012 脱敏 | 外部使用固定AP_SECRET标记覆盖native session/tool/request、工具正文、scope、选项ID/标签，遍历security_audit和approval_audit_links全部可读字符串列，均无marker；现有错误为语义型 | L4审计内容；未签进程stderr/所有Debug/真实日志全链路脱敏 |
| AP-013 关闭/所有权 | 历史34回归含小deadline、Drop取消、强刷失败责任保留；ApprovalService为prepare/commit端口，无自建真实writer任务 | 应用回归部分；真实审批发送future取消、15秒全关闭及OS资源未验 |
| AP-014 ledger满额 | 外部真实库插至100000条，拒绝新决定CapacityExhausted；原已提交决定仍replay原Command，行数100000不驱逐 | L4实际容量；没有运行24小时墙钟测试，不以无过期字段代签长时验收 |

## 外部probe与一次修正

外部源码`.tmp/wp2-approval-independent/src/lib.rs` SHA-256 `2F4758FF67640093C0FD1E7A3173C06CC52865E4F6CFA08D1233AF38BD78D034`。通过冻结公开Storage/ApprovalStore/ApprovalService/HistoryCoordinator接口，独立SQL连接seed可信actor/Run/工具来源及注入故障；历史提交真实，OutputControl明确为ResponsibilityRetained替身。没有实现假发行者/假writer来宣布真实能力。

首轮外部3组中2通过、1失败（退出101）：核心“强刷真实COMMIT失败→Pending且0裁决”已通过，失败发生在本tester额外假定“250ms后成功重刷可在同一个已stopped owner自动恢复审批”的unwrap。读合同“不自动恢复执行”及forced-failure停止事实后确认该额外假设无依据。未改产品或CL；保留首轮日志，改成更严格的“重刷持久化成功仍stopped，不能自动重新授权”，重新3项通过，再新增未支持事实分支后最终4项通过。此为外部probe错误假设修正，不记作产品缺陷修复或通过降低预期掩盖失败。

## 问题、限制与清理

限定范围未发现可复现新阻断产品缺陷。以下稳定ID是未执行边界，不代替产品完成：

- **AP-LIMIT-001**：private原生发行、client_gate→source owner→SQLite统一锁、强刷后版本重新复核、实际writer提交仍属WP3；内部ActorContext、ApprovalObservation和数据库decided不能当真实运行授权。
- **AP-LIMIT-002**：真实ACP选项/typed工具乱序及跨Run/跨session全矩阵、真实取消v1/v2、Web审批点击/错误状态、真实网络/进程崩溃窗口尚未本次执行；不得据L4签CL-013–016/033全部通过。
- **AP-LIMIT-003**：没有完整200项/向量/供应链或独立reviewer复核；本报告只复用其来源为开发日志的事实，不签最终交付。
- **AP-TST-001（测试资源清理，已处理）**：现有审批10测试结束进程退出后留下26个自有`acp-wp2-approval-42236-*`合成目录，best-effort Drop并未全部删净。本tester逐个验证绝对路径位于自有根且名称/PID匹配，随后清理26个，复查剩余0；没改测试源码。此为测试夹具清理观察，不能外推为产品进程泄漏。外部合成库/源码/日志保留自有根供复核，无用户数据。

## 日志

| 文件（均在.tmp/wp2-approval-independent/） | SHA-256 |
| --- | --- |
| approval-tests.log | D8269DE50D05A9B2DC168ECCDE0993DCCC43AFC10215D3A4E7ED715C297D4526 |
| external-final-tests.log | E782A3F79C280352D123C3B0DCA2115A49266FA7947771D2644863CB1930111C |
| history-regression.log | 2098A90ACD0C83F8A4F69E6C5856FC1FA339D760AC5D161F474B26B40EF2A486 |
| external-first-attempt.log（保留失败） | 69D1CD0A970D54DD398B79AC7E8675DA5BBD0B1954DBFC4F62CBA03A512F6887 |

前三组对应.exit为0，首轮失败.exit为101。14冻结文件前后摘要一致。后续修订需按实际影响重验，不能把此报告自动沿用到WP3或最终集成版本。
