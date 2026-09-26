# RV2-DU1 独立候选 merge 检视报告（node-link-owner / DU1）

> 说明：本轮 reviewer 子 Agent 无写文件工具（只读检视），报告由主 Agent 按其返回正文原样持久化（其中触发 doc-links 门禁的两处引用文本已由主 Agent 按 reviewer 建议改写——见 F1 的修复记录）。

## 结论

**PASS**（merge 检视，Target Revision `92fb9fdc22938c0ad743d7dec7a1d0932b38cde5`）；发现数：**0 × P0、1 × P1（工件范围，非候选树缺陷）、3 × P2**。Merge verdict：**OK with notes**。

## Review Context

| 项 | 值 |
| --- | --- |
| Review ID / Type / Stage | RV2-DU1 / merge / 候选准备（DU1，变更 `node-link-owner`） |
| Repository | `D:\Project\acp-remote-wt\nl-owner-integration`（候选 worktree） |
| Base / Target | base `94a64e1f15d26f54a6601985fd13b1440fec8170`；target `92fb9fdc22938c0ad743d7dec7a1d0932b38cde5` |
| 版本稳定性 | `watchdog_diff` 报工作区相对候选 HEAD 无改动 |
| 实际检查范围 | ①候选/基线/源提交的真实性核实（独立读 `.git` 引用与 reflog）；②merge 保真性（无冲突解决、无遗漏提交、无夹带）；③主 Agent 三处直接修复在候选树生效；④各 RV 报告「待候选 review 核对」项在候选树生效；⑤门禁脚本/检查计划未被弱化；⑥候选检查日志与命令的逐条对应 |
| 未执行（如实记录） | 未执行任何命令（无 shell/node/git）；未执行 E2E（not-applicable）；`cargo-deny`/`gitleaks` 只在 CI |
| 限制 | 无 base→target 的已提交 diff 制品；「候选树与源提交逐字节一致」以「引用/reflog 事实 + git 平凡合并语义 + 冲突残留全仓搜索」替代证明 |

## Correct（已核对且成立）

1. **版本与基线独立核实通过**：main = 94a64e1f、源 = 4c5a3f00、候选 = 92fb9fdc；reflog 显示候选由单次 `ort` 合并产生、无后续提交；基线自候选构造以来未移动。
2. **无冲突解决、无二次修补**：全仓无冲突标记、无 .orig/.rej 残留；源分支历史线性、合并基 = main 本身，合并结果在 git 语义上必然等于源提交的树（与集成报告的 `git diff --stat` 为空互证）。
3. **无遗漏、无夹带**：du1 报告的 31 个已验收提交逐个存在于源分支 reflog；除「变更目录规划工件 + 文档 + 代码」外未见无关交付面。
4. **门禁未被弱化**：`package.json` 的 check 十道门禁原样；2 个 `ignored` 是仓库声明的辅助用例；脚本口径未改。
5. **主 Agent 三处直接修复在候选树生效**：`2937850f`（§4 三条窄入口 + §5.1 指针）、`a882c06f`（§11 限定语 + 两处例外量词 + 修订记录顺序）、`4c5a3f00`（rv1-wp8 勘误）。
6. **RV3-WP7-F1/F2/F3 在候选树生效**（f9dbece1）：代价句、占位期客户端后果 + core 用例（候选 PV5 日志中实际执行）、常量钉死断言与 pump 注释。
7. **候选检查证据与候选提交绑定、命令与 plan 一致**：PV1 逐子项 exit 0（973 passed/2 ignored）；PV2 exit 0（含 server→acpr-wire 格）；PV5 394 passed/0 failed/0 ignored，declared=executed。
8. **RV 报告的「待候选核对」项逐条闭环**（RV1-WP8、RV3-WP7、RV2-WP5/WP6/WP7 的 PV5 PENDING 均由候选轮次新证据补齐）。

## Findings

