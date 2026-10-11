/**
 * 主机与连接面板（R19）。
 *
 * ## 两条 MUST 的渲染落点
 *
 * - **Agent 列表来自快照**：本组件渲染的 `model.agents` 来自 `HostPanelModel`，
 *   而后者只由「快照 agents + 事件覆盖层」构造。断连时列表照常渲染并标注来源。
 * - **覆盖层缺席即状态未知**：每行都渲染 `data-agent-connection`，缺席时是
 *   `unknown`（文案「状态未知」），并且面板给出缺席原因；MUST NOT 渲染成「已断开」，
 *   MUST NOT 因为缺席把行删掉。
 *
 * 同名展示名以标识区分：每行都渲染 `agent.id`，因此两个同名 Agent 是两行而不是一行。
 *
 * 迁移到 RNW 原语（`design.md` D1）：`View`/`Text` + `StyleSheet`。列表语义用 `role` 补回
 * （容器 `role="list"`、行 `role="listitem"`，RNW 据此渲染 `<ul>`/`<li>`）；所有 `data-*`
 * 属性名与取值一律原样经 `dataSet` 进 DOM——它们是 e2e 唯一的机器证据。
 */

import type { ReactElement } from "react";

import type { HostPanelModel } from "../features/host-panel-model";
import type { Palette } from "./theme";
import { StyleSheet, Text, View, fontFamily, fontSize, palettes, radius, spacing, useTheme } from "./theme";

export interface HostConnectionPanelProps {
  readonly model: HostPanelModel;
}

/** 按调色板建样式表：浅/深各建一次（模块加载时）。 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    panel: {
      gap: spacing.md,
      backgroundColor: colors.panel,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.md,
      padding: spacing.lg,
      boxShadow: colors.shadow,
    },
    head: {
      gap: spacing.xs,
      paddingBottom: spacing.sm,
      borderBottomWidth: 1,
      borderBottomColor: colors.line,
    },
    connection: {
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.lg,
      fontWeight: "600",
    },
    note: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    blocking: {
      gap: spacing.xs,
      padding: spacing.md,
      borderWidth: 1,
      borderColor: colors.danger,
      borderRadius: radius.sm,
      backgroundColor: colors.dangerSoft,
    },
    blockingText: {
      color: colors.danger,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    agentList: {
      gap: spacing.xs,
    },
    agentRow: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "baseline",
      gap: spacing.sm,
      paddingVertical: spacing.sm,
      borderBottomWidth: 1,
      borderBottomColor: colors.line2,
    },
    agentName: {
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
    },
    agentId: {
      color: colors.ink3,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
    },
    agentDefault: {
      paddingHorizontal: spacing.xs,
      borderRadius: radius.sm,
      backgroundColor: colors.accentSoft,
      color: colors.accentInk,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    agentConnection: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    agentReason: {
      paddingHorizontal: spacing.xs,
      borderRadius: radius.sm,
      backgroundColor: colors.warnSoft,
      color: colors.warn,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    identity: {
      gap: spacing.xs,
    },
    identityRow: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "baseline",
      gap: spacing.sm,
    },
    identityLabel: {
      minWidth: 88,
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    identityValue: {
      color: colors.ink,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.sm,
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

/** 主机身份的一行：标签 + 取值。 */
function IdentityRow({ label, value }: { readonly label: string; readonly value: string }): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

  return (
    <View style={styles.identityRow}>
      <Text style={styles.identityLabel}>{label}</Text>
      <Text style={styles.identityValue}>{value}</Text>
    </View>
  );
}

/** Agent 行。 */
function AgentRow({ agent }: { readonly agent: HostPanelModel["agents"][number] }): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

  return (
    // `role="listitem"` → RNW 渲染 `<li>`，与父容器的 `role="list"` 配成一份合法列表。
    <View
      role="listitem"
      style={styles.agentRow}
      dataSet={{ agentRow: agent.id, agentConnection: agent.connection }}
    >
      <Text style={styles.agentName} dataSet={{ agentDisplayName: agent.displayName }}>
        {agent.displayName}
      </Text>
      {/* 同名 Agent 以标识区分：id 始终呈现。 */}
      <Text style={styles.agentId} dataSet={{ agentIdLabel: "true" }}>
        {agent.id}
      </Text>
      {agent.isDefault ? (
        <Text style={styles.agentDefault} dataSet={{ agentDefault: "true" }}>
          默认
        </Text>
      ) : null}
      <Text style={styles.agentConnection} dataSet={{ agentConnectionLabel: agent.connection }}>
        {agent.connectionLabel}
      </Text>
      {agent.disconnectReason === null ? null : (
        <Text style={styles.agentReason} dataSet={{ agentDisconnectReason: "true" }}>
          {agent.disconnectReason}
        </Text>
      )}
    </View>
  );
}

/** 主机与连接面板。 */
export function HostConnectionPanel({ model }: HostConnectionPanelProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

  return (
    <View style={styles.panel} dataSet={{ route: "hosts" }}>
      <View style={styles.head} dataSet={{ connectionState: model.connectionState }}>
        <Text style={styles.connection} dataSet={{ connectionLabel: "true" }}>
          {model.connectionLabel}
        </Text>
        {model.offlineSnapshotNote === null ? null : (
          <Text style={styles.note} dataSet={{ offlineSnapshotNote: "true" }}>
            {model.offlineSnapshotNote}
          </Text>
        )}
        {model.blocking === null ? null : (
          <View style={styles.blocking} role="alert" dataSet={{ blockingDetail: model.blocking.cause }}>
            <Text style={styles.blockingText}>{model.blocking.reason}</Text>
            <Text style={styles.blockingText}>{model.blocking.nextStep}</Text>
          </View>
        )}
      </View>

      {model.unknownStateNote === null ? null : (
        <Text style={styles.note} dataSet={{ unknownStateNote: "true" }}>
          {model.unknownStateNote}
        </Text>
      )}

      <View role="list" style={styles.agentList} dataSet={{ agentList: "configured" }}>
        {model.agents.map((agent) => (
          <AgentRow key={agent.id} agent={agent} />
        ))}
      </View>

      {model.host === null ? (
        <Text style={styles.note} dataSet={{ hostIdentity: "unpaired" }}>
          尚未配对，没有可显示的主机身份信息。
        </Text>
      ) : (
        <View style={styles.identity} dataSet={{ hostIdentity: "paired" }}>
          {/* 地址行单独写：它要带 `data-host-address`（e2e 从这里取精确地址）。 */}
          <View style={styles.identityRow}>
            <Text style={styles.identityLabel}>地址</Text>
            <Text style={styles.identityValue} dataSet={{ hostAddress: model.host.address }}>
              {model.host.address}
            </Text>
          </View>
          <IdentityRow label="主机 ID" value={model.host.hostId ?? "未知"} />
          <IdentityRow label="本机设备 ID" value={model.host.deviceId ?? "未知"} />
        </View>
      )}
    </View>
  );
}
