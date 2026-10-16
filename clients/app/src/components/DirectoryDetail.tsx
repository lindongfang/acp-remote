/**
 * 目录详情页（R15）。
 *
 * 详情页呈现「这个目录里的会话」（按时间倒序平铺、每页固定条数）与「在这个目录里新建会话」
 * 的入口（设备不再有创建权限维度，入口恒可用）。页面头部是「返回 + 标题 + 会话数芯片 +
 * 路径」（原型 `viewDirDetail` 的 `.pagehead`），底栏左侧是「⊕ 新建」与副作用提示、右侧是
 * 翻页（`design.md` D4）。
 *
 * ## 两种「空」互不冒充
 *
 * - **别名不在快照里** → `data-directory-detail-state="unknown_directory"`，文案说的是链接失效；
 * - **目录在，但一条会话都没有** → `data-directory-sessions="empty"`，文案说的是「还没有会话」。
 *
 * 详情页没有查询词与月份档（规格用「目录详情按时间倒序分页」替换了旧的月份分档与目录内搜索），
 * 因此**不存在**第三种「数据在、只是被筛掉了」的空态：列表为空只可能是「该目录一条会话都没有」
 * ——这里也就不再有 `data-directory-detail-no-match` 这一支。
 *
 * ## 判定全在 feature 层（D4）
 *
 * 组件只渲染 `DirectoryDetailModel`：每页条数、总页数、当前页切片、纠正后的页码都由
 * `src/features/directory-model.ts` 的纯函数算出，组件里没有业务分支。页码住在 store 里
 * （`state.directoryDetailPage`），组件把输入框里的**原文**上抛给页面，再由页面交给
 * `setDirectoryDetailPage` 纠正——所以「非法输入就地纠正」是 feature 层的可断言行为，而不是
 * 组件里的回填：输入框回填的就是模型给出的生效页码。
 *
 * ## 页码只在这两种时机提交（原型 `commitPage`）
 *
 * 输入框是**草稿 + 提交**两段式：`onChangeText` 只把原文写进本地草稿（`pageDraft`），回车
 * （`onSubmitEditing`）与失焦（`onBlur`）才上抛给页面（原型把 `commitPage` 挂在 `onkeydown`
 * 与 `onchange` 上，`accessibilityLabel` 也写着「回车跳转」）。于是「输入过程中列表不重画、
 * 输入框能被清空重输」与「提交后非法页码就地纠正」同时成立——若每个 `input` 事件都上抛，
 * 纠正值会被立刻写回 DOM，输入框就再也清不空（第 5 页退格得 `""` → 被纠正回 `5`）。
 *
 * 草稿与模型的关系只有一条：`null` = 跟随模型，非 `null` = 用户正在编辑的原文。写入草稿的
 * 只有 `onChangeText`，清空它的只有提交——于是模型在提交之后的任何纠正（回落当前页、夹到
 * 边界）都会立刻回填到输入框，而用户正在输入的原文不会被任何一次重渲染冲掉。
 *
 * ## 路径只来自服务端下发（`DirectoryRowModel.path`）
 *
 * 路径逐字取自快照条目上的 `path` 键，未下发时为 `null`——此时**不呈现**路径行，也 MUST NOT
 * 用别名或展示名拼一段看起来像路径的文本。组件不做任何字符串加工：超宽时交给
 * `numberOfLines`（CSS 省略号）处理，DOM 里的文本仍是那一段逐字路径。
 *
 * ## 响应式
 *
 * RNW 没有媒体查询，`useWindowDimensions()` 是唯一的宽度来源，断点取 `breakpoints.wide` 这个
 * token（`design.md` D9：三处硬编码的 `720` 收口到一处）。**窄屏与宽屏只差样式**：页头、会话行、
 * 路径、创建入口与翻页在两档下都渲染，`data-*` 集合完全相同（这是可被渲染测试与浏览器用例
 * 断言的「不因宽度少显示」）。`availableWidth` 只是把宽度变成可注入的入参，让
 * `renderToStaticMarkup`（Node 下 `Dimensions.get('window')` 恒为 0）能渲染出宽屏那一档。
 */

