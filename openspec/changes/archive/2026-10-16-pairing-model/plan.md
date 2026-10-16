<!-- 执行安排：把 specs 的行为要求与 design 的技术方案转成可派发的并行工作包。
     plan 只写“谁在哪个隔离工作区做什么、按什么顺序合入、怎么验”，不重复 specs/design 的内容。 -->

## Execution Model

- 派发：主 Agent 用 `task` 工具的 batch 形状一次性扇出多个工作包；每个工作包是**一次 spawn**。
- 隔离：`task.isolation`（`patch` 模式、`apply: false`）。工作包之间不共享目录。
- 交付：`<id>.patch` + `agent://<id>` + `history://<id>`；工作包不产生 commit、不产生分支。
- 合入：主 Agent 按 `## Merge Order` 串行 `git apply`，每次合入后跑主检出 Project Verify。
- 验证：隔离工作区内**没有** `target/` 与 `node_modules/`，因此仓库级检查只在主检出执行。
- 检视：每个工作包由一个**独立** spawn（不与作者同实例）读 patch 与 `agent://<id>` 后给出结构化结论。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 身份层：删除两套配对短验证码（派生、装配、域表条目）与设备侧授权词汇，授权展开只剩导出授权（`identity-pairing`、`identity-handshake`、`scope-expansion`） | none | wp-coder | `crates/identity-auth/`（`src/` 与 `tests/`）、`docs/IDENTITY_AND_AUTH_CONTRACT.md`，以及**越界补做的** `crates/{sync,node-link}-protocol` 的域表与向量期望（原计划无包认领，按「全仓搜不到短验证码域标识」的判据一并处理；WP10 因此只负责剩余项） | patch：域表与派生消失、展开表收窄；该契约文档按实绩改写 | `cargo test -p identity-auth` |
| WP2 | 存储与核心：设备记录不再有授权集合、逐命令的设备授权判定删除、`scopes` 列的处置（删列走 migration 或留作不再写入的历史列）、不再可能发生的安全动作 `device.scopes_changed` 的处置，与合同文案同步（`sync-command-handling`、`sync-session-create`、`sync-event-delivery`、`sync-workspace-catalog`、`admin-state-persistence` 相关口径） | code:WP1 | wp-coder | `crates/core/src/, crates/storage-sqlite/src/, docs/CORE_PORTS_AND_STORAGE.md, docs/SECURITY_DESIGN.md`（仅 §14.2 的审计动作清单一行） | patch：写入集与读取路径、列与安全动作的处置决定与实现 | `cargo test -p core -p storage-sqlite`；`npm run check`（`check:drift`） |
| WP3 | 入站面：配对方法的字段（去掉短验证码与授权集合）、状态响应的已批准字段、命令执行前提改为「已认证」、事件与目录不再按授权过滤（`sync-pairing-http`、`local-admin-methods`、`sync-device-auth`、`sync-workspace-catalog`、`sync-event-delivery`）；**并补做** `core` 侧「目录来源可以是路径」这一缝（`ResolvedWorkspace.alias` 变可选 + 路径解析入口 + 错误分类），原计划无包认领 | code:WP1, code:WP2 | wp-coder | `crates/server/src/`，外加 `crates/core/src/` 的目录来源解析缝（`model/config.rs`、`use_cases.rs`、按需 `broker.rs`） | patch：两个面的字段与判定 + 可选的目录来源 | `cargo test -p server`；`cargo test -p core` |
| WP4 | CLI：`device pair` 的确认仪式去掉短验证码展示，参数面收窄为不再有 `--sas`／`--request`／`--pack`（设备方向不提交任何授权词汇，`node pair` 保留 `--grant`），非交互只用指纹逐字匹配；并修正 app 内部仍按旧口径构造设备配对的测试辅助（`crates/app/src/daemon/sync_ingress_tests.rs`）（`cli-commands`） | code:WP3 | wp-coder | `crates/app/src/, docs/LOCAL_ADMIN_PROTOCOL.md` | patch：参数面与展示面；该文档 §5.3 字段表与 §5.8 映射表/示例同步 | `cargo check -p app --all-targets` 干净、`cargo test -p app --lib` 通过、`cargo clippy -p app --lib -- -D warnings`（**`cargo test -p app` 整体要等 WP8**:`crates/app/tests/` 的旧口径用例归 WP8） |
| WP5 | 前端：用配对载荷公钥验证认领响应的主机证明、已批准响应的主机公钥与载荷逐字节比对、删除创建入口的权限门控与相关说明、目录改为用户可输入（`pwa-web-client` 的两条新增要求与三条替换要求） | none | wp-coder | `clients/app/src/, clients/app/app/` | patch：验签与比对、门控移除、目录输入 | `cd clients/app && npm run check` |
| WP6 | 机器资产：删除两套短验证码的域条目与固定向量、`commands.json` 去掉授权包与预设、配对/状态 schema 去掉 `scopes`、`session.create` 的 payload 形状加上 `workspacePath`（与 `workspaceAlias` 恰好出现一个）与相应 fixture、相关校验脚本判据同步 | none | wp-coder | `compatibility/, fixtures/, schemas/, scripts/` | patch：资产与判据同步 | `npm run check` |
| WP7 | 权威文档与 ADR：`AGENTS.md` §4 的不变量改写、`SECURITY_DESIGN.md`（§10 授权表、§9.5 缩减传播、§:241 展示项、威胁模型补"为什么不需要短验证码"）、按 §9 **修订 ADR-0001**、协议与前端文档、`README.md` | none | wp-coder | `AGENTS.md, docs/, README.md` | patch：文档与实现口径一致 | `npm run check`（`check:docs` 判据） |
| WP8 | Rust 侧契约测试：配对状态不再返回短验证码、设备记录无授权集合、命令不再按授权判拒、导出授权仍按 grant 判定；以及 `session.create` 的目录来源分支（`workspacePath` 成功 + 三类失败的可观察分类，WP3 自述未写、转由本包落地） | code:WP2, code:WP3 | wp-tester | `crates/app/tests/, crates/server/tests/` | patch：可证伪的用例 | `cargo test -p app -p server` |
| WP9 | 前端浏览器级 e2e：假主机返回签名不匹配的认领响应与公钥不一致的已批准响应时客户端拒绝；无任何授权设置即可创建入口可用 | code:WP5 | wp-tester | `clients/app/e2e/` | patch：用例与断言 | `cd clients/app && npm run check` |
| WP10 | 协议 crate 的剩余镜像面（域表与向量期望已由 WP1 补做）：认证与配对的 wire DTO 去掉设备 scope 集合、`session.create` 的 payload 类型接受 `workspacePath`、fixture 期望计数同步 | code:WP1, code:WP6 | wp-coder | `crates/sync-protocol/, crates/node-link-protocol/` | patch：DTO 字段、payload 类型、期望计数（并核验 WP1 已做的域表/向量期望仍一致） | `cargo test -p sync-protocol -p node-link-protocol`；`npm run check` |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 无重叠（它另带一份自有文档 `docs/IDENTITY_AND_AUTH_CONTRACT.md`；该前缀与 WP4/WP7 的 `docs/` 重叠但**文件不相交**，故仍按序串行合入） | `cargo test -p identity-auth` |
| 2 | WP6 | 写范围 `compatibility/`/`fixtures/`/`schemas/`/`scripts/` 与其它包不重叠；早合入以固定机器合同（WP1 删掉的域条目要在这一包之后才在资产侧消失） | `npm run check` |
| 3 | WP7 | 写范围 `AGENTS.md`/`docs/`/`README.md` 与其它包的 `docs/` 前缀重叠但文件不相交（`CORE_PORTS_AND_STORAGE.md` 归 WP2、`IDENTITY_AND_AUTH_CONTRACT.md` 归 WP1、`LOCAL_ADMIN_PROTOCOL.md` 归 WP4）→ 与 WP2 串行 | `npm run check` |
| 4 | WP10 | 依赖 WP6 改完的资产（域表条目、fixture 计数、`workspacePath` 形状）与 WP1 改完的身份层形状；`crates/{sync,node-link}-protocol` 此前无包认领，不插进来 PV1 不可能绿 | `cargo test -p sync-protocol -p node-link-protocol`；`npm run check` |
| 5 | WP2 | `docs/CORE_PORTS_AND_STORAGE.md` 与 WP7 重叠，须在其后 | `cargo test -p core -p storage-sqlite`；`npm run check` |
| 6 | WP3 | 依赖 WP1/WP2/WP10（认证响应不再带设备 scope 集合、`packs` 删除后限流的命令族来源换成本地策略） | `cargo test -p server` |
| 7 | WP4 | 依赖 WP3 的方法面 | `cargo test -p app` |
| 8 | WP8 | 依赖 WP2/WP3 的行为；与 WP4 同属 `crates/app/`（`src/` 与 `tests/`）→ 在 WP4 之后 | `cargo test -p app -p server` |
| 9 | WP5 | 与 Rust 侧无 code 依赖，可在任意点合入；但 PV3 必须在 WP6/WP10 之后重跑（wire 形状由它们固定） | `cd clients/app && npm run check` |
| 10 | WP9 | 与 WP5 同属 `clients/app/`（`src/`/`app/` 与 `e2e/`）→ 在 WP5 之后 | `cd clients/app && npm run check` |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | `cargo test --workspace`（主检出根） | 全部通过 |
| PV2 | `npm run check`（主检出根） | 十项合同门禁全绿（含 transcript 域表与固定向量、命令目录、schema） |
| PV3 | `cd clients/app && npm run check`（`clients/app`） | typecheck + `typecheck:e2e` + vitest + `expo export` 全绿 |
| PV4 | `cargo clippy --workspace --all-targets -- -D warnings`（主检出根） | 无 warning |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer | 删除是否彻底（域表、派生、展开、固定向量引用无残留）、是否误删导出授权的展开 | 无 CRITICAL/MAJOR 未闭环项 |
| WP2 | reviewer | 授权判定删除是否留下"半套"判定、`scopes` 列处置与合同漂移门禁是否一致 | 无 CRITICAL/MAJOR 未闭环项 |
| WP3 | reviewer | 两个面的字段与状态码是否与 specs 一致；事件/目录过滤移除后是否泄露不该下发的内容 | 无 CRITICAL/MAJOR 未闭环项 |
| WP4 | reviewer | 非交互确认只剩指纹逐字匹配后是否仍失败关闭；`--sas` 移除是否残留引用 | 无 CRITICAL/MAJOR 未闭环项 |
| WP5 | reviewer | 验签是否用配对载荷里的公钥（而不是响应自带）、比对是否逐字节、门控移除后是否仍无凭据输入 | 无 CRITICAL/MAJOR 未闭环项 |
| WP6 | reviewer | 机器资产删除后各判据是否仍能独立失败（不是"删了测试就绿"） | 无 CRITICAL/MAJOR 未闭环项 |
| WP7 | reviewer | `AGENTS.md` §4 的不变量改写是否与其他权威文档一致；ADR 修订是否符合 §9 的要求 | 无 CRITICAL/MAJOR 未闭环项 |
| WP8 | reviewer | 用例是否可证伪（断言字段缺席/行为变化，而不是断言代码路径） | 无 CRITICAL/MAJOR 未闭环项 |
| WP9 | reviewer | 假主机用例是否真的断言"拒绝"（而不是断言"没有成功"） | 无 CRITICAL/MAJOR 未闭环项 |
| WP10 | reviewer | 域表与 registry 是否逐条一致（删除后不残留 SAS）；设备 scope 从 wire DTO 消失后是否仍有半套字段；期望计数是否与 fixture 实际集合一致（不是把数字改成刚好通过） | 无 CRITICAL/MAJOR 未闭环项 |

