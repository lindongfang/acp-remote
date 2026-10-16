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
 * MCP 配置与 Agent 凭据一律在电脑上配置——因此本页没有**凭据**输入项。
 *
 * 唯一的输入是**一次性**的二维码内容（design D3 的粘贴入口）：它由 store 校验后立即用于
 * claim 与轮询，不进页面状态；地址片段里的那一份读完即被清除（§7.1）。
 *
 * ## 页面不解析、不读地址栏
 *
 * 结构校验、claim、轮询、落盘与「已配对后触发连接」都在 `ClientStore` 与组合根里
 * （`store.submitPairingQrText` / `store.startPairingFromAddress`）：本页不碰 `fetch`、
 * 不读地址栏、不写任何存储（`AGENTS.md` §6：平台能力只由更外层装配）。
 */

import { useRouter } from "expo-router";
import { useEffect } from "react";
import type { ReactElement } from "react";

import { PairingPanel, RuntimeUnavailable } from "../src/components";
import { initialPairingPageModel } from "../src/features/pairing-model";
import type { PairingKind } from "../src/features/pairing-model";
import { useClientState, useClientStore } from "../src/features/runtime";

/**
 * 配对入口的页面主体，两类入口**共用**（design D4：复用同一组件、以类型参数区分，不复制一套 UI）。
 *
 * `kind` 是**本页路由声明的期望类型**（design D2），随片段的读取与粘贴的提交一起交给 store：
 * 载荷类型与它不一致时 store 明确失败且不发任何请求。`app/node-link/pair.tsx` 用
 * `kind="node-link"` 复用这一份主体，因此两类入口的展示与提交流程只有一份实现。
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

  if (store === null || state === null) return <RuntimeUnavailable route="pair" />;

  return (
    <main>
      <nav>
        <button type="button" data-action="back-to-dirs" onClick={() => router.back()}>
          返回目录
        </button>
      </nav>
      <PairingPanel
        model={store.pairingPage()}
        onSubmit={(text) => {
          void store.submitPairingQrText(text, kind);
        }}
        onRestart={() => {
          store.setPairing(initialPairingPageModel());
        }}
      />
    </main>
  );
}

export default function PairRoute(): ReactElement {
  return <PairingRouteBody kind="sync" />;
}
