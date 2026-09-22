// 提交信息规范（Conventional Commits）的唯一机器定义。
//
// 类型与 scope 词表只在这里维护一处：本地 `commit-msg` 钩子（`.husky/commit-msg`）、
// CI 的提交范围校验（`.github/workflows/ci.yml` 的 `commits` job，走 `npm run lint:commits`）
// 和文档（`AGENTS.md` §8）都指向本文件，不要在别处再抄一份词表。
//
// 与 `@commitlint/config-conventional` 的差异，都是为了中文仓库的实际书写：
// - 主题用中文书写，没有大小写概念，因此关闭 `subject-case`；
// - 中文正文不按英文 100 列折行，因此关闭 `body-max-line-length` / `footer-max-line-length`；
// - `type-enum` 与 `scope-enum` 收窄到本仓库实际存在的边界。
//
// 合并提交、`Revert ...` 与 `fixup!` / `squash!` 提交由 commitlint 的默认 ignore 规则跳过。
// scope 可选；写了就必须落在 `SCOPES` 里。仓库出现新边界（新 crate、新协议、新交付面）时
// 在这里补一项，并在同一提交里更新 `AGENTS.md` §8 的说明。

/** 允许的提交类型。`!` 或 `BREAKING CHANGE:` 表示破坏性变更。 */
export const TYPES = [
  "feat", // 新增能力
  "fix", // 修复缺陷
  "docs", // 只改文档
  "style", // 只改格式（不改行为）
  "refactor", // 不改行为的重构
  "perf", // 性能
  "test", // 测试
  "build", // 构建、依赖、工具链
  "ci", // CI 配置
  "chore", // 其他杂项
  "revert", // 回滚
];

/** 允许的 scope，对应仓库的模块与交付面边界（`docs/MODULE_ARCHITECTURE.md` §3.1）。 */
export const SCOPES = [
  "core",
  "storage", // storage-sqlite
  "sync", // sync-protocol、server::sync
  "node-link", // node-link-protocol、server::node_link、node-link-client
  "acp", // acp-protocol、server::acp_facade
  "agent-host",
  "identity", // identity-auth、identity-keystore
  "server",
  "app", // daemon、CLI、组合根
  "frontend", // Web/PWA 与后续原生客户端
  "transcript", // acpr-transcript
  "wire", // acpr-wire
  "compat", // compatibility/、schemas/、fixtures/
  "scripts", // scripts/ 下的合同校验脚本
  "docs",
  "ci",
  "deps",
  "release", // npm 分发与发布
  "repo", // 仓库级配置（workspace、.gitignore、AGENTS.md 等）
];

export default {
  extends: ["@commitlint/config-conventional"],
  rules: {
    "type-enum": [2, "always", TYPES],
    "scope-enum": [2, "always", SCOPES],
    "subject-empty": [2, "never"],
    "subject-case": [0],
    "body-max-line-length": [0],
    "footer-max-line-length": [0],
  },
};