| ID | 级别 | 位置 | 问题 | 处理 |
| --- | --- | --- | --- | --- |
| RV2-DU1-F1 | **P1（工件范围，不在候选树内，不阻断 merge 判定）** | 规划根 `reports/du1-integrate.md` 第 118/119 行 | 该行内的段落引用被 check-doc-links 按邻近文档名归因而误报（该报告无数字小节/归属到 CONFIG_REFERENCE 无此节） | **主 Agent 已修复**（改写为「第 x 节」隔断归属，规划根 `npm run check:docs` 复绿） |
| RV2-DU1-F2 | P2 | 候选树 `openspec/changes/node-link-owner/**` | 候选树内 verification.md 无 Merge History（快照早于候选记录）；reports/ 无 du1-integrate.md 与 .log | 主 Agent 口径：最终同步在合入前后完成（verification.md 的 Merge History 已写候选记录；`.log` 原始证据不入库为既定惯例，合入后主分支上的规划目录与权威规划根一致） |
| RV2-DU1-F3 | P2 | 主检出合入步骤 | 主检出当时的 `M docs/DEVELOPMENT_PLAN.md`（行尾噪声）+ 未跟踪变更目录会挡住合入 | 主 Agent 在 6.6 前处理（checkout 恢复噪声 + 移走未跟踪副本）并留证 |
| RV2-DU1-F4 | P2 | `plan.md` Main E2E 的 `basis` | 「本变更不改 wire 协议与封闭词表」与 D13 事实不符 | **主 Agent 已修复**（改为「按用户裁决 A 调整了 node.challenge 的 wire 必填字段，不改变 E2E 适用性判断」） |

## Assessment（检查 ID 逐条核对）

- PV1/PV2（候选 92fb9fdc）：du1-candidate.log ATTEMPT2 段逐子项 exit 0；REUSED 口径记录。
- PV5（候选，Windows）：394 passed / 0 failed / 0 ignored；flake 已登记为 EX8 并以 attempt 2 为准。
- 检查计划未被弱化；E2E not-applicable 有用户批准记录。
- 待补：CI 的 deps/advisories/secrets 三个 job（无本地等价物）；主分支阶段 PV1–PV5 属 6.7。

## 残余风险

1. attempt 1 的 PV5 flake（EX8）：根因未定位，若主分支轮次复现须升级为缺陷立案。
2. 规划根 reports/ 的新增报告都需遵守「段落引用用「」隔断」约定（避免复现 F1 类误报）。
3. CI 专属判定与 Unix 专有用例本机未覆盖。

## handoff_index

```yaml
handoff_index:
  - { task_id: "6.4", role: reviewer, phase: review, stage: candidate, target_revision: "92fb9fdc22938c0ad743d7dec7a1d0932b38cde5", evidence_type: REVIEW, evidence_id: RV2-DU1, report_path: "reports/rv2-du1.md", result: PASS, evidence_status: NEW, applicability_basis: "新建独立只读 reviewer（不继承实现/集成对话）；独立读取 .git 引用/reflog 核实版本与无冲突解决；逐项核对主 Agent 修复与各 RV 待核对项在候选树生效；1 条 P1（工件范围，已修复）+ 3 条 P2", source_evidence: NOT_APPLICABLE }
  - { task_id: "6.5", role: reviewer, phase: review, stage: candidate, target_revision: "92fb9fdc22938c0ad743d7dec7a1d0932b38cde5", evidence_type: CHECK, evidence_id: PV1, report_path: "reports/du1-integrate.md", result: PASS, evidence_status: REUSED, applicability_basis: "候选轮次证据（du1-candidate.log ATTEMPT2）由集成子 Agent 产出；reviewer 只读核对未复跑", source_evidence: { id: "PV1", report_path: "reports/du1-integrate.md", target_revision: "92fb9fdc22938c0ad743d7dec7a1d0932b38cde5" } }
  - { task_id: "6.5", role: reviewer, phase: review, stage: candidate, target_revision: "92fb9fdc22938c0ad743d7dec7a1d0932b38cde5", evidence_type: CHECK, evidence_id: PV2, report_path: "reports/du1-integrate.md", result: PASS, evidence_status: REUSED, applicability_basis: "同上", source_evidence: { id: "PV2", report_path: "reports/du1-integrate.md", target_revision: "92fb9fdc22938c0ad743d7dec7a1d0932b38cde5" } }
  - { task_id: "6.5", role: reviewer, phase: review, stage: candidate, target_revision: "92fb9fdc22938c0ad743d7dec7a1d0932b38cde5", evidence_type: CHECK, evidence_id: PV5, report_path: "reports/du1-integrate.md", result: PASS, evidence_status: REUSED, applicability_basis: "候选轮次 attempt 2 命令与 plan PV5 列一致、394/0/0；attempt 1 的 flake 已登记为 EX8", source_evidence: { id: "PV5", report_path: "reports/du1-integrate.md", target_revision: "92fb9fdc22938c0ad743d7dec7a1d0932b38cde5" } }
```
