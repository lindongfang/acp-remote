/**
 * 「运行时未装配」的显式说明。
 *
 * ## 为什么需要它，而不是让页面渲染空数据
 *
 * 组合根（平台端口 + 同步客户端的装配）尚未接线时，store 为 `null`。若页面照常渲染
 * 「本机没有任何目录」，用户会把它读成「这台电脑上确实没有目录」——那是 R15 的
 * **第一种**空态，与事实不符，也可能诱导用户去删配置。
 *
 * 因此这里给第三种、语义完全不同的呈现：明确说明客户端运行时尚未装配，并保留
 * `data-runtime="unavailable"` 供浏览器用例定位。
 */

import type { ReactElement } from "react";

export interface RuntimeUnavailableProps {
  /** 缺失装配的路由名，便于定位。 */
  readonly route: string;
}

/** 运行时未装配提示。 */
export function RuntimeUnavailable({ route }: RuntimeUnavailableProps): ReactElement {
  return (
    <section data-route={route} data-runtime="unavailable" role="alert">
      <h1>同步运行时未装配</h1>
      <p>
        本页面需要组合根注入平台端口（设备身份、本地缓存、生命周期）与同步客户端后才能连接电脑。
        在装配完成前，这里不会显示任何目录、会话或 Agent 数据——以免把「尚未连接」显示成
        「本机没有任何目录」。
      </p>
    </section>
  );
}
