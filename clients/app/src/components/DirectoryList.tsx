/**
 * 目录列表（R15）。
 *
 * ## 卡片与卡体
 *
 * - **卡片头**由 `DirectoryCard` 按原型的槽位渲染（折叠箭头、目录图标、展示名、同名消歧的
 *   路径尾段、计数胶囊、运行中/待处理徽标、弹性空白、`⊕ 新建`、`查看全部 ›`）。
 * - **卡体是缩进树**：本组件只把内联会话行（`SessionRow`）或那一行「这个目录还没有会话」
 *   交给卡片的 `children`，缩进与左边线由卡片负责。
 *
 * ## 路径与目录标识
 *
 * 目录项上唯一的路径来源是 `DirectoryRowModel.pathTail`（feature 层已按「同名才给」判定过：
 * 展示名唯一时为 `null`）。组件**不**拼接、不规范化和不反查路径，也没有从目录标识或展示名推出
 * 路径的表达方式。目录标识不进界面：它**只**作为根节点的 `data-directory-id`（路由与命令引用
 * 的内部标识、e2e 的锚点），本组件不把它渲染成任何文字。
 *
 * ## 三种「空」互不冒充
 *
 * - `empty/no_directories`、`empty/directories_without_sessions` → `EmptyState`（带 `data-empty-state`）；
 *   其中「目录存在但无会话」这一支**还要在其下照常列出目录项**：空态指引写着「在下面选一个
 *   目录进入详情页」，列表就必须真的存在，否则指引指向空处。列表容器仍是同一个
 *   `data-directory-list="directories"`，卡片也仍是同一份渲染；`no_directories` 时
 *   `model.directories` 是空数组 → 不呈现列表。
 * - `no_match`（有快照、有目录，但当前查询/筛选没有命中）→ **另一处**呈现「没有匹配的结果」，
 *   **不占用** `data-empty-state`（否则会污染 R15-4 的空态负向断言）；
 * - `loading`（从未同步过）→ 等待同步，同样不占用任何一种空态。
 *
 * ## 交互
 *
 * - **搜索框**：受控（`query` + `onQueryChange`），判定在 feature 层（`buildDirectoryPageModel`），
 *   组件只把值画出来、把输入上抛；缺省不呈现（没有消费者的输入框是坏的）。
 * - **筛选段**：`SegmentControl`，取值 `all`/`running`/`pending`，同样受控。
 * - **内联会话行**：`sessionsByDirectoryId` 由页面从 store 取（只依赖快照，与详情页的查询词/月份无关），
 *   每个目录项只列前 `MAX_INLINE_SESSIONS` 条（原型 `MAX_ROWS = 5`）。
 * - **进入详情页的两条路径**（与卡片头一一对应）：**展示名**（`data-action="open-directory"`，
 *   与会话数无关）与**「查看全部 ›」**（`data-action="open-directory-all"`，只有该目录有会话时
 *   才给出）。两者都走 `onOpenDirectory`，动作取值可区分是为了让消费方能分别断言。
 * - **目录内创建**：`onCreateSession(directoryId)`（**必填**）；每个目录项恒呈现「⊕ 新建」——
 *   设备不再有创建会话的权限维度，因此组件不做也不接受任何权限判定或说明。
 * - **工具条右端的顶部创建入口**：`onCreateSessionToBrowse()`（**可选**，缺省不呈现），画在搜索框与
 *   筛选段右边（原型工具条的位置就是那里）。它与卡头上的「⊕ 新建」是两个可区分的控件：前者目录未知、
 *   后者目录已定（`onCreateSession(directoryId)`）。它**不**受「空态/加载中不呈现搜索框与筛选段」的约束——
 *   一个目录都没记录或还在等第一份快照时，它恰恰是目录页上唯一的创建路径。
 * - **折叠**：每个目录项**各自**的局部状态，翻转一个目录标识不影响其它目录项。
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
  Pressable,
  StyleSheet,
  Text,
  TextInput,
  View,
  breakpoints,
  fontFamily,
  fontSize,
  fontWeight,
  palettes,
  radius,
  spacing,
  useTheme,
  useWindowDimensions,
} from "./theme";

/** 内联会话行的展示上限（原型 `MAX_ROWS = 5`）。 */
const MAX_INLINE_SESSIONS = 5;

