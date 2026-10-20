/**
 * 中间只读文件查看区（`session-file-browser` 5.2/5.3；spec「中间文件查看与关闭恢复」「只读预览与主动下载」）。
 *
 * ## 与来源对应的三种内容
 *
 * - `preview`：`session.files.read` 的有界字节块。文本按**不可信纯文本**呈现（不注入 HTML/SVG、
 *   不加载其中引用的远端资源）；图片只接受 PNG/JPEG/GIF/WebP，并通过平台端口创建同源 Blob URL；
 *   其余按二进制处理，给出大小、预览截断说明与主动下载入口。
 * - `git_diff`：相对 HEAD 的合并差异。行内容原样呈现（已带 `+`/`-`/空格前缀），hunk 头保留；
 *   二进制、冲突、不可用与截断各有显式说明，不伪造文本差异。
 * - `session_changes`：同一文件的历次记录，按时间排列，单次统计各随记录保留。
 *
 * ## 关闭始终可用
 *
 * 加载中、失败、过时三种状态都渲染右上角的「关闭文件查看」按钮（可键盘触发）；页面负责在关闭
 * 后把焦点还给触发文件行、恢复窄屏原来的文件列表。组件不拥有草稿、滚动锚点或条目展开状态。
 *
 * ## 图片地址的唯一来源
 *
 * 组件不碰 `URL.createObjectURL`：`createPreviewUrl` 由页面从 feature 服务（平台端口）注入。
 * 没有该能力时显式说明「当前平台不支持图片预览」，绝不退化为 data URL 或远端地址。
 */

import { createElement, useEffect, useRef, useState } from "react";
import type { ReactElement } from "react";

import type { FileContentState, FilePreviewContent, SessionFileDownloadState } from "../features";
import { FILE_SOURCE_LABELS } from "../features";
import type { GitDiffView, SessionChangeRecord } from "../domain/session-files";
import { ChangeDiff } from "./FileChangesPanel";
import type { Palette } from "./theme";
import { Pressable, ScrollView, StyleSheet, Text, View, fontFamily, fontSize, palettes, radius, spacing, useTheme } from "./theme";

/** 平台端口返回的预览 URL 形状（与 `features/session-file-download.ts` 同一结构）。 */
export interface FilePreviewArtifact {
  readonly url: string;
  readonly revoke: () => void;
}

export interface FileContentViewerProps {
  readonly content: Exclude<FileContentState, { readonly kind: "conversation" }>;
  /** 会话变更的历史范围说明（完整加载时为 `null`）。 */
  readonly historyNotice?: string | null;
  readonly onClose: () => void;
  readonly onRetry?: (() => void) | undefined;
  readonly onOpenChangeDiagnostics?: ((eventId: string | null) => void) | undefined;
  /** 主动下载（只对 `preview` 内容有意义）。 */
  readonly onDownload?: ((path: string, sizeBytes: string | null, mimeType: string) => void) | undefined;
  readonly onCancelDownload?: (() => void) | undefined;
  readonly onReleaseDownload?: (() => void) | undefined;
  readonly downloadState?: SessionFileDownloadState | null;
  readonly createPreviewUrl?:
    | ((bytes: Uint8Array, mimeType: string) => FilePreviewArtifact | null)
    | undefined;
}

