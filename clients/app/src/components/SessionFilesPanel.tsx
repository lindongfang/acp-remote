/**
 * 「文件」右栏（spec「文件栏提供三种来源」（标题沿用旧增量，正文为两来源）「全部文件逐层浏览与过滤」
 * 「Git 变更的展示与不可用原因」）。
 *
 * ## 两个来源各一条渲染路径，互不冒充
 *
 * 标题固定为「文件」，下面只有两个选项：全部文件 / Git 变更。来源的选中值、两份数据与不可用原因
 * 都由 `SessionFilesPanelModel` 给出（`features/session-files.ts` 是唯一组装点），本组件不判定
 * 「该不该降级」：
 *
 * - **全部文件**：逐层展开目录。根层始终加载；子目录第一次展开时由 {@link DirectoryLevel} 触发
 *   `onLoadMoreAllFiles(path)`。忽略开关默认关闭（即显示被忽略项），`.git` 条目在这里**再过滤
 *   一次**——服务端已经隐藏，但「`.git` 永不出现」不能只依赖上游一道防线。
 * - **Git 变更**：staged/unstaged/untracked 都只是 `status` 的不同取值，统一按文件列出；非仓库、
 *   Git 不可用、冲突、二进制与截断都各自有显式说明，不用空列表冒充干净仓库。范围是整个会话
 *   工作目录的未提交修改，界面明确说明它可能包含人工与其他会话的改动。
 *
 * 会话改动只在对话时间线呈现（结构化差异 + 诊断入口），文件栏不提供第三种来源、记录选择器或
 * 专属刷新。
 *
 * ## 展开状态住在组件里，滚动位置由父级滚动容器保留
 *
 * 展开的目录是本组件的局部 UI 状态（不改变任何数据判定）；窄屏打开文件时右栏仍在 DOM 里
 * （只是不占主体），因此关闭后展开目录与滚动位置都不丢。组件不拥有草稿、时间线锚点或条目
 * 展开状态。
 */

import { useEffect, useState } from "react";
import type { ReactElement } from "react";

import type { FileSource, SessionFilesPanelModel } from "../features/session-files";
import { FILE_SOURCES, FILE_SOURCE_LABELS } from "../features/session-files";
import type { GitStatusEntry } from "../protocol/session-files";
import type { Palette } from "./theme";
import { Pressable, StyleSheet, Text, View, fontFamily, fontSize, palettes, radius, spacing, useTheme } from "./theme";

export interface SessionFilesPanelProps {
  readonly model: SessionFilesPanelModel;
  /** 打开文件内容（来源 + 根内相对路径）。缺省时条目不可打开。 */
  readonly onOpenFile?: ((source: FileSource, path: string) => void) | undefined;
  /** 切换来源；缺省时选项不可点（不是点了没反应的按钮）。 */
  readonly onSelectSource?: ((source: FileSource) => void) | undefined;
  /** 手动刷新当前来源。 */
  readonly onRefresh?: (() => void) | undefined;
  /** 切换「隐藏忽略文件」。 */
  readonly onToggleHideIgnored?: (() => void) | undefined;
  /** 加载某一层目录的下一页（首次展开也走它）。 */
  readonly onLoadMoreAllFiles?: ((path: string) => void) | undefined;
  /** 继续加载 Git 变更的下一页。 */
  readonly onLoadMoreGitChanges?: (() => void) | undefined;
}

