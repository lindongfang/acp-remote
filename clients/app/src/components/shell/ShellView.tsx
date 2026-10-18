/**
 * 共享外壳的**呈现层**（容器见 `./Shell.tsx`）。
 *
 * 拆出来的理由有两条：
 *
 * 1. 呈现只吃模型与回调，因此「配对路由不呈现顶栏」「明文连接呈现横幅与徽标」这类行为可以用
 *    `renderToStaticMarkup` 加显式输入断言，不必造浏览器环境；
 * 2. 容器 import `expo-router`，而 `vitest` 的 node 环境解析真实 `expo-router` 会失败——
 *    呈现层不碰路由，因此两个文件各担一件事。
 *
 * 样式全部经 `StyleSheet.create` 编译成**同源样式表里的类**：不写 CSS 文件、不写内联 `<style>`、
 * 也不用 `style=` 属性（网页专属属性走 `theme.ts` 的 `webStyle` 断言点，见那里的说明）。
 */

import type { ReactElement, ReactNode } from "react";

import type { Palette, ThemePreference } from "../theme";
import {
  Pressable,
  StyleSheet,
  Text,
  THEME_PREFERENCE_LABELS,
  THEME_PREFERENCES,
  View,
  fontFamily,
  fontSize,
  fontWeight,
  motion,
  palettes,
  radius,
  spacing,
  useTheme,
  webStyle,
  zIndex,
} from "../theme";
import type { ShellModel } from "./shell-model";

/** 未加密托管时的横幅文案（原型 `.devbanner` 的口径）。 */
const DEV_BANNER_TEXT =
  "开发模式：连接未加密，使用独立的短期开发身份 —— 仅用于本机调试，不可作为安全验收结果。";

