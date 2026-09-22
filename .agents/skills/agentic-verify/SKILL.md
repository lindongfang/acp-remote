---
name: agentic-verify
description: 验收 `agentic` 工作流变更的最终主分支实现和交付证据；用于该 schema 的最终验收、/opsx:verify 或归档前检查。普通代码审查或流程配置编辑不触发产品验收。
---

# Agentic Final Verification

这是 agentic 的证据验收入口。CLI 的 artifact 完成状态与任务计数作为输入保留，
不充当验收结论。此入口不实现产品修复、代码 review、测试或归档。

1. 从用户请求或会话确定变更；不明确时用 `npx --quiet --no-install openspec list --json` 查找，多个候选时询问。
   保留已选择的 `--store` 等根目录选择参数，后续命令使用同一规划根。
   同时确定本次是“只读核查”还是“执行验收并更新进度”：用户仅要求只读检查时不写任何文件，
   后续 `e2e check` 与任务回写都使用只读模式（`e2e check --no-write`）；只有明确授权更新进度时才执行会回写的检查。
   若在测试 worktree 中执行，显式传入权威规划根（`--planning-root <主仓库路径>`），不用当前目录推断。
2. 执行 `npx --quiet --no-install openspec status --change <name> --json` 和
   `npx --quiet --no-install openspec instructions apply --change <name> --json`，确认 schemaName 为 agentic。
   其他 schema 不套用本入口。读取 `context`、全部 `contextFiles` 和存在的自定义指令；
   即使 `all_done` 下自定义指令已被替换，也继续以下检查。
   同时运行 `npx --quiet --no-install openspec-agentic e2e check --change <name> --json`（只读核查加 `--no-write`）：它给出项目开关值、
   该变更 Main E2E 判据的结论（PASS / FAIL / BLOCKED），以及 required 变更是否已有 `e2e run` 写入的成功执行
   记录（含命令、退出码、提交与输出片段）；单变更检查要求变更已完成，仅扩展拥有的最终 E2E 行与正在执行的
   最终验收行（[final-verification]）可待办，其余任务未勾完即 BLOCKED。判 PASS 时该检查
   会按 `[e2e-owned]` 标记自动勾选最终 E2E 任务行、非 PASS 时自动将其回退（行级单一所有者，这是该行状态的唯一来源，不视为产品修改）；
   只读核查用 `--no-write`，此时不回写，只报告需要修正的任务。
   FAIL 或 BLOCKED 时不得给出可归档结论，先执行 E2E 并留证或回写计划。该检查只做结构比对，不证明测试真实性；
   本入口不执行测试，也不代替实现者用 `--run-if-missing` 执行。
3. 用 `npx --quiet --no-install openspec schema which agentic --json` 定位 schema，完整读取
   `<schema.path>/procedures/acceptance.md`，按该文件完成验收。
   程序文件缺失或无法读取时报告 BLOCKED，不回退到默认 verify 的“全部通过”。

默认 `/opsx:verify` 的需求、场景和设计一致性检查仍需完成；agentic 的证据检查和
当前最终验收任务例外按上述专用程序执行。不要重新进入 apply 编码循环，
也不要将本次一致性检查冒称为新的独立 code review。
