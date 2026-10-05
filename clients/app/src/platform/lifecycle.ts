/**
 * 生命周期端口的**声明**（`docs/FRONTEND_DESIGN.md` §3 的 Platform Ports）。
 *
 * 归属 WP5b。页面**不能**直接监听 `document`/`window` 的生命周期事件
 * （`AGENTS.md` §5「页面不能直接操作 … 平台生命周期」），因此这里把
 * 「可见性变化 / 即将卸载 / 内存压力」收敛成端口，由 WP6 的状态机与 WP7 的页面消费。
 *
 * v1 只交付 Web（PWA）：`lifecycle.web.ts` 是唯一实现，**不写** `.native.ts`
 * （原生端的前后台切换、系统终止与恢复按 `docs/FRONTEND_DESIGN.md` §8.2 属后续变更）。
 */

/** 页面可见性：与 `docs/FRONTEND_DESIGN.md` §5 的连接状态机配合使用。 */
export type VisibilityState = "visible" | "hidden";

/** 生命周期事件。 */
export type LifecycleEvent =
  /** 页面变为前台/后台。后台时不应假装已在线，也不应把输入标记为已发送。 */
  | { readonly kind: "visibility"; readonly state: VisibilityState }
  /** 页面即将被卸载（`pagehide`/`beforeunload`）：用于释放连接、丢弃未完成快照。 */
  | { readonly kind: "pagehide" };

/** 取消订阅。 */
export type Unsubscribe = () => void;

/**
 * 生命周期端口。
 *
 * 只暴露订阅与只读查询：订阅回调由平台层在**真实**事件上触发，
 * 消费方无法伪造一次 `visibility` 事件（那会让状态机相信一个不存在的平台事实）。
 */
export interface LifecyclePort {
  /** 订阅生命周期事件；返回取消订阅函数。 */
  subscribe(listener: (event: LifecycleEvent) => void): Unsubscribe;
  /** 当前可见性；非浏览器环境（测试/SSR）按 `visible` 返回。 */
  currentVisibility(): VisibilityState;
  /**
   * 请求持久化存储（`navigator.storage.persist()`）。
   *
   * `docs/FRONTEND_DESIGN.md` §4.5：无论获批与否，浏览器清除数据都按密钥丢失处理，
   * 因此返回的布尔值只用于诊断与提示，**不得**被当作「身份一定还在」的依据。
   */
  requestPersistentStorage(): Promise<boolean>;
}