/**
 * 宽屏断点（design D7）：可用宽度不小于它时目录卡片并排，否则单列。
 *
 * 取值来自 `theme.ts` 的 `breakpoints.wide`（D7/D10 要求这个数只在一处出现），
 * 本组件不再自己写一次字面量。
 */
const WIDE_BREAKPOINT = breakpoints.wide;

/** 筛选段的取值与文案（spec：全部 / 运行中 / 待处理）。 */
const FILTER_OPTIONS = [
  { value: "all", label: "全部" },
  { value: "running", label: "运行中" },
  { value: "pending", label: "待处理" },
] as const satisfies readonly { readonly value: DirectoryFilter; readonly label: string }[];

export interface DirectoryListProps {
  readonly model: DirectoryPageModel;
  /** 进入某个目录详情（页面负责导航）。 */
  readonly onOpenDirectory: (directoryId: string) => void;
  /**
   * 在该目录内创建会话（页面负责打开弹层与提交）。
   *
   * **必填**：设备不再有创建会话的权限维度，目录项上的创建入口恒可用，因此调用方没有
   * 「因为权限而不给回调」这个选项，组件也不再承接任何权限说明。
   */
  readonly onCreateSession: (directoryId: string) => void;
  /**
   * **顶部**创建入口（目录未知：「⊕ 新建」）：页面据此打开「逐层浏览并选定一层」那条链路。
   *
   * 可选：缺省不呈现该入口（本组件的其它消费方不带这条路径）。它画在工具条**右端**（原型的
   * 工具条就是这个位置），与每张卡头上的「⊕ 新建」是两个可区分的控件——后者的目录已经定了
   * （`onCreateSession(directoryId)`），前者的目录还没定。
   */
  readonly onCreateSessionToBrowse?: (() => void) | undefined;
  /**
   * 内联会话行的来源：按目录标识分组的会话行（页面从 store 取）。
   * 缺省空 Map，等价于「只呈现目录项、不列会话行」。
   */
  readonly sessionsByDirectoryId?: ReadonlyMap<string, readonly SessionRowModel[]>;
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
    toolbar: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      gap: spacing.sm,
    },
    // 顶部创建入口：贴近工具条右端（原型那里是 `spacer` + `⊕ 新建`）。搜索框的 `flexGrow` 先吃掉
    // 余量，`marginLeft: auto` 管的是窄屏换行后这一行只剩入口时它仍贴右端。
    createEntry: {
      marginLeft: "auto",
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.lg,
      borderRadius: radius.sm,
      backgroundColor: colors.accentSoft,
      borderWidth: 1,
      borderColor: colors.accent,
    },
    createEntryText: {
      color: colors.accentInk,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
      fontWeight: fontWeight.medium,
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
    // 卡体里的「这个目录还没有会话」一行：与卡片同一处缩进（缩进本身在卡片的卡体上）。
    emptyLine: {
      paddingVertical: spacing.md,
      paddingHorizontal: spacing.lg,
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    // 空态与「没有匹配的结果」都是原型 `.blank` 那一块：虚线边框、居中。
    blank: {
      alignItems: "center",
      backgroundColor: colors.panel,
      borderWidth: 1,
      borderStyle: "dashed",
      borderColor: colors.line,
      borderRadius: radius.md,
      paddingVertical: spacing.xl,
      paddingHorizontal: spacing.xl,
      gap: spacing.sm,
    },
    blankTitle: {
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.lg,
      fontWeight: "600",
    },
    blankText: {
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      textAlign: "center",
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
    onCreateSessionToBrowse,
    sessionsByDirectoryId,
    query = "",
    onQueryChange,
    filter = "all",
    onFilterChange,
    onOpenSession,
  } = props;
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const { width } = useWindowDimensions();
  const wide = width >= WIDE_BREAKPOINT;
  /**
   * 折叠是**视图内部**的临时状态：它不需要跨页存活（查询与筛选需要，因此那两个值住在
   * store 里），也不改变任何数据判定——所以留在这里，而不是又开一个 store 字段。
   */
  const [collapsedDirectoryIds, setCollapsedDirectoryIds] = useState<readonly string[]>([]);

  /**
   * 工具条右端的**顶部创建入口**（页面给了回调才画）。
   *
   * 它的出现**不**跟搜索框/筛选段走：那两者受「有没有可搜索/可筛选的内容」约束，而入口通向的是
   * 「逐层浏览主机目录」这条路——还没拿到快照、一个目录都没记录时它反而是页面上唯一的创建路径
   * （原型同样把 `⊕ 新建` 画在主工具栏里，与列表是否为空无关）。
   */
  const createEntry =
    onCreateSessionToBrowse === undefined ? null : (
      <Pressable
        role="button"
        style={styles.createEntry}
        dataSet={{ action: "create-session-browse" }}
        onPress={onCreateSessionToBrowse}
      >
        <Text style={styles.createEntryText}>⊕ 新建</Text>
      </Pressable>
    );

  if (model.kind === "loading") {
    return (
      <View style={styles.page} dataSet={{ route: "dirs" }}>
        {/* 还没有内容可搜索/可筛选，因此只有那一行工具条与它的入口（见 `createEntry`）。 */}
        {createEntry === null ? null : (
          <View style={styles.toolbar} dataSet={{ directoryToolbar: "true" }}>
            {createEntry}
          </View>
        )}
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
   *
   * 卡体只有两种：内联会话行，或那一行「这个目录还没有会话」。后者只在**该目录一条会话都没有**
   * 时呈现（`counts.total === 0`）——「没有会话数据源」与「这个目录没有会话」是两回事，
   * 前者只是不列行，不冒充后者的文案。
   */
  const renderDirectoryCard = (directory: DirectoryRowModel): ReactElement => {
    const collapsed = collapsedDirectoryIds.includes(directory.directoryId);
    const inline = (sessionsByDirectoryId?.get(directory.directoryId) ?? []).slice(0, MAX_INLINE_SESSIONS);
    return (
      <DirectoryCard
        key={directory.directoryId}
        displayName={directory.displayName}
        directoryId={directory.directoryId}
        // 同名消歧尾段：逐字来自服务端下发的路径，且只在同名时非 `null`（feature 层判定）。
        pathTail={directory.pathTail}
        counts={directory.counts}
        collapsed={collapsed}
        onToggleCollapsed={() => {
          // 只翻转本项的目录标识：其它目录项的折叠态不受影响（spec 场景「目录项折叠」）。
          setCollapsedDirectoryIds(
            collapsedDirectoryIds.includes(directory.directoryId)
              ? collapsedDirectoryIds.filter((item) => item !== directory.directoryId)
              : [...collapsedDirectoryIds, directory.directoryId],
          );
        }}
        // 展示名入口：无论该目录有多少会话都可用（0 会话的目录同样可达详情页，spec 场景）。
        onOpen={() => {
          onOpenDirectory(directory.directoryId);
        }}
        // 「查看全部 ›」：只要该目录有会话就给出，否则会呈现一个通向空页面的入口。
        onOpenAll={
          directory.counts.total === 0
            ? null
            : () => {
                onOpenDirectory(directory.directoryId);
              }
        }
        // 目录内创建：入口恒可用（页面无条件给出回调）。
        onCreate={() => {
          onCreateSession(directory.directoryId);
        }}
      >
        {inline.length > 0 ? (
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
        ) : directory.counts.total === 0 ? (
          <Text style={styles.emptyLine}>这个目录还没有会话</Text>
        ) : null}
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

      {/* 空态下没有可搜索/可筛选的内容，因此不呈现搜索框与筛选段；「搜索无结果」时**必须**呈现，
          否则用户没有把关键词或筛选改回去的入口。顶部创建入口不跟这条约束走（见上面 `createEntry`），
          它一个目录都没记录时更该在，所以只要它在，工具条这一行就画出来。 */}
      {model.kind === "empty" && createEntry === null ? null : (
        <View style={styles.toolbar} dataSet={{ directoryToolbar: "true" }}>
          {model.kind === "empty" || onQueryChange === undefined ? null : (
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
          {model.kind === "empty" || onFilterChange === undefined ? null : (
            <SegmentControl
              label="筛选"
              value={filter}
              options={FILTER_OPTIONS}
              onChange={onFilterChange}
            />
          )}
          {createEntry}
        </View>
      )}

      {model.kind === "empty" ? (
        <>
          {/* 空态标记与文案保持既有取值（`data-empty-state` 仍只在这一支出现）；
              其下**照常**列出已登记的目录项：指引写着「在下面选一个目录进入详情页」，
              列表就必须真的存在。`no_directories` 时 `directories` 是空数组，
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
