## Why

`docs/DEVELOPMENT_PLAN.md` 的实施切片 3（行 34–38）还没有任何实现：workspace 里既没有 `identity-auth`，也没有 `identity-keystore`，因此 `docs/IDENTITY_AND_AUTH_CONTRACT.md` 冻结的状态机、握手入口、授权展开与 keystore 端口**没有任何代码或测试**。其后果在依赖链上是直接的：`server::node_link`（切片 5）与 `server::sync`（切片 7）没有认证可调用，`server::local_admin` + Daemon/CLI（切片 4）的配对、确认、撤销与 Provider 凭据接口没有可装配的实现，而 `docs/SECURITY_DESIGN.md` §18.1 要求的「把 12 个 transcript 固定向量与畸形输入固化成常驻 Rust 测试」目前只有 JavaScript 侧的结构校验，没有 Rust 消费方。

同时，切片 3 的验收条件（配对过期/并发认领/重复认领、proof 重放、密钥变化、撤销后再认证、错误 scope、keystore 不可用、私钥不进入 SQLite/日志/错误）**在契约里已经写死**（`IDENTITY_AND_AUTH_CONTRACT.md` §4.2/§4.5/§6/§8），但没有任何可执行证据。本变更把「身份、配对与授权」作为第三个纵向实现切片落地，为切片 4–7 提供唯一可用的身份边界。

## What Changes

- 新增 crate `identity-auth`（加入 workspace `members`）：纯状态机，不依赖平台 API、不写 `cfg` 平台分支、不做文件或网络 IO。范围包括：
  - 配对状态机（设备与节点两种目标共用一套状态机，差异在绑定校验与集合字段）：创建、认领校验（先结构后密码学）、落定（批准/拒绝）、SAS 派生、失败计数与失效、过期终结、重启后「未确认且无法继续验密」的终结；
  - 握手入口：hello 校验与挑战签发、proof 校验（验签公钥**只**来自持久化信任）、一次性 nonce 与 15 秒 TTL、收尾副作用写集，以及交给 core 的已验证事实（`Actor` 只由此构造）与凭据状态；
  - 授权展开：`pack.*` / `preset.*` / `grant.*` → 命令级 scope 集合（含 `preset.*` 的递归展开、未知名称显式拒绝、`local.*` 永不可远程授予），输出只有展开后的 scope/grant，不向 core 传「已授权」标志；
  - 端口：`IdentityKeystore`（密钥与 Provider 凭据）与新增的熵源端口（挑战 nonce、pairing secret、密钥材料随机性），以及 `Clock` 注入；所有时间与随机性都经注入边界，便于测试注入过期、回拨与确定性。
- 新增 crate `identity-keystore`（加入 workspace `members`）：实现 `identity-auth` 定义的端口。
  - Windows x64（首个交付平台）：DPAPI（当前用户 scope）包裹私钥字节 + 进程内 `p256` 签名；私钥只在签名瞬间存在于内存（已知取舍，`SECURITY_DESIGN.md` §9.2）；
  - 非 Windows：**失败关闭**（编译通过 + 运行时明确 `Unavailable`），不提供持久化 fallback——macOS Keychain 与 Linux Secret Service 属各自平台交付前的范围（`SECURITY_DESIGN.md` §20 已定案维持失败关闭）；
  - 明确的开发档：进程内实现只用于显式开发模式与测试，默认不启用（`CONFIG_REFERENCE.md` §8 的 `identity.keystore = "ephemeral"`）；
  - 平台差异只以 `cfg` 子模块 + wrapper crate 表达，`unsafe_code = "forbid"` 保持不变；密钥、Provider 凭据与 pairing secret 不出端口、不进日志/`Debug`/错误/测试快照。
