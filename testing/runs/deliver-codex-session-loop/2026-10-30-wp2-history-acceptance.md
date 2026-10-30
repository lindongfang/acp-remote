# WP2 2.2 历史保存：独立复测

日期：2026-10-30。独立tester，只读冻结候选；使用独占外部probe与target实际执行。不改产品或验收预期，不代签reviewer，不签Harness/Web、Job Object、最终CL或任务全部完成。

结论：HIST-R001修复候选的history专项34项及外部补验2项均通过；本次领域/异步所有者/真实SQLite范围未发现新阻断缺陷。原候选HIST-R001失败与审批403未执行记录保留；修复版确定性窗口及多等待者回归实际执行通过，原问题仍由提出的reviewer关闭。

## 两个版本与执行环境

- 旧候选：manifest SHA-256 `C1BB66052A63846ECC67F8595AA5126A8081D6BE269C9F08BDCBF2B306E39A72`。只读核对13文件匹配；首次owner运行被自动审批服务403阻止，命令未执行。随后reviewer发现HIST-R001高风险：schedule后才注册Notify可能丢首条输出唤醒，旧候选暂停且没有独立PASS；本tester不把开发者旧187+4自测改写成独立执行。
- 修复候选：`C:/Users/jack/.codex/worktrees/codex-session-wp2/acp-remote/.tmp/wp2-2.2-hist-r001-logs/frozen-files.json`，SHA-256 `EF2FE20EEC101129B4342870106BC81CC61F8F9E4E891A347C20BAD0298C94FF`；base HEAD仍为`65b46c01d6b95fc1f1a8223b1c92a729a885c185`，实际未提交候选以13文件清单为准，不以HEAD替代。
- 独立核对全部13文件前后一致，清单在`.tmp/wp2-history-independent/fixed-before.sha256`及`fixed-after.sha256`，SHA-256 `C71E62656BE9F19F415398B5E420607EDC011352BAF999435E853006826EEDB4`。其中application/history.rs为`686D12998D1826E77CA3183C3CE0FC69981C0FDADE601722EC85645BDF66ED58`，其tests.rs为`7031D8BFE132F87CAD1F6BF24AFD433DF2E82D1C35DF14C32ACFADCE7A14F52B`；其余11文件仍匹配冻结manifest。
- 新冻结版按授权重试审批成功，没有更换执行方式绕过审查。真实owner CI前置probe成功；Windows本机rustc/cargo 1.98.1。CARGO_TARGET_DIR、TEMP、TMP只在进程内设到`D:/Project/acp-remote/.tmp/wp2-history-independent`（target子目录）。无用户库、凭据、原生会话或实际Harness参与；未安装npm依赖，外部Rust工程离线解析缓存依赖，锁文件仅在独占目录。

## 实际命令

| 命令 | 结果 |
| --- | --- |
| 冻结worktree：`cargo test --manifest-path app/Cargo.toml --locked --lib history::tests -- --nocapture` | 退出0；34 passed、0 failed、0 ignored、156 filtered，运行5.83秒。22项application、12项session_storage。 |
| 自有工程：`cargo test --manifest-path D:/Project/acp-remote/.tmp/wp2-history-independent/Cargo.toml --offline` | 退出0；2 passed、0 failed、0 ignored，0.09秒；0项doc-test不计功能覆盖。 |

34项中`crash_after_reservation_child`在普通父运行会直接返回；其逻辑由另一个parent测试实际启动同测试exe执行，子进程明确exit93且被wait回收。不能将普通helper返回本身当一次额外崩溃证明。

已读取开发者verification及原始日志/退出文件：旧final2 fmt/clippy/owner/test/diff为0且187+4；修复索引声明history34、完整Rust190+4。本文只将本tester上述实际执行标为独立结果；其余156项、vectors、完整fmt/clippy/build不在本次独立命令范围内，既有开发/CI证据须保留其来源。

## 13组准备矩阵执行归属

