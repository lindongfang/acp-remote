<!-- MU3a 的 premerge receipt。premerge 门禁 PASS 之后持久化；final/archive 阶段据此核对本单元的历史合入是否仍成立。 -->

```agentic-premerge
version: 1
target_ref: refs/heads/main
target_commit: 8ca1e9ec365e0cb91b08dca6f9d554f0534db29c
candidate_commit: a4440d683dc44ecb69d2ffeb596fbff53170272c
delivery_unit: MU3a
contract_digest: sha256:ecfc245c119d41b1b76a7bfdedde9b11f93e751628a894f4b56f1fc2dba90e0e91
requirements_digest: sha256:c2eb41a848e4e9bd24f4e03550709fe3a44c560574fe45c4bbf040fb34cf2752
planning_digest: plan-v2:sha256:78d7d1f6a0818326acab3fc17170a2b452453c42d6a5ac09632a68d1140d0229
verify:
  result: PASS
  candidate_commit: a4440d683dc44ecb69d2ffeb596fbff53170272c
  executor: coder-w5a-r2（作者） / merger-mu3a（合入轮复跑）
  command: npm run check（仓库根十道，cwd=.worktrees/wp5a）＋ cargo fmt/clippy/test --workspace；并 npx tsc --noEmit + npx vitest run + npx expo export --platform web（cwd=clients/app）
  evidence:
    path: reports/PV1-wp5a-r2.log
    sha256: sha256:8cd510d29e5f8b73d3bf4087cbfa72e00ab801533f83df19fe648a68c0443646
review:
  result: PASS
  candidate_commit: a4440d683dc44ecb69d2ffeb596fbff53170272c
  reviewer: ReviewerWp5aRecheck
  author: coder-w5a-r2
  evidence:
    path: reports/review-wp5a-r2.md
    sha256: sha256:bba88c0f2299768454bb3aeecc00dc8870c3b81e517d502761ec0283597e4a6e
notes: "MU3a = WP5a（Order 4，独立单元）。本变��第一批前端代码：建立 clients/app 的 Web/PWA 工程骨架、七层单向依赖目录、严格类型化的 src/protocol/（以 paths.ts 上溯四级解析仓库根读取 schemas/f1xtures，不复制任何合同资产）与 src/domain/ 视图模型。自 main 起 38 files / +15746，可快进；schemas/、fixtures/、crates/、docs/ 与根 package.json 零改动。两轮检视：第 1 轮 PASS 并留下 4 项 MINOR（F1 protocol barrel 运行期带入 node:*；F2 分层方向检测被裸 barrel/副作用导入绕过；F3 扫描根不含 app/** 使页面层约束成为空断言；F4 viewDef 声明未校验且 viewSchema 路径不受约束），第 2 轮修复合再次 PASS 且零新增——复核方独立只读复现 5 种注入形态在当前实现全部检出而在修复前全部漏检，并自造 6 类新绕过形态均未逃逸，扩根后合法 app→components/features 无误报。Web-only 范围出自 tasks 2.6 的显式裁决（无 .native.ts、platforms=['web']）。deps/advisories/secrets 三个 CI-only job 本地无等价物，未执行亦未声称通过。"
```