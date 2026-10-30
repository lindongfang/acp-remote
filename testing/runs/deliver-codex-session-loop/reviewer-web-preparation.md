# Web 准备候选独立静态预审

2026-10-30。结论：**发现 WEB-R001–003，尚不批准组件候选无条件接线；不是WP4最终验收。** 仅静态预审，未改产品、未运行浏览器DOM/backend/ACP，不将未挂载页面判完成。

候选根 `C:/Users/jack/.codex/worktrees/codex-session-web/acp-remote`。handoff SHA256=`A4AD3BFA3C80FB7CACC5614F589C1C089997C93E6FE6CA98AD91D5473D3947A8`，实际首尾14/14文件摘要与其中清单一致。main.ts=`CE6D443E44D4C920D7E7B184BEAAB83EF713B68B8F9CFF4FC6D58499D5D14815`、styles.css=`0109AA878DCE55A26B539A81DD3AADFE9A3E94E651D82541CDD104FF348FAFE2`均与交接基线一致；SessionPanel未挂载。读取主D合同CBF1BFE389CA3824884918C0BA9147B117C472359D396A7A132F150204B7D071及最新tests定义，完整读取新增session源码、management客户端与相关事件解析和测试。读取实际日志：12测试文件/74测试通过、tsc --noEmit、Vite构建成功；未独立重跑，构建仅14模块且不能证明未挂载panel运行。

## WEB-R001 — P1：异步投影安装缺少提交时版本/代际仲裁

位置：clients/web/src/session/panel.ts:85–96、117–125，onEvent的baseline分支直接并发启动installBaseline。版本检查在读取投影之后、merge和loadApprovals之前；两个异步下载之间没有独占安装序号，install闭包还捕获了此前的nextItems/nextVersions。旧任务在较新任务安装后恢复，可直接安装旧映射并把revision/liveRevision/state/approvals写回旧值。

可执行时序：同会话快速重复订阅，前一次baseline尚在事件队列中，第二次select已生成新view signal；前后两份baseline事件均使用当前signal启动加载。A通过版本检查并阻塞审批正文，B完成并安装更高revision，A随后install覆盖B。仅比较事件到达时/投影开始时版本不能防止此窗口；server发送顺序正确也不能保证异步读取完成顺序。未按真实DOM执行，此为源码可达风险。

建议：按订阅/view代际关联baseline，串行投影安装或使用单调安装token；每次实际提交前再次核当前代际/revision/liveRevision并基于最新版本表合并，旧任务不提交状态/映射。补确定性延迟baseline与approval/body Promise的组件测试，证明旧任务晚完成无回退、不会丢较新item。接线前修复。

## WEB-R002 — P2：目录列表先读详情会必然耗尽合法分页游标

位置：panel.ts:49–72。choices在当前页逐个完整读取detailsRef后才用nextCursor请求下一页；合同列表游标60秒，但单详情对象合法16MiB，在共享510ms命令节流下1366块需约11.6分钟。第一页面有大详情且nextCursor非null时，下一页必然stale_cursor；当前没有列表stale_cursor恢复机制，refresh总是从头重复大详情，无法取得后续有效条目。Workspace/Harness两种均有此结构，虽原路径/参数通常较小，能力元数据数组/长详情仍允许此范围。

建议先在游标有效期内收集有限短summary/详情引用，再下载详情，或按列表页渐进读取且提供明确有界续取机制；过期重新获取时不要无限重读首个大详情。保留节点revision/身份范围校验、1024行/内存显式预算，不截字段。测试以虚拟时钟/命令预算构造多页与超过60秒详情读取，证明后页可达；不要提高协议限流或租约。

## WEB-R003 — P2：同一批次重复item可改变不可变身份字段而未报完整性错误

位置：panel.ts:99–111，model.ts projection解析。merge循环只把每个item与安装前this.#versions比较；同批先出现的新item并不进入该表，故同一itemId在baseline.turns与liveItems中出现时，两个候选都可能current=undefined。ItemVersions.apply仅比较版本/摘要，不比较sourceId/turnId/firstPosition/kind/protocol等身份字段；只要正文相同，后一个候选可替换成不同source/位置。snapshot重复ID亦相同，turn解析只检查轮内位置升序，不保证跨数组item身份一致。

建议先建立批次身份索引，遇同itemId必须保持session/turn/source/首次位置/kind/protocol/version身份一致（内容版本是另一个可变维度），再按contentVersion合并；重复相同版本异正文继续报错。允许baseline的合法committed/live同item合并，不应简单拒绝所有重复ID。加同批跨数组/跨turn身份冲突测试，证明失败保留原完整投影。