/** 按调色板建样式表：浅/深各建一次（模块加载时）。 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    viewer: {
      flexGrow: 1,
      flexShrink: 1,
      flexBasis: 0,
      minHeight: 0,
      width: "100%",
      backgroundColor: colors.panel,
    },
    viewerScroll: {
      flexGrow: 1,
      flexShrink: 1,
      minHeight: 0,
    },
    head: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "baseline",
      gap: spacing.sm,
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.md,
      borderBottomWidth: 1,
      borderBottomColor: colors.line,
    },
    sourceLabel: {
      paddingHorizontal: spacing.sm,
      paddingVertical: 2,
      borderRadius: radius.sm,
      color: colors.ink2,
      backgroundColor: colors.line2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    path: {
      flexShrink: 1,
      color: colors.ink,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.sm,
    },
    spacer: {
      flexGrow: 1,
    },
    close: {
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.sm,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
    },
    closeText: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    body: {
      flexGrow: 1,
      flexShrink: 1,
      flexBasis: 0,
      minHeight: 0,
      gap: spacing.sm,
      padding: spacing.md,
    },
    note: {
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    warning: {
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.sm,
      borderRadius: radius.sm,
      color: colors.ink2,
      backgroundColor: colors.dangerSoft,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    errorTitle: {
      color: colors.danger,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      fontWeight: "600",
    },
    textBody: {
      color: colors.ink,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
      lineHeight: 18,
    },
    imageWrap: {
      alignItems: "flex-start",
      gap: spacing.xs,
    },
    action: {
      alignSelf: "flex-start",
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.md,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
    },
    actionText: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    diff: {
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
      overflow: "hidden",
    },
    diffLine: {
      paddingVertical: 1,
      paddingHorizontal: spacing.sm,
      color: colors.ink2,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
    },
    diffAdded: {
      color: colors.ok,
      backgroundColor: colors.okSoft,
    },
    diffDeleted: {
      color: colors.danger,
      backgroundColor: colors.dangerSoft,
    },
    diffContext: {
      color: colors.ink3,
    },
    diffHunk: {
      color: colors.accent,
      backgroundColor: colors.accentSoft,
    },
    diffHeader: {
      color: colors.ink3,
      backgroundColor: colors.line2,
    },
    record: {
      gap: spacing.xs,
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.md,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
    },
    recordHead: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "baseline",
      gap: spacing.sm,
    },
    chip: {
      paddingHorizontal: spacing.sm,
      paddingVertical: 2,
      borderRadius: radius.sm,
      color: colors.ink2,
      backgroundColor: colors.line2,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
    },
    added: {
      paddingHorizontal: spacing.sm,
      paddingVertical: 2,
      borderRadius: radius.sm,
      color: colors.ok,
      backgroundColor: colors.okSoft,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
    },
    deleted: {
      paddingHorizontal: spacing.sm,
      paddingVertical: 2,
      borderRadius: radius.sm,
      color: colors.danger,
      backgroundColor: colors.dangerSoft,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
    },
    summary: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

/** 已登记的二进制下载 MIME（下载文件名只取安全 basename）。 */
const BINARY_MIME = "application/octet-stream";

/** 图片扩展名 → MIME（只用于 Blob 的类型标签；信任的是实际 magic 校验）。 */
const IMAGE_MIME_BY_EXTENSION: Readonly<Record<string, string>> = {
  png: "image/png",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  gif: "image/gif",
  webp: "image/webp",
};

/** 从实际字节判断栅格图片格式；不认识时返回 `null`（按二进制处理，不猜成图片）。 */
function imageMimeTypeOf(bytes: Uint8Array, path: string): string | null {
  const ascii = (start: number, end: number): string =>
    String.fromCharCode(...bytes.slice(start, Math.min(end, bytes.length)));
  if (bytes.length >= 8 && bytes[0] === 0x89 && ascii(1, 4) === "PNG" && bytes[4] === 0x0d && bytes[5] === 0x0a) {
    return "image/png";
  }
  if (bytes.length >= 3 && bytes[0] === 0xff && bytes[1] === 0xd8 && bytes[2] === 0xff) return "image/jpeg";
  if (bytes.length >= 6 && (ascii(0, 6) === "GIF87a" || ascii(0, 6) === "GIF89a")) return "image/gif";
  if (bytes.length >= 12 && ascii(0, 4) === "RIFF" && ascii(8, 12) === "WEBP") return "image/webp";
  // 实际格式无法确认时不把 `fileKind: image` 当成授权：退回扩展名白名单，仍不认识就按二进制。
  const extension = path.slice(path.lastIndexOf(".") + 1).toLowerCase();
  return path.includes(".") ? IMAGE_MIME_BY_EXTENSION[extension] ?? null : null;
}

/** 下载时使用的 MIME（服务端只给出内容大类；图片按实际 magic，其余用通用二进制）。 */
function downloadMimeTypeOf(preview: FilePreviewContent): string {
  if (preview.fileKind === "image") return imageMimeTypeOf(preview.bytes, preview.path) ?? BINARY_MIME;
  return BINARY_MIME;
}

/** 规范十进制字节数 → 便于阅读的大小（未知/非法时不编造）。 */
export function formatBytes(sizeBytes: string): string {
  let value: bigint;
  try {
    value = BigInt(sizeBytes);
  } catch {
    return `${sizeBytes} 字节`;
  }
  if (value < 1024n) return `${value.toString()} 字节`;
  const units = ["KiB", "MiB", "GiB"];
  let scaled = Number(value) / 1024;
  let unit = 0;
  while (scaled >= 1024 && unit < units.length - 1) {
    scaled /= 1024;
    unit += 1;
  }
  return `${scaled.toFixed(1)} ${units[unit]}`;
}

