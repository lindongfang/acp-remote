# WP1 独立检视报告（Round 2）— `feat/sync-vocab` @ `0754d4c`

- 检视对象：`feat/sync-vocab`，工作树 `D:\Project\acp-remote-wt\wp1-vocab`
- HEAD：`0754d4cdcbb0af67d463b0a35ae17dbcf4f21695`（`fix(sync): 回填 §9.4 身份变更不变量并修正两处文档表述`）
- 被检视的前一提交：`ceedba8896c7d6a35df504a252e4c113d80c8308`
- 基线：`6c093f145aa3d69dcd573a6d94e31b692acb5d4b`
- 构建隔离：`CARGO_TARGET_DIR=D:\Project\acp-remote-wt\target-wp1-review2`
- 检视方式：全新独立复核。所有结论由我自己比对基线 blob 得到，不采信 Round-1 报告，也不采信修复提交的信息性正文。
- 工作树 `git status --porcelain` 输出为空（无未提交残留）。本文件是本次唯一写入。

---

## 结论

**PASS**

Round 1 的唯一 MAJOR（F1：`docs/SECURITY_DESIGN.md` §9.4 静默删除 `Node identity 变化进入 identity_changed，不能自动接受。`）已**真正修复**：该行在 HEAD 中存在，与基线 blob **逐字节相同**（UTF-8 hex 完全一致），且相对邻项位置未被破坏。修复提交 `0754d4c` 的改动范围恰好是三个文档文件、3 insertions / 2 deletions，未触及任何代码、schema、fixture 或 `crates/sync-protocol/tests/envelope_fixtures.rs`，且三个文件全部落在 WP1 声明的 Write Scope 内。

我对 `6c093f1..HEAD` 全部五个被改动的文档文件做了逐行删除枚举（19 行删除），逐条分类：**17 行 intended change，0 行 unauthorized deletion，2 行 header/日期元数据更新**。除 F1 外没有发现任何其它基线规范条目被削弱、改写、搬家或丢弃。

Round 1 的其余 MINOR（F2 计数常量、F3 DEVELOPMENT_PLAN 措辞、F4 尾随空格、F5 负例路径）状态：F3、F4 已修复且修复正确、无回归；F2 按任务要求保持分支内取值未动，已确认与 `ceedba8` 完全一致；F5 维持 Round-1 的判定（不构成发现）。

八条命令全部 exit 0。

---

## 一、MAJOR 的独立复核（F1）

### 1.1 该行存在且逐字节等同基线

我从基线 blob 与 HEAD blob 分别抽取全部 `identity_changed` 命中行并逐字节比较（未依赖文本 diff 的可读性）：

```
baseline hits(1-idx): [240, 454, 463]
HEAD hits(1-idx):     [242, 458, 467]

BASE 240 '- Node identity 变化进入 `identity_changed`，不能自动接受。'
      hex 2d204e6f6465206964656e7469747920e58f98e58c96e8bf9be585a520606964656e746974795f6368616e67656460efbc8ce4b88de883bde887aae58aa8e68ea5e58f97e38082
HEAD 242 '- Node identity 变化进入 `identity_changed`，不能自动接受。'
      hex 2d204e6f6465206964656e7469747920e58f98e58c96e8bf9be585a520606964656e746974795f6368616e67656460efbc8ce4b88de883bde887aae58aa8e68ea5e58f97e38082
```

结论：**存在，字节完全相同**（含全角句点、无尾随空格差异）。另外两处命中（审计动作取值表 `:458` 与决定记录 `:467`）同样与基线逐字节一致。

### 1.2 位置与邻项一致

基线 §9.4 条目序列（`6c093f1:docs/SECURITY_DESIGN.md:233-242`）：

```
233 ### 9.4 配对和认证
235 - 精确状态、HMAC、SAS…见 IDENTITY_AND_AUTH_CONTRACT.md。
236 - Device/Node pairing 只能由目标节点本地管理入口创建和确认。
237 - 配对 UI 必须显示设备名称、key fingerprint、SAS、请求 scopes 和过期时间。
238 - 用户确认前设备记录不能获得 active 权限。
239 - 每个新 WSS 完整认证，不签发长期 bearer session/refresh token。
240 - Node identity 变化进入 `identity_changed`，不能自动接受。
242 ### 9.5 撤销与轮换
```

HEAD §9.4（`docs/SECURITY_DESIGN.md:234-244`）：

