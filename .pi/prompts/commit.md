---
description: 在会话里把改动整理成一次符合 Conventional Commits 的提交
argument-hint: "[本次提交要强调的重点]"
---

把当前工作区的改动整理成一次符合 Conventional Commits 的提交。

type / scope 词表的唯一机器定义是仓库根的 [`commitlint.config.mjs`](../../commitlint.config.mjs)：**先读它**，不要凭记忆写 type 或 scope，也不要在本模板里另抄一份。规则说明见 `AGENTS.md` §8。

## 1. 先取证

- `git status --short`：区分已暂存、未暂存、未跟踪。
- `git diff --cached` 与 `git diff`：读完整改动；未跟踪文件按需读内容。
- `git log --oneline -10`：对齐仓库既有写法（中文主题、`type(scope): 主题`）。

## 2. 用对话确认意图，不要猜

- 主题用中文，说清「为什么」，不要复述 diff。
- 改动里包含多个互不相关的关注点时，先问用户这次提交哪一个；宁可拆成多次提交，也不要写一个含糊的 `chore: 杂项`。
- 仅凭 diff 判断不出动机时（格式化、开关取舍、行为回退等）直接问用户，不要编造理由。
- 用户给出的重点写进正文，或作为选 scope 的依据。

## 3. 提议提交信息并等待确认

给出候选 `<type>(<scope>): <中文主题>`（scope 可省略，用了就必须落在词表里），必要时附正文解释动机与影响；破坏性变更按 Conventional Commits 写 `!` 或 `BREAKING CHANGE:` 尾注。

**用户明确同意前不要执行 `git commit`。** 用户要求调整 type / scope / 措辞时，改完再确认一次。

## 4. 边界

- 词表里没有合适的 scope 时不要临时造一个：这是仓库新边界，需要**在同一次改动里**更新 `commitlint.config.mjs` 与 `AGENTS.md` §8，而不是绕过钩子。
- 只暂存本次提交涉及的显式路径（`git add -- <路径>`），不要用 `git add -A` 把无关改动一起带上。
- 不提交疑似密钥或令牌（`.env*`、`*.pem`、含 token 的配置）；发现时停下来问用户。
- 不要把 prompt 正文、密钥、认证载荷写进提交信息或日志。

## 5. 提交与校验

- 用普通 `git commit`（可用多条 `-m`），让 `.husky/commit-msg` 钩子照常执行。
- 禁止 `--no-verify`、`--no-gpg-sign`；除用户明确要求外不做 `--amend`，任何情况下都不 `git push`。
- 钩子拒绝时读它的报错，按 `commitlint.config.mjs` 修正信息后重试，不要绕过。
- 只想校验不提交时，把信息写到临时文件后运行 `npx --no -- commitlint --edit <文件>`。

## 6. 汇报

结束时报告：提交哈希、完整提交信息、涉及的文件、钩子是否通过、工作区还剩哪些未提交内容。
