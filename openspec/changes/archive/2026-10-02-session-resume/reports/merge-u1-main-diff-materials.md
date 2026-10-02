### git diff --stat 81e350ff340014265eb7c9251237c799d4357fee..0d2be6d (merge commit)
 .github/workflows/ci.yml                           |    3 +-
 AGENTS.md                                          |    2 +-
 Cargo.lock                                         |    1 +
 README.md                                          |    4 +-
 compatibility/acp/v1/matrix.json                   |    4 +-
 compatibility/commands/v1/commands.json            |    5 +-
 crates/acp-protocol/src/message.rs                 |  220 +++
 crates/acp-protocol/src/methods.rs                 |    6 +-
 crates/acp-protocol/tests/session_resume.rs        |  233 +++
 crates/agent-host/src/bin/acpr-fake-acp-agent.rs   |  285 +++-
 crates/agent-host/src/host.rs                      |  209 ++-
 crates/agent-host/src/session.rs                   |   22 +-
 crates/agent-host/tests/resume.rs                  |  684 +++++++++
 crates/agent-host/tests/session.rs                 |  394 ++++-
 crates/app/Cargo.toml                              |    1 +
 crates/app/tests/session_resume_e2e.rs             | 1617 ++++++++++++++++++++
 crates/app/tests/support/owner.rs                  |  181 ++-
 crates/core/src/broker.rs                          |  593 ++++++-
 crates/core/src/model/backend.rs                   |   57 +-
 crates/core/src/model/config.rs                    |    2 +-
 crates/core/src/model/error.rs                     |   12 +-
 crates/core/src/model/ids.rs                       |   20 +
 crates/core/src/model/session.rs                   |    5 +-
 crates/core/src/model/tests.rs                     |   81 +-
 crates/core/src/ports.rs                           |   53 +-
 crates/core/src/use_cases.rs                       |  423 ++++-
 crates/identity-auth/src/authorization.rs          |    4 +-
 crates/identity-auth/tests/authorization.rs        |    5 +-
 crates/node-link-protocol/src/command.rs           |  201 ++-
 crates/node-link-protocol/src/common.rs            |   13 +
 .../node-link-protocol/tests/envelope_fixtures.rs  |    4 +-
 .../tests/session_resume_command.rs                |  229 +++
 crates/server/src/local_admin/test_support.rs      |   23 +
 crates/server/src/node_link/command.rs             |  415 ++++-
 crates/server/src/node_link/command/tests.rs       |  626 +++++++-
 crates/server/src/node_link/conn/tests.rs          |    4 +
 crates/server/src/node_link/resource/tests.rs      |    7 +
 crates/storage-sqlite/src/migrate.rs               |   42 +-
 crates/storage-sqlite/src/session_store.rs         |   66 +-
 crates/storage-sqlite/tests/commit.rs              |   14 +
 crates/storage-sqlite/tests/contract_v03.rs        |    2 +
 crates/storage-sqlite/tests/enum_coverage.rs       |    2 +
 crates/storage-sqlite/tests/migration.rs           |  454 +++++-
 crates/storage-sqlite/tests/resume_columns.rs      |  598 ++++++++
 crates/storage-sqlite/tests/retention.rs           |    4 +
 .../storage-sqlite/tests/session_version_rule.rs   |    6 +
 docs/ACP_COMPATIBILITY_MATRIX.md                   |   11 +-
 docs/CORE_PORTS_AND_STORAGE.md                     |   51 +-
 docs/DEVELOPMENT_PLAN.md                           |    4 +-
 docs/MODULE_ARCHITECTURE.md                        |    7 +-
 docs/NODE_LINK_PROTOCOL.md                         |   26 +-
 docs/SECURITY_DESIGN.md                            |    2 +
 docs/SESSION_CONTINUITY_DESIGN.md                  |   28 +-
 docs/SYNC_PROTOCOL.md                              |    3 +-
 fixtures/node-link/v1/README.md                    |    4 +
 .../v1/invalid/session-resume-with-cwd.json        |   22 +
 fixtures/node-link/v1/manifest.json                |   16 +
 .../v1/valid/command-submit-session-resume.json    |   20 +
 .../command-terminal-session-resume-completed.json |   28 +
 openspec/changes/session-resume/.openspec.yaml     |    2 +
 openspec/changes/session-resume/design.md          |  189 +++
 .../changes/session-resume/dispatch-queue.jsonl    |   36 +
 openspec/changes/session-resume/plan.md            |  477 ++++++
 openspec/changes/session-resume/proposal.md        |   76 +
 .../reports/cr-c1-candidate-review.md              |  146 ++
 .../changes/session-resume/reports/cr1-review.md   |   84 +
 .../changes/session-resume/reports/cr2-review.md   |   76 +
 .../changes/session-resume/reports/cr3-review.md   |  119 ++
 .../changes/session-resume/reports/cr4-review.md   |   60 +
 .../session-resume/reports/cr5-review-round2.md    |  207 +++
 .../changes/session-resume/reports/cr5-review.md   |  100 ++
 .../session-resume/reports/cr6-review-round2.md    |  125 ++
 .../changes/session-resume/reports/cr6-review.md   |   74 +
 .../changes/session-resume/reports/cr7-review.md   |   86 ++
 .../session-resume/reports/cr8-review-round2.md    |  156 ++
 .../changes/session-resume/reports/cr8-review.md   |  235 +++
 .../reports/dr1-dependency-review-round10.md       |  202 +++
 .../reports/dr1-dependency-review-round11.md       |  251 +++
 .../reports/dr1-dependency-review-round12.md       |  237 +++
 .../reports/dr1-dependency-review-round13.md       |  224 +++
 .../reports/dr1-dependency-review-round14.md       |  157 ++
 .../reports/dr1-dependency-review-round15.md       |  133 ++
 .../reports/dr1-dependency-review-round16.md       |  186 +++
 .../reports/dr1-dependency-review-round17.md       |   71 +
 .../reports/dr1-dependency-review-round18.md       |  103 ++
 .../reports/dr1-dependency-review-round2.md        |  121 ++
 .../reports/dr1-dependency-review-round3.md        |  123 ++
 .../reports/dr1-dependency-review-round4.md        |  158 ++
 .../reports/dr1-dependency-review-round5.md        |  140 ++
 .../reports/dr1-dependency-review-round6.md        |  143 ++
 .../reports/dr1-dependency-review-round7.md        |  162 ++
 .../reports/dr1-dependency-review-round8.md        |  205 +++
 .../reports/dr1-dependency-review-round9.md        |  204 +++
 .../reports/dr1-dependency-review.md               |  114 ++
 .../session-resume/reports/merge-u1-candidate.md   |  650 ++++++++
 .../reports/merge-u1-integrate-wp3.md              |  395 +++++
 .../reports/merge-u1-integrate-wp4.md              |  534 +++++++
 .../reports/merge-u1-integrate-wp5.md              |  547 +++++++
 .../reports/merge-u1-integrate-wp6.md              |  343 +++++
 .../session-resume/reports/merge-u1-integrate.md   |  308 ++++
 .../session-resume/reports/premerge-receipt-u1.md  |   33 +
 .../reports/provisioner-node-modules.md            |  207 +++
 .../reports/provisioner-repoint-tp2.md             |  615 ++++++++
 .../reports/provisioner-repoint-wp3.md             |  369 +++++
 .../reports/provisioner-repoint-wp4-wp5.md         |  428 ++++++
 .../reports/provisioner-repoint-wp6-and-tp2.md     |  541 +++++++
 .../reports/provisioner-worktrees.md               |  276 ++++
 .../changes/session-resume/reports/scout-target.md |  179 +++
 .../session-resume/reports/tp1-test-design.md      |  613 ++++++++
 .../session-resume/reports/tp2-test-design.md      |  268 ++++
 .../changes/session-resume/reports/tp2-tester.md   | 1055 +++++++++++++
 .../changes/session-resume/reports/wp1-coder.md    |  186 +++
 .../changes/session-resume/reports/wp2-coder.md    |  152 ++
 .../changes/session-resume/reports/wp3-coder.md    |  369 +++++
 .../changes/session-resume/reports/wp4-coder.md    |  242 +++
 .../session-resume/reports/wp5-coder-fix-cr5f1.md  |  195 +++
 .../changes/session-resume/reports/wp5-coder.md    |  277 ++++
 .../reports/wp5-fix-diff-4e53fcf-to-1376e1b.txt    |    3 +
 .../changes/session-resume/reports/wp6-coder-f3.md |  224 +++
 .../changes/session-resume/reports/wp6-coder.md    |  296 ++++
 .../session-resume/specs/acp-wire-protocol/spec.md |   24 +
 .../session-resume/specs/local-agent-host/spec.md  |   43 +
 .../specs/node-link-owner-server/spec.md           |   62 +
 .../specs/storage-schema-v2-migration/spec.md      |   53 +
 .../specs/workspace-resolution/spec.md             |   24 +
 openspec/changes/session-resume/tasks.md           |   78 +
 openspec/changes/session-resume/verification.md    |  397 +++++
 schemas/node-link/v1/command.schema.json           |   75 +-
 schemas/node-link/v1/common.schema.json            |   10 +
 scripts/check-command-catalog.mjs                  |    5 +-
 130 files changed, 23103 insertions(+), 213 deletions(-)

