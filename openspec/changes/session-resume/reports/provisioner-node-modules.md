# Provisioner Report（session-resume / tasks 1.3 续：worktree 依赖缓存隔离）

## Shared Report

| 字段 | 值 |
| --- | --- |
| task_id | `1.3`（按 plan.md 的 Runtime Resources 判定分配/隔离资源；本轮为已创建 worktree 补齐 PV2 所需的 `node_modules` 依赖缓存） |
| role | provisioner |
| phase | runtime |
| agent_context | 任务级 provisioner 子 Agent（delegated，`fork_turns=none` 等效：仅接收本任务模板 + Assignment/Target/Resources/Workspace/Output 最小输入，不继承编码讨论与实现者推理） |
| target_revision | `81e350ff340014265eb7c9251237c799d4357fee`（`git rev-parse HEAD`，与全部 8 个 worktree 的固定基线一致） |
| scope | 仅 worktree 内 `node_modules` 依赖缓存的挂接、就绪核实与释放方法记录；不含产品代码/测试脚本/配置修改，不含 PV1/PV2 判定，不含其他执行者实例的启停，不含提交/切换分支/推送 |
| changes | NOT_APPLICABLE（本角色不改产品代码、测试脚本或配置） |
| checks | NOT_APPLICABLE（本角色不判断证据是否充分，不运行 PV1/PV2 等检查） |
| issues | 无 FAIL / BLOCKED 问题；无越界操作。1 条 FAIL 尝试记录在 observations（shell 转义导致的命令失败，已用等价安全命令替代，不构成资源问题） |
| result | PASS（8/8 worktree 的依赖缓存已挂接并逐项核实可解析；未复制、未 `npm ci`、未改动被跟踪文件） |
| evidence_paths | `D:\Project\acp-remote\openspec\changes\session-resume\reports\provisioner-node-modules.md`（本报告）；worktree 与构建目录的交接收据见 `…\reports\provisioner-worktrees.md` |
| resource_cleanup | 本轮未清理：8 个 junction 处于「已分配、待执行者使用」状态。释放方法与 2 条破坏性操作禁令见下方 `resource_cleanup` |

## Target（已核实）

- 仓库（权威工作区）：`D:\Project\acp-remote`
- 基线提交：`81e350ff340014265eb7c9251237c799d4357fee`（`git rev-parse HEAD`）
- 8 个 worktree 的 `HEAD` 均为该基线（`git worktree list` 输出逐条为 `[agentic/session-resume-*]` / `[integration/session-resume-du1]`，sha 列同为 `81e350f`）
- 依赖源：`D:\Project\acp-remote\node_modules`（主仓库已由 `npm ci` 安装，154 条 `package-lock` 条目、105 个顶层包，`node -e "require('ajv')"` 输出 `main-ajv-ok`）

## 为什么需要这一步（资源判定）

- `node_modules` 未被 git 跟踪（`.gitignore:13:node_modules/`，`git check-ignore -v node_modules` 在 8/8 worktree 输出该行），因此 `git worktree add` 出来的目录内不存在该目录。
- `npm run check`（PV2 入口）的 10 道门禁全部是 Node 脚本，需要 `node_modules`：
  - `check:schemas` → `scripts/check-schema-fixtures.mjs`，裸依赖 `ajv-formats`、`ajv/dist/2020.js`（`grep` 出 `scripts/*.mjs` 的全部非 `node:` 裸导入只有这两个）；
  - `check:agentic` → `scripts/agentic-gate.mjs` 以**仓库根相对路径**启动 `node_modules/@dongfanglin/openspec-agentic/bin/openspec-agentic.mjs` 与 `node_modules/@fission-ai/openspec/bin/openspec.js`（`scripts/agentic-gate.mjs:34,39,50`），缺 `node_modules` 时该门禁无法执行。
- 选择 **目录联接（junction）** 而非复制或 `npm ci`：不产生第二份依赖树、不写入任何被跟踪文件、不消耗磁盘，且每个 worktree 内 `node_modules` 的解析结果与主仓库逐字节一致（同一份物理目录）。

## Runtime 判定（本轮新增资源）