/** UTF-8 解码（失败返回 `null`：按二进制处理，不替换成可读但失真的文本）。 */
function decodeUtf8(bytes: Uint8Array): string | null {
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    return null;
  }
}

/** Git 差异行的展示档位。 */
type GitLineKind = "added" | "deleted" | "context" | "hunk" | "header";

/** Git 差异行的展示档位（服务端下发的是已带前缀的原始行，这边不补第二个前缀）。 */
function gitLineKindOf(line: string): GitLineKind {
  if (line.startsWith("@@")) return "hunk";
  if (line.startsWith("+++") || line.startsWith("---") || line.startsWith("diff --git") || line.startsWith("index ")) {
    return "header";
  }
  if (line.startsWith("+")) return "added";
  if (line.startsWith("-")) return "deleted";
  return "context";
}

const GIT_LINE_STYLE: Readonly<Record<GitLineKind, "diffAdded" | "diffDeleted" | "diffContext" | "diffHunk" | "diffHeader">> = {
  added: "diffAdded",
  deleted: "diffDeleted",
  context: "diffContext",
  hunk: "diffHunk",
  header: "diffHeader",
};

/** Git 不可用原因 → 已登记文案。 */
const GIT_UNAVAILABLE_LABELS: Readonly<Record<NonNullable<GitDiffView["unavailable"]>, string>> = {
  not_repository: "此目录不是 Git 仓库。",
  git_unavailable: "Git 不可用：未安装或无法启动。",
  too_large: "差异超过单次下发上限，无法完整显示。",
};

/** Git 差异区。 */
function GitDiffBody({ diff }: { readonly diff: GitDiffView }): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

  return (
    <View style={styles.body} dataSet={{ fileViewContent: "git_diff", gitDiffKind: diff.kind, gitDiffBase: diff.base }}>
      <Text style={styles.note} dataSet={{ gitDiffBaseLabel: diff.base }}>
        {diff.base === "head" ? "相对 HEAD 与当前工作树的合并差异（不拼接暂存/未暂存补丁）。" : "新仓库（空基准）：展示文件的完整新增内容。"}
      </Text>
      {diff.kind === "binary" ? (
        <Text style={styles.warning} dataSet={{ gitDiffBinary: "true" }}>
          二进制文件不提供文本差异。
        </Text>
      ) : null}
      {diff.kind === "conflict" ? (
        <Text style={styles.warning} dataSet={{ gitDiffConflict: "true" }}>
          这个文件存在未解决的冲突，下面保留冲突标记的原文，不伪造「已合并」结果。
        </Text>
      ) : null}
      {diff.unavailable === null ? null : (
        <Text style={styles.warning} dataSet={{ gitDiffUnavailable: diff.unavailable }}>
          {GIT_UNAVAILABLE_LABELS[diff.unavailable]}
        </Text>
      )}
      {diff.truncated ? (
        <Text style={styles.warning} dataSet={{ gitDiffTruncated: "true" }}>
          差异超过上限，已截断；这里不把截断后的内容当作完整差异。
        </Text>
      ) : null}
      {diff.diffLines.length === 0 ? null : (
        <View style={styles.diff} dataSet={{ gitDiffLines: String(diff.diffLines.length) }}>
          {diff.diffLines.map((line, index) => {
            const kind = gitLineKindOf(line);
            return (
              <Text
                key={`git-diff:${String(index)}`}
                style={[styles.diffLine, styles[GIT_LINE_STYLE[kind]]]}
                dataSet={{ gitDiffLineKind: kind, gitDiffHunk: kind === "hunk" ? "true" : "false" }}
              >
                {line}
              </Text>
            );
          })}
        </View>
      )}
    </View>
  );
}

