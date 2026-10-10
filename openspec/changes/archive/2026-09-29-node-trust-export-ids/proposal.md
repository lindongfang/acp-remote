<!-- 记录动机、范围、能力与影响；行为写 specs，方案写 design，协作写 plan。 -->

## Why

切片 5（`node-link-owner`）在可见性上只实现了「按权限类别」这一层：出口可见性 = 该 Export 未撤销 ∧ `export.scopes ∩ 节点 grants ≠ ∅`。于是 Owner 无法表达「这个 Access 节点只准用我上架的这 2 个 Export」——要么按类别全给，要么对**所有**节点撤销该 Export。当时经用户裁决把独立的 `exportIds` 维度推后，并在 `NODE_LINK_PROTOCOL.md` §8.2 写明落地前置四项（配对时填报、撤销语义、迁移、兼容流程）。本次把这四项一次补齐。

## What Changes

- **新语义（收窄型白名单）**：信任记录新增 `exportIds` 集合；Export 可见性 = **未撤销 ∧ `scopes ∩ grants ≠ ∅` ∧ `exportId ∈ exportIds`**。清单只能收窄，不能放宽。
- **严接口径（用户选择 A）**：清单**没有「未设置」状态**——它是必需集合，空清单 = 该节点看不到任何 Export。旧库升级时既有信任行的清单写空，**不得默认放权**（与 v1→v2 给 Import 迁 `grants_json` 的既有口径一致）；代价是升级后既有配对需要在本机重新确认才能恢复可见。
- **配对时填报**：`node.pair.confirm` 增加 `exportIds`（必需，可为空数组；每个 id 必须本地存在且未撤销，且其 `scopes` 与该次 `grants` 有交集，否则 `local.not_found` / `local.invalid_params`）；`node.pair.confirm` 与 `node.list` 的 `result` 回显 `exportIds`。
- **撤销语义**：`export.revoke` **不**级联清理清单——条目保留，但因「未撤销」条件自然失效；重新 `export.create` 得到新 `exportId`，不会意外复活。写入 §8.2。
- **迁移 v3 → v4**：`owned_node` 增加清单列（`NOT NULL`，默认空）、版本常量与 `PRAGMA user_version` 推进到 4、旧库升级保留全部既有数据、幂等且第二次打开字节不变。
- **存储形状**：清单用 JSON 列（与 `grants_json`/`imported_import.grants_json` 的既有做法一致），不新建关联表——A 语义下没有「未设置 vs 空」两态，单一集合列即可。
- **不新增 wire**：`exportIds` 只在本机管理面与 CLI 可见，**不**进入 Node Link 握手/catalog；Access 侧仍按 catalog 快照推导 `local.not_found` / `local.unavailable`。
- `docs/NODE_LINK_PROTOCOL.md` §8.2 的信任记录形状与「新增 `exportIds`」待办注记、`CORE_PORTS_AND_STORAGE.md` §3.5/§5/§7/§9、`LOCAL_ADMIN_PROTOCOL.md` §5.4/§5.8、`schemas/local-admin/v1` 与 `fixtures/local-admin/v1` 同步；漂移门禁（§7 DDL ↔ `migrate.rs`、§5 端口 ↔ `ports.rs`）随动。
- **BREAKING（本地管理面，v1 内合同修订）**：`node.pair.confirm` 的 `exportIds` 是新增**必需**字段，按 `LOCAL_ADMIN_PROTOCOL.md` §8 属不兼容变更；因 v1 未发布、CLI 与 Daemon 同版本发布，按「v1 内合同修订」处理并记录（先例：`node.challenge.catalogRevision`）。

## Capabilities

### New Capabilities

无（本次变化落在既有能力的既有需求上，不新建能力路径）。

### Modified Capabilities

- `node-link-owner-server`: catalog 投影与资源访问的可见性需求增加 `exportIds` 条件（收窄），并明确「撤销不级联清理清单、重新上架不复活旧 id」。
- `local-admin-methods`: `node.pair.confirm` 的 `params`/`result` 与 `node.list` 的 `result` 增加 `exportIds`，定义填报校验与失败语义。
- `storage-schema-v2-migration`: 版本常量与 migration 幂等需求增加 v4（`owned_node` 清单列、`user_version`/版本键推进、旧行置空不得默认放权）。

## Impact

- `crates/storage-sqlite`：`migrate.rs`（v3 → v4）、`owned_node` DDL 与读取/写入路径、迁移与管理 store 测试、可能的升级夹具。
- `crates/core`：`NodeRecord`（新增字段）、`PairingSettlement::Approved`（新增集合）、`NodeWrite`/配对落定写集、`Broker` 的 Owner 侧授权判定（必须与目录层同口径）、`use_cases` 的配对/节点用例入口。
- `crates/server`：`node_link::catalog::visible_exports`（唯一判定点）与 `resource`/命令管线的复用点、相关测试。
- `crates/app`：CLI `node pair confirm` / `node list` 的参数与输出；组合根不承载业务规则。
- 合同与机器资产：`schemas/local-admin/v1/envelope.schema.json`、`fixtures/local-admin/v1/**`。
- 权威文档：`docs/NODE_LINK_PROTOCOL.md` §8.2 + 修订记录、`docs/CORE_PORTS_AND_STORAGE.md` §3.5/§5/§7/§9/§11、`docs/LOCAL_ADMIN_PROTOCOL.md` §5.4/§5.8，`docs/SECURITY_DESIGN.md` 若涉及授权措辞。
- 规范：`openspec/specs/{node-link-owner-server,local-admin-methods,storage-schema-v2-migration}/spec.md` 经本变更增量同步。