```
234 ### 9.4 配对和认证
236 - 精确状态、HMAC、SAS…（未变）
237 - Device/Node pairing…（未变）
238 - 配对 UI…（未变）
239 - 用户确认前…（未变）
240 - 配对授予的 scope 是**设备级**的…（本变更新增，位于 239 与 241 之间）
241 - 每个新 WSS 完整认证，不签发长期 bearer session/refresh token。
242 - Node identity 变化进入 `identity_changed`，不能自动接受。
244 ### 9.5 撤销与轮换
```

即：唯一变化是新增的第 240 行；被修复行仍紧跟在「每个新 WSS 完整认证…」之后、仍是 §9.4 的最后一条、仍与 §9.5 之间隔一空行。**相对邻项位置与基线完全一致**，未被移动或降级为子条目。

### 1.3 `git diff` 层面确认

`git diff 6c093f1..HEAD -- docs/SECURITY_DESIGN.md` 的 hunk 中，`identity_changed` 行**不再以 `-` 出现**；§9.4 的 hunk 只有一条 `+`（新增的设备级 scope 规则）。修复提交 `0754d4c` 对该文件的改动是纯 `1 insertion`，无删除。

---

## 二、系统性文档删除分类（`6c093f1..HEAD`）

我先用 `git diff --name-only` 确认整个范围内被改动的 `.md` 文件集合恰为 5 个：
`README.md`、`docs/DEVELOPMENT_PLAN.md`、`docs/FRONTEND_DESIGN.md`、`docs/SECURITY_DESIGN.md`、`docs/SYNC_PROTOCOL.md`。
再以 `--unified=0` 逐文件抽取全部 `-` 行，共 **19 行**，逐条判定如下。