### git log --oneline 81e350ff340014265eb7c9251237c799d4357fee..0d2be6d
0d2be6d feat(session-resume): 支持会话恢复的 Owner 侧路由与跨节点恢复
2ed142d chore(repo): 集成 TP2（会话恢复测试补强与受控路径恢复用例）到 U1 集成基线
3484541 docs(test): 订正 R25 覆盖分档与判别式注释
f44301e test(app): 补 R25 规范化结果变化的两族变体并修正 R31 观察点
a02e2fd test(session-resume): 补 wire 层命令契约用例与格式修正
f8133f2 test(session-resume): 补受控路径恢复用例与恢复观察点
f04a989 test(session-resume): 补协议/存储/后端三层恢复用例
95051f9 chore(repo): 集成 WP6（node-link owner session.resume 路由与恢复路径收口）到 U1 集成基线
a7bc596 fix(node-link): 收紧 session.resume 的幂等比对并补登记
76ab011 feat(node-link): 路由 session.resume 并收口恢复路径的编译涟漪
5ab7e9d chore(repo): 集成 WP5（agent-host 会话恢复后端路径与 ACP 会话标识暴露）到 U1 集成基线
1376e1b fix(agent-host): 为 fake ACP child 增加 --dump-request-params 选项
b486c53 chore(repo): 集成 WP4（storage-sqlite v5 恢复列与 load_recovery 窄读取）到 U1 集成基线
4a3882d feat(storage): v5 migration 追加恢复列并实现 load_recovery 窄读取
4e53fcf feat(agent-host): 落地会话恢复的后端路径与 ACP 会话标识暴露
8a08db8 chore(repo): 集成 WP3（core session.resume 恢复语义）到 U1 集成基线
4f7a235 docs(core): 修正模块架构 §4.1 的端口方法名写法
3e41f90 feat(core): 落地 session.resume 的恢复用例与终态入口
aac0356 feat(core): 落地会话恢复的端口、值对象与两列落盘提交点
b0a387b chore(repo): 集成 WP2（node-link session.resume 与封闭词表原子点）到 U1 集成基线
e8feaf9 chore(repo): 集成 WP1（acp-protocol session/resume DTO）到 U1 集成基线
32f71f5 feat(node-link): 加入 session.resume 命令与封闭词表原子点
248d9b9 feat(acp): 新增 session/resume 类型化 DTO 并提升矩阵交付节奏