/** 会话变更的历次记录。 */
function SessionChangesBody({
  records,
  historyNotice,
  onOpenChangeDiagnostics,
}: {
  readonly records: readonly SessionChangeRecord[];
  readonly historyNotice: string | null;
  readonly onOpenChangeDiagnostics: ((eventId: string | null) => void) | undefined;
}): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

  return (
    <View style={styles.body} dataSet={{ fileViewContent: "session_changes", sessionChangeCount: String(records.length) }}>
      {historyNotice === null ? null : (
        <Text style={styles.note} dataSet={{ sessionHistoryNotice: "true" }}>
          {historyNotice}
        </Text>
      )}
      {records.length === 0 ? (
        <Text style={styles.note} dataSet={{ sessionChangesEmpty: "true" }}>
          这个文件没有已加载的改动记录。
        </Text>
      ) : null}
      {records.map((record, index) => (
        <View key={record.eventId} style={styles.record} dataSet={{ sessionChangeRecord: record.eventId, sessionChangeIndex: String(index + 1) }}>
          <View style={styles.recordHead}>
            <Text style={styles.chip}>{record.kind}</Text>
            <Text style={styles.note} dataSet={{ sessionChangeTime: record.createdAt }}>
              {record.createdAt}
            </Text>
            {record.addedLines === null ? null : (
              <Text style={styles.added} dataSet={{ sessionChangeAddedLines: record.addedLines }}>
                {`+${record.addedLines}`}
              </Text>
            )}
            {record.deletedLines === null ? null : (
              <Text style={styles.deleted} dataSet={{ sessionChangeDeletedLines: record.deletedLines }}>
                {`-${record.deletedLines}`}
              </Text>
            )}
            {record.addedLines === null && record.deletedLines === null ? (
              <Text style={styles.note} dataSet={{ sessionChangeLineCountUnavailable: "true" }}>
                无行数统计
              </Text>
            ) : null}
          </View>
          <Text style={styles.summary} dataSet={{ sessionChangeSummary: "true" }}>
            {record.summary}
          </Text>
          <ChangeDiff diff={record.diff} />
          {onOpenChangeDiagnostics === undefined ? null : (
            <Pressable
              role="button"
              style={styles.action}
              dataSet={{ action: "open-session-change-diagnostics", eventId: record.eventId }}
              onPress={() => {
                onOpenChangeDiagnostics(record.eventId);
              }}
            >
              <Text style={styles.actionText}>查看这条改动的 ACP 原文（诊断）</Text>
            </Pressable>
          )}
        </View>
      ))}
    </View>
  );
}

