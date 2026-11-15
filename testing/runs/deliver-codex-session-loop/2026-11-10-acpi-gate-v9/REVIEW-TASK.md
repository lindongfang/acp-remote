# Independent review task

Repository: D:/Project/acp-remote.

This is ACPI v9. Read AGENTS.md, openspec/config.yaml,
openspec/role/reviewer.md, the active proposal/plan/complete tests/design,
relevant specs and source, plus this package's CONTRACT.md, DESIGN-DELTA.md,
spec-candidate.md, tests-candidate.md, TESTER-REVIEW-v8.md, TESTER-REVIEW.md,
REVIEW-v6.md, TESTER-REVIEW-EVIDENCE.md, historical reference reports,
SOURCE.json and MANIFEST.json. Inspect current source to verify the proposed
atomic service bundle, typed authentication error propagation, module
visibility, socket ownership, writer, shutdown and all ten test boundaries.

Produce an independent coding-gate review in this directory as REVIEW.md,
with examined hashes, concrete findings and explicit pass/block scope. Check
every ACPI-V6-001—005 finding, the five v7 tester gaps, and v8 tester findings
ACPI-V8-001—003 against the actual v9 candidate and state open/closed for
each, without impersonating any historical role. Verify that the new
connection-bound one-shot `UploadRouteFaults` seam has an exact injection
point and no normal-build behavior, and that the R01 and control-only paths
are executable. Do not edit product code, active specs or acceptance
definitions. Do not run builds or tests. Historical reports are inputs, not
authority to declare the new candidate passed.

The user authorized continued implementation after prerequisites and independent review. This task concerns only the Rust authenticated WSS upload route coding gate. Web UI, final independent acceptance and archive remain outstanding.