/** 按调色板建样式表：浅/深各建一次（模块加载时）。 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    panel: {
      gap: spacing.sm,
      paddingVertical: spacing.md,
      paddingHorizontal: spacing.sm,
      backgroundColor: colors.panel2,
    },
    head: {
      flexDirection: "row",
      alignItems: "baseline",
      gap: spacing.sm,
      paddingBottom: spacing.sm,
      borderBottomWidth: 1,
      borderBottomColor: colors.line,
    },
    title: {
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
      fontWeight: "600",
    },
    headSpacer: {
      flexGrow: 1,
    },
    sources: {
      flexDirection: "row",
      borderBottomWidth: 1,
      borderBottomColor: colors.line,
    },
    sourceOption: {
      flex: 1,
      alignItems: "center",
      paddingVertical: spacing.sm,
      borderBottomWidth: 2,
      borderBottomColor: "transparent",
    },
    sourceOptionSelected: {
      borderBottomColor: colors.accent,
    },
    sourceText: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    sourceTextSelected: {
      color: colors.ink,
      fontWeight: "600",
    },
    action: {
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.sm,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel,
    },
    actionText: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    actionDisabled: {
      opacity: 0.45,
    },
    note: {
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    notice: {
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.sm,
      borderRadius: radius.sm,
      backgroundColor: colors.line2,
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    error: {
      color: colors.danger,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    list: {
      gap: 0,
      minWidth: 0,
    },
    treeChildren: {
      marginLeft: 12,
      paddingLeft: 12,
      borderLeftWidth: 1,
      borderLeftColor: colors.line,
    },
    positiveName: { color: colors.ok },
    negativeName: { color: colors.danger },
    directory: {
      gap: 0,
    },
    directoryRow: {
      flexDirection: "row",
      alignItems: "center",
      gap: 6,
      minHeight: 32,
      paddingHorizontal: 6,
    },
    entry: {
      flexDirection: "row",
      alignItems: "center",
      gap: 6,
      minHeight: 32,
      paddingHorizontal: 6,
      paddingLeft: 22,
    },
    entryRestricted: {
      opacity: 0.7,
      flexWrap: "wrap",
    },
    entryName: {
      flexShrink: 1,
      color: colors.ink,
      fontFamily: fontFamily.sans,
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
    chipWarn: {
      paddingHorizontal: spacing.sm,
      paddingVertical: 2,
      borderRadius: radius.sm,
      color: colors.danger,
      backgroundColor: colors.dangerSoft,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
    },
    changedFileSelected: {
      backgroundColor: colors.accentSoft,
    },
    selectablePath: {
      flex: 1,
      minWidth: 0,
      justifyContent: "center",
      minHeight: 32,
    },
    path: {
      minWidth: 0,
      flexShrink: 1,
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    hidden: {
      display: "none",
    },
    rowMeta: {
      flexDirection: "row",
      alignItems: "center",
      gap: 6,
      flexShrink: 0,
    },
    marker: {
      width: 12,
      textAlign: "center",
      fontFamily: fontFamily.mono,
      fontSize: fontSize.sm,
    },
    chevron: {
      width: 10,
      fontSize: fontSize.xs,
      color: colors.ink3,
    },
    entryButton: {
      alignSelf: "flex-start",
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.md,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
    },
    entryButtonText: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

/** Git 状态 → 文案（staged/unstaged 只是同一文件未提交状态的两面，统一按文件列出）。 */
const GIT_STATUS_LABELS: Readonly<Record<GitStatusEntry["status"], string>> = {
  added: "新增",
  modified: "修改",
  deleted: "删除",
  renamed: "重命名",
  untracked: "未跟踪",
};

/** 根外/不可读/不支持的显式说明；`null` = 可打开。 */
function restrictedNote(reason: string | undefined): string {
  switch (reason) {
    case "outside_root":
      return "根外链接：条目可见，但不能打开、预览或下载目标。";
    case "unreadable":
      return "这个条目无法读取。";
    case "unsupported":
      return "这个条目类型不支持打开。";
    default:
      return "这个条目不能打开。";
  }
}

/** `.git` 目录与 worktree 的 `.git` 管理文件在任何一页都不呈现。 */
function isGitAdminEntry(name: string): boolean {
  return name.toLowerCase() === ".git";
}

