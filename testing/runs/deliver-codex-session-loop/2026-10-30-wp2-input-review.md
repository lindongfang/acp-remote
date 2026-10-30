# WP2 task2.1 输入受理候选独立代码复核

日期：2026-10-30。**结论：有限静态代码检视 PASS，未发现该七文件内层候选的确定阻断缺陷；不签 task2.1 完成、不签 CL-017–022 产品验收或最终Web/L5/CL-030。** 仅写本报告，未改产品、测试、规划或checkbox。审批保存/历史流式缓存/资源/Web接线不在本候选已实现范围。

独立上下文延续规划reviewer，不继承coder实现对话。读取AGENTS、reviewer角色及D盘当前权威spec/design/session-contract/tests；没有用C盘工作区旧规划作为行为依据。coder声称156库+4JWS和Clippy仅为入口信息，本次未读取相应完整运行日志、未重新执行，因此不列为本reviewer已验证。

## 固定范围

工作区 `C:/Users/jack/.codex/worktrees/codex-session-wp2/acp-remote`；实际HEAD为`65b46c01d6b95fc1f1a8223b1c92a729a885c185`。先读取并固定下表七文件SHA256，再通读全部实现和开发测试；结束重算仍为下表摘要。普通sandbox的git -C初次Permission denied，文件只读可用；随后精确require_escalated只读Git diff/status/HEAD成功，无自动审批拒绝、未改索引。实际diff仅lib.rs模块声明和storage.rs升级/工厂接线，另新增domain/application/session_storage三个目录五文件，符合本次七文件声明。

| 工作区内文件 | SHA256 |
| --- | --- |
| `app/src/lib.rs` | `4645DFF1BCB1FE5940C818E938AF987F64DC4283857130FB3C2516C4E75CC6AC` |
| `app/src/storage.rs` | `C07618ACD0DB6CF90050435BCA95B12144622837AF651AF72D22162E294281DA` |
| `app/src/domain/mod.rs` | `DE1329C04997DFF7474B099C60B498195719DEBD4CDFAEF743392C45C706E30E` |
| `app/src/application/mod.rs` | `E99F4BDC70E394B7AF06A6581A901A2A2C6761D45FEEA76FF8CCF63C2DC06E3A` |
| `app/src/session_storage/mod.rs` | `DBD56A7DE362C9736E924FC014527579EE18C6893F08BE0807C0F121D747EAB7` |
| `app/src/session_storage/schema.rs` | `81966828A362F0B0CF6DC13D88E44095A66457DB097703EB24BA55E2D9A04FAC` |
| `app/src/session_storage/tests.rs` | `FA2FE59429979D1551EDBCCF16C83801A6E7B43813487703C967E2CF5EFA60FB` |

## 实现核对

- schema.rs先read-only preflight，再打开WAL/FULL/foreign_keys池；版本0仅空表库、1/2需核心定义及版本一致，2还核对新增表/唯一前台索引定义。迁移在一个BEGIN IMMEDIATE事务建立session结构，更新node_meta.schema_version和PRAGMA user_version，最后校验再COMMIT；失败返回关闭池。核心表不是删除重建，现有nodeId/publicKey/keyFileRef、clients、旧命令/审计和node_state revision不被迁移DML修改。DPAPI文件不在此代码写范围。
- storage.rs的SCHEMA_VERSION为2，write_node_meta拒绝旧版本写入；旧schema1 initializer的原始版本门禁会拒绝2。开发用例中“旧程序拒绝”只是读取PRAGMA后assert_ne，不是运行旧二进制，不能把该条测试写成真实旧程序验收。
- persist_input与admit_input从数据库clients核对trusted和当前auth_generation；input对象还绑定actor/session/generation/purpose，重新计算SHA256。不以调用方自报trusted取代数据库；跨actor引用无法消费，审计仅元数据。
- admit_input使用BEGIN IMMEDIATE包住Command、Turn、Run、用户ConversationItem、已有contentRef关联、session_state单调位置/版本和受理审计；任一错误或提交失败不返回受理成功。相同(actor,requestId)按动作/引用/内容摘要比较，重复返回原关联、冲突拒绝，重复不再生成Run。重试在busy前识别原请求，允许查询原受理。新请求检查loaded、前台Run和100000命令上限；唯一部分索引防止两个SQLite连接同时建立前台执行。
- claim_dispatch在事务中复核actor和session状态，仅accepted可转dispatching；对Run必须恰更新一行，COMMIT后才返回非Clone DispatchPermit。第二次claim得None，不靠内存标志保证唯一领取。没有在数据库事务里调用外部进程。Permit意味着单次持久领取，不能证明外部系统精确一次执行；领取后崩溃允许不派发而记未知，不自动补发。
- recover_interrupted将未决会话标interrupted、Run标unknown、Command标delivery_unknown，同事务且不派发；重启后的同请求查询保留关联、不产生新Permit。未知Run仍占前台互斥，避免上一输入可能已产生副作用时直接再执行。
- 正常路径未引入unwrap/expect/panic；语义错误分在domain，SQL错误不回显敏感正文。application依赖InputRepository端口，domain不依赖SQL/网络/进程。现有工程依赖未在这七文件外变更。

