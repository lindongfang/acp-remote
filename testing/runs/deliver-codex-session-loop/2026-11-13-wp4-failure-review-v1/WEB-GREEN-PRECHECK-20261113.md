# Web Green 交付预检与独立执行前置

2026-11-13，独立 reviewer 只读检视。**产品三叶及冻结 Red/Green 开发证据预检通过；窄合暂不放行，WEC-FR-002 必需验证缺口保持 open。** 尚未读取到正式独立执行脚本与绑定收据，不能预签未来独立结果。未改产品、测试定义、他人报告或 Git index；未运行测试、构建、类型检查或运行器探针。

## 版本与范围

候选基目录 `testing/runs/deliver-codex-session-loop/2026-11-13-web-error-contract-fix-v1/`。

| 对象 | 独立核验身份 |
| --- | --- |
| SOURCE-GREEN.json 自身 SHA-256 | `c205f0e23dc80c3de7d8226d6bdd4555e875e8c90454d242987f560c7f9649d6` |
| Green canonical / 351 叶 | `0448cbe203f87f1b75077f07416e1d8a021633fc8871e874d6a8528b453297a0` |
| SOURCE-RED.json 自身 SHA-256 | `354dbe2b206ad99a2e9547407087626ec27ea9bac45f2a216e0f0122757f92e1` |
| Red canonical / 351 叶 | `b6bd6b4e8c25d428cfd582810eb34d2c208cf6bdd200530ee9b68e9a908d350f` |
| Root 基线 / 347 叶 | `4b50dccdcee713c9fc03d19c0f91f60066b4076ee08002149aeedc001d9a5338` |
| Git index | `ac76c7ad84c1d4333fc64a710647e60aeb96015eb4407f996a4bf6adc91fbff4` |

均实际重算 canonical。实际 Root/Green 范围文件树与各清单一致，347/351 叶逐项字节数与哈希吻合；Red 以 `red-source` 的修改叶和未修改 Root 基线重建核 351 叶，所有 Red 修改叶有保留副本；Green 修改叶备份同样吻合。九个差异精确是原审定三产品加六测试叶，无删除。全部测试叶在 Red/Green 间同字节；Root 当前 347 叶仍未改变。scope 仍是 src/tests 加五个根配置/入口文件，排除 Root 既存 qa，不冒整个仓库身份。

产品实际差异符合编码门：protocol.ts 仅 union 和 knownManagementError switch 各增 33；默认未知码异常未改。error-messages.ts 为穷尽 `Record<ManagementErrorCode,string>` 和纯查表函数。main.ts 仅导入 helper 并替换请求错误原七码 switch；其余错误分支不变。client/reconnect/upload action 及 server 未在候选写集中。逐项核 vectors、union、parser switch、生产文案表均为审定 C50，精确文案匹配 50/50。

## 亲读开发执行证据

读原 stdout/stderr、JSON 逐例结果、RESULT，以及八次 DEPENDENCIES-BEFORE/AFTER；全部 1439 文件清单的 canonical 独立重算一致，均为 `dd097a21189008970617be4dba6aeba4ac78f899a07858fecf1b6ad628eed6ca`。这是前后文件一致的写后证据，不证明 OS 强只读或中途零写入。

| 运行 | 实际结果 |
| --- | --- |
| baseline-management-v1 | exit 0，25/25 |
| red-typecheck-v1 | exit 1，准备缺 module 的失败，不能当行为 Red |
| red-typecheck-v2 | exit 0，真实 Red 前已修正准备 |
| red-contract-v1 | exit 1，268 行：152 passed、116 failed、0 其他状态 |
| green-contract-v1 | exit 0，268/268 |
| green-full-check-v1 | exit 0 |
| green-full-test-v1 | exit 0，917/917、34 个测试文件 |
| green-full-build-v1 | exit 0，Vite 实际产物落专属 dist |

