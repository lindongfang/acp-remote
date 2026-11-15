# §8A 公共 ACP 权限投影独立验收

结论：独立公共API的12组AP测试全部通过；新strategy下§6A公共API回归17项全部通过；对应仓库集成目标clippy -D warnings通过。本结论仅为L3协议子范围，不签SQL裁决、认证/资源owner接线、后台权限生命周期、WSS/Web或真实Harness及完整L5/L6。

## 固定范围与执行

实际运行于Windows、仓库工具链rustc/cargo 1.98.1，独占外部`request-tester/target`。合成内存双工对端，无网络、用户身份、用户数据库或真实Harness；未读取或外发Web审计元数据。通过真实公开Connection/Strategy API初始化v1/v2、发送合成JSON-RPC请求并解析实际响应帧；无调用私有生产解析函数或替换生产实现。

定义SHA256=`180b42e5c80fce85e2a18a7367a642d9da6b5037fa0cd95c5023a239916ff526`。执行前后四份冻结文件与主执行值一致：strategy cb91ed1d...、tests b161e952...、permission_subject开发测试8bd86e2c...、README b01b11a5...。全长及model/transport、独立用例与Cargo.lock摘要见`acp-permission-manifest.json`。独立验收源为`app/tests/acceptance/acp_permission_subject.rs`，SHA256=`cccda47e932366e486ec54400e320cf9973a9d8ede5f4778d425bc21aaf2b276`；仅新增该资产及根集成模块声明，未改生产代码或用户已有预期。

```powershell
cargo test --offline --locked --manifest-path app/Cargo.toml --test session_loop_acceptance acp_permission_subject -- --nocapture
cargo test --offline --locked --manifest-path app/Cargo.toml --test session_loop_acceptance acp_discovery_replay -- --nocapture
cargo clippy --offline --locked --manifest-path app/Cargo.toml --test session_loop_acceptance -- -D warnings
```

AP初次及最终复跑均12/12、exit0；§6A回归17/17、exit0；其他目标测试过滤不计本次执行。首轮clippy发现新验收辅助函数的needless_pass_by_value；改为直接移动RPC id到合成帧对象，输入JSON与预期未改变，最终clippy exit0。原失败日志保留。git diff --check exit0；新资产已rustfmt。PowerShell把cargo正常stderr格式化为NativeCommandError标签，最终真实退出值和全部用例结果均记录，不将标签误报为产品失败。

证据：`acp-permission-tests.log`、最终`acp-permission-tests-final.log`、`acp-permission-6a-regression.log`、原`acp-permission-clippy.log`、最终`acp-permission-clippy-final.log`及`acp-permission-exit-final.json`，均在本报告目录。

## 逐组结果

| AP子断言 | 状态 | 实际证据概要 |
| --- | --- | --- |
| v2-subject-optional | pass | absent/null分别selected与cancelled实际编码成功，supported且tool=None，session/raw保留 |
| v2-command-optional-tool | pass | toolCallId absent/null/string三种合法command均supported，原广告selected，terminal/meta/未来字段raw保留 |
| subject-shape-error | pass | bool/数组/数值/字符串、缺type及type null/bool/数组/数值均InvalidMessage、连接关闭、零响应 |
| command-known-types | pass | command/cwd缺失/null及command/cwd/toolCallId已理解字段错类型均协议错误、零响应 |
| v2-tool-call | pass | 合法tool投影与raw保真；必需toolCall/toolCallId缺失/null/错类型均协议错误 |
| v1-regression | pass | 无v2 title/subject的合法v1仍通过；v2 subject不能替代v1必需toolCall，非法tool结构拒绝 |
| required-envelope | pass | title/session/options/每option必需字段错误均拒绝；v2空options错误，v1空options无默认允许且可一次cancelled |
| original-advertisement | pass | 带额外空白raw字节与字段逐一相同；篡改可见tool/session/supported/options不增加资格；无连接来源的Strategy手造对象与替换raw均零写拒绝；原广告仍可合法响应 |
| unsupported-safe-cancel | pass | future/_custom subject或option kind、混合已知未知kind及重复optionId均拒绝所有selected，即使调用者改投影；随后可一次cancelled，第二次零帧 |
| rpc-scalar | pass | 同连接同时整数7、字符串"7"/"007"分别以原标量返回；null/bool/数组/对象/小数/超安全整数拒绝 |
| scope-duplicate-once | pass | 同raw同ID跨连接、未请求id拒绝；同连接未答复/已答复后重复ID关闭，已答复对象不能再响应；独立连接各自合法 |
| cancel-unknown-write | pass | 无subject/无tool command的响应writer错误/超时/future取消均使连接不可安全复用，复用零写；cancelled后的旧selected与第二次响应拒绝，无自动重发 |

§6A17项在本次cb91strategy下重新实际执行，覆盖最高版一次协商/回退边界、list/replay独立能力、版本化完成/快照、同目标窗口互斥、原始结构、分页/帧/ID预算及错误取消不重试等现有独立用例。没有沿用旧strategy的通过结果代签。

## 未验证范围

协议层selected只证明对原广告的合法编码，不是持久批准或执行ACK。session/cancel真实协议owner对全部适用请求的处理、当前原生目标/连接/请求归属、审批SQL唯一裁决/提交/实际交付围栏、无Run后台工作与Run结束后的应用有效性、WSS/Web及真实Harness均未执行。当前修正不开放产品批准入口，完整权限L5/L6维持not-run。管理SQL候选另按独立M定义与其冻结源码验收，本报告不混用其开发27例或迁移结构证据。

无本阶段产品失败问题；本次合成通道/future均随测试完成释放，无用户资源待清理。