| # | 文件:行 | 被删除的基线内容（摘要） | 分类 | 依据 |
|---|---|---|---|---|
| 1 | `README.md:15` | `sync-protocol` crate 描述行（原为「…33 个事件视图的类型化投影，以及配对 HTTPS 载荷」） | **intended** | 同行替换版在同一句内追加「12 条 Sync 命令的封闭词表（含 `session.create` 的 `{ workspaceAlias, agentId }` payload 与 `completed { sessionId, session }` 终态结果）」；原有全部能力描述逐项保留，仅插入。事实核对：schema `commandName` 枚举实测 12 项、Rust `CommandName::ALL: [CommandName; 12]`。 |
| 2 | `docs/DEVELOPMENT_PLAN.md:4` | `> 基线：2026-09-23（…2026-10-01 随 session-resume…）` | **intended** | 头部元数据行，仅在括号内追加「，2026-10-02 随 `sync-workspaces-and-create` 补充切片 7 的 Sync 合同新增 `session.create` 与本机 workspace/Agent 目录」，其余文字逐字保留。 |
| 3 | `docs/DEVELOPMENT_PLAN.md:62` | 验收行「…**首版不提供会话创建入口**；Owner 离线时…」 | **intended** | 这是本变更要改写的目标表述本身（design D5 / FRONTEND §9 第 1 条）。替换行同时保留「重连从已 ACK cursor 恢复且不重复显示或执行命令」「Owner 离线时只显示可用元数据，不把输入误报为已发送」，并新增目录浏览与 `session.create` 终态验收。无其它验收条目被丢弃。 |
| 4 | `docs/FRONTEND_DESIGN.md:4` | `> 版本：0.3` | **intended** | 版本号提升为 `0.4`，并新增同位置修订记录行。 |
| 5 | `docs/FRONTEND_DESIGN.md:6` | `> 日期：2026-09-18` | **intended** | 改为 `> 日期：2026-10-02（2026-09-18 基线）`，保留基线日期。 |
| 6 | `docs/FRONTEND_DESIGN.md:40`（§2.2） | 「当前 PWA v1 不展示会话创建；Node Link 已支持的 `session.create` 只能选择 Owner 导出的 Agent 与 workspace template，未来 PWA 只增加该受限入口。」 | **intended** | 本变更要改写的产品边界句。替换行保留 Node Link 侧仍额外要求 Export template 的事实，并把 PWA 侧改为受限目录内入口。 |
| 7 | `docs/FRONTEND_DESIGN.md:282`（§9 第 1 条） | 「只能查看和操作当前节点可见的已有会话；PWA v1 无法通过 UI 或构造普通命令创建会话。」 | **intended** | 同上，验收条目的目标改写。替换行仍保留「只能查看和操作当前节点可见的已有会话」并新增服务端必须拒绝、离线可渲染等约束。 |
| 8 | `docs/SECURITY_DESIGN.md:4` | `> 版本：0.3` | **intended** | 版本号提升为 `0.4` + 新增修订记录行。 |
| 9 | `docs/SECURITY_DESIGN.md:5` | `> 日期：2026-09-18  `（含两个尾随空格） | **intended** | 改为 `> 日期：2026-10-02  `，**尾随两个空格被保留**，与该文件头部块引号的硬换行约定一致。 |
| 10 | `docs/SECURITY_DESIGN.md:286`（§10.2 表行） | `| session.create | mutation | session.create | 无（仅 Node Link） | grant.remote-work | mvp（Node Link）/ Sync 首版不暴露 |` | **intended** | 表格单元格改写为 `pack.create-session` + 「Node Link 与 Sync 共用命令名，payload 不同」。`grant` 列（`grant.remote-work`）未变。 |
| 11 | `docs/SECURITY_DESIGN.md:290`（§10.2 条目） | 「`session.create` 是设备/Access principal 的 scope，同时要求 Owner 侧 `grant.remote-work`；请求只能引用 Export 中发布的 Agent 与 workspace template，不能提交任意 Owner 路径或 Provider/MCP 凭据。Node Link 首个纵向切片必须实现它以支持 Zed `session/new`；Sync 首版不暴露该入口。」 | **intended** | 改写为双面说明。我逐句核对保留情况：① 「是设备/Access principal 的 scope，同时要求 Owner 侧 `grant.remote-work`」→ 保留；② 「不能提交任意 Owner 路径或 Provider/MCP 凭据」→ 强化为 Sync 面「`exportId`、`templateParams`、`cwd`、绝对路径、MCP 配置与凭据一律不被接受」；③ 「Node Link 首个纵向切片必须实现它以支持 Zed `session/new`」→ 逐字保留；④ 「请求只能引用 Export 中发布的 Agent 与 workspace template」→ 在 Node Link 面保留（「Node Link 侧仍额外要求 `exportId` 与 Export 发布的 workspace template」）。仅「Sync 首版不暴露该入口」被替换为本变更要求的新语义。**无净削弱。** |
| 12 | `docs/SECURITY_DESIGN.md:310`（§10.3） | 「远程 `session.create` 是唯一与 workspace 相关的受限远程能力，且只能引用 Export 发布的 alias/template。未来改变这条边界属于产品与安全边界变更，需要更新本文及 `INITIAL_DESIGN.md`，不能只增加一个 command schema。」 | **intended** | 前半句按面拆分（Node Link 面 Export / Sync 面本机登记引用），后半句「未来改变这条边界属于产品与安全边界变更，需要更新本文及 `INITIAL_DESIGN.md`，不能只增加一个 command schema。」**逐字保留**。 |
| 13 | `docs/SYNC_PROTOCOL.md:6` | `> 日期：2026-09-18` | **intended** | 头部日期行，改为含基线的合并写法。 |
| 14 | `docs/SYNC_PROTOCOL.md:187` | 「本表的**五个** feature 构成 v1 baseline…」 | **intended** | 改为「**七个**」。实测：`features.json` 的 sync 段共 7 条且全部 `delivery: mvp`，文档 §5.2 表同步新增 2 行。**计数正确。** |
| 15 | `docs/SYNC_PROTOCOL.md:1202`（§11.3 未定义清单中的 `session.create`） | 单独一行 `session.create` | **intended** | 从「Sync v1 尚未定义，或仅允许 Node 本地管理入口」清单中移出，同一 hunk 内已在可授予清单（`:1200`）加入。清单其余条目（`session.delete`、`session.resume`、`local.*` 共 8 项）未变。 |
| 16 | `docs/SYNC_PROTOCOL.md:1213`（§11.3 正文） | 「`session.create` 是 Node Link 命令，首阶段 Sync 不暴露；将来经 Sync 暴露时必须同时满足 `grant.remote-work`，且只能引用 Export 发布的 agent 与 workspace template。…」 | **intended** | 改写为双层授权。**`grant.remote-work` 的双层约束被保留**（替换行：「设备侧必须持 scope `session.create`…Owner 侧必须满足 `grant.remote-work`」）。「只能引用 Export 发布的 agent 与 workspace template」在 Sync 面被替换为「只接受本机已登记 workspace 的别名与已配置的 Agent 标识…没有 Export 概念」，这是本变更的核心语义，属设计内。 |
| 17 | `docs/SYNC_PROTOCOL.md:1266`（§11.5 表行） | `| session.create | mutation | 禁止 | { agentId, exportId, workspaceAlias, templateParams?: object } | 仅 Node Link…；Sync v1 收到返回 command.unsupported |` | **intended** | Sync 面形状改写为 `{ workspaceAlias, agentId }` + 终态/失败分类。与 schema 一致。 |
| 18 | `docs/SYNC_PROTOCOL.md:1269`（§11.5 表后说明） | 「…`command` 枚举只包含其中 `transport` 含 `sync` 的 **11** 条；`session.create` 与 `session.resume` 只经 Node Link 接受…不出现在本文件的枚举里。」 | **intended** | 11→12，`session.create` 从「只经 Node Link」清单移除。该句末尾「任一处增删命令名都必须同步修改 `commands.json`、两个协议 schema、`SECURITY_DESIGN.md` 第 10.2 节与本节」**逐字保留**。 |
| 19 | `docs/SYNC_PROTOCOL.md:1486`（§16.2） | 「增加 `session.create`、imported resource origin…等能力需要新 feature/schema；授权由 scope 和 Owner Export Policy 决定，不能再按手机/电脑形态硬编码。」 | **intended** | 原句**逐字保留**为前缀，仅追加「`session.create` 与本机目录已按此条以**可选 feature** 引入…不需要 major version」的说明。 |

