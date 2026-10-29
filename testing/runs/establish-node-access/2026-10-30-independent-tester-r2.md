# establish-node-access r2 独立定向验收

状态：阶段验收完成；仅定向复测R01/R02及正常身份拒绝后重新配对，不代签reviewer或整体验收。r3最终清理和当前逐例计数见同日independent-tester-r3报告。

- 清单：`2026-10-30-acceptance-r2.sha256`，77项，SHA-256 `409A743B2E725B43B05C3CF644F1506812D51294C902F9C3A2E612D6FF733506`；开始时独立逐项校验77/77匹配。
- 本轮继续使用tester独立 `.tmp/independent-acceptance-20261030/target` 增量编译r2嵌入Web，cargo build --locked --offline退出0；使用新data-r2/30452 origin，不触及用户30447。
- 旧27AAB报告保持历史结论；本报告不重新跑Rust全量。生命周期使用真实Chromium/IndexedDB/生产页面，必要的可控延迟与合成pagehide/pageshow事件作为故障刺激另行标注，不冒充真实BFcache导航。

## 实际结果

1. r2 binary SHA-256 `7AA40C5E22AA8D70125A478BE2BE71D83978CB3FEA3DD9AF7D1E71D642BFDBA2`。真实受信HTTPS/ego TaskSpace4、30452新data，首轮TTY配对明确y批准，clientId=`4viwZsGzsYhPcfVg7Rb1lw`，Node ID=`b0164dd94ab9568ab97df4f5fac64f62592d8f31f724ee0c793eda69d83b0080`，私钥不可导出。
2. R01未验证节点关闭：在真实生产页面内注入仅替代WebSocket传输的故障适配器，分别在hello后、任何hostProof之前关闭4401/4410。两例均保留clientId、公钥和Node ID，不清除pin、不显示配对表单；online后观察2.5秒，只有1次连接且只发送hello。此为页面级可控恶意传输测试，不声称是真实服务器发出该恶意关闭。恢复原生WebSocket后原身份正常认证。证据：`.tmp/independent-acceptance-20261030/r2-unverified-close.json`。
3. R02 initialize：真实IndexedDB读请求success回调被精确挂起。分别执行“pagehide后释放，再pageshow”与“pagehide后连续两次pageshow，再释放”。挂起时均0连接；隐藏状态释放不修改panel、不新建连接；恢复后只创建1条管理连接，原key/clientId/node保持不变。证据：`r2-initialize-lifecycle.json`。
4. R02 reset：真实owner撤销已认证身份，收到原生4410/revoked。仅挂起真实IndexedDB delete事务complete回调；pagehide后两次pageshow，boot没有越过reset写新key/建连接；再次pagehide再释放回调，隐藏期间无UI覆盖、无key写入/连接。最后pageshow只生成一个新unbound身份，私钥不可导出、clientId=null。证据：`r2-reset-lifecycle.json`。事件为合成生命周期刺激，调用真实生产页面的事件处理链；不将此扩展为浏览器所有BFcache/导航组合。
5. 真实4410后的新key再次认领，终端保持pending_confirmation直至owner明确y批准，获得 `Vha9faFdWcKQknGxCw1A-A` 并正常认证。pagehide断开后由owner撤销该身份，reload原生WSS明确收到4401/authentication failed；直接进入新key配对页。新key再次真实认领/明确y批准，获得 `ZRM22m9bN-uLRcvglc3ToA` 并读取管理列表。`r2-4401.json`、`r2-current-approved.json` 保存最终状态；没有绕过终端批准。
6. 独立读回 `.tmp/acceptance-r2-check.log`：7文件46个Web测试、Rust118库+4JWS均通过，含格式/Clippy及Web类型/构建命令链；日志SHA-256 `43D777453C257C2A69E43011444113166ED156389DCF7D941E088B0216F4E52D`。同版本同范围复用这些工程断言；本tester另运行strict OpenSpec及git diff --check，均退出0。与独立cargo build结合支持NA-001工程检查，不升级其他行为用例。

## 结论与后续版本

本轮R01未验证hostProof关闭的身份保护、R02上述initialize/reset受控交错及真实4401/4410新key再批准子集通过，未发现这几个已执行断言的产品失败。保留范围限制：本地proof错误与远端关闭优先级的全部组合、真实BFcache导航/其他异步阶段尚未穷举；不代签reviewer关闭问题。

随后reviewer报告R03：r2页面恢复绕过60秒退避/终端停止。上面的initialize/reset测试没有构造限流/终端停止，不能反证R03；r2不能据局部通过判为整体无阻断。主流程已冻结r3，单列报告验证该新增回归。r2节点已owner stop退出0（PID32292），同自有data/TaskSpace由r3定向接管，最终清理见r3报告；用户30447保持运行。旧27AAB报告未改写为r2结果。
