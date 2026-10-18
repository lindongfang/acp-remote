/**
 * 翻页控件（目录页与目录详情页共用）。
 *
 * 固定渲染：只有一页时两个按钮都是禁用态，页码输入仍显示 `1 / 1`。页码输入是「草稿 + 提交」
 * 两段式——输入期间只改本地草稿，回车/失焦才把原文上抛；纠正与切片全在 feature 层。
 *
 * `data` 由消费方给出属性名：目录页与详情页各自需要可区分的 e2e 锚点，组件本身不关心
 * 自己长在哪一页。
 */

import { useState } from "react";
import type { ReactElement } from "react";

import type { Palette } from "./theme";
import {
  Pressable,
  StyleSheet,
  Text,
  TextInput,
  View,
  fontFamily,
  palettes,
  radius,
  spacing,
  useTheme,
} from "./theme";

/** 翻页控件在 DOM 上的属性名（`dataSet` 的键）。 */
export interface PagerDataKeys {
  /** 翻页区根节点，值为 `"true"`。 */
  readonly pager: string;
  /** 当前页。 */
  readonly page: string;
  /** 总页数。 */
  readonly pageCount: string;
  /** 页码输入框，值为 `"true"`。 */
  readonly input: string;
  /** 模型给出的生效页码。 */
  readonly inputValue: string;
}

export interface PagerProps {
  readonly page: number;
  readonly pageCount: number;
  /** 提交页码原文（只在回车与失焦时调用）；纠正由调用方完成。 */
  readonly onPageChange: (requested: string) => void;
  readonly data: PagerDataKeys;
}

function buildStyles(colors: Palette) {
  return StyleSheet.create({
    pager: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      gap: spacing.sm,
    },
    pagerButton: {
      // 原型 `.btn.sm`：5px 11px 内边距、12.5px 字。
      paddingVertical: 5,
      paddingHorizontal: 11,
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
      fontSize: 12.5,
      lineHeight: 20,
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
      fontSize: 12.5,
      lineHeight: 20,
    },
    pageInput: {
      width: 40,
      minHeight: 28,
      // 原型 `.pageinput`：4px 6px 内边距、40px 宽、12.5px 字。
      paddingVertical: spacing.xs,
      paddingHorizontal: 6,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: 12.5,
      lineHeight: 20,
      textAlign: "center",
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

/** 固定渲染的翻页控件。 */
export function Pager({ page, pageCount, onPageChange, data }: PagerProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  // `null` = 跟随模型；非 `null` = 用户正在编辑的原文。
  const [draft, setDraft] = useState<string | null>(null);

  const commit = (): void => {
    const requested = draft;
    setDraft(null);
    if (requested === null) return;
    onPageChange(requested);
  };

  return (
    <View
      style={styles.pager}
      dataSet={{
        [data.pager]: "true",
        [data.page]: String(page),
        [data.pageCount]: String(pageCount),
      }}
    >
      <Pressable
        role="button"
        style={page > 1 ? styles.pagerButton : [styles.pagerButton, styles.pagerButtonDisabled]}
        disabled={page <= 1}
        dataSet={{ action: "page-prev" }}
        onPress={() => {
          onPageChange(String(page - 1));
        }}
      >
        <Text style={page > 1 ? styles.pagerButtonText : [styles.pagerButtonText, styles.pagerButtonTextDisabled]}>
          ‹ 上一页
        </Text>
      </Pressable>
      <View style={styles.pagerInfo}>
        <Text style={styles.pagerInfoText}>第</Text>
        <TextInput
          style={styles.pageInput}
          value={draft ?? String(page)}
          accessibilityLabel="页码（回车跳转）"
          // 输入只改草稿：输入期间不写仓库，所以列表不重画；纠正发生在提交之后。
          onChangeText={(text) => {
            setDraft(text);
          }}
          onSubmitEditing={commit}
          onBlur={commit}
          dataSet={{ [data.input]: "true", [data.inputValue]: String(page) }}
        />
        <Text style={styles.pagerInfoText}>{`/ ${String(pageCount)} 页`}</Text>
      </View>
      <Pressable
        role="button"
        style={page < pageCount ? styles.pagerButton : [styles.pagerButton, styles.pagerButtonDisabled]}
        disabled={page >= pageCount}
        dataSet={{ action: "page-next" }}
        onPress={() => {
          onPageChange(String(page + 1));
        }}
      >
        <Text style={page < pageCount ? styles.pagerButtonText : [styles.pagerButtonText, styles.pagerButtonTextDisabled]}>
          下一页 ›
        </Text>
      </Pressable>
    </View>
  );
}
