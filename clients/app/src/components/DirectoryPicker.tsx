/**
 * 「选择文件夹」弹层（R16 的目录浏览，`workspace.browse`）。
 *
 * ## 数据只来自服务端
 *
 * 当前层与每一层条目都由服务端下发：`path` 是请求的**逐字回显**，条目只有名称、绝对路径与
 * 「是否已登记」。本组件因此不解析任何路径，也不由路径反推上一级——「上一级」走的是面包屑里
 * 上一段的**服务端路径**，「新建文件夹」的候选路径由 `create-session-model.ts` 的
 * `buildCreateFolderPath` 从「服务端回显的当前层 + 用户输入的一段名字」构成，并当场校验名字。
 *
 * ## 服务端不支持浏览时保留「选已登记目录」这条路
 *
 * 未协商 `core.workspace-browse.v1` 时**根本不发命令**（服务端会在触碰文件系统之前回
 * `protocol.feature_required`），弹层改为呈现已登记目录集合供选择，并明确说明浏览能力不可用——
 * MUST NOT 静默失败，也 MUST NOT 让用户手工输入一个必然失败的路径。
 *
 * 那条说明按**原因**分岔（`unavailable` prop，取自弹层模型的目录块）：还没连上主机时说的是
 * 「尚未连接」，只有已连上却没协商到该能力才说「服务端没有提供」。选择器自己分不出这两者
 * （它的 `browser.supported` 只有「能不能」一个布尔），因此这个事实由页面从模型层带进来。
 *
 * ## props
 *
 * 前四项是派发时冻结的接口（`plan.md`）：**初始路径**、**已登记目录集合**、**确认/取消回调**，
 * 加上取得数据所必需的接线（`browser` 是当前层状态，`onBrowse` / `onCreateFolder` 是上抛的动作），
 * 以及 `unavailable`（不能浏览的原因，见上）。组件本身不碰仓库与传输：页面把 store 的选择器与
 * 用户意图接进来。
 *
 * 迁移到 RNW 原语（`design.md` D1）：`View`/`Text`/`Pressable`/`TextInput` + `StyleSheet`；
 * 遮罩的 `position: fixed` 经 `webStyle` 直通（与外壳的抽屉/遮罩同一手法，不引入 CSS 文件）。
 */

import { useEffect, useRef, useState } from "react";
import type { ReactElement } from "react";

import type {
  BrowseUnavailableReason,
  CreateFailureKind,
  DirectoryBrowserModel,
  DirectorySelection,
  RegisteredDirectoryModel,
} from "../features/create-session-model";
import {
  BROWSE_EMPTY_FALLBACK_NOTICES,
  BROWSE_TRUNCATED_NOTICE,
  BROWSE_UNAVAILABLE_NOTICES,
  createFailureKind,
  selectionForCurrentLevel,
} from "../features/create-session-model";
import type { Palette } from "./theme";
import {
  Pressable,
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
  webStyle,
  zIndex,
} from "./theme";

export interface DirectoryPickerProps {
  /**
   * 初始路径：开始浏览的那一层（服务端下发过的绝对路径）；`null` = 该节点的根列表（此电脑）。
   *
   * 它是**打开时的请求目标**：挂载后第一次请求就是它，因此「在哪里打开」由调用方一次说清，
   * 组件不必猜。
   */
  readonly initialPath: string | null;
  /** 已登记目录集合（逐字来自快照）：不支持浏览时保留的选择，也是「已登记」标注的对照。 */
  readonly registeredDirectories: readonly RegisteredDirectoryModel[];
  /** 当前层与浏览状态（页面从仓库读出）。 */
  readonly browser: DirectoryBrowserModel;
  /** 请求某一层（`null` = 根列表）：页面据此派发 `workspace.browse`。 */
  readonly onBrowse: (path: string | null) => void;
  /** 在当前层新建一层目录：候选路径由模型层构造并校验。 */
  readonly onCreateFolder: (name: string) => void;
  /**
   * 「不能逐层浏览」的原因（`browser.supported` 为假时用它决定说明口径）。
   *
   * 缺省 `"unsupported"`：调用方给不出更准的事实时按「服务端未提供该能力」的口径呈现（与既有文案
   * 逐字相同）。页面从弹层模型的目录块取更准的那一份——未连上主机时是 `"offline"`，那时说
   * 「服务端没有提供该能力」是在断言一件本机无从知道的事（`selectedFeatures` 为空只是「还没协商」）。
   */
  readonly unavailable?: BrowseUnavailableReason | undefined;
  readonly onConfirm: (selection: DirectorySelection) => void;
  readonly onCancel: () => void;
}