/** 文本/图片/二进制预览。 */
function PreviewBody({
  preview,
  onDownload,
  onCancelDownload,
  onReleaseDownload,
  downloadState,
  createPreviewUrl,
}: {
  readonly preview: FilePreviewContent;
  readonly onDownload: FileContentViewerProps["onDownload"];
  readonly onCancelDownload: (() => void) | undefined;
  readonly onReleaseDownload: (() => void) | undefined;
  readonly downloadState: SessionFileDownloadState | null;
  readonly createPreviewUrl: FileContentViewerProps["createPreviewUrl"];
}): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const mimeType = preview.fileKind === "image" ? imageMimeTypeOf(preview.bytes, preview.path) : null;
  const [imageUrl, setImageUrl] = useState<string | null>(null);
  const [imageFailed, setImageFailed] = useState(false);
  const createRef = useRef(createPreviewUrl);
  createRef.current = createPreviewUrl;

  // Blob URL 的生命周期与这份字节绑定：内容更换/组件卸载即撤销。
  useEffect(() => {
    setImageFailed(false);
    if (preview.fileKind !== "image" || mimeType === null) {
      setImageUrl(null);
      return;
    }
    const create = createRef.current;
    if (create === undefined) {
      setImageUrl(null);
      setImageFailed(true);
      return;
    }
    const artifact = create(preview.bytes, mimeType);
    if (artifact === null) {
      setImageUrl(null);
      setImageFailed(true);
      return;
    }
    setImageUrl(artifact.url);
    return () => {
      artifact.revoke();
    };
  }, [preview.fileKind, preview.bytes, mimeType]);

  const download =
    downloadState !== null && downloadState.kind !== "idle" && downloadState.path === preview.path
      ? downloadState
      : null;

  return (
    <View style={styles.body} dataSet={{ fileViewContent: "preview", previewKind: preview.fileKind }}>
      <Text style={styles.note} dataSet={{ previewSize: preview.sizeBytes }}>
        {`${formatBytes(preview.sizeBytes)}${preview.previewTruncated ? "（预览已按上限截断）" : ""}`}
      </Text>
      {preview.previewTruncated ? (
        <Text style={styles.warning} dataSet={{ previewTruncated: "true" }}>
          预览超过上限，只展示已读取的部分；需要完整内容时请下载。
        </Text>
      ) : null}

      {preview.fileKind === "text" ? <TextPreview preview={preview} /> : null}

      {preview.fileKind === "image" ? (
        mimeType === null ? (
          <Text style={styles.warning} dataSet={{ imagePreviewUnsupported: "format" }}>
            这个文件虽然被标记为图片，但实际格式不在支持的范围内（PNG/JPEG/GIF/WebP），因此不渲染。
          </Text>
        ) : imageFailed ? (
          <Text style={styles.warning} dataSet={{ imagePreviewUnsupported: "platform" }}>
            当前平台不支持创建 Blob 预览地址，因此不加载图片。
          </Text>
        ) : imageUrl === null ? (
          <Text style={styles.note} dataSet={{ imagePreviewLoading: "true" }}>
            正在创建图片预览…
          </Text>
        ) : (
          <View style={styles.imageWrap} dataSet={{ filePreviewImage: "ready" }}>
            {createElement("img", {
              src: imageUrl,
              alt: preview.path,
              "data-file-preview-image": "true",
              "data-file-preview-mime": mimeType,
              style: { maxWidth: "100%", maxHeight: "60vh", objectFit: "contain" },
            })}
          </View>
        )
      ) : null}

      {preview.fileKind === "binary" ? (
        <Text style={styles.note} dataSet={{ binaryPreview: "true" }}>
          二进制文件不渲染内容。可以主动下载后在本机打开。
        </Text>
      ) : null}

      {download === null ? (
        onDownload === undefined ? null : (
          <Pressable
            role="button"
            style={styles.action}
            dataSet={{ action: "download-file", filePath: preview.path }}
            onPress={() => {
              onDownload(preview.path, preview.sizeBytes, downloadMimeTypeOf(preview));
            }}
          >
            <Text style={styles.actionText}>下载文件</Text>
          </Pressable>
        )
      ) : (
        <View style={styles.record} dataSet={{ downloadState: download.kind, downloadPath: download.path }}>
          {download.kind === "downloading" ? (
            <>
              <Text style={styles.note} dataSet={{ downloadBytes: String(download.bytesWritten) }}>
                {`正在下载：已写入 ${String(download.bytesWritten)} 字节${
                  download.totalBytes === null ? "" : ` / ${formatBytes(download.totalBytes)}`
                }`}
              </Text>
              {onCancelDownload === undefined ? null : (
                <Pressable
                  role="button"
                  style={styles.action}
                  dataSet={{ action: "cancel-download" }}
                  onPress={onCancelDownload}
                >
                  <Text style={styles.actionText}>取消下载</Text>
                </Pressable>
              )}
            </>
          ) : null}
          {download.kind === "saved" ? (
            <>
              <Text style={styles.note} dataSet={{ downloadSaved: String(download.bytesWritten) }}>
                {`已保存 ${download.fileName}（${String(download.bytesWritten)} 字节，${download.mode === "stream" ? "直接落盘" : "浏览器 Blob 回退"}）。`}
              </Text>
              {download.url === null ? null : (
                <View dataSet={{ downloadArtifact: download.mode }}>
                  {createElement(
                    "a",
                    {
                      href: download.url,
                      download: download.fileName,
                      "data-download-artifact": download.mode,
                      style: { color: theme.colors.ink2, fontSize: 13 },
                    },
                    `保存 ${download.fileName}`,
                  )}
                </View>
              )}
              {onReleaseDownload === undefined ? null : (
                <Pressable
                  role="button"
                  style={styles.action}
                  dataSet={{ action: "release-download" }}
                  onPress={onReleaseDownload}
                >
                  <Text style={styles.actionText}>释放下载资源</Text>
                </Pressable>
              )}
              {onDownload === undefined ? null : (
                <Pressable
                  role="button"
                  style={styles.action}
                  dataSet={{ action: "download-again", filePath: preview.path }}
                  onPress={() => {
                    onDownload(preview.path, preview.sizeBytes, downloadMimeTypeOf(preview));
                  }}
                >
                  <Text style={styles.actionText}>重新下载</Text>
                </Pressable>
              )}
            </>
          ) : null}
          {download.kind === "cancelled" ? (
            <Text style={styles.note} dataSet={{ downloadCancelled: String(download.bytesWritten) }}>
              已取消下载，未完成的字节已经释放。
            </Text>
          ) : null}
          {download.kind === "failed" ? (
            <Text style={styles.warning} dataSet={{ downloadFailed: download.error.code }}>
              {`下载失败：${download.error.message}`}
            </Text>
          ) : null}
        </View>
      )}
    </View>
  );
}

