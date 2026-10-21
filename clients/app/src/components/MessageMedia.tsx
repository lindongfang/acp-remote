/**
 * 消息里的类型化媒体（图片预览 / 音频播放 / 文件下载 / 资源链接）。
 *
 * ## 只消费专用视图
 *
 * 输入是 `domain/media.ts` 投影出的 `MediaBlockView`，不是 `acp.rawJson`：客户端不解析 ACP 原文
 * 去补媒体（诊断抽屉里的原文只按不可信文本呈现）。
 *
 * ## 读取走已认证的附件连接
 *
 * 图片/音频的字节由页面通过 `onLoad(attachmentId, sessionId, mimeType)` 请求（仓库用分块读取
 * 生成 Blob URL），本组件只渲染 `ready` 状态里的 URL。对象 URL 的生命周期由内容自己负责：
 * 组件卸载时 `onRelease` 让仓库立即撤销它，不留下永久公开地址。
 *
 * ## 不可用是显式事实
 *
 * `expired` / `not_fetched` / `unsupported` 各有独立说明；没有嵌入内容（`attachmentId === null`）
 * 时也不会画一个空 `<img>`。外部资源（`resource`）只显示 URI 与「未自动获取」，
 * 渲染本身不发起任何网络请求（没有 `<a href>` 自动加载、没有 `<img src=http…>`）。
 */

import { useEffect, useRef } from "react";
import type { ReactElement } from "react";

import type { AttachmentMediaView, MediaBlockView } from "../domain/media";
import { MEDIA_UNAVAILABLE_LABELS } from "../domain/media";
import type { Palette } from "./theme";
import { Pressable, StyleSheet, Text, View, fontFamily, fontSize, palettes, radius, spacing, useTheme } from "./theme";

export interface MessageMediaProps {
  /** 这条消息的媒体块（按到达顺序）。 */
  readonly media: readonly MediaBlockView[];
  /** 已读取的媒体：`attachmentId → 状态`（来自 `ConversationPageModel.media`）。 */
  readonly sources: ReadonlyMap<string, AttachmentMediaView>;
  /** 归属会话：读取路径要求会话绑定。 */
  readonly sessionId: string | null;
  /** 请求读取一条媒体（页面接到仓库的 `loadAttachmentMedia`）。 */
  readonly onLoad?: ((attachmentId: string, sessionId: string, mimeType: string) => void) | undefined;
  /** 释放一条媒体的对象 URL（组件卸载时调用）。 */
  readonly onRelease?: ((attachmentId: string) => void) | undefined;
  /** 文件下载入口（页面负责生成 Blob URL 并触发保存）。 */
  readonly onDownload?: ((attachmentId: string, mimeType: string, fileName: string) => void) | undefined;
}

/** 按调色板建样式表：浅/深各建一次（模块加载时）。 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    root: {
      gap: spacing.xs,
      marginTop: spacing.xs,
    },
    block: {
      gap: spacing.xs,
      padding: spacing.sm,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
    },
    caption: {
      color: colors.ink3,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
    },
    action: {
      alignSelf: "flex-start",
      paddingVertical: 2,
      paddingHorizontal: spacing.sm,
      borderWidth: 1,
      borderColor: colors.accent,
      borderRadius: radius.sm,
      backgroundColor: colors.accentSoft,
    },
    actionText: {
      color: colors.accentInk,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    failed: {
      color: colors.danger,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

/**
 * 图片/音频的读取生命周期。
 *
 * 只在「可用、有附件标识、还没读过」时请求一次；卸载时释放对象 URL。`sources` 的状态由仓库
 * 产生，因此重复渲染不会重复读取（`ready`/`failed` 都不是触发条件）。
 *
 * ## 回调身份不参与依赖
 *
 * 页面把 `onLoad`/`onRelease` 写成内联箭头（每次父组件 commit 都是新函数）。若把它们放进依赖，
 * 每次重渲染都会先执行 cleanup → 释放对象 URL / 清掉 `loading` 状态 → 再重新 `onLoad`，
 * 读结果因代次变化被丢弃，媒体永远到不了 `ready`，并形成 commit 环。因此这里用 ref 固定
 * **最新**回调：effect 只在附件标识（和读取状态）真正变化时运行，cleanup 只依赖 `attachmentId`。
 */
function useAttachmentSource(
  attachmentId: string | null,
  sessionId: string | null,
  mimeType: string,
  source: AttachmentMediaView | undefined,
  onLoad: MessageMediaProps["onLoad"],
  onRelease: MessageMediaProps["onRelease"],
): AttachmentMediaView | undefined {
  const onLoadRef = useRef(onLoad);
  const onReleaseRef = useRef(onRelease);
  // 在 effect 里更新 ref（不在渲染期写）：本轮 commit 的 effect 按声明顺序执行，因此下面的
  // 读取/释放 effect 读到的总是本次渲染的回调。
  useEffect(() => {
    onLoadRef.current = onLoad;
    onReleaseRef.current = onRelease;
  });
  useEffect(() => {
    if (attachmentId === null || sessionId === null || source !== undefined) return;
    onLoadRef.current?.(attachmentId, sessionId, mimeType);
  }, [attachmentId, sessionId, mimeType, source]);
  useEffect(() => {
    if (attachmentId === null) return;
    return () => {
      onReleaseRef.current?.(attachmentId);
    };
  }, [attachmentId]);
  return source;
}

