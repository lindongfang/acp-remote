/**
 * 可填写的提问表单（`design.md` D6；spec「授权与提问应答的可用边界」）。
 *
 * ## 「不可表达」不是「空答案」
 *
 * 字段构造与校验全在 `domain/form.ts`：文本、整数/数值、布尔、单选、字符串数组多选；必填与
 * 长度/范围校验；**任何必需字段**的类型不在子集里时，`buildElicitationForm` 直接给出
 * `unavailable`，本组件因此**不渲染提交按钮**——绝不把空对象/`null` 当成答案发出去。
 * 拒绝与取消仍可用（它们本来就是合法动作）。
 *
 * ## 精确整数
 *
 * 整数控件的值是原始文本；`validateElicitationForm` 把它包成 `JsonNumberLiteral`，序列化时写成
 * 裸 JSON 数字。`9007199254740993` 不会先经过 `number` 再丢精度。
 *
 * ## 校验发生在提交时
 *
 * 用户边填边改时不报错（避免刚输入一个字符就红）；点击提交时逐字段校验，错误就地显示。
 * 服务端仍会再做一次校验，因此这里的通过不等于服务端一定接受（本组件不冒充那个保证）。
 */

import { useState } from "react";
import type { ReactElement } from "react";

import type { ElicitationField, ElicitationForm as FormModel, FormInputValue } from "../domain/form";
import { validateElicitationForm } from "../domain/form";
import type { Palette } from "./theme";
import { Pressable, StyleSheet, Text, TextInput, View, fontFamily, fontSize, palettes, radius, spacing, useTheme } from "./theme";

export interface ElicitationFormProps {
  /** 由当前有效请求的 schema 构造（`buildElicitationForm`）。 */
  readonly form: FormModel;
  /** 提交已通过客户端校验的值；缺省时提交按钮不可用。 */
  readonly onSubmit?: ((values: Readonly<Record<string, FormInputValue>>) => void) | undefined;
  /** 拒绝这张表单（仍可表达的动作，不受字段支持度影响）。 */
  readonly onDecline?: (() => void) | undefined;
  /** 取消这次提问。 */
  readonly onCancel?: (() => void) | undefined;
  /** 外部不可提交的原因（例如交互已过期）；与表单自身错误一起呈现。 */
  readonly disabledReason?: string | null;
}

/** 按调色板建样式表：浅/深各建一次（模块加载时）。 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    root: {
      gap: spacing.sm,
      paddingTop: spacing.xs,
    },
    field: {
      gap: 2,
    },
    label: {
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      fontWeight: "600",
    },
    description: {
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    input: {
      minHeight: 32,
      paddingVertical: 4,
      paddingHorizontal: spacing.sm,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel,
      color: colors.ink,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.sm,
    },
    choices: {
      flexDirection: "row",
      flexWrap: "wrap",
      gap: spacing.xs,
    },
    choice: {
      paddingVertical: 2,
      paddingHorizontal: spacing.sm,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel,
    },
    choiceSelected: {
      borderColor: colors.accent,
      backgroundColor: colors.accentSoft,
    },
    choiceText: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    choiceTextSelected: {
      color: colors.accentInk,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
      fontWeight: "600",
    },
    error: {
      color: colors.danger,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    unavailable: {
      color: colors.warn,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    actions: {
      flexDirection: "row",
      flexWrap: "wrap",
      gap: spacing.sm,
    },
    primary: {
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.md,
      borderWidth: 1,
      borderColor: colors.accent,
      borderRadius: radius.sm,
      backgroundColor: colors.accent,
    },
    primaryText: {
      color: colors.panel,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      fontWeight: "600",
    },
    secondary: {
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.md,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel,
    },
    secondaryText: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    disabled: {
      borderColor: colors.line,
      backgroundColor: colors.panel2,
    },
    disabledText: {
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

/** 一个字段的标签行（必填标记与说明）。 */
function FieldLabel({ field }: { readonly field: ElicitationField }): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  return (
    <View style={styles.field}>
      <Text style={styles.label} dataSet={{ formFieldLabel: field.name, formFieldRequired: String(field.required) }}>
        {field.required ? `${field.label} *` : field.label}
      </Text>
      {field.description === null ? null : <Text style={styles.description}>{field.description}</Text>}
    </View>
  );
}

/** 单选/多选的选项按钮。 */
function ChoiceButtons({
  field,
  selected,
  multiple,
  onToggle,
}: {
  readonly field: Extract<ElicitationField, { kind: "enum" | "multi_enum" }>;
  readonly selected: readonly string[];
  readonly multiple: boolean;
  readonly onToggle: (value: string) => void;
}): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  return (
    <View style={styles.choices}>
      {field.choices.map((choice) => {
        const active = selected.includes(choice.value);
        return (
          <Pressable
            key={choice.value}
            role={multiple ? "checkbox" : "radio"}
            accessibilityState={multiple ? { checked: active } : { selected: active }}
            style={active ? [styles.choice, styles.choiceSelected] : styles.choice}
            dataSet={{
              formChoice: choice.value,
              formChoiceField: field.name,
              formChoiceSelected: String(active),
            }}
            onPress={() => {
              onToggle(choice.value);
            }}
          >
            <Text style={active ? styles.choiceTextSelected : styles.choiceText}>{choice.label}</Text>
          </Pressable>
        );
      })}
    </View>
  );
}

