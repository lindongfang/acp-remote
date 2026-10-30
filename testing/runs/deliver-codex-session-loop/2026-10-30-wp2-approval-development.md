# WP2 审批事务开发阶段

日期：2026-10-30。候选仅在 `C:/Users/jack/.codex/worktrees/codex-session-wp2/acp-remote`；本文记录开发结果，不代签独立检查或最终产品验收。

## 候选与检查

冻结清单 `.tmp/wp2-2.3-logs/frozen-files.json` 包含相对主D已集成2.2的14个变动文件及各自主目录基线摘要；清单SHA256为 `88A094ABC6682B3F7CA70AEB640725928C36B09A7C6489EFBD9BF68ED24B07AA`。正式开发日志索引为同目录 `verification.json`，owner、fmt、严格Clippy、all-targets、history专项均记录exit0。全部测试日志SHA256为 `60905B88228DBFF73094F4198ECC6F33B84DE9E52128A1A7782E7458CDE6B0F0`，实际200项库测试及4项协议向量；history专项34项，日志SHA256 `B649D793DDC16F7A2BC898E1EEC0F12537FEA7ABF2C3F7DD8E0A90F90439AC7A`。

实现分为审批领域类型、SQLite仓储和prepare/commit应用端口。审批记录保留原始请求、选项、精确工具引用及原生请求标量类型，来源关联会话/Run/实例/连接代际；决定与Command及审计事务提交，交付单独一次认领，写入传输与对端执行分开。强刷在外层协调锁外准备，短事务前检查同一历史owner和版本检查点。裁决与输入共享actor/request去重空间。取消、终态、代际失效和重启保留既有决定、阻止旧请求重用；未知交付不自动重发。

10组新增开发测试包含独立SQLite池竞争、真正延迟外键COMMIT失败、强刷失败及准备后dirty变化、共享请求命名空间、失效/重启和审计边界；真实/替身具体范围由独立测试报告另核验。先前失败编译/检查日志保留，不能把第一次失败记录写成通过。

## 当前边界

尚未集成主D，独立tester/reviewer正在同一冻结候选上审验。实际可信发行者、client_gate与资源owner协调、最终原生writer核验仍属于WP3，公共内部坐标或SQLite已决定状态不构成外部授权。测试中的工具/输出owner为明确替身，不签真实Harness、Web、Windows进程树或CL最终通过，task2.3不勾选。

schema2仍是未发布草案，其早期中间布局会被严格校验拒绝；不允许清库或自动重建，schema1升级保留已有身份/信任/审计。没有启动用户实际节点、修改其原生会话或凭据。未提交或推送。

## 同版独立结果与主目录集成

独立报告为本目录 `tester-wp2-approval.md` 与 `reviewer-wp2-approval.md`。tester实际10审批+4外部补验+34历史回归通过，包含两独立池屏障竞争、真实历史COMMIT故障零裁决、10万容量及未知交付分支。首轮外部probe错误假设“停止的owner强刷后能自行恢复授权”被撤销，失败日志保留；最终更严格验证成功重刷仍不自动重授权。reviewer独立核14文件、五日志摘要及实际重跑10审批/34历史，限定内层PASS。两者均未代签真实issuer/writer/ACP/Web。

主执行在集成前逐项验证候选sha256和D目标mainSha256、新文件不存在，再复制14文件并逐项重算一致。持久清单为 `2026-10-30-wp2-approval-source.json`，与冻结原件字节相同。集成后主D真实owner下`npm.cmd run check` exit0（Web50、Rust200库+4向量、fmt/Clippy/Web生产构建）；日志 `.tmp/wp2-integrated/approval-check.log` SHA256 `DB65928059165B5763E43AC5E74F569B20F96974E23E677ADC5CC60252B9EEE3`。随后`npm.cmd run build:rust` exit0，日志 `.tmp/wp2-integrated/approval-build.log` SHA256 `D5E1D823D7628C693BA88215D6A7594CD27D9D70357D9C0132580C475BEA09DC`，没有启动用户节点。

上文“尚未集成”保留冻结时阶段，当前以本节为准。task2.3最终判据仍待真实产品，不勾。WP3协议与平台已按plan隔离拆分开工，主目录产品保持本阶段固定版本，后续候选独立审验后串行集成。
