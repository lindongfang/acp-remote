<!-- 技术方案：解释「怎么修」。动机见 proposal.md（Why / What Changes），行为契约见 specs/pwa-web-client/spec.md。 -->

# Design

## Context

- 协议要求（`docs/SYNC_PROTOCOL.md` §7.1）：配对入口校验「当前 origin 与 secure context」——二维码里的 `canonicalOrigin` 必须与本页实际来源一致，且页面必须处于安全上下文（PWA 形态即 https 或 loopback）。
- 现状（修复前）：这两条都没实现。配对入口是 `clients/app/src/features/client-store.ts` 的 `#beginPairing`（唯一入口）：它已在那里读完载荷（`readQrPayload`）并在**发任何请求之前**校验载荷类型（`kindMismatchFailure`，本仓库刚合入的 `node-link-pair-route` 建立的口径）。来源不符的失败面现在只在**事后**（`ensureIdentity` 的来源绑定检查，`pairing-model.ts` 已有 `auth.origin_mismatch` 这条码与提示文案），而不具备安全上下文的运行形态会一路走到 `crypto.subtle` 失败或静默降级。
- 前端配对入口自己的失败码是小集合（`pairing-model.ts` 的 `PAIRING_ERROR_HINTS` 只覆盖 `protocol.schema_invalid`、`auth.origin_mismatch`，另有 `internal.unavailable`），且这些码来自**唯一机器定义**的错误码表（`compatibility/errors/v1/errors.json`，前端 union 在 `src/protocol/common.ts` 逐条对应）。该表是封闭词表：新增码必须同步 schema enum 与 fixture 并让 `npm run check` 通过。
- 环境事实的读法有既有先例：`composition.ts` 直接注入/读取一次性的浏览器能力（`fetch`、`crypto.subtle`），而有状态的东西走 `src/platform/**` 的端口。`src/platform/` 的成员表是**冻结的装配面**（该文件注释明确不在其中加成员）。

## Goals / Non-Goals

**Goals:**

- 两条校验都落在**唯一入口**、且在发任何配对请求之前收口。
- 失败**可区分**：来源不符与非安全上下文各自有独立、可读、不含配对秘密的原因。
- 断言可证伪：去掉任一条判定，对应用例变红。

**Non-Goals:**

- 不改服务端（Owner 侧已按规范校验 canonical origin 与绑定）；不改协议语义与 wire 消息。
- 不实现「非安全上下文下仍能配对」的替代路径（规格明确要求拒绝，且不得用可导出形式的密钥替代）。
- 不动 `src/platform/**` 的冻结成员表。

## Decisions

**D1 —— 两条校验都放在 `#beginPairing`（唯一入口），顺序为「安全上下文 → 来源一致 → 载荷类型」。**
理由：`#beginPairing` 已经是「读完载荷、发任何请求之前」的唯一收口点，放这里就不可能有第二条绕过路径；顺序把「环境根本不满足」与「内容对不上」分开，用户看到的第一条原因就是最根本的那条。类型校验保持在最后，因为它需要已读出的载荷。

**D2 —— 环境事实（是否安全上下文、当前来源）经 `CompositionInputs` 注入，默认实现读浏览器全局。**
理由：非安全上下文与跨来源这两个形态在正常测试环境里造不出来，必须可注入才能写成断言；而 `src/platform/**` 的成员表是冻结的、不属本变更范围。既有先例是 `fetchImpl`/`crypto.subtle` 这类一次性能力的注入方式（与有状态的 `platform` 端口分工不同），因此注入面放在组合根输入而不是平台端口。

**D3 —— 失败码：来源不符复用既有的 `auth.origin_mismatch`；非安全上下文**新增** `auth.insecure_context`（按封闭词表的流程同步注册表、schema enum 与 fixture）。**
理由：`auth.origin_mismatch` 的语义与文档口径正是「来源不符」，直接复用、零新增；而非安全上下文没有语义贴合的既有码——`capability.unsupported_by_client` 在仓库里被 `degradation-model.ts` 用作「内容/能力降级」的三分类之一（客户端/服务端/Agent），借到配对入口会把两套语义搅在一起。因此新增一个 `auth.*` 码：`auth.origin_mismatch` 本身就是「客户端页面侧前置条件」被登记进该表的先例，所以这一步与词表口径一致，而不是把客户端本地概念塞进协议语义。新增必须走 `AGENTS.md` §6/§9 的同步清单（`compatibility/errors/v1/errors.json` + schema enum + fixture + `npm run check`）。

**D4 —— 来源比较用协议既有的归一化口径，不新造字符串比较。**
理由：`canonicalOrigin` 在协议里有明确的规范形态（§3.2 的归一化），前端已有该口径的实现（身份绑定检查用的就是它）；随手比较端口、尾部斜杠或大小写会误判，也会与 §7.1 的要求不一致。

**D5 —— 非安全上下文的失败不得回退到可导出密钥或静默降级。**
理由：规格明文要求；且本仓库的安全不变量是「PWA 私钥不得存入 localStorage、能力不足时只能进入显式开发模式」。

## Risks / Trade-offs

- [新增错误码的合同面] → 必须同步注册表、schema enum 与 fixture 并让 `npm run check` 全绿；漏一处 `check:errors` 会红（这是有意的门禁）。
- [注入面放在组合根而非平台端口] → 若将来 `src/platform/` 的解冻把环境事实纳入端口，这里需要跟着搬一次；本变更选择不动冻结面。
- [失败文案] → 文案必须可读且**不回显配对秘密**；沿用既有「任意异常文本不透传」的口径（任务 1.3）。
- [判据只测文案] → 三条断言都必须落到可判定事实（失败态 + claim/status 计数为 0 + 秘密不出现在文案或错误对象里）。

## Migration Plan

不涉及数据迁移、schema（除错误码登记这一处合同资产）或部署步骤。回滚 = 逆向该提交；回滚后回到「来源不符只在事后被身份绑定拦下、非安全上下文一路走到 WebCrypto 失败或静默降级」。

## 已知文档缺口（随本变更更新）

- `docs/FRONTEND_DESIGN.md` §4.5 与该变更登记的「未实现」条目要改写成实绩（任务 2.1）。
- `docs/SYNC_PROTOCOL.md` §7.1 的实现现状登记（proposal 的 Impact 提到）如仍写「前端未实现」，一并改为实绩；§7.1 的行为要求本身不变。