| 资源 | 判定 | 本角色动作 | 就绪条件 | 结果 |
| --- | --- | --- | --- | --- |
| `node_modules` 依赖缓存 | 每个 worktree 独立路径、物理共享同一份只读使用 | 为 8 个 worktree 建立 `node_modules` → `D:\Project\acp-remote\node_modules` 的 junction | 每个 worktree 内 `node -e "require('ajv')"` 与只读门禁脚本可运行 | 8/8 PASS |
| Node 运行时 | 宿主全局，无隔离需求 | 核实版本满足 `check` 的 Node ≥ 22.12（README/AGENTS §10） | `node --version` ≥ 22.12 | `v24.19.0` PASS（`npm 12.0.2`） |
| Cargo registry（供 `check:boundaries` 的 `cargo metadata`/`cargo tree` 使用） | 宿主全局、本变更不分配 | 只核实存在性，不分配、不隔离 | `CARGO_HOME` 指向的 registry 已填充 | `CARGO_HOME=D:\Application\Rust\.cargo`，`registry/{cache,index,src}` 存在 PASS（未运行 cargo，仅存在性核实） |
| 端口 / 容器 / 数据库 / 账号 / 外部服务 | NOT_APPLICABLE | 本变更无此类资源 | — | — |
| 独占仲裁 | NOT_APPLICABLE | 依赖缓存为只读共享（不写），无不可隔离资源，无队列独占 | — | — |

## junction 挂接与就绪核实（逐目录）

执行命令模式（8 条逐一执行，全部 `exit=0`）：

```text
MSYS2_ARG_CONV_EXCL='/J' cmd //c mklink /J "<worktree 的 Windows 路径>\node_modules" "D:\Project\acp-remote\node_modules"
```

| WP | Worktree（绝对路径） | `node_modules` 状态 | `realpathSync.native` 目标 | `require('ajv')` | `scripts/check-doc-links.mjs` | `git status --porcelain` | 结果 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| WP1 | `D:\Project\acp-remote-wt\session-resume-wp1` | 本轮新建 junction（`mklink /J` exit=0） | `D:\Project\acp-remote\node_modules` | `ajv-ok 8.20.0` | `doc links OK: 398 relative links, 6747 section refs across 403 markdown files`（exit=0） | 0 行（干净） | PASS |
| WP2 | `…\session-resume-wp2` | 本轮新建 junction（exit=0） | 同上 | `ajv-ok 8.20.0` | 同上（exit=0） | 0 行 | PASS |
| WP3 | `…\session-resume-wp3` | 本轮新建 junction（exit=0） | 同上 | `ajv-ok 8.20.0` | 同上（exit=0） | 0 行 | PASS |
| WP4 | `…\session-resume-wp4` | 本轮新建 junction（exit=0） | 同上 | `ajv-ok 8.20.0` | 同上（exit=0） | 0 行 | PASS |
| WP5 | `…\session-resume-wp5` | 本轮新建 junction（exit=0） | 同上 | `ajv-ok 8.20.0` | 同上（exit=0） | 0 行 | PASS |
| WP6 | `…\session-resume-wp6` | 本轮新建 junction（exit=0） | 同上 | `ajv-ok 8.20.0` | 同上（exit=0） | 0 行 | PASS |
| TP1 | `…\session-resume-tp1` | 本轮新建 junction（exit=0） | 同上 | `ajv-ok 8.20.0` | 同上（exit=0） | 0 行 | PASS |
| U1 | `…\session-resume-du1` | 本轮新建 junction（exit=0） | 同上 | `ajv-ok 8.20.0` | 同上（exit=0） | 0 行 | PASS |

补充核实（8/8 worktree 全部执行）：

| 检查项 | 命令 | 结果 |
| --- | --- | --- |
| junction 未被 git 视为改动 | `git check-ignore -v node_modules` | 8/8 → `.gitignore:13:node_modules/` |
| `ajv/dist/2020.js` + `ajv-formats` 可解析（`check:schemas` 的实际裸依赖） | `node -e "require('ajv/dist/2020.js');require('ajv-formats')"` | 8/8 → `ajv2020+ajv-formats-ok 3.0.1` |
| `check:agentic` 需要的两个 CLI 入口可解析 | `fs.existsSync('node_modules/@dongfanglin/openspec-agentic/bin/openspec-agentic.mjs')` / `…/@fission-ai/openspec/bin/openspec.js` | 8/8 → 两个均 `EXISTS` |
| `npm run check` 链上 11 个脚本入口文件存在 | `[ -f <worktree>/scripts/<script> ]` | 11 个脚本 × 8 worktree 全部 present |
| `check:agentic` 的既有资产存在 | `openspec/agentic.yaml`、`openspec/config.yaml`、`openspec/.agentic-install.json`、`openspec/schemas/agentic`、`.agents/skills/agentic-verify/SKILL.md`、`compatibility/acp/v1/matrix.json` | 各 8/8 present（均为被跟踪文件，随 worktree 检出） |

