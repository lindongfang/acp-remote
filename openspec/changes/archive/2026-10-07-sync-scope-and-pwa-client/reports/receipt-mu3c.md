<!-- MU3c 的 premerge receipt。premerge 门禁 PASS 之后持久化。 -->

```agentic-premerge
version: 1
target_ref: refs/heads/main
target_commit: 263d3ba8b48ae5dae8ab87b16bc4c0ea86f62314
candidate_commit: 4ada2a24e23ff3f07d37eb0face70921a66ed987
delivery_unit: MU3c
contract_digest: sha256:ecfc245c119d41b1b76a7bfdde9b11f93e751628a894f4b56f1fc2dba90e0e91
requirements_digest: sha256:c2eb41a848e4e9bd24f4e03550709fe3a44c560574fe45c4bbf040fb34cf2752
verify:
  result: PASS
  candidate_commit: 4ada2a24e23ff3f07d37eb0face70921a66ed987
  executor: coder-w6-r4（作者） / merger-mu3c（合入轮复跑）
  command: npx tsc --noEmit + npx vitest run（17 文件 / 269 passed）+ node scripts/run-browser-check.mjs（真实 Chromium 16/16）+ npx expo export --platform web ＋ npm run check（根十道，cwd=.worktrees/wp6）
  evidence:
    path: reports/PV3-wp6-r4.log
    sha256: sha256:961fd954f41be0e75faf058adae4c916d38cfb0410bdb7364326eed893f3f37a
review:
  result: PASS
  candidate_commit: 4ada2a24e23ff3f07d37eb0face70921a66ed987
  reviewer: ReviewerWp6R4
  author: coder-w6-r4
  evidence:
    path: reports/review-wp6-r4.md
    sha256: sha256:da9cbf406a6b459586f4e55feef388c776d8c41de9d2798c139be117a0c359e3
notes: "MU3c = WP6（Order 6，独立单元），经 4 轮实现 + 4 轮独立检视收敛。交付 clients/app/src/sync-client/ 与 clients/app/src/state/：逐连接签名握手、subscribe/ACK/增量重放、eventId 去重、快照暂存与原子替换、session.read (createdAt,messageId) 复合游标分页、稳定 requestId 幂等重试，以及 12 态连接互斥状态机与 7 值命令状态机。**根因收敛**：r2 的无界重订阅循环（303 subscribe/302 gap/0 事件）与 r3 的静默丢失（可见事件 5/6/7 全丢、ackedCursor 推到 8 并宣告 online、事后重放判 duplicate 永久不可恢复）同源于 evaluate 用 last+1 连续性判 gap，与 SYNC_PROTOCOL.md:658『全局 sequence 对单个设备可以有不可见的空洞，不能据此推断隐藏事件』矛盾；第 4 轮结构性删除该假设，改以服务端屏障（caught_up/snapshot_end）与『水位之下又送来一条从未呈现过的事件』（late）为判据，reviewer 逐条对照 §9.2/§9.3/§9.5 判定与协议一致。reviewer 自建 Server93（不复用作者替身）22 探针用例独立复现六条断言全成立，R14 两条要求各自构造可证伪场景。既有测试改写逐条对照后确认无一条实质性变弱。**用户 2026-10-05 裁决带 2 项 P1 合入**（reviewer 原文『FAIL（可带 2 项 P1 修复合入）』）：P1-N1 快照屏障被账本拒绝后门面 #ackedCursor 仍被覆盖、P1-N2 该越界游标使下次连接构造期抛错导致同步永久卡死；两条构成同一失效链，归属 WP7 接线轮顺带修复，合入报告与最终验收须复述。deps/advisories/secrets 三个 CI-only job 本地无等价物，未执行亦未声称通过。"
```
