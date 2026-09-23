# ADR-0008：CI 供应链工具与第三方 action 固定策略

- 状态：Accepted
- 日期：2026-09-23
- 影响范围：CI（`.github/workflows/ci.yml`）、依赖与许可证策略（`deny.toml`）、密钥扫描策略（`.gitleaks.toml`）、
  工具链固定（`rust-toolchain.toml`）、本地钩子（`.husky/pre-commit`）、`SECURITY_DESIGN.md` §16.2 与 §18.1

## 背景

`SECURITY_DESIGN.md` §16.2 要求「CI 使用固定 action/toolchain major 或 digest、锁定 Rust/npm 依赖并保存审核记录」，
并要求生成依赖许可证清单以便定位受影响版本。在本次变更前，这些要求只有一半落地：依赖靠 `Cargo.lock` /
`package-lock.json` 锁定，候选依赖的准入审查（`AGENTS.md` §7 的「必要性、维护状态、许可证、平台支持和安全风险」）
**只有人工纪律，没有机器判据**；许可证清单、依赖安全公告与密钥扫描都不存在。

`.github/workflows/ci.yml` 当时还写着一条更窄的约束：「只用 GitHub 官方 action 且固定 major；不引入第三方 toolchain
action」。这条约束在实践中有两个问题：

1. **「固定 major」不能顺着旧版本号理解**。Node 20 runtime 已从 GitHub hosted runner 移除（`gitleaks-action` v3 的
   迁移说明给出的时间点是 2026-06-02 切换默认、2026-09-16 彻底移除），因此 `actions/checkout@v4`、
   `actions/setup-node@v4` 这类旧 major 不再是因为「稳定」而可用，而是会直接失败。固定 major 这条规则的含义
   必须收窄为「固定到仍受支持的 runtime 上的 major」。
2. **依赖公告与密钥扫描没有官方 action**。硬守「只用官方」等于放弃这两类判定，而它们恰好是 §16.2 与
   `AGENTS.md` §3（私钥与 token 不得进入日志、测试快照等）最需要机器判据的部分。

备选方案：

- **A：全部自研。** 许可证与 advisory 判定的自研等价物不现实（需要 advisory 数据源与 SPDX 求值）；密钥扫描可以自研，
  但只能用关键词、已知 token 前缀、可疑文件名与 fixture 白名单，识别不了格式陌生但熵值很高的凭据，
  且维护成本落在本项目。
- **B：引入成熟第三方 action，按 digest 固定。** 覆盖面与维护成本都更好，代价是把第三方代码引入 CI 信任边界。

决策依据：本次选择 B，并用三项义务补回 B 带来的信任成本——按完整 commit SHA 固定、在本 ADR 记录
「许可证 / 向外发送的数据 / 失败模式」、不引入无法审阅其行为的工具。

## 决策

1. **官方 `actions/*` 固定到 major，且必须是受支持 Node runtime 上的 major。** 当前使用 `actions/checkout@v7`、
   `actions/setup-node@v7`、`actions/cache@v6`。理由：官方 action 由 GitHub 维护，逐 commit 审阅其实现变化不可行，
   固定 major 是上游推荐形态；但 major 的选择必须跟随 runtime 生命周期，不能因为「以前一直用 v4」而保留。
2. **第三方 action 一律按完整 commit SHA 固定**，并在同一行注释里写明对应 tag：
   `EmbarkStudios/cargo-deny-action@3c6349835b2b7b196a839186cb8b78e02f7b5f25`（v2.1.1）、
   `gitleaks/gitleaks-action@e0c47f4f8be36e29cdc102c57e68cb5cbf0e8d1e`（v3.0.0）。
   SHA 变更等同引入新工具：必须在 PR 里说明来源、变更内容与理由，不能由 Dependabot 直接合入
   （`.github/dependabot.yml` 的 `github-actions` 分组注释已写明这一点）。