/** 一层目录（根为 `""`）。展开与首次加载都在这一层完成。 */
function DirectoryLevel({
  path,
  depth,
  model,
  expanded,
  onToggleDirectory,
  onOpenFile,
  onLoadMore,
}: {
  readonly path: string;
  readonly depth: number;
  readonly model: SessionFilesPanelModel;
  readonly expanded: ReadonlySet<string>;
  readonly onToggleDirectory: (path: string) => void;
  readonly onOpenFile: ((source: FileSource, path: string) => void) | undefined;
  readonly onLoadMore: ((path: string) => void) | undefined;
}): ReactElement | null {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const directory = model.all.directories.get(path);
  const open = path === "" || expanded.has(path);

  // 展开的子目录第一次渲染时没有缓存：由这里触发第一页（组件渲染不产生副作用）。
  useEffect(() => {
    if (!open || directory !== undefined || onLoadMore === undefined) return;
    onLoadMore(path);
  }, [open, directory, onLoadMore, path]);

  if (!open) return null;

  const entries = (directory?.entries ?? []).filter((entry) => !isGitAdminEntry(entry.name))
    .sort((left, right) => Number(right.kind === "directory") - Number(left.kind === "directory") ||
      left.name.localeCompare(right.name, undefined, { numeric: true, sensitivity: "base" }));

  return (
    <View style={styles.directory} dataSet={{ fileDirectory: path === "" ? "root" : path, fileDepth: String(depth) }}>
      {directory === undefined ? (
        <Text style={styles.note} dataSet={{ fileDirectoryLoading: "true" }}>
          正在加载这一层…
        </Text>
      ) : null}
      {directory?.loading === true ? (
        <Text style={styles.note} dataSet={{ fileDirectoryLoading: "true" }}>
          正在加载这一层…
        </Text>
      ) : null}
      {directory?.error == null ? null : (
        <Text style={styles.error} dataSet={{ fileListError: directory.error.code }}>
          {`这一层加载失败：${directory.error.message}`}
        </Text>
      )}
      {entries.length === 0 && directory !== undefined && directory.loading === false && directory.error === null ? (
        <Text style={styles.note} dataSet={{ fileDirectoryEmpty: "true" }}>
          这一层没有可显示的条目。
        </Text>
      ) : null}
      {entries.map((entry) => {
        const rowKey = `all:${entry.path}`;
        if (entry.kind === "directory") {
          let target = entry;
          let label = entry.name;
          const chain = [entry.path];
          // 只压缩已经成功加载的单分支目录，不额外扫描磁盘或猜测尚未加载的结构。
          while (true) {
            const cached = model.all.directories.get(target.path);
            if (cached === undefined || cached.loading || cached.error !== null || cached.next !== null) break;
            const children = cached.entries.filter(child => !isGitAdminEntry(child.name));
            const child = children[0];
            if (children.length !== 1 || child?.kind !== "directory" || chain.includes(child.path)) break;
            target = child;
            label += "/" + child.name;
            chain.push(child.path);
          }
          const directoryPath = target.path;
          const isOpen = chain.some(path => expanded.has(path));
          const childExpanded = isOpen ? new Set([...expanded, directoryPath]) : expanded;
          return (
            <View key={entry.path} style={styles.list}>
              <Pressable
                role="button"
                style={styles.directoryRow}
                aria-expanded={isOpen}
                dataSet={{
                  fileRow: rowKey,
                  fileDirectoryRow: directoryPath,
                  fileDirectoryExpanded: String(isOpen),
                  action: "toggle-directory",
                }}
                onPress={() => {
                  if (isOpen) {
                    for (const path of chain) if (expanded.has(path)) onToggleDirectory(path);
                  } else onToggleDirectory(directoryPath);
                }}
              >
                <Text style={styles.chevron}>{isOpen ? "▾" : "▸"}</Text>
                <FileEntryIcon directory /><Text numberOfLines={1} style={styles.entryName}>{label}</Text>
              </Pressable>
              <View style={styles.treeChildren}><DirectoryLevel
                path={directoryPath}
                depth={depth + 1}
                model={model}
                expanded={childExpanded}
                onToggleDirectory={onToggleDirectory}
                onOpenFile={onOpenFile}
                onLoadMore={onLoadMore}
              /></View>
            </View>
          );
        }

        const canOpen = entry.openable && onOpenFile !== undefined;
        const note = entry.openable ? null : restrictedNote(entry.restricted);
        return (
          <View
            key={entry.path}
            tabIndex={0}
            style={[styles.entry, !entry.openable && styles.entryRestricted,
              model.content.kind === "file" && model.content.context.source === "all" &&
              model.content.context.path === entry.path && styles.changedFileSelected]}
            dataSet={{
              fileRow: rowKey,
              fileEntry: entry.path,
              fileEntryKind: entry.kind,
              fileEntryOpenable: String(entry.openable),
              fileEntryRestricted: entry.restricted ?? "none",
            }}
          >
            <FileEntryIcon path={entry.path} />
            {canOpen ? (
              <Pressable
                role="button"
                style={styles.selectablePath}
                dataSet={{ action: "open-file", fileSource: "all", filePath: entry.path }}
                onPress={() => {
                  onOpenFile("all", entry.path);
                }}
              >
                <Text numberOfLines={1} style={styles.entryName}>{entry.name}</Text>
              </Pressable>
            ) : (
              <Text numberOfLines={1} style={styles.entryName}>{entry.name}</Text>
            )}
            {entry.kind === "link" ? <Text style={styles.chip}>链接</Text> : null}
            {entry.kind === "other" ? <Text style={styles.chip}>特殊文件</Text> : null}
            {/* 根外链接始终可见：这里给出「为什么打不开」，不把条目静默藏掉。 */}
            {note === null ? null : (
              <Text style={[styles.note, { flexBasis: "100%", paddingLeft: 22 }]} dataSet={{ fileEntryRestrictedNote: entry.restricted ?? "blocked" }}>
                {note}
              </Text>
            )}
          </View>
        );
      })}
      {directory?.next == null || onLoadMore === undefined ? null : (
        <Pressable
          role="button"
          style={styles.entryButton}
          dataSet={{ action: "load-more-directory", directoryPath: path }}
          onPress={() => {
            onLoadMore(path);
          }}
        >
          <Text style={styles.entryButtonText}>加载更多（这一层）</Text>
        </Pressable>
      )}
    </View>
  );
}

