import { useState } from "react";
import type { activeConversationTodo } from "../features/conversation-todo";
import { Pressable, ScrollView, Text, View, fontSize, radius, spacing, useTheme } from "./theme";

export const TODO_STATUS_LABELS = { pending: "待开始", in_progress: "进行中", completed: "已完成" } as const;

/** 展开是会话内的视图偏好；事件更新只替换清单，不重置用户的展开选择。 */
export function ConversationTodo({ todo }: { readonly todo: NonNullable<ReturnType<typeof activeConversationTodo>> }) {
  const [expanded, setExpanded] = useState(false);
  const { colors } = useTheme();
  const current = todo.entries.find((item) => item.status === "in_progress");
  return <View dataSet={{ composerTodo: "true" }} style={{ marginHorizontal: spacing.md, marginTop: spacing.sm,
    borderWidth: 1, borderColor: colors.line, borderRadius: radius.md, backgroundColor: colors.panel, overflow: "hidden" }}>
    <Pressable role="button" accessibilityLabel="任务清单" accessibilityState={{ expanded }} aria-expanded={expanded}
      onPress={() => setExpanded(!expanded)} dataSet={{ action: "toggle-composer-todo" }}
      style={{ minHeight: 44, padding: spacing.sm, flexDirection: "row", alignItems: "center", gap: spacing.sm }}>
      <Text style={{ color: colors.ink3 }}>{expanded ? "▾" : "▸"}</Text>
      <Text style={{ color: colors.ink, fontSize: fontSize.sm, fontWeight: "600" }}>任务清单</Text>
      <Text style={{ color: colors.accentInk, fontSize: fontSize.xs }}>{todo.completed}/{todo.entries.length}</Text>
      <Text numberOfLines={1} style={{ flex: 1, color: colors.ink2, fontSize: fontSize.xs }}>{current?.content ?? (todo.completed === 0 ? "尚未开始" : "还有任务未完成")}</Text>
    </Pressable>
    <View accessibilityRole="progressbar" accessibilityLabel="任务完成进度"
      accessibilityValue={{ min: 0, max: todo.entries.length, now: todo.completed }}
      style={{ height: 3, backgroundColor: colors.line2 }}>
      <View style={{ height: 3, width: `${todo.completed / todo.entries.length * 100}%`, backgroundColor: colors.accent }} />
    </View>
    {expanded ? <ScrollView style={{ maxHeight: 200 }}>
      <View role="list" style={{ padding: spacing.sm, gap: spacing.sm }}>
        {todo.entries.map((item, index) => <View key={index} role="listitem" dataSet={{ todoStatus: item.status }}
          style={{ flexDirection: "row", alignItems: "flex-start", gap: spacing.sm }}>
          <Text style={{ color: item.status === "completed" ? colors.ok : item.status === "in_progress" ? colors.accent : colors.ink3, fontSize: fontSize.xs }}>
            {TODO_STATUS_LABELS[item.status]}
          </Text>
          <Text style={{ flex: 1, color: item.status === "completed" ? colors.ink3 : colors.ink,
            fontSize: fontSize.sm, lineHeight: 22, textDecorationLine: item.status === "completed" ? "line-through" : "none" }}>{item.content}</Text>
        </View>)}
      </View>
    </ScrollView> : null}
  </View>;
}
