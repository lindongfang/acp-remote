/**
 * 变更文件栏（spec「变更文件的呈现边界」；`design.md` D8/D10）。
 *
 * ## 只有结构化统计，没有差异正文
 *
 * `file.changed` 的视图里**没有**差异行：本栏呈现每条改动的展示路径、类型、摘要与行数统计，
 * 并给每条改动一个「查看 ACP 原文（诊断）」入口（原文在 `ChangeDiagnosticsPanel` 里作为不可信
 * 文本呈现）。组件不解析 `acp.rawJson`，也不据它生成任何差异行——「把原文解析后渲染成差异」
 * 在这里没有数据通路。
 *
 * ## 判定不出的统计省略而不是填零
 *
 * `addedLines` / `deletedLines` 是 `string | null`：`null` 表示「没有依据」（模型已按 D8 处理）。
 * 组件只在非 `null` 时渲染对应标记，两侧都为 `null` 时给出一句说明——填 `+0 / -0` 会把
 * 「不知道」说成「没有改动」。
 *
 * ## 展开态由页面持有
 *
 * 哪一条的诊断是展开的属于页面状态（与目录页的查询词同理：组件是纯函数，只吃 props）。
 * 未接线（回调缺省）时不渲染诊断入口，而不是渲染一个点了没有反应的按钮。
 */

import type { ReactElement } from "react";

import type { FileChangeView } from "../features/conversation-timeline";
import { ChangeDiagnosticsPanel } from "./ChangeDiagnosticsPanel";
import type { Palette } from "./theme";
import { Pressable, StyleSheet, Text, View, fontFamily, fontSize, palettes, radius, spacing, useTheme } from "./theme";

export interface FileChangesPanelProps {
  readonly changes: readonly FileChangeView[];
  /** 当前展开诊断的那一处改动（取 `diagnostics.eventId`）；缺省 `null` = 全部收起。 */
  readonly openDiagnosticsEventId?: string | null;
  /** 打开 / 收起某处改动的诊断（`null` = 收起）；缺省时不渲染诊断入口。 */
  readonly onOpenChangeDiagnostics?: ((eventId: string | null) => void) | undefined;
}

/** 按调色板建样式表：浅/深各建一次（模块加载时）。 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    panel: {
      gap: spacing.sm,
      padding: spacing.md,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.md,
      backgroundColor: colors.panel2,
    },
    head: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "baseline",
      gap: spacing.sm,
      paddingBottom: spacing.sm,
      borderBottomWidth: 1,
      borderBottomColor: colors.line,
    },
    kicker: {
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
      fontWeight: "600",
    },
    count: {
      color: colors.ink3,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
    },
    list: {
      gap: spacing.sm,
    },
    change: {
      gap: spacing.xs,
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.md,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel,
    },
    row: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "baseline",
      gap: spacing.sm,
    },
    path: {
      flexShrink: 1,
      color: colors.ink,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.sm,
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
    note: {
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    entry: {
      alignSelf: "flex-start",
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.md,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
    },
    entryText: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

/** 一条改动。 */
function ChangeRow({
  change,
  openDiagnosticsEventId,
  onOpenChangeDiagnostics,
}: {
  readonly change: FileChangeView;
  readonly openDiagnosticsEventId: string | null;
  readonly onOpenChangeDiagnostics: ((eventId: string | null) => void) | undefined;
}): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const eventId = change.diagnostics.eventId;
  const diagnosticsOpen = openDiagnosticsEventId === eventId;

  return (
    <View
      role="listitem"
      style={styles.change}
      dataSet={{
        fileChange: change.changeId,
        changeKind: change.kind,
        changePath: change.displayPath,
        changeOutsideWorkspace: String(change.outsideWorkspace),
      }}
    >
      <View style={styles.row}>
        <Text style={styles.path} dataSet={{ changePathLabel: change.displayPath }}>
          {change.displayPath}
        </Text>
        <Text style={styles.chip}>{change.kind}</Text>
        {change.outsideWorkspace ? (
          <Text style={styles.chip} dataSet={{ changeOutsideWorkspaceNote: "true" }}>
            工作区之外
          </Text>
        ) : null}
      </View>

      <Text style={styles.summary} dataSet={{ changeSummary: "true" }}>
        {change.summary}
      </Text>

      <View style={styles.row}>
        {/* 「判定不出」时没有对应属性，而不是 `+0`。 */}
        {change.addedLines === null ? null : (
          <Text style={styles.added} dataSet={{ changeAddedLines: change.addedLines }}>
            {`+${change.addedLines}`}
          </Text>
        )}
        {change.deletedLines === null ? null : (
          <Text style={styles.deleted} dataSet={{ changeDeletedLines: change.deletedLines }}>
            {`-${change.deletedLines}`}
          </Text>
        )}
        {change.addedLines === null && change.deletedLines === null ? (
          <Text style={styles.note} dataSet={{ changeLineCountUnavailable: "true" }}>
            这次改动没有下发行数统计。
          </Text>
        ) : null}
      </View>

      <Text style={styles.note} dataSet={{ changeStatisticsScope: "true" }}>
        统计只覆盖 Agent 在工具调用里声明的改动；差异正文没有结构化来源，因此本栏不呈现。
      </Text>

      {onOpenChangeDiagnostics === undefined ? null : (
        <Pressable
          role="button"
          style={styles.entry}
          dataSet={{ action: "open-file-change-diagnostics", eventId, diagnosticsOpen: String(diagnosticsOpen) }}
          onPress={() => {
            onOpenChangeDiagnostics(diagnosticsOpen ? null : eventId);
          }}
        >
          <Text style={styles.entryText}>{diagnosticsOpen ? "收起 ACP 原文（诊断）" : "查看 ACP 原文（诊断）"}</Text>
        </Pressable>
      )}

      {diagnosticsOpen ? <ChangeDiagnosticsPanel diagnostics={change.diagnostics} /> : null}
    </View>
  );
}

/** 变更文件栏。 */
export function FileChangesPanel({
  changes,
  openDiagnosticsEventId,
  onOpenChangeDiagnostics,
}: FileChangesPanelProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const open = openDiagnosticsEventId ?? null;

  return (
    <View
      style={styles.panel}
      dataSet={{ fileChanges: changes.length === 0 ? "empty" : "list", fileChangeCount: changes.length }}
    >
      <View style={styles.head}>
        <Text style={styles.kicker}>变更文件</Text>
        <Text style={styles.count} dataSet={{ fileChangeCountLabel: String(changes.length) }}>
          {`${changes.length} 处`}
        </Text>
      </View>

      {changes.length === 0 ? (
        <Text style={styles.note} dataSet={{ fileChangesEmptyNote: "true" }}>
          这个会话还没有已呈现的文件改动（改动事件带结构化统计时才会出现在这里）。
        </Text>
      ) : (
        <View role="list" style={styles.list} dataSet={{ fileChangeList: "changes" }}>
          {changes.map((change) => (
            <ChangeRow
              key={change.changeId}
              change={change}
              openDiagnosticsEventId={open}
              onOpenChangeDiagnostics={onOpenChangeDiagnostics}
            />
          ))}
        </View>
      )}
    </View>
  );
}