**分类统计：19 行删除 = 17 行 intended change + 2 行头部日期/版本元数据行（计入 intended）+ 0 行 unauthorized deletion。**

> Round-1 报告的 F1（`identity_changed`）在本次枚举中**不再出现**，因为它已在 HEAD 中恢复；`git diff 6c093f1..HEAD` 不再输出该行。这也交叉验证了 §1 的字节级比对。

### 2.1 非文档文件的删除行同样已清点

为避免「只查文档」留下盲区，我也枚举了范围内非 `.md` 改动文件的全部删除行，共 **17 行**，全部为 intended：

- `compatibility/commands/v1/commands.json`（2）：`session.create` 行（`pack: null`→`pack.create-session`、`transport` 加 `sync`）与 `pack.approve` 行末补逗号。
- `crates/identity-auth/tests/authorization.rs`（2）：两处注释更新 + 1 个新测试（纯新增）。
- `crates/node-link-protocol/src/command.rs`（2）：仅文件顶部 `//!` 模块注释。
- `crates/sync-protocol/src/command.rs`（7）：全部为「11 条→12 条」的注释/数组长度/`is_query` 匹配臂更新。
- `crates/sync-protocol/tests/envelope_fixtures.rs`（2）：见 §四。
- `schemas/sync/v1/command.schema.json`（2）：`commandName.description` 与枚举末项补逗号。

**无一处删除触及 `crates/core/src/broker.rs`、`compatibility/errors/`、`schemas/node-link/` 或任何既有安全规则。**

---

## 三、修复提交 `0754d4c` 的范围审查

`git show --name-only --format="" 0754d4c`：

```
docs/DEVELOPMENT_PLAN.md
docs/FRONTEND_DESIGN.md
docs/SECURITY_DESIGN.md
```

`git diff ceedba8..HEAD --stat`：

```
 docs/DEVELOPMENT_PLAN.md |  2 +-
 docs/FRONTEND_DESIGN.md  |  2 +-
 docs/SECURITY_DESIGN.md |  1 +
 3 files changed, 3 insertions(+), 2 deletions(-)
```

逐项核对：

- **无代码**：`crates/**` 零改动。✔
- **无 schema**：`schemas/**` 零改动。✔
- **无 fixture**：`fixtures/**` 零改动。✔
- **无 `envelope_fixtures.rs`「顺手修正」**：该文件 `git diff ceedba8..HEAD` 为空。✔（见 §四）
- **无 `broker.rs` / `compatibility/errors/`** 触碰：零改动。✔
- **三个文件均在 WP1 声明 Write Scope 内**（`docs/DEVELOPMENT_PLAN.md`、`docs/FRONTEND_DESIGN.md`、`docs/SECURITY_DESIGN.md` 三项均在该列表中）。✔

### 3.1 F3 的修复：`docs/DEVELOPMENT_PLAN.md:62` 措辞**属实**（我独立核实）

修复后的句子：「命令目录与 `schemas/sync/v1/command.schema.json`（node-link schema 无 `pack` 约束、无需变更）、`sync-protocol` 的 `CommandName`/payload/result、授权镜像的 `pack.create-session` 与固定向量都已就位」。

我独立核实「node-link schema 无 `pack` 约束」这一断言：
- 在 `schemas/node-link/v1/command.schema.json` 中检索 `pack`，**零命中**（grep 无 matches）。
- `git diff 6c093f1..HEAD --stat -- schemas/node-link/` 输出为**空** → node-link schema 零改动，与「无需变更」一致。
- 该 schema 中 `session.create` 仍以 `const` 出现在 4 处（`:754` body、`:859`/`:1052` 结果分支、`:61` 枚举），即 node-link 面形状未被破坏，与 `crates/node-link-protocol/src/command.rs` 仅改注释相符。
- `node scripts/check-command-catalog.mjs` exit 0，该脚本对 `transport` 含 `node_link` 的命令集与 node-link schema 枚举做集合相等断言（`scripts/check-command-catalog.mjs:225` 附近），故「无需变更」也通过机器校验。