## 已见正确边界与未验证项

- content.read逐块检查引用/偏移/长度/12KiB、16MiB、固定SHA及最终摘要，UTF-8 fatal解码；abort不交付部分字节。上传逐index验证并共用命令节流。32KiB帧、strict信封深度/重复键检查及socket字节背压保留。
- projection inline/引用排他，manifest字段白名单、类型/外层snapshot/revision核验、同轮连续片段合并；64MiB组装/可见正文与详情预算明确报错而非截断。但不可用组件单测推导实际队列/租约边界已通过。
- 版本表按更高contentVersion合并，接受同version同SHA新ref与committedVersion推进，不展示正常保存状态。当前只比较SHA而非显式逐字节比较；输入内容已实际重算SHA，实用完整性较强，但最终实现应按合同保留严格同版本正文一致性语义。
- DOM源码使用createElement/textContent，未知结构可展开，未见innerHTML/动态脚本/外链执行sink。未知审批supported=false禁点，restriction及非pending禁批；transport_written未显示工具成功。未运行实际DOM，不能签可访问性/交互/脱敏展示最终通过。
- 客户端没有重连后自动重发副作用循环，超时关闭并显示结果不明；没有把命令accepted当原生loaded。已知commandId仅支持显式查询。真实断连重认证、原子baseline读取期间的事件与完整性、被撤销后的清理和后台Run继续运行仍需真实集成验证。
- 不增加依赖；未挂main/styles符合当前准备边界。本报告不要求绕过早期装配限制；修复仅在授权准备组件范围进行，最终WP4接线后必须冻结同版再审。

WEB-R001–003均Open，由coder修复后原reviewer按新冻结摘要复查；不改tester预期，不因74项单测通过代关。产品最终CL-030/031/042等保持not-run。

## 同日追加：WEB-R001–003 原问题修复复查

新候选清单 `clients/web/.tmp/wp4-web-prepare/r001-r003/handoff.json` SHA256=`48EC154E5A735E563A80AD04CF3AB0AD987CAFBC8C23F3B9163BC0A4C354DC13`。实际首尾核对20/20文件均匹配清单；main/styles仍与上节基线一致。实际读取installation/item-store/catalogue及panel接线和三份新增回归，读取84项全量开发测试成功日志；本reviewer另在该C工作区实际执行 `npm.cmd run test -- tests/session-panel-state.test.ts tests/session-item-store.test.ts tests/session-catalogue.test.ts`，exit0，3文件10测试通过。未改源码或D主产品。

- **WEB-R001 Closed（准备组件范围）**：InstallationGuard在同步提交处核对view generation、已提交operation sequence、revision/liveRevision；select/close重置代际，旧任务不能越过较新安装。merge返回的闭包在实际提交时以最新items/versions重新合并，不再捕获旧映射；最终状态与正文同步提交。独立重跑延迟旧baseline审批读取→新baseline先安装、view切换期间旧任务完成两条确定性回归通过。它们使用最小document替身并stub render，不是实际浏览器DOM或可访问性验收。
- **WEB-R002 Closed（准备组件范围）**：refresh先顺序collectSummaries完成短页捕获，再下载长详情；同批catalogue barrier使其他正文/详情命令等待，避免长详情堵住后页。捕获revision、唯一identity/cursor、1024行/2MiB短摘要预算明确失败，取消等待不启动内容请求。独立虚拟时钟使用真实CommandBudget验证后页先取得，之后1366节流间隔超过60秒仍保留后页条目。不是实际16MiB网络/租约吞吐证明，真实集成仍须验证。
- **WEB-R003 Closed（准备组件范围）**：assertItemIdentities同时覆盖已安装和同批候选，读前检查且提交重检；mergeVerifiedItems以itemId/contentVersion键逐字节比较同版本内容，允许合法committed/live不同版本，身份/同版本正文冲突保持原映射。独立重跑同批跨数组source冲突在读前失败、合法更新与后续冲突、实际baseline冲突保留无关已安装项等回归通过。补上的byte比较亦落实了上一节同SHA仍需正文一致性语义。

**本报告三个问题全部 Closed；修复候选可作为未挂载Web准备组件继续后续串行集成，不是WP4完成或最终验收通过。** 冻结候选声明的合同F4E8BDD72935DF71A7983E04EFE1A3522909533DDF90A6FE1AD532E94C47258C较原CBF1新增内部catalog合同；已核查修复所依赖的§9仍相同范围，未将§10未实现动作结果冒充产品。真实DOM、认证节点、撤销、lease、backend/ACP及最终组合均仍未由本次执行；最终挂载和固定版本集成后需重审。
