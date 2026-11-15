# Root P2A + Domain 借用：独立组合验收

结论：指定root组合 **143/143实际通过**，为P2A19+原回归112+Domain借用公开12；严格all-target clippy及fmt均exit0。107项来源前后全部吻合。没有产品失败、测试预期变更或root写入；只签本固定组合实际子范围，不完成P2B/current、首开交付、Node owner或完整CL/L5/L6。

## 固定来源与资产归属

root `testing/runs/deliver-codex-session-loop/2026-10-31-p2a-dbr-integration-source.json` SHA256 `1896b236e703b8274dc8ab246257dadbacb85d76254bd15891f047fd650cddf1`。tester自行读取全部107路径核hash，执行后再逐项核107/107一致，见 `p2a-root-before.json` / `p2a-root-after.json`；manifest自身也仍同hash。

P2A源码仍 `4cf2b6d36a8a3095bed73de496d926cec871e97bc0addf431256be5a8d54b503`、P1仍0164d462；本次真实依赖root新Domain views `01e8ad736724f71a26fab128f819f41f1f65b359b06face964aa5d117c4eb933` / staging `ad3bf8644505a1f9656e58bc94580e146ca52702351570eda9b56c4e88cc8443`，没有以旧外部Domain结果代替。相对原外部P2A的来源差异记录在 `p2a-root-combination-delta.json`，含Domain新实现/开发tests、README、公开资产及两合同；生产P2A和原ACP/P1未变。

root P2A主资产SHA `a4a85b41a00a4b7a046ac7a0006504d9ee3b79e74b993e4e3a47da9f3ddc1382`，与prepared逐字相同；仅移除一行显式support path属性就逐字恢复原独立主资产b15310c2。support `e733dbcbb617189e698ce757685d9375ed0479b721a0966a6f339139cd8484bf`逐字未变。无assert/fixture/顺序/样式调整。Domain公开三资产分别2538c111/36292daa/3ca0debb，均符合root manifest；它们归原request_tester维护，本轮只复跑，不修改或代签其私有分支。比较结果在 `p2a-root-assets-comparison.json`。

## 实际执行

Windows x64，rustc/cargo1.98.1；tester独占CARGO_TARGET_DIR=`activity-target`，CARGO_INCREMENTAL=0，与root全量检查构建隔离。固定root Cargo.lock SHA `87f27b619281b37015c79ceae8d95471c8a4de50f202f4dd8a6d99fa0d27e324`前后不变；全部offline/locked，无新增依赖/锁解析/外发audit。仅终端合成双工及本次测试自有资源，不读用户数据库/凭据/配置，不运行真实Harness或浏览器。

`run-p2a-root-integration.ps1`保存完整可重复命令。通过进程级 `powershell.exe -NoProfile -ExecutionPolicy Bypass -File`执行，不改机器策略；UTC及真实退出码在 `p2a-root-exits.json`。PowerShell包cargo正常stderr进度的NativeCommandError文本不视为产品失败，依据同一shell的实际exit及测试输出。

| 实际命令/范围 | 结果 | 原始证据 |
| --- | --- | --- |
| root `cargo test --offline --locked --manifest-path D:/Project/acp-remote/app/Cargo.toml`，选择下列8个公开二进制，`-- --nocapture` | exit0；8主二进制均0失败/忽略/过滤 | p2a-root-tests.log |
| acp_tool_projection_acceptance | 19/19；8192小码矩阵、R/D表、双版257工具/300KB晚完成、FIFO/预算/来源等原断言不改 | 同上 |
| session_loop_acceptance + acp_message_id_acceptance + acp_view_observations_acceptance | 69+6+11=86/86 | 同上 |
| view_budget_acceptance + view_budget_allocator_acceptance | 20+6=26/26；原定向bulk真实null各路径仍实际命中 | 同上 |
| domain_borrow_acceptance + domain_borrow_allocator_acceptance | 9+3=12/12；满4096/节点32768、零分配、并发Drop等所属公开资产回归 | 同上 |
| `cargo clippy --offline --locked --manifest-path D:/Project/acp-remote/app/Cargo.toml --all-targets -- -D warnings` | exit0 | p2a-root-clippy.log |
| `cargo fmt --manifest-path D:/Project/acp-remote/app/Cargo.toml -- --check` | exit0 | p2a-root-fmt.log |

总143=19+86+26+12。Domain公开父测试实际另外启动一个exact子进程，输出为1 passed/8 filtered；该受控子结果属于9项父测试实现，不重复累计成144，也不把它的显式过滤伪称0过滤。主二进制9/9仍0忽略/过滤。没有用cargo启动成功或0项文档测试计功能通过。

Root新Domain组合中P2A实际布局仍ToolProjectionRead552、ObservationRead304，额外248≤512；替换峰值2369恰足成功、差1保留旧位；P2A payload512及费用原预期保持。原预算allocator实际identity/window请求变为[64,200,64]，受费固定上界与原断言仍通过；不能把底层布局变化概括成费用规则已放宽。

## 精确结论和未执行

本轮验证的是原P2A A–H已执行子断言在新Domain组合中不回退；详细语义及原未验界限沿 `p2a-tool-anchor-acceptance.md`。本次没有重跑6封装负编译；其产品类型来源仍原4cf、先前外部证据归原报告，不冒作root新增运行。Domain私有MAX/revision8项由原tester另验，本轮未执行、不代签；clippy编译不能替代运行。

同步apply内部CAS/u64耗尽、P2A中间allocator null/短写未新增独立动态注入；内层预算26回归不能说成每个wrapper步骤都注入。满4096借用端口12项通过不等于P2B已具备可授权current输出、原位writer满引用仍可Capacity。真实单一producer owner、公平调度、操作撤销/权限/WSS、可信重放前缀和分页、Web安全、真实Harness/Windows完整进程树及完整L5/L6仍未验。

P2B仅保留独立规划状态 `p2b-current-reader-planning-status.md`，TG01–06保持open-definition（包括真实compaction广告来源），全部not-run；本次组合通过不关闭这些编码前缺口或完整VPROJ-PLAN-002/任务3.2。

tester没有修改任何root产品/测试/锁/index、旧外部候选或原冻结证据。全部公开测试作用域资源正常Drop；没有本轮真实网络/服务/用户DB或脱离owner后台任务。Root全量owner check/build由主执行另记录，不借本组合结果代签整个交付。
