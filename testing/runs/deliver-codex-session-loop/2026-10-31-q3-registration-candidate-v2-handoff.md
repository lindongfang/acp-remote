# R1/R2/R3 修订二交接

本修订取代首候选供当前独立验收；首候选和全部旧证据保持原字节。本修订未扩张产品规则或原生 writer 能力。

source-manifest.json SHA256 5eeac76995a33c7e2ed410d3100b117052a405e67dcfd32b8052c15637163ba3。source/ 包含全部 136 个固定源/构建输入文件，仅 app/src/application/queue_owner/service.rs 相对 candidate-v1 变化，SHA256 1cbd530dead9f908ee4402a31e397b4b42892346efde56d658f041070edfb89f。Web dist/JWS fixture 及公开接口和 sealed 故障 hook 签名均不变。

修复 REG-IMPL-OWNER-001：reconcile 的候选 owner 查找后，在最终 tasks/state 同一短锁边界内再次按请求 target/id 核对当前 pending；从该项取得 probe 和 worker。已有 worker 时只检查该 probe；新 worker 的 ticket、guard、任务注册都绑定该同一项。不存在先捕获 A 的 probe、后给 B 标 worker 的缝隙。没有新增生产 yield、可伪造 key 或测试 setter。reviewer 已静态亲读确认反例消除，问题关闭仍待同源独立回归。

from-v1.patch SHA256 4e1a5e79adac43deeedbd109decc03058ffafb334d89e12f223f9a4331ca963b；from-baseline.patch SHA256 d26931dead0d95d7cd4ef62b78d6a0ebf88a34804b39d822ef836ae43157d449。两份均以 core.autocrlf=false 的 git apply --check 对各自原目录只读检查成功。

本修订已执行：严格 cargo clippy --all-targets --locked -- -D warnings 通过（上级 clippy-owner-revision2.txt）；cargo test --lib registry_ 18/18（registry-owner-revision2.txt，包含 17 本次新增和 1 旧 management 匹配）；旧原字节 shared_input_queue_acceptance 39/39（queue39-owner-revision2.txt）。domain53/53 是首候选中相同领域源的历史证据，未将它虚写成本修订重跑。开发回归不替代新并发问题的独立判定。

其余实现说明、失败历史和范围限制见 candidate-v1/HANDOFF.md 与上级 shutdown-ownership-supplement.md。首候选额外全lib尝试在受限进程中因Windows权限相关失败和无界管道等待被中止，不能算全量通过；精确自有test exe余量0，证据在 candidate-v1/post-freeze-environment-evidence.json。旧 management_independent.rs:240 的 schema4 版本固定断言已单独记录实际5/预期4失败，由独立tester提供版本适配；coder未改旧独立断言。root将于集成后在实际owner require_escalated环境执行全工程。

当前仍等待 tester/reviewer 对本修订的正式同源验收。未改根产品、索引、旧设计/报告/独立oracle、baseline或真实用户数据；无新依赖、无提交推送。真实 WSS 认证、Harness writer、首写许可及协议终态归因不属于本交付，live默认Blocked。