## 身份与接线边界：不能夸大为外部安全已完成

ActorContext的字段与EntityId构造器公开，任何Rust调用者可构造clientId/auth_generation。它只是内部repository参数，注释亦明确不是认证或审批grant；数据库“当前可信”查询不证明调用者持有该身份私钥。当前InputService不持有AuthenticatedActor、client_gate或审批发行者，也不负责目标权限和撤销后的实际socket/进程派发。

这不构成本次尚无外部入口的内层候选身份绕过结论，但后包必须以真正不可伪造actor和client_gate接线，不得直接从Web JSON构造ActorContext并调用。claim_dispatch注释要求执行owner持gate跨副作用；目前仅注释/端口契约，尚无真正调用者可供证明。不得把SessionStore公开方法视为对不可信调用者的授权API。审批发行、广告选项/连接代际、唯一裁决及交付未知的完整产品逻辑仍属后续2.3/WP3。

同理，recover_interrupted目前是显式入口，没有在Storage::open自动运行，也未改node启动；外部接线必须在允许命令/dispatch前由owner调用一次并检查结果，失败不得ready。若漏接，旧accepted命令在重启后仍可能被claim；当前静态通过不免除该集成门禁。

## 开发测试检查与证据限制

实际阅读测试源码，包含7类受理语句/真实延迟外键COMMIT失败、16并发重试/双claim、竞争actor/引用作用域/冲突/stale generation、独立连接并发、容量与安全整数、cancelling/releasing互斥；schema1升级保留、DDL/版本触发器/真实COMMIT失败回滚、未知/矛盾/缺表/改列及schema2缺索引拒绝。这些是有意义的SQLite开发测试，不是仅镜像函数返回。

下列是稳定验证跟踪项，当前不作为已发现生产缺陷，也不据此删除原最终验收要求：

| ID | 范围/位置 | 当前状态及必须保留的后续验证 |
| --- | --- | --- |
| WP2-V001 | session_storage/tests.rs:346 | restart用例实际正常storage.close/drop/reopen、手工recover，派发数是本地simulated_dispatches；不能称断电/杀进程或真实外部派发计数通过。独立测试应在提交前、提交后领取前、持久dispatching后外部派发前后分别终止自有进程，重新启动真实owner流程，核对原关联及零自动重派。 |
| WP2-V002 | domain/mod.rs:21；session_storage/mod.rs:177、214 | 外部actor/client_gate及startup recovery接线未实现，必须在WP3/4及相应CL验收证明；公开上下文不等授权，数据库trusted查询不等身份证明，单Permit不等实际外部精确一次。 |
| WP2-V003 | schema.rs:146/165；tests.rs:513/567 | 代码及fixture支持迁移原子性，尚未独立执行本候选测试、真实schema1节点副本升级/认证撤销回归、旧二进制拒绝或OS断电。未来真实库升级还须按合同先停节点、owner锁及一致性备份；本次静态审查不授权直接迁移用户数据。 |

允许在既定WP2范围继续实现及补独立测试；若要将task2.1判完成，必须另对固定版本运行并保存适用检查、独立测试及所需产品断言，不能据本静态PASS勾任务。2.2/2.3未实现内容与后续Web/ACP/Windows资源及L5/L6保持未执行。

## 权威依据摘要（主目录当前文件）

| 文件 | SHA256 |
| --- | --- |
| `D:/Project/acp-remote/openspec/changes/deliver-codex-session-loop/session-contract.md` | `BB8F6661EB66C43EE4CFAD7B31FA30C4B07AD5FACDADD3A96D200F7FB264A7CA` |
| `D:/Project/acp-remote/openspec/changes/deliver-codex-session-loop/design.md` | `D42BEBFB4E93DEF9E8B20CCB5FB710E12EE531CA86F85A2DE7AFD96FB6CD566C` |
| `D:/Project/acp-remote/openspec/changes/deliver-codex-session-loop/tests.md` | `6E6DAD8DE4B68ADE19021BF5518A4115173A1E1329C0F799FC7B26956E0A3AF6` |
| `D:/Project/acp-remote/openspec/changes/deliver-codex-session-loop/specs/session-execution/spec.md` | `61067A30D797F0AD3789226CAA3B1491299AA4F45400F9230221298823ED00A6` |
