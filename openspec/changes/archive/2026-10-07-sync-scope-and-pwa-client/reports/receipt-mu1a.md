<!-- MU1a 的 premerge receipt。premerge 门禁 PASS 之后持久化；final/archive 阶段据此核对本单元的历史合入是否仍成立。 -->

```agentic-premerge
version: 1
target_ref: refs/heads/main
target_commit: b964ae313abb8b83fc9acb02fbc062448c5c23bc
candidate_commit: ad9ad3c6b532ce0cc9366c8cc34a217f9f466933
delivery_unit: MU1a
contract_digest: sha256:e4cc671c11eeac9f64cf259271ca7821a896a9a25fcbbbda4d4d16fb7e896aa5
requirements_digest: sha256:c2eb41a848e4e9bd24f4e03550709fe3a44c560574fe45c4bbf040fb34cf2752
planning_digest: plan-v2:sha256:4e2fb347b974d0e8b6d347afec7cbd50591524f79ae7b33cf0d98ced853c88e1
verify:
  result: PASS
  candidate_commit: ad9ad3c6b532ce0cc9366c8cc34a217f9f466933
  executor: MergerMu1a
  command: npm run verify（cwd=候选合入 worktree，CARGO_TARGET_DIR=.target-wt/mu1a）
  evidence:
    path: reports/PV1.log
    sha256: sha256:fa308ab101ca3b745cf3a085baf0d6ae173d659c9b200089cae25987adf0de01
review:
  result: PASS
  candidate_commit: ad9ad3c6b532ce0cc9366c8cc34a217f9f466933
  reviewer: ReviewerMu1aCandidate
  author: MergerMu1a
  evidence:
    path: reports/review-mu1a-candidate-r1.md
    sha256: sha256:451d4c9a7153cd3dd587a8de3cc2b7965f72671816a9463474560a2462aeefce
notes: "MU1a = WP1 + WP2（Order 1）。候选由两次 git merge --no-ff 组装，29 paths / +746 / -222；共享写点无真冲突，git merge-tree 复算树与候选树一致。候选 PV1 全绿，含 tasks 6.3 要求实测的 check:assets 计数门禁（73/14 自洽）。候选独立 review 为 merge 类型，两条 MAJOR 均不阻断候选：schema_drift 枚举等值断言缺口归 TP1/MU1b；主检出中断合并态已在恢复中清除。deps/advisories/secrets 三个 CI-only job 本地无等价物，未执行亦未声称通过。"
```