| 组 | 本次实际断言及边界 | 层级与限制 |
| --- | --- | --- |
| HI-001 | 499/500ms与未满64KiB独立分支；外部probe另从10000起点检查10499/10500，不错误使用进程绝对0 | L3受控单调时钟；save成功可走真实SQLite |
| HI-002 | 65535/65536累计raw脏字节；同item替换另计受理字节；全部ItemEnd/RunEnd/BeforeApproval/Release/Shutdown强制刷新无周期等待 | L3替身计数，不以metadata提早触发raw阈值 |
| HI-003 | 249ms不新增save、250ms实际重试；第2失败未停止、第3失败仅1次stop请求；成功重置计数 | L3替身失败及端口计数；L4真实SQLite失败协调另测 |
| HI-004 | 阻塞重试时4999/5000ms独立watchdog触发，取消在途后保留PersistentFailure原因；外部probe成功后到旧5秒点仍不错误停止 | L3真实Tokio future/替身写入失败，非真实进程停止 |
| HI-005 | 会话高水位前一字节/恰好与硬上限；flight旧版+new dirty分别计量，旧ack不清新dirty/首脏时钟，空payload/metadata仍有界 | L3精确retained计数，原raw触发量与retained容量分开 |
| HI-006 | 多session共享节点128MiB、高水位/硬上限/并发保留不越界，同session多scope累计16MiB；达到阈值拒绝输入并刷新/停止 | L3受控存储，原子预算真实共享协调器，非进程RSS峰值证明 |
| HI-007 | typed scope/owner handle隔离；工具更新保留item/source/首次位置，其他同字节source不合并；错误scope/身份及存储条件更新冲突不改旧内容 | L3+L4；公共update_item不提供用户expectedVersion参数，不虚构该API的CAS协议 |
| HI-008 | v3/v5晚到旧版不回退，同版异字节VersionConflict、同item异source IdentityConflict；后项失败整批回滚、旧内容保留 | L4真实SQLite；外部probe独立补v5→v1→同v5异内容 |
| HI-009 | reserve失败/安全计数上限不推进；真实子进程预留后exit93，重开next_position=65、下一预留从65，不复用1–64间隙 | L4真实SQLite+真实本测试子进程，不是Harness/daemon崩溃 |
| HI-010 | 未知原始字段/大opaque原生ID逐字保真；外部probe加入空白、NUL、非UTF8字节，实际写入/读取逐字相等 | L4真实BLOB，无JSON重编码；非Web解析/展示验收 |
| HI-011 | worker立即64KiB、Drop取消在途save并清flight；修复后idle schedule窗口首条1字节重新算499→500期限，65536即时；三个已注册forced waiters均被广播唤醒、单scope峰值1 | L3精确cfg屏障，非随机stress推断；HIST-R001回归实际通过 |
| HI-012 | forced失败返回错误、ResponsibilityRetained明确；close 5ms期限遇阻返回CloseTimeout且请求stop；shutdown/Drop拥有并取消任务；干净RunEnd退役scope metadata | L3异步端口，非真实进程树；15秒上限代码检查，未用真实15秒挂起及OS端口关闭验收替代 |
| HI-013 | body/item/revision/真实deferred COMMIT失败均不推进提交；pin与版本替换独立连接事务串行、旧版pin保留和释放回收、重开失效；旧Run完整性不被新Run成功覆盖，未受理input引用不被输出清理误删 | L4真实SQLite，pin并非60分钟认证读取租约/网络身份机制 |

外部probe位于`.tmp/wp2-history-independent/src/lib.rs`，SHA-256 `324E72B2870C975A21C7ECBFC7B97CDDC42C439C83E9AA9A70EC9AB5C59203D9`。其reserve委托真实HistoryStore，save在失败分支显式返回替身错误、成功分支调用真实commit_items；OutputControl为ResponsibilityRetained替身。第二项纯真实SQLite存储任意字节/版本冲突。fixture仅SQL登记可信actor和已加载会话，不冒充认证/审批/注册已实现。

## 问题与覆盖边界

- **HIST-R001（原reviewer高风险）**：旧候选FAIL保留；新实现将notified/pin/enable置于检查时限/schedule之前。三个确定性测试均实际通过：首条小内容、首条65536及多forced等待者。本tester记录修复复测证据，不替原提出者关闭问题。
- **HI-LIMIT-001（未执行）**：真实Harness停输出、共享实例隔离、Windows Job Object和完整子孙进程树、真实节点关闭端口没有接线证据；OutputControl的Requested/ResponsibilityRetained不表示已停止。未签CL-011/026/030全部通过。
- **HI-LIMIT-002（未执行）**：网关逐块认证/撤销、60分钟读取租约、pageSize历史API/Web、审批前交付联动、daemon startup恢复调用未在此层验收。旧2.1证明按受影响文件和新版本另行判断，不能无条件外推。
- **HI-LIMIT-003（有限覆盖）**：小deadline超时已执行；15秒全预算、finish_stop无限挂起及所有OS资源关闭需后续装配验收。当前内存预算验证的是声明retained对象计量，不承诺总进程RSS不超同一值。

## 证据与清理

| 自有日志 | SHA-256 |
| --- | --- |
| `.tmp/wp2-history-independent/history-fixed-tests.log` | 72F321DD59ECD244E1AE7B45C8EEE5A6083B055929E3EAA095B7026F718A1BB6 |
| `.tmp/wp2-history-independent/external-fixed-tests.log` | A96C2677626BC6695325104B1318A44654C06094EF810AB9CB5FD5F8682C39EB |

相应.exit文件均0。现有测试专用acp-wp2-history目录未残留，外部合成库及构建日志仅保留自有目录供复核，无用户数据。前后13文件摘要一致。未来候选/合同变化须重新判断有效范围；本报告不勾tasks或签最终交付。
