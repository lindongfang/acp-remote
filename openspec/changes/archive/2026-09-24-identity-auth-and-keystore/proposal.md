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
