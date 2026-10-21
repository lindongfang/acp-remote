/**
 * 目录列表（R15）。
 *
 * ## 卡片与卡体
 *
 * - **卡片头**由 `DirectoryCard` 按原型的槽位渲染（折叠箭头、目录图标、展示名、同名消歧的
 *   路径尾段、计数胶囊、运行中/待处理徽标、弹性空白、`⊕ 新建`、`查看全部 ›`）。
 * - **卡体是缩进树**：本组件只把内联会话行（`SessionRow`）或那一行「这个目录还没有会话」
 *   交给卡片的 `children`，缩进与左边线由卡片负责。
 * - **目录卡片始终单列堆叠**（原型 `.stack`），不随窗口宽度改成并排。
 * - **卡体底部入口**：可见行超过内联上限时，卡体底部再渲染一个「查看全部 ›」
 *   （`data-action="open-directory-all-footer"`），与卡片头那个入口动作相同、取值不同。
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
 * - **内联会话行**：`sessionsByDirectoryId` 由页面从 store 取（数据源只依赖快照），随后按
 *   **当前查询/筛选**过滤（`filterSessionsForDirectory`，与原型 `passFilter` + `passQuery` 同口径），
 *   每个目录项只列过滤后的前 `MAX_INLINE_SESSIONS` 条（原型 `MAX_ROWS = 5`）。展示名命中查询词时
 *   保留全部（按筛选后的）行；只命中部分标题时只保留命中的行。
 * - **进入详情页的三条路径**：**展示名**（`data-action="open-directory"`，与会话数无关）、
 *   卡片头的**「查看全部 ›」**（`data-action="open-directory-all"`）与卡体底部的同名入口
 *   （`data-action="open-directory-all-footer"`，仅超过内联上限时出现）。三者都走
 *   `onOpenDirectory`，动作取值可区分是为了让消费方能分别断言。
 * - **目录内创建**：`onCreateSession(directoryId)`（**必填**）；每个目录项恒呈现「⊕ 新建」——
 *   设备不再有创建会话的权限维度，因此组件不做也不接受任何权限判定或说明。
 * - **顶部创建入口**：`onCreateSessionToBrowse()`（**可选**，缺省不呈现）。它始终画在工具条右端
 *   （搜索框与筛选段右边）；「一个目录都没有」的空态另在说明块里给一个常规尺寸的同一入口。
 *   它与卡头上的「⊕ 新建」是两个可区分的控件：前者目录未知、后者目录已定
 *   （`onCreateSession(directoryId)`）。
 * - **折叠**：每个目录项**各自**的局部状态，翻转一个目录标识不影响其它目录项。
 * - **排序与分页**：feature 层按「该目录最近一次会话时间」倒序排好并每页切 5 个；组件只在列表底部
 *   渲染固定翻页控件（`onPageChange` 缺省时不渲染）。页码纠正与草稿状态由共享 `Pager` 负责。
 *
 * 组件是纯函数：所有判定与数据来自 props 里的视图模型，交互一律回调上抛，不 import store。
 */

import { useState } from "react";
import type { ReactElement } from "react";

import type { DirectoryFilter, DirectoryPageModel, DirectoryRowModel, SessionRowModel } from "../features/directory-model";
import {
  DIRECTORY_EMPTY_COPY,
  filterSessionsForDirectory,
} from "../features/directory-model";
import { DirectoryCard } from "./DirectoryCard";
import { EmptyState } from "./EmptyState";
import { Pager } from "./Pager";
import { SegmentControl } from "./SegmentControl";
import { SessionRow } from "./SessionRow";
import type { Palette } from "./theme";
import {
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  TextInput,
  View,
  fontFamily,
  fontSize,
  fontWeight,
  palettes,
  radius,
  spacing,
  useTheme,
} from "./theme";

/** 内联会话行的展示上限（原型 `MAX_ROWS = 5`）。 */
const MAX_INLINE_SESSIONS = 5;

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
   * 可选：缺省不呈现该入口（本组件的其它消费方不带这条路径）。它画在工具条**右端**；
   * 「一个目录都没有」的空态另在说明块里给一个常规尺寸的同一入口。它与每张卡头上的「⊕ 新建」
   * 是两个可区分的控件——后者的目录已经定了（`onCreateSession(directoryId)`），前者的目录还没定。
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
  /** 页码提交（输入框原文，回车/失焦时调用）；缺省不呈现翻页控件。 */
  readonly onPageChange?: ((requested: string) => void) | undefined;
}