### git diff --stat 2ed142dedec2facf8f6e174d1aa165f549cbc47e 0d2be6d  (expect: only openspec assets, 68 files)
 68 files changed, 15168 insertions(+)

### git diff --stat 2ed142dedec2facf8f6e174d1aa165f549cbc47e 0d2be6d -- . ':(exclude)openspec/changes/session-resume'  (expect: EMPTY = product tree identical)
[EMPTY-ABOVE-CONFIRMS-IDENTICAL]

### git diff --stat 0d2be6d..69f1ac1 (follow-up doc fix)
 openspec/changes/session-resume/reports/merge-u1-integrate-wp4.md | 2 +-
 1 file changed, 1 insertion(+), 1 deletion(-)

---

# 第 2 轮复核（merger-A6，目标版本 `69f1ac1bc6af81461d199257f512965f646ec55c`）

第 1 轮（merger-A5）的素材在目标 `0d2be6d` 上采集。本轮在修正提交 `69f1ac1` 之后**重新逐条复核**，结论与第 1 轮一致，并补充了完整输出。下方命令均在 `D:/Project/acp-remote` 执行。

## A. `git diff --stat 81e350ff340014265eb7c9251237c799d4357fee..HEAD`

```text
$ git diff --shortstat 81e350ff340014265eb7c9251237c799d4357fee..HEAD
 130 files changed, 23103 insertions(+), 213 deletions(-)
```

