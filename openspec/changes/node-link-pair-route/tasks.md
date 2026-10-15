# Tasks

## 1. 定取舍

- [ ] 1.1 核实现状并留证据：前端路由清单（`expo export` 产物）与服务端二维码 URL 的构造处（`node.pair.begin`），并说明 `/node-link/pair` 为何落到单页回退（保留前缀口径）。完成判据：报告含两侧的 `文件:行` 与一句结论。
- [ ] 1.2 在 design.md 选①前端补路由或②服务端改 URL，列备选与影响（含是否触碰 `docs/NODE_LINK_PROTOCOL.md` §13.1）。完成判据：Decision 与 Risks 可被 reviewer 逐条核对。

## 2. 实现与回归

- [ ] 2.1 按取舍实现。完成判据：打开 `/node-link/pair#data=…` 真的进入可用配对入口（DOM 断言），而改回原状时断言变红。
- [ ] 2.2 补一条「两类入口互不混淆」的断言。完成判据：sync 与 node-link 二维码各走各自流程，交叉喂错内容时明确失败而不是误入流程。
