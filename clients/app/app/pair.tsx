/**
 * 配对页（路由 `#/pair`，`data-route="pair"`）。
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
 * 不读 `location`、不写任何存储（`AGENTS.md` §6：平台能力只由更外层装配）。
 */

import { useRouter } from "expo-router";
import { useEffect } from "react";
import type { ReactElement } from "react";

import { PairingPanel, RuntimeUnavailable } from "../src/components";
import { initialPairingPageModel } from "../src/features/pairing-model";
import { useClientState, useClientStore } from "../src/features/runtime";

export default function PairRoute(): ReactElement {
  const store = useClientStore();
  const state = useClientState();
  const router = useRouter();

  // 二维码地址直接打开这一页时，片段里带着配对内容：读一次（并立即清除片段）即进入认领流程。
  // 依赖只有 `store`：组合根异步装配完成后才可用，而片段只读一次（读完已被清除）。
  useEffect(() => {
    void store?.startPairingFromAddress();
  }, [store]);

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
          void store.submitPairingQrText(text);
        }}
        onRestart={() => {
          store.setPairing(initialPairingPageModel());
        }}
      />
    </main>
  );
}
