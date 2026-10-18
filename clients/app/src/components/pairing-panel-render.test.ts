/**
 * 配对面板的静态呈现（tasks 4.2 / 4.4）。
 *
 * 状态测试钉住的是模型；这里钉住「模型 → 页面」这一层确实只渲染固定文案：
 *
 * - 误扫新设备 `q2=` 与旧设备 `data=` 两种片段都只显示各自的固定页面，片段内容（可能含取回
 *   凭据/配对秘密）没有任何进入 DOM 的通路，失败页也不提供提交按钮；
 * - 配对码输入态显示 origin、HTTPS 与静态提示，且**没有**数字倒计时（lookup 前没有服务端
 *   `expiresAt`）；
 * - 失败页只渲染模型里的固定 `presentation`，即便 `error.message` 带着内部细节也不会出现。
 *
 * 页面路由（`app/pair.tsx`）本身的挂载由浏览器用例覆盖（`src/**` 不 import `app/**`，
 * `src/layers.test.ts` 是硬判据）。
 */

import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { PairingPanel } from "./PairingPanel";
import {
  applyAwaitingCode,
  applyDeviceQrAppGuide,
  applyLegacyDeviceQrUnsupported,
  applyPairingFailure,
  initialPairingPageModel,
} from "../features/pairing-model";
import type { PairingPageModel } from "../features/pairing-model";

const ORIGIN = "https://work-pc.example.ts.net";

/** 一段不会被页面合法呈现的片段内容：既是取回凭据的形状，也带明显的哨兵文本。 */
const FRAGMENT_TEXT = "q2=AgABAgMEBQYHCAkKCwwNDg8QERITFBUWFxgZGhscHR4fSENTMl9TRU5USU5FTA";

function render(model: PairingPageModel): string {
  return renderToStaticMarkup(createElement(PairingPanel, { model }));
}

describe("配对面板：设备二维码片段的固定呈现", () => {
  it("误扫新设备短二维码：只显示 App 指引，片段内容不进 DOM、没有提交按钮", () => {
    const html = render(
      applyDeviceQrAppGuide(initialPairingPageModel("sync"), "sync"),
    );

    expect(html).toContain('data-pairing-phase="device_qr_app_only"');
    expect(html).toContain("请在 ACP Remote App 内扫码");
    expect(html).toContain("请在 ACP Remote App 内扫码，或返回网页使用配对码");
    // 片段内容（含取回凭据的载荷）没有任何渲染通路。
    expect(html).not.toContain(FRAGMENT_TEXT);
    expect(html).not.toContain("AgABAgMEBQYH");
    // 这条路径不发请求，页面也没有可提交的配对码入口。
    expect(html).not.toContain('data-action="submit-pairing-code"');
    expect(html).not.toContain('data-pairing-countdown="true"');
  });

  it("旧设备 data= 片段：明确提示已停用，不渲染片段内容", () => {
    const html = render(
      applyLegacyDeviceQrUnsupported(initialPairingPageModel("sync"), "sync"),
    );

    expect(html).toContain('data-pairing-phase="legacy_device_qr_unsupported"');
    expect(html).toContain("设备二维码旧格式已停用");
    expect(html).toContain("本页没有读取其中的任何内容");
    expect(html).not.toContain(FRAGMENT_TEXT);
    expect(html).not.toContain('data-action="submit-pairing-code"');
  });
});

describe("配对面板：配对码入口与失败文案", () => {
  it("配对码输入态：origin、HTTPS 与静态提示可见，lookup 前没有数字倒计时", () => {
    const html = render(applyAwaitingCode(initialPairingPageModel("sync"), "sync", ORIGIN, true));

    expect(html).toContain('data-pairing-phase="awaiting_code"');
    expect(html).toContain("输入配对码");
    expect(html).toContain('data-pairing-code-origin-value="https://work-pc.example.ts.net"');
    expect(html).toContain('data-pairing-secure="true"');
    expect(html).toContain("配对码 5 分钟内有效。");
    expect(html).toContain('data-pairing-code-entry="true"');
    expect(html).toContain('data-action="submit-pairing-code"');
    // 反例：输入态也渲染 `0:00` 倒计时 → 断言变红。
    expect(html).not.toContain('data-pairing-countdown="true"');
    expect(html).not.toContain("0:00");
  });

  it("非安全上下文：输入态给非 HTTPS 徽标，而不是把 HTTPS 写在页面上", () => {
    const html = render(applyAwaitingCode(initialPairingPageModel("sync"), "sync", ORIGIN, false));

    expect(html).toContain('data-pairing-secure="false"');
    expect(html).toContain("非 HTTPS");
  });

  it("失败页只渲染固定文案：底层 error.message 不会出现在 DOM 里", () => {
    const html = render(
      applyPairingFailure(
        applyAwaitingCode(initialPairingPageModel("sync"), "sync", ORIGIN, true),
        {
          code: "internal.unavailable",
          message: "内部细节：pairingSecret=AAECAwQFBgcICQoLDA0ODw",
          retryable: true,
          details: { pairingSecret: "AAECAwQFBgcICQoLDA0ODw" },
        },
      ),
    );

    expect(html).toContain('data-pairing-phase="failed"');
    expect(html).toContain("配对请求未能完成");
    expect(html).toContain("重新开始配对不会产生第二条配对记录。");
    // 固定文案之外没有任何后端正文/细节的渲染通路。
    expect(html).not.toContain("内部细节");
    expect(html).not.toContain("AAECAwQFBgcICQoLDA0ODw");
    // 错误码只作为诊断锚点保留。
    expect(html).toContain('data-pairing-error="internal.unavailable"');
    expect(html).not.toContain('data-action="submit-pairing-code"');
  });

  it("node-link 地址过期：DOM 只呈现节点专用的过期文案，没有配对码入口或配对码失败码", () => {
    const html = render(
      applyPairingFailure(initialPairingPageModel("node-link"), {
        code: "pairing.expired",
        message: "二维码已过期",
        retryable: false,
        details: {},
      }),
    );

    // 过期是入口给出的结论（终态），不是本侧失败；诊断码仍是真实的 `pairing.expired`。
    expect(html).toContain('data-pairing-phase="expired"');
    expect(html).toContain('data-pairing-error="pairing.expired"');
    expect(html).not.toContain("data-pairing-failure=");
    expect(html).toContain("配对地址已过期");
    expect(html).toContain("请在电脑上重新发起配对，并用新的节点地址打开本页。");
    // 反例：若过期仍走配对码口径 → 页面出现「配对码不对或已过期」与无法执行的输入指引。
    expect(html).not.toContain("配对码不对或已过期");
    expect(html).not.toContain("pairing.code_invalid");
    expect(html).not.toContain("生成新的配对码后再输入");
    expect(html).not.toContain('data-pairing-code-entry="true"');
    expect(html).not.toContain('data-action="submit-pairing-code"');
  });
});
