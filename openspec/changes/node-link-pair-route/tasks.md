# Tasks

## 1. 定取舍

- [x] 1.1 核实现状并留证据：前端路由清单（`expo export` 产物）与服务端二维码 URL 的构造处（`node.pair.begin`），并说明 `/node-link/pair` 为何落到单页回退（保留前缀口径）。完成判据：报告含两侧的 `文件:行` 与一句结论。完成证据：二维码 URL 构造在 `crates/server/src/local_admin/pairing.rs:202`（注释口径）与 `:225`（实际拼接 `/node-link/pair`，sync 侧对应 `:175` 的 `/pair`）；前端路由由文件约定决定，`clients/app/app/` 下只有 `index.tsx`、`pair.tsx`、`hosts.tsx`、`dir/[alias].tsx`、`session/[id].tsx`，**没有** `node-link/` 目录；保留前缀只到版本段 `/node-link/v1`，因此该请求落到静态面的单页回退、入口文档被返回，而 expo-router 匹配不到路由 ⇒ 空页面。
- [x] 1.2 在 design.md 选①前端补路由或②服务端改 URL，列备选与影响（含是否触碰 `docs/NODE_LINK_PROTOCOL.md` §13.1）。完成判据：Decision 与 Risks 可被 reviewer 逐条核对。完成证据：design 的 D1 取备选①（前端补路由）：备选②要改二维码内容里的路径，而路径属 §13.1 内容格式 ⇒ 触碰协议文档与 proposal 的非目标；备选①不动二维码内容、协议文档与服务端，只加一个前端路由（静态面单页回退本就覆盖任意非协议路径）。影响面与风险（路由路径与保留前缀的关系、互不混淆的判定边界、必须有 DOM 级断言）已写进 Risks。

## 2. 实现与回归

- [ ] 2.1 按取舍实现。完成判据：打开 `/node-link/pair#data=…` 真的进入可用配对入口（DOM 断言），而改回原状时断言变红。
- [ ] 2.2 补一条「两类入口互不混淆」的断言。完成判据：sync 与 node-link 二维码各走各自流程，交叉喂错内容时明确失败而不是误入流程。
