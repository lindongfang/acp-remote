// 提交信息 type/scope 的唯一机器定义；新模块落地时同步维护。
export const TYPES = ["feat", "fix", "docs", "style", "refactor", "perf", "test", "build", "ci", "chore", "revert"];
export const SCOPES = ["repo", "docs", "ci", "deps", "release", "scripts"];
export default {
  extends: ["@commitlint/config-conventional"],
  rules: {
    "type-enum": [2, "always", TYPES],
    "scope-enum": [2, "always", SCOPES],
    "subject-empty": [2, "never"],
    "subject-case": [0],
    "body-max-line-length": [0],
    "footer-max-line-length": [0]
  },
  ignores: [(message) => /^merge(?:\(|:)/i.test(message)]
};
