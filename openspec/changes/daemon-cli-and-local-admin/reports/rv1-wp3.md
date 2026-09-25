# reviewer 报告 · RV1-WP3（WP3 全部代码，交付前 branch review）

```
task_id: 3.6
role: reviewer（独立检视子 Agent，非本变更实现者）
phase: work-package review
agent_context: 隔离子 Agent（deepseek/deepseek-flash）；只继承任务单与契约文档，不继承 WP3 实现对话；
  工具集只有 read/grep/find/ls + watchdog_diff，无 shell/git，全程只读
target_revision: 313d2a2（WP3 代码最终版；其后 9376e96 仅为主 Agent 记录变更）
base_revision: 2636dbe
scope: crates/server/src/transport/local/**、crates/server/src/local_admin/**、crates/server/tests/**、
  crates/server/Cargo.toml、根 Cargo.toml（members）、Cargo.lock、2.21 契约资产
  （schemas/local-admin/v1/envelope.schema.json、docs/LOCAL_ADMIN_PROTOCOL.md、
   scripts/check-catalog.mjs、fixtures/local-admin/v1/**）
checks: C1–C10 逐项静态核对（未执行命令，无 shell）
issues: RV1-WP3-F1…F6（F1/F2/F3/F4/F5 MINOR，F6 SUGGESTION）；无 CRITICAL/MAJOR
result: PASS
resource_cleanup: 未占用任何资源；工作树未被写入
```

## Review Context

- **隔离方式**：WP3 全部代码落地后独立派发，未参与实现对话；结论只从代码、机器合同与日志得出。
- **无 git/shell 的限制与替代方法**：无法读取 `2636dbe..313d2a2` 的真实 diff → 直接读目标版本文件全文（范围 1–4 逐文件读完），并用 `watchdog_diff` 确认 `crates/server/**`、根 `Cargo.toml`、`Cargo.lock` 无未提交改动，因此读到的 server 侧内容即 HEAD 已提交内容；HEAD↔313d2a2 的映射依据任务单声明 + 日志中测试名与代码逐条对得上（如 `node_revoke_retry_and_unknown_id_report_not_found`、`unimplemented_methods_answer_local_unsupported`、`hyphen_at_a_segment_boundary_is_a_naming_error`）作为旁证。
- 检视期间工作树另有**范围外的在途改动**（tasks.md 2.22 的 `storage-sqlite` `AuditStore`：未暂存 `admin/mod.rs` 与未跟踪 `admin/audit.rs`、`tests/admin_audit.rs`），不在本轮范围、不影响检视文件，仅登记为环境事实。
- 未执行 `cargo test`/`clippy`/`fmt`/`npm run check`，未执行 E2E（本版本无 `crates/app`，无端到端入口）；所有测试结论为**静态核对**（读断言 + 与日志/交接报告交叉），不是复跑结果。

## 十项重点核对（结论摘要）