3. **每个第三方 action 必须记录许可证、向外发送的数据与失败模式**（本节即该记录）：
   - `EmbarkStudios/cargo-deny-action@v2.1.1`：Apache-2.0 / MIT 双许可，无许可证密钥要求。镜像基于
     `rust:1.85.0-alpine3.20`（按 digest 固定），构建时从 GitHub release URL 下载 `cargo-deny 0.20.2` 的
     musl tarball 且**不校验 checksum**。运行时把仓库内容留在 runner 内，需要网络访问 crates.io 与
     RustSec advisory-db（github.com）。
   - `gitleaks/gitleaks-action@v3.0.0`：**不是开源许可**，v2.0.0 起从 MIT 改为 Gitleaks LLC 的专有 EULA。
     个人账号仓库免费；组织账号仓库需要 `GITLEAKS_LICENSE` secret，否则 job 失败。它把许可证密钥、仓库名与
     仓库 owner 发送到 keygen.sh 做许可证校验——这是本仓库 CI 中唯一的第三方数据外流，被发送的内容不含代码与凭据；
     扫描在 runner 内完成，代码不出 runner。本仓库是个人账号，因此**刻意不设置** `GITLEAKS_LICENSE`，
     并在 workflow 里注明「转为组织账号时必须补」。许可形态是这次接受的已知成本，不是意外发现。
   - `gitleaks` CLI 版本由 workflow 的 `GITLEAKS_VERSION`（8.30.1）显式固定：不固定时实际判定版本隐藏在 action 的
     dist 里，仓库中看不到，升级也就无法在 diff 里被审阅。
   - **覆盖范围（读 v3.0.0 的 `src/index.js` 与 `src/gitleaks.js` 得出，不是推测）**：action 只在
     `push` / `pull_request` 事件上追加 `--log-opts`，因此那两种事件只扫**本次范围**；`schedule` /
     `workflow_dispatch` 走的是不带范围的 `gitleaks detect`，也就是**全历史**扫描。这使每日定时任务成为
     「发现历史里已有凭据」的唯一入口，也让 `fetch-depth: 0` 有了明确用途（解析推送范围需要两端提交在本地）。
     命令行固定带 `--redact`，命中内容在日志、job summary 与 SARIF artifact 里被脱敏——对**公开仓库**
     （本仓库的可见性，见 `README.md`）这是必要的。
   - **仓库级原生检测是配套手段，不是替代**：本仓库已启用 GitHub 的 `secret_scanning` 与
     `secret_scanning_push_protection`。push protection 在**推送前**拦截已知 provider 模式的凭据，
     提供 CI 给不了的时序；但本项目将要产生的凭据是自研格式（P-256 私钥、base64url 定长密钥材料），
     provider 模式认不出来，而 `secret_scanning_non_provider_patterns` 当前为 disabled。
     两者的覆盖范围与待办写在 `SECURITY_DESIGN.md` §18.1 与 `README.md`。
4. **依赖判定分两个 job，两者都阻塞。**
   `deps` = `cargo-deny check bans licenses sources` + `npm audit --audit-level=high`：由锁文件决定，判定稳定。
   `advisories` = `cargo-deny check advisories`：由上游发布时间驱动。
   两者都不做 `continue-on-error`。理由：本仓库的安全立场是明确失败而不是静默降级（`SECURITY_DESIGN.md` §15）。
   如果实践中出现「上游 advisory 阻塞与依赖无关的 PR」，正确处理是按 §16 加一条带理由的 `ignore`，或把该 job
   明确改为非阻塞**并在本 ADR 记录这个决定**；不允许悄悄加 `continue-on-error` 了事。
5. **`deny.toml` 只保留本仓库能给出理由的键**，不整份照抄上游模板（上游 `[bans] deny` 列表是 Embark Studios 自己的
   策略）。`[advisories] ignore`、`[licenses] exceptions`、`[bans] deny` 当前有意为空；每加一条都要在同一改动里
   写明「受影响的 crate 与版本范围、为什么本项目不受影响、何时移除」。`[bans] multiple-versions` 暂为 `warn`
   （当前 13 组重复版本全部是传递依赖，收紧为 `deny` 前必须逐组给出 skip 理由），`[sources] unknown-registry`
   与 `unknown-git` 为 `deny`，直接拒绝 crates.io 之外的来源。
6. **工具链版本唯一来源是 `rust-toolchain.toml`**，CI 用 sed 从该文件读取 channel，workflow 里不再写版本号。
   固定到具体版本而不是 `stable` 的理由：clippy 的 lint 集合与 rustfmt 的输出随版本变化，
   `AGENTS.md` §8 用 `-D warnings` 把 clippy 当门禁，版本不固定就会产生「本机绿、CI 红」这类只能靠人记住的假失败。
7. **密钥扫描的允许清单政策**（`.gitleaks.toml` 内的正文是权威）：只允许公开的固定测试向量或已记录的假阳性，
   每条必须写理由与核对人；真实凭据的正确处置是轮换 + 清理历史 + 记录事件（`SECURITY_DESIGN.md` §17），
   不是加允许清单。
   当前没有任何允许清单条目。
8. **文档引用进入 `npm run check`**（`check:docs`）：文档是本仓库的权威（`AGENTS.md` §1），相对链接、锚点与
   「指名了文档的 `§X.Y` 引用」失效必须被机器判据抓住。归属规则刻意保守（只认同一子句内紧邻指名的文档），
   因此它**不能**代替维护者在重编号后通读文档：无法归因的引用只统计、不判定。

