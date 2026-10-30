# WP2 2.1 冻结候选：领域及真实 SQLite 独立复测

日期：2026-10-30。独立 tester，只读冻结产品源码、独立复跑现有测试，并在独占外部probe补边界；不继承 coder 对话，不以开发自评为执行证据。本报告仅覆盖领域/SQLite层，不签 Web、L5、最终任务或 CL-017–022 全部通过。

结论：冻结候选的12项session_storage测试和独立外部probe的2项测试均实际通过；已验证原子受理、真实SQLite写入/COMMIT失败回滚、独立连接并发互斥、同请求防重和单次派发认领、内容边界/篡改、旧代数拒绝、部分迁移保留与故障路径。当前范围未发现可复现的阻断缺陷；更高层真实派发、启动恢复、身份/权限接线及完整升级验收仍未执行，不以本结果代替。

## 固定输入

- 受测worktree：`C:/Users/jack/.codex/worktrees/codex-session-wp2/acp-remote`，HEAD `65b46c01d6b95fc1f1a8223b1c92a729a885c185`。实际候选包含未提交的lib.rs/storage.rs及新增application/domain/session_storage，不能只以HEAD标识候选。
- 冻结7文件逐项SHA-256如下；运行前后清单完全一致。清单位于`.tmp/wp2-independent/frozen-before.sha256`、`frozen-after.sha256`，清单SHA-256 `AD7D08681AEB5144168E93508747291380849863238DF39E0EB7A818B77EEEE4`。
- 权威合同与用例读取自主工作区`D:/Project/acp-remote/openspec/changes/deliver-codex-session-loop/`：session-contract SHA-256 `BB8F6661EB66C43EE4CFAD7B31FA30C4B07AD5FACDADD3A96D200F7FB264A7CA`，tests SHA-256 `6E6DAD8DE4B68ADE19021BF5518A4115173A1E1329C0F799FC7B26956E0A3AF6`。

| 冻结文件 | SHA-256 |
| --- | --- |
| app/src/lib.rs | 4645DFF1BCB1FE5940C818E938AF987F64DC4283857130FB3C2516C4E75CC6AC |
| app/src/storage.rs | C07618ACD0DB6CF90050435BCA95B12144622837AF651AF72D22162E294281DA |
| app/src/domain/mod.rs | DE1329C04997DFF7474B099C60B498195719DEBD4CDFAEF743392C45C706E30E |
| app/src/application/mod.rs | E99F4BDC70E394B7AF06A6581A901A2A2C6761D45FEEA76FF8CCF63C2DC06E3A |
| app/src/session_storage/mod.rs | DBD56A7DE362C9736E924FC014527579EE18C6893F08BE0807C0F121D747EAB7 |
| app/src/session_storage/schema.rs | 81966828A362F0B0CF6DC13D88E44095A66457DB097703EB24BA55E2D9A04FAC |
| app/src/session_storage/tests.rs | FA2FE59429979D1551EDBCCF16C83801A6E7B43813487703C967E2CF5EFA60FB |

## 执行与真实边界

沿用本机Windows、rustc/cargo 1.98.1；在真实owner环境先加载现有`testing/ci/windows-owner.ps1`，owner probe成功。仅进程环境将CARGO_TARGET_DIR设为`D:/Project/acp-remote/.tmp/wp2-independent/target`，TEMP/TMP设为独占`.tmp/wp2-independent`。未安装npm依赖；外部probe使用本机缓存离线解析Rust依赖，锁文件仅写独占目录。没有修改冻结源码/索引，也没有使用真实用户数据库、配置、凭据或Harness会话。

| 命令 | 实际结果 |
| --- | --- |
| 冻结worktree：`cargo test --manifest-path app/Cargo.toml --locked --lib session_storage::tests -- --nocapture` | 退出0，12 passed、0 failed、0 ignored、144 filtered out，测试运行0.73秒。被过滤项未执行。 |
| 外部probe：`cargo test --manifest-path D:/Project/acp-remote/.tmp/wp2-independent/Cargo.toml --offline` | 退出0，2 passed、0 failed、0 ignored，0.94秒；doc-tests 0项不计功能覆盖。 |
| 冻结worktree：`cargo fmt --manifest-path app/Cargo.toml --all --check` | 退出0。 |
| 冻结worktree：`git diff --check` | 退出0；仅提示Git后续可能LF→CRLF，不曾改写文件。 |