**结论：F3 的修复不仅消除了误导，而且其新表述经我独立核实为真。未引入新的不准确。**

### 3.2 F4 的修复：`docs/FRONTEND_DESIGN.md:6` 是**纯空白层恢复**

我用基线 blob 与 HEAD blob 逐行 repr 比对头部块：

```
BASELINE:
 3 '> 状态：编码前客户端约束  '
 4 '> 版本：0.3'
 5 '> 修订记录（2026-09-18）：明确 PWA 本地缓存为固定常量…imported 正文不占用配额。  '
 6 '> 日期：2026-09-18'

HEAD:
 3 '> 状态：编码前客户端约束  '
 4 '> 版本：0.4'
 5 '> 修订记录（2026-10-02）：…  '          (新增)
 6 '> 修订记录（2026-09-18）：明确 PWA 本地缓存为固定常量…imported 正文不占用配额。  '
 7 '> 日期：2026-10-02（2026-09-18 基线）'
```

- `> 修订记录（2026-09-18）：…` 一行与基线**逐字节相同**（长度均为 80，含行末两个空格）。
- 该文件头部块的既有约定是「除块末尾行外，每个块引号行以两个尾随空格作 Markdown 硬换行」（基线第 3、5、7 行均如此，末行第 8 行无）；HEAD 的第 3、5、6、8 行均带两个尾随空格，**与基线约定完全一致**。
- `git show 0754d4c -- docs/FRONTEND_DESIGN.md` 的 hunk 只有一行：把 `。` 后缺失的两个空格补回。**没有内容编辑**。

**结论：F4 已正确修复，且是该文件 Markdown 硬换行约定下的最小、正确的恢复。**

### 3.3 修复是否引入回归

- `node scripts/check-doc-links.mjs` exit 0（本次 section refs 由 Round-1 的 8678 增至 8697、markdown 文件由 485 增至 486，增量来自 Round-1 报告文件本身进入仓库，不影响判定）。
- 修复后的 `SECURITY_DESIGN.md` §9.4 顺序经复核无语义断裂（§1.2）。
- 未新增任何错误码引用、任何跨文档交叉引用的失效。

---

## 四、`crates/sync-protocol/tests/envelope_fixtures.rs` 计数常量

按任务要求专门确认。`git diff ceedba8..HEAD -- crates/sync-protocol/tests/envelope_fixtures.rs` **输出为空** → 修复提交未触碰该文件。

三个修订的常量声明对照：

```
            6c093f1 (baseline)   ceedba8 (WP1)   HEAD
:25 VALID   60                   64               64
:26 ENVELOPE 2                    2                2
:27 BODY     5                    7                7
:29 PAIRING  5                    5                5
```

**结论：两个常量保持分支内取值 64 / 7 未动，修复者没有试图「修正」它们。** 相对基线的 60→64 / 5→7 变更仍只来自 `ceedba8`（WP1 新增 6 条 manifest case 所致的派生更新）。集成时的合并取值仍应由集成方按重算决定（Round-1 独立重算给出的 67 / 9 未被本轮改动影响）。

---

## 五、Round-1 PASS 判据在 HEAD 上的复核

