/**
 * 浏览器准备草稿端口的**声明入口**——转出 [`./preparation-draft.shared`]。
 *
 * 为什么本文件只是一句转出：共享声明必须住在**没有平台变体**的模块里。Metro 在 Web 平台
 * 把 `./preparation-draft` 解析到 `preparation-draft.web.ts`；若共享类型/常量留在这里，
 * Web 实现与 `platform/index.ts` 在真实 Web 产物里会取到 `undefined`（同批在
 * `local-cache`/`secure-storage` 上实测过）。见 [`./preparation-draft.shared`](./preparation-draft.shared.ts)。
 *
 * v1 只交付 Web：**不写** `.native.ts`（准备草稿属于本机 Web 准备流程）。
 */

export * from "./preparation-draft.shared";