/** 按调色板建样式表：浅/深各建一次（模块加载时），渲染路径上不再新建样式对象。 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    page: {
      width: "100%",
      flexGrow: 1,
      flexShrink: 1,
      minHeight: 0,
    },
    // 工具条固定，目录块在自己的滚动区里上下滑动（原型 `.scroll{flex:1;overflow:auto}`）。
    scroller: {
      flexGrow: 1,
      flexShrink: 1,
      minHeight: 0,
    },
    scrollBody: {
      flexGrow: 1,
      width: "100%",
    },
    content: {
      width: "100%",
      // 原型 `.wrap` 的 max-width 1180 是内容宽度（Chrome 下该页实际按 content-box 计算），
      // 加上两侧 24px 内边距后外框是 1228；RNW 按 border-box 计算，因此这里写 1228。
      maxWidth: 1228,
      alignSelf: "center",
      padding: spacing.xl,
      gap: spacing.md,
    },
    pagerRow: {
      flexDirection: "row",
      justifyContent: "flex-end",
      paddingTop: spacing.md,
      borderTopWidth: 1,
      borderTopColor: colors.line,
    },
    // 原型 `.toolbar`：一条横贯内容区的工具条，底部与列表分隔。
    toolbar: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      gap: spacing.lg,
      paddingVertical: spacing.md,
      paddingHorizontal: spacing.xl,
      borderBottomWidth: 1,
      borderBottomColor: colors.line,
    },
    // 顶部创建入口：原型 `.btn.sm.primary`（实心主色底、白字）。它在工具条里由 `toolbarSpacer`
    // 推到右端；同一个元素画在空态块里时（原型 `nodirs`）不需要那个空白，靠空态块自行居中。
    createEntry: {
      borderWidth: 1,
      borderColor: colors.accent,
      flexShrink: 0,
      paddingVertical: 5,
      paddingHorizontal: 11,
      borderRadius: 9,
      backgroundColor: colors.accent,
    },
    createEntryText: {
      // 原型的 `.btn.primary` 用纯白：主题里没有 white token，与 `PairingPanel` 同一取值。
      color: "#ffffff",
      fontFamily: fontFamily.sans,
      fontSize: 12.5,
      lineHeight: 20,
      fontWeight: "400",
    },
    // 空态块里的创建入口用原型 `btn primary` 的常规尺寸（不是工具条里的 `btn sm`）。
    createEntryLarge: {
      borderWidth: 1,
      borderColor: colors.accent,
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.lg,
      borderRadius: radius.sm,
      backgroundColor: colors.accent,
    },
    createEntryTextLarge: {
      color: "#ffffff",
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
      lineHeight: 22.4,
      fontWeight: fontWeight.medium,
    },
    // 原型工具条里的 `<span class="spacer">`：把创建入口推到最右端。
    toolbarSpacer: {
      flexGrow: 1,
    },
    // 原型 `.search`：图标 + 输入框同在一个带边框的容器里，最大宽度 420。
    searchWrap: {
      flexGrow: 1,
      flexShrink: 1,
      minWidth: 200,
      maxWidth: 420,
      flexDirection: "row",
      alignItems: "center",
      gap: spacing.sm,
      paddingVertical: 7,
      paddingHorizontal: spacing.md,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel,
    },
    searchIcon: {
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
      lineHeight: 22.4,
    },
    searchInput: {
      flexGrow: 1,
      flexShrink: 1,
      minWidth: 0,
      padding: 0,
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
      lineHeight: 22.4,
      backgroundColor: "transparent",
    },
    // 目录卡片始终单列堆叠（原型 `.stack`），不随宽度改成并排。
    list: {
      flexDirection: "column",
      gap: 18,
    },
    sessions: {
      flexDirection: "column",
    },
    // 卡体里的「这个目录还没有会话」一行：与卡片同一处缩进（缩进本身在卡片的卡体上）。
    emptyLine: {
      paddingVertical: 14,
      paddingHorizontal: 18,
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      lineHeight: 20.8,
    },
    // 卡体底部的「查看全部 ›」（原型 `.footline`）：与卡片头那个入口同一动作，单独可定位。
    footline: {
      paddingVertical: 10,
      paddingHorizontal: 18,
      borderTopWidth: 1,
      borderTopColor: colors.line2,
    },
    footLink: {
      alignSelf: "flex-start",
      paddingVertical: spacing.xs,
      paddingHorizontal: 6,
      borderRadius: radius.sm,
    },
    footLinkText: {
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      lineHeight: 20.8,
    },
    // 空态与「没有匹配的结果」都是原型 `.blank` 那一块：虚线边框、居中。
    blank: {
      alignItems: "center",
      backgroundColor: colors.panel,
      borderWidth: 1,
      borderStyle: "dashed",
      borderColor: colors.line,
      borderRadius: radius.md,
      // 原型 `.blank{padding:56px 24px}`：与 `EmptyState` 的空态块同一体量。
      paddingVertical: 56,
      paddingHorizontal: spacing.xl,
      gap: spacing.sm,
    },
    blankTitle: {
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.lg,
      lineHeight: 27.2,
      fontWeight: "600",
    },
    blankText: {
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      lineHeight: 20.8,
      textAlign: "center",
    },
    loading: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
      lineHeight: 22.4,
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
    onPageChange,
  } = props;
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  /**
   * 折叠是**视图内部**的临时状态：它不需要跨页存活（查询与筛选需要，因此那两个值住在
   * store 里），也不改变任何数据判定——所以留在这里，而不是又开一个 store 字段。
   */
  const [collapsedDirectoryIds, setCollapsedDirectoryIds] = useState<readonly string[]>([]);

  /**
   * **顶部创建入口**（页面给了回调才画）。
   *
   * 它通向「逐层浏览主机目录」这条路：`nodirs` 时画在空态块里，其余分支画在工具条右端；
   * 加载中与「目录存在但无会话」时仍然出现——那时它是页面上唯一的创建路径。
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
  // 原型 `nodirs` 的 `newDirButton("btn primary")`：常规尺寸，画在空态说明块里。
  const createEntryLarge =
    onCreateSessionToBrowse === undefined ? null : (
      <Pressable
        role="button"
        style={styles.createEntryLarge}
        dataSet={{ action: "create-session-browse" }}
        onPress={onCreateSessionToBrowse}
      >
        <Text style={styles.createEntryTextLarge}>⊕ 新建会话</Text>
      </Pressable>
    );

  if (model.kind === "loading") {
    return (
      <View style={styles.page} dataSet={{ route: "dirs" }}>
        {/* 还没有内容可搜索/可筛选，因此只有那一行工具条与它的入口（见 `createEntry`）。 */}
        {createEntry === null ? null : (
          <View style={styles.toolbar} dataSet={{ directoryToolbar: "true" }}>
            <View style={styles.toolbarSpacer} />
            {createEntry}
          </View>
        )}
        <View dataSet={{ directoryState: "loading" }} />
      </View>
    );
  }


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
    const allRows = sessionsByDirectoryId?.get(directory.directoryId) ?? [];
    const visibleRows = filterSessionsForDirectory({
      rows: allRows,
      displayName: directory.displayName,
      query,
      filter,
    });
    const inline = visibleRows.slice(0, MAX_INLINE_SESSIONS);
    const hasMore = visibleRows.length > MAX_INLINE_SESSIONS;
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
          <>
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
            {/* 超过内联上限时，卡体底部再给一次入口（原型 `.footline`）。动作与卡片头那个
                「查看全部」相同，但用独立取值，避免消费方把两处入口数成同一个控件。 */}
            {hasMore ? (
              <View style={styles.footline}>
                <Pressable
                  role="button"
                  style={styles.footLink}
                  dataSet={{ action: "open-directory-all-footer" }}
                  onPress={() => {
                    onOpenDirectory(directory.directoryId);
                  }}
                >
                  <Text style={styles.footLinkText}>查看全部 ›</Text>
                </Pressable>
              </View>
            ) : null}
          </>
        ) : allRows.length === 0 ? (
          <Text style={styles.emptyLine}>这个目录还没有会话</Text>
        ) : (
          // 有会话、但都被当前查询/筛选挡住（原型 `emptyline`「没有符合条件的会话」）。
          <Text style={styles.emptyLine}>没有符合条件的会话</Text>
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
    <View role="list" style={styles.list} dataSet={{ directoryList: "directories" }}>
      {directories.map(renderDirectoryCard)}
    </View>
  );

  const isEmpty = model.kind === "empty";
  const isNoDirectories = isEmpty && model.empty === "no_directories";
  // 目录页分页：每页 5 个目录（feature 层切片），翻页控件固定在列表底部。
  // 没有任何目录（no_directories）时没有可翻的内容，不画翻页。
  const pageInfo = model.kind === "ready" || model.kind === "empty" ? model : null;
  const pager =
    onPageChange === undefined || pageInfo === null || pageInfo.total === 0 ? null : (
      <View style={styles.pagerRow} dataSet={{ directoryPagerRow: "true" }}>
        <Pager
          page={pageInfo.page}
          pageCount={pageInfo.pageCount}
          onPageChange={onPageChange}
          data={{
            pager: "directoryListPager",
            page: "directoryListPage",
            pageCount: "directoryListPageCount",
            input: "directoryListPageInput",
            inputValue: "directoryListPageValue",
          }}
        />
      </View>
    );
  // 工具条在**所有**状态都保留搜索、筛选与右上角创建入口：空态下它们同样可用，
  // 用户不必先滚到空态块里的按钮才能开始创建。空态块里另有一个常规尺寸的同一入口。
  const toolbarCreate = createEntry;
  const searchField =
    onQueryChange === undefined ? null : (
      <View style={styles.searchWrap} dataSet={{ directorySearchWrap: "true" }}>
        <Text style={styles.searchIcon}>🔍</Text>
        <TextInput
          value={query}
          onChangeText={onQueryChange}
          placeholder="搜索目录或会话"
          placeholderTextColor={theme.colors.ink3}
          accessibilityLabel="搜索目录"
          style={styles.searchInput}
          // `data-directory-search` 是 e2e 定位输入框的锚点；当前值另存一份，便于断言
          // 「返回目录页时查询词仍在」（受控控件的 value 在 DOM 里只有属性值可读）。
          dataSet={{ directorySearch: "true", directorySearchValue: query }}
        />
      </View>
    );

  return (
    <View style={styles.page} dataSet={{ route: "dirs" }}>
      {/* 工具条：搜索、筛选与右上角创建入口在所有状态下保持可用。 */}
      {searchField === null && onFilterChange === undefined && toolbarCreate === null ? null : (
        <View style={styles.toolbar} dataSet={{ directoryToolbar: "true" }}>
          {searchField}
          {onFilterChange === undefined ? null : (
            <SegmentControl
              label="筛选"
              value={filter}
              options={FILTER_OPTIONS}
              onChange={onFilterChange}
            />
          )}
          {toolbarCreate === null ? null : <View style={styles.toolbarSpacer} />}
          {toolbarCreate}
        </View>
      )}

      <ScrollView
        style={styles.scroller}
        contentContainerStyle={styles.scrollBody}
        dataSet={{ directoryScroller: "true" }}
      >
        <View style={styles.content} dataSet={{ directoryContent: "true" }}>

        {model.kind === "empty" ? (
          <>
            {/* `data-empty-state` 仍只在这一支出现；其下**照常**列出已登记的目录项：
                指引写着「在下面选一个目录进入详情页」，列表就必须真的存在。`no_directories`
                时 `directories` 是空数组，因此不呈现列表，也就没有任何目录项；创建入口按原型
                画在空态块内。 */}
            <EmptyState
              kind={model.empty}
              title={DIRECTORY_EMPTY_COPY[model.empty].title}
              guidance={DIRECTORY_EMPTY_COPY[model.empty].guidance}
              action={isNoDirectories ? createEntryLarge : null}
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
        {pager}
        </View>
      </ScrollView>
    </View>
  );
}