| # | 判据 | 结论 | 本轮独立证据 |
|---|---|---|---|
| 1 | 命令目录 ≡ sync schema 枚举 ≡ SYNC_PROTOCOL §11.5 ≡ SECURITY_DESIGN §10.2 ≡ `broker::required_grant`（未动） | **通过** | `commands.json`：`session.create` → `transport: ["sync","node_link"]`、`pack: "pack.create-session"`、`grant: "grant.remote-work"`；`schemas/sync/v1/command.schema.json` 的 `commandName.enum` 实测 **12** 项含 `session.create`；`CommandName::ALL: [CommandName; 12]` 含 `CommandName::SessionCreate`。`git diff 6c093f1..HEAD -- crates/core/src/broker.rs` 为**空**（零漂移）。`node scripts/check-command-catalog.mjs` → `command catalog OK: 13 commands`，exit 0；该脚本在 `:245`/`:250`/`:253-262` 分别对 §11.5、§10.2 与 `broker.rs::required_grant` 做集合相等断言。 |
| 2 | `pack.create-session` 成员恰为 `session.create`，且不进任何 preset | **通过** | `authorization.rs:38` `("pack.create-session", &["session.create"])`；`PRESETS`（`:42-53`）仍只有 `preset.remote-control`（observe/interact/configure-session/approve）与 `preset.read-only`（observe）；`commands.json` 的 `packs` 新增该包，`presets` 两项未变。新测试 `create_session_pack_expands_to_session_create_and_stays_out_of_presets` 逐 preset 断言不展开出 `session.create`，实跑 ok。 |
| 3 | 两个新 feature ID 三方一致 | **通过** | `features.json` sync 段新增 `core.local-catalog.v1`、`core.session-create.v1`（均 `mvp`/`required:false`）；`docs/SYNC_PROTOCOL.md:187-188` §5.2 表新增对应两行且计数句 5→7；`fixtures/sync/v1/valid/auth-client-hello-session-create.json` 的 `supportedFeatures` 同时列出两者。`node scripts/check-features.mjs` → `feature registry OK: 13 feature ids across 2 protocols`，exit 0。实测 features.json sync 段 7 条全为 mvp，与「七个 feature」一致。 |
| 4 | 未新增错误码 | **通过** | `git diff 6c093f1..HEAD --stat -- compatibility/errors/` 为**空**；`node scripts/check-error-registry.mjs` → `error registry OK: 58 codes across 2 protocols`，exit 0。文档新引用的 `authorization.scope_denied`、`protocol.schema_invalid`、`internal.unavailable`、`protocol.feature_required` 及 `uncertain`（对应既有 `command.uncertain`，`errors.json:29`）均为既有码。 |
| 5 | Sync payload 只接受 `workspaceAlias`/`agentId`，未知键被拒 | **通过** | Schema `sessionCreate`：`payload` 为 `additionalProperties: false` + `required: ["workspaceAlias","agentId"]`，两字段 `minLength:1/maxLength:128`，body 层同样 `additionalProperties:false` 且不含 `sessionId`；`command` body `oneOf` 已加入该 def；`completed → result: sessionCreateResult{sessionId, session: SessionSummary}` 的条件分支已加入。Rust：`SessionCreate`（`command.rs:268`）带 `#[serde(deny_unknown_fields)]`，`CommandName::SessionCreate` 在 `ALL`/`as_str`/`from_str`/`is_query`(`false`)/`requires_session_id`(`false`)/`requires_expected_version`(`false`)/`from_raw` 双臂中全部显式出现。 |
| 6 | 设备级副作用句在 `SECURITY_DESIGN.md` §9 | **通过** | `:240`：「配对授予的 scope 是**设备级**的…**一次授予覆盖该节点当时及此后新增的全部已登记 workspace 与已配置 Agent，新登记的资源自动进入已授权范围，无需重新授权**。配对确认页必须原样展示这句副作用…撤销该 scope 或撤销设备立即使后续命令被拒（见 §10.2）。」；§10.2 `:293` 重复并加严。 |
| 7 | `FRONTEND_DESIGN.md` §9 第 1 条不再禁止 PWA 会话创建 | **通过** | `:286` 第 1 条已改为受限入口（只能在已登记目录内创建、payload 只有两个引用、未持 scope 时不可用且服务端也必须拒绝、离线可渲染），不再含「无法…创建会话」的禁令。README 引用的 §2.2/§4.1/§7/§9 四个锚点在文中均存在（`:35`、`:102`、`:229`、`:282`）。 |
| 8 | 无凭据泄漏进新增产物 | **通过** | 6 个新增 fixture 对 `token\|secret\|password\|api[-_]?key\|bearer\|credential` 检索**零命中**；`command-result-session-create-completed.json` 全文只含别名 `claude-code` 与 UUID，无规范化路径。唯一含路径字面量的负例 `invalid/command-session-create-with-cwd.json` 为 spec 强制要求的 `cwd` 被拒向量，值是合成串（维持 Round-1 的 F5 判定）。 |

---

## 六、八条命令的真实输出

工作目录 `D:\Project\acp-remote-wt\wp1-vocab`，`CARGO_TARGET_DIR=D:\Project\acp-remote-wt\target-wp1-review2`。

### 1) `node scripts/check-command-catalog.mjs`
```
command catalog OK: 13 commands
exit=0
```

### 2) `node scripts/check-features.mjs`
```
feature registry OK: 13 feature ids across 2 protocols
exit=0
```

### 3) `node scripts/check-schema-fixtures.mjs`
```
schema fixtures OK: 124 valid, 28 invalid (ajv Draft 2020-12), 39 event views bound
exit=0
```

