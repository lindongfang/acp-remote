# U1 premerge receipt（版本化）

> 本文件是 tasks 6.6 要求的**版本化 receipt**：由 `verification.md` 的 `## Premerge` 块在 premerge 门禁 PASS 后原样固化。生成时间 2026-10-01；绑定候选 `2ed142dedec2facf8f6e174d1aa165f549cbc47e`、目标 `81e350ff340014265eb7c9251237c799d4357fee`。
> 门禁复跑结果：`workflow check --stage premerge` = **PASS**（0 errors）。

```agentic-premerge
version: 1
delivery_unit: U1
target_ref: refs/heads/main
target_commit: 81e350ff340014265eb7c9251237c799d4357fee
candidate_commit: 2ed142dedec2facf8f6e174d1aa165f549cbc47e
contract_digest: sha256:fed00eb68cb7ef3cfab595a49374a6bb434807d770151c9e8b2d19bea7bb96ba
requirements_digest: sha256:5394327093e57d05b49ea4b04db708fcb97160791ea6cca8cec5a833154408fa
verify:
  result: PASS
  candidate_commit: 2ed142dedec2facf8f6e174d1aa165f549cbc47e
  evidence: {path: reports/merge-u1-candidate-PV1-stage2-workspace-test.log, sha256: "sha256:a93f928e74d1e517b3185d22492d6f8b67ea65f0fd0dbe5931ee73ff99761f0a"}
review:
  result: PASS
  candidate_commit: 2ed142dedec2facf8f6e174d1aa165f549cbc47e
  reviewer: reviewer-C1（独立隔离子 Agent run f494a2c4-800c-462f-9ad3-92a436a80162，CR-C1 Round 1，0×CRITICAL/0×MAJOR）
  author: WP1–WP6 = coder-A/B/C/D/E/E2/E3/F、TP2 = tester-A/A2/A3（合并执行者 merger-A2/A3/A4/A5）
  evidence: {path: reports/cr-c1-candidate-review.md, sha256: "sha256:04e99e078b48b95e95193f4c93c9cf3404df72d05cce34f931e8cfc3b3b9969b"}
alternative_checks:
  - name: "C1：cargo test --locked --workspace --all-features（含 fake ACP Agent 驱动的恢复路径；1074 passed / 0 failed / 2 ignored）"
    result: PASS
    candidate_commit: 2ed142dedec2facf8f6e174d1aa165f549cbc47e
    evidence: {path: reports/merge-u1-candidate-PV1-stage2-workspace-test.log, sha256: "sha256:a93f928e74d1e517b3185d22492d6f8b67ea65f0fd0dbe5931ee73ff99761f0a"}
  - name: "C2：npm run check（合同漂移、依赖方向、封闭词表与 ACP 固定向量；10 道门禁全绿）"
    result: PASS
    candidate_commit: 2ed142dedec2facf8f6e174d1aa165f549cbc47e
    evidence: {path: reports/merge-u1-candidate-PV2.log, sha256: "sha256:ea718e0d81b9d09dca30081cdfcb5fd9732e2a05f6bd6faf900d70dad0648f4c"}
```