/** 提问表单。 */
export function ElicitationForm({
  form,
  onSubmit,
  onDecline,
  onCancel,
  disabledReason,
}: ElicitationFormProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const [values, setValues] = useState<Readonly<Record<string, FormInputValue>>>(
    () => (form.kind === "form" ? form.initial : {}),
  );
  const [errors, setErrors] = useState<Readonly<Record<string, string>>>({});
  const [detail, setDetail] = useState<string | null>(null);

  if (form.kind === "unavailable") {
    return (
      <View style={styles.root} dataSet={{ elicitationForm: "unavailable", formUnavailableReason: form.reason }}>
        <Text style={styles.unavailable} dataSet={{ formUnavailableDetail: "true" }}>
          {form.detail}
        </Text>
        <Text style={styles.description}>仍可拒绝或取消这次提问；不会以空答案代替填写。</Text>
        <View style={styles.actions}>
          {onDecline === undefined ? null : (
            <Pressable
              role="button"
              style={styles.secondary}
              dataSet={{ action: "decline-elicitation" }}
              onPress={onDecline}
            >
              <Text style={styles.secondaryText}>拒绝</Text>
            </Pressable>
          )}
          {onCancel === undefined ? null : (
            <Pressable
              role="button"
              style={styles.secondary}
              dataSet={{ action: "cancel-elicitation" }}
              onPress={onCancel}
            >
              <Text style={styles.secondaryText}>取消</Text>
            </Pressable>
          )}
        </View>
      </View>
    );
  }

  const update = (name: string, value: FormInputValue): void => {
    setValues((current) => ({ ...current, [name]: value }));
    setErrors((current) => {
      if (!Object.hasOwn(current, name)) return current;
      const next = { ...current };
      delete next[name];
      return next;
    });
  };

  const submit = (): void => {
    if (onSubmit === undefined) return;
    const validated = validateElicitationForm(form, values);
    if (!validated.ok) {
      setErrors(validated.errors);
      setDetail(validated.detail);
      return;
    }
    setErrors({});
    setDetail(null);
    onSubmit(values);
  };

  return (
    <View style={styles.root} dataSet={{ elicitationForm: "supported", formFieldCount: String(form.fields.length) }}>
      {form.unsupportedOptional.length === 0 ? null : (
        <Text style={styles.description} dataSet={{ formUnsupportedOptional: form.unsupportedOptional.join(",") }}>
          {`这些可选字段的类型当前不支持，不会被收集：${form.unsupportedOptional.join("、")}`}
        </Text>
      )}

      {form.fields.map((field) => {
        const error = errors[field.name];
        return (
          <View key={field.name} style={styles.field} dataSet={{ formField: field.name, formFieldKind: field.kind }}>
            <FieldLabel field={field} />
            {field.kind === "boolean" ? (
              <Pressable
                role="switch"
                accessibilityState={{ checked: values[field.name] === true }}
                style={values[field.name] === true ? [styles.choice, styles.choiceSelected] : styles.choice}
                dataSet={{ formBoolean: String(values[field.name] === true), formBooleanField: field.name }}
                onPress={() => {
                  update(field.name, values[field.name] !== true);
                }}
              >
                <Text style={values[field.name] === true ? styles.choiceTextSelected : styles.choiceText}>
                  {values[field.name] === true ? "是" : "否"}
                </Text>
              </Pressable>
            ) : field.kind === "enum" ? (
              <ChoiceButtons
                field={field}
                selected={typeof values[field.name] === "string" ? [values[field.name] as string] : []}
                multiple={false}
                onToggle={(value) => {
                  update(field.name, value);
                }}
              />
            ) : field.kind === "multi_enum" ? (
              <ChoiceButtons
                field={field}
                selected={Array.isArray(values[field.name]) ? (values[field.name] as readonly string[]) : []}
                multiple
                onToggle={(value) => {
                  const current = Array.isArray(values[field.name]) ? (values[field.name] as readonly string[]) : [];
                  update(
                    field.name,
                    current.includes(value) ? current.filter((item) => item !== value) : [...current, value],
                  );
                }}
              />
            ) : (
              <TextInput
                value={typeof values[field.name] === "string" ? (values[field.name] as string) : ""}
                multiline={false}
                placeholder={field.kind === "text" ? "" : field.kind === "integer" ? "整数" : "数值"}
                style={styles.input}
                dataSet={{ formInput: field.name, formInputKind: field.kind }}
                onChangeText={(text: string) => {
                  update(field.name, text);
                }}
              />
            )}
            {error === undefined ? null : (
              <Text style={styles.error} dataSet={{ formError: field.name }}>
                {error}
              </Text>
            )}
          </View>
        );
      })}

      {detail === null ? null : (
        <Text style={styles.error} dataSet={{ formErrorDetail: "true" }}>
          {detail}
        </Text>
      )}
      {disabledReason === null || disabledReason === undefined ? null : (
        <Text style={styles.error} dataSet={{ formDisabledReason: "true" }}>
          {disabledReason}
        </Text>
      )}

      <View style={styles.actions}>
        <Pressable
          role="button"
          disabled={onSubmit === undefined}
          style={onSubmit === undefined ? [styles.primary, styles.disabled] : styles.primary}
          dataSet={{ action: "submit-elicitation-form" }}
          onPress={submit}
        >
          <Text style={onSubmit === undefined ? styles.disabledText : styles.primaryText}>提交</Text>
        </Pressable>
        {onDecline === undefined ? null : (
          <Pressable
            role="button"
            style={styles.secondary}
            dataSet={{ action: "decline-elicitation" }}
            onPress={onDecline}
          >
            <Text style={styles.secondaryText}>拒绝</Text>
          </Pressable>
        )}
        {onCancel === undefined ? null : (
          <Pressable
            role="button"
            style={styles.secondary}
            dataSet={{ action: "cancel-elicitation" }}
            onPress={onCancel}
          >
            <Text style={styles.secondaryText}>取消</Text>
          </Pressable>
        )}
      </View>
    </View>
  );
}