1. **framing 关闭规则** —— 一致：1 MiB 上限（合法取等号，两侧测试断言）、空帧/未知 channel/混用 channel 关闭、提交上限 32（32 条全通 + 第 33 条关闭 + 计数回落）、**framing/协议级关闭路径全部 `break CloseReason`，错误帧不可能产生**（`assert_closed_without_frame` 在 6 个用例断言零字节输出）；`0x02` 在 facade 缺席期：结构化 warning + 即连即关，**不调用** `FacadeAttachmentRegistry::attach`，与 §3.1 实现状态注记逐句一致。
2. **endpoint 与访问控制** —— 一致：SID 哈希 = SHA-256 前 8 字节小写 hex（不附加换行/前缀，用例独立复算）、pipe/socket 命名与 `XDG_RUNTIME_DIR` 回落、Unix `0700`/`0600` 设后核对、符号链接与非 socket 拒绝、残留 socket 仅在连不上时删除；**对端凭据无旁路**：Windows/Unix 两条路径只有 SID/uid 精确相等才 `Ok`，其余分支先写 `authorization.denied` 再 `Err`，从不在拒绝时交出 stream。
3. **信封与 nil 哨兵** —— 一致：`v` 非 1 → 连接级关闭、closed object、canonical 小写 UUID、method 语法与词表、`params` 必须 object；nil 哨兵只出现在 3 个错误路径（`PayloadNotJson`/`NotAnObject`/`IdNotCanonicalUuid`），**无任何路径用 nil 构造成功响应**。
4. **路由只经 core 用例** —— 一致：`Actor::` 20 处全为 `Actor::LocalCli`；server 不依赖 `storage-sqlite`、无 sqlite/sqlx 标识符；两处不经用例者均为合同明示（`AuditStore::query` 与 `IdentityKeystore` 端口）；server 内判定只做协议错误码翻译，不复制业务规则。
5. **凭据与秘密** —— 一致（附 F6 加固）：`values` 只以 `SecretBytes` 进 `put_secret`，`result` 只回字段名；失败日志只带 method/code；测试双向断言响应不含明文、回滚用例注入含路径/SQL 的适配器文本并断言不泄漏；`pairingUrl`/secret/SAS 不落日志（`PairingSessions` 的 `Debug` 只手写 `has_public_origin`）；`audit.export` 固定 10 个 §14.2 元数据列。
6. **配对判定点** —— 一致：`local.expired`（rejected/expired 或过期）、`Conflict`（未认领/已完成）、`not_found`（未知 id/目标族不匹配）与 §5.3/§5.4/§6 一致；**无出站网络调用**（`mode="access"` 直接 `local.unsupported`，crate 无 HTTP 客户端）；`public_origin` 为 `None` 时在任何写入前失败关闭（断言三张表计数为 0）；确认集合越界 → `local.invalid_params` 且状态不推进；「内存已批准、库无信任」不可达（`mark_pairing_approved` 只在 `settle_pairing` 提交成功后调用，失败用例断言库无信任 + 内存保留可重试）。
7. **2.23 revoke 前置读取** —— 一致：三处都在调用撤销用例前把「不存在或已撤销」收敛为 `local.not_found`，成功形状未变；fake 已改为与存储同款 `COALESCE` 语义，重试断言**不再永真**（并断言重试不触发第二次关闭、首次撤销时间未被改写）。
8. **2.21 四处一致性** —— 一致：schema pattern + **恰好 25 项** enum、docs §4 同字面量 + §5.7 结构 + 文首 1.3 版本注记、门禁两处正则、Rust `Method::ALL`(25)/`is_method_name` 语义等价；`local.unsupported`（语法合法不在集内，如 `daemon.doctor`、`device.rotate-key.begin`）与 `local.invalid_request`（段首/段中/段尾连字符）两侧都有常驻测试与 fixture，且 manifest↔ajv↔Rust 漂移测试双向绑定。
9. **工程不变量** —— 一致：`crates/` 内无 Rust `unsafe`；非测试路径无 `unwrap/expect/panic/unreachable`；依赖边只落在 §5 矩阵 `server` 行允许的格子；无 `dbg!`/`println!`/`todo!`/`#[ignore]`/`#[should_panic]`。
10. **测试有效性抽查（8 条）+ 三项指定缺口** —— 8 条抽样均具反例能力（framing 三态、第 33 条上限、重复 in-flight id、revoke 重试、provider 回滚、origin 失败关闭、确认越界不推进、fixture↔manifest 双向）。缺口点名：`import.remove` **未知 id** 缺一条常驻断言（F1）；`export.create` 重试用例存在但**对本层查重 guard 不具反例能力**（F2）；`Method::NodeRotateKeyBegin` 三处覆盖，**无缺口**。

## Findings

### RV1-WP3-F1 · MINOR — `import.remove` 的「未知 id」缺一条常驻用例

- 位置：`crates/server/src/local_admin/router.rs:461-470`（`import_remove`）；测试 `router.rs:1900-1930` 区域。
- 事实：规格要求「`import.remove` 不存在时返回 `local.not_found`」（`specs/local-admin-methods/spec.md`、§7）；现有用例只覆盖「删除成功后再删」与 `"bad id"` → `local.invalid_params`，**没有**「从未存在过的合法形状 id」这条。
- 影响：映射路径相同（存储 `DELETE` + 行数判定），风险低，但合同把「未知 id」列为验收点。
- 最小修复：既有用例加一行 `ImportRemove {"importId": "import-missing"}` 断言 `LocalErrorCode::NotFound`。

### RV1-WP3-F2 · MINOR — `export.create` 重试用例对本层查重 guard 不具反例能力（fake 与真实存储在**相反方向**上不一致）

