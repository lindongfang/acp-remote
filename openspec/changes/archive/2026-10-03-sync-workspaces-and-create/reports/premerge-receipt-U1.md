```agentic-premerge
version: 1
delivery_unit: U1（integrated）
target_ref: refs/heads/main
target_commit: 6c093f145aa3d69dcd573a6d94e31b692acb5d4b
candidate_commit: a6f6210bcb8c1aacd81a09bf5169cb5eb5e287c7
contract_digest: sha256:08669be58a99bae6bdcae2c415fc4f4324580caef22f0611ad14314de627b5ba
requirements_digest: sha256:3b6c490b45bce5f7df4419a70a4b653da27f9e18fd31e7f811dde336057a6411
verify:
  result: PASS
  candidate_commit: a6f6210bcb8c1aacd81a09bf5169cb5eb5e287c7
  evidence: {path: reports/candidate-verify-round2.log, sha256: "sha256:c328abf04e77e8e2f227f44761bffbbfc9b789920841083a3e359b3f506dac05"}
review:
  result: PASS
  candidate_commit: a6f6210bcb8c1aacd81a09bf5169cb5eb5e287c7
  reviewer: reviewer-candidate
  author: merger-1-r2
  evidence: {path: reports/candidate-review.md, sha256: "sha256:43bb3eefb94e4064ac1c61e131746a37f2089f195e4f72a50d0be00e8bb19045"}
alternative_checks:
- name: ALT1
  result: PASS
  candidate_commit: a6f6210bcb8c1aacd81a09bf5169cb5eb5e287c7
  evidence: {path: reports/candidate-verify-round2.log, sha256: "sha256:c328abf04e77e8e2f227f44761bffbbfc9b789920841083a3e359b3f506dac05"}
- name: ALT2
  result: PASS
  candidate_commit: a6f6210bcb8c1aacd81a09bf5169cb5eb5e287c7
  evidence: {path: reports/candidate-verify-round2.log, sha256: "sha256:c328abf04e77e8e2f227f44761bffbbfc9b789920841083a3e359b3f506dac05"}
```