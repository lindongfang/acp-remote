import { createElement } from "react";
import { createRoot } from "react-dom/client";
import { ConversationComposer } from "../../../src/components/ConversationComposer";
import type { ConversationInputModel } from "../../../src/features/conversation-input-model";
import { openAttachmentMediaPort } from "../../../src/platform/attachment.web";
import { expectEqual, expectThat, waitFor } from "../support";
import type { Check } from "../support";

export const COMPOSER_FILE_CHECKS: readonly Check[] = [{
  name: "COMPOSER-FILES: 添加与拖放只选择文件，不发送消息",
  async run() {
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);
    const port = openAttachmentMediaPort(() => crypto.randomUUID());
    const input: ConversationInputModel = {
      state: "editable", blockedBy: null, draft: "", canSubmit: true, disabledReason: null,
      attachmentsSupported: true, attachmentNotice: "", pathRefs: [],
      draftInMemoryOnly: false, draftNotice: null,
      submission: { kind: "none" }, cancel: { kind: "unavailable" },
      attachments: { attachments: [], phase: "empty", canSubmit: true, blockedReason: null,
        capabilityNotice: null, uploadedBytes: "0", totalBytes: "0", error: null },
    };
    let pickedKind: string | undefined;
    let droppedName: string | undefined;
    let picks = 0;
    let drops = 0;
    let submissions = 0;
    const render = (supported: boolean) => root.render(createElement(ConversationComposer, {
      input: { ...input, attachmentsSupported: supported },
      onPickAttachment: () => {
        picks += 1;
        void port.pick("file").then((files) => {
          pickedKind = files[0]?.kind;
        });
      },
      onPasteAttachments: (event) => {
        drops += 1;
        void port.fromPaste(event).then((files) => {
          droppedName = files[0]?.fileName;
        });
      },
      onSubmit: () => { submissions += 1; },
    }));
    const originalClick = HTMLInputElement.prototype.click;
    try {
      render(true);
      await waitFor(() => container.querySelector("[data-composer-drop-zone]") !== null, "输入区挂载");
      HTMLInputElement.prototype.click = function () {
        const transfer = new DataTransfer();
        transfer.items.add(new File(["image"], "photo.png", { type: "image/png" }));
        this.files = transfer.files;
        this.dispatchEvent(new Event("change"));
      };
      container.querySelector<HTMLButtonElement>('[data-action="pick-attachment"]')?.click();
      await waitFor(() => picks === 1 && pickedKind === "image", "点击添加并按 MIME 识别图片");
      const zone = container.querySelector("[data-composer-drop-zone]");
      expectThat(zone !== null, "拖放区域存在");
      const transfer = new DataTransfer();
      transfer.items.add(new File(["hello"], "notes.txt", { type: "text/plain" }));
      const drag = new DragEvent("dragover", { bubbles: true, cancelable: true, dataTransfer: transfer });
      zone?.dispatchEvent(drag);
      expectThat(drag.defaultPrevented, "允许放入文件");
      zone?.dispatchEvent(new DragEvent("drop", { bubbles: true, cancelable: true, dataTransfer: transfer }));
      await waitFor(() => drops === 1 && droppedName === "notes.txt", "拖放文件接入附件端口并保留文件名");
      expectEqual(submissions, 0, "选择与拖放不会发送消息");
      render(false);
      await waitFor(() => container.querySelector('[aria-disabled="true"][data-action="pick-attachment"]') !== null, "能力不可用时禁用添加");
      zone?.dispatchEvent(new DragEvent("drop", { bubbles: true, cancelable: true, dataTransfer: transfer }));
      expectEqual(drops, 1, "能力不可用时不接收拖放文件");
    } finally {
      HTMLInputElement.prototype.click = originalClick;
      root.unmount();
      container.remove();
    }
  },
}];