- 位置：`test_support.rs:493-505`（`FakeExports::put_export`：任何已存在 key 都 `Conflict(AlreadyExists)`）vs `crates/storage-sqlite/src/admin/export.rs:321-380`（真实实现：**只有已撤销行** `Conflict`；未撤销的既有 `export_id` 走 `ON CONFLICT DO UPDATE` 改写并 `Ok`）。
- 触发/预期/实际：`router.rs:327-338` 的查重是 §5.5「`exportId` 已存在 → `local.conflict`」在真实存储下**唯一**保障；测试在 fake 更严的语义下无条件通过 ⇒ 删掉 guard 后 `-p server` 仍全绿，而生产会退化成**静默覆盖既有 Export**。
- 影响：测试强度/证据链问题（当前生产代码正确），与 2.23 修掉的「fake 与存储不一致导致永真断言」同类、方向相反。
- 最小修复：`FakeExports::put_export` 与存储同形（未撤销 → 覆盖并 `Ok`，已撤销 → `Conflict(AlreadyExists)`），再验证「有 guard 绿、删 guard 红」。

### RV1-WP3-F3 · MINOR — Windows `accept` 的两个错误路径会让 endpoint 永久不可用

- 位置：`crates/server/src/transport/local/platform/windows.rs:67-80`。
- 触发/预期/实际：`self.pending.take()` 先取走唯一待连接实例；随后 `connect().await` 或补建下一实例的 `ServerOptions::new().create(...)` 若返回 `Err`，函数 `?` 直接返回且 `self.pending` 保持 `None` ⇒ 此后每次 `accept()` 只能返回 `EndpointError::Os("…没有可用的 pipe 实例")`，而代码注释声明的不变量是「无论本次连接是否被接受，endpoint 都必须继续可用」（该不变量只在凭据拒绝路径成立）。mio 文档明示 `connect()` 的正常 I/O 错误会立即返回（客户端连后立即断开即可触发），故 `Err` 可达。
- 影响：同 OS 用户（本人）在错误时刻断开一次，可能让管理通道此后不可用；若组合根把非 `PeerRejected` 错误视为致命，Daemon 会随之退出。**不是**安全问题（失败关闭、不放行连接），是可用性/健壮性不对称（Unix 侧 `listener.accept()` 失败后 listener 仍可用）。
- 最小修复：把「取实例 → connect → 补建下一实例」做成可重试一步（失败时先重建 `pending` 再返回错误），或改为「先补建、再 connect」。

### RV1-WP3-F4 · MINOR（契约对齐，已由主 Agent 收敛）— 「方法失败」未产生 §14.2 审计事件

- 位置：`router.rs:1119-1126`（失败分支只 `tracing::warn!(method, code)`）；对照 `docs/LOCAL_ADMIN_PROTOCOL.md` §7 末条与 spec 场景 WHEN 列表。
- 事实：`SECURITY_DESIGN.md` §14.2 的封闭词表**没有**「方法失败」类别（`AuditAction` 20 取值有测试钉死），误用 `authorization.denied` 会把参数错误与 scope 拒绝混为一谈；实现已在代码注释记录取舍。
- 主 Agent 处置：**采用合同侧收敛**——`docs/LOCAL_ADMIN_PROTOCOL.md` §7 改为「拒绝连接、§14.2 类别覆盖的安全动作与撤销记审计事件；方法失败只记结构化日志」，spec 场景 WHEN 列表同步去掉「方法失败」并注明原因。不新增审计类别（那需独立 ADR + §14.2 + `AuditAction` + 落库 CHECK 同批改动）。

### RV1-WP3-F5 · MINOR（契约对齐，已由主 Agent 收敛）— spec 的 `FacadeAttachmentId` MUST 句在本切片不成立

- 位置：`specs/local-admin-channel/spec.md` 的 channel 用途绑定 MUST 句与 attachment 生命周期场景；实现侧 `connection.rs:197-213`（即连即关、不 attach）。
- 事实：§3.1 实现状态注记与 design 决策 5 明确「不占用 `FacadeAttachmentId`」，但 MUST 句在 313d2a2 上没有可满足路径（只有 `attachment.rs` 单测）。
- 主 Agent 处置：在该 MUST 句加范围限定「**facade 落地后**；本切片 facade 缺席期不分配，见 §3.1 实现状态注记」，并在 verification 登记为接受的偏差。

### RV1-WP3-F6 · SUGGESTION（加固）— 凭据明文可被 `Debug` 打印；两处不可达枚举分支

