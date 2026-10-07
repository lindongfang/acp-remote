<!-- MU3d 的 premerge receipt。premerge 门禁 PASS 之后持久化。 -->

```agentic-premerge
version: 1
target_ref: refs/heads/main
target_commit: 4ada2a24e23ff3f07d37eb0face70921a66ed987
candidate_commit: f6ea252e09fd65834a67fe7123c786beec339209
delivery_unit: MU3d
contract_digest: sha256:ecfc245c119d41b1b76a7bfdde9b11f93e751628a894f4b56f1fc2dba90e0e91
requirements_digest: sha256:c2eb41a848e4e9bd24f4e03550709fe3a44c560574fe45c4bbf040fb34cf2752
verify:
  result: PASS
  candidate_commit: f6ea252e09fd65834a67fe7123c786beec339209
  executor: coder-w7-r2（作者） / merger-mu3d（合入轮复跑）
  command: npx tsc --noEmit + npx vitest run（28 文件 / 359 passed）+ node scripts/run-browser-check.mjs（真实 Chromium 16/16）+ npx expo export --platform web ＋ npm run check（根十道，cwd=.worktrees/wp7）
  evidence:
    path: reports/PV3-wp7-r2.log
    sha256: sha256:020c2421656f4df0bd8eb024184b75cd24c396ef019b8f31d52c22d3e4d52a85
review:
  result: PASS
  candidate_commit: f6ea252e09fd65834a67fe7123c786beec339209
  reviewer: ReviewerWp7R2
  author: coder-w7-r2
  evidence:
    path: reports/review-wp7-r2.md
    sha256: sha256:86ddba5e8cfc98b03b18834a26e753d737f6e0864ef07e1b6bc4a8f1233c484d
notes: "MU3d = WP7（Order 7，独立单元），经 2 轮实现 + 2 轮独立检视收敛，第 1 轮检视 FAIL（1 CRITICAL + 2 MAJOR）、第 2 轮 PASS（0 CRITICAL / 0 MAJOR / 3 MINOR）。交付 R15–R19 全部页面与共享组件：目录页（只显示展示名与别名、不推导本机路径、同名目录不合并、两种空态可区分）、目录详情、对话页（上下文条三态、游标翻页）、配对页、创建弹层（载荷只含两个引用）、降级卡片与诊断抽屉（三类可区分、原文未同步不渲染占位）、主机与连接面板（覆盖层缺席呈现状态未知、不得以会话活跃度推断）。Write Scope：标准 app/ + src/features/ + src/components/，另有两处**用户 2026-10-05/06 授权的例外**——src/sync-client/（仅修复 MU3c 带入的 P1-N1/N2）与 src/composition.ts（层级中立组合根）。**第 1 轮的三条阻断项**：CRITICAL-1 门面 #isAdoptableBarrier 以旧 epoch 的 ackedCursor 为基准，服务端事件库重建后合法新 epoch 快照被永久丢弃（第 1 轮 P1 修复自身引入的回归）；MAJOR-1 client-store 把 raw 硬编码 absent 使 raw_not_synced 分支在生产路径不可达（违反 R18 MUST）；MAJOR-2 降级卡片把非 unsupported 原因兜底为 client、谎称「客户端不支持」。三者均由第 2 轮定点修复并经 reviewer 自写探针独立复现关闭，两个平凡实现方向各自复现为红。**组合根首次落地**（review-wp7-r1 判为计划级遗漏而非阶段边界）：src/composition.ts 落地 openPlatform()、五个缺失端口、HttpPairingTransport、SyncClient 接线，app/_layout.tsx 不再写死 store={null}；layers.ts 与 layers.test.ts 一行未改。**最高风险路径由 reviewer 实测**（作者与 r1 reviewer 均未执行）：真实 P-256 密钥对经 rememberPairedHost 写入真实记录，P256_SPKI_PREFIX 与 Node 真实 SPKI 前 26 字节逐字节一致，用 device-proof.json 的 65 字节 SEC1 公钥 + 64 字节 P1363 签名端到端 verify:true（DER 对照 false），错误签名阻断 host_identity_changed(4409)，无「验签失败默认通过」降级分支。3 条 MINOR 登记备查（配对无 UI 入口、authenticatedInfo getter 缺 connected 门控、dispose() 使模块单例不可复活），均不违反 spec MUST、非本轮引入。deps/advisories/secrets 三个 CI-only job 本地无等价物，未执行亦未声称通过。"
```
