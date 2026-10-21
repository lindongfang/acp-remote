/**
 * 连接问题弹层（`design.md` D1/D5）。
 *
 * 五种变体（普通离线 + 四个阻断态）共用这一份呈现：标题/说明/动作来自
 * `features/connection-problem.ts` 的同一个模型，`AuthTransition`（首屏认证失败）与
 * `ShellView`（已认证后的连接问题）渲染的是同一个组件，因此文案与动作不会出现第二份副本。
 *
 * 弹层**不可关闭**：没有关闭按钮、没有点遮罩关闭的分支。问题只有三种出路——等待自动重连、
 * 重新配对、重新加载；多一个「关掉但什么也没解决」的出口只会让用户停在半失效的页面里。
 */

import { useState } from "react";
import type { ReactElement } from "react";

import type { ConnectionProblemModel } from "../../features/connection-problem";
import type { Palette } from "../theme";
import { Pressable, StyleSheet, Text, View, fontFamily, fontSize, fontWeight, palettes, radius, spacing, useTheme } from "../theme";

function buildStyles(colors: Palette) {
  return StyleSheet.create({
    card: {
      // 由内容撑开、再用 maxWidth 封顶：边框始终贴着内容，而不是宽屏上固定撑成一条长条。
      width: "auto",
      maxWidth: 560,
      alignSelf: "center",
      flexDirection: "column",
      gap: spacing.md,
      padding: spacing.xl,
      borderWidth: 1,
      borderColor: colors.line,
      // 对话框是卡片而不是胶囊：999px 的 `radius.lg` 会把四角变成药丸形。
      borderRadius: radius.md,
      backgroundColor: colors.panel,
    },
    title: {
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.lg,
      lineHeight: 27.2,
      fontWeight: fontWeight.medium,
    },
    description: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      lineHeight: 20.8,
    },
    codes: {
      flexDirection: "row",
      flexWrap: "wrap",
      gap: spacing.md,
    },
    code: {
      color: colors.ink3,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
      lineHeight: 17.6,
    },
    reconnect: {
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.md,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
      gap: spacing.xs,
    },
    reconnectText: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      lineHeight: 20.8,
    },
    disclosure: {
      flexDirection: "row",
      alignItems: "center",
      gap: spacing.xs,
      paddingVertical: spacing.xs,
    },
    disclosureText: {
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
      lineHeight: 17.6,
    },
    detail: {
      padding: spacing.sm,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
    },
    detailHidden: {
      display: "none",
    },
    detailText: {
      color: colors.ink2,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
      lineHeight: 17.6,
    },
    action: {
      alignSelf: "flex-end",
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.lg,
      borderWidth: 1,
      borderColor: colors.accent,
      borderRadius: radius.sm,
      backgroundColor: colors.accentSoft,
    },
    actionText: {
      color: colors.accentInk,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      lineHeight: 20.8,
      fontWeight: fontWeight.medium,
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

/** 下次重试时刻的钟点（只用于呈现，不参与任何判定）。 */
function retryClock(atMs: number): string {
  const date = new Date(atMs);
  const pad = (value: number): string => String(value).padStart(2, "0");
  return `${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
}

export interface ConnectionProblemDialogProps {
  readonly problem: ConnectionProblemModel;
  readonly onRepair: () => void;
  readonly onReload: () => void;
}

/** 连接问题卡片：标题、说明、码位、技术详情、重连进度与唯一动作。 */
export function ConnectionProblemDialog({
  problem,
  onRepair,
  onReload,
}: ConnectionProblemDialogProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const [detailOpen, setDetailOpen] = useState(false);

  return (
    <View
      role="dialog"
      aria-modal={true}
      aria-label={problem.title}
      style={styles.card}
      dataSet={{ connectionProblem: problem.kind }}
    >
      <Text style={styles.title} dataSet={{ problemTitle: "true" }}>
        {problem.title}
      </Text>
      <Text style={styles.description} dataSet={{ problemDescription: "true" }}>
        {problem.description}
      </Text>

      <View style={styles.codes}>
        {problem.errorCode === null ? null : (
          <Text style={styles.code} dataSet={{ problemErrorCode: problem.errorCode }}>
            错误码：{problem.errorCode}
          </Text>
        )}
        {problem.closeCode === null ? null : (
          <Text style={styles.code} dataSet={{ problemCloseCode: String(problem.closeCode) }}>
            关闭码：{problem.closeCode}
          </Text>
        )}
      </View>

      {problem.reconnect === null ? null : (
        <View style={styles.reconnect} dataSet={{ problemReconnect: "true" }}>
          <Text style={styles.reconnectText} dataSet={{ problemReconnectAttempts: String(problem.reconnect.attempts) }}>
            {problem.reconnect.attempts === 0
              ? "客户端会立即开始重试。"
              : `已重试 ${problem.reconnect.attempts} 次。`}
          </Text>
          <Text style={styles.reconnectText} dataSet={{ problemReconnectNext: "true" }}>
            {problem.reconnect.nextRetryAtMs === null
              ? "正在尝试重新连接…"
              : `下次重试：${retryClock(problem.reconnect.nextRetryAtMs)}`}
          </Text>
        </View>
      )}

      <Pressable
        role="button"
        aria-label={detailOpen ? "收起技术详情" : "展开技术详情"}
        style={styles.disclosure}
        dataSet={{ action: "toggle-problem-detail", problemDetailOpen: String(detailOpen) }}
        onPress={() => {
          setDetailOpen(!detailOpen);
        }}
      >
        <Text style={styles.disclosureText}>{detailOpen ? "▾" : "▸"} 技术详情</Text>
      </Pressable>
      {/* 收起只是 `display: none`：诊断文本始终在 DOM 里，与原型 `details` 的形态一致。 */}
      <View
        style={detailOpen ? styles.detail : [styles.detail, styles.detailHidden]}
        dataSet={{ problemDetail: "true" }}
      >
        <Text style={styles.detailText}>{problem.technicalDetail}</Text>
      </View>

      {problem.action === "repair" ? (
        <Pressable
          role="button"
          style={styles.action}
          dataSet={{ problemAction: "repair" }}
          onPress={onRepair}
        >
          <Text style={styles.actionText}>重新配对</Text>
        </Pressable>
      ) : problem.action === "reload" ? (
        <Pressable
          role="button"
          style={styles.action}
          dataSet={{ problemAction: "reload" }}
          onPress={onReload}
        >
          <Text style={styles.actionText}>重新加载</Text>
        </Pressable>
      ) : null}
    </View>
  );
}