/** Git 变更列表（状态、旧路径、冲突/二进制与分页）。 */
function GitChanges({
  model,
  onOpenFile,
  onLoadMore,
}: {
  readonly model: SessionFilesPanelModel;
  readonly onOpenFile: ((source: FileSource, path: string) => void) | undefined;
  readonly onLoadMore: (() => void) | undefined;
}): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const git = model.git;

  if (model.gitNotice !== null) {
    return (
      <Text style={styles.notice} dataSet={{ gitNotice: git.unsupported ?? "unsupported" }}>
        {model.gitNotice}
      </Text>
    );
  }
  if (git.gitState === "not_repository") {
    return (
      <Text style={styles.notice} dataSet={{ gitState: "not_repository" }}>
        此目录不是 Git 仓库。选项保留，但这里没有可展示的未提交改动。
      </Text>
    );
  }
  if (git.gitState === "unavailable") {
    return (
      <Text style={styles.notice} dataSet={{ gitState: "unavailable" }}>
        Git 不可用：未安装 Git，或本次查询失败。全部文件仍可浏览，但忽略过滤不可用。
      </Text>
    );
  }
  if (git.error !== null && git.entries.length === 0) {
    return (
      <Text style={styles.error} dataSet={{ gitError: git.error.code }}>
        {`Git 变更读取失败：${git.error.message}`}
      </Text>
    );
  }

  return (
    <View style={styles.list} dataSet={{ gitChanges: git.entries.length === 0 ? "empty" : "list", gitEntryCount: String(git.entries.length) }}>
      <Text style={styles.notice} dataSet={{ gitScopeNotice: "true" }}>
        范围是整个会话工作目录的未提交修改（含已暂存、未暂存与未跟踪文件），可能包含人工或其他会话的改动。
      </Text>
      {git.error === null ? null : (
        <Text style={styles.error} dataSet={{ gitError: git.error.code }}>
          {`刷新失败，下面保留上次成功的结果：${git.error.message}`}
        </Text>
      )}
      {git.loading && git.entries.length === 0 ? (
        <Text style={styles.note} dataSet={{ gitLoading: "true" }}>
          正在读取 Git 变更…
        </Text>
      ) : null}
      {git.gitState === "available" && git.entries.length === 0 && !git.loading ? (
        <Text style={styles.note} dataSet={{ gitEmpty: "true" }}>
          没有 Git 变更。干净仓库不是「不可用」。
        </Text>
      ) : null}
      <ChangeTree items={git.entries} pathOf={(entry) => entry.path} renderItem={(entry) => {
        // 删除文件也能打开：`session.git.diff` 以 HEAD 为基准返回删除行。
        const canOpen = onOpenFile !== undefined;
        const appearance = changeAppearance(entry.status);
        const nameStyle = appearance === "negative" ? styles.negativeName : appearance === "positive" ? styles.positiveName : {};
        const label = entry.path.split("/").pop() ?? entry.path;
        return (
          <View
            key={`${entry.status}:${entry.path}`}
            tabIndex={0}
            style={[styles.entry, model.content.kind === "file" && model.content.context.source === "git" &&
              model.content.context.path === entry.path && styles.changedFileSelected]}
            aria-label={`${entry.path}，${GIT_STATUS_LABELS[entry.status]}${entry.previousPath === undefined ? "" : `，原路径 ${entry.previousPath}`}`}
            dataSet={{
              fileRow: `git:${entry.path}`,
              gitEntry: entry.path,
              gitStatus: entry.status,
              gitConflicted: String(entry.conflicted),
              gitBinary: String(entry.binary),
            }}
          >
            <FileEntryIcon path={entry.path} />
            {canOpen ? (
              <Pressable
                role="button"
                style={styles.selectablePath}
                dataSet={{ action: "open-git-file", fileSource: "git", filePath: entry.path }}
                onPress={() => {
                  onOpenFile("git", entry.path);
                }}
              >
                <Text numberOfLines={1} style={[styles.path, nameStyle]}>{label}</Text>
              </Pressable>
            ) : (
              <Text numberOfLines={1} style={[styles.path, { flex: 1 }, nameStyle]}>{label}</Text>
            )}
            <View style={styles.rowMeta}>
            {appearance === "neutral" ? <Text style={styles.note}>{GIT_STATUS_LABELS[entry.status]}</Text> : <Text style={[styles.marker, nameStyle]} dataSet={{ fileChangeMarker: appearance }}>{appearance === "negative" ? "-" : "+"}</Text>}
            {entry.previousPath === undefined ? null : (
              <Text style={[styles.note, styles.hidden]} dataSet={{ gitPreviousPath: entry.previousPath }}>
                {`旧路径：${entry.previousPath}`}
              </Text>
            )}
            {entry.conflicted ? <Text style={styles.chipWarn}>冲突</Text> : null}
            {entry.binary ? <Text style={styles.chip}>二进制</Text> : null}
            {entry.status === "deleted" ? (
              <Text style={[styles.note, styles.hidden]} dataSet={{ gitDeletedNote: "true" }}>
                文件已删除：完整内容见相对 HEAD 的差异。
              </Text>
            ) : null}
            </View>
          </View>
        );
      }} />
      {git.next == null || onLoadMore === undefined ? null : (
        <Pressable
          role="button"
          style={styles.entryButton}
          dataSet={{ action: "load-more-git" }}
          onPress={onLoadMore}
        >
          <Text style={styles.entryButtonText}>加载更多 Git 变更</Text>
        </Pressable>
      )}
    </View>
  );
}

