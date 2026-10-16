/**
 * 配对面板（首次配对 + 重新配对入口）。
 *
 * 页面**没有**任何凭据、MCP 配置或路径输入：设备私钥由 `src/platform/` 生成且不可导出，
 * 这些配置只能在电脑上完成。本组件只呈现配对状态机的结果，并提供唯一的输入入口——
 * 粘贴一段二维码内容（design D3 的取舍：v1 用粘贴而不是扫码，不需要摄像头也不必新增解码依赖）。
 *
 * ## 组件不判定、不解析
 *
 * 输入框只把**文本**交给 `onSubmit`（页面转给 `ClientStore`），结构校验、claim、轮询与落盘
 * 都在 store 与组合根里。提交后立刻清空输入框：粘进来的内容含一次性的配对秘密，
 * 不留在组件状态里，也不出现在呈现上。
 *
 * ## 迁移到样式基座（`design.md` D1/D7）
 *
 * 形态取自原型的配对页舞台：标题、引导语、录入行、主机信息与终态说明各占一块。
 * **字段与 `data-*` 锚点一个都没有变**（`data-route`、`data-pairing-phase`、`data-pairing-guidance`、
 * `data-pairing-entry`、`data-pairing-input`、`data-action="submit-pairing-qr"`、`data-pairing-host`、
 * `data-host-address`、`data-pairing-awaiting-desktop`、`data-pairing-device-id`、
 * `data-pairing-error`、`data-action="restart-pairing"`），取值也原样。
 *
 * 可点控件一律 `role="button"`（RNW 据此渲染 `<button>`，`disabled` 也随之成为原生属性）。
 */

import { useState } from "react";
import type { ReactElement } from "react";

import type { PairingPageModel } from "../features/pairing-model";
import { isPairingInFlight } from "../features/pairing-model";
import type { Palette, TextInputProps } from "./theme";
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

export interface PairingPanelProps {
  readonly model: PairingPageModel;
  /** 提交一段二维码内容（地址 / 片段 / 载荷）。 */
  readonly onSubmit: (text: string) => void;
  readonly onRestart: () => void;
}

/**
 * 配对输入框的类型面。
 *
 * 配对进行中要**真的**禁用输入（这一刻内存里持有一份含配对秘密的载荷，不再接受第二次提交），
 * 而 RNW 的 `TextInput` 正是把 `disabled` 原样交给宿主 `<input>`（浏览器级用例按原生
 * `disabled` 属性判定它）——但 `react-native` 的公开类型没有这一项。这与 `theme.ts` 为
 * `dataSet` 补声明是同一类取舍：本组件只用到这一处，因此把断言收敛在这里，而不是从组件
 * 文件去改全局类型。
 */
type PairingInputProps = TextInputProps & { readonly disabled?: boolean };
const PairingInput = TextInput as unknown as (props: PairingInputProps) => ReactElement;