- 合同与门禁同步：`Cargo.toml`（`members` 与依赖登记）、`docs/MODULE_ARCHITECTURE.md`（§3、§3.1、§4.8、§4.12、§5 的已落地状态与 wrapper 选型结论）、`docs/IDENTITY_AND_AUTH_CONTRACT.md`（把实现期定型的入口签名、熵源端口与 keystore 端口最终形状写回；该文档自身要求「实现前必须定型」）、`README.md`「仓库当前状态」、`docs/DEVELOPMENT_PLAN.md` §2、`AGENTS.md` §4 的模块状态表、`.gitleaks.toml`（自研 keystore 条目格式定稿时同步密钥扫描规则）。
- **不修改**任何 wire 合同：`schemas/`、`fixtures/`、`compatibility/` 保持原样；本变更只新增**消费**既有 fixtures 与 `commands.json` 的 Rust 测试（transcript 重算、验签、HMAC、SAS、畸形输入、scope 展开表漂移）。
- 本次**不包含**：Daemon/CLI 与 `server::local_admin`（切片 4）、`server::node_link`（切片 5）、`node-link-client` 的 access 侧 claim 与 `server::acp_facade`（切片 6）、`server::sync` 与 Web/PWA（切片 7）；macOS/Linux keystore 后端；任何新的 ACP/Sync/Node Link/Local Admin wire 取值。

## Intent and Constraints

```agentic-intent
sources:
  - "用户在 2026-09-24 的本会话中先给出上下文引用 `docs/DEVELOPMENT_PLAN.md#L36:38`，指向 `## 3. 实施切片` 的 `### 3. 身份、配对与授权`（该文件行 34–38 原文：「实现纯状态机 `identity-auth`、平台适配 `identity-keystore`，覆盖 Node/Device 长期身份、一次性配对、每条新 WSS 连接的 challenge-response、scope 展开、撤销和 nonce 重放防护。密钥职责与端口签名以[身份与认证合同](IDENTITY_AND_AUTH_CONTRACT.md)为准。」/「验收：现有 transcript 固定向量、P1363 签名、HMAC、SAS 及畸形输入成为常驻测试；节点和设备身份不可混用；错误权限、重复 claim、过期及密钥变化均失败关闭；私钥不进入普通 SQLite 字段、日志或协议错误。」），未附加自由文字。"
  - "同会话用户原话：「Propose a new change - create the change and generate all artifacts in one step.」——对上一轮所列选项 1（「起草切片 3 的 agentic 变更提案」）的选择：授权建立本变更并生成全部规划工件，同时明确「planning artifacts only」，不在同一响应内开始实现。"
  - "`AGENTS.md` §8/§10 与 `openspec/config.yaml` 的 context：实施切片必须走 OpenSpec agentic 流程；改端口、值对象或合同形状时必须同一变更内同步权威文档并让 `npm run check` 全绿；E2E 为 `not-applicable` 时须逐变更取得用户批准并写入 `plan.md` 的 `downgrade_approval`。"
