/**
 * 附件选择/预览的**平台端口**（PWA 实现见 `attachment.web.ts`）。
 *
 * ## 为什么端口在平台层、形状却在 feature 层重复声明
 *
 * `src/layers.ts` 禁止 `platform/` 被内层依赖，也禁止平台层 import 更外层的 `features/`。
 * 因此这里声明一份**结构等价**的端口形状；`features/ports.ts` 的 `AttachmentMediaPort`
 * 结构上兼容它，组合根直接传入即可（与 `DeviceIdentityLike` 同一套做法）。
 *
 * ## 不透明句柄
 *
 * 选择器返回的句柄带展示元数据，真正的字节留在平台实现里（`File`/`Blob`）。上层只在
 * `readChunk` 里按 32 KiB 读取，因此「选择阶段只看本机预览、点击发送后才上传」在类型上就成立。
 */

/** 附件种类（与协议 `mediaReference.kind` 同义）。 */
export type PlatformAttachmentKind = "image" | "audio" | "file";

/** 一次选择得到的一项（句柄的字节不在这里）。 */
export interface PlatformAttachmentHandle {
  /** 页内稳定标识；一次选择内唯一。 */
  readonly localId: string;
  readonly kind: PlatformAttachmentKind;
  readonly fileName: string;
  readonly mediaType: string;
  /** 十进制字符串（平台不把它读成 number 再交回）。 */
  readonly byteLength: string;
}

export interface PlatformAttachmentPort {
  /** 打开平台选择器；取消返回空数组（不是错误）。 */
  pick(kind: PlatformAttachmentKind): Promise<readonly PlatformAttachmentHandle[]>;
  /** 从粘贴事件里取出图片/文件；没有可粘贴内容返回空数组。 */
  fromPaste(event: unknown): Promise<readonly PlatformAttachmentHandle[]>;
  /** 读取 `[offset, offset+length)`（调用方保证 32 KiB 以内）。 */
  readChunk(handle: PlatformAttachmentHandle, offset: bigint, length: number): Promise<Uint8Array>;
  /** 选择阶段的本机预览 URL；平台不支持返回 `null`。 */
  previewUrl(handle: PlatformAttachmentHandle): string | null;
  /** 媒体字节 → 对象 URL；平台不支持返回 `null`。 */
  createObjectUrl(bytes: Uint8Array, mimeType: string): string | null;
  /** 立即撤销对象 URL。 */
  releaseUrl(url: string): void;
}