116 个实际 Red 失败按 fullName 独立归组为 parser 33、client 隔离 33、UI 精确文案 50，无其它失败；旧码和负例通过。原 17 码抽取比较逐行 before/after 相等，保存的 main-before/main-after/helper 身份分别绑定基线和 Red。新 UI Red 明确基于等价抽取，不冒原 main 未改字节。CRLF 锚点停机和表头误计准备失败有开发记录；不合并为行为 Red，也不覆盖原日志。

日志实际有 PowerShell UTF-16LE BOM；本审查捕获脚本先以 UTF-8 解码读取时未检出计数，在写收据前停止，随后改按 BOM 正确解码读取。仅改 reviewer 自有捕获工具，未改日志、未重跑测试。

开发结果为开发结果，本 reviewer 只读复核，不冒独立 tester 亲自执行。

## WEC-FR-002：必需重连边界验证缺证（open）

严重性：本轮窄合前验证阻断；不是已发现产品故障。目标版本为上方 Green。

1. `workspace/clients/web/tests/reconnect.test.ts:9-28` 的 C50 矩阵每行新建 reconnect，只证明新状态分类和回调时机。冻结 WEC-007 明确要求“每个终止分类前先设 transient timer”，当前矩阵未逐项证明取消既有 timer。原六个 reconnect 用例也未枚举冻结表全部 connection_closed code：null/1001/1006/1011/1012/1013/4408/4500，限流 4409/4429。终止分类是否正确清理旧 timer不能从新对象零 timer 推断。
2. `workspace/clients/web/tests/management.test.ts` 的 C50 authenticated isolation 中 recovery 是空 `vi.fn()`，虽然验证 A/B/C、120 秒计数、单 socket 和 client 无自动重发，但没有实际调用 consumer 连接恢复路径或产生新 socket 世代。冻结 WEC-008 明确不把仅 callback spy 当恢复流程，要求针对三个既有码的恢复回调和世代观察。当前测试无法证明“连接恢复之后也没有重发 A”。

最小可执行补证：由独立 tester 在自有测试叶调用真实 `ManagementReconnect` 与 `ManagementConnection`，先排 transient，再对 C50 各终止码及认证终止分类核取消；补全固定 close code/time 边界；对三个旧可恢复业务码在 timer callback 中驱动真实 consumer reconnect、记录新 socket 工厂/认证/命令帧，按 action 加目标/内容统计 A 仅一次。其余 47 码无恢复 callback。不得修改产品重连、main、上传状态机或冻结预期来凑绿。依然明确未驱动 main 真页面/WSS/Harness。

已把此缺口及补证路径发给 coder 与独立 tester。由本人读取固定补证源码、执行证据再关闭；当前不因开发 917 全绿直接关闭。

## 独立执行脚本门及窄合条件

tester 静态隔离方案可执行，但正式前还须亲读实际脚本、完整展开命令、唯一输出路径、最终 Green 及完整复制依赖清单，确认不写 coder 证据或共享 node_modules、不安装联网、不更改预期。脚本门通过后才能独立执行，不以尚未出现的脚本路径视为已通过。

窄合需同时满足：本 Green 351 叶及九叶范围仍吻合；独立自有完整依赖执行 check/full test/build 全有实际日志、计数和退出码；WEC-001–010 结果逐项映射，WEC-FR-002 补证经本人复查关闭；前后 Root/候选/锁/index/依赖身份不变；无新阻断。届时只放审定三产品叶及六开发测试叶，Root 安装仍须逐叶验证和实际工程复验。独立测试叶保留自有证据及后续长期用例同步安排，不暗中扩大产品写集。

完整 WP4、真实浏览器 main 页面、TLS/WSS、真实 Harness、49 producer 逐项故障注入均不在本次签署范围。

固定输入：`WEB-GREEN-PRECHECK-INPUTS.json`，SHA-256 `5670d281c333160c3ee06ce82e5aa34ad217df81bfca4a8cb49e6cba4c5c58b7`；其中保存实际读取源、日志和收据哈希。只读捕获工具为 `capture-web-green-review.mjs`。
