/**
 * 目录列表（R15）。
 *
 * ## 三条 MUST 在渲染上的落点
 *
 * - **只显示展示名与别名**：一行里只有 `displayName` 与 `alias` 两段文字，加上三项会话汇总。
 *   组件**没有**任何接收路径的 props，因此「顺手把 path 也显示出来」没有表达方式；
 *   内联会话行同理，只渲染 `SessionRow`（它只有标题、Agent、状态与时间）。
 * - **同名以别名区分**：`alias` 始终渲染（不放在「同名时才显示」的分支里）——
 *   那种分支正是路径消歧得以藏身的地方。
 * - **两种空态可区分**：`EmptyState` 的 `kind` 取自 `DirectoryEmptyState`，且标题与指引
 *   来自两套不同的词表。
 *
 * ## 三种「空」互不冒充（design D5）
 *
 * - `empty/no_directories`、`empty/directories_without_sessions` → `EmptyState`（带 `data-empty-state`）；
 *   其中「目录存在但无会话」这一支**还要在其下照常列出目录项**（design D1-1）：
 *   空态指引写着「在下面选一个目录进入详情页」，列表就必须真的存在，否则指引指向空处。
 *   列表容器仍是同一个 `data-directory-list="directories"`，卡片也仍是同一份渲染；
 *   `no_directories` 时 `model.directories` 是空数组 → 不呈现列表（spec：一个目录都没有时
 *   MUST NOT 呈现任何目录项）。
 * - `no_match`（有快照、有目录，但当前查询/筛选没有命中）→ **另一处**呈现「没有匹配的结果」，
 *   **不占用** `data-empty-state`（否则会污染 R15-4 的空态负向断言）；
 * - `loading`（从未同步过）→ 等待同步，同样不占用任何一种空态。
 *
 * ## 交互
 *
 * - **搜索框**：受控（`query` + `onQueryChange`），判定在 feature 层（`buildDirectoryPageModel`），
 *   组件只把值画出来、把输入上抛；缺省不呈现（没有消费者的输入框是坏的）。
 * - **筛选段**：`SegmentControl`，取值 `all`/`running`/`pending`，同样受控。
 * - **内联会话行**：`sessionsByAlias` 由页面从 store 取（只依赖快照，与详情页的查询词/月份无关），
 *   每个目录项只列前 `MAX_INLINE_SESSIONS` 条（原型 `MAX_ROWS = 5`）。
 * - **进入详情页的两条路径**（design D1/D3）：卡片**标题**（`data-action="open-directory"`，
 *   与会话数无关）与**「查看全部 N 个 ›」**（`data-action="open-directory-all"`，只有该目录有
 *   会话时才给出；N 是目录项的 `counts.total`，全量口径而非内联行数）。两者都走
 *   `onOpenDirectory`，动作取值可区分是为了让消费方能分别断言。
 * - **目录内创建**：`onCreateSession(alias)`；缺省不给时卡片不呈现「⊕ 新建」。
 *   是否给出由页面按权限决定——组件不做权限判定。
 * - **折叠**：每个目录项**各自**的局部状态，翻转一个别名不影响其它目录项。
 * - **同步时间**：`lastSyncedAtMs` 非 `null` 才画；从未成功替换过快照时为 `null`，
 *   此时绝不编造时刻（spec「从未成功同步」MUST NOT）。
 *
 * 组件是纯函数：所有判定与数据来自 props 里的视图模型，交互一律回调上抛，不 import store。
 */

import { useState } from "react";
import type { ReactElement } from "react";

