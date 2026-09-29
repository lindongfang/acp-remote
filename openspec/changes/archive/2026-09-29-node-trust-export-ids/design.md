# Design: node-trust-export-ids

## 背景

切片 5（`node-link-owner`）落地时，Owner 侧信任记录没有 `exportIds` 列，可见性只由「未撤销 ∧ `export.scopes ∩ node.grants ≠ ∅`」决定。`NODE_LINK_PROTOCOL.md` §8.2 当时写明：落地 `exportIds` 必须同时定义**配对时的填报**、**撤销语义**、**迁移**与**§2.3 的兼容流程**。本设计把这四项落地。

用户已定的四项语义（2026-09-27 本会话，见 `proposal.md` 的意图块）：① 收窄（与 scopes ∩ grants 取交集）；② 方案 A——清单是必需集合、空清单 = 无可见、旧行迁移置空且**不得默认放权**；③ Export 撤销不级联清理清单条目；④ 确认时拒绝「不存在/已撤销」与「与本次 grants 不相交」的条目。范围与三项小决策另见 proposal：`exportIds` 为**必需**参数、Access 侧孤儿 Import 本次只文档化、CLI 空清单打印警告不阻断。

## D1 语义：收窄型白名单，判定点仍然是两个（且必须同口径）

Export 对该 Access 节点可见 ⟺ **三个条件同时成立**：

1. 该 Export 未撤销；
2. `export.scopes ∩ node.grants ≠ ∅`；
3. `export.exportId ∈ node.export_ids`。

判定点仍然只有两处，且都从**持久记录实时**读取，禁止任何进程内缓存副本：

- `server::node_link::catalog::visible_exports`（唯一实现；`resource.attach` 与事件/ACK 的归属复核复用它）。它已经拿到 `Option<&NodeRecord>` 与 `&[ExportRecord]`，因此条件 3 只是多读一个字段，不加新端口。
- `core` 的 Owner 侧命令授权（`Broker::node_allowed` / `UseCases` 的 `Actor::Node` 分支）：它已注入 `TrustStore` 并读取该对端的 `access` 信任行，条件 3 用**同一条节点行**的 `export_ids` 判定，保证「目录里看不到的 Export 也不能被命令选中」。

**为什么不做成替代而不是交集**：清单只能收窄，不能放宽——否则授权面会因为一份列表而绕过 `scopes ∩ grants`，违反「有效权限是交集」的既有口径（`NODE_LINK_PROTOCOL.md` §10、`CORE_PORTS_AND_STORAGE.md` §6 第 5 条）。

## D2 存储形状：`owned_node` 末尾新增 JSON 列，不建关联表

- 新增列 `export_ids_json TEXT NOT NULL DEFAULT '[]'`，编码与既有集合列一致（有类型 JSON 数组文本，空集合固定 `'[]'`；读写都经 `ExportId` 构造器校验）。
- **不建关联表**：方案 A 下清单没有「未设置 vs 空」两态，单一集合列即可；关联表（如 Access 侧的 `imported_import_export`）是为「一个 Import 关联多个 Export 且需要跨族归属仲裁」而存在的，这里不需要。
- **列追加在 §7.3 建表语句的末尾**（不是为了好看，见 D3）。

## D3 迁移 v3 → v4：`ALTER TABLE` + 末尾追加，保证升级库与新建库列清单逐项相等

已核实的既有约束：

- `FILE_FORMAT_VERSION`/`OWNED_SCHEMA_VERSION`/`IMPORTED_SCHEMA_VERSION` 目前都是 3（`crates/storage-sqlite/src/migrate.rs`）；升级段按 `file_version < FILE_FORMAT_VERSION` 在同一事务里跑，已是最新版的库跳过全部升级步骤（因此第二次打开不重写 `sqlite_master`）。
- 迁移测试对**升级库 vs 新建库**的比较口径是 `pragma table_info` 的**列清单**（`migration.rs` 的 `column_names`，当前只覆盖 `imported_*`），不是 `sqlite_master` 文本；对**同一库重复打开**才比 `sqlite_master` 文本逐字节相等。

据此决定：

1. 常量：`FILE_FORMAT_VERSION = 4`、`OWNED_SCHEMA_VERSION = 4`，`IMPORTED_SCHEMA_VERSION` **保持 3**（imported 家族本次不变；两族版本本就分开维护）。
2. 升级段 `V4_UPGRADE_OWNED`：`ALTER TABLE owned_node ADD COLUMN export_ids_json TEXT NOT NULL DEFAULT '[]'`。**不**走 12-step 重建——重建的唯一理由是改 CHECK（v2→v3 的先例），而这里只加列；`NOT NULL` 由默认值满足，既有行因此得到 `'[]'`，即**不默认放权**。
3. 新建库一侧把同一列加在 §7.3 `CREATE TABLE owned_node` 的**末尾**；因为 `ALTER TABLE ADD COLUMN` 也把列追加在末尾，升级库与新建库的 `pragma table_info` 列顺序逐项相等。
4. 漂移门禁：`docs/CORE_PORTS_AND_STORAGE.md` §7.3 的 `owned_node` DDL 文本与 `migrate.rs` 的常量同批更新（门禁归一化后逐条相等）。
5. 迁移测试补齐：把「升级库列清单 == 新建库列清单」的断言从 `imported_*` 扩到 `owned_*`（含 `owned_node`）；升级后既有节点行 `export_ids_json = '[]'`；`user_version`/版本键推进到 4/4/3；第二次打开不改 `sqlite_master`、不写行；「版本过新拒绝打开」用例仍按 `FILE_FORMAT_VERSION + 1` 构造。

