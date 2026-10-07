<!-- MU2 的 premerge receipt。premerge 门禁 PASS 之后持久化；final/archive 阶段据此核对本单元的历史合入是否仍成立。 -->

```agentic-premerge
version: 1
target_ref: refs/heads/main
target_commit: 33040d78324ff51be49219fcb3aac054d0100cc9
candidate_commit: 5ba44fd35be2cd3717006c0995a8404c3315f388
delivery_unit: MU2
contract_digest: sha256:ecfc245c119d41b1b76a7bfdde9b11f93e751628a894f4b56f1fc2dba90e0e91
requirements_digest: sha256:c2eb41a848e4e9bd24f4e03550709fe3a44c560574fe45c4bbf040fb34cf2752
planning_digest: plan-v2:sha256:e5b00e62e26459d6dcd522d9f07ffb2bf38607b4daafa96c44b3cee92ad21a66
verify:
  result: PASS
  candidate_commit: 5ba44fd35be2cd3717006c0995a8404c3315f388
  executor: main（候选复跑） / merger-mu2（合入轮复跑）
  command: npm run verify（cwd=.worktrees/integ，CARGO_TARGET_DIR=.target-wt/mu2int）＋ cargo test --locked -p core -p storage-sqlite -p agent-host --all-features
  evidence:
    path: reports/PV1.log
    sha256: sha256:8b218a4a33a76b07d654a76f1cc8b05c7e4951adb6bbd30b883681c5dc0c2abb
review:
  result: PASS
  candidate_commit: 5ba44fd35be2cd3717006c0995a8404c3315f388
  reviewer: ReviewerMu2Candidate
  author: merger-mu2
  evidence:
    path: reports/review-mu2-candidate-r1.md
    sha256: sha256:6a55ead436c4d799437a2538cece2aeb2381da18287791de0b26faed263885e8
notes: "MU2 = WP3 + WP4 + TP2（Order 3）。候选 5ba44fd 自 33040d7 起 26 files / +6788 / -44，可快进。内容：WP3 core 派生 file.changed（行级 Myers diff 自实现、无新依赖）、节点级 Agent 事件落库、会话标题两层可选与权威 updated_at（R5–R9）；WP4 agent-host 的 Diff 逐字节透传与 profile 进程生命周期上报（R8/R21/R22）；组合根接线 crates/app/src/compose.rs（绑定 NodeEvents → Broker::commit_node_event，用 Weak 断开强引用环）；AC1 四项断言补齐（六 passed）；TP2 31 条 core 生产者行为测试。冻结合同 schemas/、crates/sync-protocol/、fixtures/、vendor/ 与两个 Cargo.toml 均零改动。检视过程中修复了两处真实缺陷：R7 前导 .. 越过根导致 displayPath 泄露工作区外目录结构（MAJOR，1550909 修复）；R9 updated_at 取事件自报时间而非 Daemon 提交时钟（违反 design D7、破坏 SYNC_PROTOCOL §10.2 排序）。deps/advisories/secrets 三个 CI-only job 本地无等价物，未执行亦未声称通过。"
```