前两命令使用cmd原生重定向，避免PowerShell将cargo普通stderr当终止错误。sandbox最初无法识别worktree/读取其Git状态，后续require_escalated只读核对HEAD、状态、格式与差异成功；不是产品失败。测试完成无运行中会话，现有测试的`acp-wp2-*`专用目录未残留；独立probe仅保留专用合成数据库、源码、锁文件、构建目录和日志供复核。

## 断言核对

| 范围/场景级别 | 现有实际断言及独立补验 | 结论范围 |
| --- | --- | --- |
| CL-003/004原子受理，L3/L4局部 | Command/Turn/Run/ConversationItem/audit各1条、关联ID、引用、版本/首位置；6类SQL写入故障及真实SQLite deferred FK COMMIT故障后5表无部分记录、序列不推进、原上传对象保留；去掉故障后原请求可首次受理 | 真实SQLite事务通过；没有外部真实派发 |
| 同请求防重/冲突 | 16并发相同请求仅1次fresh、关联一致；异引用同ID冲突、忙碌时原请求仍可查询；actor查询隔离/旧代数拒绝；100000条满额拒绝且不驱逐 | 仓储层通过；没有真实Web断线重连或24小时墙钟跨度执行 |
| 独立连接互斥 | 现有独立Storage/pool连接竞争同会话，恰1成功另一Busy；外部probe进一步用两个独立pool争同请求，恰1fresh、同一关联，再争同Command dispatch claim，恰1 permit且数据库仅1 Run | 真实文件SQLite独立连接通过；派发permit不等于外部命令已执行 |
| 取消/释放互斥 | 直接SQL fixture将Run改cancelling后新输入Busy，可靠终态fixture后session releasing拒绝；dispatch前重新检查releasing且原Command仍accepted | 领域状态fixture通过；未发真实取消/释放 |
| 派发未知/重启 | 关闭并重新打开SQLite，手工调用recover_interrupted两次返回1/0；accepted及dispatching保留原关联、delivery_unknown、不再claim、新请求InvalidState；本地模拟派发计数不增长 | 真实库重开/协调函数通过；非进程崩溃测试、非daemon启动自动接线、非真实Harness零重发证明 |
| 内容/撤销边界补验 | 外部probe接受恰好16MiB、拒绝多1字节；跨会话引用拒绝；改写body但保持长度导致摘要不符拒绝；受理后直接SQL撤销并增代数，claim及command查询拒绝，Command仍accepted | 真实仓储公开接口通过；不签client_gate/writer撤销临界区及网络目标权限 |
| schema1→2保留 | 真实SQLite schema1合成副本升级；nodeId/publicKey/keyFileRef、部分clients撤销/公钥/代数、management结果、audit ID、node revision与opaque identity文件字节保留；PRAGMA版本2 | 合成迁移层通过；非完整真实DPAPI/配对身份验收 |
| 迁移故障/损坏拒绝 | DDL冲突、node_meta版本trigger、真实deferred COMMIT失败后两版本仍1、会话DDL无部分新增、clients数量保留；未知版本/版本矛盾/缺表/改列/缺索引拒绝且主库文件字节不变 | 实际SQLite故障及只读预检通过；主库字节比较不等同所有WAL/附属文件取证 |

独立外部probe源码在`.tmp/wp2-independent/src/lib.rs`，SHA-256 `0B7E0BD23A5D63EB2BBFFB3B512B97CAE7192E4BA7C77C43032DC5534AA19D4B`；用公开Storage/SessionStore接口与真实SQLite独立连接驱动，不调用私有测试helper，不改产品。fixture直接登记已可信actor、已加载会话，不冒充注册/认证/审批实现。

## 问题与未执行项

本次未发现需要提出产品修复的可复现阻断缺陷。以下稳定编号为证据缺口，不能当成已完成项关闭：

