/**
 * 配对面板（`docs/FRONTEND_DESIGN.md` §4.1）。
 *
 * ## 两种载荷获取方式，一套呈现
 *
 * 配对内容来自地址片段（`/pair#data=…` 或 `/node-link/pair#data=…`），或者来自 `/pair` 无片段
 * 时的 8 位配对码输入（原型 S0）。输入态只呈现公开来源与单字符格；配对码只活在本组件的
 * 内存状态里，提交给 `ClientStore.submitPairingCode` 后立即随阶段切换清除，**不**进入模型、
 * URL、Cookie 或任何存储。本组件不判定、不解析、不发请求。
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
 * 颜色一律来自 `useTheme()` 的当前调色板（浅/深各建一次样式表）；页面右上角的
 * `PairingThemeToggle` 复用共享主题偏好，本组件自身不持有主题状态。共享外壳在配对路径隐藏
 * 全局顶栏，本页不绘制页头与返回入口。
 */

import { useEffect, useRef, useState } from "react";
import type { ReactElement } from "react";
import type * as ReactNative from "react-native";
// @ts-expect-error `react-native-web` 不发布类型声明；运行时绑定与 `./theme` 同一取舍。
import * as ReactNativeWeb from "react-native-web";

import type { PairingPageModel } from "../features/pairing-model";
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
} from "./theme";

/** 运行时取 `react-native-web` 的动画实现，类型取 `react-native` 的公开声明。 */
const Animated: typeof ReactNative.Animated = ReactNativeWeb.Animated;
const Easing: typeof ReactNative.Easing = ReactNativeWeb.Easing;

export interface PairingPanelProps {
  readonly model: PairingPageModel;
  /**
   * 配对码输入态提交（`/pair` 无 fragment 时）。码只在组件内存与本回调里存在，
   * 不进入模型、URL 或任何存储。
   */
  readonly onSubmitCode?: (code: string) => void;
}

/** 配对码的位数（40 bit = 8 个 Crockford 字符）。 */
const CODE_LENGTH = 8;

/**
 * 单个输入格的文本 → 待分配的字符序列。
 *
 * - 单字符（含删除后的空串）原样返回，交给所在格；
 * - 多字符（粘贴整码，或没有选中时的追加）返回待分配序列：满 8 位从头填满全部格，
 *   更短则从**被编辑的格**开始写入（见 {@link codeCellsAfterEdit}）；
 * - 多个分隔符、超过 8 位等形态返回 `null`（忽略这次变更）；字符合法性最终由
 *   `normalizePairingCode` 在提交时判定，格式不符因此仍会得到明确的失败呈现。
 */
function codeCharactersOf(text: string): string | null {
  const trimmed = text.trim().toUpperCase();
  if (trimmed.length === 0) return "";
  if (trimmed.includes("-")) {
    if (trimmed.length !== CODE_LENGTH + 1 || trimmed.charAt(4) !== "-") return null;
    return `${trimmed.slice(0, 4)}${trimmed.slice(5)}`
      .replaceAll("I", "1")
      .replaceAll("L", "1")
      .replaceAll("O", "0");
  }
  if (trimmed.length > CODE_LENGTH) return null;
  return trimmed.replaceAll("I", "1").replaceAll("L", "1").replaceAll("O", "0");
}

/**
 * 一次单元格文本变更 → 新的 8 格。
 *
 * 单字符（含删除后的空串）只改**被编辑的那一格**；多字符变更从**这一格**开始写入，
 * 写不到的格保持原值。只有整码（8 位或 `XXXX-XXXX`）才从头填满全部格：那是粘贴整码，
 * 起点由码本身决定。此前的实现对所有多字符变更都从格 0 重建，输入第 4 位时的一次
 * 「多字符」回调会把前面已输入的位覆盖掉。
 *
 * 纯函数：组件只负责把结果放进状态与移动焦点，行为因此可以单独断言。
 */
