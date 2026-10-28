# Plan

## Execution Model

主执行者统筹并承担配置 coder；docs_coder 是隔离文档 coder，tester 独立规划/验收，reviewer 独立检视。三者不继承开发对话。基线 e84e0137c5266337ed003ae9376c537589b59591，用户未提交修改为零。worktree 分别为 .worktrees/workflow-docs 和 .worktrees/workflow-tester；主配置在根工作区。交付使用文件差异和可读报告，不提交。

## Parallel Assessment

WP1 与 WP2 无共享写范围，接口为下述固定路径，可并行；tester 规划可与准备工作并行。WP3 等待集成快照，独立测试与检视可并行。相同文件仅主执行者集成后修改。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 配置/角色/模板/结构门禁 | 方案与用例检视 | 主执行者 coder | openspec/**（除adr和tester用例）、scripts/**、.gitignore | 差异及说明 | 根目录 check:openspec 与独立负向测试 |
| WP2 | ADR迁移和现行文档索引 | 方案检视 | docs_coder | docs/**、openspec/adr/**、README.md、AGENTS.md、现行代码中的ADR引用 | 隔离工作区文件差异 | git diff --check、集成后 check:docs |
| WP3 | 独立验收和交付检视 | WP1、WP2 | tester、reviewer | 独立报告；tester拥有tests.md验收用例 | 固定快照结果与问题闭环 | tests.md 对应入口、核实需求与真实差异 |
| WP4 | 修复、回归同步、归档准备 | WP3 | 主执行者、tester、reviewer | 问题涉及范围、testing/suites、归档目录 | 最终报告与归档 | npm run verify、受影响重测与复查 |

## Merge Order

WP1、WP2 并行，主执行者按文件清单合入 WP2，不使用全局覆盖。WP3 对同一文件摘要快照只读验收。修复后重建快照并重测相关用例，WP4 最后处理路径和历史记录。

## Project Verify

根目录 node/npm 依赖已存在；worktree 使用仓库同一 lock 安装依赖，Rust 构建隔离 CARGO_TARGET_DIR。具体用例见 tests.md，最终执行 npm run verify；前端产品未改，无需前端验收。负向验证在临时目录或独立工作区进行，禁止破坏主工作区。

## Code Review

reviewer 先检视方案/用例，之后检视固定快照中的配置、依赖闭包、角色边界、门禁漏检及ADR链接。阻断问题关闭前不得标记完成，coder 不代签 tester/reviewer。

## Main E2E

mode: not-applicable。本变更不改变产品用户链路；替代为真实 OpenSpec CLI 的产物依赖、用例缺失/损坏拒绝与 docs 检查。ego-browser 不适用于本次工具链变更。

## Handoff

每次交接明确角色、已确认要求、适用规则、基线/内容摘要、读写范围、交付文件与通过条件。独立角色可读设计理由但不继承 coder 对话或自评。

## Findings

| ID | Source | Severity | Finding | Status | Verification |
| --- | --- | --- | --- | --- | --- |
| REV-P01 | reviewer | 阻断 | 未来探索逐轮确认未明确 | reviewer复查关闭 | design Decision Coverage及WF-006 |
| REV-P02 | reviewer | 阻断 | coder开发单元测试职责不完整 | reviewer复查关闭 | design Decisions及WF-011 |
| REV-P03 | reviewer | 建议 | 候选CLI用例产物顺序 | reviewer复查关闭 | WF-008要求tests先于plan |
| REV-P04 | reviewer | 建议 | 缺擅改验收预期反例 | reviewer复查关闭 | WF-011验收修改反例 |
| REV-P05 | reviewer | 建议 | 最终快照和结果记录范围 | reviewer复查关闭 | design明确受测清单与独立证据记录 |
| REV-S1-001 / SELF-001 | reviewer/coder | 阻断 | README开头遗留默认串行和按风险自检 | reviewer S2复查关闭 | README统一新工作流 |
| REV-S1-002 | reviewer | 阻断 | 长期suite仍绑定一次性变更验收 | tester复验、reviewer S2复查关闭 | 长期suite按当前能力及稳定ID重写，删除一次性迁移例 |

## Verification Results

S1 内容快照清单在 openspec/testing/runs/workflow-s1/content.json；2059条文件/删除记录，SHA-256为37b58ebf4671b246c01295836d6f02a5e7d6ddea2cf14f0dae81069c8667c459。该摘要排除本文件、tasks勾选和忽略的原始报告。tester/reviewer已核对清单，独立报告保存在同一runs目录。S1因上述两个问题未通过交付检视，后续版本将重测复查。

主执行者已在S1根工作区运行npm run verify，退出0：合同门禁、格式、Clippy和Rust工作区测试通过，原始输出 .worktrees/workflow-verify.log。前端逻辑未改，不执行前端验收；CI依赖/供应链/密钥专项未在本次本地验证，不声称通过。34项结构自动化测试通过，不代表L5/L6语义验收已完成。

S2文件/删除清单仍2059条，SHA-256为79c5f7ae5a3ad16a026745a486bdedf410076bf1dc2c2994f61f8c51f45da917；与S1仅README、长期suite、检查器注释和新增真实CLI测试4个受测路径变化。tester/reviewer独立复算一致；对应原始报告 openspec/testing/runs/workflow-s2/tester.md 与 reviewer.md。35项自动化测试（含真实CLI产物依赖/恢复/归档保真）及tester独立npm run check通过，S1的97项独立负向断言在实现未变范围复用；17份ADR仅链接变化、520份旧归档逐字未变。

主执行者在S2再次实际运行npm run verify，退出0，原始输出 .worktrees/workflow-verify-s2.log。tester归档前适用验收通过，reviewer交付检视两项阻断已关闭。WF-012/013的实际主变更归档及后置检查尚待执行，不在此提前标通过；tester和reviewer均独立确认将其置于非tracked follow-up，避免归档前置循环。归档后以新独立运行报告记录路径映射、内容保真和受影响检查，不改写此历史记录；后置未通过不得最终交付或PR。