/** 按调色板建样式表：浅/深各建一次（模块加载时）。 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    panel: {
      alignSelf: "center",
      width: "100%",
      maxWidth: 560,
      gap: spacing.md,
      padding: spacing.xl,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.md,
      backgroundColor: colors.panel,
      boxShadow: colors.shadow,
    },
    title: {
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xl,
      fontWeight: fontWeight.medium,
    },
    guidance: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
    },
    entry: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      gap: spacing.sm,
    },
    input: {
      flexGrow: 1,
      flexShrink: 1,
      minWidth: 240,
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.md,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
    },
    submit: {
      flexShrink: 0,
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.lg,
      borderWidth: 1,
      borderColor: colors.accent,
      borderRadius: radius.sm,
      backgroundColor: colors.accentSoft,
    },
    submitDisabled: {
      borderColor: colors.line,
      backgroundColor: colors.panel2,
    },
    submitText: {
      color: colors.accentInk,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      fontWeight: fontWeight.medium,
    },
    submitTextDisabled: {
      color: colors.ink3,
    },
    host: {
      gap: spacing.xs,
      padding: spacing.md,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
    },
    hostRow: {
      flexDirection: "row",
      alignItems: "baseline",
      gap: spacing.md,
    },
    hostKey: {
      minWidth: 74,
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    hostValue: {
      flexShrink: 1,
      color: colors.ink2,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.sm,
      fontWeight: fontWeight.value,
    },
    awaiting: {
      color: colors.warn,
      backgroundColor: colors.warnSoft,
      borderRadius: radius.sm,
      padding: spacing.md,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    deviceId: {
      color: colors.ink3,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
    },
    error: {
      color: colors.danger,
      backgroundColor: colors.dangerSoft,
      borderWidth: 1,
      borderColor: colors.danger,
      borderRadius: radius.sm,
      padding: spacing.md,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    restart: {
      alignSelf: "flex-start",
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.md,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
    },
    restartText: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

/** 配对面板。 */
export function PairingPanel({ model, onSubmit, onRestart }: PairingPanelProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const [pasted, setPasted] = useState("");
  // 进行中（内容已读到、还没到终态）：这一刻仓库里持有一份含 `pairingSecret` 的载荷，
  // 因此不再接受第二次提交，输入也随之禁用——避免两条并行的 claim 与轮询。
  const inFlight = isPairingInFlight(model);
  const canSubmit = !inFlight && pasted.trim().length > 0;

  return (
    <View
      role="region"
      aria-label="与这台电脑配对"
      style={styles.panel}
      dataSet={{ route: "pair", pairingPhase: model.phase }}
    >
      <Text role="heading" style={styles.title}>
        与这台电脑配对
      </Text>
      <Text style={styles.guidance} dataSet={{ pairingGuidance: "true" }}>
        {model.guidance}
      </Text>

      <View style={styles.entry} dataSet={{ pairingEntry: "true" }}>
        <PairingInput
          value={pasted}
          disabled={inFlight}
          placeholder="粘贴二维码内容"
          aria-label="配对二维码内容"
          style={styles.input}
          dataSet={{ pairingInput: "true" }}
          onChangeText={(text: string) => {
            setPasted(text);
          }}
        />
        <Pressable
          role="button"
          disabled={!canSubmit}
          style={canSubmit ? styles.submit : [styles.submit, styles.submitDisabled]}
          dataSet={{ action: "submit-pairing-qr" }}
          onPress={() => {
            onSubmit(pasted.trim());
            setPasted("");
          }}
        >
          <Text style={canSubmit ? styles.submitText : [styles.submitText, styles.submitTextDisabled]}>
            开始配对
          </Text>
        </Pressable>
      </View>

      {model.host === null ? null : (
        <View style={styles.host} dataSet={{ pairingHost: "true" }}>
          <View style={styles.hostRow}>
            <Text style={styles.hostKey}>地址</Text>
            <Text style={styles.hostValue} dataSet={{ hostAddress: model.host.address }}>
              {model.host.address}
            </Text>
          </View>
          <View style={styles.hostRow}>
            <Text style={styles.hostKey}>主机 ID</Text>
            <Text style={styles.hostValue}>{model.host.hostId ?? "未知"}</Text>
          </View>
        </View>
      )}

      {model.awaitingDesktopConfirmation ? (
        <Text style={styles.awaiting} dataSet={{ pairingAwaitingDesktop: "true" }}>
          请到电脑上确认这次配对请求。
        </Text>
      ) : null}

      {model.deviceId === null ? null : (
        <Text style={styles.deviceId} dataSet={{ pairingDeviceId: model.deviceId }}>
          本机设备 ID：{model.deviceId}
        </Text>
      )}

      {model.error === null ? null : (
        <Text role="alert" style={styles.error} dataSet={{ pairingError: model.error.code }}>
          {model.error.message}
        </Text>
      )}

      {model.canRestart ? (
        <Pressable
          role="button"
          style={styles.restart}
          dataSet={{ action: "restart-pairing" }}
          onPress={onRestart}
        >
          <Text style={styles.restartText}>重新扫码</Text>
        </Pressable>
      ) : null}
    </View>
  );
}
