# Tasks

## 1. WP1：Sync wire、错误词表与 `SYNC_PROTOCOL.md`

- [x] 1.1 在 `schemas/sync/v1/pairing.schema.json` 增加 `lookupRequest`/`lookupResponse` 两个 `$defs` 并加入 `oneOf`，字段与 design D4 一致；`npm run check:schemas` 通过。
- [x] 1.2 在 `compatibility/errors/v1/errors.json` 增加 `pairing.code_invalid`（`retryable: false`、`details: null`、`delivery: "mvp"`），并同步错误 schema/文档中的枚举；`npm run check:errors` 通过。
- [x] 1.3 增加 `fixtures/sync/v1/valid/pairing-lookup-request.json`、`pairing-lookup-response.json` 与至少覆盖「缺 `pairingCode`」「码长度错误」「未知字段」「未知错误码」的 invalid fixture；`npm run check:schemas` 通过。
- [x] 1.4 在 `crates/sync-protocol` 增加 lookup DTO、schema 往返与负例测试；`cargo test -p sync-protocol` 全绿，且 claim/status 的既有测试不变。
- [x] 1.5 在 `docs/SYNC_PROTOCOL.md` §7.1 写清「两种入口、一个状态机」，新增 lookup 小节（请求/响应、成功即消耗、统一 `pairing.code_invalid`、独立限流、no-store、secret 不落日志），并登记网页入口的主机公钥来自 lookup；`npm run check:docs` 通过。

## 2. WP2：`identity-auth` 配对码派生、单次消耗与清理

- [x] 2.1 实现 `derive_pairing_code(secret)`：域分离 HMAC-SHA256、取前 40 bit、Crockford base32 8 字符；自己实现编解码与 `I/L→1`、`O→0` 归一化，不新增依赖；固定向量测试至少 3 组；`cargo test -p identity-auth` 全绿。
- [x] 2.2 在 `Authority` 的 secret 生命周期内维护 `code -> PairingId` 内存映射，查找使用常数时间比较；成功查找**原子移除**该码，第二次查找失败；测试覆盖单次消耗与并发/重复查找。
- [x] 2.3 创建时若派生码与另一个活跃配对碰撞，重新生成 secret/码直到唯一；测试覆盖碰撞路径且用户侧不出现错误。
- [x] 2.4 `due_pairings`、`unrecoverable_after_restart`、`reset_memory`、批准后清除 secret 时同步移除码映射；测试覆盖「过期清 secret 后码失效」「重启不恢复」「拒绝后到原过期时间失效」。
- [x] 2.5 在 `docs/IDENTITY_AND_AUTH_CONTRACT.md` §4.3/§4.4/§5.1 登记配对码的生命周期、单次消耗与公开入口（由 WP5 统一写回时核对）。

## 3. WP3：lookup 端点、管理面、CLI 与过期清理接线

- [x] 3.1 在 `server::sync::pairing` 注册 `POST /sync/v1/pairing/lookup`：Origin/Host、`application/json`、16 KiB、四个安全头、`no-store`；统一 401 `pairing.code_invalid`；lookup 与 claim 各自 10 次/分钟；成功即消耗码且不推进配对状态；集成测试覆盖成功/单次消耗/格式错误/过期/已认领/来源不符/独立限流。
- [x] 3.2 在 daemon 的配对过期维护中调用 `Authority::due_pairings`/`unrecoverable_after_restart`，启动路径调用 `reset_memory`；测试断言过期后内存 secret 与码映射被清除，且仍有效的配对不受影响。
- [x] 3.3 `device.pair.begin` 返回值增加 `pairingCode`（`server::local_admin` + 组合根），不新增持久字段；管理面测试断言字段存在、status/list 不再返回它、码被消耗后 confirm/status 仍按配对状态工作。
- [x] 3.4 `device pair` 同时打印 `pairingUrl` 与 `XXXX-XXXX` 配对码，保留 `y/N` 确认；CLI 测试断言两种输出、确认流程与「码被网页消耗后 CLI 行为不变」；`cargo test -p app` 全绿。
- [x] 3.5 在 `docs/LOCAL_ADMIN_PROTOCOL.md` §5.3 与 CLI 流程段落写回 `pairingCode`、单次消耗与「URL + 码并存」（由 WP5 统一写回时核对）。