/** 「文件」右栏。 */
export function SessionFilesPanel({
  model,
  onOpenFile,
  onSelectSource,
  onRefresh,
  onToggleHideIgnored,
  onLoadMoreAllFiles,
  onLoadMoreGitChanges,
}: SessionFilesPanelProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(() => new Set());
  const root = model.all.directories.get("");
  const gitState = root?.gitState ?? null;
  const ignoreDisabledReason =
    model.allNotice !== null
      ? model.allNotice
      : gitState === "not_repository"
        ? "此目录不是 Git 仓库，无法应用忽略规则。"
        : gitState === "unavailable"
          ? "Git 不可用，忽略规则无法生效。"
          : null;

  // 进入「全部文件」且根层尚未加载：由这里触发一次（store 负责去重与在途合并）。
  useEffect(() => {
    if (model.selectedSource !== "all") return;
    if (model.allNotice !== null || !model.connectionOnline) return;
    if (root !== undefined || onLoadMoreAllFiles === undefined) return;
    onLoadMoreAllFiles("");
  }, [model.selectedSource, model.allNotice, model.connectionOnline, onLoadMoreAllFiles, root]);

  // 进入「Git 变更」且还没有任何成功结果：触发一次刷新（失败后 error 非空，不再自动重试）。
  useEffect(() => {
    if (model.selectedSource !== "git") return;
    if (model.gitNotice !== null || !model.connectionOnline) return;
    if (model.git.gitState !== null || model.git.loading || model.git.error !== null) return;
    onRefresh?.();
  }, [model.selectedSource, model.gitNotice, model.connectionOnline, model.git.gitState, model.git.loading, model.git.error, onRefresh]);

  return (
    <View style={styles.panel} dataSet={{ filesPanel: "true", fileSource: model.selectedSource }}>
      <View style={styles.head}>
        <Text style={styles.title} dataSet={{ filesPanelTitle: "true" }}>
          文件
        </Text>
        <View style={styles.headSpacer} />
        {onRefresh === undefined ? null : (
          <Pressable
            role="button"
            style={model.refreshState.kind !== "available" ? [styles.action, styles.actionDisabled] : styles.action}
            dataSet={{ action: "refresh-files", refreshDisabled: String(model.refreshState.kind !== "available") }}
            disabled={model.refreshState.kind !== "available"}
            onPress={onRefresh}
          >
            <Text style={styles.actionText}>{model.refreshState.kind === "loading" ? "刷新中…" : "刷新"}</Text>
          </Pressable>
        )}
      </View>

      {model.refreshState.kind === "unavailable" ? <Text style={styles.note} dataSet={{ fileRefreshUnavailable: "true" }}>{model.refreshState.reason}</Text> : null}
      <View style={styles.sources} dataSet={{ fileSources: "true" }}>
        {FILE_SOURCES.map((source) => {
          const selected = source === model.selectedSource;
          return (
            <Pressable
              key={source}
              role="button"
              style={selected ? [styles.sourceOption, styles.sourceOptionSelected] : styles.sourceOption}
              dataSet={{
                action: "select-file-source",
                fileSourceOption: source,
                fileSourceSelected: String(selected),
              }}
              disabled={onSelectSource === undefined}
              onPress={() => {
                onSelectSource?.(source);
              }}
            >
              <Text style={selected ? [styles.sourceText, styles.sourceTextSelected] : styles.sourceText}>
                {FILE_SOURCE_LABELS[source]}
              </Text>
            </Pressable>
          );
        })}
      </View>

      {model.connectionOnline ? null : (
        <Text style={styles.notice} dataSet={{ filesOffline: "true" }}>
          当前不在线：已加载的文件与 Git 结果可能不是最新的。
        </Text>
      )}

      {model.selectedSource === "all" ? (
        <View style={styles.list}>
          {model.allNotice === null ? null : (
            <Text style={styles.notice} dataSet={{ filesAllNotice: "true" }}>
              {model.allNotice}
            </Text>
          )}
          {onToggleHideIgnored === undefined ? null : (
            <Pressable
              role="button"
              style={
                ignoreDisabledReason === null ? styles.action : [styles.action, styles.actionDisabled]
              }
              dataSet={{
                action: "toggle-hide-ignored",
                hideIgnored: String(model.all.hideIgnored),
                ignoreToggleDisabled: String(ignoreDisabledReason !== null),
              }}
              disabled={ignoreDisabledReason !== null}
              onPress={onToggleHideIgnored}
            >
              <Text style={styles.actionText}>
                {model.all.hideIgnored ? "显示忽略文件" : "隐藏忽略文件"}
              </Text>
            </Pressable>
          )}
          {ignoreDisabledReason === null ? (
            <Text style={[styles.note, styles.hidden]} dataSet={{ ignoreToggleScope: "true" }}>
              默认显示隐藏与被 Git 忽略的文件；`.git` 管理条目始终隐藏。
            </Text>
          ) : (
            <Text style={styles.note} dataSet={{ ignoreToggleDisabledNote: "true" }}>
              {ignoreDisabledReason}
            </Text>
          )}
          {model.allNotice === null ? (
            <DirectoryLevel
              path=""
              depth={0}
              model={model}
              expanded={expanded}
              onToggleDirectory={(path) => {
              setExpanded((current) => {
                const next = new Set(current);
                if (next.has(path)) next.delete(path);
                else next.add(path);
                return next;
              });
            }}
              onOpenFile={onOpenFile}
              onLoadMore={onLoadMoreAllFiles}
            />
          ) : null}
        </View>
      ) : null}

      {model.selectedSource === "git" ? (
        <GitChanges model={model} onOpenFile={onOpenFile} onLoadMore={onLoadMoreGitChanges} />
      ) : null}
    </View>
  );
}

