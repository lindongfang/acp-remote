/**
 * 配对页（路由 `/pair`，`data-route="pair"`）。
 *
 * ## 路由走正常路径，片段只承载配对内容（design D4）
 *
 * 配对二维码是 `<origin>/pair#data=<base64url-json>`：秘密**必须**在片段里
 * （`SYNC_PROTOCOL.md` §7.1），而 Expo Router 的路由匹配只看 `pathname`（片段与查询都不进
 * 路径），因此两者不争用同一个位置。本页因此不把片段当路由：原型把路由写在片段里
 * （`prototypes/README.md`），照抄那套口径的注释与这里的实际行为矛盾，已改为正常路径。
 * 核实方式与产物证据见 `src/routing.test.ts` 的文件头（构建产物、类型化路由与路由器的
 * 链接实现三处一致）。
 *
 * 配对是 HTTPS 面（`schemas/sync/v1/pairing.schema.json`）：传输端口由组合根用 `fetch`
 * 实现，本页只呈现状态机产出的模型。设备私钥由 `src/platform/` 生成且不可导出，
 * MCP 配置与 Agent 凭据一律在电脑上配置——因此本页没有**凭据**输入项，也没有输入框：
 * 地址片段是**唯一**入口（design D1）。
 *
 * ## 页面不解析、不读地址栏
 *
 * 结构校验、claim、轮询、落盘、到期定时与「已配对后触发连接」都在 `ClientStore` 与组合根里
 * （`store.startPairingFromAddress`）：本页不碰 `fetch`、不读地址栏、不写任何存储
 * （`AGENTS.md` §6：平台能力只由更外层装配）。
 *
 * ## 页面不绘制外壳
 *
 * 共享外壳装在根布局里（`design.md` D1），由 `usePathname()` 判定配对入口不呈现全局顶栏
 * （`shell-model.ts` 的 `isPairingPath`）。本页因此也不再自建页头或返回入口：配对页就是
 * 一个居中的状态舞台（design D5）。审批通过后短暂呈现过渡，约 1.6 秒后自动 `replace` 到
 * 目录页（`app/index.tsx`，路由 `/`）——恢复路径由电脑重新生成的配对地址给出，而不是页内按钮。
 */

import { useRouter } from "expo-router";
import { useEffect } from "react";
import type { ReactElement } from "react";

import { PairingPanel, RuntimeUnavailable } from "../src/components";
import { StyleSheet, View, spacing } from "../src/components/theme";
import type { PairingKind } from "../src/features/pairing-model";
import { useClientState, useClientStore } from "../src/features/runtime";

/** 已批准后停留在这里的过渡时长（design D4 / 原型 S1）。 */
const APPROVED_REDIRECT_MS = 1_600;

/**
 * 页面骨架：只有一块居中的舞台。
 *
 * 配色由共享外壳与 `PairingPanel` 承担，本页不设背景色，也不提供主题切换（design D5）。
 */
const STYLES = StyleSheet.create({
  page: {
    flex: 1,
    alignItems: "center",
    justifyContent: "center",
    padding: spacing.lg,
  },
});

/**
 * 配对入口的页面主体，两类入口**共用**（design D4：复用同一组件、以类型参数区分，不复制一套 UI）。
 *
 * `kind` 是**本页路由声明的期望类型**（design D2），随片段的读取交给 store：
 * 载荷类型与它不一致时 store 明确失败且不发任何请求。`app/node-link/pair.tsx` 用
 * `kind="node-link"` 复用这一份主体，因此两类入口的展示流程只有一份实现。
 */
export function PairingRouteBody({ kind }: { readonly kind: PairingKind }): ReactElement {
  const store = useClientStore();
  const state = useClientState();
  const router = useRouter();

  // 二维码地址直接打开这一页时，片段里带着配对内容：读一次（并立即清除片段）即进入与本页
  // 对应的流程。依赖只有 `store` 与 `kind`：组合根异步装配完成后才可用，而片段只读一次
  // （读完已被清除），`kind` 是这一份主体的常量参数。
  useEffect(() => {
    void store?.startPairingFromAddress(kind);
  }, [store, kind]);

  const page = store?.pairingPage() ?? null;
  const approvedSync = page !== null && page.phase === "approved" && page.kind === "sync";

  // 审批通过后自动进目录（design D4）：不等连接进入 online，也不要求用户再点一次。
  // 阶段变化或卸载时清理，避免离开页面后仍发生一次导航。
  useEffect(() => {
    if (!approvedSync) return;
    const timer = setTimeout(() => {
      // 目录页就是根路由：`app/index.tsx` 匹配 `/`（`DIRS_ROUTE.path === "/"`）。
      router.replace("/");
    }, APPROVED_REDIRECT_MS);
    return () => {
      clearTimeout(timer);
    };
  }, [approvedSync, router]);

  if (store === null || state === null || page === null) {
    return <RuntimeUnavailable route="pair" />;
  }

  return (
    <View role="main" style={STYLES.page}>
      <PairingPanel model={page} />
    </View>
  );
}

export default function PairRoute(): ReactElement {
  return <PairingRouteBody kind="sync" />;
}
