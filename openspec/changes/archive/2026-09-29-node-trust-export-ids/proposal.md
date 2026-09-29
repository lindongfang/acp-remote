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

## Intent and Constraints

```agentic-intent
sources:
  - "2026-09-27 本会话，用户原话：「第5部分实现，遗留了一个问题。我想把 exportIds 真正实现出来：数据库加列（又要一次迁移）、配对确认命令加参数、CLI/文档全套跟上」"
  - "2026-09-27 本会话，用户对四个语义选择的回答：「1. A」「2. 当前项目还没上线，需要考虑吗」「3. 按你推荐来」「4. 同意」——① 关系取收窄（交集）；② 取方案 A（严格白名单：空清单 = 无可见，旧行迁移置空、不得默认放权）；③ Export 撤销后清单条目保留、不级联清理；④ 确认时拒绝无效或不与本次 grants 相交的条目"
  - "2026-09-27 本会话，用户追问：「当前项目还没上线，需要考虑（迁移/兼容）吗」——主 Agent 的回答与据此确认的范围：不做过渡期/双写/旧客户端混用，但迁移脚本、版本常量、旧库升级测试与门禁同步仍必须做"
  - "2026-09-26 存档变更 openspec/changes/archive/2026-09-27-node-link-owner/design.md D14 与其 verification.md：`exportIds` 独立维度记为后续阶段待办，并列出需要的面（core model + `owned_node` 加列迁移 + `node.pair.confirm` 参数 + LOCAL_ADMIN_PROTOCOL/CLI/文档）"
  - "docs/NODE_LINK_PROTOCOL.md §8.2：落地 `exportIds` 必须同时定义配对时填报、撤销语义与迁移，并按 §2.3 的兼容流程处理，不能把现有字段当成已有能力"
constraints:
  - "可见性只能收窄：`exportIds` 与既有 scopes ∩ grants 取交集，任何路径都不得因清单而放宽权限"
  - "可见性保持单一判定点：catalog 投影、`resource.attach`/事件与命令授权必须共用同一份判定，不得各写一套"
  - "`exportIds` 不进入 Node Link wire；Access 侧不理解、也拿不到该清单，只按 catalog 可见集工作"
  - "迁移必须版本化、单事务、可重复、字节稳定；版本过新拒绝打开；`check:contract-drift` 逐条断言必须全绿"
  - "既有信任行的清单一律置空（不得默认放权），与 v1 → v2 的 `grants_json` 处理口径一致"
  - "合同资产与文档同批同步：`schemas/local-admin/v1`、`fixtures/local-admin/v1`、`docs/**`，并让 `npm run check` / `npm run verify` 全绿"
non_goals:
  - "不做「配对后修改可见清单」的入口（如 `node.trust.set-export-ids`），本次只在配对确认时填报"
  - "不新增审计动作（沿用节点配对既有的 `node.paired`），不扩大 `SECURITY_DESIGN.md` §14.2 的闭合词表"
  - "不改 Access 侧 `import.add` 的参数与语义，不改「没有 catalog 快照 → `local.unavailable`、有快照但无该 id → `local.not_found`」的区分"
  - "不实现 `catalog.changed` 增量推送，不改 catalog 批次与其它协商限额"
  - "不触碰 Sync、ACP 与前端面"
success_criteria:
  - "本机 `node pair confirm` 可带零个或多个 `--export-id`，`node list` 能看到该节点的清单"
  - "清单之外的 Export 对该节点不可见，即使其 scopes ∩ grants 非空；catalog 与资源访问结论一致"
  - "清单内 Export 被撤销后不可见；重新上架得到新 id，旧 id 不会复活"
  - "v3 库升级到 v4 后清单为空、既有会话/审计/信任数据与序列保留、第二次打开逐字节不变"
  - "`npm run verify` 与 `node scripts/check-crate-boundaries.mjs` 全绿"
decision_bounds:
  - "可自主决定：存储列为 JSON 的具体编码与列名、迁移语句与测试组织、CLI 参数写法与帮助文本、文档措辞、错误码选用（在既有 `local.*` 词表内）"
  - "需用户决策：改变「收窄而非放宽」的方向、把清单放进 wire、新增配对后修改方法或新增审计动作"
assumptions:
  - "项目未上线（用户已确认），因此不做兼容窗口、双写或旧客户端混用；既有本机/dev 配对在升级后需要重新确认才能恢复可见，这是方案 A 的预期代价"
  - "`node.pair.confirm` 的 `exportIds` 为必需字段，按 v1 内合同修订处理（先例：`node.challenge.catalogRevision`）；若用户要求可选/带默认值，需改本行与 specs"
```

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