/** 文件夹与文件使用不同轮廓，避免依赖系统表情字体。 */
function FileEntryIcon({ directory = false, path = "" }: { readonly directory?: boolean; readonly path?: string }): ReactElement {
  const { colors } = useTheme();
  const extension = path.split(".").pop()?.toLowerCase();
  const code = extension !== undefined && ["ts", "tsx", "js", "jsx", "rs", "json"].includes(extension);
  const document = extension === "md" || extension === "txt";
  const glyph = code ? (extension === "json" ? "{}" : "‹›") : "≡";
  if (!directory) return <View aria-hidden={true} dataSet={{ fileIcon: "file", fileIconType: code ? "code" : document ? "document" : "file" }}
    style={{ width: 16, height: 18, alignItems: "center", justifyContent: "center", flexShrink: 0 }}>
    {code || document ? <Text style={{ fontFamily: fontFamily.mono, fontSize: code ? 13 : 16, color: colors.ink3 }}>{glyph}</Text> :
      <View style={{ width: 11, height: 14, borderWidth: 1, borderColor: colors.ink3, borderRadius: 2 }}>
        <View style={{ position: "absolute", top: 3, right: 2, width: 4, borderTopWidth: 1, borderTopColor: colors.ink3 }} />
      </View>}
  </View>;
  return <View aria-hidden={true} dataSet={{ fileIcon: directory ? "folder" : "file" }} style={{
    width: 16, height: 12, marginTop: 3, flexShrink: 0,
    borderWidth: 1.5, borderColor: colors.ink3, borderRadius: 3,
  }}>{directory ? <View style={{ position: "absolute", top: -4, left: -1, width: 8, height: 4,
    borderWidth: 1, borderBottomWidth: 0, borderColor: colors.ink3, borderTopLeftRadius: 2, borderTopRightRadius: 2 }} /> : null}</View>;
}