/** 按调色板建样式表：浅/深各建一次（模块加载时），渲染路径上不再新建样式对象。 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    root: {
      flexGrow: 1,
      flexShrink: 1,
      minHeight: 0,
      flexDirection: "column",
      backgroundColor: colors.bg,
      // 全局滚动条：`scrollbar-width` / `scrollbar-color` 是**继承**属性，在根节点上声明一次即
      // 覆盖后代全部滚动容器；RNW 把 `scrollbarWidth` 编译成显式支持项（取值为 `none` 时还会
      // 附上一条 `::-webkit-scrollbar` 规则），取值仍来自调色板。
      ...webStyle({ scrollbarWidth: "thin", scrollbarColor: `${colors.line} transparent` }),
    },
    content: {
      flexGrow: 1,
      flexShrink: 1,
      minHeight: 0,
      flexDirection: "column",
      // 路由容器之外的底色也跟随主题：宽屏页面自身的最大宽度两侧不会露出浏览器的白底。
      backgroundColor: colors.bg,
    },
    devBanner: {
      flexDirection: "row",
      justifyContent: "center",
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.xl,
      backgroundColor: colors.warnSoft,
      borderBottomWidth: 1,
      borderBottomColor: colors.line,
    },
    devBannerText: {
      color: colors.warn,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      lineHeight: 20.8,
      textAlign: "center",
    },
    topBar: {
      flexDirection: "row",
      alignItems: "center",
      gap: spacing.md,
      height: 56,
      flexShrink: 0,
      paddingHorizontal: spacing.xl,
      backgroundColor: colors.panel,
      borderBottomWidth: 1,
      borderBottomColor: colors.line,
      zIndex: zIndex.topBar,
    },
    logo: {
      flexShrink: 0,
      flexDirection: "row",
      alignItems: "center",
      gap: spacing.sm,
    },
    logoMark: {
      width: 18,
      height: 18,
      borderRadius: 5,
      backgroundColor: colors.accent,
      alignItems: "center",
      justifyContent: "center",
    },
    logoMarkInner: {
      width: 8,
      height: 8,
      borderRadius: 2,
      backgroundColor: colors.panel,
    },
    logoText: {
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
      lineHeight: 22.4,
      fontWeight: fontWeight.medium,
    },
    spacer: {
      flexGrow: 1,
      flexShrink: 1,
      flexBasis: 0,
    },
    capsule: {
      minWidth: 0,
      flexShrink: 1,
      flexDirection: "row",
      alignItems: "center",
      gap: spacing.sm,
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.sm + 2,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.lg,
      backgroundColor: colors.panel2,
    },
    capsuleBorderWarn: {
      borderColor: colors.warnSoft,
    },
    capsuleBorderDanger: {
      borderColor: colors.dangerSoft,
    },
    dot: {
      width: 7,
      height: 7,
      flexShrink: 0,
      borderRadius: 4,
    },
    dotOk: {
      backgroundColor: colors.ok,
    },
    dotWarn: {
      backgroundColor: colors.warn,
    },
    dotDanger: {
      backgroundColor: colors.danger,
    },
    capsuleLabel: {
      flexShrink: 1,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
      lineHeight: 22.4,
    },
    capsuleShortWord: {
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      lineHeight: 20.8,
      opacity: 0.9,
    },
    // 折角：原型用一段内联 SVG（文字字形的高低不对）。这里用两条边框旋转 45° 画出同一个形状，
    // 同样是样式类，不引入 SVG 依赖、也不写内联样式。
    capsuleCaret: {
      width: 9,
      height: 9,
      borderRightWidth: 1.6,
      borderBottomWidth: 1.6,
      borderColor: colors.ink3,
      transform: "rotate(45deg)",
    },
    toneTextOk: {
      color: colors.ink2,
    },
    toneTextWarn: {
      color: colors.warn,
    },
    toneTextDanger: {
      color: colors.danger,
    },
    devBadge: {
      color: colors.warn,
      backgroundColor: colors.warnSoft,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
      lineHeight: 17.6,
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.sm + 2,
      borderRadius: radius.lg,
      overflow: "hidden",
    },
    iconButton: {
      flexShrink: 0,
      width: 30,
      height: 30,
      alignItems: "center",
      justifyContent: "center",
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
    },
    iconGlyph: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      lineHeight: 20.8,
    },
    // 主题开关的图标：太阳/月亮都用图形画（不依赖字体里有没有那个字形）。原型控制台里可选的
    // 「图标样式 / 图标含义」不属产品行为，这里固定为「点击后是什么」的含义。
    iconMoon: {
      width: 14,
      height: 14,
      overflow: "hidden",
    },
    iconMoonBody: {
      width: 13,
      height: 13,
      borderRadius: 7,
      backgroundColor: colors.ink2,
    },
    iconMoonBite: {
      position: "absolute",
      top: -4,
      left: 5,
      width: 13,
      height: 13,
      borderRadius: 7,
      backgroundColor: colors.panel,
    },
    scrim: {
      top: 0,
      right: 0,
      bottom: 0,
      left: 0,
      backgroundColor: "rgba(10,12,16,.35)",
      opacity: 0,
      pointerEvents: "none",
      zIndex: zIndex.scrim,
      ...webStyle({ position: "fixed" }),
    },
    scrimOpen: {
      opacity: 1,
      pointerEvents: "auto",
    },
    scrimMotion: webStyle({ transitionProperty: "opacity", transitionDuration: `${motion.base}ms` }),
    drawer: {
      top: 0,
      right: 0,
      bottom: 0,
      width: 420,
      padding: spacing.xl,
      backgroundColor: colors.panel,
      borderLeftWidth: 1,
      borderLeftColor: colors.line,
      gap: spacing.lg,
      transform: "translateX(100%)",
      zIndex: zIndex.drawer,
      ...webStyle({ position: "fixed", maxWidth: "92vw" }),
    },
    drawerOpen: {
      transform: "translateX(0)",
    },
    drawerMotion: webStyle({
      transitionProperty: "transform",
      transitionDuration: `${motion.drawer}ms`,
      transitionTimingFunction: "ease",
    }),
    noMotion: webStyle({ transitionDuration: "0ms" }),
    drawerTitle: {
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.lg,
      lineHeight: 27.2,
      fontWeight: fontWeight.medium,
    },
    drawerSection: {
      flexDirection: "column",
      gap: spacing.sm,
    },
    drawerSectionTitle: {
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
      lineHeight: 17.6,
      fontWeight: fontWeight.medium,
    },
    drawerNote: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      lineHeight: 20.8,
    },
    drawerWarnNote: {
      color: colors.warn,
    },
    themeChoices: {
      flexDirection: "row",
      flexWrap: "wrap",
      gap: spacing.sm,
    },
    themeChoice: {
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.md,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
    },
    themeChoiceOn: {
      borderColor: colors.accent,
      backgroundColor: colors.accentSoft,
    },
    themeChoiceText: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      lineHeight: 20.8,
    },
    themeChoiceTextOn: {
      color: colors.accentInk,
      fontWeight: fontWeight.medium,
    },
    toast: {
      left: "50%",
      bottom: 26,
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.lg,
      borderRadius: radius.sm,
      backgroundColor: colors.ink,
      zIndex: zIndex.toast,
      ...webStyle({ position: "fixed", transform: "translateX(-50%)" }),
    },
    toastMotion: webStyle({
      transitionProperty: "opacity, transform",
      transitionDuration: `${motion.base}ms`,
    }),
    toastText: {
      color: colors.bg,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      lineHeight: 20.8,
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

export interface ShellViewProps {
  readonly model: ShellModel;
  /** 当前主题偏好（抽屉里的三态控件据此标出选中项）。 */
  readonly themePreference: ThemePreference;
  /** 当前实际生效的是不是深色（主题开关据此决定切换方向与图标）。 */
  readonly themeDark: boolean;
  readonly reducedMotion: boolean;
  readonly helpOpen: boolean;
  readonly toastMessage: string | null;
  readonly onToggleTheme: () => void;
  readonly onSelectTheme: (next: ThemePreference) => void;
  readonly onOpenHelp: () => void;
  readonly onCloseHelp: () => void;
  readonly onOpenHosts: () => void;
  readonly children?: ReactNode;
}