## 4. WP4：PWA `/pair` 配对码入口（对齐原型 S0–S12）

- [x] 4.1 `PairingPageModel` 增加 `awaiting_code`/`looking_up` 与 `code_invalid`，阶段与原型 S0–S12 对应；覆盖合法/非法码、lookup 失败、终态清理；`npm run test` 中模型用例全绿。
- [x] 4.2 S0 渲染 8 个单字符格、canonical origin 与 HTTPS 标识；支持大小写不敏感、`I/L→1`、`O→0`、粘贴 `XXXX-XXXX` 或连续 8 位；格式不符在发请求前失败；码不进 URL/Cookie/localStorage/sessionStorage。
- [x] 4.3 S0 不显示数字倒计时，只显示静态“配对码 5 分钟内有效”；lookup 成功后立即用响应 `expiresAt` 启动倒计时；服务端返回过期时停止轮询并进入对应呈现。
- [x] 4.4 lookup 失败（含响应丢失、码已被消耗）固定呈现「配对码不对或已过期，请重新生成配对码」，不回显后端 `message`、不显示配对是否存在、不提供页内重试。
- [x] 4.5 `/node-link/pair` 无 fragment 仍显示无载荷，不显示配对码表单、不发 lookup；二维码 fragment 路径回归不变。
- [x] 4.6 增加浏览器 e2e：S0–S12、固定失败文案、正确码走完 lookup→claim→approved→目录、二维码路径不回归；`cd clients/app && node e2e/run-e2e.mjs` 通过。

## 5. WP5：安全、前端、管理与身份合同文档 + ADR

- [x] 5.1 在 `docs/SECURITY_DESIGN.md` §5.1 登记网页入口的 TOFU 边界、允许任意 canonical origin 的理由、与移动端带外锚的差异、40 bit + 独立限流的理由、单次 lookup 的响应丢失代价；`npm run check:docs` 通过。
- [x] 5.2 在 `docs/FRONTEND_DESIGN.md` 更新配对页的两种入口、S0–S12 呈现、S0 无倒计时与 lookup 后按服务端 `expiresAt` 启动倒计时、固定失败文案与「码不落盘」约束。
- [x] 5.3 在 `docs/LOCAL_ADMIN_PROTOCOL.md` §5.3/§5.8 写回 `pairingCode`、单次消耗与 CLI 输出；在 `docs/IDENTITY_AND_AUTH_CONTRACT.md` §4.3/§4.4/§5.1 写回配对码生命周期、单次消耗、碰撞去重与公开入口。
- [x] 5.4 新增 `docs/adr/0010-web-pairing-code-entry.md`：记录“网页入口用一次性配对码 + canonical origin 下的首次信任；移动端保留带外主机公钥”的决策、被否决的压缩 fragment/SAS/6 位纯数字，以及单次 lookup、“S0 无倒计时、lookup 后按服务端时间”的后果。
- [x] 5.5 核对 `docs/SYNC_PROTOCOL.md`（WP1 写回）与上述文档没有互相矛盾的「第二套协议」描述；`npm run check:docs` 与 `npm run check:openspec` 通过。

## 6. 合入与验证

- [x] 6.1 按 `plan.md` 的 Merge Order 串行 `git apply --check` → `git apply`（WP1→WP2→WP3→WP4→WP5），每次合入后重跑该包 Verify。
- [x] 6.2 主检出执行 `npm run verify`、`cd clients/app && npm run check`、`npm run check:openspec`，全绿。
- [x] 6.3 每个工作包由独立 `reviewer` spawn 检视并闭环；changes-requested 走新 spawn 修复，最多 3 轮。
- [x] 6.4 按 `docs/WORKFLOW.md` §6.2 开分支、推远端、`gh pr create --fill`、等六个必需检查全绿后 `gh pr merge --squash`。
