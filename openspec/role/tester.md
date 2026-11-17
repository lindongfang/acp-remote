# tester 测试人员

## 独立性与输入

使用不继承 `coder` 开发对话的独立人员或 Agent 会话。输入为用户确认要求、`AGENTS.md`、`openspec/config.yaml`、`specs`/`design`/`plan`、受测 Git `commit SHA` 及交付判据；自行读取代码和现有测试，不以开发自评作为通过依据。

## 编码前（仅针对即将开工的变更）

- 仅当变更正式进入编码前准备阶段时，依据当前 `proposal`/`specs`/`design` 和真实代码架构编写一份 `tests.md`；未开工的远期变更不预写或联动维护 `tests.md`。
- 按[测试模板](../schemas/spec-driven/templates/tests.md)登记实际需要的 L1–L6/LX 用例与四列需求关联表：L5 覆盖需求的正常、异常、取消与边界场景，L6 明确端到端链路与真实/模拟边界，LX 严格限于 Harness 无法通过工具验证的场景。
- 编写或更新 `tests.md` 后运行 `node testing/suites/verify-suite.mjs --change <name>` 自动校验用例字段、稳定 ID 及增量 `Requirement`/`Scenario` 100% 覆盖完整性，再交独立 `reviewer` 在编码前门禁一次性合审。

## 集成验收执行

- 若需补充纳入版本库的自动化验收测试代码（如 `app/tests/*_acceptance.rs` 或前端验收用例），在锁定受测集成 `commit SHA` 前先提交入本地分支，确保正式验收期间受跟踪文件工作区保持干净并与 `reviewer` 共享同一 `commit SHA`。
- 在与 `reviewer` 相同的固定集成 Git `commit SHA` 上执行正式验收（核对 `git rev-parse HEAD` 与干净受跟踪工作区，不再逐文件重算手工 SHA-256 清单）。同 commit、同范围且环境符合要求的检查结果可直接复用，不机械重跑全量构建。
- 使用隔离的测试数据目录、端口和受管进程；只读复用已有编译产物时不额外复制源码树与冷编译目录。失败、取消或结束后清理本次创建的进程树与临时资源，保护用户真实数据。
- 核对实际执行的测试数量与断言有效性，零用例或仅启动/截图不算通过。缺环境、凭据或入口如实记 `not-run`，不自动转为 LX，也不以模拟替身冒充真实 Harness 通过。
- 详细运行日志保存在本地被 Git 忽略的 `testing/runs/<change>/` 下，同时将精简、可独立判断的逐例结果摘要交主执行者更新至 `plan.md` 的 `Verification Results` 节，不把大段过程日志或源码副本入库。

## 闭环与归档合并

- 发现问题时记录稳定 ID、严重性、受测 `commit SHA` 与复现步骤；`coder` 提交修复 commit 后，仅针对受影响用例与必要回归项重测并关闭自己提出的问题。
- 归档前将稳定用例按**单一主归属能力**合并到 `testing/suites/<capability>/tests.md`（若同一能力历经多个归档变更，在 `Source definitions:` 累加归档源路径；跨能力共用用例只在主能力套件保留一份 `### <ID>` 正文，关联套件按 ID 说明引用），并运行 `npm run check:spec` 验证一致性。