import type { DirectoryFilter, DirectoryPageModel, DirectoryRowModel, SessionRowModel } from "../features/directory-model";
import { CONTENT_ORIGIN_LABELS, DIRECTORY_EMPTY_COPY } from "../features/directory-model";
import { DirectoryCard } from "./DirectoryCard";
import { EmptyState } from "./EmptyState";
import { SegmentControl } from "./SegmentControl";
import { SessionRow } from "./SessionRow";
import type { Palette } from "./theme";
import {
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

/** 内联会话行的展示上限（原型 `MAX_ROWS = 5`）。 */
const MAX_INLINE_SESSIONS = 5;

/** 宽屏断点（design D7）：可用宽度不小于它时目录卡片并排，否则单列。 */
const WIDE_BREAKPOINT = 720;

/** 筛选段的取值与文案（spec：全部 / 运行中 / 待处理）。 */
const FILTER_OPTIONS = [
  { value: "all", label: "全部" },
  { value: "running", label: "运行中" },
  { value: "pending", label: "待处理" },
] as const satisfies readonly { readonly value: DirectoryFilter; readonly label: string }[];

export interface DirectoryListProps {
  readonly model: DirectoryPageModel;
  /** 进入某个目录详情（页面负责导航）。 */
  readonly onOpenDirectory: (alias: string) => void;
  /**
   * 在该目录内创建会话（页面负责打开弹层与提交）；缺省表示没有这个能力，
   * 此时卡片不呈现「⊕ 新建」。是否给出由页面按 `session.create` 权限决定。
   */
  readonly onCreateSession?: (alias: string) => void;
  /**
   * 内联会话行的来源：按目录别名分组的会话行（页面从 store 取）。
   * 缺省空 Map，等价于「只呈现目录项、不列会话行」。
   */
  readonly sessionsByAlias?: ReadonlyMap<string, readonly SessionRowModel[]>;
  /** 受控搜索框的当前值；缺省 `""`。 */
  readonly query?: string;
  /** 搜索框的输入回调；缺省不呈现搜索框。 */
  readonly onQueryChange?: (query: string) => void;
  /** 受控筛选段的当前值；缺省 `"all"`。 */
  readonly filter?: DirectoryFilter;
  /** 筛选段的切换回调；缺省不呈现筛选段。 */
  readonly onFilterChange?: (filter: DirectoryFilter) => void;
  /** 打开某个会话（页面负责导航）；缺省时会话行只是展示。 */
  readonly onOpenSession?: (sessionId: string) => void;
  /**
   * 无创建会话权限时的提示条文案；`null`/缺省不呈现（有权限时不留痕，因此「缺省等价于有权限」）。
   * 文案由页面给出：两页共用同一句来源（`CreateSessionPermission.note`），不各写一套说法。
   */
  readonly createPermissionNote?: string | null;
}

/** 按调色板建样式表：浅/深各建一次（模块加载时），渲染路径上不再新建样式对象。 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    page: {
      gap: spacing.md,
    },
    origin: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "baseline",
      gap: spacing.sm,
    },
    originText: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    syncedAt: {
      color: colors.ink3,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
    },
    notice: {
      backgroundColor: colors.warnSoft,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.md,
    },
    noticeText: {
      color: colors.warn,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    toolbar: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      gap: spacing.sm,
    },
    search: {
      flexGrow: 1,
      flexShrink: 1,
      minWidth: 200,
      paddingVertical: 8,
      paddingHorizontal: spacing.md,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel,
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
    },
    // 窄屏单列；宽屏并排（同一份子节点，只有方向与换行不同——信息集合不因宽度变化）。
    list: {
      flexDirection: "column",
      gap: spacing.md,
    },
    listWide: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "stretch",
      gap: spacing.md,
    },
    sessions: {
      flexDirection: "column",
    },
    blank: {
      backgroundColor: colors.panel,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.md,
      padding: spacing.md,
      gap: spacing.xs,
    },
    blankTitle: {
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.lg,
      fontWeight: "600",
    },
    blankText: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    loading: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

/** 目录列表。 */
export function DirectoryList(props: DirectoryListProps): ReactElement {
  const {
    model,
    onOpenDirectory,
    onCreateSession,
    sessionsByAlias,
    query = "",
    onQueryChange,
    filter = "all",
    onFilterChange,
    onOpenSession,
    createPermissionNote = null,
  } = props;
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const { width } = useWindowDimensions();
  const wide = width >= WIDE_BREAKPOINT;
  /**
   * 折叠是**视图内部**的临时状态：它不需要跨页存活（查询与筛选需要，因此那两个值住在
   * store 里），也不改变任何数据判定——所以留在这里，而不是又开一个 store 字段。
   */
  const [collapsedAliases, setCollapsedAliases] = useState<readonly string[]>([]);

  if (model.kind === "loading") {
    return (
      <View style={styles.page} dataSet={{ route: "dirs" }}>
        <Text style={styles.loading} dataSet={{ directoryState: "loading" }}>
          尚未收到第一份快照，正在等待同步……
        </Text>
      </View>
    );
  }

  /**
   * 上次同步时刻的展示形式：按 **UTC 值固定位置切片**（`YYYY-MM-DD HH:MM UTC`）。
   *
   * 刻意不用 `Date` 的本地时区格式化、也不画「N 分钟前」：前者会在 Windows 开发机与 Linux CI
   * 之间漂移，后者需要一个时钟，而 `renderToStaticMarkup` 与浏览器断言都不该依赖运行时刻——
   * 同一份视图模型必须在任何机器上渲染出同一段文本。精确值另经 `data-synced-at` 原样进 DOM。
   */
  const syncedAtClock =
    model.lastSyncedAtMs === null
      ? null
      : `${new Date(model.lastSyncedAtMs).toISOString().slice(0, 16).replace("T", " ")} UTC`;

  /**
   * 一项目录卡片。`ready` 与「目录存在但无会话」的 `empty` 共用这一份渲染：两种分支下卡片的
   * 形状、属性与入口**不会分叉**——空态里那张卡片与别处的卡片是同一个东西。
   */
  const renderDirectoryCard = (directory: DirectoryRowModel): ReactElement => {
    const collapsed = collapsedAliases.includes(directory.alias);
    const inline = (sessionsByAlias?.get(directory.alias) ?? []).slice(0, MAX_INLINE_SESSIONS);
    return (
      <DirectoryCard
        key={directory.alias}
        displayName={directory.displayName}
        alias={directory.alias}
        counts={directory.counts}
        // 最近活动时间由模型给出（该目录下会话 `updatedAt` 的最大值）；无会话为 `null`。
        latestActivityAt={directory.latestActivityAt}
        collapsed={collapsed}
        onToggleCollapsed={() => {
          // 只翻转本项的别名：其它目录项的折叠态不受影响（spec 场景「目录项折叠」）。
          setCollapsedAliases(
            collapsedAliases.includes(directory.alias)
              ? collapsedAliases.filter((item) => item !== directory.alias)
              : [...collapsedAliases, directory.alias],
          );
        }}
        // 标题入口：无论该目录有多少会话都可用（0 会话的目录同样可达详情页，spec 场景）。
        onOpen={() => {
          onOpenDirectory(directory.alias);
        }}
        // 「查看全部 N 个 ›」：只要该目录有会话就给出（原型同此），否则会呈现「查看全部 0 个」。
        onOpenAll={
          directory.counts.total === 0
            ? null
            : () => {
                onOpenDirectory(directory.alias);
              }
        }
        // 目录内创建：调用方没给回调就不呈现（调用方按权限决定给不给）。
        onCreate={
          onCreateSession === undefined
            ? null
            : () => {
                onCreateSession(directory.alias);
              }
        }
      >
        {inline.length === 0 ? null : (
          <View role="list" style={styles.sessions}>
            {inline.map((row) =>
              // `exactOptionalPropertyTypes`：`onOpenSession` 缺省时不能显式传 `undefined`。
              onOpenSession === undefined ? (
                <SessionRow key={row.id} session={row} />
              ) : (
                <SessionRow
                  key={row.id}
                  session={row}
                  onOpen={() => {
                    onOpenSession(row.id);
                  }}
                />
              ),
            )}
          </View>
        )}
      </DirectoryCard>
    );
  };

  /**
   * 目录卡片列表。`role="list"` 让 RNW 渲染成 `<ul>`，目录卡片与内联会话行各自
   * `role="listitem"` → `<li>`。容器与属性在 `ready` 与空态两支下是同一个
   * （`data-directory-list="directories"`），消费方不必按分支换选择器。
   */
  const directoryCardList = (directories: readonly DirectoryRowModel[]): ReactElement => (
    <View
      role="list"
      style={wide ? styles.listWide : styles.list}
      dataSet={{ directoryList: "directories" }}
    >
      {directories.map(renderDirectoryCard)}
    </View>
  );

  return (
    <View style={styles.page} dataSet={{ route: "dirs" }}>
      {/* 内容来源：断连时必须看得出这些内容来自上次同步。 */}
      <View style={styles.origin} dataSet={{ contentOrigin: model.contentOrigin }}>
        <Text style={styles.originText}>{CONTENT_ORIGIN_LABELS[model.contentOrigin]}</Text>
        {model.lastSyncedAtMs === null || syncedAtClock === null ? null : (
          // 从未成功替换过快照时 `lastSyncedAtMs` 为 `null`：那时**不**画时间（spec MUST NOT）。
          <Text style={styles.syncedAt} dataSet={{ syncedAt: String(model.lastSyncedAtMs) }}>
            {`已同步 ${syncedAtClock}`}
          </Text>
        )}
      </View>

      {createPermissionNote === null ? null : (
        <View style={styles.notice} dataSet={{ createPermissionNote: "true" }}>
          <Text style={styles.noticeText}>{createPermissionNote}</Text>
        </View>
      )}

      {/* 空态下没有可搜索/可筛选的内容，因此不呈现工具条；「搜索无结果」时**必须**呈现，
          否则用户没有把关键词或筛选改回去的入口。 */}
      {model.kind === "empty" ? null : (
        <View style={styles.toolbar}>
          {onQueryChange === undefined ? null : (
            <TextInput
              value={query}
              onChangeText={onQueryChange}
              placeholder="搜索目录或会话"
              placeholderTextColor={theme.colors.ink3}
              accessibilityLabel="搜索目录"
              style={styles.search}
              // `data-directory-search` 是 e2e 定位输入框的锚点；当前值另存一份，便于断言
              // 「返回目录页时查询词仍在」（受控控件的 value 在 DOM 里只有属性值可读）。
              dataSet={{ directorySearch: "true", directorySearchValue: query }}
            />
          )}
          {onFilterChange === undefined ? null : (
            <SegmentControl
              label="筛选"
              value={filter}
              options={FILTER_OPTIONS}
              onChange={onFilterChange}
            />
          )}
        </View>
      )}

      {model.kind === "empty" ? (
        <>
          {/* 空态标记与文案保持既有取值（`data-empty-state` 仍只在这一支出现）；
              其下**照常**列出已登记的目录项：指引写着「在下面选一个目录进入详情页」，
              列表就必须真的存在（design D1-1）。`no_directories` 时 `directories` 是空数组，
              因此不呈现列表，也就没有任何目录项。 */}
          <EmptyState
            kind={model.empty}
            title={DIRECTORY_EMPTY_COPY[model.empty].title}
            guidance={DIRECTORY_EMPTY_COPY[model.empty].guidance}
          />
          {model.directories.length === 0 ? null : directoryCardList(model.directories)}
        </>
      ) : model.kind === "no_match" ? (
        // 数据在、只是被查询/筛选挡住：与两种 `data-empty-state` 是**互不冒充**的第三种呈现。
        <View style={styles.blank} dataSet={{ directoryNoMatch: "true" }}>
          <Text style={styles.blankTitle}>没有匹配的结果</Text>
          <Text style={styles.blankText}>换个关键词，或把筛选切回「全部」。</Text>
        </View>
      ) : (
        directoryCardList(model.directories)
      )}
    </View>
  );
}
