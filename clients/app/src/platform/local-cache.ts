/**
 * 本地缓存与生命周期端口的**声明与纯策略**（R20 平台侧）—— 转出 [`./local-cache.shared`]。
 *
 * 为什么本文件只剩一句转出：共享声明（接口、固定常量、纯策略）必须住在一个**没有平台变体**的
 * 模块里。Metro/Expo 在 Web 平台把 `./local-cache` 解析到 `local-cache.web.ts`，若共享声明留在
 * 本文件，那么 `.web.ts` 与 `index.ts` 的「从 `./local-cache` 取共享符号」在真实 Web 产物里会解析到
 * Web 实现本身，取到 `undefined`（实测：配对写缓存时报
 * `assertLocalSummary is not a function`；`npm run check` 的单测与假主机 e2e 走另一套解析，
 * 抓不到这个形态）。见 [`./local-cache.shared`](./local-cache.shared.ts) 的文档。
 *
 * 本文件保留，是为了让「平台端口的声明入口」这个名字继续存在（原生/测试侧从它取共享符号，
 * Web 侧拿到的是 [`./local-cache.web`](./local-cache.web.ts)，后者同样从 shared 模块转出）。
 */

export * from "./local-cache.shared";
