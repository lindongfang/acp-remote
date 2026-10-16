<!-- 技术方案：解释「怎么修」。动机见 proposal.md（Why / What Changes），行为契约见 specs/pwa-web-client/spec.md。 -->

# Design

## Context

- 服务端为本机 node-link 配对生成的二维码内容：`https://<origin>/node-link/pair#data=<base64url-json>`（`crates/server/src/local_admin/pairing.rs:202`，构造于 `:225`）；sync 设备配对是 `https://<origin>/pair#data=…`（同文件 `:175`）。
- 前端路由（`clients/app/app/` 的文件约定）：`index.tsx`（`/`）、`pair.tsx`（`/pair`）、`hosts.tsx`、`dir/[alias].tsx`、`session/[id].tsx`。**没有** `/node-link/pair` 对应的路由文件。
- 为什么它落到空页面：`/node-link/pair` 不在协议保留前缀内（保留前缀只到版本段 `/node-link/v1`），因此静态面的单页回退把入口文档返回给浏览器，而 expo-router 匹配不到路由 ⇒ 空白。也就是说：**扫码打开的是一个空路由**。
- 前端已有的载荷读取口：`clients/app/src/composition.ts:106` 记录二维码片段标记（协议 §7.1 的 `/pair#data=<base64url-json>`），并有 `readQrPayload`（`composition.test.ts` 用它解析 `${ORIGIN}/pair#data=…`）；`clients/app/src/routing.test.ts` 覆盖路由口径。
- 约束：`docs/NODE_LINK_PROTOCOL.md` §13.1 定义二维码内容格式；proposal 的 Non-Goal 明确不改它。

## Goals / Non-Goals

**Goals:**

- 打开 `/node-link/pair#data=…` 落到**可用的配对入口**（可读的地址/主机信息 + 可提交的输入），而不是空路由。
- 两类入口**互不混淆**：sync 载荷与 node-link 载荷各走各自的流程；把一类的内容喂给另一类时**明确失败**，不得误入流程。
- 断言可证伪：路径被改错时用例变红。

**Non-Goals:**

- 不改二维码内容格式（§13.1）与 node-link 配对语义；不改服务端的 URL 构造（见 D1）。
- 不改 node-link 与 sync 的配对协议/载荷结构本身。
- 不放宽 CSP、不改静态面的响应头或路由回退口径。

## Decisions

**D1 —— 前端补路由（备选①），不改服务端 URL（备选②）。**
理由：备选②要改二维码内容里的路径，而路径是 §13.1 内容格式的一部分 ⇒ 触碰协议文档与非目标；备选①只加一个前端路由文件，二维码内容、协议文档、服务端全部不动，且静态面的单页回退本来就覆盖任意非协议路径 ⇒ 成本最小、影响面最窄。
影响：`clients/app/**` 与其测试；`docs/FRONTEND_DESIGN.md` 需登记新路由（客户端行为变化）。

**D2 —— 路由把「期望的配对类型」显式传给配对入口，而不是让入口从载荷猜。**
理由：规格的第二条场景要求「两类入口互不混淆」。若入口只看载荷形状去猜，一条被改写的载荷（或未来的新类型）会被另一类流程静默接受；由路由声明类型、入口校验载荷与声明是否一致，才能把「喂错内容」变成**明确失败**。

**D3 —— 载荷读取复用既有的 `readQrPayload` 口径，不新造解析。**
理由：片段标记（`#data=<base64url-json>`）与失败形态已在前端既有实现与其测试中定义；本变更只在它之外补「类型是否匹配」这一层。

**D4 —— 入口复用 sync 配对页的展示与提交流程，不复制一套。**
理由：两类配对的**界面**要求相同（显示对端地址/主机信息、接受用户输入并提交），差异在协议层类型 ⇒ 复用同一组件、以类型参数区分；复制会带来两份要保持同步的 UI。

## Risks / Trade-offs

- [路由路径本身看起来像协议前缀] → `/node-link/pair` 与保留前缀 `/node-link/v1` 同段但不同版本段，不冲突（已在 proposal 中说明）；若将来协议要占用 `/node-link/*` 的其它段，需要重新评估本路由——在 `docs/FRONTEND_DESIGN.md` 的登记里写明这一点。
- [两类入口互不混淆的判定边界] → 「明确失败」的形态要落到可断言的状态（失败态 + 文案不回显配对秘密），而不是静默停在引导页；实现时按既有失败态的风格给出，并在用例里锁定。
- [只加路由不会自动让 e2e 覆盖到] → 必须补一条**在真实静态面形态下**打开该 URL 的断言（DOM 级），否则「路由存在但入口不可用」这类回归无法被抓到。

## Migration Plan

不涉及数据迁移、schema 或部署步骤：前端新增一个路由文件并在入口处加类型校验。回滚 = 逆向该提交；回滚后 `/node-link/pair` 重新回到空路由。

## 已知文档缺口（随本变更更新）

- `docs/FRONTEND_DESIGN.md` 的客户端行为章节需登记新路由与「两类入口互不混淆」的口径（§9 的映射：客户端行为变化更新该文件）。
- `docs/NODE_LINK_PROTOCOL.md` §13.1 不变（D1 的选择就是不改它）。