- 位置①：`params.rs:317-326` —— `ProviderConfigure` derive `Debug` 且 `pub values: Vec<(String, String)>` 保存**明文凭据**（crate 私有类型，当前无调用点打印；与 core 对秘密的既有口径不一致，未来任一 `tracing::debug!(?configure)` 即泄漏）。最小修复：手写 `Debug`（只打印字段名与计数）或让 `values` 承载 `SecretBytes`。
- 位置②：`envelope.rs:255-262` 的 `MissingField{FIELD_ID}` 与 params 文案分支**不可达**（缺 id 走 `IdNotCanonicalUuid`、params 非 object 走 `ParamsNotObject`）。最小修复：删除或在 `message()` 里 `_ =>` 兜底并注释。

## Assessment

- **正确性**：C1–C9 逐条与合同一致，无 CRITICAL/MAJOR。三条最关键安全性质逐一独立复核：framing 错误**绝无**错误帧；对端凭据校验**无旁路**（成功分支要求 SID/uid 精确相等，失败分支先审计再关闭且不交出 stream）；路由**恒**以 `Actor::LocalCli` 走 core 用例（无 SQLite 直连、无第二套授权）。
- **契约一致性**：25 项方法词表四处一致；§3.1/§4/§5.7/§6/§7 的行为都能在代码中找到实现与测试；F4/F5 的措辞冲突已由主 Agent 收敛。
- **测试有效性**：8 条抽样均具反例能力；2.23 的 fake 保真修正有效；三项指定缺口 1 项缺断言（F1）、1 项断言强度不足（F2）、1 项无缺口。
- **范围外未核**：跨用户拒绝、Windows 后续实例 DACL 继承、Linux CI 执行、`cargo-deny`/`gitleaks` 均无法独立验证，不凭本报告认为通过。
- **结论：PASS**（无已确认 CRITICAL/MAJOR；F1/F2/F3 建议同变更内修，F6 加固；F4/F5 已收敛）。**Merge verdict：OK with notes**。

## 未覆盖 / 无法验证（明确声明）

1. 未读真实 diff（无 git/shell）；HEAD↔313d2a2 无法由 reviewer 自证。
2. 未执行任何命令/E2E；需主 Agent 在同版本执行并留证：`cargo fmt --all -- --check`、`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`、`cargo test --locked --workspace --all-features`、`npm run check`（重点 schemas/commands/docs/drift/boundaries）。仅 CI：`cargo-deny`、`gitleaks`（本地无等价物，未声称通过）。
3. 平台限制：跨用户拒绝、Windows 后续 pipe 实例 DACL 继承不可执行；Unix 路径仅静态阅读。
4. 范围外：`storage-sqlite` 的在途 2.22 改动、`vendor/windows-local-ipc/**`（由 RV1-WP2 覆盖）、WP4 的 `app`/CLI（尚不存在）。
5. 未复核 gitignored 日志的产出来源，只核对其内部一致性与代码/交接报告的相符性。

> 主 Agent 补注（executed）：3.5 的 `cargo fmt --all -- --check`、`cargo clippy`、`cargo test --locked --workspace --all-features`（含 workspace 全量）与 `npm run check` 已在 313d2a2 同一版本上执行并留证（`reports/wp3-final-verify.log`，`EXIT(npm run verify)=0`；Windows IPC 用例 `reports/pv5-windows-ipc.log`，`EXIT=0`）。上表第 2 条的 not-run 清单因此在其范围内已由主 Agent 补齐。

```yaml
handoff_index:
  - task_id: "3.6"
    role: reviewer
    phase: work-package review
    stage: work-package
    target_revision: "313d2a2"
    evidence_type: REVIEW
    evidence_id: RV1
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/rv1-wp3.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "WP3 全部代码在 313d2a2 上逐文件静态检视（transport::local 的 framing/endpoint/平台 ACL/attachment/audit hook、local_admin 的 envelope/error/method/params/router/pairing/daemon/audit/view/test_support、crates/server/tests 五个文件、两处 Cargo.toml 与 Cargo.lock、2.21 的 schema/docs/门禁/fixture）；结论基于十项重点核对与 6 条发现（F1/F2/F3/F4/F5 MINOR、F6 SUGGESTION，无 CRITICAL/MAJOR）。隔离：独立子 Agent、只读、无 shell/git，未继承实现对话；crates/server/** 与两处 Cargo 文件经 watchdog_diff 确认无未提交改动。限制：未读真实 diff、未跑测试与 E2E、跨用户拒绝与 Windows DACL 继承不可独立验证。"
    source_evidence: NOT_APPLICABLE
```