**回滚**：与既有口径一致——v4 库对旧二进制是「版本过新」拒绝打开，回滚 = 恢复备份 + 回退二进制，不提供自动降级迁移。

## D4 校验位置与失败语义：在配对落定的同一事务内

- `core::model` 的 `PairingSettlement::Approved` 增加 `granted_export_ids: Vec<ExportId>`（已去重、字典序排序）。
- 校验放在**存储层的 `settle_pairing` 事务内**，与既有 `granted_grants ⊆ requested_grants` 同址（`crates/storage-sqlite/src/admin/trust.rs`）。理由：先读后写若分两次调用就不原子，会出现「校验通过后该 Export 刚好被撤销、写进一条永不生效的清单」。
- 规则：
  - 每个 id 必须能在 `owned_export` 里查到且未撤销 → 否则 `NotFound(EntityRef::Export)`，适配层映射 `local.not_found`；
  - 每个 id 的 `scopes` 与本次 `granted_grants` 必须有交集 → 否则 `InvalidRequest`，适配层映射 `local.invalid_params`；
  - 空集合合法（= 该节点看不到任何 Export），此时不查任何 Export。
- 失败即整事务回滚：不创建信任行、不推进配对状态、不写审计（与 `settle_pairing` 既有语义一致）。

## D5 形状与 wire：只在本机管理面暴露，Node Link 不加字段

- `core::model::NodeRecord` 增加 `export_ids: Vec<ExportId>`（构造器校验去重与排序）。
- `docs/LOCAL_ADMIN_PROTOCOL.md` §5.4：`NodeRecord` 文本加 `exportIds string[]`；`node.pair.confirm` 的 `params` 加 `exportIds`、`result` 回显 `exportIds`；`node.list` 经 `NodeRecord` 自然带出。
- **机器可判定的范围要说清**：`schemas/local-admin/v1/envelope.schema.json` 只把 `params`/`result` 建模为通用对象（逐方法形状由文档拥有），因此「`exportIds` 必填」由**守护进程运行期**强制（缺字段 → `local.invalid_params`）并用测试固定，不由 JSON Schema 的 `required` 表达。方法集、错误码与 `local.*` 能力三处**都不变**（不新增方法/错误码），因此 `schemas/`、`fixtures/local-admin/v1/`、`compatibility/commands/v1/commands.json` 无需改动。
- Node Link wire **不加**字段：Access 侧不需要知道自己的清单，仍按 catalog 可见集推导 `local.not_found` / `local.unavailable`；握手、catalog、resource、command 的 wire 形状与 fixture 全不动。

## D6 CLI 行为

- `node pair confirm` 增加可重复的 `--export-id <id>`（零次 = 空清单）；零次时打印**醒目警告**（"该节点将看不到任何 Export"）但照常执行（用户选择 (b)），不引入 `--allow-none` 之类的新开关。
- `node list` 输出清单；空清单如实显示（不隐藏），与运维视角一致。
- CLI 只做参数收集与展示，不做业务判定（组合根不承载业务规则）。

## D7 审计：不新增动作

配对确认沿用既有的 `node.paired`（它已表达「建立信任并授予初始授权」）。本次**不**新增 `node.export_ids_changed` 之类的动作：没有配对后修改入口，就没有「授权变化」这种事件；`AuditAction` 闭合词表、`SECURITY_DESIGN.md` §14.2 与两张审计表的 CHECK 因此**都不动**（也就避免了第二次 12-step 重建）。

## D8 兼容、已知限制与副作用（全部写进权威文档）

