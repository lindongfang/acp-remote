# Tasks

## 1. 定取舍

- [x] 1.1 核实现状并留证据：前端路由清单（`expo export` 产物）与服务端二维码 URL 的构造处（`node.pair.begin`），并说明 `/node-link/pair` 为何落到单页回退（保留前缀口径）。完成判据：报告含两侧的 `文件:行` 与一句结论。完成证据：二维码 URL 构造在 `crates/server/src/local_admin/pairing.rs:202`（注释口径）与 `:225`（实际拼接 `/node-link/pair`，sync 侧对应 `:175` 的 `/pair`）；前端路由由文件约定决定，`clients/app/app/` 下只有 `index.tsx`、`pair.tsx`、`hosts.tsx`、`dir/[alias].tsx`、`session/[id].tsx`，**没有** `node-link/` 目录；保留前缀只到版本段 `/node-link/v1`，因此该请求落到静态面的单页回退、入口文档被返回，而 expo-router 匹配不到路由 ⇒ 空页面。
- [x] 1.2 在 design.md 选①前端补路由或②服务端改 URL，列备选与影响（含是否触碰 `docs/NODE_LINK_PROTOCOL.md` §13.1）。完成判据：Decision 与 Risks 可被 reviewer 逐条核对。完成证据：design 的 D1 取备选①（前端补路由）：备选②要改二维码内容里的路径，而路径属 §13.1 内容格式 ⇒ 触碰协议文档与 proposal 的非目标；备选①不动二维码内容、协议文档与服务端，只加一个前端路由（静态面单页回退本就覆盖任意非协议路径）。影响面与风险（路由路径与保留前缀的关系、互不混淆的判定边界、必须有 DOM 级断言）已写进 Risks。

## 2. 实现与回归

- [x] 2.1 按取舍实现。完成判据：打开 `/node-link/pair#data=…` 真的进入可用配对入口（DOM 断言），而改回原状时断言变红。完成证据：新增路由 `clients/app/app/node-link/pair.tsx`（复用 `/pair` 的入口主体，只声明 `kind="node-link"`）；e2e **NL-1** 在真实静态面形态下断言 `[data-pairing-phase]="node_link_scanned"`、地址可读（`https://work-pc.example.ts.net`）、输入非空后提交可用、零请求（`53/53` 通过）。回退探针：把该路由临时改成渲染空 ⇒ **NL-1 与 NL-2 都变红**（`51/53`，报「等待配对入口渲染超时」）；另 `src/routing.test.ts` 的文件↔路径清单在删文件/改错路径时也会在断言层变红。
- [x] 2.2 补一条「两类入口互不混淆」的断言。完成判据：sync 与 node-link 二维码各走各自流程，交叉喂错内容时明确失败而不是误入流程。完成证据：路由把**期望类型**显式传给唯一入口（`#beginPairing` 在读完载荷后、发任何请求之前校验 `read.kind !== expectedKind` ⇒ 落到 `failed`）；e2e **NL-2** 断言两个方向都 `failed` + `protocol.schema_invalid`、文案指出是哪一类、不含配对秘密、零请求；单测另覆盖纯函数层与「端点过正则但 `new URL` 解析失败 ⇒ 明确失败态」（后者是第 2 轮检视的 finding 1，已闭环）。
