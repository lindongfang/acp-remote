# ACP Remote

本地优先的 ACP（Agent Client Protocol）中转站：一个节点可以直接管理本地 Agent，也可以通过 Node Link 导入其他节点导出的 Agent；手机、电脑、Zed、PWA 与 CLI 都是客户端形态，不是固定权限角色。

## 仓库当前状态

**设计层完成，实现已开始。** 仓库当前包含权威文档、机器可读合同（JSON Schema）、跨语言固定向量（fixtures）、兼容性矩阵与校验脚本，以及一个随增量增长的 Rust workspace（`docs/MODULE_ARCHITECTURE.md` §3.1）：

| crate | 已实现范围 |
|---|---|
| `acpr-transcript` | 长度前缀 transcript codec 与表驱动校验（叶子 crate，无协议语义） |
| `acpr-wire` | 跨协议共用的 wire 值对象与字段校验机制（叶子 crate，无协议词表） |
| `core` | `core::model` 的值对象与不变量、`core::use_cases` 用例面、`core::ports` 端口签名、`core::broker`（每会话串行、active turn、幂等、交互仲裁、先提交后发布、失败关闭）；运行时依赖只有 `async-trait`/`thiserror`/`p256`/`sha2`（后两者用于 `PeerPublicKey` 的构造期点校验与指纹派生） |
| `storage-sqlite` | §7 的 `owned_*`/`imported_*` 表结构与 migration、保留窗口/容量清理（含 imported 家族度量）、附件内容寻址、崩溃恢复与只读失败关闭；实现 `SessionStore`/`ReadView`/`RemoteDeliveryStore`/`AttachmentStore`，以及 §7 管理表上的三个管理 store（`TrustStore`/`ExportStore`/`LocalConfigStore`：写集一事务提交、失败关闭、容量纳入） |
| `sync-protocol` | v1 的全部 18 个消息类型（信封与消息类型分派，`auth`/`sync`/`control`/`error`/`event`/`command` 六个家族 body）、33 个事件视图的类型化投影，以及配对 HTTPS 载荷（二维码 / claim / status / HTTP 错误体） |
| `node-link-protocol` | §9.3/§9.4 的 transcript domain/tag 表、v1 的全部 29 个消息类型（信封与分派，`handshake`/`catalog`/`resource`/`command`/`error` 五个家族 body）与配对 HTTPS 载荷 |
| `acp-protocol` | JSON-RPC 信封分类与方向/required 校验、ACP v1 wire DTO（`initialize`/`session/new`/`session/prompt`/`session/update` 的 11 种判别子/`session/request_permission`/elicitation 与 content block）、`RawDocument` 原文承载与逐字节回写、capability wire 形状、固定 v1 消息上限（1 MiB），以及由 `fixtures/acp/v1/manifest.json` 与 `compatibility/acp/v1/matrix.json` 驱动的契约测试 |
| `agent-host` | 本机 ACP 子进程作为 `AgentCatalog`/`SessionBackendFactory`/`SessionEndpoint`：启动与监督、stdio 分帧、request id 与 ACP session id 映射、capability 协商与调用门控、权限/elicitation 转发、超时与取消、stderr 有界采集、Windows Job Object（Unix 进程组）进程树清理、profile 与凭据注入边界、wire→core mapper |

尚未开始：前端工程，以及 `server`、`node-link-client`、`identity-auth`、`identity-keystore`、`app`（每落地一个才加入 workspace `members`）。

