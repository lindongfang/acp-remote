/**
 * 配对页（路由 `#/pair`，`data-route="pair"`）。
 *
 * 配对是 HTTPS 面（`schemas/sync/v1/pairing.schema.json`）：传输端口由组合根用 `fetch`
 * 实现，本页只呈现状态机产出的模型。设备私钥由 `src/platform/` 生成且不可导出，
 * 因此本页没有任何凭据输入。
 */

import { useRouter } from "expo-router";
import type { ReactElement } from "react";

import { PairingPanel, RuntimeUnavailable } from "../src/components";
import { initialPairingPageModel } from "../src/features/pairing-model";
import { useClientState, useClientStore } from "../src/features/runtime";

export default function PairRoute(): ReactElement {
  const store = useClientStore();
  const state = useClientState();
  const router = useRouter();

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
        onRestart={() => {
          store.setPairing(initialPairingPageModel());
        }}
      />
    </main>
  );
}