export function codeCellsAfterEdit(
  cells: readonly string[],
  index: number,
  characters: string,
): readonly string[] {
  if (characters.length > 1) {
    const start = characters.length >= CODE_LENGTH ? 0 : index;
    return cells.map((cell, position) => {
      const offset = position - start;
      return offset >= 0 && offset < characters.length ? characters.charAt(offset) : cell;
    });
  }
  return cells.map((cell, position) => (position === index ? characters : cell));
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
    codeOriginCard: {
      alignSelf: "center",
      width: "100%",
      maxWidth: 420,
      alignItems: "center",
      gap: spacing.xs,
      padding: spacing.md,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
    },
    secureBadge: {
      color: colors.ok,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
      fontWeight: fontWeight.medium,
    },
    insecureBadge: {
      color: colors.warn,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
      fontWeight: fontWeight.medium,
    },
    codeCells: {
      flexDirection: "row",
      justifyContent: "center",
      gap: spacing.xs,
    },
    codeCell: {
      width: 32,
      height: 46,
      padding: 0,
      borderWidth: 1,
      borderColor: colors.line2,
      borderRadius: radius.sm,
      backgroundColor: colors.panel,
      color: colors.ink,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xl,
      fontWeight: fontWeight.value,
      textAlign: "center",
    },
    primary: {
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.md,
      borderWidth: 1,
      borderColor: colors.accent,
      borderRadius: radius.sm,
      backgroundColor: colors.accentSoft,
    },
    primaryText: {
      color: colors.accentInk,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      fontWeight: fontWeight.medium,
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
export function PairingPanel({ model, onSubmitCode }: PairingPanelProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const { presentation } = model;

  // 配对码只活在这个组件的内存里：不写入模型、URL、Cookie 或任何存储。离开输入态
  // （lookup 已提交、失败或页面卸载）即清空。
  const [codeCells, setCodeCells] = useState<readonly string[]>(() =>
    Array.from({ length: CODE_LENGTH }, () => ""),
  );
  const codeCellRefs = useRef<Array<ReactNative.TextInput | null>>([]);
  const codeEntryActive = presentation.kind === "code_entry";
  useEffect(() => {
    if (codeEntryActive) return;
    setCodeCells(Array.from({ length: CODE_LENGTH }, () => ""));
  }, [codeEntryActive]);

  const focusCodeCell = (index: number): void => {
    const target = codeCellRefs.current[Math.max(0, Math.min(index, CODE_LENGTH - 1))];
    target?.focus();
  };

  const changeCodeCell = (index: number, text: string): void => {
    const characters = codeCharactersOf(text);
    if (characters === null) return;
    setCodeCells((current) => codeCellsAfterEdit(current, index, characters));
    if (characters.length > 1) {
      // 多字符从被编辑的格开始写入：焦点落在写完的下一格（整码粘贴时是最后一格）。
      const start = characters.length >= CODE_LENGTH ? 0 : index;
      focusCodeCell(start + characters.length);
      return;
    }
    focusCodeCell(characters.length === 0 ? index - 1 : index + 1);
  };

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
    if (presentation.kind === "code_entry") {
      const origin = model.origin ?? "未知来源";
      return (
        <>
          <Text role="heading" style={styles.title}>
            {presentation.title}
          </Text>
          <Text style={styles.lede}>{presentation.lede}</Text>
          {/* S0 的公开事实：将配对的 canonical origin 与安全上下文（HTTPS）。 */}
          <View style={styles.codeOriginCard} dataSet={{ pairingCodeOrigin: origin }}>
            <Text style={styles.cardKey}>将与此电脑配对</Text>
            <Text style={styles.cardValue} dataSet={{ pairingCodeOriginValue: origin }}>
              {origin}
            </Text>
            {model.secureContext ? (
              <Text style={styles.secureBadge} dataSet={{ pairingSecure: "true" }}>
                HTTPS
              </Text>
            ) : (
              <Text style={styles.insecureBadge} dataSet={{ pairingSecure: "false" }}>
                非 HTTPS
              </Text>
            )}
          </View>
          {/* 8 个单字符格：码只在组件内存里，提交后立即随阶段切换清空。 */}
          <View style={styles.codeCells} dataSet={{ pairingCodeEntry: "true" }}>
            {codeCells.map((cell, index) => (
              <TextInput
                // 8 个格的位置是固定的，序号即键。
                key={index}
                value={cell}
                onChangeText={(text) => {
                  changeCodeCell(index, text);
                }}
                autoCapitalize="characters"
                autoCorrect={false}
                autoComplete="off"
                // 聚焦即选中已有字符：直接输入一位会替换该格，而不是在其后追加成两位。
                selectTextOnFocus={true}
                accessibilityLabel={`配对码第 ${index + 1} 位`}
                style={styles.codeCell}
                dataSet={{ pairingCodeCell: String(index) }}
                ref={(node) => {
                  codeCellRefs.current[index] = node;
                }}
              />
            ))}
          </View>
          {/* 静态有效期提示：lookup 之前不显示数字倒计时（design D6）。 */}
          {presentation.note === null ? null : (
            <Text style={styles.note} dataSet={{ pairingCodeHint: "true" }}>
              {presentation.note}
            </Text>
          )}
          <Pressable
            role="button"
            style={styles.primary}
            dataSet={{ action: "submit-pairing-code" }}
            onPress={() => {
              onSubmitCode?.(codeCells.join(""));
            }}
          >
            <Text style={styles.primaryText}>继续配对</Text>
          </Pressable>
        </>
      );
    }

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
        // 「配对页已渲染」的锚点：e2e/外壳用例以它定位本页。
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
