/**
 * 诊断抽屉（R18：保留原文与事件元数据供诊断；`design.md` D7）。
 *
 * ## 形态：右侧抽屉（原型 `.dx` / `#dxScrim`）
 *
 * 从内联 `<aside>` 改成固定定位的右侧抽屉 + 遮罩：抽屉头是标题与关闭入口，主体是元数据、
 * 诊断说明与原文三块。`data-diagnostics-drawer="open"` 与 `data-action="close-diagnostics"`
 * 原样保留（e2e 与组件测试都按它们定位），`Esc` 关闭也保留——抽屉打开时在本组件内挂一个
 * `keydown` 监听（原型把 `closeDx()` 挂在同一处），它只依赖 `onClose`，因此页面与主机面板
 * 两个调用方都不必各自记得这条行为。
 *
 * ## 「事件视图」的展开态与被查看的事件绑定
 *
 * 展开态住在本组件里，而抽屉在 `model === null` 时只是返回空片段（自身不卸载）：查看另一条
 * 事件时必须显式复位，否则会继承上一条的展开态。改前它是 `<details>`，随抽屉关闭被整体卸载、
 * 每次重挂都收起；复位因此按**事件标识**（`model.eventId`，关闭记为 `null`）触发。
 *
 * ## 原文未同步时这里**没有**原文分支
 *
 * `RawAvailability` 是判别联合：`not_synced` 分支在本组件里没有可渲染的正文槽位，
 * 因此「渲染一段占位 JSON 假装完整」在这里没有表达方式。取而代之的是一段明确的说明，
 * 外加字节长度与摘要（有的话）——它们是**元数据**，不是原文。
 *
 * 「原文与元数据」的元数据部分（事件类型、事件 ID、全局序号、来源、时间）无论原文是否
 * 可用都完整呈现：诊断需要它们。
 *
 * ## 样式机制不变（D8）
 *
 * 全部经 `StyleSheet.create` + `theme.ts` 的 token；抽屉的层级取 `zIndex.drawer`、
 * 遮罩取 `zIndex.scrim`（原型 `header.top` 30 / `.scrim` 35 / `.drawer` 40）。
 */

import { useEffect, useState } from "react";
import type { ReactElement } from "react";

import type { DegradedEventModel } from "../features/degradation-model";
import { RAW_NOT_SYNCED_LABELS, UNSUPPORTED_LABELS } from "../features/degradation-model";
import type { Palette } from "./theme";
import {
  Pressable,
  StyleSheet,
  Text,
  View,
  fontFamily,
  fontSize,
  fontWeight,
  palettes,
  radius,
  spacing,
  useTheme,
  webStyle,
  zIndex,
} from "./theme";

export interface DiagnosticsDrawerProps {
  /**
   * 选中的事件：降级条目，或用户主动点「查看 ACP 原文」的任意记录
   * （`reason.kind === "explicit"`）。`null` 表示抽屉未打开。
   */
  readonly model: DegradedEventModel | null;
  readonly onClose: () => void;
}

/** 元数据行：标签 + 等宽值（值带自己的锚点）。 */
function MetadataRow({
  label,
  value,
  anchor,
}: {
  readonly label: string;
  /**
   * 值；`null` 表示该字段不适用（历史条目没有全局投递序号、也没有 `origin`）——原样渲染为空，
   * 不补占位符。锚点与它的取值不受影响。
   */
  readonly value: string | null;
  readonly anchor: "event-type" | "event-id" | "global-sequence" | "origin" | "created-at";
}): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  return (
    <View style={styles.metaRow}>
      <Text style={styles.metaKey}>{label}</Text>
      <Text style={styles.metaValue} dataSet={{ metadata: anchor }}>
        {value}
      </Text>
    </View>
  );
}

