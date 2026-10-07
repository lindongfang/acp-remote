<!-- MU3b 的 premerge receipt。premerge 门禁 PASS 之后持久化。 -->

```agentic-premerge
version: 1
target_ref: refs/heads/main
target_commit: a4440d683dc44ecb69d2ffeb596fbff53170272c
candidate_commit: 263d3ba8b48ae5dae8ab87b16bc4c0ea86f62314
delivery_unit: MU3b
contract_digest: sha256:ecfc245c119d41b1b76a7bfdde9b11f93e751628a894f4b56f1fc2dba90e0e91
requirements_digest: sha256:c2eb41a848e4e9bd24f4e03550709fe3a44c560574fe45c4bbf040fb34cf2752
verify:
  result: PASS
  candidate_commit: 263d3ba8b48ae5dae8ab87b16bc4c0ea86f62314
  executor: coder-w5b-r2（作者） / merger-mu3b（合入轮复跑）
  command: npx tsc --noEmit + npx vitest run + node scripts/run-browser-check.mjs（真实 Chromium，cwd=clients/app）＋ npm run check（根十道，cwd=.worktrees/wp5b）
  evidence:
    path: reports/PV3-wp5b-r2.log
    sha256: sha256:b33bdf61e256ec5afddf50ab4f06ee50c1597cea2cbc65184e1f0197ed3fdb98
review:
  result: PASS
  candidate_commit: 263d3ba8b48ae5dae8ab87b16bc4c0ea86f62314
  reviewer: ReviewerWp5bRecheck
  author: coder-w5b-r2
  evidence:
    path: reports/review-wp5b-r2.md
    sha256: sha256:71f3e483baece4232a9589072ee72cf2c5eaac47615ee6fe8b51581aa2a3ef6b
notes: "MU3b = WP5b（Order 5，独立单元）。建立 clients/app/src/platform/：R11 不可导出设备身份（extractable:false 生成、每条私钥读取路径重新断言、私钥仅用于 subtle.sign、无可导出降级、能力丢失 fail closed）、按 transcript 规范的编码（与仓库根 fixtures/sync/v1/transcripts/device-proof.json 及 compatibility/transcripts/v1/transcripts.json 逐字节一致，不复制向量）、R20 的 local-cache（imported 正文不落盘、8 MiB/30 天/LRU）与生命周期端口。两轮检视均 PASS：第一轮留下 7 项非阻断（含两项用本包 fake IndexedDB 实际复现的并发写竞态与 ensureIdentity 双密钥），第二轮 263d3ba 全部修复——UTF-8 字节长度守卫、写路径串行化、ensureIdentity single-flight、R20 落盘闸门接线并加强为运行期键名允许清单、真实 Chromium runner、移除 beforeunload、清除死常量。复核方以仓库外黑盒打包 + 旧实现并排差分核实：140 例零差异、12/12 冻结向量逐字节一致、6 类并发交错全收敛无死锁泄漏、single-flight 三条失败路径经 finally 必清理。deps/advisories/secrets 三个 CI-only job 本地无等价物，未执行亦未声称通过。"
```