constraints:
  - "依赖方向（`MODULE_ARCHITECTURE.md` §5 矩阵为唯一判据）：`identity-auth` 只允许依赖 `core`、`sync-protocol`、`node-link-protocol`、`acpr-transcript`；`identity-keystore` 只允许依赖 `identity-auth`；除 `app` 外没有 crate 依赖 `identity-keystore`。两个 crate 都是首批新增成员，落地时必须让 `check:boundaries` 通过。"
  - "`identity-auth` 是纯状态机：不依赖平台 API、不写 `cfg` 平台分支、不做文件/网络 IO、不读系统时间、不碰 SQLite（`IDENTITY_AND_AUTH_CONTRACT.md` §2/§5.1）。持久化事实以写集/事实值返回给调用方落库；密钥与熵只经端口。"
  - "密码学硬约束（`IDENTITY_AND_AUTH_CONTRACT.md` §3、`SECURITY_DESIGN.md` §9）：原语固定 `p256` + `sha2` + `hmac`；只接受 64 字节 P1363（`r || s`），实现中不得出现 `from_der`；合法 high-S 与 low-S 都必须接受，`r`/`s` 为 0 必须拒绝；base64url 必须无填充；验签前先做长度与类型断言；transcript 一律经 `acpr-transcript` 的表驱动编解码，不自行拼字节，不复制 domain/tag 常量。"
  - "不泄漏：私钥、keystore 材料与 pairing secret 明文不得出现在 `core::model`、SQLite、日志、错误、测试快照或协议载荷中（`SECURITY_DESIGN.md` §13.1/§14.1）；`SecretBytes` 不实现 `Debug`/`Serialize`；keystore 端口不提供导出私钥的方法。"
  - "失败关闭：keystore 不可用、引用缺失/损坏、撤销后再认证、`Revoked`/`Unknown` 凭据状态、scope 缩减都必须返回明确失败，不得静默降级或自动重新生成身份（`SECURITY_DESIGN.md` §20、`CORE_PORTS_AND_STORAGE.md` §11.2）。"
  - "平台与依赖：DPAPI/Secret Service 等平台 API 必须经 wrapper crate，两个 crate 都不得直接 FFI（workspace `unsafe_code = \"forbid\"` 且不得为个别 crate 放开）；新增第三方依赖按 `AGENTS.md` §7 核验必要性、维护状态、许可证与平台支持，且不得抬高 `rust-version = 1.85`；`deny.toml` 的 `[graph] targets` 仍是 Windows x64 + Linux gnu，加入新平台必须同步该处。"
  - "Windows x64 优先：首个交付是 Windows 节点；Linux/macOS 的 keystore 后端在其平台交付前保持失败关闭，不引入持久化 fallback（`SECURITY_DESIGN.md` §20 已定案）。"
  - "不引入新配置键：本变更的两个 crate 由组合根注入参数（目录、Clock、熵源、keystore 后端）；确需新键时必须同步 `docs/CONFIG_REFERENCE.md`，不得只写代码。"
  - "本地入口 `npm run verify`；`cargo-deny`（deps/advisories）与 `gitleaks`（secrets）只在 CI 运行，本地没有等价物，不得声称已在本地通过（`AGENTS.md` §8）。"
non_goals:
  - "不实现切片 4–7：Daemon/CLI 与 `server::local_admin`、`server::node_link`、`node-link-client`（含 `node.pair.begin mode = access` 的远程 claim 调用）、`server::acp_facade`、`server::sync` 与 Web/PWA。本变更不产生任何可端到端运行的产品路径。"
  - "不实现 macOS Keychain、Linux Secret Service 或任何非硬件保护的持久化 fallback；非 Windows 一律失败关闭。"
  - "不修改 `schemas/`、`fixtures/`、`compatibility/` 的既有内容，不新增或删除错误码、feature ID、命令名、pack/preset/grant 取值。"
  - "不做 Node key 轮换（`node.rotate-key.*` 属 post_mvp）、不做组织级用户身份、不把 `localPrincipalRef` 升格为安全凭据。"
  - "不引入 Noise、SQLCipher、字段加密、云中继或账号中心；不自行设计加密算法。"
  - "不在本变更内抬高 MSRV / 工具链基线，也不为放开 `unsafe_code` 而新增 crate 级 lint 覆盖。"