/** 一条媒体的呈现。 */
export function MessageMedia({
  media,
  sources,
  sessionId,
  onLoad,
  onRelease,
  onDownload,
}: MessageMediaProps): ReactElement | null {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  if (media.length === 0) return null;

  return (
    <View style={styles.root} dataSet={{ messageMedia: "true", messageMediaCount: String(media.length) }}>
      {media.map((block, index) => (
        <MediaBlock
          key={`${block.kind}:${index}`}
          block={block}
          sources={sources}
          sessionId={sessionId}
          onLoad={onLoad}
          onRelease={onRelease}
          onDownload={onDownload}
        />
      ))}
    </View>
  );
}

/** 单个媒体块：按 `kind` 分派到图片/音频/文件/资源/不支持五种形态。 */
function MediaBlock({
  block,
  sources,
  sessionId,
  onLoad,
  onRelease,
  onDownload,
}: {
  readonly block: MediaBlockView;
  readonly sources: ReadonlyMap<string, AttachmentMediaView>;
  readonly sessionId: string | null;
  readonly onLoad: MessageMediaProps["onLoad"];
  readonly onRelease: MessageMediaProps["onRelease"];
  readonly onDownload: MessageMediaProps["onDownload"];
}): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

  if (block.kind === "unsupported") {
    return (
      <View style={styles.block} dataSet={{ mediaBlock: "unsupported", mediaOriginalType: block.originalType }}>
        <Text style={styles.caption}>
          {`当前客户端不支持这类内容（原始类型：${block.originalType}）。`}
        </Text>
      </View>
    );
  }

  if (block.kind === "resource") {
    return (
      <View style={styles.block} dataSet={{ mediaBlock: "resource", mediaUri: block.uri }}>
        <Text style={styles.caption}>{`资源：${block.name ?? block.uri}`}</Text>
        <Text style={styles.caption}>{MEDIA_UNAVAILABLE_LABELS.not_fetched}</Text>
      </View>
    );
  }

  if (block.availability !== "available") {
    return (
      <View style={styles.block} dataSet={{ mediaBlock: block.kind, mediaAvailability: block.availability }}>
        <Text style={styles.failed}>{MEDIA_UNAVAILABLE_LABELS[block.availability]}</Text>
      </View>
    );
  }

  if (block.kind === "file") {
    return (
      <View style={styles.block} dataSet={{ mediaBlock: "file", mediaMimeType: block.mimeType }}>
        <Text style={styles.caption}>
          {`文件：${block.displayName ?? block.attachmentId}（${block.mimeType}，${block.byteLength} 字节）`}
        </Text>
        {onDownload === undefined ? null : (
          <Pressable
            role="button"
            style={styles.action}
            dataSet={{ action: "download-attachment", attachmentId: block.attachmentId }}
            onPress={() => {
              onDownload(block.attachmentId, block.mimeType, block.displayName ?? block.attachmentId);
            }}
          >
            <Text style={styles.actionText}>下载文件</Text>
          </Pressable>
        )}
      </View>
    );
  }

  return (
    <AttachmentMediaBody
      block={block}
      sources={sources}
      sessionId={sessionId}
      onLoad={onLoad}
      onRelease={onRelease}
    />
  );
}

/** 图片/音频的载体：读取生命周期在**独立组件**里（hook 不能放在条件分支之后）。 */
function AttachmentMediaBody({
  block,
  sources,
  sessionId,
  onLoad,
  onRelease,
}: {
  readonly block: Extract<MediaBlockView, { kind: "image" | "audio" }>;
  readonly sources: ReadonlyMap<string, AttachmentMediaView>;
  readonly sessionId: string | null;
  readonly onLoad: MessageMediaProps["onLoad"];
  readonly onRelease: MessageMediaProps["onRelease"];
}): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const source = useAttachmentSource(
    block.attachmentId,
    sessionId,
    block.mimeType,
    block.attachmentId === null ? undefined : sources.get(block.attachmentId),
    onLoad,
    onRelease,
  );

  return (
    <View
      style={styles.block}
      dataSet={{
        mediaBlock: block.kind,
        mediaMimeType: block.mimeType,
        mediaAttachmentId: block.attachmentId ?? "none",
        mediaState: source?.kind ?? (block.attachmentId === null ? "no_content" : "idle"),
      }}
    >
      {block.attachmentId === null ? (
        <Text style={styles.caption}>
          {`这条 ${block.kind === "image" ? "图片" : "音频"}内容没有随消息下发嵌入字节，只有外部地址。`}
        </Text>
      ) : source === undefined || source.kind === "loading" ? null : source.kind === "failed" ? (
        <Text style={styles.failed}>{`读取失败：${source.detail}`}</Text>
      ) : block.kind === "image" ? (
        <img
          src={source.url}
          alt="消息图片"
          data-message-media-image="true"
          data-message-media-mime={source.mimeType}
          style={{ maxWidth: "100%", maxHeight: "50vh", objectFit: "contain", borderRadius: 6 }}
        />
      ) : (
        <audio controls src={source.url} data-message-media-audio="true" data-message-media-mime={source.mimeType} />
      )}
    </View>
  );
}
