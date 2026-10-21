/**
 * 设备身份与安全存储的**端口**（`docs/FRONTEND_DESIGN.md` §3 的 Platform Ports）——
 * 转出 [`./secure-storage.shared`]。
 *
 * 为什么本文件只剩一句转出：共享声明（接口、领域取值、错误类型、存储键常量）必须住在一个
 * **没有平台变体**的模块里。Metro/Expo 在 Web 平台把 `./secure-storage` 解析到
 * `secure-storage.web.ts`，若共享声明留在本文件，Web 实现里「从 `./secure-storage` 取
 * `DEVICE_IDENTITY_STORE`/`SecureStorageError`」在真实 Web 产物里会解析到 Web 实现本身，取到
 * `undefined`（同批在 `local-cache` 上实测到 `assertLocalSummary is not a function`；
 * `npm run check` 的单测与假主机 e2e 走另一套解析，抓不到这个形态）。
 * 见 [`./secure-storage.shared`](./secure-storage.shared.ts) 的文档。
 */

export * from "./secure-storage.shared";
