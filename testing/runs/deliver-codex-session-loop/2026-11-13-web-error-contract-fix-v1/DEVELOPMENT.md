# Web 顶层错误合同：开发结果

2026-11-13。仅开发验证，候选尚未合入 Root；不签独立验收或完整 WP4。

产品范围为三叶：`protocol.ts` 在既有17码闭集中增加33码，未知码及帧结构仍严格拒绝；新纯函数以 `Record<ManagementErrorCode,string>` 覆盖50码中性文案；`main.ts` 仅委托请求错误分支，连接/认证/配对分支不变。`rate_limited` 保留既有客户端合同，不冒称有当前服务端 producer；`revoked`、成功 payload 内 `result.error` 不扩入顶层码。client/reconnect/upload产品、server、依赖/锁/配置/样式均未修改。

编码依据为本目录 `PLAN.md` 及独立 `WEB-CODING-GATE-20261113.md`。候选基线347叶，Green351叶；canonical为 `0448cbe203f87f1b75077f07416e1d8a021633fc8871e874d6a8528b453297a0`。`SOURCE-RED.json`、`SOURCE-GREEN.json` 固定所有文件，变化原字节分别保存在 `red-source/`、`green-source/`；原7码及其余10码的抽取前后实际输出对比见 `equivalent-red-baseline/RESULT.json`，17/17完全相同。UI Red是等价抽取后运行，不是原字节main内执行；parser/client Red为原实现。

实际结果：

| 检查 | 结果 |
| --- | --- |
| 未改产品、固定隔离运行器管理基线 | 25/25通过，exit0 |
| Red前类型检查v1 | exit1，固定向量未写出造成缺module；保留原日志 |
| 修正向量准备后Red前类型检查v2 | exit0 |
| 新测试真实Red | 268项：152通过、116失败；parser33、client33、文案50失败，未知/认证负例通过，exit1 |
| 相同六叶测试Green | 268/268通过，exit0 |
| Web全量测试 | 34文件、917/917通过，exit0 |
| Web类型检查 | exit0 |
| Web构建（含tsc --noEmit） | exit0，产物位于本次独占runs目录 |

Red/Green逐码参数行独立执行，不把首行失败后未执行项累计通过。client场景使用真实WebCrypto双向签名校验和FakeSocket，A为真实client构造的device.revoke mutation，B同连接在途，C后续只读；原负例仍4400并拒绝全部pending，已知码只拒A。A/B/C结算后推进120秒假时钟，按action及目标身份核A仅一次发送；这是受控client/reconnect范围，不声称驱动main真实恢复页面。上传begin/chunk/finish/status错误对象保留及清理失败不覆盖原错误属于既有语义回归。

运行器使用Node22.20.0、native配置加载和独占cache/TEMP/LOCALAPPDATA，避免默认junction缓存及token写入。每次实跑共享依赖1439叶前后路径/字节/SHA一致（清单含隐藏缓存，不排除）；这是写后核验，不是OS强制只读。没有安装依赖、网络或npm audit。所有日志和 `runs/*/RESULT.json` 保留，退出码和依赖摘要由 `verify-development.mjs` 独立重算为开发收据。

准备脚本曾在CRLF锚点和表头计数保护处停止；它们不是预期产品Red，未据此声称新测试生效。真实Red仅指 `red-contract-v1` 的实际Vitest结果。原失败日志不覆盖。

剩余：独立tester自有源码/完整依赖副本正式验收、reviewer同版交付检视、获准后Root精确九叶安装及安装后检查。真实WSS、main浏览器页面、真实Harness、服务端49个producer故障注入、完整WP4和全库检查未在本包执行。整体任务仍9/28；npm audit按用户决定未执行。