/** 诊断原因的一句话说明（四类降级原因 + 用户主动查看原文的 `explicit`）。 */
function ReasonNote({ model }: { readonly model: DegradedEventModel }): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const reason = model.reason;
  if (reason.kind === "explicit") {
    return (
      <Text style={styles.reason} dataSet={{ degradedReason: "explicit" }}>
        以下是你请求查看的事件原文。
      </Text>
    );
  }
  if (reason.kind === "no_dedicated_view") {
    return (
      <Text style={styles.reason} dataSet={{ degradedReason: "no_dedicated_view" }}>
        当前客户端没有为该事件提供专用视图，以下是它的原始内容。
      </Text>
    );
  }
  if (reason.kind === "unsupported") {
    return (
      <Text
        style={styles.reason}
        dataSet={{ degradedReason: "unsupported", unsupportedBy: reason.by }}
      >
        {UNSUPPORTED_LABELS[reason.by]}：{reason.detail}
      </Text>
    );
  }
  if (reason.kind === "raw_not_synced") {
    return (
      <Text style={styles.reason} dataSet={{ degradedReason: "raw_not_synced" }}>
        {RAW_NOT_SYNCED_LABELS[reason.reason]}。
      </Text>
    );
  }
  return (
    <Text style={styles.reason} dataSet={{ degradedReason: "binary_not_delivered" }}>
      结构化事件已收到，但它引用的二进制内容没有下发到本设备。
    </Text>
  );
}

