# Tasks

## 1. 实现与断言

- [x] 1.1 在配对入口补「`canonicalOrigin` 与当前来源一致」的校验：不满足即失败且不发请求。完成判据：单元测试断言来源不符时 claim/status 计数为 0，且把该判定去掉时用例变红。完成证据：`client-store.ts` 的唯一入口 `#beginPairing` 里对 sync 载荷比较两侧归一化后的来源（`pairing-model.ts` 的 `canonicalOriginOf`），不符即落 `auth.origin_mismatch`；用例 `pairing-model.test.ts:673` 断言失败态 + 可读文案 + `claims/polls/scheduled` 全 0。判别力探针：把该判定改成恒假后，**该用例与「D1 顺序」「1.3 秘密不透传」共 3 条变红**（主检出实测）。
- [x] 1.2 在配对入口补「安全上下文」校验：不满足即明确失败。完成判据：单元测试注入非安全上下文，断言失败文案可读且不发请求；去掉判定时用例变红。完成证据：环境事实（`secureContext`/`origin`）经组合根输入注入、缺省惰性读 `globalThis.isSecureContext` / `location.origin`（未动 `src/platform/**` 的冻结成员表）；用例 `pairing-model.test.ts:718` 注入非安全上下文，断言 `auth.insecure_context`（本次新增码）+ 文案可读 + 零请求；e2e 新增 **NL-3** 同样覆盖（非安全上下文与来源不符都明确失败且零请求）。
- [x] 1.3 断言失败文案不含配对秘密（沿用既有「任意异常文本不透传」的口径）。完成判据：注入带 secret 的异常，断言它不出现在文案或错误对象里。完成证据：用例 `pairing-model.test.ts:760` 断言两条前置失败路径的 `error` 与 `state` 序列化都不含 `QR_SECRET`；沿用既有 `publicErrorForQrInput` 对异常文本的口径。判别力：让异常文本透传即变红（上述探针实测该条会一起红）。

## 2. 文档

- [x] 2.1 把 `docs/FRONTEND_DESIGN.md` §4.5 与该变更登记的「未实现」条目改写成实绩（或保留为「待裁决」并说明）。完成判据：`npm run check:docs` 与 `npm run check:drift` 全绿。完成证据：`docs/FRONTEND_DESIGN.md` §4.5 的「未实现（相邻项）」整句改写为实绩（并加修订记录行）；`docs/SYNC_PROTOCOL.md` §7.1 追加一句实现现状登记（行为要求原文未动）；根 `npm run check` 退出码 0（含 `check:docs`、`check:drift`、`check:errors` 与新增错误码的四处一致）。