### 4) `node scripts/check-contract-assets.mjs`
```
contract assets OK: 17 schemas, 165 fixture files, 12 transcript vectors re-encoded from input, 20 negative vectors rejected as declared, 2 SAS values recomputed
exit=0
```

### 5) `node scripts/check-error-registry.mjs`
```
error registry OK: 58 codes across 2 protocols
exit=0
```

### 6) `node scripts/check-doc-links.mjs`
```
doc links OK: 407 relative links, 8697 section refs across 486 markdown files
note: 4176 section refs 的归属文档由上下文决定（未在同一子句内指名文档），按设计未判定；设 DOC_LINKS_VERBOSE=1 可列出位置
exit=0
```

### 7) `cargo test --locked -p sync-protocol -p identity-auth`
```
   Running unittests src\lib.rs (…identity_auth-04a43de0e8efac3c.exe)   test result: ok. 0 passed; 0 failed
   Running tests\authorization.rs (…authorization-cdbe3b892a80c815.exe)  test result: ok. 16 passed; 0 failed
   Running tests\handshake.rs (…handshake-caf8580f85751818.exe)         test result: ok. 20 passed; 0 failed
   Running tests\pairing.rs (…pairing-2f089… pairing-2e089f59038f4a7.exe) test result: ok. 30 passed; 0 failed
   Running tests\parity.rs (…parity-437fc2b703be4e8.exe)               test result: ok. 2 passed; 0 failed
   Running tests\ports.rs (…ports-8e31cbc4b086ce47.exe)                 test result: ok. 9 passed; 0 failed
   Running tests\transcripts.rs (…transcripts-edce904b883cb8d68.exe)    test result: ok. 5 passed; 0 failed
   Running unittests src\lib.rs (…sync_protocol-e32ab710be3c308a.exe)  test result: ok. 4 passed; 0 failed
   Running tests\body_constraints.rs (…body_constraints-fc25a850acec5582.exe) test result: ok. 9 passed; 0 failed
   Running tests\envelope_fixtures.rs (…envelope_fixtures-6a60562edcca5582.exe) test result: ok. 7 passed; 0 failed
   Running tests\field_constraints.rs (…field_constraints-4001aed38f659b6e.exe) test result: ok. 10 passed; 0 failed
   Running tests\pairing_fixtures.rs (…pairing_fixtures-60b2bd8e69259b80.exe) test result: ok. 6 passed; 0 failed
   Running tests\schema_drift.rs (…schema_drift-ffeb29f9ba1f273.exe)     test result: ok. 5 passed; 0 failed
   Running tests\tables_match_registry.rs (…tables_match_registry-41a311c50ea70d82.exe) test result: ok. 3 passed; 0 failed
   Running tests\transcript_vectors.rs (…transcript_vectors-e7f11210b749eb5c.exe) test result: ok. 2 passed; 0 failed
   Running tests\view_projections.rs (…view_projections-44b8dd04b0393606.exe) test result: ok. 3 passed; 0 failed
   Doc-tests identity_auth                                             test result: ok. 2 passed; 0 failed
   Doc-tests sync_protocol                                             test result: ok. 0 passed; 0 failed
finished in 16.29s
```
零失败、零失败断言。`envelope_fixtures` 目标 7 passed —— 即 64/2/7/5 的分支内取值与实际 manifest 分类自洽（不是被改成 67/9 后的失败态）。

### 8) `cargo clippy --locked -p sync-protocol -p node-link-protocol -p identity-auth --all-targets --all-features -- -D warnings`
```
    Checking acpr-wire v0.0.0 (…\crates\acpr-wire)
    Checking sync-protocol v0.0.0
    Checking node-link-protocol v0.0.0
    Checking core v0.0.0
    Checking identity-auth v0.0.0
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 5.72s
```
`-D warnings` 下零诊断。

---

## 七、`git diff 6c093f1..HEAD --stat`

