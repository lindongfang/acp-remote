/**
 * 配对面板（`docs/FRONTEND_DESIGN.md` §4.1）。
 *
 * ## 页面只有一个入口，也只有一个来源
 *
 * 配对内容只从浏览器地址片段进入（`/pair#data=…` 或 `/node-link/pair#data=…`）；本组件
 * **没有**输入框、粘贴框、提交按钮或「重新扫码」入口。它只是状态页：渲染 `ClientStore`
 * 产出的固定呈现（`PairingPresentation`），不判定、不解析、不发请求。
 *
 * ## 只渲染固定文案
 *
 * 标题、说明、警示、色调与是否显示倒计时全部来自模型；后端错误 `message` 与任何载荷内容
 * 都没有进入页面的通道。`data-pairing-error` 锚点仍带错误码，供诊断与测试使用。
 *
 * ## 转圈与倒计时
 *
 * 进行中状态画一个居中转圈；圆环内是载荷 `expiresAt` 推出的剩余有效期（`m:ss`，每秒重算）。
 * 归零只显示 `0:00`，状态迁移由 `ClientStore` 的到期定时器负责，页面不自行改状态。
 * 动画用 React Native `Animated`（`useNativeDriver: false`），不用 CSS keyframes。
 *
 * ## 主题
 *
 * 颜色一律来自 `useTheme()` 的当前调色板（浅/深各建一次样式表）；本页不提供页内主题切换，
 * 也不绘制页头与返回入口——共享外壳在配对路径隐藏全局顶栏，本页就是这个例外下的全部内容。
 */

import { useEffect, useRef, useState } from "react";
import type { ReactElement } from "react";
import type * as ReactNative from "react-native";
// @ts-expect-error `react-native-web` 不发布类型声明；运行时绑定与 `./theme` 同一取舍。
import * as ReactNativeWeb from "react-native-web";

import type { PairingPageModel } from "../features/pairing-model";
import type { Palette } from "./theme";
import {
  StyleSheet,
  Text,
  View,
  fontFamily,
  fontSize,
  fontWeight,
  palettes,
  radius,
  spacing,
  useTheme,
} from "./theme";

/** 运行时取 `react-native-web` 的动画实现，类型取 `react-native` 的公开声明。 */
const Animated: typeof ReactNative.Animated = ReactNativeWeb.Animated;
const Easing: typeof ReactNative.Easing = ReactNativeWeb.Easing;

export interface PairingPanelProps {
  readonly model: PairingPageModel;
}