未执行（越界）：不运行 `npm run check` / 各 `check:*` 子门禁的**判定**（启动成功与依赖可解析只表示资源就绪，不表示任何产品检查通过，也不替代 PV2）；不运行 `cargo metadata`/`cargo tree`/`cargo test`；不运行 `npm ci`/`npm install`；不提交、不推送、不切换分支、不修改被跟踪文件。

## observations

1. **依赖缓存必须存在，且已按 8/8 补齐**：worktree 里没有 `node_modules`（未跟踪），而 `npm run check` 的 `check:schemas` 与 `check:agentic` 硬依赖它；不补齐则每个 WP 都无法在自己的 worktree 内执行 PV2，只能退回主仓库运行——那会破坏「候选版本 + 独立 worktree」的隔离前提。本轮已消除该阻塞。
2. **为什么不用 `npm ci` / 复制**：`npm ci` 会在 8 个 worktree 各生成一份依赖树（磁盘与时耗 ×8），且需要网络；复制同理。junction 让 8 个 worktree 解析到同一份物理目录，`ajv` 版本（8.20.0）与主仓库完全一致，不存在版本漂移。
3. **一次命令失败（非资源问题）**：首轮用 `cmd //c mklink /J "D:\\…\\$d\\node_modules"` 时，本 shell（MINGW64 bash 5.3.15）未展开紧跟在 `\\` 后的 `$d`，cmd 报「参数格式不正确」；随后 `MSYS2_ARG_CONV_EXCL='*'` 又把 `//c` 一并排除导致 cmd 进入交互模式。最终用 `cygpath -w` 生成 Windows 路径 + 只对 `/J` 关闭 MSYS 参数转换的方式成功。原始输出保留在 `commands`。这是 shell 转义问题，与资源可用性无关，8/8 junction 已在最终方式下创建并核实。
4. **⚠ 共享可变风险（必须转达执行者与主 Agent）**：junction 是**读写共享**的。任何执行者在 worktree 内运行 `npm install` / `npm ci` / `npm update`（或任何写 `node_modules` 的命令）都会**穿透 junction 直接改写主仓库 `D:\Project\acp-remote\node_modules`**，从而同时影响其他 7 个 worktree 与主仓库、并可能造成版本漂移。本变更的 PV2 只要求 `npm run check`（只读脚本），因此该风险在正常流程下不会触发；但执行者**不得**在 worktree 内执行任何安装类命令。若某 WP 确需变更依赖，须由主 Agent 决策，不能在 worktree 内擅自执行。
5. **未做依赖完整性校验**：本轮只验证了脚本实际用到的裸依赖（`ajv`、`ajv-formats`）与两个 CLI 入口在 worktree 内可解析，未逐一枚举 154 条 `package-lock` 条目的存在性。依据是「同一物理目录 + 主仓库已 `npm ci`」——解析结果不可能比主仓库更差。如果某 WP 在 PV2 中报出 `MODULE_NOT_FOUND`，那说明主仓库依赖树本身缺项，应向主 Agent 报告，而不是在 worktree 内补装。
6. **未清理任何他人实例**：8 个 junction、8 个 worktree、8 个独立 `CARGO_TARGET_DIR` 均在分配状态；未触碰其他变更（`export-ids*`、`node-link-owner` 等）的 worktree 与资源。
7. **报告的权威目录约束满足**：报告写在主仓库 `D:\Project\acp-remote\openspec\changes\session-resume\reports\`，未写入任何 worktree 内（写入 worktree 会污染该 WP 的干净起点）。

## resource_cleanup

本轮**未执行清理**（8 个 junction 处于已分配、待执行者使用状态；按角色边界不清理他人尚未结束的实例）。

约定释放方法（单元交付完成后由本角色独占执行）：

1. 回收 worktree **之前，先用 `cmd //c rmdir "<worktree>\node_modules"` 断开 junction**（该命令只删除联接本身，不删除目标内容）。
2. 再执行既有回收流程：`git -C D:/Project/acp-remote worktree remove "D:/Project/acp-remote-wt/session-resume-<name>"`，随后按合入状态 `git branch -d`，并清空/删除 `D:\Project\acp-remote-target\session-resume-<name>`（方法承接 `provisioner-worktrees.md` 的 `resource_cleanup`）。
3. 回收后用 `git worktree list` 与 `test -e <path>` 核实条目消失，并记录到下一轮 provisioner 报告。