## Main E2E

```yaml
mode: required
reason: 本变更改的是配对这一刻的信任建立与设备可用性，必须在真实 Daemon 与真实页面上证明「配对完成即可用」这条链路仍然闭合。
basis: docs/WORKFLOW.md §7 按被测面选 prelude：判据落在 Web/PWA 面，用 browser prelude 驱动真实页面；断言可观察行为（配对进入 online、目录可用、能在目录里用真实 Agent 建成会话），而不是只看启动成功。
alternative_checks:
  - 「拒绝被签名不匹配的认领响应、拒绝公钥与载荷不一致的已批准响应」由 WP9 在假主机上断言（真 Daemon 无法诚实地产出被篡改的响应）；这不替代真机判据，只覆盖真机无法构造的那一侧
```

## Risks

- **[已登记的 minor，WP6 独立检视发现]** `fixtures/sync/v1/manifest.json` 新增的两条 `session.create` 负例声明 `expectedKeyword: "oneOf"`，而 `check-schema-fixtures.mjs` 只判「失败关键字集合包含声明值」：任何 `type: "command"` 的消息被拒时信封层的 `oneOf` 必然出现，因此该声明**无法判别 payload 层的原因**（实测：把 `workspacePath` 改名后该负例仍因 `additionalProperties` 被拒、检查仍绿）。**实际影响有限**：负例的拒绝本身仍被断言（删掉 payload 的 `oneOf` → 该负例被接受 → 变红），漏掉的是「原因」这一维度。要真正修需要让判据同时比对失败 instancePath/schemaPath（`scripts/`，属 WP6 范围），本次未做，留给后续变更。

- **第一次改写 `AGENTS.md` §4 的安全不变量**：审阅强度最高，WP7 必须与 `SECURITY_DESIGN.md`、ADR-0001 逐条对齐。
- **跨变更同文件**：与 `session-create-settle-order` 同时改 `SYNC_PROTOCOL.md` 与两份规格（不同要求块），且与 `acp-cwd-path-form` 同时在 `crates/core/` 有改动（前者 `derive.rs` 的可见性、本变更的授权判定）——三者合入必须**串行**，后合入者重跑 Project Verify。
- **`scopes` 列的处置**：删列牵动 migration 与合同漂移门禁，留作历史列则是死重量；WP2 必须在 patch 里给出选择与依据。
- **浏览器拿不到宿主绝对路径**：目录输入在界面上只能是文本，WP5 要给可理解的失败与提示；把会话开在敏感目录的风险由用户承担，界面提示是唯一的缓解。
- **WP5 与 Rust 侧不可并行验证契约**：前端依赖 wire 形状（`scopes` 字段消失）由 WP6 固定；若 WP6 未先合入，WP5 的 typecheck 可能对旧 schema 通过而掩盖漂移——故 PV3 在 WP6 之后必须重跑。
