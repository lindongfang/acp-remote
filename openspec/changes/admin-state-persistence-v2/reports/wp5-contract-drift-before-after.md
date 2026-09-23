# WP5 · 合同漂移门禁改前/改后（W0）

命令：`node scripts/check-contract-drift.mjs`（仓库根，只读脚本）

## 改前（红）

退出码：1

```text
contract drift: §7 块 1 (OWNED_SCHEMA_V1) 第 14 条语句不一致
  文档: create table owned_audit ( audit_id integer primary key autoincrement, at text not null, action text not null check (act
  代码: create table owned_audit ( audit_id integer primary key autoincrement, at text not null, action text not null check (act
contract drift: §7 块 2 (IMPORTED_SCHEMA_V1) 第 5 条语句不一致
  文档: create table imported_audit ( audit_id integer primary key autoincrement, at text not null, action text not null check (
  代码: create table imported_audit ( audit_id integer primary key autoincrement, at text not null, action text not null check (
contract drift: trait TrustStore 的方法集 不一致
  仅文档: asyncfnclaim_pairing(&self,claim:PairingClaim,at:Timestamp)->Result<PairingClaimOutcome,PortError>
  仅文档: asyncfncreate_pairing(&self,pairing:PairingRecord)->Result<(),PortError>
  仅文档: asyncfnexpire_pairings(&self,at:Timestamp)->Result<u64,PortError>
  仅文档: asyncfnnode(&self,id:&NodeId)->Result<Option<NodeRecord>,PortError>
  仅文档: asyncfnrevoke_device(&self,id:&DeviceId,at:Timestamp,reason:RevokeReason)->Result<(),PortError>
  仅文档: asyncfnrevoke_node(&self,id:&NodeId,at:Timestamp,reason:RevokeReason)->Result<(),PortError>
  仅文档: asyncfnsettle_pairing(&self,id:&PairingId,settlement:PairingSettlement,at:Timestamp)->Result<TrustRecordRef,PortError>
  仅文档: asyncfnupsert_device(&self,record:DeviceRecord,at:Timestamp)->Result<(),PortError>
  仅文档: asyncfnupsert_node(&self,record:NodeRecord,at:Timestamp)->Result<(),PortError>
  仅代码: asyncfnclaim_pairing(&self,write:PairingClaimWrite)->Result<PairingClaimOutcome,PortError>
  仅代码: asyncfncreate_pairing(&self,write:PairingWrite)->Result<(),PortError>
  仅代码: asyncfnexpire_pairings(&self,write:ExpiryWrite)->Result<u64,PortError>
  仅代码: asyncfnnode(&self,id:&NodeId,kind:NodeKind)->Result<Option<NodeRecord>,PortError>
  仅代码: asyncfnnodes_for(&self,id:&NodeId)->Result<Vec<NodeRecord>,PortError>
  仅代码: asyncfnpairing_peer(&self,id:&PairingId)->Result<Option<PairingPeer>,PortError>
  仅代码: asyncfnpeer_key(&self,peer:&PeerIdentity)->Result<Option<PeerPublicKey>,PortError>
  仅代码: asyncfnput_device(&self,write:DeviceWrite)->Result<(),PortError>
  仅代码: asyncfnput_node(&self,write:NodeWrite)->Result<(),PortError>
  仅代码: asyncfnrevoke_device(&self,write:DeviceRevocation)->Result<(),PortError>
  仅代码: asyncfnrevoke_node(&self,write:NodeRevocation)->Result<(),PortError>
  仅代码: asyncfnsettle_pairing(&self,write:PairingSettlementWrite)->Result<TrustRecordRef,PortError>
contract drift: trait ExportStore 的方法集 不一致
  仅文档: asyncfnremove_import(&self,id:&ImportId,at:Timestamp)->Result<(),PortError>
  仅文档: asyncfnrevoke_export(&self,id:&ExportId,at:Timestamp)->Result<(),PortError>
  仅文档: asyncfnupsert_export(&self,export:ExportRecord,at:Timestamp)->Result<(),PortError>
  仅文档: asyncfnupsert_import(&self,import:ImportRecord,at:Timestamp)->Result<(),PortError>
  仅代码: asyncfnadd_import(&self,write:ImportWrite)->Result<(),PortError>
  仅代码: asyncfnput_export(&self,write:ExportWrite)->Result<(),PortError>
  仅代码: asyncfnremove_import(&self,write:ImportRemoval)->Result<(),PortError>
  仅代码: asyncfnrevoke_export(&self,write:ExportRevocation)->Result<(),PortError>

```

## 改后（绿）

退出码：0

```text
contract drift OK: §7 的 36 条 DDL 与 crates\storage-sqlite\src\migrate.rs 逐条一致；§5 的 15 个 trait / 87 个方法签名与 crates\core\src\ports.rs 一致

```

对照：改前的 3 类差异（§7 块 1 第 14 条 `owned_audit` 的 CHECK、§7 块 2 第 5 条 `imported_audit` 的 CHECK、`TrustStore`/`ExportStore` 方法集）全部消失。语句条数：改前两侧都是 24 条（owned 18 = 9 表 + 9 索引，imported 6 = 5 表 + 1 索引）；落地 v2 后代码侧先变为 36 条（owned 29、imported 7），文档侧同步到同样的 36 条；`§5` 侧文档 15 个 trait / 87 个方法签名与 `crates/core/src/ports.rs` 相等。这两组计数由本脚本在每次 `npm run check` 里重算，条数下降或解析退化会被脚本自己的下限断言挡住。
