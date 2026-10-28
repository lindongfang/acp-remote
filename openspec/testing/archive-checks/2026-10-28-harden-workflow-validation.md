# harden-workflow-validation 归档后复核

本记录单列归档后的证据，不改写归档内的历史plan和验收结论。

## 受测内容与移动映射

- 基线提交：dfe136128844f016e3d806948997a2df7a61b712；本次修复尚未提交。
- 原路径：openspec/changes/harden-workflow-validation/。
- 归档路径：[2026-10-28-harden-workflow-validation](../../changes/archive/2026-10-28-harden-workflow-validation/)。
- [S2受测清单](../../changes/archive/2026-10-28-harden-workflow-validation/verification-manifest.json)的SHA-256为ade09aa0591097f9a908ab8be34af9c37c79238a6faea980999a26ae44350396，18项；仅变更目录前缀按上述映射替换，其他路径不变。
- plan只对Findings之前归一LF后取摘要；tasks只归一LF并把checkbox状态替换为未勾选；其余受测内容逐字节摘要。所有实现、配置、测试及用例均在清单内，排除的结果记录不能掩盖实现变化。
- 归档移动的8个文件为.openspec.yaml、proposal.md、design.md、tests.md、plan.md、tasks.md、verification-manifest-s1.json、verification-manifest.json；移动前后逐字节SHA-256全相等。
- 将上述文件按文件名排序形成{path,sha256}数组，再对JSON.stringify(array)求SHA-256，归档前后均为fa0fb99774d8d9e1e3c586cf88556ed4d65633cacb9e2fd5d56f3d1dcb684db6。

## 已完成验收与检查

- 主执行者在S2执行根目录npm.cmd run verify，退出0：全部合同门禁、47/47工作流测试、Rust格式、Clippy与工作区测试通过。原始日志.worktrees/harden-verify-s2.log为附加证据。
- 独立tester对WF-HARDEN-001～008全部验收通过，具体独立断言摘要已入[归档plan](../../changes/archive/2026-10-28-harden-workflow-validation/plan.md)。包括任务12项、YAML基础64项及REV-001双模式12项/围栏标题1项、规范根5布局、完整门禁9场景、真实CLI依赖4断言；独立npm.cmd run check退出0，47/47通过。
- 独立reviewer在S2复算18项匹配，相比S1只变更两个脚本；重新检视变动，复用未变16项依据。REV-001已由原reviewer复查关闭，其他前置问题全部闭环。
- 归档CLI确认8/8任务完成，无增量规范需要同步；长期回归suite保留原ID并补新反例。Why短于50字符为非阻断建议，保留已验收历史内容，不回写归档。
- 归档后根目录npm.cmd run check退出0，47/47工作流测试及全部合同通过；日志.worktrees/harden-post-archive.log。后置记录新增后另运行check-doc-links和git diff --check，均退出0。

## 独立归档复核

- reviewer：归档后复核通过。独立复算8/8原字节摘要及aggregate一致，映射后S2清单18/18匹配，旧目录消失且归档恰8文件；最终plan逐ID摘要、问题关闭/限制/复用理由与tester结果相符，8任务有相应证据。复用已核对的归档后check退出0，独立git diff --check退出0，无新增阻断。
- tester：归档后复核通过。独立复算8/8原字节保真及aggregate一致，路径映射后18/18匹配；旧目录不存在、tasks8/8，最终plan摘要如实。复用归档后check退出0，47/47通过，无新增问题。两个角色均确认后置结论写本记录，保留归档plan原来的历史状态。

## 范围与限制

本次只改开发工作流，不改产品/前端。预存workspace_mkdir_browser浏览器用例因需本机Chromium及构建Web UI被标记ignored，不声称已经运行；CI供应链、安全公告及密钥专项不属于本地verify，不声称已通过。L1无类型变化、L6无产品链路变化、LX无人类专属操作，不适用理由及替代检查见归档tests。

独立原始报告保存在被忽略的openspec/testing/runs/harden-s1、harden-s2和harden-post目录，仅作附加证据；本记录与归档plan保存可独立判断的摘要。S1失败历史保留，不能把S2结论回填为S1通过。
