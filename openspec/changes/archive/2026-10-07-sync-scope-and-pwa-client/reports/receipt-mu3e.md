<!-- MU3e 的 premerge receipt。premerge 门禁 PASS 之后持久化。 -->

```agentic-premerge
version: 1
target_ref: refs/heads/main
target_commit: 1e50bdeacccfa257901c88a7277e9aae818da395
candidate_commit: aed96a270441e9e9271a4c1ea9cd1ebcaadf5f9c
delivery_unit: MU3e
contract_digest: sha256:ecfc245c119d41b1b76a7bfdde9b11f93e751628a894f4b56f1fc2dba90e0e91
requirements_digest: sha256:c2eb41a848e4e9bd24f4e03550709fe3a44c560574fe45c4bbf040fb34cf2752
verify:
  result: PASS
  candidate_commit: aed96a270441e9e9271a4c1ea9cd1ebcaadf5f9c
  executor: testing-3-r2 / testing-4-r2（作者） / merger-mu3e（合入轮复跑）
  command: npx tsc --noEmit + npx vitest run（37 文件 / 458 passed）+ node scripts/run-browser-check.mjs（真实 Chromium 16/16）+ node e2e/run-e2e.mjs（21/21）＋ npm run check（根十道，cwd=.worktrees/mu3e-merge）
  evidence:
    path: reports/PV3-mu3e.log
    sha256: sha256:223f1748ef12bcd5e92cbdc97fee3a1059f6261938089e23af9bc1e52891c168
review:
  result: PASS
  candidate_commit: aed96a270441e9e9271a4c1ea9cd1ebcaadf5f9c
  reviewer: ReviewerMu3eCandidate
  author: testing-3-r2/testing-4-r2
  evidence:
    path: reports/review-mu3e-candidate-r1.md
    sha256: sha256:8816d82b6ab4e765f53f31a8fdace495fe1ef2820f9dca39d1fbda55d83c99d1
notes: "MU3e = TP3 + TP4（Order 8，末端测试单元），经 TP3 2 轮实现 + 2 轮独立检视、TP4 2 轮实现 + 2 轮独立检视收敛，两包最终复核均 PASS（0 CRITICAL / 0 MAJOR）。候选从目标基线 1e50bde 起（已含 WP7 r3 的 R15-6 修复），--no-ff 依次合并 feat/tp3（6e83070）与 feat/tp4（0a19d39）。**候选审查独立核实**：24 个路径的候选 blob 逐字节等于且仅等于其唯一来源（TP3 9 + TP4 12 + main-r3 3），路径集恰为三来源并集、无夹带无缺失、两两重叠为 0；两个合并提交经 git merge-tree --write-tree 复算，树的 SHA 与候选实际树逐字节相同（纯合并、无夹带编辑）。**R15-6 用例在候选上与 feat/tp4(0a19d39) 逐字节相同、断言未放松**，其转绿源于 1e50bde 的实现修复（新增 findDirectoryByAlias 在 directories_without_sessions 态按快照 workspaces 投影 zero-count 行），而非断言弱化。**这是本变更首次为组合根取得浏览器级自动化证据**：TP4 的 e2e 直接挂未经改动的 app/_layout.tsx，页面确实离开 RuntimeUnavailable（既有组件测试全走 renderToStaticMarkup、useEffect 从未执行过）。TP3 补齐了 tasks 2.6 划归它的跨语言向量对拍：真实 crypto.subtle + 真实 transcript.ts + 组合根生产 HostIdentityPort/DigestPort 跑通 device-proof.json 与 host-challenge.json 每个向量，判据是向量自带的 Rust 侧 p1363 签名反验生产字节（等价于逐字节相同）。e2e 独立复跑 3 次均 21/21、红集合为空。3 条 MINOR 登记备查（flipSignature 注释算据错误但行为正确、__pageErrors 跨用例累积的诊断噪声、TP3 四条非阻断观察），均不阻塞。deps/advisories/secrets 三个 CI-only job 本地无等价物，未执行亦未声称通过。"
```