## 结果

- 依赖许可证、依赖来源、依赖公告、密钥材料、文档引用与工具链版本都从「人工纪律」变成「机器判据」，
  且本地与 CI 的比较口径一致（`npm run verify` = CI 的 `checks` job；`deny.toml` 的 `[graph] all-features`
   与 action 默认参数一致）。
- **残余风险**（必须显式承认，不能因为「CI 绿了」就当成已解决）：
  1. `cargo-deny-action` 在镜像构建时从 GitHub release URL 下载 cargo-deny 二进制且不校验 checksum：
     上游若替换该版本的 release 资产，我们没有 digest 可以发现。缓解：action 引用按 commit SHA 固定，
     工具版本（0.20.2）在本 ADR 与 `deny.toml` 注释中记录，升级时人工比对。
  2. `gitleaks-action` 的许可证校验会把许可证密钥、仓库名与 owner 发送到 keygen.sh，且其许可为专有 EULA。
     这是主动接受的条件。
  3. **CI 的密钥扫描无法阻止凭据落地**：push / PR 上它只检查本次范围，且必须在提交已经进入远端之后才运行；
     每日定时任务的全历史扫描发现的也只能是「已经存在」的凭据。删掉文件不等于删掉提交，处置只能是
     轮换 + 清理历史（`SECURITY_DESIGN.md` §17）。推送前的时序本该由 GitHub 的 push protection 提供（已启用），
     但它只认已知 provider 模式，本项目的自研格式密钥不在其中；而能覆盖自研格式的
     `secret_scanning_non_provider_patterns` **已核实为在本仓库不可用**（个人账号 public 仓库、无 GHAS；
     界面无 Secret scanning 区域、API 不接受该字段），因此**这一类凭据在推送前没有任何服务端防线**。
     机制采用「一份规则、两个执行器」：规则写在 `.gitleaks.toml`，CI 的 `secrets` job 与本地
     `.husky/pre-commit`（`gitleaks git --pre-commit --redact --staged`，扫暂存内容）共用同一份；
     本机未装 `gitleaks` 时钩子只提示并跳过（不让本地门禁依赖仓库不随附的二进制），因此这条防线的强度
     取决于是否装了它。**规则本身等密钥格式定稿再加**：只有默认规则集时写不出准确的自研格式正则，
     而是在 `identity-auth`/`identity-keystore` 开始产生真实密钥时与格式定义在同一改动里落地。
     能拦住「红状态进入 main」的是 main 的分支保护：
     本 ADR 写完后先建于 2026-09-23（ruleset `main-protection`，id 23858733），随后同日升到 PR 必需档——
     `deletion` + `non_fast_forward` + `required_status_checks`（`strict = true`，五个上报名）+
     `pull_request`（`required_approving_review_count = 0`），并**保留 `RepositoryRole admin` 的 bypass**
     作为紧急出口。因此默认路径变成 PR（三个只能在 CI 跑的判定因此成为先于落地的门禁），
     但直推在技术上仍可行——所以落地流程必须写在文档里才生效：见 `AGENTS.md` §8。
  4. `advisories` job 的失败可能来自与本次改动无关的上游 advisory，需要人工判断是升级依赖还是记录 `ignore`。
  5. **本机（Windows）无法执行这些判定的等价物**：crates.io index 传输在本机网络下不稳定，
     `cargo install --locked cargo-deny@0.20.2` 未能完成，因此 `deny.toml` 的字段形状是对齐 cargo-deny 0.20.2
     自带模板（只保留能给出理由的键）写成的，**首次真实执行发生在 CI**。这条同样适用于 `.gitleaks.toml`。
  6. 门禁数量与 CI job 数量不再有单一数字定义：`AGENTS.md` §10 与 `README.md` 说明「顺序以 `package.json` 的
     `check` 脚本为准」，新增门禁时必须同时更新 `package.json`、`AGENTS.md` §10、`README.md` 与 CI 注释。
- 与 `SECURITY_DESIGN.md` §16.2 的关系：本 ADR 是该节「固定 action/toolchain major 或 digest」的具体化，
  并补充两条该节原本没有的义务——「第三方 action 的许可证与向外发送的数据必须在 ADR 记录」、
  「官方 action 的 major 必须跟随 runtime 生命周期」。
- **本次不覆盖**：发布链路的 provenance、checksum 签名与 SBOM 仍待发布阶段确定（`SECURITY_DESIGN.md` §20），
  普通 CI 接入这些判定不代表发布链路的供应链要求已经实现。
