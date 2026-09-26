# Verification: node-link-owner

> 主 Agent 持续维护的执行证据与变更历史；结构遵循 `openspec/schemas/agentic/templates/verification.md`。
> 结果仅为 PASS / FAIL / BLOCKED / NOT_APPLICABLE。

## Target

- 变更：`node-link-owner`（changeDir：`D:\Project\acp-remote\openspec\changes\node-link-owner`，权威规划根同）。
- 代码仓库：`D:\Project\acp-remote`。
- 目标主分支：`refs/heads/main` = `94a64e1f15d26f54a6601985fd13b1440fec8170`（2026-09-26 recon 核实，远端 origin 同 SHA；报告 `reports/env1-recon.md`）。
- 执行基线/提交：实现 worktree `D:\Project\acp-remote-wt\node-link-owner`（分支 `agentic/node-link-owner`，起点 94a64e1f）；权威规划根 `D:\Project\acp-remote`。其余各记录在对应行注明固定提交。

## Handoff Index

| Task ID | Role / Phase / Stage | Target Revision | Evidence Type / ID | Report Path | Result / Evidence Status | Applicability / Source Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| 1.1 | environment / recon / recon | 94a64e1f15d26f54a6601985fd13b1440fec8170 | RESOURCE / NOT_APPLICABLE | reports/env1-recon.md | PASS / NEW | 本机首次执行；工具链 node v24.19.0 / cargo 1.98.1；基线日志 reports/env1-baseline-npm-check.log、reports/env1-baseline-cargo-test.log |
| 1.2 | coder / implement / work-package | 028a4d2a71ae01364012ffec4da9a85c7f8cf905 | DELIVERY / NOT_APPLICABLE | reports/wp1-handoff.md | PASS / NEW | 契约与所有权确认，编码起点 94a64e1f；无歧义阻塞 |
| 1.3 | coder / implement / work-package | 028a4d2a71ae01364012ffec4da9a85c7f8cf905 | RESOURCE / NOT_APPLICABLE | reports/wp1-handoff.md | PASS / NEW | worktree 独立 target/；无端口/数据库/外部服务；worktree 内 npm ci |
| 2.1 | coder / implement / work-package | 028a4d2a71ae01364012ffec4da9a85c7f8cf905 | DELIVERY / NOT_APPLICABLE | reports/wp1-handoff.md | PASS / NEW | MODULE_ARCHITECTURE.md §3.1/§4.9/§5 口径，含 acpr-wire 矩阵格负向对照证据（wp1-deps.log） |
| 2.1/2.2/2.3 | reviewer / review / work-package | 028a4d2a71ae01364012ffec4da9a85c7f8cf905 | REVIEW / RV1-WP1 | reports/rv1-wp1.md | PASS / NEW | 独立检视（reviewer 子 Agent 02978e92，不继承实现对话；只读无 shell，静态核对 + 锁/指纹交叉验证）；4 项非阻断发现见 Review Findings |
| 3.1 | coder / implement / work-package | 028a4d2a71ae01364012ffec4da9a85c7f8cf905 | CHECK / PV1 | reports/wp1-verify.md | PASS / NEW | `npm run verify` 退 0（14 项子检查逐条单独执行）；718 passed / 0 failed / 2 ignored；原始日志 reports/du1-pv1.log（W0/WP1 轮次） |
| 3.1 | coder / implement / work-package | 028a4d2a71ae01364012ffec4da9a85c7f8cf905 | CHECK / PV2 | reports/wp1-verify.md | PASS / NEW | `check-crate-boundaries` 两次独立执行退 0，含 server→acpr-wire 格；执行者如实记录了一次自身包装脚本导致的假失败及受控复现（非仓库问题，证据在 du1-pv1.log） |
| 2.2 | coder / implement / work-package | 028a4d2a71ae01364012ffec4da9a85c7f8cf905 | CHECK / PV2 | reports/wp1-handoff.md | PASS / NEW | axum 0.8 + rustls 0.23(ring)/tokio-rustls 0.26 + rustls-pki-types + rcgen 0.14(dev)；证据 reports/wp1-deps.log（762 行）；rustls-pemfile→rustls-pki-types 偏离见 Check Plan Changes |
| 2.3 | coder / implement / work-package | 028a4d2a71ae01364012ffec4da9a85c7f8cf905 | CHECK / PV2 | reports/wp1-handoff.md | PASS / NEW | npm run check（10 道）+ check-crate-boundaries + cargo fmt --check + cargo check -p server，日志 wp1-deps.log |
| RV1-WP1-F1/F3/F4 | coder / fix / work-package | af9064ed6dcd3e88ab76b825253fb2dc95b74967 | DELIVERY / RV1-WP1 | reports/wp1-fix1-handoff.md | PASS / NEW | 三项非阻断发现的修复提交；fmt/boundaries/npm check 复绿 |
| 2.4 | coder / implement / work-package | 4e8a1075a48e004808f498447b54e45dbff824ce | CHECK / PV3 | reports/wp2-handoff.md | PASS / NEW | transport::net 落地（16 文件 +4318/−7）；lib 161 通过（net 72）；R1–R11 映射见 handoff |
| 2.5 | coder / implement / work-package | 4e8a1075a48e004808f498447b54e45dbff824ce | CHECK / PV3 | reports/wp2-handoff.md | PASS / NEW | TLS 两模式 + 连接级上限；R12–R18；cfg(windows) 权限用例本机执行（pv5-windows-nodelink.log）；cfg(unix) 由 Linux CI 覆盖 |
| 2.6 | coder / implement / work-package | 4e8a1075a48e004808f498447b54e45dbff824ce | CHECK / PV3 | reports/wp2-handoff.md | PASS / NEW | fmt/clippy/test 全绿、无零用例、unsafe 零命中；原始日志 wp2-transport-net.log |
| 2.4/2.5/2.6 | reviewer / review / work-package | 4e8a1075a48e004808f498447b54e45dbff824ce | REVIEW / RV1-WP2 | reports/rv1-wp2.md | PASS / NEW | 独立检视（reviewer 子 Agent ac26e1d8，不继承实现对话）；3 条 MINOR 见 Review Findings；附带复核 RV1-WP1-F1/F3/F4 全部已解决 |
| 3.3 | coder / implement / work-package | 4e8a1075a48e004808f498447b54e45dbff824ce | CHECK / PV3 | reports/wp2-verify.md | PASS / NEW | `cargo test -p server` 189 passed / 0 failed / 0 ignored；net 72 用例真实执行；--list 声明=执行一致 |
| 3.3 | coder / implement / work-package | 4e8a1075a48e004808f498447b54e45dbff824ce | CHECK / PV1 | reports/wp2-verify.md | PASS / NEW | `npm run verify` 14 子检查逐个独立执行全 0；workspace 790 passed / 0 failed / 2 ignored；日志 du1-pv1.log §S9–§S16（对齐 RV1-WP2 的 N6） |
| 3.3 | coder / implement / work-package | 4e8a1075a48e004808f498447b54e45dbff824ce | CHECK / PV2 | reports/wp2-verify.md | PASS / NEW | check-crate-boundaries 退 0（12 crate 含 acpr-wire 格） |
| RV1-WP2-F1/F2/F3/N2/N3 | coder / fix / work-package | 50a5af420ec5c25e521dd93aeb3dc9bf3a732ab5 | DELIVERY / RV1-WP2 | reports/wp2-fix1-handoff.md | PASS / NEW | 修复批落地（6 文件，net/**）；PV3 复绿；N3 已读 axum 0.8.9 源码钉死双头口径并补探针用例 |
| RV1-WP2-F1/F2/F3/N2/N3 | reviewer / recheck / work-package | 50a5af420ec5c25e521dd93aeb3dc9bf3a732ab5 | REVIEW / RV2-WP2FIX | reports/rv2-wp2fix.md | PASS / NEW | 独立复核（reviewer 子 Agent 0efa93e0，不继承实现对话）：F1/F2/F3 与 N2/N3 全部闭环；F3 握手池逐点复核（所有权/取消/背压/死锁自由/关闭序列）成立；3 条新非阻断发现见 Review Findings |
| RV2-WP2FIX-F1/F3 | coder / fix / work-package | 041aeb043d7b585b43faaaf1f40a330854b80784 | DELIVERY / RV2-WP2FIX | reports/wp2-fix2-handoff.md | PASS / NEW | 握手池饱和守卫用例（红绿实测：旧实现 Elapsed/新实现 0.52s）+ 两处承重注释；lib 165→166、net 76→77 只增不减 |
| RV1-WP4-F1..F6 | reviewer / recheck / work-package | 59bf1058851deb5b83c875a02f8046c815a9c4f0 | REVIEW / RV2-WP4 | reports/rv2-wp4.md | PASS / NEW | 六条全部已解决，无回归证据；broker.rs 连带核实为真；2 条新 P2（文档口径）当场修复于 2937850f |
| RV2-WP4-F1/F2 | main / fix / work-package | 2937850f607ea54792a55649a82b7f6878808830 | DELIVERY / RV2-WP4 | reports/rv2-wp4.md | PASS / NEW | 主 Agent 文档修正（§4 计数、§5.1 收尾指针）；npm run check 16/16 绿；RV1-WP5 附带复核 |
| 2.13 | coder / implement / work-package | 6b1f0b9cb100638eb9e15ca414d5013a86aebaf1 | CHECK / PV3 | reports/wp5-handoff.md | PASS / NEW | catalog 投影（可见性=grant 交集派生，裁决 b）+ revision=AuditStore::watermark()（store.head() 占位已弃用核销） |
| 2.14 | coder / implement / work-package | 6b1f0b9cb100638eb9e15ca414d5013a86aebaf1 | CHECK / PV3 | reports/wp5-handoff.md | PASS / NEW | attach/generation、快照（无正文、digest 原始字节规则）、origin 重放、epoch 校验 |
| 2.15 | coder / implement / work-package | 6b1f0b9cb100638eb9e15ca414d5013a86aebaf1 | CHECK / PV3 | reports/wp5-handoff.md | PASS / NEW | 扇出（publish 投 id + async 取正文）+ digest/256KiB/raw 保真 + ACK 单调性；send_with_frame 加法（快照 digest 需要实际帧文本） |
| 2.16 | coder / implement / work-package | 6b1f0b9cb100638eb9e15ca414d5013a86aebaf1 | CHECK / PV3 | reports/wp5-handoff.md | PASS / NEW | PV3 全绿（server lib 242，catalog 3 + resource 14）；日志 wp5-catalog-resource.log |
| 2.13/2.14/2.15/2.16 | reviewer / review / work-package | 6b1f0b9cb100638eb9e15ca414d5013a86aebaf1 | REVIEW / RV1-WP5 | reports/rv1-wp5.md | PASS / NEW | 独立检视（reviewer b05e7d1b）；6 条 P2 见 Review Findings；RV2-WP4-F1/F2 附带复核已闭环；PV5 轮次记 PENDING（归 2.22/3.9 门禁） |
| 3.9 | coder / implement / work-package | 6b1f0b9cb100638eb9e15ca414d5013a86aebaf1 | CHECK / PV3 | reports/wp5-verify.md | PASS / NEW | 四 crate 582 passed / 0 failed；catalog 3/3、resource 14/14 真实执行 |
| 3.9 | coder / implement / work-package | 6b1f0b9cb100638eb9e15ca414d5013a86aebaf1 | CHECK / PV1 | reports/wp5-verify.md | PASS / NEW | npm run verify 907 passed / 0 failed / 2 ignored；14 子检查逐个全 0；du1-pv1.log §WP5-0…9 |
| 3.9 | coder / implement / work-package | 6b1f0b9cb100638eb9e15ca414d5013a86aebaf1 | CHECK / PV2 | reports/wp5-verify.md | PASS / NEW | boundaries 退 0（12 crate） |
| 3.9 | coder / implement / work-package | 6b1f0b9cb100638eb9e15ca414d5013a86aebaf1 | CHECK / PV5 | reports/wp5-verify.md | PASS / NEW | 本机 342 passed / 0 failed（declared==executed）；清理 1 个早前残留临时目录 |
| 3.9（修复后重跑） | coder / implement / work-package | 27ad3f800c7821846b8991ae7d61b67f59e8ba24 | CHECK / PV3 | reports/wp5-fix-verify.md | PASS / NEW | 五 crate 625 passed / 0 failed；node_link 79/79（含 6 条新用例逐条 ok） |
| 3.9（修复后重跑） | coder / implement / work-package | 27ad3f800c7821846b8991ae7d61b67f59e8ba24 | CHECK / PV1 | reports/wp5-fix-verify.md | PASS / NEW | workspace 913 passed / 0 failed / 2 ignored；15 子检查逐个全 0（计数 +6 与修复批用例数一致） |
| 3.9（修复后重跑） | coder / implement / work-package | 27ad3f800c7821846b8991ae7d61b67f59e8ba24 | CHECK / PV5 | reports/wp5-fix-verify.md | PASS / NEW | 本机 348 passed / 0 failed（declared==executed） |
| 3.11 | coder / implement / work-package | 8bc2ff3e078549cebed2ab99b5981d5ed37e0581 | CHECK / PV3 | reports/wp6-verify.md | PASS / NEW | 五 crate 654 passed / 0 failed；command 25/25 真实执行 |
| 3.11 | coder / implement / work-package | 8bc2ff3e078549cebed2ab99b5981d5ed37e0581 | CHECK / PV1 | reports/wp6-verify.md | PASS / NEW | workspace 942 passed / 0 failed / 2 ignored；15 子检查逐个全 0；du1-pv1.log §W5/WP6 轮次 |
| 3.11 | coder / implement / work-package | 8bc2ff3e078549cebed2ab99b5981d5ed37e0581 | CHECK / PV2 | reports/wp6-verify.md | PASS / NEW | boundaries 退 0 |
| 3.11 | coder / implement / work-package | 8bc2ff3e078549cebed2ab99b5981d5ed37e0581 | CHECK / PV5 | reports/wp6-verify.md | PASS / NEW | 本机 376 passed / 0 failed；清理 WP4a 遗留临时目录 1 个 |
| 3.11（修复后重跑） | coder / implement / work-package | f630859fec505773995adb5f6cf800c4c2da3115 | CHECK / PV3 | reports/wp6-fix-verify.md | PASS / NEW | 665 passed / 0 failed（增量逐目标对上修复用例） |
| 3.11（修复后重跑） | coder / implement / work-package | f630859fec505773995adb5f6cf800c4c2da3115 | CHECK / PV1/PV2 | reports/wp6-fix-verify.md | PASS / NEW | workspace 953 passed / 0 failed；14 子检查全 0；boundaries 退 0 |
| 3.11（修复后重跑） | coder / implement / work-package | f630859fec505773995adb5f6cf800c4c2da3115 | CHECK / PV5 | reports/wp6-fix-verify.md | PASS / NEW | 本机 381 passed / 0 failed；清理早前残留 2 个 |
| RV1-WP6-F1 | coder / fix / work-package | 69419dd（完整 SHA 见 wp6-fix1-handoff.md） | DELIVERY / RV1-WP6 | reports/wp6-fix1-handoff.md | PASS / NEW | session.create 持久幂等（core create_session 带 requestId + settle_session_create；存储回填；进程内表移除，持久记录为唯一权威）；§4/§5.1/§6 第 20 条同步；红向：重启后重复创建被拦截 |
| RV1-WP6-F2/F3/F8 | coder / fix / work-package | 709ef55（同上） | DELIVERY / RV1-WP6 | reports/wp6-fix1-handoff.md | PASS / NEW | poll_pending 按 handles() 回收；终态带 turnId/version；永久失败映射非 retryable 登记码；红向证据齐 |
| RV1-WP6-F6/F10/F11/F12 | coder / fix / work-package | f630859fec505773995adb5f6cf800c4c2da3115 | DELIVERY / RV1-WP6 | reports/wp6-fix1-handoff.md | PASS / NEW | §12.7 注记 + RecordingCloser 回读断言 + 死代码删除 + watcher 取消用例；红向证据齐 |
| RV1-WP6-F1..F12 | reviewer / recheck / work-package | f630859fec505773995adb5f6cf800c4c2da3115 | REVIEW / RV2-WP6 | reports/rv2-wp6.md | PASS / NEW | 独立复核（reviewer 1ccf40bc）：8 条全部已解决、无回归（+11 与日志一致）；4 条新 P2（见 Review Findings） |
| RV1-WP5-F3/F4/F5 | coder / fix / work-package | 82e7c52c2dd3da88c12c11fe5f375438d6a4071b | DELIVERY / RV1-WP5 | reports/wp5-fix1-handoff.md | PASS / NEW | owner 校验（attach→not_found / ack→sequence_invalid，裁决 A）+ 状态表回收 + warn! 日志；红向证据齐 |
| RV1-WP5-F1 | coder / fix / work-package | fc0cbc03a8764f2a6d7a34fa671453b50d33db87 | DELIVERY / RV1-WP5 | reports/wp5-fix1-handoff.md | PASS / NEW | catalog 3 条路由级用例（过滤/分批/空集），红向证据 |
| RV1-WP5-F6 | coder / fix / work-package | 27ad3f800c7821846b8991ae7d61b67f59e8ba24 | DELIVERY / RV1-WP5 | reports/wp5-fix1-handoff.md | PASS / NEW | §4 计数句改写（三条端口 seam + 四个用例入口）；check:drift/docs 绿 |
| RV1-WP5-F1/F3/F4/F5/F6 | reviewer / recheck / work-package | 27ad3f800c7821846b8991ae7d61b67f59e8ba24 | REVIEW / RV2-WP5 | reports/rv2-wp5.md | PASS / NEW | 独立复核（reviewer 2f422f19）：F1/F4/F5/F6 已解决、F3 部分解决（残余记 RV2-WP5-F1）；2 条新非阻断发现 |
| RV2-WP5-F1/F2 | coder / fix / work-package | 5e2d17c7ac4074b1d473dffab664eed48f48f21e | DELIVERY / RV2-WP5 | reports/wp5-fix2-handoff.md | PASS / NEW | 回收路径补全（两段红向证据）+ let _ 清理；server 250 passed（+2） |
| 2.17 | coder / implement / work-package | 8bc2ff3（完整 SHA 见 wp6-handoff.md） | CHECK / PV3 | reports/wp6-handoff.md | PASS / NEW | 命令管线（wire 校验→授权交集→幂等→派发→终态映射 + watcher 推送）；core 无会话命令授权修正（3638ebd） |
| 2.18 | coder / implement / work-package | 8bc2ff3（同上） | CHECK / PV3 | reports/wp6-handoff.md | PASS / NEW | session.create 硬约束 + in-flight/限流（8bc2ff3 按 R45 调整用例） |
| 2.19 | coder / implement / work-package | 8bc2ff3（同上） | CHECK / PV3 | reports/wp6-handoff.md | PASS / NEW | ConnectionCloser 扩 close_node/export_revoked（4cbdf8a）；compose.rs 连带已单列 |
| 2.20 | coder / implement / work-package | 8bc2ff3（同上） | CHECK / PV3 | reports/wp6-handoff.md | PASS / NEW | 全量门禁绿（workspace 83 目标 0 failed；server lib 276）；日志 wp6-command.log |
| RV1-WP4-F2 | coder / fix / work-package | 556d170e1130c6ff761b57162c3530ef1bc8af00 | DELIVERY / RV1-WP4 | reports/wp4-fix1-handoff.md | PASS / NEW | 认证收尾两条路径同事务推进 last_connected_at（consume_pairing + 新增 record_node_connected）；§3.5/§4/§5.3/§11.6 同步；check:drift 90 签名一致；broker.rs 替身一行已批准连带 |
| RV1-WP4-F1/F3/F4/F5/F6 | coder / fix / work-package | 2917401cdbfaa21dc97786d0a792c1081ebb805a | DELIVERY / RV1-WP4 | reports/wp4-fix1-handoff.md | PASS / NEW | 序号账本单临界区（红向 [1,3]→[1,2]）；配额随握手释放 + 三条行为用例；未知 type 同口径记账（红向证据）；不可达分支注明；server lib 225、conn 19→24 |
| RV1-WP4-F2（存储用例补齐） | coder / fix / work-package | 59bf1058851deb5b83c875a02f8046c815a9c4f0 | DELIVERY / RV1-WP4 | reports/wp4-fix1b-handoff.md | PASS / NEW | admin_store.rs +442/−3（36→40 用例），含 4 条红向证据 |
| RV1-WP3-F2/F3/F4/F5 | coder / fix / work-package | 443c6c09ce724ca1446e733a17da445d6d0b8547 | DELIVERY / RV1-WP3 | reports/wp3-fix2-handoff.md | PASS / NEW | §5.1 口径收窄 + has_secret 生产用途登记 + 用例注释与断言 + 409 自愈注释 + NODE_LINK_PROTOCOL §13.3 澄清句；design.md D12 由主 Agent 同步 |
| 2.28 | coder / implement / work-package | a811932（完整 SHA 见 wp4-handoff.md） | DELIVERY / NOT_APPLICABLE | reports/wp4-handoff.md | PASS / NEW | core 节点握手只读视图 + 认证留痕入口；core 104 全绿；§4/§5 文档同步、check:drift 通过 |
| 2.10 | coder / implement / work-package | 038349b67a4c141120d0d9a26f6d221d43c2fb39 | CHECK / PV3 | reports/wp4-handoff.md | PASS / NEW | conn 模块握手/信封/序号/feature；conn 25 用例（17 端到端 + 8 单测） |
| 2.11 | coder / implement / work-package | 038349b67a4c141120d0d9a26f6d221d43c2fb39 | CHECK / PV3 | reports/wp4-handoff.md | PASS / NEW | limits 只下调、心跳/静默/慢消费者、认证限流接线；node.auth_failed 写入用例（F4） |
| 2.12 | coder / implement / work-package | 038349b67a4c141120d0d9a26f6d221d43c2fb39 | CHECK / PV3 | reports/wp4-handoff.md | PASS / NEW | fmt/clippy/test/npm check 全绿；unsafe 零命中；日志 wp4-handshake.log |
| 2.25 | coder / implement / work-package | 13f0a16b55203a8224771a23fc432fac8b5184bb | DELIVERY / NOT_APPLICABLE | reports/wp3-contract-handoff.md | PASS / NEW | Actor::PairingClaimant + NodeAuthenticated/NodeAuthFailed + 配对通道用例 + consume_pairing；core 100 用例全绿；文档三份同步 |
| 2.26 | coder / implement / work-package | 13f0a16b55203a8224771a23fc432fac8b5184bb | CHECK / PV3 | reports/wp3-contract-handoff.md | PASS / NEW | v2→v3 迁移 + consume 落盘；migration 8+1ignored、admin_store 36、enum_coverage 2；check:drift 逐条一致；日志 wp3-contract.log；首次运行 f1ef504d 中断后 resume（EX2） |
| 2.25/2.26 | reviewer / review / work-package | 13f0a16b55203a8224771a23fc432fac8b5184bb | REVIEW / RV1-WP3C | reports/rv1-wp3c.md | PASS / NEW | 独立检视（reviewer 3a61aa1a）；5 项非阻断发现见 Review Findings；PV3 正式轮次在该版本中记 PENDING（归 2.9/3.x 门禁） |
| 2.25/2.26/2.27/2.7/2.8/2.9 | reviewer / review / work-package | d127a802b75d63cd14789ea3ad504d89ce57f3f3 | REVIEW / RV1-WP3 | reports/rv1-wp3.md | PASS / NEW | 独立检视（reviewer ba4cb688）；2.25/2.26 深检复用 RV1-WP3C（增量不含端口/DDL/枚举/状态机代码）；附带复核五项全已解决 |
| 3.5 | coder / implement / work-package | d127a802b75d63cd14789ea3ad504d89ce57f3f3 | CHECK / PV3 | reports/wp3-verify.md | PASS / NEW | `cargo test -p server` 221 passed / 0 failed（--list 声明=执行；node_link 24/24、net 80/80） |
| 3.5 | coder / implement / work-package | d127a802b75d63cd14789ea3ad504d89ce57f3f3 | CHECK / PV1 | reports/wp3-verify.md | PASS / NEW | `npm run verify` 15 子检查逐个全 0；workspace 839 passed / 0 failed / 2 ignored；日志 du1-pv1.log §S17–§S26 |
| 3.5 | coder / implement / work-package | d127a802b75d63cd14789ea3ad504d89ce57f3f3 | CHECK / PV2 | reports/wp3-verify.md | PASS / NEW | check-crate-boundaries 退 0 |
| 3.5 | coder / implement / work-package | d127a802b75d63cd14789ea3ad504d89ce57f3f3 | CHECK / PV5 | reports/wp3-verify.md | PASS / NEW | `cargo test -p server -p app` 本机 Windows 293 passed / 0 failed；pv5-windows-nodelink.log WP3 轮次 |
| 3.7 | coder / implement / work-package | 319c77b715eb218ee81696aeec8257d268e0930a | CHECK / PV3 | reports/wp4-verify.md | PASS / NEW | server 248 + 追加三 crate 共 472 passed / 0 failed；conn 27/27 真实执行 |
| 3.7 | coder / implement / work-package | 319c77b715eb218ee81696aeec8257d268e0930a | CHECK / PV1 | reports/wp4-verify.md | PASS / NEW | `npm run verify` 15 子检查全 0；workspace 876 passed / 0 failed / 2 ignored；du1-pv1.log §S27–§S36（前文 md5 校验不变） |
| 3.7 | coder / implement / work-package | 319c77b715eb218ee81696aeec8257d268e0930a | CHECK / PV2 | reports/wp4-verify.md | PASS / NEW | check-crate-boundaries 退 0 |
| 3.7 | coder / implement / work-package | 319c77b715eb218ee81696aeec8257d268e0930a | CHECK / PV5 | reports/wp4-verify.md | PASS / NEW | 本机 Windows 320 passed / 0 failed（server 248 + app 72）；pv5-windows-nodelink.log WP4 轮次 |
| 3.7 | main / note / work-package | 319c77b715eb218ee81696aeec8257d268e0930a | CHECK / PV3 | reports/wp4-verify.md | NOT_APPLICABLE / INVALID（部分） | 该轮证据绑定修复前提交 319c77b7；RV1-WP4 判 FAIL（F1/F2 阻断），wp4-fix1 修复后 PV3 须在新提交重跑 |
| 3.7（修复后重跑） | coder / implement / work-package | 59bf1058851deb5b83c875a02f8046c815a9c4f0 | CHECK / PV3 | reports/wp4-fix-verify.md | PASS / NEW | 477 passed / 0 failed（含 conn 32/32、admin_store 40）+ storage 120 passed |
| 3.7（修复后重跑） | coder / implement / work-package | 2937850f607ea54792a55649a82b7f6878808830 | CHECK / PV1/PV2/PV5 | reports/wp4-fix-verify.md | PASS / NEW | B 段（当前 HEAD，权威）：npm run verify 885 passed / 0 failed；boundaries 退 0；PV5 325 passed / 0 failed；执行者正确处理了主 Agent 中途文档提交（A/B 双段，B 段为准） |
| 2.27 | coder / implement / work-package | 259920a（完整 SHA 见 wp3-handoff.md） | DELIVERY / NOT_APPLICABLE | reports/wp3-handoff.md | PASS / NEW | identity-auth status proof 入口 + core pairing_channel_view 绑定只读 + §5.1 顺序规则；core 102 / identity-auth 全绿 |
| 2.7 | coder / implement / work-package | 84678e4b98250ca8e0fe302051d5d3072f1a7397 | CHECK / PV3 | reports/wp3-handoff.md | PASS / NEW | node_link 骨架 + claim 端点（状态码全族、幂等重试、ownerProof、安全头、限流） |
| 2.8 | coder / implement / work-package | 84678e4b98250ca8e0fe302051d5d3072f1a7397 | CHECK / PV3 | reports/wp3-handoff.md | PASS / NEW | status 端点（一律 200 五状态、proof 401、nonce 重试、approved 回已授予 grants） |
| 2.9 | coder / implement / work-package | 84678e4b98250ca8e0fe302051d5d3072f1a7397 | CHECK / PV3 | reports/wp3-handoff.md | PASS / NEW | PV3 全绿（lib 189 + 集成 14/6/4/4）；日志 wp3-pairing-http.log |
| RV1-WP3C-F2/F3/F5+安全头 | coder / fix / work-package | d127a802b75d63cd14789ea3ad504d89ce57f3f3 | DELIVERY / RV1-WP3C | reports/wp3-fix1-handoff.md | PASS / NEW | §3.5 字段语义钉死、§7.2 步骤 4、migration.rs 注释、register_post 默认响应头 seam（配对路径 413/Host-400/405 均带四个安全头） |

## Checks

| Check ID / Stage / Work Package | Revision / Base | Scope | Executor | Command / Steps | Environment | Result / Exit Code | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| BASELINE（recon）/ recon / 全变更 | 94a64e1f | 合同门禁基线 | environment（scout 子 Agent 14c39ccc） | `npm run check`（D:\Project\acp-remote） | node v24.19.0 / npm 12.0.2 | PASS / 0（10 道子检查全 OK） | reports/env1-baseline-npm-check.log |
| BASELINE（recon）/ recon / 全变更 | 94a64e1f | workspace 测试基线 | environment（同上） | `cargo test --locked --workspace --all-features` | cargo/rustc 1.98.1 | PASS / 0（718 passed / 0 failed / 2 ignored） | reports/env1-baseline-cargo-test.log |
| PV1 / branch（W0/WP1）/ WP1 | 028a4d2a | 全量统一入口 | worker 59457caf | `npm run verify`（D:\Project\acp-remote-wt\node-link-owner） | node v24.19.0 / cargo 1.98.1 / Windows | PASS / 0（718 passed / 0 failed / 2 ignored；2 个 ignored 为仓库声明的辅助/生成器） | reports/du1-pv1.log、reports/wp1-verify.md |
| PV2 / branch（W0/WP1）/ WP1 | 028a4d2a | §5 矩阵逐条 | worker 59457caf | `node scripts/check-crate-boundaries.mjs` | 同上 | PASS / 0（两次独立执行） | reports/du1-pv1.log |

## Check Plan Changes

- 2026-09-26（WP3 合同扩展，主 Agent 裁决）：2.25/2.26 的写入范围扩至测试文件（`crates/core/src/model/tests.rs`、`crates/storage-sqlite/tests/{enum_coverage,migration,commit,admin_store}.rs` 及确需的同目录既有测试文件）——原范围漏列，完成条件强制要求这些测试。夹具策略选 A：不新增 v3 二进制夹具（v2→v3 用既有 `fixtures/storage/v2/empty.sqlite3`、v1→v3 沿用 `from-v1.sqlite3` 并加强断言、「过新拒绝」改用临时副本 + `PRAGMA user_version = 4`）；`owned_command` 的 actor_kind CHECK 保持三元组不动，enum_coverage 期望拆分；全部断言改动须在 coder handoff 中逐条列出原/新/理由供 reviewer 核对。
- 2026-09-26（3.5 派发更正）：主 Agent 的 3.5 派单漏列 [PV5]（权威 tasks.md 要求 [PV3]+[PV5] 本机），执行者按权威判据请示后裁决附加执行（选 B）；后续 3.7/3.9/3.11/3.13/3.15 派单必须带 [PV5]。
- 2026-09-26（用户裁决，原话「A」）：WP4 发现 v1 wire 内部不一致（连接 transcript 域含 tag 6 `catalogRevision` 但 `node.challenge`/`node.hello` 不带该字段，首次连接握手不可能完成）——按方案 A 给 `node.challenge` 增必需字段 `catalogRevision`，同步 NODE_LINK_PROTOCOL §12.2/修订记录、handshake.schema.json、fixtures、node-link-protocol 类型与 conn 实现；design.md 增 D13，tasks.md 增 2.29；§2.5 JSON 结构上限执行点列入 2.30（实现既定上限，不改合同文本）。
- 2026-09-26（WP4 实现前，主 Agent 裁决）：coder 实测发现握手期只读（节点信任/验签公钥/待消费配对/serverEpoch）与 `node.auth_failed` 写入在 core 用例面无合法入口（均为 LocalCli-only）。裁决：延续用户裁决 A 的同一方向（同类空档同一解法，不重复请示），core 补窄入口（按 accessNodeId 绑定、audit 只追加两类节点动作），授权 WP4 改 `crates/core/src/{use_cases.rs,ports.rs}` 并同步 §4/§5 + check:drift；新增任务 2.28，plan.md WP4/W3 行已同步。catalogRevision 占位口径：优先复用存储既有单调水位，否则进程内单调计数器并标注 WP5 接管。
- 2026-09-26（wp4-fix1 范围裁决）：F2 的完整修复需要 TrustStore 新增 `record_node_connected` 窄方法（重复认证也推进 `last_connected_at`、只前进不倒退），连带 `core/src/broker.rs` 测试替身一行 `unreachable!`——批准该编译连带（与 WP3 替身同款处理），Coder 须同步 §5.3/§11.6 并保证 check:drift 通过。拒绝选项 B（重复认证不推进 = 字段说谎）。
- 2026-09-26（WP5 勘察，主 Agent 裁决）：coder 只读勘察发现 G1–G5 五处缺口。已裁定：G2/G4/G5 按 2.28 先例补窄 seam（node-bound 只读、publish 投 id 由 async 任务取正文扇出、`AuditStore::watermark()` 取 `MAX(owned_audit.audit_id)` 作 catalogRevision 并弃用 store.head() 占位），G3 按「稳定但无语义的派生引用」实现。**G1（信任记录 exportIds 空档）已提交用户裁决（a/b/c），catalog 过滤点先做策略边界、R51/R52 过滤用例待裁决后补**。WP5 写范围按 seam 需要扩展（core/storage/文档 + 编译连带替身）。G1 当时提交用户裁决，结果：选 (b)（2026-09-26 用户原话「先用b实现，a记录一个待办怎么样」）——可见性由 grant 交集派生（未撤销且 scopes ∩ grants 非空）；`exportIds` 独立维度记为后续阶段待办（design.md D14）；协议文档 §8.2 修订与本变更 spec R51/R52 口径已同步，coder 已 steer 通知。
- 2026-09-26（修复后验证轮）：执行者发现主 Agent 的文档提交（2937850f）在轮次中途落入同一分支，未改文件未切提交，改为在当前 HEAD 整套复跑（B 段为权威），并在日志 §WC 逐条更正时间线——处理正确。5 个 `/tmp/acpr-*` 残留目录（fix1b 红向跑的 panic 路径清理失败）已由主 Agent 删除；绿跑不复现。登记为卫生观察项（panic 路径下 TempDir 清理不掉的已知模式，先前有 `2026-09-26-test-temp-dir-cleanup` 归档变更处理过同类问题；本次残留属同一类但不阻断）。
- 2026-09-26（WP6 实现前，主 Agent 裁决三点）：Q1 选 (a)——未终态 mutation 的 `command.status` 回 `command.accepted`（首次 acceptedAt、result=null），终态记录回 `command.terminal`，rejected 记录 terminalEventId 为 null；Q2 选 (a)——`record_node_link_auth` 接受动作集扩到 `authorization.denied`（同步 §4/§10），适配层拒绝路径有审计；Q3 批准 `resource.rs` 两个 additive 公开方法（同 send_with_frame 先例）。**附注修正**：`command.terminal` 是 §12.5 的必需推送——每条被接受的 mutation 在提交方连接挂有所有者的 watcher（断开即取消），终态出现即推；不得以「只能重查」交付。
- 2026-09-26（WP6 实现中，主 Agent 裁决）：`broker.authorize` 的 `Actor::Node` 分支对无会话命令（session.create/command.status/session.list）恒拒（Owner 侧 R66/R72/R73 因此走不通）。裁决选 (1)：Owner 侧无会话命令的授权修正为「信任 grants ∋ required_grant 且存在至少一个对该节点可见的 Export」（D14 口径），会话命令维持现状；`command.status` 靠 (actor_kind, actor_id, request_id) 唯一约束天然隔离，`session.list` 由 adapter 经 catalog 唯一判定点过滤。同步 CORE_PORTS §4/§6 第 5 条/§10，check:drift 须过。
- 2026-09-26（RV1-WP3 发现处置）：F1 以 spec 修订闭环（R26 补 secret 生命周期分支、R28 收窄、新增场景 R88，plan 门禁 PASS）；N-RV1-WP3-1（CORE_PORTS §11 现状句 vs MODULE_ARCHITECTURE §4.9）并入 WP8 收口清单；N-RV1-WP3-2（pairing_channel_view 读时拼接）不处理（无缺陷）。
- 2026-09-26（WP3 HTTP 交付，主 Agent 裁决三个 coder 标记项）：① **修复**——配对路径的接入层预拒绝（413/Host-400）也必须带四个安全头（spec R30/R31「无论成功或失败」覆盖该路径），给 `register_post` 增加每路径默认响应头 seam（transport::net 附加改动，RV1-WP3 复核）；② **接受**——status 在 secret 已清除时对终态回 200、非终态回 401 是 R26/R28 与 R27/R32 的唯一并存读法，RV1-WP3 按 spec 文本核对；③ **保持两份限流器**——WP2 的 `SlidingWindowLimiter` 键为 `IpAddr`，配对 status 键为 `PairingId`，键型不同、窗口逻辑各自几十行，按「不为复用几行代码破坏边界」不合并且不抽象共用件；RV1-WP3 须核对两者都实现 §2.5 的固定速率语义。
- 2026-09-26（WP3 HTTP 阶段，主 Agent 裁决）：coder 实测发现 D12 的三处 seam 缺环（status proof 无 identity-auth 入口、幂等重试缺已固定对端行读取、approved grants 无可读来源、§5.1 措辞与「先读后验」顺序冲突）。裁决：按方案 A 同方向补全（新增任务 2.27：identity-auth status proof 入口 + core 配对通道绑定只读 + §5.1 顺序规则）；status approved 的 grants 取已授予集合；写范围扩至 `crates/identity-auth/src/**` 与 `docs/IDENTITY_AND_AUTH_CONTRACT.md`。plan.md W2 行与 tasks.md 已同步。
- 2026-09-26（用户裁决，原话「A」）：网络侧配对生命周期在 core 用例面的空档按**方案 A** 解决——core 新增 `Actor::PairingClaimant` 变体、配对通道用例入口（claim/status 接受绑定配对的 claimant）、`consume_pairing` 写入口、`AuditAction::NodeAuthenticated`/`NodeAuthFailed`，存储走 v2→v3 表重建迁移；design.md 新增 D12，plan.md 的 WP3 范围与 tasks.md 任务 2.25/2.26 已同步。Specs 行为需求不变（Coverage Index 不受影响）。

- 2026-09-26（WP2 实现中裁决）：Host 边界在「`allowed_hosts` 为空且 `public_origin = null`」（默认配置）下存在合同空档——`CONFIG_REFERENCE.md` §1 的字面规则此时无 Host 可接受，与 spec `node-link-listener` 的 R2（默认 loopback 可达）冲突。裁决口径（主 Agent，WP2 coder 请示后）：两者都空时只接受 loopback 形态 Host（`127.0.0.1`/`[::1]`/`localhost`，忽略端口），依据 `SECURITY_DESIGN.md` §7.2（拒绝任意 Host/DNS rebinding）与 R2。该口径不改动 spec 既有需求（R9–R11 白名单与 public_origin 分支不变）；`CONFIG_REFERENCE.md` §1 的条文补写已列入 WP8（该文件不在 WP2 写范围）。
- 2026-09-26（RV1-WP2 notes 归集）：N1（§4.9 现状注记补 transport::net）与 N7（§1 补「两者都空 → loopback-only」条文并指向 §7.2）并入 WP8 写回清单；N-RV2-1（握手池用例的覆盖表登记注明「非需求性回归保护」）与 N-RV2-2（origin_host 接受 http:// 与协议侧 https-only 不一致，统一口径评估）同列 WP8；N2（WS 路径 Host 拒绝用例）与 N3（Sec-WebSocket-Protocol 双头用例）并入 WP2 修复批 wp2-fix1；N5（认证限流准入闸门）列入 WP4 派发输入；N4（非 loopback 无 public_origin/allowed_hosts 组合的 Host 退化）维持「只告警」——该组合不在 CONFIG_REFERENCE §1 三形态表内，不构成授权面；N6 已由 3.3 的 du1-pv1.log §S9–§S16（绑定 4e8a1075、exit=0、末段完整）对齐。

- 2026-09-26（WP1 交付）：design.md D1 的候选 `rustls-pemfile 2` 上游已归档（GitHub archived，README 指向后继 API），实际登记改用 `rustls-pki-types` 的 `pem` 模块（本就是 rustls 传递依赖，不新增供应商）。范围变化仅依赖清单一项；specs/plan 的需求与检查不变；已同步 design.md D1 措辞。该偏离将在 RV1-WP1 独立检视中重点核对。
- 2026-09-26（RV1-WP1-F2 裁决）：WP1 的 `crates/server/Cargo.toml` 依赖预登记超出 plan 原声明写范围；主 Agent 裁决接受（选项 a），plan.md 的 WP1 Write Scope 与 Execution Waves 单一写入者清单已补登该文件与 `Cargo.lock`。理由：无预登记则 `Cargo.lock` 无新增节点，`check:boundaries` 对 `server→acpr-wire` 格无可判定依赖边；回退成本高于计划文本补登。
- 2026-09-26（WP1 交付）：TLS provider 定为 rustls 自带 `ring`（Apache-2.0 AND ISC、MSRV 1.66、预生成汇编）；落选 `aws-lc-rs`（aws-lc-sys 的 cmake build-dep 非 optional，破坏「固定工具链即可构建」）与 `rustls-rustcrypto`（0.0.2-alpha 停更）。`deny.toml` 未改动（未引入 allow 表外的新 SPDX id）。
- 截至 2026-09-26 apply 启动：plan 阶段 `workflow check --stage plan` 已 PASS，contractDigest `sha256:39b48dbe83c78b387e0b2072fba2672904728ab9d0680758f62f44f6704bfd1b`。

## Dependency Handoffs

| Downstream | Upstream | Accepted Revision / Evidence | Start Revision | Transfer / Inclusion Check | Invalidation |
| --- | --- | --- | --- | --- | --- |
| WP2 | WP1 | af9064ed（WP1 交付 028a4d2a + RV1 修复 af9064ed）；RV1-WP1 PASS（rv1-wp1.md）、PV1/PV2 W0 轮次 PASS（wp1-verify.md） | 94a64e1f（main 基线） | `git log` 核实 agentic/node-link-owner 含两提交；依赖口径冻结于 Cargo.toml `[workspace.dependencies]` | 依赖口径或矩阵变化 → WP2–WP7 复验 |
| WP3 | WP2 | 4e8a1075（WP2 交付）+ RV1-WP2 PASS；041aeb04（RV2 修复复核 PASS） | W1 完成点 | `cargo build -p server` 只经 transport::net 公开项接入配对处理器 | 路由形状变化 → WP3 复验 |
| WP4 | WP2、WP3 | 50a5af42（WP2 fix 后）+ 443c6c09（WP3 含 seam）；RV1-WP3/RV1-WP3C PASS | W2 完成点 | conn 只经 net/配对公开形状；`PeerTrust` 快照取自持久化信任 | 形状变化 → WP4–WP6 复验 |
| WP5 | WP4 | 1efd69ff 前的 WP4 链（含 RV2-WP4 PASS 于 59bf1058 + 主 Agent 文档修正 2937850f） | W3 完成点 | catalog/resource 只经 conn 注册表与 core seam；`send_with_frame` 加法经 RV1-WP5 核对 | 注册表形状变化 → WP5/WP6 复验 |
| WP6 | WP4、WP5 | 5e2d17c7（WP5 全部修复后）+ RV2-WP5 PASS | W4 完成点 | command 经 resource.rs 的 additive seam（两个公开方法），无第二份附件事实来源 | 同上 |

## Runtime Resources

- 1.1 recon（environment）：只读命令 + `target/` 构建副产物；未改动任何受版本控制文件；无需释放的资源。
- 实现 worktree：`D:\Project\acp-remote-wt\node-link-owner`（分支 `agentic/node-link-owner`），其 `target/` 与主仓库 `target/` 物理隔离，满足 1.3 的 `CARGO_TARGET_DIR` 隔离约定；loopback 一律 `127.0.0.1:0`。
- 1.3 的正式隔离记录：WP1 coder（worker 子 Agent 953c1b4c）确认 worktree 自带 `target/` 与主仓库物理隔离（等效独立 `CARGO_TARGET_DIR`），worktree 内 `npm ci` 已执行，本 WP 不使用端口/数据库/外部服务；证据 reports/wp1-handoff.md。

## Review Findings

| ID | Revision | Reviewer | Location | Severity / Impact | Resolution | Recheck Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| RV1-WP1（总评） | 028a4d2a | reviewer 子 Agent 02978e92（不继承实现对话） | reports/rv1-wp1.md | PASS，无 CRITICAL/MAJOR | — | 限制已记录（无 shell/网络，静态核对 + 锁/指纹交叉验证） |
| RV1-WP1-F1 | 028a4d2a | 同上 | Cargo.toml 新增注释 / wp1-deps.log §11.4 | MINOR：注释与日志对 base64 0.22 的归属断言与锁文件事实矛盾 | 主 Agent 裁决：修复（交 WP1 coder 修正注释与日志表述） | 已修复于 af9064ed；RV1-WP2 复核：已解决 |
| RV1-WP1-F2 | 028a4d2a | 同上 | crates/server/Cargo.toml | MINOR：该文件不在 WP1 声明写范围（授权只存在于派发提示） | 主 Agent 裁决：选 (a) 接受预登记——plan.md WP1 写范围与单一写入者清单已补 `crates/server/Cargo.toml`/`Cargo.lock`（本条即裁决记录）；回退会使 §5 矩阵格失去可判定依赖边，不可取 | 已闭环（计划文本即修复） |
| RV1-WP1-F3 | 028a4d2a | 同上 | MODULE_ARCHITECTURE.md 头部 | SUGGESTION：修订记录未加行 | 随 F1 同批修复 | 已修复于 af9064ed；RV1-WP2 复核：已解决 |
| RV1-WP1-F4 | 028a4d2a | 同上 | wp1-deps.log §7.1 | SUGGESTION：日志引用与 amend 后提交不一致 | 随 F1 同批修复（加指针句） | 已修复于 af9064ed；RV1-WP2 复核：已解决 |
| RV1-WP2（总评） | 4e8a1075 | reviewer 子 Agent ac26e1d8（不继承实现对话） | reports/rv1-wp2.md | PASS，无 CRITICAL/MAJOR | — | 附带复核 RV1-WP1-F1/F3/F4 全部「已解决」 |
| RV1-WP2-F1 | 4e8a1075 | 同上 | transport/net/mod.rs 文档 vs http.rs/ws.rs 公开面 | MINOR：「不暴露 axum 类型」与公开 API 不符 | 主 Agent 裁决：按 (a) 改文档句为如实表述（修复批 wp2-fix1） | 已修复于 50a5af42；RV2-WP2FIX 复核：已解决 |
| RV1-WP2-F2 | 4e8a1075 | 同上 | host.rs HostPolicy::new | MINOR：白名单非空时跳过 public_origin 校验 | 修复（无条件校验 + 补用例，wp2-fix1） | 已修复于 50a5af42；RV2-WP2FIX 复核：已解决 |
| RV1-WP2-F3 | 4e8a1075 | 同上 | listener.rs accept 内联 TLS 握手 | MINOR：direct 模式预先认证接入速率可被压至 ≈1/10s | 主 Agent 裁决：按 (a) 修复——握手移出 accept 关键路径（有界并发握手池）；direct 是节点间主要部署形态，可用性弱点不带入候选（wp2-fix1） | 已修复于 50a5af42；RV2-WP2FIX 复核：已解决 |
| RV2-WP2FIX-F1 | 50a5af42 | reviewer 子 Agent 0efa93e0 | listener.rs 握手池 + tests.rs | MINOR：池饱和路径无守卫用例（承重顺序依赖 drop(permit) 先于 send） | 主 Agent 裁决：补饱和用例（允许测试专用注入缩小池容量以保持快速；wp2-fix2） | 已修复于 041aeb04（含红绿实测证据）；待 RV1-WP3 附带复核 |
| RV2-WP2FIX-F2 | 50a5af42 | 同上 | listener.rs:58,66,516-518 | MINOR/report-only：背压成本门槛 = 64 条并发预认证连接 / 单次 10 s | 主 Agent 裁决：记为已知取舍（不改码）；WP4 接线认证限流时顺带评估「按 IP 限制在途握手数」 | 已记录（本条即记录） |
| RV2-WP2FIX-F3 | 50a5af42 | 同上 | listener.rs:557-562 | SUGGESTION：不可达 None 分支缺注释 | 随 wp2-fix2 同批补注释 | 已修复于 041aeb04；待 RV1-WP3 附带复核 |
| RV1-WP3C（总评） | 13f0a16b | reviewer 子 Agent 3a61aa1a（不继承实现对话） | reports/rv1-wp3c.md | PASS，无 CRITICAL/MAJOR | — | 5 项非阻断发现；下四行逐项裁决 |
| RV1-WP3C-F1 | 13f0a16b | 同上 | specs/storage-schema-v2-migration 主规范 | P2：版本推进到 3 但无 MODIFIED 增量 | 主 Agent 已补写 MODIFIED 增量 spec（版本→3 + v2→v3 重建语义），proposal/plan/Coverage Index（R84–R87）同步，plan 门禁 PASS | 已闭环（规划层） |
| RV1-WP3C-F2 | 13f0a16b | 同上 | use_cases.rs/trust.rs 的 Actor::Node 字段语义 | P2：节点配对对端 id 对应哪个字段未钉死 | 主 Agent 裁决：`node` = 对端（claimant/连接对端）节点 id（与 via_node、broker.rs 既有口径一致）；§3.5 文字钉死列入 wp3-fix1，WP4 派发时按此核对 | wp3-fix1 后由 RV1-WP3 附带复核 |
| RV1-WP3C-F3 | 13f0a16b | 同上 | CORE_PORTS_AND_STORAGE.md:798 | P2：v1→v2 步骤 4 描述不存在的中途落盘 | 修复（wp3-fix1） | 已修复于 d127a802；RV1-WP3 复核：已解决 |
| RV1-WP3C-F4 | 13f0a16b | 同上 | §9 判据 14 vs node.auth_failed 无写入用例 | P2：写入归 WP4 | 列入 WP4 派发输入（2.11 必须含 node.auth_failed 写入用例） | RV1-WP4 核对 |
| RV1-WP3C-F5 | 13f0a16b | 同上 | migration.rs:3-7 模块注释 | P2 建议级：指代歧义 | 修复（wp3-fix1） | 已修复于 d127a802；RV1-WP3 复核：已解决 |
| RV1-WP3（总评） | d127a802 | reviewer 子 Agent ba4cb688（不继承实现对话） | reports/rv1-wp3.md | PASS，无 CRITICAL/MAJOR | — | 附带复核 RV2-WP2FIX-F1/F3 与 RV1-WP3C-F2/F3/F5 全部已解决；5 项 P2 见下行 |
| RV1-WP3-F1 | d127a802 | 同上 | spec R26/R28 文本张力 | P2：status 在 secret 已清除时的读法未写入规范 | 主 Agent 选 (a)：spec R26 补 secret 生命周期分支 + R28 收窄 + 新场景（Coverage R88）；NODE_LINK_PROTOCOL.md §13.3 澄清句列入 wp3-fix2 | 已闭环（规划层），文档句待 wp3-fix2 |
| RV1-WP3-F2 | d127a802 | 同上 | IDENTITY_AND_AUTH_CONTRACT.md:351 vs :352 | P2：身份合同内部矛盾（先读后验 vs 验后构造） | 修复（wp3-fix2，含 design.md D12 同步） | 已修复于 443c6c09；RV1-WP4 复核：已解决 |
| RV1-WP3-F3 | d127a802 | 同上 | 同文件 §5.1 has_secret 登记 | P2：has_secret 的生产路径用途未登记 | 修复（wp3-fix2） | 已修复于 443c6c09；RV1-WP4 复核：已解决 |
| RV1-WP3-F4 | d127a802 | 同上 | node_link/tests.rs:967-978 | P2：用例注释夸大且缺状态码断言 | 修复（wp3-fix2） | 已修复于 443c6c09；RV1-WP4 复核：已解决 |
| RV1-WP3-F5 | d127a802 | 同上 | node_link/pairing.rs:243-253 | P2 report-only：并发同内容 claim 的 409 窗口（自愈） | 修复（wp3-fix2 补注释） | 已修复于 443c6c09；RV1-WP4 复核：已解决 |
| RV1-WP4（总评） | 319c77b7 | reviewer 子 Agent 9c66fae4（不继承实现对话） | reports/rv1-wp4.md | FAIL | — | 2 条 P1 + 5 条 P2；附带复核 RV1-WP3-F2/F3/F4/F5 全部已解决；处置见下行 |
| RV1-WP4-F1 | 319c77b7 | 同上 | conn/registry.rs send/encode/enqueue | P1：出站序号在入队判定前分配，HighWater 烧掉序号 → 对端永久 sequence_invalid | 修复（wp4-fix1：准入与序号分配同一临界区 + 红向回归用例） | 修复后由 RV2-WP4 复核 |
| RV1-WP4-F2 | 319c77b7 | 同上 | session.rs/use_cases.rs/trust.rs | P1：R40 的 last_seen 收尾写集无生产路径 | 修复（wp4-fix1，选项①：认证收尾同事务推进 owned_node.last_connected_at；TrustStore 窄写路径 + CORE_PORTS §5 同步 + check:drift；授权 wp4-fix1 改 core/storage/文档，延续用户裁决 A 方向） | 修复后由 RV2-WP4 复核 |
| RV1-WP4-F3 | 319c77b7 | 同上 | session.rs:290 HandshakeSlots 生命周期 | P2：在途握手配额实为同源并发连接数限制 | 修复（wp4-fix1：握手完成即释放 + 用例钉死） | 已修复于 wp4-fix1（2917401c）；RV2-WP4 复核：已解决 |
| RV1-WP4-F4 | 319c77b7 | 同上 | conn/tests.rs 用例缺口 | P2：两条准入闸门无行为用例（RV1-WP2 N5 的要求） | 修复（wp4-fix1 补两条 loopback 用例） | 已修复于 wp4-fix1（2917401c）；RV2-WP4 复核：已解决 |
| RV1-WP4-F5 | 319c77b7 | 同上 | wire.rs:94-99 vs :227-236 | P2：未知 type 不记账序号，与 post_mvp 口径不一致 | 修复（wp4-fix1：信封可解析即记账，与 post_mvp 同口径 + 模块文档同步） | 已修复于 wp4-fix1（2917401c）；RV2-WP4 复核：已解决 |
| RV1-WP4-F6 | 319c77b7 | 同上 | session.rs 两处不可达分支 | P2：注释与实际拒绝路径不符 | 修复（wp4-fix1：删除或注释注明不可达依据） | 已修复于 wp4-fix1（2917401c）；RV2-WP4 复核：已解决 |
| RV1-WP4-F7 | 319c77b7 | 同上 | design.md D3 限流位置 | P2：design 与实现相反 | 主 Agent 已修 design.md D3（写明实际接线位置与效果等价理由） | 已闭环 |
| RV2-WP4-F1 | 59bf1058 | reviewer 子 Agent d9dfbb2a | CORE_PORTS §4 入口计数 | P2：「两条窄入口」与三条入口矛盾 | 主 Agent 当场修复（2937850f） | npm run check 16/16 绿；RV1-WP5 附带复核 |
| RV2-WP4-F2 | 59bf1058 | 同上 | IDENTITY_AND_AUTH_CONTRACT §5.1 | P2：收尾写集指针缺 record_node_connected | 主 Agent 当场修复（2937850f） | 已修复于 2937850f；RV1-WP5 附带复核：已解决 |
| RV2-WP4 残余风险登记 | 59bf1058 | 同上 | — | report-only：① FakeTrust 不给 record_node_connected 真实镜像（列入 WP6 观察项）；② revoked 行语义有意不断言；③ F3 后无同源已认证连接数上限（TLS 池 64 槽兜底，已裁定取舍） | 记录不改码 | — |
| RV1-WP5（总评） | 6b1f0b9c | reviewer 子 Agent b05e7d1b（不继承实现对话） | reports/rv1-wp5.md | PASS，无 CRITICAL/MAJOR | — | 6 条 P2；处置见下行 |
| RV1-WP5-F1 | 6b1f0b9c | 同上 | catalog.rs 路由零路由级用例 | P2：R51/R53 无路由级测试 | 修复（wp5-fix1：补 3 条路由级用例） | 修复后由 RV2-WP5 复核 |
| RV1-WP5-F2 | 6b1f0b9c | 同上 | resource.rs 扇出 | P2：R61 只有结构保证 | 记为部分覆盖；端到端断言（提交失败→连接收不到事件）列入 WP7（2.21/2.22）派发输入 | WP7 的 RV1 核对 |
| RV1-WP5-F3 | 6b1f0b9c | 同上 | resource.rs states 表不回收 | P2：状态表随时间增长 | 修复（wp5-fix1：subscribers() 顺带回收不在 registry 的 key） | 修复后由 RV2-WP5 复核 |
| RV1-WP5-F4 | 6b1f0b9c | 同上 | resource.rs ownerNodeId 未校验回显 | P2：复合身份 owner 分量可由对端任意指定 | 修复（wp5-fix1；主 Agent 裁决口径：attach 不等 → `export.not_found`、ack 不等 → `sequence_invalid`（与 §12.4 line 559 既有语义同族），无协议文档偏离） | 修复后由 RV2-WP5 复核 |
| RV1-WP5-F5 | 6b1f0b9c | 同上 | resource.rs 四处静默跳过 | P2：映射失败无日志 | 修复（wp5-fix1：补 warn!） | 已修复于 82e7c52c；RV2-WP5 复核：已解决 |
| RV1-WP5-F6 | 6b1f0b9c | 同上 | CORE_PORTS §4 计数 | P2 doc：「三条窄 seam」与四个用例入口不一致 | 修复（wp5-fix1：改写为「三条端口 seam + 四个用例入口」） | 已修复于 27ad3f80；RV2-WP5 复核：已解决 |
| RV1-WP5 残余风险登记 | 6b1f0b9c | 同上 | — | report-only：① 扇出队列满即丢（D6/G4 既定取舍，WP7 集成测试显式记录）；② resource.ack 无频率限制（合同无此条款，仅登记） | 记录不改码 | — |
| RV2-WP5-F1 | 27ad3f80 | reviewer 子 Agent 2f422f19 | resource.rs 回收点 | P2：attach-未订阅/re-attach 清订阅的连接条目仍无回收路径 | 修复（wp5-fix2：states 遍历中凡不在 registry.handles() 的 key 一律回收） | 已修复于 5e2d17c7（红向证据）；待 RV1-WP6 附带复核 |
| RV2-WP5-F2 | 27ad3f80 | 同上 | resource.rs:762 | SUGGESTION：未使用绑定 `let _ = attachment` | 修复（wp5-fix2：改 is_none() 提前返回） | 已修复于 5e2d17c7；待 RV1-WP6 附带复核 |
| wp5-fix2 残余登记 | 5e2d17c7 | coder 自报 | resource.rs subscribers() | report-only：handles() 快照取在 states 锁之前的理论误回收窗口——实际不可达，命中后果为可恢复的 attach_generation_stale；彻底关闭需快照入临界区（新锁嵌套顺序），超派单范围 | 主 Agent 裁决：记为已知残余，不改码（命中后果可恢复） | RV1-WP6 附带核对 |
| RV2-WP5 残余风险登记 | 27ad3f80 | 同上 | — | report-only：② ResourceRoute::new 第三参数必须传本机 node id（WP7 派发输入）；③ warn! 无自动化回归保护（v1 无日志捕获设施，登记）；⑤ knownRevision 非空分支缺路由级用例（列入 WP6 观察项） | 记录不改码 | — |
| RV1-WP6（总评） | 8bc2ff3e | reviewer 子 Agent fbf5360b（不继承实现对话） | reports/rv1-wp6.md | FAIL（1 条 P1 阻断 + 8 P2 + 3 SUGGESTION） | — | 附带复核 RV2-WP5-F1/F2 已解决；RV2-WP4 残余① 仍开放未被触及；逐项处置见下行 |
| RV1-WP6-F1 | 8bc2ff3e | 同上 | command.rs 进程内 session_creates + core create_session | P1：session.create 幂等只在进程内（重启双重创建 + 重查 not_found 非 uncertain） | 修复（wp6-fix1，选项 a：create_session 携带 client requestId 并写持久幂等行 + 终态；core/文档同步 + drift；修 MAX_TRACKED 注释/淘汰语义） | 已修复于 wp6-fix1（三提交）；RV2-WP6 复核：已解决 |
| RV1-WP6-F2 | 8bc2ff3e | 同上 | command.rs poll_pending | P2：无 pending 项的连接条目不回收 | 修复（wp6-fix1） | 已修复于 wp6-fix1（709ef55）；RV2-WP6 复核：已解决 |
| RV1-WP6-F3 | 8bc2ff3e | 同上 | terminal/accepted result | P2：completed 恒 {}、turnId 被丢 | 裁决：「非空」原意 = 非 null（spec R66/design D8 已写明）；wp6-fix1 透传 core 收据有意义字段 | 同上 |
| RV1-WP6-F4 | 8bc2ff3e | 同上 | command.status 重查形状 | P2：字面与唯一可行实现冲突 | 主 Agent 已同步 spec R66 + design D8（未终结→同形 accepted；查询重查→not_found） | 已闭环（规划层） |
| RV1-WP6-F5 | 8bc2ff3e | 同上 | export.revoked 推送面 | P2：只覆盖有 attachment/订阅的连接 | 主 Agent 已在 design D7 写明窄口径（catalog-only 连接靠实时投影与命令复核自愈） | 已闭环（规划层） |
| RV1-WP6-F6 | 8bc2ff3e | 同上 | 三个查询 unsupported 未入权威文档 | P2 | 修复（wp6-fix1：§12.7 注记） | 已修复于 wp6-fix1；RV2-WP6 复核：已解决 |
| RV1-WP6-F7 | 8bc2ff3e | 同上 | capability 支 | P2：交集表述与实现不一致 | 主 Agent 已在 design D8 写明（后端显式失败承担） | 已闭环（规划层） |
| RV1-WP6-F8 | 8bc2ff3e | 同上 | wire_error 未登记码 → retryable=true | P2：永久失败误标可重试 | 修复（wp6-fix1：已知永久失败映射非 retryable 的语义最近登记码） | 已修复于 wp6-fix1；RV2-WP6 复核：已解决 |
| RV1-WP6-F9 | 8bc2ff3e | 同上 | compose.rs RevocationCloser 只记日志 | P2：WP7 不替换则撤销只留日志 | 列入 WP7 硬门禁（2.21 派发输入 + RV1-WP7 必查） | RV1-WP7 核对 |
| RV1-WP6-F10 | 8bc2ff3e | 同上 | 撤销通知顺序用例 | SUGGESTION：顺序断言实际不可观察 | 修复（wp6-fix1：替身回读存储断言） | 已修复于 wp6-fix1；RV2-WP6 复核：已解决 |
| RV1-WP6-F11 | 8bc2ff3e | 同上 | command/tests.rs:1704 | SUGGESTION：死代码绑定 | 修复（wp6-fix1：删除） | 同上 |
| RV1-WP6-F12 | 8bc2ff3e | 同上 | dispatch 取消路径无用例 | SUGGESTION | 修复（wp6-fix1：补 Shutdown 退出用例） | 已修复于 f630859；RV2-WP6 复核：已解决 |
| RV2-WP6-F1 | f630859f | reviewer 子 Agent 1ccf40bc | command.rs:815-836 注释 + spec R66 | P2：「幂等行落盘前失败」窗口语义注释过宽（瞬态写失败同键重试可得不同结果） | 修复（wp6-fix2：注释收窄 + R66/§12.7 补一句该窗口语义） | 已修复于 1efd69ff；待 RV1-WP7 附带复核 |
| RV2-WP6-F2 | f630859f | 同上 | broker.rs settle_session_create | P2：不校验 record.command() == session.create（防御缺口，wire 不可达） | 修复（wp6-fix2：加一行 guard） | 已修复于 1efd69ff；待 RV1-WP7 附带复核 |
| RV2-WP6-F3 | f630859f | 同上 | version_conflict 的 retryable 翻转 | P2：RV1-WP6-F8 裁决的有意收窄 | 记录为已裁决取舍；切片 6 如需区分须增 registry 码（另行裁决） | 已闭环（记录） |
| RV2-WP6-F4 | f630859f | 同上 | app 无 dispatch spawn | P2：终态观察循环在真实 Daemon 未 spawn | 列入 WP7 硬门禁（与 RV1-WP6-F9 合并跟踪） | RV1-WP7 核对 |
| RV2-WP6 残余登记 | f630859f | 同上 | — | report-only：F3 的「重查不带 turnId」边界；view_command_completed 的 version→turnId 并存投影（交 Sync 切片裁定）；预持久化失败的 session.create 留无端点会话行（WP7/agent-host 复核） | 记录不改码 | — |
| RV2-WP6 残余登记 | f630859f | 同上 | — | report-only：F3 的「重查不带 turnId」边界；view_command_completed 的 version→turnId 并存投影（交 Sync 切片裁定）；预持久化失败的 session.create 留无端点会话行（已随 WP7 登记为已知残余，不修） | 记录不改码 | — |
| RV1-WP7（总评） | a3109c8a | reviewer 子 Agent d2dccebc（不继承实现对话） | reports/rv1-wp7.md | PASS，无 CRITICAL/MAJOR | — | 四条硬门禁全过；RV2-WP6-F1/F2 复核已解决；4 条非阻断发现见下行 |
| RV1-WP7-F1 | a3109c8a | 同上 | app/tests/node_link_listener.rs:311-334 | MINOR：unwired 收敛用例 vacuous（日志级别） | 修复（wp7-fix1：debug 级 + 阳性对照） | 修复后由 RV2-WP7 复核 |
| RV1-WP7-F2 | a3109c8a | 同上 | daemon.rs:1403-1417 | MINOR：网络 ingress 停 accept 的触发晚于本地排空、日志早于事实 | 修复（wp7-fix1：拆 trigger/wait + 日志移位 + 用例） | 同上 |
| RV1-WP7-F3 | a3109c8a | 同上 | core/broker.rs commit_chunk | MINOR（既有代码）：非终态批次写失败静默丢弃 → completed 但正文缺失，与 §6 第 9 条冲突 | **主 Agent 裁决选 (a) 修**：非终态批次 Unavailable 使在跑 turn 进失败/uncertain（不留 completed-无正文）；含可观测信号与新用例（含红向）；wp7-fix1 执行 | 同上 |
| RV1-WP7-F4 | a3109c8a | 同上 | app/tests/support/owner.rs:139-148 | SUGGESTION：测试复刻私有常量 | 修复（wp7-fix1：改用 identity.key()） | 已修复于 690b9172；RV2-WP7 复核：已解决 |
| RV2-WP7（总评） | 690b9172 | reviewer 子 Agent 085d1c99（不继承实现对话） | reports/rv2-wp7.md | FAIL（RV1 四项全闭环；1 条新 P1 + 2 条新 P2） | — | F3 主形态正确；RV2-WP7-F1 是 fix 引入的相邻可达路径 |
| RV2-WP7-F1 | 690b9172 | 同上 | core/broker.rs 归属兜底 + finish_turn + 立即派发 | P1：被放弃 turn 的迟到终态记到下一个 turn（误报 completed + 正文丢弃） | 修复（wp7-fix2 按建议 (a)：被放弃 turn 占住会话槽直到终态被观测 + 兜底 + core 回归用例） | 修复后由 RV3-WP7 复核 |
| RV2-WP7-F2 | 690b9172 | 同上 | owner.rs:275-278 注释 | P2：测试替身注释陈旧 | 修复（wp7-fix2） | 同上 |
| RV2-WP7-F3 | 690b9172 | 同上 | CORE_PORTS §6 第 19 条 | P2：与第 9 条新增规则对同一事件类给不同后果 | 修复（wp7-fix2：§19 交叉引用 + 第 9 条点明有意更强 + 用例） | 已修复于 5c869160；RV3-WP7 复核：已解决 |
| RV3-WP7（总评） | 5c869160 | reviewer 子 Agent f2ec54f4（不继承实现对话） | reports/rv3-wp7.md | PASS（P1 闭环，无新死锁面） | — | 3 条新 P2（文档/注释级）见下行 |
| RV3-WP7-F1 | 5c869160 | 同上 | CORE_PORTS §6 第 9 条兜底子项 | P2：无条件语气未点明「释放后迟到尾巴仍按在跑 turn 归属」的残余代价 | 修复（wp7-fix3：补显式代价句） | 修复后由候选 review（6.4）核对 |
| RV3-WP7-F2 | 5c869160 | 同上 | broker.rs 占位期 version_conflict/busy | P2：占位期客户端可见后果未登记 | 修复（wp7-fix3：§6 第 9 条补一句 + 可选 core 用例） | 同上 |
| RV3-WP7-F3 | 5c869160 | 同上 | broker.rs:255 常量 + pump 注释 | P2：常量 1200 无钉死断言；pump 注释缺占位口径 | 修复（wp7-fix3：用例补断言 + 注释补句） | 已修复于 f9dbece1；RV1-WP8 附带核对：生效 |
| RV1-WP8（总评） | 0161a778 | reviewer 子 Agent 8d784dc9（不继承实现对话） | reports/rv1-wp8.md | PASS，无 CRITICAL/MAJOR | — | 六份文档逐条对代码验证；2 条 MINOR + 2 条观察见下行 |
| RV1-WP8-F1 | 0161a778 | 同上 | CORE_PORTS §11:1403 | MINOR：缺「Owner 侧入站面」限定语 | 主 Agent 当场修复（a882c06f） | RV2/候选 review（6.4）核对 |
| RV1-WP8-F2 | 0161a778 | 同上 | CONFIG_REFERENCE.md:11 | MINOR：0.8 修订记录插在 0.7 之前 | 主 Agent 当场修复（a882c06f，移至 0.7 之后） | 同上 |
| RV1-WP8-O1 | 0161a778 | 同上 | MODULE_ARCHITECTURE:237「唯一例外」vs README:30 | report-only（既有）：量词不一致 | 主 Agent 当场修复（a882c06f，改为「两处按合同的例外」） | 同上 |
| RV1-WP8-O2 | 0161a778 | 同上 | plan.md WP8 写范围 | report-only：已批准的 CORE_PORTS 扩展未登记 | 主 Agent 已补登 plan.md WP8 行 | 已闭环 |
| RV1-WP7 观察项 | a3109c8a | 同上 | — | report-only：① `node_link.*` 键的「本切片未接线」注记 = WP8 必须完成项；② handoff 的 claim 路径笔误（/pair/claim→/pairing/claim，不影响证据） | WP8 收口时处理 ① | ① 已于 0161a778 落地（CONFIG_REFERENCE §3 注记） |
| 2.21 | coder / implement / work-package | 07b956c744bedbe6ab78047f75bc877a595f60fd | CHECK / PV4 | reports/wp7-handoff.md | PASS / NEW | 配置 unwired 收敛 + 启停序列 + status.listen + RevocationCloser 真实化 + dispatch spawn + ForkedPublisher；app 84 passed |
| 2.22 | coder / implement / work-package | a3109c8a80ec806cd7899b72709ec07e579cec28 | CHECK / PV5 | reports/wp7-handoff.md | PASS / NEW | 受控路径全链路（配对→确认→握手→catalog→create→attach→事件/ack→终态推送→撤销 4410）+ TLS direct 轮次 + R61 故障注入负向断言；日志 wp7-integration.log + pv5 WP7 轮次 |
| 2.21/2.22 | reviewer / review / work-package | a3109c8a80ec806cd7899b72709ec07e579cec28 | REVIEW / RV1-WP7 | reports/rv1-wp7.md | PASS / NEW | 独立检视（reviewer d2dccebc）；四条硬门禁全过；RV2-WP6-F1/F2 复核已解决；4 条非阻断发现见 Review Findings |
| 3.13 | coder / implement / work-package | a3109c8a80ec806cd7899b72709ec07e579cec28 | CHECK / PV4 | reports/wp7-verify.md | PASS / NEW | app 84 passed / 0 failed（含全链路 e2e 2/2 + listener 8/8 真实 listener/TLS） |
| 3.13 | coder / implement / work-package | a3109c8a80ec806cd7899b72709ec07e579cec28 | CHECK / PV1/PV2 | reports/wp7-verify.md | PASS / NEW | workspace 966 passed / 0 failed；15 子检查逐个全 0；boundaries 退 0 |
| 3.13 | coder / implement / work-package | a3109c8a80ec806cd7899b72709ec07e579cec28 | CHECK / PV5 | reports/wp7-verify.md | PASS / NEW | 本机 393 passed / 0 failed（declared==executed）；清理早前遗留 32 个临时目录 |
| RV1-WP7-F3 | coder / fix / work-package | ee5f63199a31d99fb04b5df45ce675fe00af151f | DELIVERY / RV1-WP7 | reports/wp7-fix1-handoff.md | PASS / NEW | 非终态批次写失败 → abandon_failed_turn（turn.failed + command.uncertain），不再 completed-无正文；红向证据（改前 Completed vs Uncertain）；注意：R61 断言随之变化，旧 PV5 证据 STALE，须重跑 |
| RV1-WP7-F1/F2/F4 | coder / fix / work-package | 690b91722f6ef1306660fde55ce1afd0d4607f4b | DELIVERY / RV1-WP7 | reports/wp7-fix1-handoff.md | PASS / NEW | vacuous 用例修复 + 停 accept 时机（trigger/wait 拆分 + 日志移位 + 用例）+ 测试常量复刻移除；红向证据齐 |
| 3.13（修复后重跑） | coder / implement / work-package | 690b91722f6ef1306660fde55ce1afd0d4607f4b | CHECK / PV4 | reports/wp7-fix-verify.md | PASS / NEW | app 85 passed / 0 failed（e2e 2/2 新 R61 断言、listener 8/8、daemon_lifecycle 11/11） |
| 3.13（修复后重跑） | coder / implement / work-package | 690b91722f6ef1306660fde55ce1afd0d4607f4b | CHECK / PV3 | reports/wp7-fix-verify.md | PASS / NEW | 五 crate 668 passed / 0 failed（core lib 115 = +2 broker 用例） |
| 3.13（修复后重跑） | coder / implement / work-package | 690b91722f6ef1306660fde55ce1afd0d4607f4b | CHECK / PV1/PV2/PV5 | reports/wp7-fix-verify.md | PASS / NEW | workspace 969 passed / 0 failed；boundaries 退 0；PV5 本机 394 passed / 0 failed + R61 约定形式（--test-threads=1）2/2 刷新 STALE 证据 |
| RV2-WP7-F1 | coder / fix / work-package | 008dcef5fedab4e2d82efce71f24053919f4cb50 | DELIVERY / RV2-WP7 | reports/wp7-fix2-handoff.md | PASS / NEW | 被放弃 turn 占住会话槽直到终态被观测（有界驱动轮次 + 终态观测释放兜底）；两条回归用例含 P1 原症状复现红向；§6 第 9/19 条同步 |
| RV2-WP7-F2/F3 | coder / fix / work-package | 5c869160e8dab0361b248124f631beba239183c6 | DELIVERY / RV2-WP7 | reports/wp7-fix2-handoff.md | PASS / NEW | 测试替身注释改写 + §19 交叉引用与对照用例 |
| 3.13（fix2 后重跑） | coder / implement / work-package | 5c869160e8dab0361b248124f631beba239183c6 | CHECK / PV1/PV2/PV3/PV4/PV5 | reports/wp7-fix2-verify.md | PASS / NEW | PV1 972/0/2、PV3 671/0/2（core 118=+3 对 fix2 用例）、PV4 85/0/0、PV5 394/0/0 + 约定形式 e2e 2/2；无资源残留 |
| RV3-WP7-F1/F2/F3 | coder / fix / work-package | f9dbece12fd9104bc297cf97655f965fbc1dd4e9 | DELIVERY / RV3-WP7 | reports/wp7-fix3-handoff.md | PASS / NEW | §6 第 9 条代价句 + 占位期客户端后果句 + 常量钉死断言 + pump 注释；core 118→119；coder 自报 wp7-integration.log 的并发写入交叉区域已重建（带标注）——主 Agent 已抽查日志尾部完好 |
| 2.23 | coder / implement / work-package | 0161a7782f12e29278aa311b1e1784b08d4ae8b9 | DELIVERY / NOT_APPLICABLE | reports/wp8-handoff.md | PASS / NEW | 六份权威文档状态写回 + CONFIG_REFERENCE §1/§3 注记 + CORE_PORTS 两处现状句（范围扩展已批准）；8 处同类过期句最小收敛（handoff 第 4 条逐条列出） |
| 2.24 | coder / implement / work-package | 0161a7782f12e29278aa311b1e1784b08d4ae8b9 | CHECK / PV1/PV2 | reports/wp8-handoff.md | PASS / NEW | 固定提交上 npm run verify + boundaries 两轮 exit 0；workspace 973 passed / 0 failed；core 闭包与 allow-list 一致；du1-pv1.log W7/WP8 轮次 |
| 3.16 | reviewer / review / work-package | 0161a7782f12e29278aa311b1e1784b08d4ae8b9 | REVIEW / RV1-WP8 | reports/rv1-wp8.md | PASS / NEW | 独立检视（reviewer 8d784dc9）；2 MINOR + 2 观察见 Review Findings；RV3-WP7-F1/F2/F3 附带核对生效 |
| RV1-WP8-F1/F2/O1 | main / fix / work-package | a882c06f71c8c4c2e4b509eb175d77d54e1cb921 | DELIVERY / RV1-WP8 | reports/rv1-wp8.md | PASS / NEW | 主 Agent 当场修复（限定语补齐、修订记录顺序、例外量词）；npm run check 绿；候选 review（6.4）核对 |
| 6.4 | reviewer / review / candidate | 92fb9fdc22938c0ad743d7dec7a1d0932b38cde5 | REVIEW / RV2-DU1 | reports/rv2-du1.md | PASS / NEW | 候选 merge 检视（reviewer b35b3fef）：无冲突/无遗漏/无夹带/门禁未弱化、主 Agent 修复与各 RV 待核对项在候选树生效；F1（工件 doc-links，已修复并复绿）/F3/F4 已处理，F2 按既定同步口径 |
| 6.6 | integrator / merge / main | 654c0c1944c75bbc016eab775f3f3ff8aca9bf35 | DELIVERY / NOT_APPLICABLE | reports/du1-integrate.md | PASS / NEW | ff-only 合入（premerge 门禁 PASS 后）；refs 未移动、is-ancestor 成立；未推送/未回滚 |
| 6.7 | integrator / merge / main | 654c0c1944c75bbc016eab775f3f3ff8aca9bf35 | CHECK / PV1/PV2/PV5 | reports/du1-integrate.md | PASS / NEW | 主分支检查：973 passed / 394 passed；flake 用例本轮 PASS 未复现 |
| 6.8 | main / merge / main | 654c0c19 | REVIEW / RV2-DU1（复用） | reports/rv2-du1.md | NOT_APPLICABLE / REUSED | ff 合入无新增差异，按计划由主 Agent 记录依据（原 review ID RV2-DU1 已核实候选树与源一致），不重复同范围审查 |
| 7.1 | coder / implement / final-main | 392efb791015b6a86a99ec9fd2ead45fe8c2881a | CHECK / PV1 | reports/final-alternative-checks.md | PASS / NEW | `npm run verify` 退 0；15 子检查逐个 0；workspace 973 passed / 0 failed / 2 ignored；du1-pv1.log「最终替代验证轮次」 |
| 7.1 | coder / implement / final-main | 392efb791015b6a86a99ec9fd2ead45fe8c2881a | CHECK / PV2/PV3/PV4 | reports/final-alternative-checks.md | PASS / NEW | boundaries 退 0；PV3 309/0/0；PV4 85/0/0 |
| 7.1 | coder / implement / final-main | 392efb791015b6a86a99ec9fd2ead45fe8c2881a | CHECK / PV5 | reports/final-alternative-checks.md | PASS / NEW | 本机 394 passed / 0 failed；约定形式 e2e 2/2；无资源残留 |
| WP6 coder 自报登记 | 8bc2ff3 | worker 1248254e | wp6-handoff.md「需要知晓的设计事实」 | report-only：① `session.read`/`session.mode.list`/`session.config.list` 本切片显式回 `command.unsupported`（结果形状属切片 6 正文路径；符合「不虚报」不变量）——主 Agent 接受并记录，RV1-WP6 核对；② `session.create` 幂等只在进程内（core 不写 owned_command）——**持久化幂等缺口**，RV1-WP6 重点评估是否需在切片内补；③ in-flight 越限复用 `rate_limited`；④ capability 维度在适配层不可判定（如实失败）；⑤ compose.rs 连带已列入检视 | 记录；②待 RV1-WP6 裁决 | — |

## Failures and Retests

> 已裁决（2026-09-26 用户原话「A」）：网络侧配对生命周期的 core 用例面空档已按方案 A 解决（D12 + 任务 2.25/2.26/2.27）；后续同类空档（WP4 握手只读/审计）经主 Agent 按同方向裁决（任务 2.28），不再重复请示。

| Issue ID / Task | Source / Check or E2E ID / Attempt / Version | Owner | Fix / Recovery | Review / Readiness Evidence | Retest Evidence | Current Status / Basis |
| --- | --- | --- | --- | --- | --- | --- |
| EX1 / 3.1 | worker 59457caf 自报的包装脚本问题（du1-pv1.log）：把导出的 `TMP` 指向文件导致 agent-host 11 个用例假失败 | 执行者自身 | 权威命令均在错误赋值之前执行、事后全清环境复跑 `npm run verify` 退 0；受控复现证明因果 | 不适用（非代码问题） | 全清环境复跑 PASS | 已解决（非仓库缺陷；原始证据保留在 du1-pv1.log） |
| EX2 / 2.25+2.26 | worker f1ef504d 运行失败：`edit` 的 oldText 在 admin_store.rs 未命中，运行终止于未提交中间态 | 主 Agent | 盘点确认工作区部分改动可编译（cargo check 通过）后原运行 resume（d15f62fc），指令要求先重读再编辑、门禁全绿才提交 | 不适用 | resume 完成：交付 13f0a16b，全部门禁绿 | 已解决 |
| EX3 / 2.28+2.10–2.12 | worker 4efd2fb3 运行 30 分钟超时终止（WP4 工作量超单次窗口） | 主 Agent | 盘点确认部分改动可编译后原运行 resume（b3576092），指令改为分批提交（2.28 独立提交→2.10/2.11 再提交）以防进度丢失 | 不适用 | resume 完成：交付 a811932 + 038349b，门禁绿 | 已解决 |
| EX4 / 2.13–2.16 | worker cc8a957a 运行 30 分钟超时终止（WP5 工作量超单次窗口；此前已有一次 steer 送达 G1 裁决） | 主 Agent | 盘点确认部分改动（G2/G5 seam，1086 行增量）可编译后原运行 resume（12b1deaf），指令改为分批提交（seam 一批、catalog/attach/event 各批） | 不适用 | resume 完成：交付 2dce835 + db46308 + 1264821 + 6b1f0b9，门禁绿 | 已解决 |
| EX5 / 2.17–2.20 | worker f0c2daea 运行 30 分钟超时终止（WP6 体量；此前已送达 Q1/Q2/Q3 与授权修正裁决） | 主 Agent | 盘点确认部分改动可编译后原运行 resume（1248254e），指令分批提交；compose.rs 的撤销缝连带改动列入 RV1-WP6 检视 | 不适用 | resume 完成：交付 3638ebd + bbf042e + 4cbdf8a + 8bc2ff3，门禁绿 | 已解决 |
| EX6 / 2.21–2.22 | worker 8c8eca4e 运行 30 分钟超时终止（WP7 体量） | 主 Agent | 盘点确认部分改动（app 接线 + 集成测试文件）可编译后原运行 resume（668dbd32），指令分批提交（接线一批、集成测试一批） | 不适用 | resume 完成：交付 07b956c + a3109c8，门禁绿；恢复轮修复了一个测试客户端自身的空转 bug（非产品问题） | 已解决 |
| EX7 / 6.2–6.3 | 集成候选首轮 PV1 FAIL：rv1-wp8.md 第 36 行「§7.2」缺文档名被 check:docs 误判归属（该报告在 f02d2565 提交进仓库后才被门禁扫到，0161a778 的绿证据早于它） | 主 Agent | 源分支勘误（4c5a3f00，补「SECURITY_DESIGN」文档名 + 勘误注记），主仓库 check-doc-links 复绿；集成方重建候选 | 不适用 | 待候选重跑 | 处理中 |
| EX8 / 6.3 | 候选首轮 PV5 的 `the_status_result_keeps_the_documented_field_set` 间歇失败（约 1/6，负载相关；隔离跑与整轮均 PASS；5c869160→f02d256 间无 app/server 代码改动） | 主 Agent | 裁决：登记为 flake、以 PASS 轮次为准；若候选/主分支轮次再出现同类失败则升级为缺陷立案 | 不适用 | 候选重跑与主分支轮次观察 | 已登记（观察中） |

## Merge History

```agentic-premerge
version: 1
delivery_unit: DU1
target_ref: refs/heads/main
target_commit: 94a64e1f15d26f54a6601985fd13b1440fec8170
candidate_commit: 654c0c1944c75bbc016eab775f3f3ff8aca9bf35
contract_digest: "sha256:2e3b3a0bdc82d5d1ffaeb125d35c9550e2e437296c777f298a2efd4aabb82c2e"
verify:
  result: PASS
  candidate_commit: 654c0c1944c75bbc016eab775f3f3ff8aca9bf35
  evidence: {path: reports/du1-candidate.log, sha256: "sha256:199c894596be1e44548ec03827d81d11202bb90b67f7135a4bbeda85b9b3103b"}
review:
  result: PASS
  candidate_commit: 654c0c1944c75bbc016eab775f3f3ff8aca9bf35
  reviewer: "RV2-DU1（reviewer 子 Agent b35b3fef，不继承实现/集成对话）"
  author: "各 WP coder 子 Agent（worker）"
  evidence: {path: reports/rv2-du1.md, sha256: "sha256:0c3a3943837075465113763e5af05fcea04adc6dc48a9b79d71cd3138681b074"}
alternative_checks:
  - name: "cargo test --locked -p server --all-features [PV3]：listener 绑定/路由/Host 边界/TLS 失败关闭、配对 HTTP 全状态码与幂等、握手与信封/序号规则、catalog 过滤、attach/generation、snapshot/replay、event 持久化顺序与 raw 保真、ack 单调性、命令授权/幂等/终态/session.create 约束、撤销传播、心跳与慢连接、审计边界，schema/fixture 漂移测试消费同一 manifest。"
    result: PASS
    candidate_commit: 654c0c1944c75bbc016eab775f3f3ff8aca9bf35
    evidence: {path: reports/du1-candidate.log, sha256: "sha256:199c894596be1e44548ec03827d81d11202bb90b67f7135a4bbeda85b9b3103b"}
  - name: "cargo test --locked -p app --all-features [PV4]：配置 unwired 收敛、启动/关闭序列含网络 listener、daemon.status.listen 实际地址。"
    result: PASS
    candidate_commit: 654c0c1944c75bbc016eab775f3f3ff8aca9bf35
    evidence: {path: reports/du1-candidate.log, sha256: "sha256:199c894596be1e44548ec03827d81d11202bb90b67f7135a4bbeda85b9b3103b"}
  - name: "受控路径全链路集成测试 [PV5]（crates/server 或 crates/app 的集成用例，本机 Windows 执行并留证）：脚本化 fake Access 客户端经真实 loopback listener 完成 配对 claim → 本地确认 → 握手 → catalog.snapshot → attach → snapshot/event/ack → command（含 session.create 正常与各拒绝路径）→ 撤销传播；TLS direct 用自签证书跑通同一握手。"
    result: PASS
    candidate_commit: 654c0c1944c75bbc016eab775f3f3ff8aca9bf35
    evidence: {path: reports/du1-candidate.log, sha256: "sha256:199c894596be1e44548ec03827d81d11202bb90b67f7135a4bbeda85b9b3103b"}
  - name: "npm run check / npm run verify [PV1]/[PV2]：合同门禁与全 workspace 测试，确认新增依赖与矩阵格不破坏既有 crate。"
    result: PASS
    candidate_commit: 654c0c1944c75bbc016eab775f3f3ff8aca9bf35
    evidence: {path: reports/du1-candidate.log, sha256: "sha256:199c894596be1e44548ec03827d81d11202bb90b67f7135a4bbeda85b9b3103b"}
```

证据摘要与版本化报告：verify = `reports/du1-candidate.log`（sha256:199c8945…3103b）；review = `reports/rv2-du1.md`（sha256:0c3a3943…1b074）；替代检查逐项报告：`reports/wp6-fix-verify.md`（PV3 口径，sha256:666fbdcb…89ab）、`reports/wp7-fix2-verify.md`（PV4/PV5 约定形式，sha256:37774320…a086）、`reports/pv5-windows-nodelink.log`（PV5 全链路，sha256:3af1a3d2…85b9）。

- DU1 候选（attempt 2，权威）：base `94a64e1f` + 源 `4c5a3f00` → 候选 `92fb9fdc22938c0ad743d7dec7a1d0932b38cde5`（`merge --no-ff`，无冲突，候选树与源逐字节一致）。候选检查：PV1/PV2/PV5 全 exit 0（973 passed / 394 passed；证据 `reports/du1-candidate.log` + `reports/du1-integrate.md`）。attempt 1（`8d87df18`，源 `f02d2565`）因 check:docs（rv1-wp8.md 的 §7.2 归属）被取代，已按裁决修源并重建。
- DU1 候选（attempt 3，当前）：源 `5f62e77f`（仅 openspec/ 规划工件同步，零代码变化）→ 候选 `654c0c1944c75bbc016eab775f3f3ff8aca9bf35`；PV1/PV2 在新候选重跑 exit 0（含 check:docs 对工件内容的扫描），PV5 按「零代码变化 + openspec/ 不在测试面」REUSED（依据见 `reports/du1-integrate.md` §0）。
- 集成执行者：worker 子 Agent c51411bd（独立集成 worktree `D:\Project\acp-remote-wt\nl-owner-integration`）。
- DU1 合入（6.6，执行者 8fd1ca8f）：前置障碍处理（DEVELOPMENT_PLAN.md 行尾噪声恢复 + 未跟踪规划目录移至备份）→ `git merge --ff-only 654c0c19` → main = `654c0c19`（与候选同一提交对象）；防竞态核实（refs 未移动、is-ancestor 成立）。主分支检查（6.7 前半，均为 NEW）：PV1/PV2/PV5 全绿（973 passed / 394 passed，flake 用例本轮 PASS 未复现）。记录见 `reports/du1-integrate.md` §10。
- 合入后规划记录回写（主 Agent）：备份目录的最新 `verification.md`/`tasks.md` 覆盖回 main 并提交 `392efb791015b6a86a99ec9fd2ead45fe8c2881a`（纯 openspec/ 工件）；`du1-integrate.md` 保留跟踪版本（较新，含 round 3 + §10）；15 个被 gitignore 的 .log 原始证据已从备份恢复到主检出工作区（不入库为既定惯例）。
- 6.8（主分支差异检视）：合入为 ff 到候选，无新增差异——由主 Agent 记录依据（原 review = RV2-DU1，候选树与源逐字节一致已经其核实），不重复同范围审查。

## Test Design and Authoring

不适用：Main E2E mode = `not-applicable`，不生成 TP（plan.md「Work Packages」说明）。

## Candidate E2E

不适用（mode = not-applicable）；候选替代验证（PV5 受控路径全链路）记录在 Checks 表。

## Main E2E

- 项目开关：`x-agentic.e2e.enabled = true`、`command = ""`、`maxAttempts = 3`（`openspec/config.yaml`）。
- mode：`not-applicable`；reason / basis / alternative_checks / downgrade_approval 见 plan.md「Main E2E」节（降级批准：2026-09-26 本会话用户原话「1B 2A 3批准」第 3 项）。
- E2E：NOT_APPLICABLE；替代验证为 plan.md `alternative_checks` 四项，逐项在 Checks 表留证。


## Final Assessment

（待最终验收时填写唯一 agentic-assessment 块与轮次记录。）
