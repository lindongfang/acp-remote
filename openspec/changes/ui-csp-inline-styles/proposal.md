# Proposal

## Why

真机验收（变更 `frontend-sync-wiring` 的 Main E2E，2026-10-16）发现：PWA 由 Daemon **同源托管**时，接入层给出的 CSP 是

```text
default-src 'self'; script-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'
```

**没有** `style-src 'unsafe-inline'`，而前端产物依赖**内联样式**：同一个页面实测

- `document.querySelectorAll('style')` → 两条，其中 379 字符那条的 `sheet === null`（被 CSP 拦下）；
- `document.styleSheets.length === 0`。

即**真实托管下 UI 无样式**。而 `clients/app` 的本地门禁（vitest + `expo export`）与假主机 e2e（自己起静态服务、不带这套 CSP）都抓不到这条——这正是它一直没人发现的原因。

同一批验收清单的第 7 条明确要求「若样式异常，必须把现象与处理方式写进报告；**不得在 Rust 侧放宽 CSP 掩盖**」。因此本变更把「前端必须在既定 CSP 下可用」立成一条要求，而不是去改 CSP。

## What Changes

- 让前端在**既定 CSP** 下呈现完整样式：不再依赖被该策略拦下的内联样式形态（具体做法在 design 阶段定，例如构建期产出受允许的外部样式表、或由托管层按自身策略注入）。
- 补一条**在真实 CSP 下**的可证伪断言：页面带该 CSP 加载后断言关键样式真的生效，而不是只断言 DOM 里有 `<style>` 元素。
- **非目标**：不放宽 CSP（不新增 `style-src 'unsafe-inline'`），不改协议、安全模型或静态面的响应头集合。

## Capabilities

### New Capabilities

（无。）

### Modified Capabilities

- `ui-asset-hosting`：新增「同源托管下前端样式在既定 CSP 内生效」要求（含真实 CSP 下的可证伪场景）。

## Impact

- `clients/app`（样式策略与构建产物）。
- `crates/server/src/transport/net/static_ui.rs`（CSP 的唯一定义处）：**预期不改**；若结论必须是改 CSP，先按 `AGENTS.md` §2 请求确认并同步 `docs/SECURITY_DESIGN.md`。
- `docs/FRONTEND_DESIGN.md`、`README.md` 的前端现状；现象与实测已记在 `openspec/changes/frontend-sync-wiring/plan.md` 的「真机验收记录」。
