# Proposal

## Why

`docs/SYNC_PROTOCOL.md` §7.1 要求配对入口校验「当前 origin 与 secure context」：二维码里的 `canonicalOrigin` 必须与**本页实际所处的来源**一致，且页面必须处于安全上下文（PWA 形态下即 https 或 loopback）。前端目前**没有**实现这两条校验——变更 `frontend-sync-wiring` 的 WP2 已把它如实登记为未做的相邻项（`plan.md` 的「真机验收记录」与 §4.5 的实现现状都记着）。

现状的风险不是理论值：配对秘密与主机材料都随二维码内容进来，若不校验「我是不是在与二维码声明的那台主机说话」，用户把另一个来源的二维码粘进来的失败面是**事后**（`ensureIdentity` 的来源不符）而不是**入口处**，而 secure context 更是从来没人查过——不具备安全上下文的运行形态（非 https、非 loopback 的来源）会一路走到 WebCrypto 失败或静默降级。

## What Changes

- 在配对入口（粘贴/片段读取处）补两条校验：① 二维码的 `canonicalOrigin` 与当前页面的来源一致；② 当前页面处于安全上下文。
- 两条都不满足时**明确失败**（进入可区分的失败态，不发任何请求），文案不得回显配对秘密。
- 补断言：两条各自在实现被回退时变红；并覆盖「来源不符」「非安全上下文」两种失败形态。

## Capabilities

### New Capabilities

（无。）

### Modified Capabilities

- `pwa-web-client`：新增「配对入口校验来源与安全上下文」要求。

## Impact

- `clients/app/src/composition.ts`、`clients/app/src/features/pairing-model.ts` 与其测试。
- `docs/FRONTEND_DESIGN.md`（§4.5 的身份边界）、`docs/SYNC_PROTOCOL.md` §7.1 的实现现状登记。
- 与 `ui-csp-inline-styles`、`node-link-pair-route` 无重叠（不同文件）。