/** 按调色板建样式表：浅/深各建一次（模块加载时）。 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    scrim: {
      top: 0,
      right: 0,
      bottom: 0,
      left: 0,
      backgroundColor: "rgba(10,12,16,.35)",
      zIndex: zIndex.scrim,
      ...webStyle({ position: "fixed" }),
    },
    drawer: {
      top: 0,
      right: 0,
      bottom: 0,
      width: 440,
      padding: spacing.lg,
      gap: spacing.md,
      backgroundColor: colors.panel,
      borderLeftWidth: 1,
      borderLeftColor: colors.line,
      zIndex: zIndex.drawer,
      ...webStyle({ position: "fixed", maxWidth: "96vw" }),
    },
    head: {
      flexDirection: "row",
      alignItems: "center",
      gap: spacing.sm,
      paddingBottom: spacing.sm,
      borderBottomWidth: 1,
      borderBottomColor: colors.line,
    },
    title: {
      flexGrow: 1,
      flexShrink: 1,
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
      fontWeight: fontWeight.medium,
    },
    close: {
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.md,
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
      minHeight: 0,
      gap: spacing.md,
      // 抽屉是固定高度的浮层，长原文（`rawJson`）因此自己滚（原型 `.dxbody{overflow:auto}`）：
      // `overflow: "auto"` 不在 `react-native` 的取值里，走 `webStyle` 断言点进去。
      ...webStyle({ overflowY: "auto" }),
    },
    meta: {
      gap: spacing.xs,
    },
    metaRow: {
      flexDirection: "row",
      alignItems: "baseline",
      gap: spacing.md,
    },
    metaKey: {
      minWidth: 86,
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    metaValue: {
      flexShrink: 1,
      color: colors.ink2,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
    },
    reason: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    section: {
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
      overflow: "hidden",
    },
    sectionHead: {
      flexDirection: "row",
      alignItems: "center",
      gap: spacing.sm,
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.md,
      backgroundColor: colors.panel2,
    },
    sectionCaret: {
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    sectionTitle: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    sectionKicker: {
      color: colors.ink3,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
    },
    sectionBody: {
      padding: spacing.md,
      borderTopWidth: 1,
      borderTopColor: colors.line2,
    },
    sectionBodyHidden: {
      display: "none",
    },
    // 原文按 Agent 给出的字节原样呈现（RNW 会转义）：等宽；RNW 的 `Text` 默认 `white-space:
    // pre-wrap`，因此缩进与换行原样保留、长串在词边界折行。
    raw: {
      color: colors.ink2,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
    },
    // 原文未下发时的说明：醒目但与「三类能力不支持」分开（那是另一段文案）。
    rawUnavailable: {
      padding: spacing.md,
      borderLeftWidth: 3,
      borderLeftColor: colors.warn,
      borderRadius: radius.sm,
      backgroundColor: colors.warnSoft,
      color: colors.warn,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

/** 诊断抽屉。 */
export function DiagnosticsDrawer({ model, onClose }: DiagnosticsDrawerProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const [eventViewOpen, setEventViewOpen] = useState(false);
  const open = model !== null;

  // Esc 关闭（原型把 `closeDx()` 挂在 document 的 keydown 上）。`renderToStaticMarkup` 与原生
  // 平台没有 document，因此先按存在性守门；抽屉没打开时不挂监听。
  useEffect(() => {
    if (!open) return;
    if (typeof document === "undefined") return;
    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key !== "Escape") return;
      onClose();
    };
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [open, onClose]);

  // 「事件视图」的展开态与**被查看的事件**绑定：查看对象变化时复位（也覆盖「展开 → 关闭 →
  // 打开另一条」这条路径）。改前它是 `<details>`——随抽屉关闭被整体卸载，每次重挂都收起；
  // 抽屉本身现在常驻（`model === null` 时只是返回空片段），因此这条复位必须显式写出来，
  // 否则会继承上一条事件的展开态。`data-*` 锚点、Esc 与遮罩行为都不受影响。
  const viewedEventId = model === null ? null : model.eventId;
  useEffect(() => {
    setEventViewOpen(false);
  }, [viewedEventId]);

  if (model === null) return <></>;

  return (
    <>
      {/* 遮罩：点它即关闭（原型 `#dxScrim.onclick = closeDx`）。 */}
      <Pressable
        role="button"
        aria-label="关闭事件详情"
        style={styles.scrim}
        dataSet={{ diagnosticsScrim: "open" }}
        onPress={onClose}
      />

      <View role="dialog" aria-modal={true} aria-label="事件详情" style={styles.drawer} dataSet={{ diagnosticsDrawer: "open" }}>
        <View style={styles.head}>
          <Text style={styles.title}>事件详情</Text>
          <Pressable
            role="button"
            style={styles.close}
            dataSet={{ action: "close-diagnostics" }}
            onPress={onClose}
          >
            <Text style={styles.closeText}>关闭</Text>
          </Pressable>
        </View>

        <View style={styles.body}>
          <View style={styles.meta} dataSet={{ diagnosticsMetadata: "true" }}>
            <MetadataRow label="事件类型" value={model.eventType} anchor="event-type" />
            <MetadataRow label="事件 ID" value={model.eventId} anchor="event-id" />
            <MetadataRow label="全局序号" value={model.globalSequence} anchor="global-sequence" />
            <MetadataRow label="来源" value={model.originKind} anchor="origin" />
            <MetadataRow label="时间" value={model.createdAt} anchor="created-at" />
          </View>

          <ReasonNote model={model} />

          {model.raw.kind === "available" ? (
            <View style={styles.section}>
              <View style={styles.sectionBody}>
                <Text style={styles.raw} dataSet={{ rawJson: "available" }}>
                  {model.raw.rawJson}
                </Text>
              </View>
            </View>
          ) : model.raw.kind === "not_synced" ? (
            <Text style={styles.rawUnavailable} dataSet={{ rawUnavailable: model.raw.reason }}>
              原文未同步（{RAW_NOT_SYNCED_LABELS[model.raw.reason]}）；字节长度 {model.raw.byteLength}
              {model.raw.sha256 === null ? "" : `；摘要 ${model.raw.sha256}`}
            </Text>
          ) : (
            <Text style={styles.rawUnavailable} dataSet={{ rawUnavailable: "absent" }}>
              该事件没有随附原文。
            </Text>
          )}

          <View style={styles.section} dataSet={{ rawView: "event-view" }}>
            <Pressable
              role="button"
              style={styles.sectionHead}
              dataSet={{ action: "toggle-event-view", rawViewOpen: String(eventViewOpen) }}
              onPress={() => {
                setEventViewOpen(!eventViewOpen);
              }}
            >
              <Text style={styles.sectionCaret}>{eventViewOpen ? "▾" : "▸"}</Text>
              <Text style={styles.sectionTitle}>事件视图</Text>
              <Text style={styles.sectionKicker}>payload.view</Text>
            </Pressable>
            {/* 结构化视图始终在 DOM（收起只是 `display: none`），与原型 `details` 的形态一致。 */}
            <View style={eventViewOpen ? styles.sectionBody : [styles.sectionBody, styles.sectionBodyHidden]}>
              <Text style={styles.raw}>{JSON.stringify(model.view, null, 2)}</Text>
            </View>
          </View>
        </View>
      </View>
    </>
  );
}
