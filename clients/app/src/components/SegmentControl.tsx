/**
 * 分段控件（原型的 `.seg`）：目录页的会话状态筛选与目录详情页的月份档共用同一个原语。
 *
 * 组件不判定「哪一项该出现」——选项由 feature 层的视图模型给出，这里只负责把当前值与
 * 点击上抛（D3：判定在 feature 层，页面只渲染）。
 *
 * `label` 同时是本组的**可访问名**与机器标识：RNW 把它渲染成容器的 `aria-label`，同时经
 * `dataSet` 渲染成 `data-segment`——e2e 用 `[data-segment="…"]` 区分同页面上的多组分段
 * （筛选与月份都可能有 `all` 这一项，只看选项取值无法区分是哪一组）。原型的分段控件没有
 * 组标题，因此这里不渲染可见的组名。
 */

import type { ReactElement } from "react";

import type { Palette } from "./theme";
import { Pressable, StyleSheet, Text, View, fontFamily, fontSize, palettes, radius, spacing, useTheme } from "./theme";

export interface SegmentControlProps<T extends string = string> {
  readonly label: string;
  readonly value: T;
  readonly options: readonly { readonly value: T; readonly label: string }[];
  readonly onChange: (value: T) => void;
}

/** 按调色板建样式表：浅/深各建一次（模块加载时）。 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    group: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignSelf: "flex-start",
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel,
    },
    option: {
      paddingVertical: 6,
      paddingHorizontal: spacing.md,
      backgroundColor: "transparent",
    },
    optionOn: {
      paddingVertical: 6,
      paddingHorizontal: spacing.md,
      backgroundColor: colors.accentSoft,
    },
    label: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    labelOn: {
      color: colors.accentInk,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      fontWeight: "600",
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

/**
 * 一组互斥的分段选项。
 *
 * 取值类型由调用方推断（筛选段是封闭三值 `DirectoryFilter`、月份段是 `string`）：
 * 这样调用方能把 `onChange` 直接接上收窄过的 setter，不必在调用点写一次强制转换。
 */
export function SegmentControl<T extends string = string>({
  label,
  value,
  options,
  onChange,
}: SegmentControlProps<T>): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

  return (
    // `radiogroup`/`radio` 是分段控件对应的 ARIA 形状：互斥、当前项由 `aria-checked` 表达。
    <View role="radiogroup" accessibilityLabel={label} style={styles.group} dataSet={{ segment: label }}>
      {options.map((option) => {
        const selected = option.value === value;
        return (
          <Pressable
            key={option.value}
            role="radio"
            accessibilityState={{ checked: selected }}
            style={selected ? styles.optionOn : styles.option}
            dataSet={{ segmentOption: option.value, segmentSelected: String(selected) }}
            onPress={() => {
              onChange(option.value);
            }}
          >
            <Text style={selected ? styles.labelOn : styles.label}>{option.label}</Text>
          </Pressable>
        );
      })}
    </View>
  );
}