构成拆分（`git diff --name-only 81e350f..HEAD` 共 130 项）：

| 类别 | 数量 | 说明 |
| --- | --- | --- |
| 产品 / 合同 / 文档 | **62** | `docs/**` 8、`crates/storage-sqlite/tests` 7、`crates/core/src/model` 6、`crates/core/src` 3、`crates/storage-sqlite/src` 2、`crates/node-link-protocol/src` 2、`crates/node-link-protocol/tests` 2、`crates/agent-host/src` 2、`crates/agent-host/tests` 2、`crates/acp-protocol/src` 2、`crates/acp-protocol/tests` 1、`crates/agent-host/src/bin` 1、`crates/identity-auth/src` 1、`crates/identity-auth/tests` 1、`crates/server/src/node_link{,/command,/conn,/resource}` 各 1、`crates/server/src/local_admin` 1、`crates/app/{,tests,tests/support}` 各 1、`schemas/node-link/v1` 2、`fixtures/node-link/v1` 3、`compatibility/commands/v1` 1、`compatibility/acp/v1` 1、`scripts/check-command-catalog.mjs` 1、`AGENTS.md` 1、`README.md` 1、`.github/workflows/ci.yml` 1、`Cargo.lock` 1 |
| 规划与证据资产 | **68** | 全部在 `openspec/changes/session-resume/**`（proposal / design / plan / tasks / verification / 5 份增量 specs / dispatch 台账 / 角色报告） |

拆分依据：`git diff --name-only 81e350f..HEAD | grep -v '^openspec/changes/session-resume/' | sed 's#/[^/]*$##' | sort | uniq -c | sort -rn`，且 `git diff --name-only 81e350f..HEAD | grep -cv '^openspec/changes/session-resume/'` = **62**。

## B. `git log --oneline 81e350ff340014265eb7c9251237c799d4357fee..HEAD`

```text
$ git rev-list --count 81e350ff340014265eb7c9251237c799d4357fee..HEAD
24
```

```text
69f1ac1 docs(repo): 修正 WP4 集成报告中对评审报告的节引用
0d2be6d feat(session-resume): 支持会话恢复的 Owner 侧路由与跨节点恢复
2ed142d chore(repo): 集成 TP2（会话恢复测试补强与受控路径恢复用例）到 U1 集成基线
3484541 docs(test): 订正 R25 覆盖分档与判别式注释
f44301e test(app): 补 R25 规范化结果变化的两族变体并修正 R31 观察点
a02e2fd test(session-resume): 补 wire 层命令契约用例与格式修正
f8133f2 test(session-resume): 补受控路径恢复用例与恢复观察点
f04a989 test(session-resume): 补协议/存储/后端三层恢复用例
95051f9 chore(repo): 集成 WP6（node-link owner session.resume 路由与恢复路径收口）到 U1 集成基线
a7bc596 fix(node-link): 收紧 session.resume 的幂等比对并补登记
76ab011 feat(node-link): 路由 session.resume 并收口恢复路径的编译涟漪
5ab7e9d chore(repo): 集成 WP5（agent-host 会话恢复后端路径与 ACP 会话标识暴露）到 U1 集成基线
1376e1b fix(agent-host): 为 fake ACP child 增加 --dump-request-params 选项
b486c53 chore(repo): 集成 WP4（storage-sqlite v5 恢复列与 load_recovery 窄读取）到 U1 集成基线
4a3882d feat(storage): v5 migration 追加恢复列并实现 load_recovery 窄读取
4e53fcf feat(agent-host): 落地会话恢复的后端路径与 ACP 会话标识暴露
8a08db8 chore(repo): 集成 WP3（core session.resume 恢复语义）到 U1 集成基线
4f7a235 docs(core): 修正模块架构 §4.1 的端口方法名写法
3e41f90 feat(core): 落地 session.resume 的恢复用例与终态入口
aac0356 feat(core): 落地会话恢复的端口、值对象与两列落盘提交点
b0a387b chore(repo): 集成 WP2（node-link session.resume 与封闭词表原子点）到 U1 集成基线
e8feaf9 chore(repo): 集成 WP1（acp-protocol session/resume DTO）到 U1 集成基线
32f71f5 feat(node-link): 加入 session.resume 命令与封闭词表原子点
248d9b9 feat(acp): 新增 session/resume 类型化 DTO 并提升矩阵交付节奏
```

