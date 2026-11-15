# WP2 / 2.1 独立模型验收

2026-10-31。结论：独立外部消费 crate 的9个公开 API 测试全部通过；仅覆盖 CL-043–045/056/066/067/072–074 的纯模型子范围，不签完整 L5、L6 或产品交付。没有发现此阶段阻断。

## 版本与执行

产品源位于已有实现上的增量工作区 D:/Project/acp-remote；四文件及 app/Cargo.lock 摘要见 activity-source.json。外部 crate 在 activity-probe/，通过公开 `acp_remote::domain::{activity,native}` 访问，未修改或复用 coder 的测试正文。tester 独占 activity-target/；没有启动节点、Harness、浏览器、进程树、SQL、网络或凭据读取。

命令：`cargo test --manifest-path <本输出根>/activity-probe/Cargo.toml --offline -- --nocapture`，设置 CARGO_TARGET_DIR 为本输出根 activity-target/。实际退出码0：9 passed、0 failed、0 ignored；doc-tests 0，不将其计为功能覆盖。完整最终输出在 activity-probe/test.log。

首次外部 crate 自行解析依赖并运行9/9后，为防止偏离产品锁，复制 app/Cargo.lock 为外部锁的种子再运行同命令；最终所有外部依赖包版本均来自产品锁，仅新增本地 probe 包。未修改产品锁。PowerShell 对 Cargo 的 stderr 进度文本显示 NativeCommandError 装饰，命令最终退出码0且9项实际执行成功，不是编译/断言失败。

## 逐稳定 ID 子范围

| Case ID | 实际断言与结果 |
| --- | --- |
| CL-043 | unknown 即使没有 Run 仍阻挡；断连后旧/新连接 idle 不能当恢复，插入回报拒绝。PASS（模型）。 |
| CL-044、CL-045 | 两种停止证据各自逐缺身份、唯一归属、恢复、管理协调、旧代际、目标及停止证据；缺一拒绝，完整恢复仍须新连接可靠 idle；旧输入只返回管理关联而不复活旧 token。PASS（模型）。 |
| CL-056 | 显式来源缺失拒绝，同来源别名共用slot，不同来源同短ID独立，第二owner拒绝；三种epoch的类型互异。PASS（模型）。未验证持久请求代际/实际名额。 |
| CL-066 | 无Run原生active阻挡但保留权限/取消；前台完成时后台active/unknown独立，不构造平台输入。PASS（模型）。 |
| CL-067 | 有/无Run关联审批在平台输入完成后仍存在，后台idle也不能移除；只显式retire移除，冲突Run关联拒绝。PASS（引用模型）。未验证真实审批来源或交付。 |
| CL-072 | idle取消无work/Run/额外阻挡，不推断后台停止；进入并结束下一轮后旧cancel仍不重新成为当前。PASS（模型）。 |
| CL-073 | 插入待确认及已确认但未终态两情况，重复idle与取消捕获均不释放输入；可靠拒绝只解决对应输入。PASS（模型）。 |
| CL-074 | 取消在自然完成前/后两序列，旧终态不结算新轮；跨目标及同连接新load代际拒绝旧token。PASS（模型）。 |

## 限制与下一步

模型接受的 RecoveryFacts/可靠状态由未来应用所有者证明，本轮不验证其可信发行者、实际停止/恢复保证。唯一身份/资源注册与真正并发由后续节点装配负责。JWS签发、水位、SQLite事务、审计、协议处理/真实 Harness 与 Windows 树全部未执行；9/9不能把完整稳定用例标成通过。

下一阶段合同门槛已在统一候选 CL-004/018/075–084 中覆盖，尤其签名独立用途、固定截止精确边界、水位不可回退与写失败、10000命令/100000审计保护及schema1/2→3无损；逐动作摘要数组与DDL/事务步骤须先固定后独立检视。request_tester 的阶段定义属于同组稳定ID的细化，不另建平行产品规则。

## 持续验收资产集成

父执行授权的 tester 独占范围内，已将独立 `activity-probe/src/lib.rs` 逐字复制至主仓库 `app/tests/acceptance/activity_model.rs`，复制后先核对 SHA-256 完全相同（`0d38365d38124f7dc67afdabc8156f41993456999570ec51aca58dd6df2e95f8`），随后仅对此测试文件执行 `rustfmt --edition 2024`。格式化后 SHA-256 为 `df529b00c092e53916782bbda9029cbc36478a04fb17a3fc1fb8f1d84300a9b3`。测试断言与预期未调整，产品模型未修改。

实际检查：该单文件 `rustfmt --edition 2024 --check` 与 `git diff --check` 退出码0。原独立9/9日志仍对应外部 crate 原始测试；仓库持续入口由父执行新增 `app/tests/session_loop_acceptance.rs` 路径适配后统一运行。未把文件复制或格式检查当作仓库入口已运行。

| 资产 | 路径/证据映射 |
| --- | --- |
| 持续测试正文 | 主仓库 `app/tests/acceptance/activity_model.rs`；9个公开API测试，稳定ID对应关系与上表相同。 |
| 原独立测试及日志 | 外部输出根 `activity-probe/src/lib.rs`、`activity-probe/test.log`，保持原始来源与9/9结果。 |
| 产品模型受测摘要 | 外部 `activity-source.json`；父执行复制为主仓库 `testing/runs/deliver-codex-session-loop/2026-10-31-activity-model-source.json`。 |
| 独立报告副本 | 本文件由父执行重复制为主仓库 `testing/runs/deliver-codex-session-loop/2026-10-31-activity-model-acceptance.md`，tester 不并行编辑该副本。 |
| 集成资产摘要 | 外部 `activity-integration-source.json` 记录复制/格式化文件、原日志与测试正文摘要；不覆盖原受测模型快照。 |

新增持续入口后的运行证据应另记实际命令、版本与结果。上述集成不扩展纯模型证据范围，完整 L5/L6 继续待验。
