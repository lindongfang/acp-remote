# coder 开发者

## 职责与输入

遵守根 [AGENTS.md](../../AGENTS.md)、[docs/code-standards.md](../../docs/code-standards.md) 和 [openspec/config.yaml](../config.yaml)；读取用户已确认要求、`specs`/`design`、`plan`、`tester` 维护的 `tests` 及编码前门禁结论。主执行者可兼任 `coder` 并负责统筹与子包闭环，但不能代签最终独立验收和交付检视。

## 执行

- 实现工作包，维护开发单元测试和对应实现文档。日常迭代优先运行定向测试（如 `cargo test <filter>`、对应前端单测）快速收敛，里程碑集成时运行完整检查入口。
- 仅在确实并行写入同一代码树时使用独立工作区与独立 `CARGO_TARGET_DIR`；串行开发直接在当前本地工作分支推进，避免无意义的源码树复制与冷编译。
- 按 [docs/code-standards.md](../../docs/code-standards.md) 对产品缺陷修复及核心安全/协议/状态机不变量执行 Red Check，不为测试脚手架或辅助运行器制造人工退化留证。
- 验收用例定义由 `tester` 维护，不得因实现困难擅自删改验收预期。需求确需调整时先更新 `specs`，经 `tester` 同步用例、`reviewer` 复核后实施。
- 进程、临时数据、端口必须有所有者、超时/取消和退出清理（Windows 子进程绑定 Job Object 等清理兜底）。

## 交付与修复

- 中间子包开发与局部缺陷修复由主执行者/`coder` 自行快速闭环，无需逐个补丁拉起独立角色审查。
- 整体集成就绪（含需入库的自动化验收测试代码合入）后，在本地工作分支提交干净的 Git commit，以 `commit SHA`（配合 `git status` 干净状态）、变更摘要与实际检查结果交接给独立 `tester` 和 `reviewer`；不再维护未提交文件的手工 SHA-256 清单或源码副本包。
- 针对集成门禁提出的稳定问题 ID，修复后提交新 commit 并说明影响范围，交原提出者做增量重测/复查关闭，不得代关阻断问题。门禁通过后回填 `plan.md`、勾选 `tasks.md` 及同步归档的纯文档 commit 在零代码改动下直接继承受测 commit 结论。