/**
 * 弹层是**模态对话框**：`role="dialog"` + `aria-modal`（与外壳的帮助抽屉同一写法）。
 *
 * 两个属性写在遮罩那一层：原型里 `.dirpick` 既是遮罩也是那个 `role="dialog"` 的元素，
 * 且唯一的交互容器就是它。
 */

/** 按调色板建样式表：浅/深各建一次（模块加载时）。 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    scrim: {
      top: 0,
      right: 0,
      bottom: 0,
      left: 0,
      alignItems: "center",
      justifyContent: "center",
      padding: spacing.xl,
      backgroundColor: "rgba(10,12,16,.45)",
      zIndex: zIndex.drawer,
      ...webStyle({ position: "fixed" }),
    },
    box: {
      width: 460,
      gap: spacing.md,
      backgroundColor: colors.panel,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.lg,
      boxShadow: colors.shadow,
      ...webStyle({ maxWidth: "100%", maxHeight: "84vh" }),
    },
    head: {
      flexDirection: "row",
      alignItems: "center",
      gap: spacing.sm,
      paddingVertical: spacing.md,
      paddingHorizontal: spacing.lg,
      borderBottomWidth: 1,
      borderBottomColor: colors.line,
    },
    headTitle: {
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
      fontWeight: fontWeight.medium,
    },
    headWhere: {
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    spacer: {
      flexGrow: 1,
    },
    close: {
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.sm,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
      borderWidth: 1,
      borderColor: colors.line2,
    },
    closeText: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    body: {
      gap: spacing.sm,
      paddingVertical: spacing.md,
      paddingHorizontal: spacing.lg,
    },
    crumbRow: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      gap: spacing.xs,
    },
    crumb: {
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.xs,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
    },
    crumbText: {
      color: colors.accent,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
    },
    separator: {
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    up: {
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.sm,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
      borderWidth: 1,
      borderColor: colors.line,
    },
    upText: {
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    list: {
      gap: spacing.xs,
    },
    entry: {
      flexDirection: "row",
      alignItems: "center",
      gap: spacing.sm,
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.sm,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
    },
    entryName: {
      flexShrink: 1,
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      fontWeight: fontWeight.medium,
    },
    entryFlag: {
      flexShrink: 0,
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    entryFlagRegistered: {
      flexShrink: 0,
      color: colors.ok,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    entryArrow: {
      flexGrow: 1,
      textAlign: "right",
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    emptyLine: {
      paddingVertical: spacing.md,
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    noticeWarn: {
      color: colors.warn,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    noticeInfo: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    newRow: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      gap: spacing.sm,
    },
    newButton: {
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.md,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
      borderWidth: 1,
      borderColor: colors.line,
    },
    newButtonText: {
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    newHint: {
      flexShrink: 1,
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    newInput: {
      flexGrow: 1,
      flexShrink: 1,
      minWidth: 120,
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.sm,
      borderRadius: radius.sm,
      borderWidth: 1,
      borderColor: colors.line,
      backgroundColor: colors.panel2,
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    primary: {
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.md,
      borderRadius: radius.sm,
      backgroundColor: colors.accentSoft,
      borderWidth: 1,
      borderColor: colors.accent,
    },
    primaryText: {
      color: colors.accentInk,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      fontWeight: fontWeight.medium,
    },
    plain: {
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.md,
      borderRadius: radius.sm,
      backgroundColor: colors.panel,
      borderWidth: 1,
      borderColor: colors.line2,
    },
    plainText: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    foot: {
      flexDirection: "row",
      alignItems: "center",
      gap: spacing.sm,
      paddingVertical: spacing.md,
      paddingHorizontal: spacing.lg,
      borderTopWidth: 1,
      borderTopColor: colors.line,
    },
    where: {
      flexShrink: 1,
      color: colors.ink3,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

/**
 * 当前层的说明文案：根列表不是目录，因此提示先选一个盘/层。
 *
 * 读取失败的那一层**不**说成「当前位置」：`here` 是乐观写入的**请求**路径，那一层并没有读出来
 * （条目已清空、`status` 是 `failed`），把它说成已进入的层是同一处的第二句不实之言（第一句是
 * 下面的「新建文件夹」曾经照样提供）。
 */
