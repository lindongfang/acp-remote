/**
 * 目录详情页（R15）。
 *
 * 目录详情呈现「这个目录里的会话」与「在这个目录里新建会话」的入口（后者受 R16 的权限闸门
 * 控制），并在本次变更里接上三件事：目录内搜索、月份分档与「当前显示 N / M」、按可用宽度
 * 自适应的布局（`design.md` 的 D5/D6/D7/D8）。
 *
 * ## 三种「空」互不冒充
 *
 * - **别名不在快照里** → `data-directory-detail-state="unknown_directory"`，文案说的是链接失效；
 * - **目录在，但一条会话都没有** → `data-directory-sessions="empty"`，文案说的是「还没有会话」；
 * - **目录在、会话也在，只是被查询或月份档挡住** → `data-directory-detail-no-match="true"`，
 *   文案说的是「没有符合条件的会话」。它**不占用** `data-directory-sessions`：一旦占用，
 *   这一支就与上面那种空态混为一谈，R15 既有的「别名失效 vs 没有会话」负向断言也会被污染。
 *
 * 「当前显示 N / M」的两个数字全部来自视图模型（`shown` 随月份档变化、`total` 是查询命中后的
 * 总数、与月份档无关），组件不自己数一遍——否则两处口径迟早会分叉。
 *
 * ## 判定全在 feature 层
 *
 * 组件只渲染 `DirectoryDetailModel` 与调用方给的宽度：匹配、分档、N/M 都由
 * `src/features/directory-model.ts` 的纯函数算出（D3），组件里没有业务分支。
 *
 * ## 响应式（D7）
 *
 * RNW 没有媒体查询，`useWindowDimensions()` 是唯一的宽度来源。**窄屏与宽屏只差样式**：
 * 会话行、三项汇总、创建入口在两档下都渲染，`data-*` 集合完全相同（这是可被渲染测试与
 * 浏览器用例断言的「不因宽度少显示」）。`availableWidth` 只是把宽度变成可注入的入参，
 * 让 `renderToStaticMarkup`（Node 下 `Dimensions.get('window')` 恒为 0）能渲染出宽屏那一档。
 */

import type { ReactElement } from "react";

import type { DirectoryDetailModel } from "../features/directory-model";
import { CONTENT_ORIGIN_LABELS } from "../features/directory-model";
import type { CreateSessionEntryModel } from "../features/create-session-model";
import { SegmentControl } from "./SegmentControl";
import { SessionRow } from "./SessionRow";
import type { Palette } from "./theme";
import {
  Pressable,
  StyleSheet,
  Text,
  TextInput,
  View,
  fontFamily,
  fontSize,
  palettes,
  radius,
  spacing,
  useTheme,
  useWindowDimensions,
} from "./theme";

/** 宽屏断点（像素）：窄于此值单列，否则头部与工具条并排。 */
const WIDE_LAYOUT_MIN_WIDTH = 720;

export interface DirectoryDetailProps {
  readonly model: DirectoryDetailModel;
  readonly createEntry: CreateSessionEntryModel;
  readonly onOpenSession: (sessionId: string) => void;
  readonly onCreateSession: () => void;
  readonly canCreate: boolean;
  /** 目录内搜索词；缺省 `""`（等于不启用搜索）。 */
  readonly query?: string;
  /** 搜索词变化（页面负责写进仓库）。 */
  readonly onQueryChange?: (query: string) => void;
  /** 月份档切换（页面负责写进仓库）。 */
  readonly onMonthChange?: (month: string | "all") => void;
  /** 上次成功同步的时刻（epoch 毫秒）；断连时标注用。缺省 `null` = 没有可标注的时间。 */
  readonly lastSyncedAtMs?: number | null;
  /** 可用宽度（像素）；缺省取 `useWindowDimensions()`。渲染测试据此注入窄/宽两档。 */
  readonly availableWidth?: number;
}

/**
 * 同步时刻的展示形式：`YYYY-MM-DD HH:MM UTC`。
 *
 * 按 UTC 固定位置切片，不走本地时区：同一份输入必须渲染出同一段文本，否则组件测试的断言
 * 会随 CI 与开发机的时区漂移。原始毫秒数同时经 `dataSet` 原样进入 DOM，需要精确值时从属性取。
 */
function formatSyncedAt(lastSyncedAtMs: number): string {
  const iso = new Date(lastSyncedAtMs).toISOString();
  return `${iso.slice(0, 10)} ${iso.slice(11, 16)} UTC`;
}