当前优先交付 Windows x64 的 Daemon/CLI 与 Node Link 闭环，Linux 延后开发；完整平台顺序见 [初始设计 §14](docs/INITIAL_DESIGN.md#14-npm-分发)。共享代码的 Linux CI 保留，不代表 Linux 产品已可运行。

逐切片的实施顺序与验收节点见 [开发计划](docs/DEVELOPMENT_PLAN.md)；产品与协议语义仍以各权威合同为准。

管理状态的端口签名、写集 DTO、值对象与 DDL 已并入 [核心与存储合同 §3/§5/§7](docs/CORE_PORTS_AND_STORAGE.md#11-管理状态持久化合同形状已并入-357)（该节 §11 保留设计理由与索引），并由 `scripts/check-contract-drift.mjs` 逐条断言；`identity-auth` 的内部状态机、握手入口、授权展开与 keystore 端口冻结在 [身份与认证合同](docs/IDENTITY_AND_AUTH_CONTRACT.md)；本地通道的 ACP 流会话语义与管理载荷的机器表达见 [本地管理通道](docs/LOCAL_ADMIN_PROTOCOL.md) §3.1 与 [`schemas/local-admin/v1/`](schemas/local-admin/v1/)。SQLite 侧的 `TrustStore`/`ExportStore`/`LocalConfigStore` **落盘实现已落地**（写集一事务提交、失败关闭与容量纳入）；仍未实现的是 Daemon/CLI 接线与 `identity-auth`/`identity-keystore`，因此在它们完成前，不能声称配对、撤销或本地配置已经端到端可用。

## 权威文档

修改代码或设计前先读 `AGENTS.md`，它列出各文档的职责边界与维护规则。主要入口：

| 文档 | 内容 |
|---|---|
| [docs/INITIAL_DESIGN.md](docs/INITIAL_DESIGN.md) | 产品目标、系统行为、阶段计划 |
| [docs/MODULE_ARCHITECTURE.md](docs/MODULE_ARCHITECTURE.md) | crate / 模块职责与依赖方向 |
| [docs/SYNC_PROTOCOL.md](docs/SYNC_PROTOCOL.md) | 客户端同步协议（认证、cursor、命令、事件） |
| [docs/NODE_LINK_PROTOCOL.md](docs/NODE_LINK_PROTOCOL.md) | 节点间资源导出/导入协议 |
| [docs/LOCAL_ADMIN_PROTOCOL.md](docs/LOCAL_ADMIN_PROTOCOL.md) | CLI 与 Daemon 之间的本地管理通道 |
| [docs/SECURITY_DESIGN.md](docs/SECURITY_DESIGN.md) | 威胁模型、信任边界与安全验收 |
| [docs/ACP_COMPATIBILITY_MATRIX.md](docs/ACP_COMPATIBILITY_MATRIX.md) | ACP v1 覆盖范围与各层策略 |
| [docs/FRONTEND_DESIGN.md](docs/FRONTEND_DESIGN.md) | 前端阶段、状态模型与平台边界 |
| [docs/IDENTITY_AND_AUTH_CONTRACT.md](docs/IDENTITY_AND_AUTH_CONTRACT.md) | 身份与认证：配对状态机、握手入口、授权展开与 keystore 端口（实现前合同） |
| [docs/CONFIG_REFERENCE.md](docs/CONFIG_REFERENCE.md) | Daemon 配置键、类型与默认值 |
| [docs/adr/](docs/adr/) | 已接受的架构决策 |

## 合同检查

合同门禁只依赖 Node ≥ 22.12（`commitlint` 21 的下限）与仓库内的 devDependencies，不参与运行时产物；
Rust 侧的编译器版本以 [`rust-toolchain.toml`](rust-toolchain.toml) 为唯一来源（本地 rustup 与 CI 读同一个文件，
因此不会有「本机绿、CI 红」的 clippy / rustfmt 版本假失败）：

```text
npm ci          # 首次或依赖变动后
npm run verify  # 合同门禁 + fmt / clippy / test，与 CI 的 checks job 同源
```

`npm run check` 串行执行十项检查：① schema 与 fixture（ajv, Draft 2020-12，含消息类型与事件视图的覆盖门禁；本地管理通道的 envelope 是第四棵资产树，但它没有 `message.schema.json`，因此不参与消息类型覆盖门禁）；② 命令目录与本仓库封闭词表的一致性——`commands.json`、两个协议 schema、SYNC §11.5 与 SECURITY §10.2 的表格、`core::broker::required_grant` 这份 Rust 镜像，以及本地管理的方法集与错误码（`LOCAL_ADMIN_PROTOCOL.md` 的方法小节与 §6 表格 ↔ `schemas/local-admin/v1/envelope.schema.json`；`local.*` 能力 ↔ `commands.json` 的 `localCapabilities`）；③ 错误码 registry；④ feature ID 词表（registry、两份协议文档与 fixture 三方一致）；⑤ 需要真正计算的资产绑定（`$ref` 与 `$id`、`rawJson` 字节与摘要、事件 `payloadDigest` 的 ACPR-CJ1 重算、transcript 固定向量重编码与畸形输入负向量）；⑥ ACP 兼容矩阵与 vendored 上游快照；⑦ 文档引用门禁（`check:docs` → `scripts/check-doc-links.mjs`：相对链接的目标文件存在、`#anchor` 命中目标文档的标题或显式锚点、指名了文档的 `§X.Y` 引用能在该文档解析；引用归属刻意保守，无法归因的只统计不判定）；⑧ crate 依赖方向门禁（`MODULE_ARCHITECTURE.md` §5 的矩阵，外加 `core` 依赖闭包的冻结 allow-list）；⑨ 合同漂移门禁（§7 的表结构 ↔ `crates/storage-sqlite/src/migrate.rs`、§5 的端口 ↔ `crates/core/src/ports.rs`）；⑩ agentic 流程与规范（`check:agentic` → `scripts/agentic-gate.mjs`：`openspec-agentic doctor` 断言所用流程确为扩展的 agentic —— 引擎版本等于扩展 pin、`schema: agentic`、受管文件无漂移、AGENTS.md 有验收路由；随后 `openspec validate --all --strict` 校验变更与规范，无活动变更时以 0 退出。该脚本同时设置 `OPENSPEC_TELEMETRY=0`、`OPENSPEC_NO_UPDATE_CHECK=1`、`DO_NOT_TRACK=1`，关闭引擎默认开启的遥测与更新检查）。

CI（[`.github/workflows/ci.yml`](.github/workflows/ci.yml)）在 push、PR 与每日定时任务上跑五个 job：`checks`（`npm run check` + `npm run check:rust`，与本地 `npm run verify` 同源）、`commits`（提交范围规范）、`deps`（`cargo-deny` 的 bans/licenses/sources 与 `npm audit`）、`advisories`（`cargo-deny` 的 advisory 判定）、`secrets`（`gitleaks` 密钥扫描：push/PR 扫本次范围，每日定时任务扫全历史）。Linux runner 会真正执行 `#[cfg(unix)]` 的权限路径（`0700`/`0600`/模式位判定），这些在 Windows 开发机上不会跑到。

`deps` / `advisories` / `secrets` 需要网络或额外二进制，**没有包含在 `npm run verify` 里**（依赖判决见 [`deny.toml`](deny.toml)，密钥扫描规则见 [`.gitleaks.toml`](.gitleaks.toml)）；工具版本、许可证、向外发送的数据与已知残余风险见 [ADR-0008](docs/adr/0008-ci-supply-chain-tooling.md)。依赖更新由 [`.github/dependabot.yml`](.github/dependabot.yml) 提出：升版前有冷却期（避免第一时间采用刚发布的版本），minor/patch 分组、major 单独提交，且分组 PR 同样要过全部 job。

提交信息遵循 Conventional Commits（`<type>(<scope>)!?: <主题>`）：type 与 scope 词表以 [`commitlint.config.mjs`](commitlint.config.mjs) 为唯一机器定义，本地由 husky 的 `.husky/commit-msg` 钩子在 `npm install` 时装配，CI 的独立 `commits` job 会对本次推送/合并请求引入的提交范围再校验一次（`npm run lint:commits -- --from <base> --to <head>`，`--no-verify` 绕得过本地钩子但绕不过它）。规则说明见 `AGENTS.md` §8。

上游 ACP 固定快照（`schemas/acp/v1/upstream/schema.json`，来源与 sha256 见 `compatibility/acp/v1/matrix.json` 的 `protocol` 块）由 `check:acp` 重算 digest 并校验 commit 与 major 版本目录；`fixtures/acp/v1` 也按同一快照做 ajv 校验。升级快照必须同时改固定值、vendored 文件与矩阵行，且先通过 `node scripts/check-acp-compatibility.mjs`。

## 分支保护

CI 的判定只有在分支保护要求它时才真的能拦住合并：[`.github/workflows/ci.yml`](.github/workflows/ci.yml) 定义检查，「合并前必须通过」却是 GitHub 的仓库设置，不属于版本控制内容。

**必需检查的 context 必须是 check-runs 上报的名字，不是 job id**——这一点实测过：填 job id（`checks`、`deps`……）时
`rulesets/<id>` 会原样存下，但没有任何 check 叫这个名字，规则就变成「永久等待」，非绕过 actor 的 PR 永远合不进去
（写错时自己有 bypass 感觉不到，属于安静的地雷）。正确名字从权威接口取：

```text
gh api repos/<owner>/<repo>/commits/<sha>/check-runs --jq '.check_runs[].name' | sort -u
```

本仓库的五个是：**合同门禁 + Rust 检查**、**提交信息规范**、**依赖许可证与来源**、**密钥扫描**、
**依赖安全公告**（最后一个是否必需按 [ADR-0008](docs/adr/0008-ci-supply-chain-tooling.md) 决策 4 判断：
它的失败可能来自与本次改动无关的上游 advisory）。

设置入口是仓库 Settings → Rules/Branches（`gh api -X PUT repos/<owner>/<repo>/branches/main/protection` 也适用，但 payload 形状取决于要开哪几条，建议先用界面）。核实当前状态与可用性：

```text
gh auth status
gh api repos/lindongfang/acp-remote --jq .private     # 私有仓库的分支保护需要付费 plan
gh api user --jq .plan
gh api repos/lindongfang/acp-remote/branches/main/protection   # 404 = 未启用或不可用
```

分支保护在 public 仓库上随 GitHub Free 就有，私有仓库需要 GitHub Pro/Team/Enterprise。**本仓库是 public**
（2026-09-23 核实：`gh api repos/lindongfang/acp-remote --jq .visibility` 返回 `public`），所以这一项没有 plan 前提。

**已核实的现状（2026-09-23）**：ruleset **`main-protection`**（id `23858733`）——`target: branch`、
`enforcement: active`、条件 `ref_name: ["~DEFAULT_BRANCH"]`；规则为 `deletion` + `non_fast_forward` +
`required_status_checks`（`strict_required_status_checks_policy = true`、`do_not_enforce_on_create = false`，
五个必需检查就是上面那五个上报名，已逐个与 check-runs 对上）+ `pull_request`
（`required_approving_review_count = 0`；单人仓库必须是 0，否则自己的 PR 无人可批准）；
bypass list 保留 `RepositoryRole admin / always`。`branches/main/protection` 仍返回 404：
本仓库用 ruleset 而不是经典分支保护，两者不需要同时开。

这一档（**PR 必需 + 保留 admin 紧急出口**）的实际含义：默认路径是 PR，因此三个只能在 CI 运行的判定
（依赖许可证与来源、依赖安全公告、密钥扫描）是先于落地的门禁；bypass 让直推仍然可行，但那是紧急出口不是
日常路径（直推会跳过这三个判定）。变更落地流程（分支 → PR → `gh pr checks --watch` → squash 合并）写在
`AGENTS.md` §8。

没有再上「移除 bypass」的理由：对单人仓库它不是对你的安全边界（你本来就能改 ruleset），只是减速带；
而它的代价是真实的一一某个 workflow 改动把必需检查弄红时，所有 PR 都进不来，你还得先去改设置。
多人协作时再考虑。

仍待办：本地密钥扫描的自研格式**规则**（机制已装好：`.husky/pre-commit` 会调 gitleaks 扫暂存内容，
规则与 CI 共用 `.gitleaks.toml`；规则等密钥格式定稿再加），详情见下方与
`docs/adr/0008-ci-supply-chain-tooling.md` 的残余风险 3。

已核实为**已开启的**（2026-09-23）：`secret_scanning`、`secret_scanning_push_protection`、
Dependabot 告警与安全更新。push protection 在推送前拦截已知 provider 模式的凭据——这个时序 CI 给不了；
但本项目自研格式的密钥要靠 CI 的 `gitleaks` 或通用模式检测。两层仓库级检测与运行时扫描的分工写在
`SECURITY_DESIGN.md` §18.1。

两个仍有待开启的，**只能走界面，REST API 写不进去**（实测：`PATCH /repos/{owner}/{repo}` 带
`security_and_analysis.secret_scanning_non_provider_patterns` / `secret_scanning_validity_checks`
既不报错也不生效，字段原样返回 `disabled`）：

两个曾计划开启的子特性，**已核实为在本仓库不可用**（2026-09-23），不要再去翻开关：

- REST API 不接受这两个字段：`PATCH /repos/{owner}/{repo}` 带
  `security_and_analysis.secret_scanning_non_provider_patterns` / `secret_scanning_validity_checks`
  既不报错也不生效，读回仍是 `disabled`；
- 仓库的 `security_and_analysis.advanced_security` 为 `null`（该仓库不适用 GitHub Advanced Security）；
- 界面侧：Settings → **Advanced Security**（个人账号仓库下是这个名称；组织账号的文档里叫
  “Code security” / “Code security and analysis”）页面里**没有 Secret scanning 区域**——public 仓库的
  secret scanning 与 push protection 由 GitHub 自动开启、不提供开关，而 non-provider patterns /
  validity checks 属于 GHAS 特性。

定性：这是**账号/仓库类型的平台限制**（能力缺口），不是配置遗漏，**不必为此升级付费 plan**。
（顺带修正一个概念：`validity checks` 只是把命中的凭据发给签发方校验是否仍有效，对没有签发方的自研格式
密钥本就没有意义；真正有用的是 non-provider patterns，而它不可用。）

由此得到一条必须记住的推论：**自研格式的凭据在推送前没有任何服务端防线**。push protection 只认 provider
模式，而本项目的 P-256 私钥与 base64url 配对密钥是自研格式；又因为仓库是 public，一旦进了历史就是公开的。

机制已经装好，采用「**一份规则、两个执行器**」：

- 规则写在 [`.gitleaks.toml`](.gitleaks.toml)，CI 的 `secrets` job 与本地钩子共用同一份（不维护两套）；
- 本地执行器是 `.husky/pre-commit` 调 `gitleaks git --pre-commit --redact --staged`（来自上游
  `.pre-commit-hooks.yaml` 的官方写法，不是自拟参数），扫的是**暂存内容**，命中内容由 `--redact` 不打印；
- 本机未安装 `gitleaks` 时钩子**只提示并跳过**（不让提交依赖一个仓库不随附的二进制），CI 仍会判定。

**规则本身等密钥格式定稿再加**：现在只有默认规则集，而自研格式的正则只有在格式定稿后才写得准
（过早写会既误报又漏报）。触发条件是「`identity-auth`/`identity-keystore` 开始产生真实密钥」，
且格式定义与规则必须在**同一改动**里落地（见 `docs/adr/0008-ci-supply-chain-tooling.md` 残余风险 3）。

关于「Dependabot 安全更新」还有一个前置条件值得记下：它要求 **Dependabot 告警先开**，否则
`PUT .../automated-security-fixes` 直接返回 422「Vulnerability alerts must be enabled」。顺序是：

```text
gh api -X PUT repos/lindongfang/acp-remote/vulnerability-alerts      # Dependabot 告警（先决条件）
gh api -X PUT repos/lindongfang/acp-remote/automated-security-fixes  # 安全更新
```

三类设置的实际作用不同，**以 Rulesets 界面的原文为准**：

- **Require status checks to pass**：作用在 **ref 更新**上，界面原文是「Choose which status checks must pass
  before the ref is updated. When enabled, commits must first be pushed to another ref where the checks pass.」——
  直推一个未经过检查的新提交会被拒；正确流程是先把提交推到另一个 ref（分支 / PR 分支），让检查在那里通过，
  再让 main 更新到那些提交。它**不只拦「合并」**：即使不开「要求 PR」，直推也会被拦（前提是下面的绕过设置）。
  两个子选项：`Require branches to be up to date before merging` 只对 PR 生效；
  `Do not require status checks on creation` 豁免「创建 ref/分支」这类场景。
- **Bypass**：仓库 admin 默认绕过该 ruleset 的全部规则（bypass list 里会有 `Repository admin`；
  经典分支保护里的对应开关是「Do not allow bypassing the above settings」的反面）。所以对单人仓库来说，
  只要保留 admin 绕过，这条规则拦的是别人与自动化，不拦你自己；取消绕过才会真正约束你的直推。
- **Require a pull request before merging**：把 main 变成只能经由 PR 落地。
- **Block force pushes / Restrict deletions**：默认随规则生效，不因 actor 而异。

**上线时踩到的两个坑**（改这些设置时会再遇到，所以留在这里）：

1. 必需检查的候选列表只来自**最近跑过的检查**：新 job 从未执行过时 `Add checks` 的下拉是空的，无从选起；
2. 必需检查的 context 必须是 **check-runs 上报的名字**
   （`gh api repos/<owner>/<repo>/commits/<sha>/check-runs --jq '.check_runs[].name'`），不是 workflow 里的
   job id：填错会原样存下一个永远不会上报的名字，规则变成**永久等待**，而自己有 bypass 所以感觉不到。

这也是当初先上零摩擦档、等五个 job 都绿过再收紧的原因：在检查没绿之前就上「要求 PR」，会把「合并」与
「改配置」一起锁死。

**当前档位**：默认路径是 PR（规则要求 PR + 五个必需检查 + `strict_required_status_checks_policy` 的 up-to-date），
并保留 `Repository admin` 作为紧急出口。**只有多人协作时才需要考虑移除 bypass**：对单人仓库它不是对你的
安全边界（你本来就能改 ruleset），只是减速带；而它的代价是真实的——某个 workflow 改动把必需检查弄红时，
所有 PR 都进不来，你还得先去改设置。

## 许可

Apache-2.0，见 [LICENSE](LICENSE)。
