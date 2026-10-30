# WP1 网络恢复与真实链路重试

日期：2026-10-30。主执行记录；独立结论分别见 [tester复核](2026-10-30-network-retry-acceptance.md)及[readiness复核](2026-10-30-readiness-review.md)。本记录更新当前入口，不改写上轮超时证据，不代表产品会话闭环验收。

## 原因与实际处理

用户要求重新检查网络。直接HEAD官方Codex服务仍在15秒超时；只读查看当前Windows用户设置发现ProxyEnable=1、ProxyServer=127.0.0.1:10808，WinHTTP仍为Direct且调用进程无HTTP_PROXY/HTTPS_PROXY/ALL_PROXY。显式使用该已启用代理后，官方响应405/Allow:POST，说明本次网络路径可达，不能将405当作模型执行成功。

仅为任务测试进程设置上述三个代理变量，均为`http://127.0.0.1:10808`；未修改系统代理、全局环境、用户Codex配置、TLS校验或产品配置。随后真实ACP请求取得模型输出及终态，是下面能力判断的依据。没有切换私有网关或协议，不推断其他机器使用相同代理。

网络恢复后，只读`git -c http.proxy=http://127.0.0.1:10808 ls-remote`补得官方规范仓库当时HEAD引用`abc8a31b16fc4eb6e3578ced562088e8e26d959a`，codex-acp的`refs/tags/v2.5.0`引用`34e08970aa324a2be985f5080674d5aa91d7666f`。这是远端引用观测，不假称逐字核对官网页面与该HEAD每个文件相同；实际运行仍按npm发布完整性、程序/SDK schema摘要及协议响应固定。旧报告“当时未取到revision”保持历史事实，不冒称旧步骤已经执行成功。

## 固定程序与隔离

沿用上轮锁定的codex-acp2.5.0/codex0.165.0及对应摘要，实际路径仍`.tmp/cl-wp1-probe/node_modules/`。Windows x64，真实stdio、官方适配器与原生Codex存储。新测试根`.tmp/cl-wp1-network-retry`用于直连复验，`.tmp/cl-wp1-proxy-retry`用于代理重试；分别owner-only DACL，仅临时复用用户已授权的本机登录，在专用workspace创建新会话，不加载用户原生会话。

直连复验仍v2 timeout idle；该结果留在network-retry根，不覆盖上轮或代理成功结果。所有后续成功只归代理重试环境。

## 实际覆盖

两版分别执行：initialize、session/new、合成marker输入、session/close、退出旧适配器/重新启动、同原生sessionId的session/resume、在问题中不夹答案的marker召回、session/cancel及可靠终态、权限cancelled、final close。两版均真实收到`ACK_63F9`并在跨进程恢复后召回`WP1_NATIVE_CONTEXT_63F9`。v1恢复无旧输出；v2恢复只产生当前info/usage/idle快照，不把该快照当新的完成。v1 prompt stopReason、v2 state_update idle/stopReason分别判定，未以prompt插入响应或cancel通知冒充完成。

另补两版一次性批准：只批准专用cwd中精确写`approval-probe.txt`、内容`WP1_APPROVAL_PROBE`的PowerShell命令，只选择对端广告的allow_once，不选择永久允许。两版都有同ID工具in_progress→completed及end_turn；v2还有terminal exitCode0。独立tester核对当前原始文件18字节、无BOM/换行。两版共享同一合成文件，当前文件不能单独归因每次落盘；以各版真实授权和工具completed轨迹支持有限执行兼容，不宣称更强的独立新文件落盘验收。

公开合成结果持久摘要为 [proxy-retry-summary.json](2026-10-30-proxy-retry-summary.json)，SHA256 `53621DDD880CDA73E55DA9A4302B268A2AE625E2F9E3AB57ABD23A5EC18A87DE`；其每项关联原始结果SHA256、版本/新sessionId、marker/召回判断、实际权限选择、工具状态和退出事实。原始私有日志未复制到Git，登录材料未进入摘要。原型脚本与结果精确摘要在独立报告中记录。

持久探测资产`testing/probes/codex-session-loop/authenticated.mjs`从实际原型提取，改为可移植路径/临时根参数、认证前置及精确命令白名单，SHA256 `1C7AEBAEC3C29A6029F36123FD880E038AFC0E3322F9E1F66E90B59645AD23EC`。改写后只作`node --check`，真实结果仍属于原型及其摘要，不冒称完整持久脚本已重跑。安装依赖及隔离身份准备见该目录README。

## 资源与限制

原型所有适配器进程均关闭stdio并退出0。主执行及独立tester实际CIM查询专用程序目录前缀，当时匹配为0；这不涵盖系统路径PowerShell后代，也不等于Windows Job Object完整进程树验收。四份本次临时auth.json副本已移除，独立tester存在性核对均False；用户原始登录/会话/数据库未改。测试原生状态及脱敏证据保留供复核，没有自动重发结果未知的prompt。

本次足以解除WP2的基本执行/原生恢复可行性未知。readiness-review允许先开始WP2领域/存储及其开发测试，plan已明确阶段顺序；task1.2的产品CL-005及1.3的最终目标/供应链检查仍待办，进度仍1/14。多客户端唯一裁决、Web/SQLite产品、原子实时基线、大内容预算、共享实例、完整进程树和最终CL-030/031全部仍须真实实现与独立验收。

WP2 coder已在app管理的隔离工作区启动任务2.1，基线及精确写范围登记在plan。主目录当前仅维护规划/探测/证据，未因网络恢复将任何产品验收任务提前勾选。
