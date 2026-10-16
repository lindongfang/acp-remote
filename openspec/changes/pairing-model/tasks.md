# Tasks

## 1. 身份层：删除两套短验证码与设备侧授权词汇（WP1）

- [x] 1.1 删除设备配对与节点配对的短验证码派生、装配与两个 transcript 域条目——判据：`cargo test -p identity-auth` 通过，且全仓搜不到短验证码域标识
- [x] 1.2 授权展开收窄为只剩跨节点导出授权（授权包与配对预设两类的展开删除）——判据：`cargo test -p identity-auth` 中断言设备方向提交任何授权词汇都被拒、导出授权仍照常展开
- [x] 1.3 本组的文档面：`IDENTITY_AND_AUTH_CONTRACT.md` 的授权展开与短验证码两处按实绩改写——判据：`npm run check` 的 `check:docs` 通过

## 2. 存储与核心：设备不再有授权集合（WP2）

- [x] 2.1 设备记录不再写入/读取授权集合，逐命令的设备授权判定删除——判据：`cargo test -p core -p storage-sqlite` 通过，且不再存在按设备授权拒绝的路径
- [x] 2.2 `scopes` 列的处置落地（删列走版本化 migration，或留作不再写入的历史列）并同步 `docs/CORE_PORTS_AND_STORAGE.md` §7 与 §5/§6 相关措辞；设备不再有 scope 之后 `device.scopes_changed` 这个安全动作不可能再发生，与列处置一并决定（若删动作，同改 DDL CHECK、`IdentityAuditAction` 与 `SECURITY_DESIGN.md` §14.2 的清单，三处一致）——判据：`npm run check` 的 `check:drift` 通过（36 条 DDL 逐条一致）
- [x] 2.3 本组的测试：断言设备记录的读写投影不含授权集合、事件与目录不再按授权过滤——判据：`cargo test -p storage-sqlite` 中新增用例各自可独立失败

## 3. 入站面：字段与判定（WP3）

- [x] 3.1 配对方法不再返回或接受短验证码与授权集合，状态响应的已批准分支只返回非秘密元数据与主机身份材料——判据：`cargo test -p server` 中断言这些字段缺席
- [x] 3.2 业务命令的执行前提改为「已认证」（不再有命令级授权判定），`session.resume` 在 Sync 面仍返回不支持；`session.create` 的拒绝分类按新要求改（别名未登记或目录形状不合法 → 参数类错误，解析失败 → `internal.unavailable`），不再用 `authorization.scope_denied` 表达这些拒绝（该错误码保留给节点侧 grant 与本地通道）——判据：`cargo test -p server` 中分别断言未认证被拒、不支持命令、两类目录失败分类
- [x] 3.3 事件投递与会话清单不再按设备授权过滤——判据：`cargo test -p server` 中断言同一设备能看到全部自有会话
- [x] 3.4 认证握手响应不再下发设备 scope 集合（`authenticated` 的 body 里该字段消失，服务端不再计算/填充它）——判据：`cargo test -p server` 中断言该字段缺席，且认证成功后仍能正常执行命令
- [x] 3.5 `packs` 从 `commands.json` 删除后，命令限流的**命令族**改由服务端自持策略提供，并断言 registry 里每个命令名恰好被一个族覆盖（vocabulary 仍以命令名为唯一来源）——判据：`cargo test -p server` 中原 `command_families_mirror_the_registry_packs` 的替代断言可独立变红（新增一个命令名而不归类 → 红）
- [x] 3.6 `core` 侧的「目录来源可以是路径」这一缝：`ResolvedWorkspace.alias` 变可选、新增客户端目录的解析入口（形状不合法 → 参数类错误且不触碰文件系统；解析不了 → `Unavailable`），来源是目录时 `owned_session.workspace_alias` 落 `NULL`，且**不**把该目录注册进 catalog、不回显为对端可见字段——判据：`cargo test -p core -p server` 通过，且两条来源各自的失败分类各有一条可独立变红的断言

## 4. CLI：确认仪式（WP4）

- [x] 4.1 `device pair` 不再展示短验证码、不再接受 `--sas`／`--request`／`--pack`（设备方向不提交任何授权词汇；`node pair` 保留 `--grant`），非交互只用 `--fingerprint` 逐字匹配；并修正 `crates/app/src/` 内仍按旧口径构造设备配对的测试辅助（`daemon/sync_ingress_tests.rs` 以非空 scopes 建/批准配对——WP1 的检视点出后转由本包处理）——判据：`cargo test -p app` 中新增用例断言只给 `--sas` 时报用法错误、指纹不匹配时非零退出且不改状态，且该辅助改用空集合后既有两条用例恢复绿
- [x] 4.2 `docs/LOCAL_ADMIN_PROTOCOL.md` §5.8 的映射表与示例同步——判据：`npm run check` 通过

## 5. 前端：密码学校验与去门控（WP5）

- [x] 5.1 认领响应的主机证明用配对载荷里的公钥验证，失败即停止轮询且不落盘——判据：`cd clients/app && npm run check` 通过，且新增用例断言验签失败路径
- [x] 5.2 已批准响应的主机公钥与配对载荷逐字节比对，不一致即失败不落盘——判据：同前，新增用例断言不一致路径
- [x] 5.3 删除创建入口的权限门控与「需要由电脑端授予」类说明；目录改为用户可输入（含可理解的失败与提示）——判据：`cd clients/app && npm run check` 通过，且渲染测试中断言入口恒可用、无权限文案

## 6. 机器资产（WP6）

