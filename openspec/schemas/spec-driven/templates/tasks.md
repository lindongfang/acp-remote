# Tasks

<!-- 仅在无 x-deferred: true 标记且变更进入实施准备时生成。每条真实 checkbox 必须按完整词元引用至少一个 plan.md 工作包 ID（如 WP1），
并在同一行写明完成判据（测试、命令、可观察行为或交付物）；每个开发任务组自带对应单测与实现文档。
两阶段勾选：开发任务（如 1.x）完成自测与文档后在锁定受测集成 commit SHA 前勾选；集成门禁任务（如 2.1）保持未勾选并交接给独立门禁，待门禁通过后在纯文档收尾 commit 中勾选。 -->

## 1. <!-- 开发工作包任务组名称 -->

- [ ] 1.1 WP1 <!-- 任务描述；完成判据：对应单测/检查命令与可观察结果 -->
- [ ] 1.2 WP1 <!-- 任务描述；完成判据：对应单测/检查命令与实现文档同步 -->

## 2. <!-- 集成验收与交付检视 -->

- [ ] 2.1 WP2 <!-- 固定本地分支集成 commit SHA，完成独立 tester 验收与独立 reviewer 交付检视；完成判据：必需用例通过、阻断问题闭环且摘要写入 plan.md -->

## Workflow follow-up

- 归档在同一本地提交内按原子顺序执行：① 常规变更同步增量规范至 `openspec/specs/<capability>/spec.md`（`skip_specs: true` 变更跳过） → ② 移动变更目录至 `openspec/changes/archive/YYYY-MM-DD-<name>/` 并补齐内部指向变更外相对链接的 `../` 层级，同步更新 `README.md`/`docs/` 引用 → ③ 将新增/修订用例 `### <ID>` 正文合入主归属能力套件 `testing/suites/<capability>/tests.md` 并按归档目录日期升序追加 `Source definitions`，同时更新受影响能力套件的四列 `## 需求关联` 表及 `REMOVED`/`retire_capabilities` 清理（仅在 `## 需求关联` 按 ID 跨能力引用的关联套件不追加 `Source definitions`；`skip_specs: true` 无长期套件改动时跳过） → ④ 运行 `npm run check:spec` 校验通过后提交（本节保持普通列表，严禁使用 `- [ ]` 或 `- [x]` checkbox）。