/** 按调色板建样式表：浅/深各建一次（模块加载时），渲染路径上不再新建样式对象。 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    root: {
      gap: spacing.md,
    },
    headWide: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "flex-end",
      justifyContent: "space-between",
      gap: spacing.md,
    },
    headNarrow: {
      flexDirection: "column",
      alignItems: "flex-start",
      gap: spacing.sm,
    },
    titleBlock: {
      flexShrink: 1,
      minWidth: 0,
      gap: spacing.xs,
    },
    title: {
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xl,
      fontWeight: "600",
    },
    alias: {
      color: colors.ink3,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.sm,
    },
    counts: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      gap: spacing.sm,
    },
    badgeNeutral: {
      flexDirection: "row",
      alignItems: "baseline",
      gap: spacing.xs,
      paddingVertical: 2,
      paddingHorizontal: spacing.sm,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
      borderWidth: 1,
      borderColor: colors.line2,
    },
    badgeOk: {
      flexDirection: "row",
      alignItems: "baseline",
      gap: spacing.xs,
      paddingVertical: 2,
      paddingHorizontal: spacing.sm,
      borderRadius: radius.sm,
      backgroundColor: colors.okSoft,
    },
    badgeWarn: {
      flexDirection: "row",
      alignItems: "baseline",
      gap: spacing.xs,
      paddingVertical: 2,
      paddingHorizontal: spacing.sm,
      borderRadius: radius.sm,
      backgroundColor: colors.warnSoft,
    },
    badgeNeutralText: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    badgeOkText: {
      color: colors.ok,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    badgeWarnText: {
      color: colors.warn,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    originRow: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "baseline",
      gap: spacing.sm,
    },
    origin: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    syncedAt: {
      color: colors.ink3,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
    },
    toolbarWide: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      gap: spacing.md,
    },
    toolbarNarrow: {
      flexDirection: "column",
      alignItems: "stretch",
      gap: spacing.sm,
    },
    search: {
      flexGrow: 1,
      flexShrink: 1,
      minWidth: 200,
      minHeight: 36,
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.md,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel,
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
    },
    shown: {
      marginLeft: "auto",
      color: colors.ink3,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
    },
    list: {
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.md,
      backgroundColor: colors.panel,
      overflow: "hidden",
      boxShadow: colors.shadow,
    },
    emptyBox: {
      gap: spacing.xs,
      padding: spacing.lg,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.md,
      backgroundColor: colors.panel,
    },
    emptyTitle: {
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.lg,
      fontWeight: "600",
    },
    emptyGuidance: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
    },
    foot: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      gap: spacing.sm,
    },
    createButton: {
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.lg,
      borderRadius: radius.sm,
      backgroundColor: colors.accent,
    },
    createButtonDisabled: {
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.lg,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
      borderWidth: 1,
      borderColor: colors.line,
    },
    createText: {
      color: colors.accentInk,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
      fontWeight: "600",
    },
    createTextDisabled: {
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
    },
    hint: {
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    denied: {
      flexBasis: "100%",
      color: colors.warn,
      backgroundColor: colors.warnSoft,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.sm,
      borderRadius: radius.sm,
    },
    alert: {
      color: colors.danger,
      backgroundColor: colors.dangerSoft,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
      padding: spacing.md,
      borderRadius: radius.md,
    },
    waiting: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

/** 目录详情。 */
export function DirectoryDetail({
  model,
  createEntry,
  onOpenSession,
  onCreateSession,
  canCreate,
  query = "",
  onQueryChange,
  onMonthChange,
  lastSyncedAtMs = null,
  availableWidth,
}: DirectoryDetailProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const windowDimensions = useWindowDimensions();
  const width = availableWidth ?? windowDimensions.width;
  // 只影响布局：两档渲染的信息集合必须相同（MUST NOT 因宽度不足而隐藏任何一项）。
  const wide = width >= WIDE_LAYOUT_MIN_WIDTH;

  if (model.kind === "loading") {
    return (
      <View style={styles.root} dataSet={{ route: "dir-detail" }}>
        <Text style={styles.waiting} dataSet={{ directoryDetailState: "loading" }}>
          尚未收到第一份快照，正在等待同步……
        </Text>
      </View>
    );
  }

  if (model.kind === "unknown_directory") {
    return (
      <View style={styles.root} dataSet={{ route: "dir-detail", directoryAlias: model.alias }}>
        <Text role="alert" style={styles.alert} dataSet={{ directoryDetailState: "unknown_directory" }}>
          {`目录「${model.alias}」不在最近一次同步的目录列表里。它可能已被删除，或这个链接来自更早的一次同步。`}
        </Text>
      </View>
    );
  }

  const directory = model.directory;
  const trimmedQuery = query.trim();
  // 查询词为空、且这个目录本来就没有任何会话：这时说「没有符合条件的会话」会把「还没有会话」
  // 与「被查询挡住」两件事混在一起。两个条件都取自视图模型，组件不自己判断命中。
  const noSessionsAtAll = trimmedQuery.length === 0 && model.total === 0;
  const syncedAtMs = model.contentOrigin === "last_sync" ? lastSyncedAtMs : null;
  const monthOptions: readonly { readonly value: string; readonly label: string }[] = [
    { value: "all", label: "全部" },
    ...model.months.map((bucket) => ({ value: bucket.key, label: bucket.key })),
  ];

  return (
    <View style={styles.root} dataSet={{ route: "dir-detail", directoryAlias: directory.alias }}>
      {/* 目录项仍然只有展示名与别名：没有路径，也无处可填。 */}
      <View style={wide ? styles.headWide : styles.headNarrow}>
        <View style={styles.titleBlock}>
          <Text style={styles.title} dataSet={{ directoryDisplayName: directory.displayName }}>
            {directory.displayName}
          </Text>
          <Text style={styles.alias} dataSet={{ directoryAliasLabel: "true" }}>
            {directory.alias}
          </Text>
        </View>
        {/* 三项汇总来自会话摘要，因此断连时仍然呈现；月份档与搜索都不改变它们。 */}
        <View style={styles.counts}>
          <View style={styles.badgeNeutral}>
            <Text style={styles.badgeNeutralText}>会话</Text>
            <Text style={styles.badgeNeutralText} dataSet={{ directoryCount: "total" }}>
              {String(directory.counts.total)}
            </Text>
          </View>
          <View style={styles.badgeOk}>
            <Text style={styles.badgeOkText}>运行中</Text>
            <Text style={styles.badgeOkText} dataSet={{ directoryCount: "active" }}>
              {String(directory.counts.active)}
            </Text>
          </View>
          <View style={styles.badgeWarn}>
            <Text style={styles.badgeWarnText}>待处理</Text>
            <Text style={styles.badgeWarnText} dataSet={{ directoryCount: "pending" }}>
              {String(directory.counts.pending)}
            </Text>
          </View>
        </View>
      </View>

      <View style={styles.originRow}>
        <Text style={styles.origin} dataSet={{ contentOrigin: model.contentOrigin }}>
          {CONTENT_ORIGIN_LABELS[model.contentOrigin]}
        </Text>
        {/* 从未成功同步过（`lastSyncedAtMs` 为 null）时不呈现任何同步时间。 */}
        {syncedAtMs === null ? null : (
          <Text style={styles.syncedAt} dataSet={{ syncedAt: String(syncedAtMs) }}>
            {`已同步 ${formatSyncedAt(syncedAtMs)}`}
          </Text>
        )}
      </View>

      {/* 工具条：目录内搜索 + 月份档 + 「当前显示 N / M」。两侧口径相同的一对数来自视图模型。 */}
      <View style={wide ? styles.toolbarWide : styles.toolbarNarrow}>
        <TextInput
          style={styles.search}
          value={query}
          placeholder="在此目录内搜索"
          accessibilityLabel="在此目录内搜索"
          onChangeText={(next) => {
            onQueryChange?.(next);
          }}
          dataSet={{ directoryDetailSearch: "true", directoryDetailSearchValue: query }}
        />
        <SegmentControl
          label="月份"
          // 用**生效值**（模型已对账）：请求的档若已不在 `months` 里会回落 `"all"`，
          // 直取仓库里的原始值会让整组选项的选中态全空，与列表内容自相矛盾。
          value={model.month}
          options={monthOptions}
          onChange={(next) => {
            onMonthChange?.(next);
          }}
        />
        <Text
          style={styles.shown}
          dataSet={{ directoryShown: String(model.shown), directoryTotal: String(model.total) }}
        >
          {`当前显示 ${String(model.shown)} / ${String(model.total)}`}
        </Text>
      </View>

      {model.sessions.length > 0 ? (
        // 列表语义用 role 补回：容器的 `list` 渲染成 `<ul>`、`SessionRow` 的 `listitem` 渲染成 `<li>`。
        <View role="list" style={styles.list} dataSet={{ directorySessions: "list" }}>
          {model.sessions.map((session) => (
            <SessionRow
              key={session.id}
              session={session}
              onOpen={() => {
                onOpenSession(session.id);
              }}
            />
          ))}
        </View>
      ) : noSessionsAtAll ? (
        <View style={styles.emptyBox}>
          <Text style={styles.emptyTitle} dataSet={{ directorySessions: "empty" }}>
            这个目录里还没有会话。
          </Text>
          <Text style={styles.emptyGuidance}>用下面的入口在这个目录里创建一个会话。</Text>
        </View>
      ) : (
        <View style={styles.emptyBox}>
          <Text style={styles.emptyTitle} dataSet={{ directoryDetailNoMatch: "true" }}>
            没有符合条件的会话
          </Text>
          <Text style={styles.emptyGuidance}>换个关键词，或把月份切回「全部」。</Text>
        </View>
      )}

      <View style={styles.foot}>
        <Pressable
          role="button"
          disabled={!canCreate}
          style={canCreate ? styles.createButton : styles.createButtonDisabled}
          dataSet={{ action: "open-create-sheet" }}
          onPress={onCreateSession}
        >
          <Text style={canCreate ? styles.createText : styles.createTextDisabled}>
            在这个目录里新建会话
          </Text>
        </Pressable>
        {/* 副作用提示与权限无关：入口**始终**在，说明也就始终在（不因不可用而换掉解释）。 */}
        <Text style={styles.hint} dataSet={{ createSideEffect: "true" }}>
          将在你的电脑上启动一个 Agent 进程
        </Text>
        {canCreate ? null : (
          <Text style={styles.denied} dataSet={{ createEntryUnavailable: "true" }}>
            {createEntry.permission.note}
          </Text>
        )}
      </View>
    </View>
  );
}