- **WP2-T-001（未执行，迁移验收范围）**：现有测试对旧schema1程序仅读取新库版本并`assert_ne!(version,1)`，没有运行旧程序拒绝新库；identity文件是opaque sentinel，未建立真实DPAPI重启/重新配对无需发生的完整证据。clients/management/security_audit也仅检查部分列/字段，不是全行逐项保留核验。需后续完整升级验收，不以测试名“retains...”外推全部身份合同。
- **WP2-T-002（未执行，运行时恢复与安全接线）**：recover_interrupted被测试直接调用；InputService只委托仓储，ActorContext明确不是认证授予。真实owner数据目录锁、启动就绪前协调、client_gate、目标权限、审批发行者、派发/取消进程及异常崩溃恢复尚未本次验收。当前仅领域/仓储候选，不声明这些已实现。
- **WP2-T-003（未执行，产品及检查范围）**：没有Harness/Web、L5/L6、注册、审批、保存、真实24小时去重场景；没有执行其余144项Rust测试、clippy、完整根check/build、供应链或独立reviewer检查。该限定报告不签CL-001/002最终集成通过，亦不签CL-017–022全部通过。

## 原始日志摘要

| 日志 | SHA-256 |
| --- | --- |
| `.tmp/wp2-independent/session-tests.log` | 8E35FA65643DC971956A5B02CEB930D62B10DB883DE994C084F8905F67E7D6C9 |
| `.tmp/wp2-independent/external-tests.log` | 074C264E93E18604A1E1430E293F2F8637BCCC3F04B5CEDED47BA1C695437757 |

对应`.exit`文件均记录0。后续7文件、合同或用例变化需重新判断本证据有效范围；本tester不代签并行reviewer结论或最终tasks完成。

## WP2-T-001 旧schema1程序拒绝实际schema2副本：补测

2026-10-30独立执行，未读取C工作区已开始2.2的活动源码、未重新链接其依赖。使用前述独立probe已关闭的`same-request-33800.sqlite3`，其内容来自冻结2.1候选真实Storage公开接口，含1个已受理Command和1个Run，不是仅手改user_version构造的坏库。

Node内置SQLite backup API从readOnly源连接建立一致性副本到`.tmp/wp2-independent/old-schema-rejection/data/node.sqlite3`；只在该专用副本补入schema_version=2的合成node_meta及opaque identity.key fixture，不替换任何真实身份。源库user_version=2、integrity_check=ok；副本integrity_check=ok且foreign_key_check为0。副本同时包含既有schema2真实表/约束及受理数据。保留构造脚本`old-schema-probe.mjs`与前后facts JSON。

受测旧exe为`.tmp/cl-wp1-baseline/target/debug/acp-remote.exe`，关联本tester先前上游基线build:rust成功记录，文件时间07:12、长度15405056；本次执行前后SHA-256均`7C3C72D8FBABF9B642832BE5C9931067B17A8BD910400CC8A223EB79AB5B62D4`。先前基线未记录exe构建时摘要，因此这里明确是本次固定的实际程序文件，不声称存在构建时签名/可复现构建证明；对应基线源码SCHEMA_VERSION=1且会拒绝其他非零版本，与本次程序实际错误一致。

真实owner CI前置成功后执行：

`D:/Project/acp-remote/.tmp/cl-wp1-baseline/target/debug/acp-remote.exe --config D:/Project/acp-remote/.tmp/wp2-independent/old-schema-rejection/node.toml daemon start`

旧程序实际退出1，原始stderr为`error: node startup failed: storage schema version 2 is unsupported`。不是assert_ne或缺TLS错误。专用配置给出非空TLS路径；启动源码顺序为数据目录锁→Storage→身份→TLS，此次明确在Storage版本检查拒绝，没有到达TLS或合成身份解密，也没有监听就绪。

- 副本主库运行前后SHA-256均`D42EAAD8AC642AD1958F1AE694965AA910BB292257034E77D538800DAEF7BEFA`。
- identity fixture运行前后SHA-256均`C10E45372B5E434B4F4369C8F79D814FEC90CBCAD55CBE320C6EFF5DA9F2CEAA`。
- 进程退出后专用data仅有原node.sqlite3、identity.key与本次正常创建的空.node.lock，没有WAL/SHM/journal残留；不把owner DACL加固视作正文改写。
- 命令、stderr、退出码及哈希保留在`.tmp/wp2-independent/old-schema-rejection/old-exe.log`、`old-exe.exit`、`facts-before.json`和`facts-after.json`。

**WP2-T-001中“真实旧schema1程序拒绝新库”子项现已实测通过并关闭该证据缺口。** 同ID其他限制仍未关闭：真实DPAPI身份保留、全部clients/management/security_audit逐字段保留及完整用户节点升级流程未因此通过；不签Web/Harness/Job Object或完整CL-004/最终任务。