/** 按调色板建样式表：浅/深各建一次（模块加载时）。 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    stage: {
      alignItems: "center",
      gap: spacing.md,
      width: "100%",
    },
    ringBox: {
      width: 112,
      height: 112,
      alignItems: "center",
      justifyContent: "center",
    },
    ring: {
      position: "absolute",
      top: 0,
      left: 0,
      right: 0,
      bottom: 0,
      borderRadius: 56,
      borderWidth: 4,
      borderColor: colors.line,
    },
    ringCount: {
      color: colors.ink,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xl,
      fontWeight: fontWeight.value,
    },
    title: {
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xl,
      fontWeight: fontWeight.medium,
      textAlign: "center",
    },
    lede: {
      maxWidth: 560,
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
      textAlign: "center",
    },
    note: {
      maxWidth: 560,
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      textAlign: "center",
    },
    card: {
      alignSelf: "center",
      width: "100%",
      maxWidth: 420,
      gap: spacing.xs,
      padding: spacing.md,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
    },
    cardRow: {
      flexDirection: "row",
      alignItems: "baseline",
      gap: spacing.md,
    },
    cardKey: {
      minWidth: 74,
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    cardValue: {
      flexShrink: 1,
      color: colors.ink2,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.sm,
      fontWeight: fontWeight.value,
    },
    deviceId: {
      color: colors.ink3,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
      textAlign: "center",
    },
    glyph: {
      fontSize: 40,
      lineHeight: 44,
      textAlign: "center",
    },
    glyphDanger: {
      color: colors.danger,
    },
    glyphWarning: {
      color: colors.warn,
    },
    warning: {
      flexDirection: "row",
      alignItems: "baseline",
      maxWidth: 560,
      gap: spacing.sm,
      padding: spacing.md,
      borderRadius: radius.sm,
      backgroundColor: colors.warnSoft,
    },
    warningMark: {
      color: colors.warn,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    warningText: {
      flexShrink: 1,
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

/** `expiresAtMs` + 当前时刻 → `m:ss`；`null` 表示没有可显示的截止时间。 */
function formatRemaining(expiresAtMs: number | null, nowMs: number): string | null {
  if (expiresAtMs === null) return null;
  // 向上取整：还剩不足一秒时仍显示 0:01，真正归零才显示 0:00（与服务端判定同一时刻）。
  const totalSeconds = Math.ceil(Math.max(0, expiresAtMs - nowMs) / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}

/** 配对面板。 */
export function PairingPanel({ model }: PairingPanelProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const { presentation } = model;

  // 转到环形：只在进度态运行；动画值归零，离开进度态时没有残留的旋转。
  const spin = useRef(new Animated.Value(0)).current;
  useEffect(() => {
    if (presentation.kind !== "progress") return;
    spin.setValue(0);
    const loop = Animated.loop(
      Animated.timing(spin, {
        toValue: 1,
        duration: 1_100,
        easing: Easing.linear,
        useNativeDriver: false,
      }),
    );
    loop.start();
    return () => {
      loop.stop();
    };
  }, [presentation.kind, spin]);
  const rotate = spin.interpolate({ inputRange: [0, 1], outputRange: ["0deg", "360deg"] });

  // 倒计时：每秒用绝对截止时间重算，不累加、也不猜时长。
  const countdownActive = presentation.countdown && model.expiresAtMs !== null;
  const [countdown, setCountdown] = useState<string | null>(() =>
    formatRemaining(model.expiresAtMs, Date.now()),
  );
  useEffect(() => {
    if (!countdownActive) {
      setCountdown(null);
      return;
    }
    const tick = (): void => {
      setCountdown(formatRemaining(model.expiresAtMs, Date.now()));
    };
    tick();
    const timer = setInterval(tick, 1_000);
    return () => {
      clearInterval(timer);
    };
  }, [countdownActive, model.expiresAtMs]);

  /** 主机出示信息（只在等待确认与 node-link 的对端信息里呈现）。 */
  const hostCard = (): ReactElement | null => {
    if (model.host === null) return null;
    return (
      <View style={styles.card} dataSet={{ pairingHost: "true" }}>
        <View style={styles.cardRow}>
          <Text style={styles.cardKey}>地址</Text>
          <Text style={styles.cardValue} dataSet={{ hostAddress: model.host.address }}>
            {model.host.address}
          </Text>
        </View>
        <View style={styles.cardRow}>
          <Text style={styles.cardKey}>主机 ID</Text>
          <Text style={styles.cardValue}>{model.host.hostId ?? "未知"}</Text>
        </View>
      </View>
    );
  };

  /** 固定呈现 → 居中舞台。 */
  const stage = (): ReactElement => {
    if (presentation.kind === "progress") {
      const showCountdown = presentation.countdown && countdown !== null;
      return (
        <>
          <View style={styles.ringBox} role="status" aria-label="配对中">
            <Animated.View
              style={[
                styles.ring,
                { borderTopColor: theme.colors.accent, transform: [{ rotate }] },
              ]}
            />
            {showCountdown ? (
              <Text style={styles.ringCount} dataSet={{ pairingCountdown: "true" }}>
                {countdown}
              </Text>
            ) : null}
          </View>
          <Text role="heading" style={styles.title}>
            {presentation.title}
          </Text>
          <Text style={styles.lede}>{presentation.lede}</Text>
          {presentation.note === null ? null : <Text style={styles.note}>{presentation.note}</Text>}
          {model.phase === "pending_confirmation" ? hostCard() : null}
          {model.deviceId === null ? null : (
            <Text style={styles.deviceId} dataSet={{ pairingDeviceId: model.deviceId }}>
              本机设备 ID：{model.deviceId}
            </Text>
          )}
        </>
      );
    }

    if (presentation.kind === "info") {
      return (
        <>
          <Text role="heading" style={styles.title}>
            {presentation.title}
          </Text>
          <Text style={styles.lede}>{presentation.lede}</Text>
          {presentation.note === null ? null : <Text style={styles.note}>{presentation.note}</Text>}
          {hostCard()}
        </>
      );
    }

    // 失败页：glyph + 固定标题/说明/警示。不呈现主机卡片、不提供任何按钮。
    const danger = presentation.tone === "danger";
    return (
      <>
        <Text
          style={[styles.glyph, danger ? styles.glyphDanger : styles.glyphWarning]}
          aria-hidden={true}
        >
          {danger ? "✕" : "⚠"}
        </Text>
        <Text role="heading" style={styles.title}>
          {presentation.title}
        </Text>
        <Text style={styles.lede}>{presentation.lede}</Text>
        {presentation.warning === null ? null : (
          <View style={styles.warning}>
            <Text style={styles.warningMark}>⚠</Text>
            <Text style={styles.warningText}>{presentation.warning}</Text>
          </View>
        )}
      </>
    );
  };

  return (
    <View
      role="region"
      aria-label="与这台电脑配对"
      style={styles.stage}
      dataSet={{
        route: "pair",
        pairingPhase: model.phase,
        // 「配对页已渲染」的锚点：不再是输入入口，但 e2e/外壳用例仍以它定位本页。
        pairingEntry: "true",
        ...(model.failure === null ? {} : { pairingFailure: model.failure }),
      }}
    >
      {stage()}
      {/* 错误码仍保留为诊断锚点；`message` 一律不渲染（固定文案才是页面内容）。 */}
      {model.error === null ? null : (
        <View role="alert" dataSet={{ pairingError: model.error.code }} />
      )}
    </View>
  );
}