/** 只按可靠相对路径建目录；未知或根外路径保持独立，不猜测归属。 */
function ChangeTree<T>({ items, pathOf, renderItem, prefix = "" }: {
  readonly items: readonly T[];
  readonly pathOf: (item: T) => string | null;
  readonly renderItem: (item: T) => ReactElement | null;
  readonly prefix?: string;
}): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(new Set());
  const directories = new Map<string, T[]>();
  const leaves: T[] = [];
  for (const item of items) {
    const path = pathOf(item);
    const rest = path?.slice(prefix.length);
    const slash = rest?.indexOf("/") ?? -1;
    if (rest === undefined || slash < 0) { leaves.push(item); continue; }
    const name = rest.slice(0, slash);
    const group = directories.get(name) ?? [];
    group.push(item);
    directories.set(name, group);
  }
  return <View style={styles.list}>
    {[...directories].sort(([a], [b]) => a.localeCompare(b, undefined, { numeric: true, sensitivity: "base" })).map(([name, children]) => {
      let path = prefix + name + "/";
      let label = name;
      // 单一路径合并展示为 a/b；完整路径和目录关系仍保留。
      while (children.length > 0) {
        const segments = children.map(item => pathOf(item)?.slice(path.length).split("/") ?? []);
        const next = segments[0]?.[0];
        if (next === undefined || !segments.every(parts => parts.length > 1 && parts[0] === next)) break;
        label += "/" + next;
        path += next + "/";
      }
      const open = !collapsed.has(path);
      return <View key={path}>
        <Pressable role="button" aria-label={(open ? "收起目录 " : "展开目录 ") + label} aria-expanded={open}
          style={styles.directoryRow} dataSet={{ fileDirectoryRow: path, fileDirectoryExpanded: String(open) }}
          onPress={() => setCollapsed(previous => { const next = new Set(previous); if (open) next.add(path); else next.delete(path); return next; })}>
          <Text style={styles.chevron}>{open ? "▾" : "▸"}</Text><FileEntryIcon directory /><Text numberOfLines={1} style={styles.entryName}>{label}</Text>
        </Pressable>
        <View style={open ? styles.treeChildren : [styles.treeChildren, styles.hidden]}>
          <ChangeTree items={children} pathOf={pathOf} renderItem={renderItem} prefix={path} />
        </View>
      </View>;
    })}
    {leaves.map(renderItem)}
  </View>;
}

function changeAppearance(kind: string): "positive" | "negative" | "neutral" {
  if (kind === "deleted" || kind === "delete") return "negative";
  if (["added", "modified", "untracked", "create", "update"].includes(kind)) return "positive";
  return "neutral";
}
