/**
 * node-link 配对页（路由 `/node-link/pair`）。
 *
 * ## 为什么这一页存在
 *
 * 服务端为本机 node-link 配对生成的二维码是
 * `https://<origin>/node-link/pair#data=<base64url-json>`（`NODE_LINK_PROTOCOL.md` §13.1，
 * 构造处 `crates/server/src/local_admin/pairing.rs` 的 `node_pairing_url`）。该路径**不在**协议
 * 保留前缀内（保留前缀只到版本段 `/node-link/v1`），因此静态面的单页回退会把入口文档交给浏览器
 * ——此时若没有本文件，expo-router 匹配不到任何路由，扫码打开的就是一个空页面。
 *
 * ## 它与 `/pair` 的关系（design D2/D4）
 *
 * 页面主体直接复用 `/pair` 的 `PairingRouteBody`，只把**期望的配对类型**声明成 `node-link`：
 * 展示与提交流程因此只有一份实现，两类入口的差别只有「本页声明的是哪一类载荷」。
 * 把 sync 设备配对内容喂到这里（或反过来）会**在读载荷之后、发任何请求之前**明确失败，
 * 而不是误入另一类流程。
 *
 * ## 本客户端对 node-link 载荷做什么
 *
 * 只做「读到并呈现」：node-link 的 claim / status（§13.2 / §13.3）由节点发起，浏览器不是它的
 * 参与方，因此本页不发任何 node-link 请求，也不开配对会话——载荷里的 `pairingSecret` 用到即弃，
 * 呈现的只有公开的对端信息（端点 host 派生的 https 来源与 owner 节点标识）。
 *
 * ## 路由路径与协议前缀的关系（design 的 Risks）
 *
 * `/node-link/pair` 与保留前缀 `/node-link/v1` 同段但不同版本段，两者不冲突。若将来协议要占用
 * `/node-link/*` 的其它段（例如把 `pair` 收进版本段之下），本路由需要重新评估——登记见
 * `docs/FRONTEND_DESIGN.md` §4.1。
 */

import type { ReactElement } from "react";

import { PairingRouteBody } from "../pair";

export default function NodeLinkPairRoute(): ReactElement {
  return <PairingRouteBody kind="node-link" />;
}