function whereLabel(browser: DirectoryBrowserModel): string {
  if (browser.status === "failed") {
    return browser.here === null ? "此电脑（这一层没读出来）" : `这一层没读出来：${browser.here}`;
  }
  if (browser.here === null) return "此电脑（先选一个盘）";
  return `当前位置：${browser.here}`;
}

/**
 * 浏览失败的分类后缀：参数类（形状非法，服务端**不触碰**文件系统）与服务端不可用类（形式合法但
 * 不可达）必须能分开，其余错误只呈现服务端给的 `message`。
 */
const FAILURE_KIND_SUFFIXES: Readonly<Record<CreateFailureKind, string>> = {
  parameter: "（参数类错误）",
  unavailable: "（服务端不可用类错误）",
  other: "",
};

/** 服务端不支持浏览时的那一支：说明不可用，并把已登记目录集合摆出来供选择。 */
function UnsupportedBranch({
  unavailable,
  registeredDirectories,
  onConfirm,
  onCancel,
}: {
  readonly unavailable: BrowseUnavailableReason;
  readonly registeredDirectories: readonly RegisteredDirectoryModel[];
  readonly onConfirm: (selection: DirectorySelection) => void;
  readonly onCancel: () => void;
}): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  return (
    <>
      {/* spec 场景「服务端不支持浏览」：明确说明不可用，并保留「选已登记目录」这条路。
          说明文字按**原因**给：未连上时不把「还不知道服务端支持什么」写成「服务端没有提供」。 */}
      <Text role="alert" style={styles.noticeWarn} dataSet={{ browseUnsupported: unavailable }}>
        {BROWSE_UNAVAILABLE_NOTICES[unavailable]}
      </Text>
      {registeredDirectories.length === 0 ? (
        // 空列表这一行也按**原因**给：还没连上时不建议「把服务端升级到支持目录浏览的版本」
        // （真实原因是还没拿到目录列表，下一步是先连上）；服务端确实未提供该能力时那条建议仍成立。
        <Text style={styles.noticeInfo} dataSet={{ browseUnsupportedFallback: "empty" }}>
          {BROWSE_EMPTY_FALLBACK_NOTICES[unavailable]}
        </Text>
      ) : (
        <View role="list" style={styles.list} dataSet={{ registeredDirectoryChoices: "true" }}>
          {registeredDirectories.map((directory) => (
            <View key={directory.alias} role="listitem">
              <Pressable
                role="button"
                style={styles.entry}
                dataSet={{
                  action: "choose-registered-directory",
                  registeredAlias: directory.alias,
                  registeredPath: directory.path ?? "",
                }}
                onPress={() => {
                  onConfirm({
                    kind: "registered",
                    alias: directory.alias,
                    displayName: directory.displayName,
                    path: directory.path,
                  });
                }}
              >
                <Text style={styles.entryName} dataSet={{ registeredName: directory.displayName }}>
                  {directory.displayName}
                </Text>
                {/* 路径未协商下来时呈现「—」，MUST NOT 用别名或展示名拼一段像路径的文本。 */}
                <Text style={styles.entryFlag}>{directory.path ?? "路径未下发"}</Text>
                <Text style={styles.entryArrow}>›</Text>
              </Pressable>
            </View>
          ))}
        </View>
      )}
      <Pressable role="button" style={styles.plain} dataSet={{ action: "close-directory-picker" }} onPress={onCancel}>
        <Text style={styles.plainText}>取消</Text>
      </Pressable>
    </>
  );
}

