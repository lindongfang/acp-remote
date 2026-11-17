# reviewer 检视人员

## 独立性与依据

使用不继承 `coder` 开发对话的独立人员或 Agent 会话，与 `tester` 由不同执行者承担。默认只读检查当前工作区（无需复制构建环境），仅在确需运行带副作用检查时隔离临时资源。

机械结构规则（Schema 产物集合、章节完整性、工作包列数、`Main E2E` YAML 格式、`WP ↔ tasks` 双向绑定、`Requirement`/`Scenario` 或 `Success Criteria` 四列映射覆盖、长期套件 `## 需求关联` 100% 覆盖及归属）统一交由 `npm run check:spec` / `node testing/suites/verify-suite.mjs --change <name>` 机器门禁执行，`reviewer` 不再做人肉正则逐字核对，专注架构、协议、安全与逻辑审查。

## 两道独立门禁

### 1. 编码前门禁（方案与用例一次性合审）
- 仅对 `.openspec.yaml` 为 `schema: spec-driven` 且即将开工的变更执行；确认 `node testing/suites/verify-suite.mjs --change <name>` 与 `npm run check:spec` 已通过后，一次性合审 `proposal`/`specs`/`design`/`tests`/`plan`/`tasks`。
- 重点核查：用户需求决策是否均有明确确认依据（未把沉默/建议当确认）、协议与安全边界是否自洽、工作包隔离与合入顺序是否可行、`tests.md` 断言是否足以约束真实行为与异常边界、LX 是否确属工具无法验证。
- 审查结论简要记录，无未闭环阻断即可放行进入完整编码实现，不在子包开发途中反复插入微审查。

### 2. 集成交付门禁（绑定集成 Commit SHA）
- 在全部工作包（及需入库的验收测试代码）提交并完成本地工程检查后，与 `tester` 针对**同一本地分支 Git `commit SHA`**（核对 `HEAD` 与干净的受跟踪状态）执行交付检视，不再核对几百文件的手工 SHA-256 清单或尾扫报告链。
- 重点核查：`git diff` 实际代码变更、需求与协议语义符合性、未知字段保真、失败/取消与并发资源清理（含 Windows 进程树与句柄）、密钥/凭据零泄漏、核心改动 Red Check 证据、`tester` 入库验收测试/协议探针代码的断言真实性与防假阳性（对照 [LESSONS.md](../../LESSONS.md) 排查仅验证前置部署/截图、模拟替身冒充真实 Harness、过宽子串匹配等伪断言），以及 `plan.md` 中 `Verification Results` 摘要的真实性。

## 问题闭环与归档继承

- 每个问题记录稳定 ID、严重性、代码/文档位置、受审 `commit SHA` 及修复建议。`coder` 在新 commit 修复后，`reviewer` 仅对 `git diff <old-sha>..<new-sha>` 差异范围及关联影响做增量复查并关闭问题，不得由 `coder` 代关。
- 收尾与归档前，核查 [LESSONS.md](../../LESSONS.md) 中是否有复发达到 3 次、需同步晋升至 [docs/code-standards.md](../../docs/code-standards.md) 的工程教训。
- 门禁通过后，仅用于回填 `plan.md` 结果摘要、勾选 `tasks.md` 集成门禁任务、同步 `openspec/specs/` 与 `testing/suites/` 及移动 `archive/` 的后续纯文档提交，只要 `git diff <tested-sha>..HEAD` 无任何产品/测试代码及构建配置变更并通过 `npm run check:spec`，直接继承 `<tested-sha>` 审查结论，无需重复触发审查循环。
