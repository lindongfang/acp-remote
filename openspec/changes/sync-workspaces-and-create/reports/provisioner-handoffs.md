# 资源与 worktree 交接记录（provisioner）

- 变更：`sync-workspaces-and-create`；单元 U1；日期 2026-10-02
- **角色路由偏离（如实记录）**：`openspec/agentic.yaml` 声明 provisioner 角色为 `deepseek/deepseek-flash`，但宿主把 `scout`/`reviewer` agent 类型路由到配额受限的模型（连续四次 `429 Go usage limit exceeded`），无法派发独立 provisioner 子 Agent。**本记录由主 Agent 直接执行并自行校验**，判据不降低，只改变执行者身份。

## 资源判定（对应 plan.md 的 `## Runtime Resources`）

| 资源 | 判定 | 隔离方案 | 负责人 |
| --- | --- | --- | --- |
| 构建目录 `CARGO_TARGET_DIR` | 可隔离 | 每个 WP 一个独立目录，以环境变量注入 | 主 Agent 分配、各执行者自建自清 |
| Node 依赖 `node_modules` | 只读共享 | 各工作树以目录 junction 指向主仓库同一份；`npm ci` 由主仓库已完成 | 主 Agent 确认就绪，执行者只读 |
| 数据库 / 端口 / 容器 / 外部服务 | 不需要 | — | — |

## worktree 创建与交接（按 (WP, Attempt)）

| WP | Attempt | worktree | 分支 | 基线提交 | Executor | Received At（取自 dispatch-queue.jsonl） | 校验方式 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| WP1 | 1 | `D:\Project\acp-remote-wt\wp1-vocab` | `feat/sync-vocab` | `6c093f145aa3d69dcd573a6d94e31b692acb5d4b` | coder-vocab | 2026-10-02T14:04:25.134Z | `git worktree add -b` 输出 HEAD 与基线一致；`git status` 干净 |
| WP1 | 2 / 3 | 同上 | 同上 | 同上 | coder-vocab-fix / coder-vocab-fix2 | 2026-10-02T15:07:01.789Z / 2026-10-02T17:58:38.991Z | 重开复用同一 worktree，未新建 |
| WP2 | 1 | `D:\Project\acp-remote-wt\wp2-wire` | `feat/sync-catalog-wire` | `6c093f145aa3d69dcd573a6d94e31b692acb5d4b` | coder-wire | 2026-10-02T14:04:28.650Z | 同上 |
| WP2 | 2 / 3 | 同上 | 同上 | 同上 | coder-wire-fix / coder-wire-fix2 | 2026-10-02T14:38:22.128Z / 2026-10-02T17:58:41.059Z | 重开复用同一 worktree |
| WP3 | 1 | `D:\Project\acp-remote-wt\wp3-core` | `feat/sync-core-projection` | `6c093f145aa3d69dcd573a6d94e31b692acb5d4b` | coder-core | 2026-10-02T14:04:30.502Z | 同上 |
| WP3 | 2 | 同上 | 同上 | 同上 | coder-core-fix | 2026-10-02T15:07:03.578Z | 重开复用同一 worktree |
| WP4 | 1 | `D:\Project\acp-remote-wt\wp4-storage` | `feat/sync-storage-v6` | `347399f45a3df3093c9f45c3f98769ba96f846c8`（= WP3 已验收交付提交，满足 `code:WP3` 的「上游进入已验收集成基线」） | coder-storage | 2026-10-02T15:44:25.310Z | `git worktree add -b feat/sync-storage-v6 <path> 347399f`，输出 HEAD 即 WP3 交付提交 |
| WP4 | 2 | 同上 | 同上 | 同上 | coder-storage-fix | 2026-10-02T17:04:05.492Z | 重开复用同一 worktree；该 worktree 亦被 merger 复用为集成工作树（计划：merger 不新建工作树） |

## 已知环境事实（影响证据解读，非缺陷）

- **`wp1-vocab` 与 `wp2-wire` 工作树中没有 `.husky/_`**，而仓库 `core.hooksPath = .husky/_`；git 在钩子路径无法解析时**静默跳过**，因此这两个工作树的提交实际未执行任何钩子。该缺口记为 `verification.md` 的 **PV0**，由候选 PV1（`npm run verify`）与远端 CI 关闭。后果之一是 WP2 的 rustfmt 缺陷直到集成时才暴露（见 `## Candidate Builds`）。
- `check-doc-links.mjs` 会扫描被 gitignore 的目录，未跟踪的评审产物中的相对链接会让门禁失败。候选构建与分支提交前均需把工作树的 `reports/` 移出（已分别移到 `_merge-stash` 与 `_stale-reports`，未被删除）。