/** 文本预览：只在 UTF-8 可解码时按纯文本呈现，否则退回二进制说明。 */
function TextPreview({ preview }: { readonly preview: FilePreviewContent }): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const text = decodeUtf8(preview.bytes);

  if (text === null) {
    return (
      <Text style={styles.warning} dataSet={{ textPreviewUndecodable: "true" }}>
        内容不是有效的 UTF-8 文本，按二进制处理。
      </Text>
    );
  }
  return (
    <View style={styles.diff} dataSet={{ textPreview: "true" }}>
      {/* 纯文本只经 `Text` 呈现：不解析 Markdown/HTML，不执行脚本，不加载其中的远端资源。 */}
      <Text style={[styles.textBody, { padding: spacing.sm }]} dataSet={{ filePreviewText: "true" }}>
        {text}
      </Text>
    </View>
  );
}

/** 中间只读文件查看区。 */
export function FileContentViewer({
  content,
  historyNotice = null,
  onClose,
  onRetry,
  onOpenChangeDiagnostics,
  onDownload,
  onCancelDownload,
  onReleaseDownload,
  downloadState = null,
  createPreviewUrl,
}: FileContentViewerProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const source = content.context.source;
  const path = content.context.path;

  return (
    <View
      style={styles.viewer}
      dataSet={{
        fileView: content.status,
        fileViewSource: source,
        fileViewPath: path,
        fileViewStale: String(content.stale),
      }}
    >
      <View style={styles.head}>
        <Text style={styles.sourceLabel} dataSet={{ fileViewSourceLabel: source }}>
          {FILE_SOURCE_LABELS[source]}
        </Text>
        <Text style={styles.path} numberOfLines={1} dataSet={{ fileViewPathLabel: path }}>
          {path}
        </Text>
        <View style={styles.spacer} />
        {createElement(
          "button",
          {
            type: "button",
            "data-action": "close-file-view",
            "aria-label": "关闭文件查看",
            tabIndex: 0,
            onClick: onClose,
            onKeyDown: (event: KeyboardEvent) => {
              if (event.key === "Enter" || event.key === " ") {
                event.preventDefault();
                onClose();
              }
            },
            style: {
              padding: "6px 10px",
              border: `1px solid ${theme.colors.line}`,
              borderRadius: 6,
              background: theme.colors.panel,
              color: theme.colors.ink,
              cursor: "pointer",
            },
          },
          "✕ 关闭文件查看",
        )}
      </View>

      <ScrollView style={styles.viewerScroll} dataSet={{ fileViewScroll: "true" }}>
      {content.stale ? (
        <View style={styles.body} dataSet={{ fileViewStaleBanner: "true" }}>
          <Text style={styles.warning}>
            这个文件在查看期间发生了变化。关闭后重新打开，或点「重新加载」读取最新内容。
          </Text>
          {onRetry === undefined ? null : (
            <Pressable
              role="button"
              style={styles.action}
              dataSet={{ action: "reload-file-view" }}
              onPress={onRetry}
            >
              <Text style={styles.actionText}>重新加载</Text>
            </Pressable>
          )}
        </View>
      ) : null}

      {content.status === "loading" ? (
        <View style={styles.body} dataSet={{ fileViewLoading: "true" }}>
          <Text style={styles.note}>正在读取文件…</Text>
        </View>
      ) : null}

      {content.status === "error" ? (
        <View style={styles.body} dataSet={{ fileViewError: content.error.code }}>
          <Text style={styles.errorTitle}>{`无法打开这个文件：${content.error.code}`}</Text>
          <Text style={styles.note}>{content.error.message}</Text>
          {onRetry === undefined ? null : (
            <Pressable
              role="button"
              style={styles.action}
              dataSet={{ action: "retry-file-view" }}
              onPress={onRetry}
            >
              <Text style={styles.actionText}>重试</Text>
            </Pressable>
          )}
        </View>
      ) : null}

      {content.status === "ready" && content.content.kind === "preview" ? (
        <PreviewBody
          preview={content.content}
          onDownload={onDownload}
          onCancelDownload={onCancelDownload}
          onReleaseDownload={onReleaseDownload}
          downloadState={downloadState}
          createPreviewUrl={createPreviewUrl}
        />
      ) : null}

      {content.status === "ready" && content.content.kind === "git_diff" ? (
        <GitDiffBody diff={content.content.diff} />
      ) : null}

      {content.status === "ready" && content.content.kind === "session_changes" ? (
        <SessionChangesBody
          records={content.content.records}
          historyNotice={historyNotice}
          onOpenChangeDiagnostics={onOpenChangeDiagnostics}
        />
      ) : null}
      </ScrollView>
    </View>
  );
}