- [x] 6.1 删除两套短验证码的域条目与固定向量、`commands.json` 去掉授权包与预设、配对与状态 schema 去掉授权集合字段——判据：`npm run check` 全绿（含固定向量重编码与契约资产判据）
- [x] 6.2 同步相关校验脚本的判据，使删除后的资产仍能被独立断言（不是删了测试就绿）——判据：`npm run check` 中各判据仍能在人为改坏一项时变红

## 7. 权威文档与 ADR（WP7）

- [x] 7.1 `AGENTS.md` §4 的不变量改写为「通过设备认证即可执行命令、设备不再有授权维度」，并同步 `SECURITY_DESIGN.md` §10 与 §:241——判据：`npm run check` 的 `check:docs` 通过，且两处措辞一致
- [x] 7.2 `SECURITY_DESIGN.md` 的威胁模型补写「为什么本协议不需要短验证码」（锚走带外 + 认领 HMAC 已承担经典职责；对载荷被整条替换与秘密泄露无效）——判据：该节有可引用的决策句
- [x] 7.3 按 `AGENTS.md` §9 修订 `docs/adr/0001-pwa-identity-and-secure-transport.md`——判据：ADR 的决策与 Profile 段落不再含「设备权限」这一维度
- [x] 7.4 `SYNC_PROTOCOL.md`、`NODE_LINK_PROTOCOL.md`、`FRONTEND_DESIGN.md`、`CONFIG_REFERENCE.md`、`README.md` 同步；并按 `AGENTS.md` §9 的映射改写 `MODULE_ARCHITECTURE.md` 与 `INITIAL_DESIGN.md` 里被本变更变成假命题的段落（`pack.*`/`preset.*` 作为授权输入、命令按设备 scope 授权、Sync 命令族的来源三类表述），只改这些，不顺带重构——判据：`npm run check` 全绿

## 8. Rust 侧契约测试（WP8）

- [x] 8.1 配对状态不再返回短验证码、设备记录无授权集合——判据：`cargo test -p app -p server` 中用例可独立失败。**具体迁移点（WP3 的检视点名）**：`crates/app/tests/sync_e2e.rs:141/:178` 的旧配对参数（`begin` 就断）、`:362-364` 对 `authenticated["body"]["scopes"]` 的断言（该键已不存在）、`crates/app/tests/sync_commands_e2e.rs:111/:140` 的旧配对参数
- [x] 8.2 命令不再按授权判拒、导出授权仍按 grant 判定——判据：同上，两侧各一条断言。**具体迁移点**：`crates/app/tests/sync_commands_e2e.rs:468` 仍断言 `authorization.scope_denied`（该设备现在会被受理并产生副作用）——改成「无任何授权集合的已认证设备照常被受理」，并保留一条节点侧 `grant.*` 判定仍生效的断言；`crates/app/tests/cli_commands.rs` 的 `pairing_requires_exact_non_interactive_values_and_honours_expiry` 需按新参数面（无 `--sas`/`--request`/`--pack`、只用 `--fingerprint`）改写
- [x] 8.3 `session.create` 的目录来源分支（WP3 自述未写，转由本包落地）：`workspacePath` 成功路径（会话建成、且来源是路径时落库 `workspace_alias` 为 `NULL`）+ 三类失败各自的**可观察**分类——别名未登记 → 参数类错误（`protocol.schema_invalid`）、形状非法（相对路径/含 `..`）→ 参数类错误且不触碰文件系统、形式合法但解析不了（不存在/不是目录）→ `internal.unavailable`；沿用 `crates/app/tests/` 既有的 `session.create` e2e 骨架——判据：`cargo test -p app` 中四条各自可独立变红

## 9. 前端浏览器级 e2e（WP9）

- [x] 9.1 假主机返回签名不匹配的认领响应时客户端拒绝——判据：`cd clients/app && npm run check` 中该用例断言「明确失败」，而不是「没有成功」
- [x] 9.2 假主机返回公钥与载荷不一致的已批准响应时客户端拒绝且不落盘——判据：同上
- [x] 9.3 无任何授权设置时创建入口可用、可提交——判据：该用例断言入口可用与载荷形状

## 10. 协议 crate 的镜像面（WP10）

- [x] 10.1 认证与配对的 wire DTO 去掉设备 scope 集合（`AuthenticatedBody.scopes`、配对状态里的设备 scope），并同步 `envelope_fixtures.rs`/`pairing_fixtures.rs`/`field_constraints.rs` 的期望与 fixture 集合——判据：`cargo test -p sync-protocol -p node-link-protocol` 通过（合入 WP1/WP6 后当前红：`valid/auth-authenticated.json` 报 `missing field scopes`）
- [x] 10.2 `session.create` 的 payload 类型接受 `workspacePath`（与 `workspaceAlias` 恰好一个），并核验 WP1 已改的域表与固定向量期望仍与登记表逐条一致（不重复改动）——判据：`cargo test -p sync-protocol -p node-link-protocol` 与 `npm run check` 全绿

## 11. 合入与真机验收

- [x] 11.1 按 plan 的 Merge Order 串行合入 WP1→WP6→WP7→WP10→WP2→WP3→WP4→WP8→WP5→WP9，每次合入后跑该序的再验证——判据：每次 `git apply --check` 通过且随后的检查全绿
- [x] 11.2 主检出跑 PV1（`cargo test --workspace`）、PV2（`npm run check`）、PV3（`cd clients/app && npm run check`）、PV4（`cargo clippy --workspace --all-targets -- -D warnings`）——判据：四条命令全绿
- [x] 11.3 执行 plan 的 Main E2E：真实 Daemon + 真实页面完成配对（不设置任何授权）并进入 online、目录可用、能在目录里用真实 Agent 建成会话——判据：三重可观察行为同时成立，观察证据留存