- `node.pair.confirm` 新增**必需**参数属 §8 意义上的不兼容变更；因 v1 未发布、CLI 与 Daemon 同版本发布，按「v1 内合同修订」处理并写进 `LOCAL_ADMIN_PROTOCOL.md` 的修订记录（先例：`node.challenge.catalogRevision`）。
- **升级副作用（必须写明）**：老库升级后既有节点行清单为空 → 这些配对在 Owner 本机重新 `node pair confirm` 之前看不到任何 Export（连接与握手仍然正常）。这是方案 A 的预期代价。
- **已知限制（登记为待办，不实现）**：清单目前**只能在配对确认时填报**，没有配对后的修改入口；**改清单只能重新配对**——要么先 `node.revoke` 再重新配对，要么直接重新配对（同一 `nodeId` 再次经 `node.pair.begin`/claim/`node.pair.confirm` 落定）。两种做法都在信任行落定后立即关闭该节点的活动连接；**但两条路径的关闭语义与关闭码必须可区分**（实现期修正：最初复用撤销路径会推 `node.trust.revoked` 并以 `4410` 关闭，合规对端会按 §15「撤销即停」不再重连——把「收窄」误读成「被撤销」）：重新配对**不**推 `node.trust.revoked`、以 `1000`（正常关闭）+ 说明性 close reason 关闭，因此收窄**自下一次握手后的新连接生效**（`NODE_LINK_PROTOCOL.md` §8.2、§14.2 表注、§15）；只有 `node.revoke` 才推 `node.trust.revoked` 并以 `4410` 关闭、停止重连。不提供配对后修改方法（范围见 proposal 的非目标）。
- **Access 侧副作用（本次只文档化）**：Owner 收窄清单后，对方本地已导入的 Import 仍存在，但对应 Export 从 catalog 消失，后续交付按既有规则失败；本次不改 `import.*` 的任何行为。

## D9 测试策略

- `storage-sqlite`：v3 → v4 迁移（旧行置空、列清单与新建库相等、幂等/字节稳定、版本推进）；`settle_pairing` 的四类校验（存在且未撤销 / 已撤销 / 不存在 / 与 grants 不相交 / 空集合）+ 失败零写入；节点行读写往返（含排序去重归一化）。
- `core`：`NodeRecord` 构造校验（去重、排序、空集合合法）；`PairingSettlement::Approved` 形状；`Broker` Owner 侧授权在「清单为空」「清单不含目标 Export」时必须拒绝（与目录层同口径）。**核心断言：清单为空时，即使 `scopes ∩ grants` 非空也不可见/不可用。**
- `server`：`visible_exports` 的三个条件组合（含空清单 = 空结果、清单内但已撤销 = 不可见）；`resource.attach` 与 catalog 结论一致（复用同一实现，加一条回归断言）。
- `app`：CLI 参数映射（零个/多个 `--export-id`）与空清单警告；`node list` 输出含清单。
- 端到端（受控路径，[PV5] 口径）：Owner 端配两个 Export，确认时只给一个 → Access 侧 catalog 只见其一、`attach` 被拒的 Export 返回 `nodelink.export.not_granted`；`import.add` 在本切片**恒**返回 `local.unavailable`（Access 侧客户端未落地 → 本机没有任何 catalog 快照，`server::local_admin::router` 的 `import_add` 不校验参数就直接返回可重试的 `local.unavailable`），因此受控路径**不**断言 `import.add` 的 `local.not_found`（那是 §5.5 里「有快照但无该 id」分支的语义，本切片不可达）。
  - 勘误（2026-09-27，实现期复核，两处）：
    - 「`attach` 被拒的 Export 返回 `export.not_found`」有误——`resource.attach` 的既有判定是：① `ownerNodeId` 不是本机、或 id 不可解析 → `nodelink.export.not_found`（与「会话不存在」同码，不泄露差别）；② 解析得出 id 但**可见性复核失败**（已撤销、`scopes ∩ grants` 不相交、**不在 `exportIds` 清单内**）→ `nodelink.export.not_granted`（`crates/server/src/node_link/resource.rs`）。两个码都在 `compatibility/errors/v1/errors.json` 的词表内，因此本次**不改 wire 错误码**；specs 未规定 `attach` 的错误码，**无验收场景受影响**。
    - 「`import.add` 该 export 得到 `local.not_found`」在本切片不成立（见上）；§5.5 的 `local.not_found` 分支留给 Access 侧客户端落地后的切片验证。

## D10 影响面（与 proposal 的 Impact 一致）

- `crates/core`：`model/export.rs` 或 `model/identity.rs`（`NodeRecord`）、`model/identity.rs`（`PairingSettlement`）、`broker.rs`/`use_cases.rs`（Owner 侧判定与透传）、`model/tests.rs`。
- `crates/storage-sqlite`：`migrate.rs`（DDL 常量 + v4 段 + 三个常量）、`admin/trust.rs`（写集校验与编解码）、节点行读取路径、`tests/{migration,admin_store}.rs`。
- `crates/server`：`node_link/catalog.rs`（条件 3）与其测试。
- `crates/app`：`cli/pairing.rs`（参数、警告、展示）。
- 文档：`NODE_LINK_PROTOCOL.md` §8.2 + 修订记录；`CORE_PORTS_AND_STORAGE.md` §3.5（`NodeRecord`/`PairingSettlement`）、§7.2（v4 段与常量）、§7.3（`owned_node` DDL）、§9 判据 28、§11.6 第 4 条、§11.7/§11.8 的说明句；`LOCAL_ADMIN_PROTOCOL.md` §5.4 + 修订记录。
- 规范（经本变更增量同步）：`node-link-owner-server`、`local-admin-methods`、`storage-schema-v2-migration`。
