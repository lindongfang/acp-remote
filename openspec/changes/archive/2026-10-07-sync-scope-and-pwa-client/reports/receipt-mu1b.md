<!-- MU1b 的 premerge receipt（TP1）。premerge 门禁 PASS 之后持久化；final/archive 阶段据此核对本单元的历史合入是否仍成立。 -->

```agentic-premerge
version: 1
target_ref: refs/heads/main
target_commit: ad9ad3c6b532ce0cc9366c8cc34a217f9f466933
candidate_commit: 33040d78324ff51be49219fcb3aac054d0100cc9
delivery_unit: MU1b
contract_digest: sha256:e4cc671c11eeac9f64cf259271ca7821a896a9a25fcbbbda4d4d16fb7e896aa5
requirements_digest: sha256:c2eb41a848e4e9bd24f4e03550709fe3a44c560574fe45c4bbf040fb34cf2752
planning_digest: plan-v2:sha256:4e2fb347b974d0e8b6d347afec7cbd50591524f79ae7b33cf0d98ced853c88e1
verify:
  result: PASS
  candidate_commit: 33040d78324ff51be49219fcb3aac054d0100cc9
  executor: main（复跑） / testing-1-r1（作者）
  command: npm run verify（cwd=.worktrees/tp1，CARGO_TARGET_DIR=.target-wt/tp1）
  evidence:
    path: reports/PV1-tp1.log
    sha256: sha256:468ecae4349b1603450a22caec6aba1c3caded7d809a048dec1713b1bdd80c54
review:
  result: PASS
  candidate_commit: 33040d78324ff51be49219fcb3aac054d0100cc9
  reviewer: ReviewerTp1
  author: testing-1-r1
  evidence:
    path: reports/review-tp1-r1.md
    sha256: sha256:35d4eb2dc61bb57bf8fa976d7294c96b69f03863592db53b530541dcb8b6da90
notes: "MU1b = TP1（Order 2）。TP1 从 MU1a 的合入提交 ad9ad3c 重新分支（原 worktree 为修复前的过期镜像，已按 tasks 6.9 重建），未改任何产品实现（git diff ad9ad3c..33040d7 -- crates/sync-protocol/src/ 为空）。交付 23 个新 fixture + manifest 只增不改 + 两个新 suite（25 断言），并把 schema_drift.rs 的枚举门禁改写为 VIEW_ENUMS 驱动，从而关闭 review-wp2-r1 的 F3 与 review-mu1a-candidate-r1 的 MU2A-R1-F1。共享写点 envelope_fixtures.rs 的计数由 WP1 的 73/14 复算为 89/21 并新增 EXPECTED_TARGETED_PAYLOADS=9。独立检视 test-case 类型判据表逐项通过，无 CRITICAL/MAJOR。deps/advisories/secrets 三个 CI-only job 本地无等价物，未执行亦未声称通过。"
```