success_criteria:
  - "配对：设备配对与节点配对共用状态机且集合字段不可混用（设备带 grants、节点带 scopes 一律拒绝）；过期/并发认领/重复认领（相同内容幂等、不同内容冲突）/拒绝与过期不创建信任/已撤销身份不因普通写入重新激活，都有测试。"
  - "pairing secret 生命周期：明文只在内存、落库只有 digest、到期清除、批准后在首次认证成功时提前清除、重启不能凭 digest 恢复（未确认且无法继续验密的配对被终结）。"
  - "SAS 由双方各自计算：取 pairing SAS transcript 的 HMAC-SHA256 输出前 4 字节按 u32be 解释后 `% 1_000_000`，补零为 6 位十进制；Daemon 不下发本机结果冒充对端结果。"
  - "握手：每个新连接完整执行 challenge-response；challenge 与 server_nonce 一次性消费、未知或已消费一律拒绝；TTL 15 秒由注入 `Clock` 判定；重放同一 proof 失败并追加认证失败审计；未知对端也照常签发挑战（不泄露存在性）；验签公钥只取自持久化信任，自带的另一把公钥不能通过。"
  - "凭据状态映射：`Active`/`ScopeReduced`/`Revoked`/`Unknown` 可区分返回；`Revoked`/`Unknown` 不降级为授权拒绝，以便上层映射到不同错误码与连接行为。"
  - "授权展开：`pack.*`/`preset.*`/`grant.*` 的 Rust 表与 `compatibility/commands/v1/commands.json` 逐条相等（漂移测试）；未知或拼错名称显式拒绝而非忽略或部分展开；`local.*` 7 项永不参与展开、永不远程授予；输出只有命令级 scope。"
  - "keystore：Windows 上 DPAPI（当前用户 scope）包裹 + 进程内签名可往返（公私钥一致、签名可被 `identity-auth` 用持久化公钥验证）；条目缺失或损坏时返回明确 `Unavailable` 而不是静默生成新身份；非 Windows 编译通过且运行期明确失败；任何路径都不把密钥或凭据写入日志、`Debug` 输出或普通 SQLite 字段。"
  - "固定向量成为常驻回归测试：`fixtures/sync/v1/transcripts/` 与 `fixtures/node-link/v1/transcripts/` 的 12 个向量可重算 transcript、验签/HMAC 与 SAS；两个 `invalid/` 目录的畸形输入（含公钥长度/曲线点/base64url、tag 乱序与重复）被拒绝。"
  - "`npm run check` 与 `npm run verify` 全绿；新增 crate 的测试确实被执行（无零用例、无全跳过）。"
decision_bounds:
  - "Agent 可自主决定：两个 crate 的内部模块划分与错误类型形状、内存态注册表（挑战缓存、配对 secret、失败计数）的设计、入口函数的具体签名与命名（定型后写回 `IDENTITY_AND_AUTH_CONTRACT.md`）、keystore 条目的文件布局与包含的私钥编码（如 PKCS#8）、熵源端口的具体形状、测试基座与 fake 端口写法、DPAPI wrapper crate 的选型（前提是满足 MSRV 1.85、许可证与维护性核验）。"
  - "需要用户决策：抬高 workspace `rust-version`/工具链基线；新增 ADR 或为某个 crate 覆盖 unsafe lint；改动 `core` 端口签名、依赖矩阵、wire 合同或封闭词表；把非硬件保护的 keystore 实现改为默认启用；把 macOS/Linux keystore 后端纳入本次范围；把 Daemon/CLI 接线（切片 4）并入本变更。"
  - "用户已批准：本变更的范围粒度（切片 3 作为一个变更，覆盖两个 crate）与「先出规划工件、不在同一响应内实现」。Main E2E 的 `not-applicable` 降级**尚未**取得本变更的批准，必须在生成 `plan.md` 前向用户取得并记录原话、时间与来源。"
assumptions:
  - "非 Windows 平台按失败关闭交付（编译通过 + 运行期明确 `Unavailable`），不实现 Secret Service/Keychain；依据是 `DEVELOPMENT_PLAN.md` 的交付顺序、`INITIAL_DESIGN.md` §14 与 `SECURITY_DESIGN.md` §20 的已定案。若用户要求本次即实现 Linux 后端，则本变更的范围与依赖面（D-Bus client、`deny.toml` 的 targets）需重开。"
  - "DPAPI wrapper 的首选候选是 `windows-dpapi 0.2.0`（安全 API、`Scope::User`、MIT OR Apache-2.0）；但它依赖已停止维护的 `winapi 0.3`，且作者为单人、`rust-version` 未声明。按 `SECURITY_DESIGN.md` §20 的核验要求，实现第一步必须实证其 MSRV/许可证/维护状态与传递依赖；不满足时**回到用户决策**（换 wrapper 或新增 ADR），不擅自放开 `unsafe`。"
  - "`identity-auth` 按合同自持 `CanonicalOrigin`/`NodeEndpoint` 值对象（不使用协议 crate 的业务类型）；为避免与协议侧同名值对象在校验语义上漂移，用同一接受/拒绝语料库做等价比对测试；若用户认为应直接复用协议 crate 的类型，则这是对 §5 依赖规则说明的调整。"
  - "熵源是合同未覆盖但实现必需的边界：本变更新增一个注入式熵源端口（由平台侧提供 OS 随机数），并在同一变更里把该端口写回 `IDENTITY_AND_AUTH_CONTRACT.md` §7；不引入自研 PRNG，也不在状态机内直接读系统随机数。"
  - "两个 crate 落地后 `MODULE_ARCHITECTURE.md` §5 矩阵的行列已经存在（`identity-auth`/`identity-keystore` 行与列都已登记），因此本变更预计只需更新「已落地/待落地」状态、`Cargo.toml` 依赖与 §4.8/§4.12 的 wrapper 结论，而不需要新增矩阵行列。"