/**
 * 外壳的呈现：开发模式横幅 → 顶栏 → 路由内容 → 抽屉/遮罩/提示（`design.md` D1 的装配顺序）。
 */
export function ShellView(props: ShellViewProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const tone = props.model.capsule.tone;
  const capsuleBorderStyle =
    tone === "warn" ? styles.capsuleBorderWarn : tone === "danger" ? styles.capsuleBorderDanger : null;
  const toneTextStyle =
    tone === "warn" ? styles.toneTextWarn : tone === "danger" ? styles.toneTextDanger : styles.toneTextOk;
  const dotStyle = tone === "warn" ? styles.dotWarn : tone === "danger" ? styles.dotDanger : styles.dotOk;
  const scrimTransition = props.reducedMotion ? styles.noMotion : styles.scrimMotion;
  const drawerTransition = props.reducedMotion ? styles.noMotion : styles.drawerMotion;

  return (
    <View style={styles.root} dataSet={{ shell: "root" }}>
      {props.model.insecure ? (
        <View style={styles.devBanner} dataSet={{ devBanner: "visible" }}>
          <Text style={styles.devBannerText}>{DEV_BANNER_TEXT}</Text>
        </View>
      ) : null}

      {props.model.topBarVisible ? (
        <View role="banner" style={styles.topBar} dataSet={{ shellTopBar: "visible" }}>
          <View style={styles.logo}>
            <View style={styles.logoMark}>
              <View style={styles.logoMarkInner} />
            </View>
            <Text style={styles.logoText}>ACP Remote</Text>
          </View>

          <Pressable
            role="button"
            style={[styles.capsule, capsuleBorderStyle]}
            aria-label={`设备与连接：${props.model.capsule.deviceLabel}（${props.model.capsule.statusLabel}）`}
            dataSet={{
              deviceCapsule: "true",
              capsuleTone: tone,
              capsuleState: props.model.capsule.state ?? "unknown",
              action: "open-hosts",
            }}
            onPress={props.onOpenHosts}
          >
            <View style={[styles.dot, dotStyle]} />
            <Text numberOfLines={1} style={[styles.capsuleLabel, toneTextStyle]} dataSet={{ capsuleDevice: "true" }}>
              {props.model.capsule.deviceLabel}
            </Text>
            {props.model.capsule.shortWord === null ? null : (
              <Text style={[styles.capsuleShortWord, toneTextStyle]} dataSet={{ capsuleShortWord: "true" }}>
                {props.model.capsule.shortWord}
              </Text>
            )}
            <View style={styles.capsuleCaret} />
          </Pressable>

          <View style={styles.spacer} />

          {props.model.insecure ? (
            <Text style={styles.devBadge} dataSet={{ devBadge: "visible" }}>
              开发模式
            </Text>
          ) : null}

          <Pressable
            role="button"
            style={styles.iconButton}
            aria-label={props.themeDark ? "当前：黑夜，点击切换" : "当前：白天，点击切换"}
            dataSet={{ action: "toggle-theme", themePreference: props.themePreference }}
            onPress={props.onToggleTheme}
          >
            {props.themeDark ? (
              <Text style={styles.iconGlyph}>{"☀️"}</Text>
            ) : (
              <View style={styles.iconMoon}>
                <View style={styles.iconMoonBody} />
                <View style={styles.iconMoonBite} />
              </View>
            )}
          </Pressable>

          <Pressable
            role="button"
            style={styles.iconButton}
            aria-label="说明"
            dataSet={{ action: "open-help" }}
            onPress={props.onOpenHelp}
          >
            <Text style={styles.iconGlyph}>ⓘ</Text>
          </Pressable>
        </View>
      ) : null}

      <View style={styles.content} dataSet={{ shellContent: "true" }}>
        {props.children}
      </View>

      <View
        role="dialog"
        aria-modal={true}
        aria-label="说明"
        aria-hidden={!props.helpOpen}
        style={[styles.drawer, props.helpOpen ? styles.drawerOpen : null, drawerTransition]}
        dataSet={{ helpDrawer: props.helpOpen ? "open" : "closed" }}
      >
        <Text style={styles.drawerTitle}>说明</Text>

        <View style={styles.drawerSection}>
          <Text style={styles.drawerSectionTitle}>主题</Text>
          <View style={styles.themeChoices}>
            {THEME_PREFERENCES.map((preference) => (
              <Pressable
                key={preference}
                role="button"
                style={[styles.themeChoice, preference === props.themePreference ? styles.themeChoiceOn : null]}
                dataSet={{
                  action: "select-theme",
                  themeChoice: preference,
                  themeChoiceState: preference === props.themePreference ? "on" : "off",
                }}
                onPress={() => {
                  props.onSelectTheme(preference);
                }}
              >
                <Text
                  style={[
                    styles.themeChoiceText,
                    preference === props.themePreference ? styles.themeChoiceTextOn : null,
                  ]}
                >
                  {THEME_PREFERENCE_LABELS[preference]}
                </Text>
              </Pressable>
            ))}
          </View>
          <Text style={styles.drawerNote}>
            「跟随系统」随电脑的深浅色设置变化；显式选择白天或黑夜后，下次打开仍按该取值呈现。
          </Text>
        </View>

        <View style={styles.drawerSection}>
          <Text style={styles.drawerSectionTitle}>连接</Text>
          <Text style={[styles.drawerNote, props.model.insecure ? styles.drawerWarnNote : null]}>
            {props.model.insecure
              ? "本页不是 HTTPS：连接未加密，只应在本机调试时使用，不能当作安全验收结果。"
              : "本页由 HTTPS 托管；与电脑的连接仍要经过配对与双向认证。"}
          </Text>
        </View>

        <View style={styles.drawerSection}>
          <Text style={styles.drawerSectionTitle}>关于</Text>
          <Text style={styles.drawerNote}>
            这是运行在浏览器里的前端。设备密钥留在本机，配对与授权都在电脑上确认。
          </Text>
        </View>
      </View>

      <Pressable
        role="button"
        aria-label="关闭说明"
        style={[styles.scrim, props.helpOpen ? styles.scrimOpen : null, scrimTransition]}
        dataSet={{ helpScrim: props.helpOpen ? "open" : "closed" }}
        onPress={props.onCloseHelp}
      />

      {props.toastMessage === null ? null : (
        <View
          role="status"
          style={[styles.toast, props.reducedMotion ? null : styles.toastMotion]}
          dataSet={{ toast: "visible" }}
        >
          <Text style={styles.toastText}>{props.toastMessage}</Text>
        </View>
      )}
    </View>
  );
}