/** 「选择文件夹」弹层。 */
export function DirectoryPicker({
  initialPath,
  registeredDirectories,
  browser,
  onBrowse,
  onCreateFolder,
  unavailable = "unsupported",
  onConfirm,
  onCancel,
}: DirectoryPickerProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  // 「新建文件夹」的折叠与输入框里的名字：纯呈现状态，住在这里（与目录卡的折叠态同一考虑）。
  const [newFolderOpen, setNewFolderOpen] = useState(false);
  const [folderName, setFolderName] = useState("");

  /**
   * 打开即请求初始层（原型 `openDirPicker` 的第一件事）。
   *
   * 依赖里只有 `initialPath`、`onBrowse` 与能力：前者由调用方算好（当前选定层的服务端路径），
   * 后者是页面用 `useCallback` 定住的稳定回调。后面用户每进一层都走点击回调，不会重跑这里。
   * ref 去重是为了防住重复挂载/重复执行时把同一层请求两遍。不支持浏览时**根本不请求**：
   * 服务端会在触碰文件系统之前回 `protocol.feature_required`，发了只会白跑一趟。
   */
  const requested = useRef<{ readonly path: string | null } | null>(null);
  useEffect(() => {
    if (!browser.supported) return;
    if (requested.current?.path === initialPath) return;
    requested.current = { path: initialPath };
    onBrowse(initialPath);
  }, [initialPath, onBrowse, browser.supported]);

  const pickCurrent = selectionForCurrentLevel(browser);
  // 「上一级」的目标路径先算成值：回调里就不再需要依赖 `browser.parent` 的非空收窄。
  const parentPath = browser.parent === null ? null : browser.parent.path;

  return (
    <View role="dialog" aria-modal={true} aria-label="选择文件夹" style={styles.scrim} dataSet={{ directoryPicker: "open" }}>
      <View style={styles.box}>
        <View style={styles.head}>
          <Text style={styles.headTitle} dataSet={{ directoryPickerTitle: "true" }}>
            选择文件夹
          </Text>
          <Text style={styles.headWhere}>主机上的目录</Text>
          <View style={styles.spacer} />
          <Pressable role="button" style={styles.close} dataSet={{ action: "close-directory-picker" }} onPress={onCancel}>
            <Text style={styles.closeText}>✕</Text>
          </Pressable>
        </View>

        <View style={styles.body}>
          {!browser.supported ? (
            <UnsupportedBranch
              unavailable={unavailable}
              registeredDirectories={registeredDirectories}
              onConfirm={onConfirm}
              onCancel={onCancel}
            />
          ) : (
            <>
              {/* 面包屑（第一段是「此电脑」）+ 上一级；每段的路径都逐字来自服务端。 */}
              <View style={styles.crumbRow} dataSet={{ browsePath: browser.here ?? "" }}>
                {browser.crumbs.map((crumb, index) => (
                  <View key={`${String(index)}|${crumb.path ?? ""}`} style={styles.crumbRow}>
                    {index === 0 ? null : <Text style={styles.separator}>›</Text>}
                    <Pressable
                      role="button"
                      style={styles.crumb}
                      dataSet={{ crumbIndex: String(index), crumbPath: crumb.path ?? "", crumbName: crumb.name }}
                      onPress={() => {
                        onBrowse(crumb.path);
                      }}
                    >
                      <Text style={styles.crumbText}>{crumb.name}</Text>
                    </Pressable>
                  </View>
                ))}
                <View style={styles.spacer} />
                {parentPath === null ? null : (
                  <Pressable
                    role="button"
                    style={styles.up}
                    dataSet={{ action: "browse-up", browseUpPath: parentPath }}
                    onPress={() => {
                      onBrowse(parentPath);
                    }}
                  >
                    <Text style={styles.upText}>↑ 上一级</Text>
                  </Pressable>
                )}
              </View>

              {/* 当前层的子目录。加载中、失败、空层与截断是四种**可区分**的呈现。 */}
              {browser.status === "loading" || browser.status === "idle" ? (
                <Text style={styles.emptyLine} dataSet={{ browseState: "loading" }}>
                  正在读取这一层的目录……
                </Text>
              ) : browser.status === "failed" ? (
                <Text
                  role="alert"
                  style={styles.noticeWarn}
                  dataSet={{
                    browseState: "failed",
                    browseFailureCode: browser.error?.code ?? "",
                    browseFailureKind: browser.error === null ? "other" : createFailureKind(browser.error),
                  }}
                >
                  {`${browser.error?.message ?? "这一层目录读取失败。"}${
                    browser.error === null ? "" : FAILURE_KIND_SUFFIXES[createFailureKind(browser.error)]
                  }`}
                </Text>
              ) : browser.entries.length === 0 ? (
                <Text style={styles.emptyLine} dataSet={{ browseState: "empty" }}>
                  这一层没有子文件夹
                </Text>
              ) : (
                <View role="list" style={styles.list} dataSet={{ browseEntries: "true" }}>
                  {browser.entries.map((entry) => (
                    <View key={entry.path} role="listitem">
                      <Pressable
                        role="button"
                        style={styles.entry}
                        dataSet={{
                          action: "enter-directory",
                          entryPath: entry.path,
                          entryName: entry.name,
                          entryRegistered: String(entry.registered),
                        }}
                        onPress={() => {
                          onBrowse(entry.path);
                        }}
                      >
                        <Text style={styles.entryName}>{entry.name}</Text>
                        {/* 已登记的目录就地标出：用户据此避免重复登记。 */}
                        <Text
                          style={entry.registered ? styles.entryFlagRegistered : styles.entryFlag}
                          dataSet={{ entryFlag: entry.registered ? "registered" : "" }}
                        >
                          {entry.registered ? "已登记" : ""}
                        </Text>
                        <Text style={styles.entryArrow}>›</Text>
                      </Pressable>
                    </View>
                  ))}
                </View>
              )}

              {browser.truncated ? (
                <Text style={styles.noticeWarn} dataSet={{ browseTruncated: "true" }}>
                  {BROWSE_TRUNCATED_NOTICE}
                </Text>
              ) : null}

              {/* 新建文件夹：只在**真的进了某一层**之后才提供——根列表不是目录；读取中/失败的那一层
                  也还没进（失败时 `here` 是乐观写入的**请求**路径，`status` 仍是 `failed`）。判据与
                  「选择此文件夹」的 `canConfirm`（`status === "ready" && here !== null`）同口径。 */}
              {browser.here === null || browser.status !== "ready" ? null : (
                <View style={styles.newRow} dataSet={{ browseNewFolderRow: "true" }}>
                  {newFolderOpen ? (
                    <>
                      <TextInput
                        value={folderName}
                        onChangeText={setFolderName}
                        placeholder="新文件夹的名字"
                        placeholderTextColor={theme.colors.ink3}
                        accessibilityLabel="新文件夹的名字"
                        style={styles.newInput}
                        dataSet={{ browseFolderName: "true", browseFolderNameValue: folderName }}
                      />
                      <Pressable
                        role="button"
                        style={styles.primary}
                        dataSet={{ action: "create-folder" }}
                        onPress={() => {
                          onCreateFolder(folderName);
                        }}
                      >
                        <Text style={styles.primaryText}>新建并选择</Text>
                      </Pressable>
                      <Pressable
                        role="button"
                        style={styles.plain}
                        dataSet={{ action: "cancel-create-folder" }}
                        onPress={() => {
                          setNewFolderOpen(false);
                        }}
                      >
                        <Text style={styles.plainText}>取消</Text>
                      </Pressable>
                    </>
                  ) : (
                    <>
                      <Pressable
                        role="button"
                        style={styles.newButton}
                        dataSet={{ action: "open-new-folder" }}
                        onPress={() => {
                          setFolderName("");
                          setNewFolderOpen(true);
                        }}
                      >
                        <Text style={styles.newButtonText}>＋ 新建文件夹…</Text>
                      </Pressable>
                      <Text style={styles.newHint}>在这里新建一层目录；创建会话时会先在主机上建它</Text>
                    </>
                  )}
                </View>
              )}

              {browser.folderError === null ? null : (
                <Text role="alert" style={styles.noticeWarn} dataSet={{ browseFolderError: "true" }}>
                  {`${browser.folderError}没有新建目录，也没有创建会话。`}
                </Text>
              )}
            </>
          )}
        </View>

        {!browser.supported ? null : (
          <View style={styles.foot}>
            <Text style={styles.where} dataSet={{ browseWhere: browser.here ?? "" }}>
              {whereLabel(browser)}
            </Text>
            <View style={styles.spacer} />
            <Pressable role="button" style={styles.plain} dataSet={{ action: "close-directory-picker" }} onPress={onCancel}>
              <Text style={styles.plainText}>取消</Text>
            </Pressable>
            <Pressable
              role="button"
              disabled={pickCurrent === null}
              style={styles.primary}
              dataSet={{ action: "confirm-directory" }}
              onPress={() => {
                if (pickCurrent !== null) onConfirm(pickCurrent);
              }}
            >
              <Text style={styles.primaryText}>选择此文件夹</Text>
            </Pressable>
          </View>
        )}
      </View>
    </View>
  );
}
