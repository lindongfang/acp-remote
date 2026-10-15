# Proposal

## Why

服务端为本机 node-link 配对生成的二维码指向 `/node-link/pair#data=…`（`crates/server/src/local_admin/pairing.rs` 的 `node.pair.begin`），而前端路由表只有 `/`、`/pair`、`/hosts`、`/dir/[alias]`、`/session/[id]`（`expo export` 产物的 `.expo/types/router.d.ts` 逐字如此，见变更 `frontend-sync-wiring` WP3 的核实与独立检视复核）。

`/node-link/pair` **不在协议保留前缀内**（保留前缀只到版本段 `/node-link/v1`，取已注册协议 path 的前两段），因此该请求落到单页回退 → 入口文档 → expo-router 匹配不到任何路由：**扫 node-link 二维码打开的是一个空路由**，配对流程根本进不去。

这条是 WP3 核实路由形态时发现的相邻缺口，属 node-link 能力的前端部分，不在 `frontend-sync-wiring` 的写范围内，本变更单独立项。

## What Changes

- 二选一并落定：① 前端提供该路径的路由（复用配对页，把 node-link 与 sync 两类配对入口区分开）；或 ② 服务端把 node-link 二维码指向已有的前端路由。取舍写进 design.md。
- 补断言：给定 node-link 二维码内容，打开后**必须落在可用的配对入口**上（而不是未匹配路由），并在路径被改错时变红。
- **非目标**：不改 `docs/NODE_LINK_PROTOCOL.md` §13.1 的二维码内容格式与 node-link 配对语义。

## Capabilities

### New Capabilities

（无。）

### Modified Capabilities

- `pwa-web-client`：新增「两类配对二维码都能落到可用入口」要求。

## Impact

- `clients/app/app/`（路由）**或** `crates/server/src/local_admin/pairing.rs`（二维码 URL 的构造）——取决于 design 的取舍。
- `docs/NODE_LINK_PROTOCOL.md` §13.1、`docs/FRONTEND_DESIGN.md`（若改变 URL 形态）。
- 既有前端 e2e（`clients/app/e2e/**`）的路由夹具可能需要同步。