```
 README.md                                          |   4 +-
 compatibility/commands/v1/commands.json            |   5 +-
 compatibility/features/v1/features.json            |  12 ++
 crates/identity-auth/src/authorization.rs          |   5 +
 crates/identity-auth/tests/authorization.rs        |  37 +++-
 crates/node-link-protocol/src/command.rs           |   6 +-
 crates/sync-protocol/src/command.rs                | 188 ++++++++++++++++++++-
 crates/sync-protocol/tests/envelope_fixtures.rs    |   4 +-
 docs/DEVELOPMENT_PLAN.md                           |   6 +-
 docs/FRONTEND_DESIGN.md                            |  12 +-
 docs/SECURITY_DESIGN.md                            |  14 +-
 docs/SYNC_PROTOCOL.md                              |  22 ++-
 .../invalid/command-session-create-with-cwd.json   |  16 ++
 .../command-session-create-with-session-id.json    |  16 ++
 fixtures/sync/v1/manifest.json                     |  32 ++++
 .../v1/valid/auth-client-hello-session-create.json |  23 +++
 .../command-result-session-create-accepted.json    |  16 ++
 .../command-result-session-create-completed.json   |  29 +++
 fixtures/sync/v1/valid/command-session-create.json |  15 ++
 schemas/sync/v1/command.schema.json                |  42 ++++-
 20 files changed, 468 insertions(+), 36 deletions(-)
```

与 Round-1 的 `+469 / −38` 相比变为 `+468 / −36`：差额恰为 F1 回填（−1 删除、+0）与 F4 尾随空格行（−1 删除、+0 行数变化），即修复只把两处删除变回插入/恢复，未引入新内容行。全部 20 个文件均在 WP1 声明 Write Scope 内，唯一的越界项 `crates/sync-protocol/tests/envelope_fixtures.rs` 维持 Round-1 的 F2 判定（派生性、最小、不阻塞）。

完整文档 diff 见 §二表格逐行分类；命令输出见 §六。

---

## 八、发现

### 无 MAJOR

### 无新增 MINOR / BLOCKER

### 沿用自 Round-1、状态已更新的条目

**F1 — MAJOR — 已修复（`docs/SECURITY_DESIGN.md:242`）**
问题：曾删除基线不变量 `- Node identity 变化进入 identity_changed，不能自动接受。`。
现状：该行在 HEAD 存在、与基线 blob 逐字节相同、相对邻项位置与基线一致（§一）。修复有效。
最小修复：**无需进一步动作**。

**F2 — MINOR — 维持现状（`crates/sync-protocol/tests/envelope_fixtures.rs:25,27`）**
现状：常量 64 / 7 与 `ceedba8` 逐字节一致，修复提交未触碰（§四）。分支内自洽，`cargo test -p sync-protocol` 的 `envelope_fixtures` 目标 7 passed。
影响：集成时该文件仍是 WP1/WP2 的冲突点，合并取值须由集成方重算决定。
最小修复：本工作树**无需修改**；建议集成提交统一处理。

**F3 — MINOR — 已修复且表述属实（`docs/DEVELOPMENT_PLAN.md:62`）**
现状：措辞已改为点明只有 `schemas/sync/v1/command.schema.json` 变更；我核实 node-link schema 无 `pack` 约束、零 diff，断言为真（§3.1）。

**F4 — MINOR — 已修复且为纯空白层恢复（`docs/FRONTEND_DESIGN.md:6`）**
现状：行与基线逐字节相同，符合本文件硬换行约定；无内容编辑（§3.2）。

**F5 — MINOR（维持 Round-1 判定，不构成发现）— `fixtures/sync/v1/invalid/command-session-create-with-cwd.json:13`**
现状：该负例含合成绝对路径字面量，是 spec 强制要求的 `cwd` 被拒向量，不构成路径泄漏。无修改需求。

### 补充观察（非发现，供参考）

- 本工作包新增的 6 个 fixture 仍以无尾随换行结尾（实测 `tail -c 1` 返回 `}`，而既有 fixture 返回 `\n`）。与 Round-1 观察一致，任何门禁都不检查该属性，两条 fixture 门禁均通过。建议在集成提交里与 WP2 的新 fixture 一并统一，避免在相邻 manifest 区域制造额外 diff。
- `compatibility/commands/v1/commands.json` 与 `features.json` 的 `revision` 字段仍为 `2026-09-18`，本工作包未上调。与仓库先例一致（`db129c2` 同样未上调），且无门禁要求。不作为发现。

---

## 九、验收清单

| 验收项 | 状态 |
|---|---|
| 明确结论（PASS / FAIL / BLOCKED） | **PASS** |
| 发现含 ID / 严重度 / file:line / 问题 / 影响 / 最小修复 | ✔（§八；MAJOR 已修复，无新增发现；沿用条目已更新状态） |
| `6c093f1..HEAD` 全部文档删除行的完整分类（intended vs unauthorized） | ✔ 19 行逐条，0 unauthorized（§二） |
| `envelope_fixtures.rs` 常量与 `ceedba8` 一致的确认 | ✔ 空 diff，64 / 7 未动（§四） |
| 八条命令的真实输出 | ✔ 全部 exit 0（§六） |
| diffstat | ✔（§七） |