构成：WP1–WP6 的 6 个交付提交 + 6 个 `chore(repo): 集成 …` 基线点 + TP2 的 4 个测试/文档提交 + 1 个 `chore(repo): 集成 TP2 …` 基线点 + 合并提交 `0d2be6d` + 修正提交 `69f1ac1` = **24**。

## C. `git show --stat 69f1ac1`

```text
commit 69f1ac1bc6af81461d199257f512965f646ec55c
Author: lindongfang <18202756749@163.com>
Date:   Thu Oct 1 21:11:59 2026 +0800

    docs(repo): 修正 WP4 集成报告中对评审报告的节引用

    cr4-review.md 实际只有 Review / Review Context / Findings / Assessment / handoff_index，没有 §9；原句的 §9 既非该报告的节，也与 wp4-coder.md 的 §9（未执行项）指向不同内容。改为引用两处真实小节，使 check:docs 能解析。

 openspec/changes/session-resume/reports/merge-u1-integrate-wp4.md | 2 +-
 1 file changed, 1 insertion(+), 1 deletion(-)
```

## D. 复核：合并没有引入任何新的产品代码差异

```text
$ git diff --stat 2ed142dedec2facf8f6e174d1aa165f549cbc47e 0d2be6d403551dc75cb7662f5a515dbc22fb2bd3 -- . ':(exclude)openspec/changes/session-resume'
[空输出]

$ git diff --stat 2ed142dedec2facf8f6e174d1aa165f549cbc47e HEAD -- . ':(exclude)openspec/changes/session-resume'
[空输出]

$ git diff --shortstat 2ed142dedec2facf8f6e174d1aa165f549cbc47e 0d2be6d403551dc75cb7662f5a515dbc22fb2bd3
 68 files changed, 15168 insertions(+)

$ git diff --name-only 2ed142dedec2facf8f6e174d1aa165f549cbc47e 0d2be6d403551dc75cb7662f5a515dbc22fb2bd3 | grep -cv '^openspec/changes/session-resume/'
0
```

**明确回答：没有。** 三条机械证据：

1. 把右端固定为合并提交 `0d2be6d`、并排除变更目录 → **空输出**。
2. 把右端换成当前 `HEAD`（含修正提交 `69f1ac1`）→ **仍然空输出**，即修正提交同样没有触碰任何产品面文件。
3. 候选与合并树之间确实有 68 个文件、15168 行的差异，但 `grep -cv` = **0**，即这 68 个文件 **100% 落在 `openspec/changes/session-resume/**`**，属于**随本次提交入库的规划/证据资产**（不是产品代码差异）。

补充：合并是零冲突、零内容改写的结构性合入（父提交对为 `81e350ff` + `2ed142d`），`--no-ff` 只为保留历史而多了一个合并节点；`69f1ac1` 是 1 行 markdown 修正。⇒ **产品代码 / 合同资产 / 文档 / 脚本 / CI 配置在候选与主分支之间逐字节相同。**