```

## Capabilities

### New Capabilities

- `identity-pairing`: 一次性配对的状态机行为——创建（含请求集合与有效期）、认领校验（先结构后密码学、绑定校验、唯一 peer）、落定（批准/拒绝与信任创建）、SAS 派生、pairing secret 生命周期、失败计数与失效、过期与重启后的终结，以及设备配对与节点配对在集合字段与绑定上的不可混用。
- `identity-handshake`: 每条新连接的 challenge-response 行为——挑战签发（含本节点身份证明与一次性 nonce）、proof 校验（公钥只来自持久化信任、验证前先做长度与类型检查）、重放与过期拒绝、确定性时间边界，以及向 core 交付的已验证事实与凭据状态（含撤销/未知不降级为授权拒绝）。
- `scope-expansion`: 授权词汇到命令级 scope 的展开行为——`pack.*`/`preset.*`/`grant.*` 的展开规则、未知名称显式拒绝、展开结果只在 wire 与记录上出现独立 scope，以及 7 项本地管理能力永不远程授予、也不参与展开。
- `platform-keystore`: 平台安全存储的可观察行为——长期密钥与 Provider 凭据只经端口进出、签名是唯一使用私钥的操作、条目标识可安全落库但不进日志、平台不可用时失败关闭且不静默生成新身份，以及不硬件保护实现的默认不启用约束。

### Modified Capabilities

无。`peer-identity-material`（公钥值对象与配对绑定）与 `admin-state-persistence`（管理表与写集）的**需求**不变：本变更只**消费**它们已冻结的形状与端口（`PeerPublicKey`、`PairingRecord`/`PairingPeer`、`TrustStore` 写集），不新增、修改或删除其行为要求。

## Impact

- **新增 crate**：`crates/identity-auth/`、`crates/identity-keystore/`；`Cargo.toml` 的 `members`，`[workspace.dependencies]` 新增/启用依赖（`hmac` 已登记待用；可能新增 `uuid`（仅解析用）、DPAPI wrapper、熵源依赖）。
- **合同门禁**：`scripts/check-crate-boundaries.mjs`（两个新成员必须逐条满足 §5 矩阵行，`identity-auth` 不得依赖 `acpr-wire`，`identity-keystore` 不得依赖 `core`）；`npm run check` 的既有门禁（`check:doc-links`、`check:command-catalog` 等）在本变更改文档后必须全绿。
- **文档**：`docs/IDENTITY_AND_AUTH_CONTRACT.md`（入口签名、熵源端口与 keystore 最终形状写回）、`docs/MODULE_ARCHITECTURE.md`（§3/§3.1/§4.8/§4.12/§5）、`README.md`（「仓库当前状态」表）、`docs/DEVELOPMENT_PLAN.md`（§2 基线段）、`AGENTS.md`（§4 模块状态表）；`.gitleaks.toml` 若自研密钥文件格式可被模式识别则同步规则。
- **不涉及**：`schemas/`、`fixtures/`、`compatibility/` 的既有资产、`core` 端口签名与值对象、`storage-sqlite` 表结构、Sync/Node Link/Local Admin wire、前端工程。
- **运行环境**：Windows 上新增 DPAPI 往返与签名一致性的平台测试（Linux CI 只覆盖失败关闭路径与共享代码）；`cargo-deny`/`gitleaks` 仍只在 CI 判定。