import type { ReactElement } from "react";
import { useEffect, useRef, useState } from "react";

import type { DirectoryDetailModel } from "../features/directory-model";
import { CONTENT_ORIGIN_LABELS } from "../features/directory-model";
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

export interface DirectoryDetailProps {
  readonly model: DirectoryDetailModel;
  /** 返回目录页（页面负责导航）；页头第一行恒呈现这个入口。 */
  readonly onBack: () => void;
  readonly onOpenSession: (sessionId: string) => void;
  readonly onCreateSession: () => void;
  /** 页码提交（输入框里的**原文**，只在回车与失焦时调用）：纠正全在 feature 层，组件只上抛。 */
  readonly onPageChange: (requested: string) => void;
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

/**
 * 翻页后把内容滚回该页顶部（规格「翻页后回到页首」）。
 *
 * 怎么找滚动容器：外壳（`app/_layout.tsx`）可能把滚动放在自己的容器里（原型就是
 * `.scroll{flex:1;overflow:auto}`），因此不能假定滚的是文档，也不能把滚动位置当成页面状态。
 * 做法是从**本页自己的根节点**（`data-route="dir-detail"`，本组件每一支都渲染它）往上找最近
 * 一个真正可滚动的祖先（`overflow-y: auto|scroll` 且内容比它高）并把它的 `scrollTop` 归零；
 * 往上走不到可滚动祖先时（整页滚动）落到 `document.scrollingElement`。
 *
 * 这是纯呈现行为：没有 DOM（原生平台、`renderToStaticMarkup`）时什么都不做，
 * 失败不该把一个渲染中的页面打崩。
 */
function scrollContentToTop(): void {
  if (typeof document === "undefined" || typeof window === "undefined") return;
  const root = document.querySelector('[data-route="dir-detail"]');
  for (let element = root?.parentElement ?? null; element !== null; element = element.parentElement) {
    const { overflowY } = window.getComputedStyle(element);
    if ((overflowY === "auto" || overflowY === "scroll") && element.scrollHeight > element.clientHeight) {
      element.scrollTop = 0;
      return;
    }
  }
  const scroller = document.scrollingElement;
  if (scroller !== null) scroller.scrollTop = 0;
}

/** 按调色板建样式表：浅/深各建一次（模块加载时），渲染路径上不再新建样式对象。 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    root: {
      gap: spacing.md,
    },
    head: {
      flexDirection: "column",
      alignItems: "flex-start",
      gap: spacing.xs,
    },
    back: {
      alignSelf: "flex-start",
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.sm,
      borderRadius: radius.sm,
    },
    backText: {
      color: colors.accent,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    titleRow: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      gap: spacing.sm,
    },
    title: {
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xl,
      fontWeight: "600",
    },
    chip: {
      paddingVertical: 2,
      paddingHorizontal: spacing.sm,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.lg,
      backgroundColor: colors.panel,
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    pathRow: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "baseline",
      gap: spacing.sm,
      minWidth: 0,
    },
    pathLabel: {
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    pathValue: {
      flexShrink: 1,
      minWidth: 0,
      color: colors.ink,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.sm,
      // 等宽值行取 500（原型 `.drow .dpath` 的 `font-weight:500`，见 `theme.ts` 的 `fontWeight.value`）。
      fontWeight: fontWeight.value,
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
    footbarWide: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      justifyContent: "space-between",
      gap: spacing.md,
      paddingTop: spacing.md,
      borderTopWidth: 1,
      borderTopColor: colors.line,
    },
    footbarNarrow: {
      flexDirection: "column",
      alignItems: "stretch",
      gap: spacing.sm,
      paddingTop: spacing.md,
      borderTopWidth: 1,
      borderTopColor: colors.line,
    },
    createBlock: {
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
    createText: {
      color: colors.accentInk,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
      fontWeight: "600",
    },
    hint: {
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    pager: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      gap: spacing.sm,
    },
    pagerButton: {
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.sm,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel,
    },
    pagerButtonDisabled: {
      opacity: 0.55,
    },
    pagerButtonText: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    pagerButtonTextDisabled: {
      color: colors.ink3,
    },
    pagerInfo: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      gap: spacing.xs,
    },
    pagerInfoText: {
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    pageInput: {
      width: 40,
      minHeight: 28,
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.xs,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      textAlign: "center",
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
  onBack,
  onOpenSession,
  onCreateSession,
  onPageChange,
  lastSyncedAtMs = null,
  availableWidth,
}: DirectoryDetailProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const windowDimensions = useWindowDimensions();
  const width = availableWidth ?? windowDimensions.width;
  // 只影响布局：两档渲染的信息集合必须相同（MUST NOT 因宽度不足而隐藏任何一项）。
  const wide = width >= breakpoints.wide;

  const page = model.kind === "ready" ? model.page : null;
  const lastPage = useRef<number | null>(null);
  // 页码草稿：`null` = 跟随模型（显示生效页码），非 `null` = 用户正在输入的原文。
  // 它是本页唯一的本地状态，且**不**参与渲染判定：列表切片只由 `model` 决定。
  const [pageDraft, setPageDraft] = useState<string | null>(null);

  useEffect(() => {
    // 只在**真的换了页**时回页首：初次渲染（含从加载态进入）不动滚动位置。
    if (page === null) return;
    const previous = lastPage.current;
    lastPage.current = page;
    if (previous === null || previous === page) return;
    scrollContentToTop();
  }, [page]);

  // 返回入口在每一支都呈现：自建导航被共享外壳取代后，它是离开本页的唯一入口（D1）。
  const back = (
    <Pressable
      role="button"
      style={styles.back}
      dataSet={{ action: "back-to-dirs" }}
      onPress={onBack}
    >
      <Text style={styles.backText}>‹ 目录</Text>
    </Pressable>
  );

  if (model.kind === "loading") {
    return (
      <View style={styles.root} dataSet={{ route: "dir-detail" }}>
        {back}
        <Text style={styles.waiting} dataSet={{ directoryDetailState: "loading" }}>
          尚未收到第一份快照，正在等待同步……
        </Text>
      </View>
    );
  }

  if (model.kind === "unknown_directory") {
    return (
      <View style={styles.root} dataSet={{ route: "dir-detail", directoryAlias: model.alias }}>
        {back}
        {/*
          文案里**不出现别名**：别名只是路由与命令引用的内部标识，界面上不得露出来
          （同名目录靠服务端下发的路径尾段消歧）。这里也不改用展示名——别名不在快照里时，
          展示名根本无从取得，`undefined` 会渲染出一个空的引号。
        */}
        <Text role="alert" style={styles.alert} dataSet={{ directoryDetailState: "unknown_directory" }}>
          这个目录不在最近一次同步的目录列表里。它可能已被删除，或这个链接来自更早的一次同步。
        </Text>
      </View>
    );
  }

  const directory = model.directory;
  const syncedAtMs = model.contentOrigin === "last_sync" ? lastSyncedAtMs : null;

  /**
   * 提交页码：回车与失焦都走这里。
   *
   * 先清草稿再上抛——清草稿等于「把编辑权交回模型」，输入框随即显示模型给出的生效页码；
   * 页面把 `requested` 纠正后写进 store，模型一变就是最终的纠正结果。
   *
   * 没有草稿（`null`）时不上抛：那说明这一轮没有编辑（典型的是回车提交后 RNW 紧接着触发的
   * 那次失焦），写一次仓库只会白白通知一轮。
   */
  const commitPage = (): void => {
    const requested = pageDraft;
    setPageDraft(null);
    if (requested === null) return;
    onPageChange(requested);
  };

  // 页头：返回 / 标题 + 会话数芯片 / 路径。别名不呈现（它只是路由与命令引用的内部标识）。
  const head = (
    <View style={styles.head}>
      {back}
      <View style={styles.titleRow}>
        <Text style={styles.title} dataSet={{ directoryDisplayName: directory.displayName }}>
          {directory.displayName}
        </Text>
        <Text style={styles.chip} dataSet={{ directorySessionCount: String(model.total) }}>
          {`${String(model.total)} 个会话`}
        </Text>
      </View>
      {/* 路径逐字取自快照下发的引用；没下发（未协商或键缺席）时不呈现这一行，也不用别名顶替。 */}
      {directory.path === null ? null : (
        <View style={styles.pathRow} dataSet={{ directoryPath: directory.path }}>
          <Text style={styles.pathLabel}>路径</Text>
          <Text style={styles.pathValue} numberOfLines={1}>
            {directory.path}
          </Text>
        </View>
      )}
    </View>
  );

  return (
    <View style={styles.root} dataSet={{ route: "dir-detail", directoryAlias: directory.alias }}>
      {head}

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
      ) : (
        <View style={styles.emptyBox}>
          <Text style={styles.emptyTitle} dataSet={{ directorySessions: "empty" }}>
            这个目录里还没有会话。
          </Text>
          <Text style={styles.emptyGuidance}>用下面的入口在这个目录里创建一个会话。</Text>
        </View>
      )}

      <View style={wide ? styles.footbarWide : styles.footbarNarrow}>
        <View style={styles.createBlock}>
          <Pressable
            role="button"
            style={styles.createButton}
            dataSet={{ action: "open-create-sheet" }}
            onPress={onCreateSession}
          >
            <Text style={styles.createText}>⊕ 新建</Text>
          </Pressable>
          {/* 副作用提示：入口恒可用（设备不再有创建权限维度），所以说明也始终在。 */}
          <Text style={styles.hint} dataSet={{ createSideEffect: "true" }}>
            将在你的电脑上启动一个 Agent 进程
          </Text>
        </View>
        {/* 只有一页时不呈现翻页控件（规格：`pageCount === 1` 即无上/下一页与页码输入）。 */}
        {model.pageCount > 1 ? (
          <View
            style={styles.pager}
            dataSet={{
              directoryDetailPager: "true",
              directoryPage: String(model.page),
              directoryPageCount: String(model.pageCount),
            }}
          >
            <Pressable
              role="button"
              style={model.page > 1 ? styles.pagerButton : [styles.pagerButton, styles.pagerButtonDisabled]}
              disabled={model.page <= 1}
              dataSet={{ action: "page-prev" }}
              onPress={() => {
                onPageChange(String(model.page - 1));
              }}
            >
              <Text style={model.page > 1 ? styles.pagerButtonText : [styles.pagerButtonText, styles.pagerButtonTextDisabled]}>
                ‹ 上一页
              </Text>
            </Pressable>
            <View style={styles.pagerInfo}>
              <Text style={styles.pagerInfoText}>第</Text>
              <TextInput
                style={styles.pageInput}
                value={pageDraft ?? String(model.page)}
                accessibilityLabel="页码（回车跳转）"
                // 输入只改草稿：输入期间不写仓库，所以列表不重画；纠正发生在提交之后。
                onChangeText={(text) => {
                  setPageDraft(text);
                }}
                onSubmitEditing={commitPage}
                onBlur={commitPage}
                dataSet={{ directoryDetailPageInput: "true", directoryDetailPageValue: String(model.page) }}
              />
              <Text style={styles.pagerInfoText}>{`/ ${String(model.pageCount)} 页`}</Text>
            </View>
            <Pressable
              role="button"
              style={
                model.page < model.pageCount
                  ? styles.pagerButton
                  : [styles.pagerButton, styles.pagerButtonDisabled]
              }
              disabled={model.page >= model.pageCount}
              dataSet={{ action: "page-next" }}
              onPress={() => {
                onPageChange(String(model.page + 1));
              }}
            >
              <Text
                style={
                  model.page < model.pageCount
                    ? styles.pagerButtonText
                    : [styles.pagerButtonText, styles.pagerButtonTextDisabled]
                }
              >
                下一页 ›
              </Text>
            </Pressable>
          </View>
        ) : null}
      </View>
    </View>
  );
}