**两条破坏性操作禁令（必须遵守，否则会误删主仓库依赖树）**：

- **禁止**在 worktree 内对 `node_modules` 使用 `rm -rf`：MSYS/Git Bash 的 `rm -rf` 会把 junction 当普通目录递归删除，可能穿透删除 `D:\Project\acp-remote\node_modules` 的真实内容。断开联接只能用 `cmd //c rmdir`（或 PowerShell `Remove-Item -LiteralPath <link>`，不带 `-Recurse`）。
- **禁止**在 junction 仍在的 worktree 根目录整体 `rm -rf`；必须先断联接再按第 2 步用 git 命令回收。

已释放的外部资源：无（本变更无端口/容器/账号/外部服务分配；未启动任何服务）。

## commands（实际执行，含原始输出）

```text
# 0) 前置事实
cd /d/Project/acp-remote && git rev-parse HEAD
  -> 81e350ff340014265eb7c9251237c799d4357fee
cd /d/Project/acp-remote && git status --porcelain
  -> ?? openspec/changes/session-resume/
ls -d node_modules  -> node_modules            # 主仓库依赖已存在（MAIN_NM_OK）
node -e "require('ajv');console.log('main-ajv-ok')"  -> main-ajv-ok
for d in wp1..wp6,tp1,du1: [ -e D:/Project/acp-remote-wt/$d/node_modules ]
  -> 8/8 NO_NM                                # 挂接前：全部不存在
git worktree list
  -> D:/Project/acp-remote-wt/session-resume-du1     81e350f [integration/session-resume-du1]
     D:/Project/acp-remote-wt/session-resume-tp1     81e350f [agentic/session-resume-tp1]
     D:/Project/acp-remote-wt/session-resume-wp1..wp6 81e350f [agentic/session-resume-wp1..wp6]

# 1) 失败的尝试（保留原始输出；原因见 observations 第 3 条）
for d in …: cmd //c mklink /J "D:\\Project\\acp-remote-wt\\$d\\node_modules" "D:\\Project\\acp-remote\\node_modules"
  -> ������ʽ����ȷ - "D:\Project\acp-remote-wt$d\node_modules"。
     exit=1                                    # 8/8 均因 shell 未展开 $d 失败
d=session-resume-wp1; cmd //c mklink /J "$(cygpath -w /d/Project/acp-remote-wt/$d/node_modules)" "$(cygpath -w /d/Project/acp-remote/node_modules)"
  -> ������ʽ����ȷ - "D:\Project\acp-remote-wt\session-resume-wp1\node_modules"。
     exit=1                                    # 路径已正确，因 MSYS 把 /J 当路径转换而失败
MSYS2_ARG_CONV_EXCL='*' cmd //c mklink /J "<link>" "<target>"
  -> Microsoft Windows [版本 10.0.26200.9457] …  D:\Project\acp-remote>   exit=0
                                               # 过度排除转换，//c 未转成 /c，cmd 进入交互模式；未创建任何 link

# 2) 成功方式（8/8 逐条执行）
d=session-resume-wp1; link=$(cygpath -w "/d/Project/acp-remote-wt/$d/node_modules"); target=$(cygpath -w /d/Project/acp-remote/node_modules)
MSYS2_ARG_CONV_EXCL='/J' cmd //c mklink /J "$link" "$target"
  -> 为 D:\Project\acp-remote-wt\session-resume-wp1\node_modules <<===>> D:\Project\acp-remote\node_modules 创建的联接
     exit=0
for d in session-resume-wp2 … wp6, session-resume-tp1, session-resume-du1: 同一命令模式
  -> 为 D:\Project\acp-remote-wt\<d>\node_modules <<===>> D:\Project\acp-remote\node_modules 创建的联接
     exit=0   （7/7 全部成功）
ls -ld /d/Project/acp-remote-wt/session-resume-wp1/node_modules
  -> lrwxrwxrwx … node_modules -> /d/Project/acp-remote/node_modules

# 3) 就绪核实（8/8，逐目录）
for d in wp1..wp6,tp1,du1:
  (cd <wt> && node -e "const a=require('ajv');console.log('ajv-ok', require('ajv/package.json').version)")
    -> ajv-ok 8.20.0                          exit=0   ×8
  (cd <wt> && node scripts/check-doc-links.mjs)
    -> doc links OK: 398 relative links, 6747 section refs across 403 markdown files
       note: 3179 section refs 的归属文档由上下文决定（未在同一子句内指名文档），按设计未判定；设 DOC_LINKS_VERBOSE=1 可列出位置
       exit=0                                 ×8
  (cd <wt> && node -e "require('ajv/dist/2020.js');require('ajv-formats');console.log('ajv2020+ajv-formats-ok', require('ajv-formats/package.json').version)")
    -> ajv2020+ajv-formats-ok 3.0.1           exit=0   ×8
  git -C <wt> status --porcelain              -> 0 行   ×8
  git -C <wt> check-ignore -v node_modules    -> .gitignore:13:node_modules/  ×8
  node -e "console.log(fs.realpathSync.native('D:/Project/acp-remote-wt/<d>/node_modules'))"
    -> D:\Project\acp-remote\node_modules     ×8

# 4) 门禁入口依赖链预检（只读，不判定）
node -e "…package.json.scripts.check.split('&&')…"
  -> check:schemas  => node scripts/check-schema-fixtures.mjs
     check:commands => node scripts/check-command-catalog.mjs
     check:errors   => node scripts/check-error-registry.mjs
     check:features => node scripts/check-features.mjs
     check:assets   => node scripts/check-contract-assets.mjs
     check:acp      => node scripts/check-acp-compatibility.mjs
     check:docs     => node scripts/check-doc-links.mjs
     check:boundaries => node scripts/check-crate-boundaries.mjs
     check:drift    => node scripts/check-contract-drift.mjs
     check:agentic  => node scripts/agentic-gate.mjs && node scripts/sync-agentic-host-entrypoints.mjs --check
grep -rhoE "(from|import|require\()…" scripts/*.mjs | … | grep -v '^node:'
  -> ajv-formats
     ajv/dist/2020.js                          # scripts/*.mjs 的全部非 node: 裸导入
[ -f <wt>/<11 个脚本> ] ×8  -> 全部 present
(cd <wt> && node -e "fs.existsSync('node_modules/@dongfanglin/openspec-agentic/bin/openspec-agentic.mjs')…")
  -> agentic-cli EXISTS | openspec-cli EXISTS  ×8
[ -e <wt>/openspec/agentic.yaml ] 等 6 项 ×8 -> present 8/8

# 5) 环境事实
node --version  -> v24.19.0
npm  --version  -> 12.0.2
node -e "require('D:/Project/acp-remote/node_modules/.package-lock.json')"
  -> main package-lock entries: 154
echo "CARGO_HOME=${CARGO_HOME}"  -> CARGO_HOME=D:\Application\Rust\.cargo
ls -d D:/Application/Rust/.cargo/registry  -> CACHEDIR.TAG  cache  index  src
date -Iseconds  -> 2026-09-30T10:40:08+08:00
git -C /d/Project/acp-remote status --porcelain
  -> ?? openspec/changes/session-resume/     # 主仓库被跟踪文件零改动
```

## handoff_index

```yaml
handoff_index:
  - task_id: "1.3"
    work_package: NOT_APPLICABLE
    role: provisioner
    phase: runtime
    round: NOT_APPLICABLE
    stage: runtime
    target_revision: "81e350ff340014265eb7c9251237c799d4357fee"
    evidence_type: RESOURCE
    evidence_id: NOT_APPLICABLE
    report_path: "openspec/changes/session-resume/reports/provisioner-node-modules.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "8 个 worktree 的 HEAD 实测均为 81e350f…；junction 与就绪核实均在本轮、该基线的 worktree 内执行；资源为本轮新建，无可复用旧证据"
    source_evidence: NOT_APPLICABLE
```
