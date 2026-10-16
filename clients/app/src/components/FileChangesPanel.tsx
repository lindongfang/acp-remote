/**
 * 变更文件栏（spec「变更文件的呈现边界」；`design.md` D6/D8/D10）。
 *
 * ## 差异行只来自结构化来源，三种形态互不冒充
 *
 * 每处改动的差异区由 `FileChangeView.diff`（`domain/timeline.ts` 的 `FileDiffView` 判别联合）
 * 决定，**只有三支**：
 *
 * - `lines`：服务端随事件下发了结构化差异行 → 逐行呈现，新增/删除/上下文各一档底色，行内容按
 *   **不可信文本**处理（只经 `Text` 呈现，不解析标记、不执行链接、不插 HTML）；
 * - `unavailable`：服务端说明了为什么没有 → 呈现 `FILE_DIFF_UNAVAILABLE_LABELS` 里该原因的
 *   已登记文案；
 * - `null`：两者都没有 → **明确说明**这一处原文未同步或超出单条上限，不渲染占位差异行假装完整。
 *
 * 组件不解析 `acp.rawJson`，也不据它生成任何差异行——「把原文解析后渲染成差异」在这里没有数据
 * 通路；原文的唯一去处是每条改动自己的诊断入口（`ChangeDiagnosticsPanel` 里作为一块不可信文本
 * 呈现）。
 *
 * ## 判定不出的统计省略而不是填零
 *
 * `addedLines` / `deletedLines` 是 `string | null`：`null` 表示「没有依据」（模型已按 D8 处理）。
 * 组件只在非 `null` 时渲染对应标记，两侧都为 `null` 时给出一句说明——填 `+0 / -0` 会把
 * 「不知道」说成「没有改动」。
 *
 * ## 展开态与选中项由页面持有
 *
 * 哪一条的诊断是展开的、哪一处改动被选中，都属于页面状态（与目录页的查询词同理：组件是纯函数，
 * 只吃 props）。选中项与时间线上带文件标识的条目**共用同一份**状态，因此从条目点进来时右栏已经
 * 选中同一个文件（spec「从条目联动到右栏」）。回调缺省时不渲染对应的可点控件，而不是渲染一个点
 * 了没有反应的按钮。
 */

import type { ReactElement } from "react";

import type { DiffLineView, FileDiffView } from "../domain";
import { FILE_DIFF_UNAVAILABLE_LABELS } from "../domain";
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
  /** 当前选中的那一处改动（`changeId`，与时间线条目共用同一份页面状态）；缺省 `null` = 未选中。 */
  readonly selectedChangeId?: string | null;
  /** 选中一处改动（上抛 `changeId`）；缺省时改动的路径不是可点控件。 */
  readonly onSelectChange?: ((changeId: string) => void) | undefined;
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
    /** 与时间线条目联动时的那一处：一眼看得出右栏已经选中它。 */
    changeSelected: {
      borderColor: colors.accent,
      backgroundColor: colors.accentSoft,
    },
    row: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "baseline",
      gap: spacing.sm,
    },
    selectablePath: {
      paddingVertical: 2,
      paddingHorizontal: spacing.xs,
      borderRadius: radius.sm,
      borderWidth: 1,
      borderColor: colors.line,
      backgroundColor: colors.panel2,
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
    /** 差异区：一列等宽行，新增/删除各一档底色、上下文行用弱化色（原型 `.diff`）。 */
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

/** 差异行的三档底色（词表封闭：`domain/timeline.ts` 的 `DiffLineKind`）。 */
const DIFF_LINE_STYLES: Readonly<Record<DiffLineView["kind"], "diffAdded" | "diffDeleted" | "diffContext">> = {
  added: "diffAdded",
  deleted: "diffDeleted",
  context: "diffContext",
};

/**
 * 一处改动的差异区：三种形态各一条分支，且**互不冒充**。
 *
 * 行内容按不可信文本处理：整行只经 `Text` 呈原样字符串，不解析行内标记、不识别链接、不插 HTML。
 * 新增/删除/上下文三档**只改底色**，不改变文本本身。
 */
function ChangeDiff({ diff }: { readonly diff: FileDiffView | null }): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

  if (diff === null) {
    return (
      <Text style={styles.note} dataSet={{ diffNotDelivered: "true" }}>
        这一处没有随事件下发结构化差异行：原文未同步，或超出单条上限。这里不渲染占位差异行，完整原文的入口在下方诊断里。
      </Text>
    );
  }

  if (diff.kind === "unavailable") {
    return (
      <Text style={styles.note} dataSet={{ diffUnavailable: diff.reason }}>
        {FILE_DIFF_UNAVAILABLE_LABELS[diff.reason]}
      </Text>
    );
  }

  return (
    <View style={styles.diff} dataSet={{ diffRegion: "lines", diffLineCount: diff.lines.length }}>
      {diff.lines.map((line, index) => (
        <Text
          // 差异行没有标识字段，按声明次序做键（它只是呈现，不参与任何判定）。
          key={`diff:${String(index)}`}
          style={[styles.diffLine, styles[DIFF_LINE_STYLES[line.kind]]]}
          dataSet={{ diffLineKind: line.kind }}
        >
          {line.text}
        </Text>
      ))}
    </View>
  );
}

/** 一条改动。 */
function ChangeRow({
  change,
  selected,
  openDiagnosticsEventId,
  onOpenChangeDiagnostics,
  onSelectChange,
}: {
  readonly change: FileChangeView;
  readonly selected: boolean;
  readonly openDiagnosticsEventId: string | null;
  readonly onOpenChangeDiagnostics: ((eventId: string | null) => void) | undefined;
  readonly onSelectChange: ((changeId: string) => void) | undefined;
}): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const eventId = change.diagnostics.eventId;
  const diagnosticsOpen = openDiagnosticsEventId === eventId;

  return (
    <View
      role="listitem"
      style={selected ? [styles.change, styles.changeSelected] : styles.change}
      dataSet={{
        fileChange: change.changeId,
        fileChangeSelected: String(selected),
        changeKind: change.kind,
        changePath: change.displayPath,
        changeOutsideWorkspace: String(change.outsideWorkspace),
      }}
    >
      <View style={styles.row}>
        {onSelectChange === undefined ? (
          <Text style={styles.path} dataSet={{ changePathLabel: change.displayPath }}>
            {change.displayPath}
          </Text>
        ) : (
          <Pressable
            role="button"
            style={styles.selectablePath}
            dataSet={{ action: "select-file-change", changeId: change.changeId, changePathLabel: change.displayPath }}
            onPress={() => {
              onSelectChange(change.changeId);
            }}
          >
            <Text style={styles.path}>{change.displayPath}</Text>
          </Pressable>
        )}
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
        统计只覆盖 Agent 在工具调用里声明的改动；差异行只来自服务端下发的结构化来源，本栏不解析 ACP 原文。
      </Text>

      <ChangeDiff diff={change.diff} />

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
  selectedChangeId,
  onSelectChange,
}: FileChangesPanelProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const open = openDiagnosticsEventId ?? null;
  const selected = selectedChangeId ?? null;

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
              selected={change.changeId === selected}
              openDiagnosticsEventId={open}
              onOpenChangeDiagnostics={onOpenChangeDiagnostics}
              onSelectChange={onSelectChange}
            />
          ))}
        </View>
      )}
    </View>
  );
}
