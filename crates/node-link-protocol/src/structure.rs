//! §2.5 的三个固定 JSON 结构上限（`docs/NODE_LINK_PROTOCOL.md` §2.5 的「JSON nesting depth 64 / 单对象字段数
//! 1,024 / 单数组元素数 10,000」，三项都**不可下调**、也不经 `node.ready.limits` 协商）。
//!
//! **执行点**：wire DTO 边界，即 [`crate::envelope::Envelope::decode`]（加上适配器的错误映射）。§2.4 要求
//! 「长度限制必须在 wire DTO 边界验证，不能延迟到 Agent adapter」；这三项与 `DecimalString`/`Base64Url`
//! 的长度上限同类，因此判定不能留给各家族 body 的类型化解码，更不能等到消费 body 的代码。
//!
//! **为什么不复用 `serde_json` 的上限**：它的递归上限默认 128（与合同的 64 不同值），且对单个对象的字段数
//! 与数组的元素数完全没有上限。本模块因此直接扫描原始文本，判定只依据下面的三个常量。
//!
//! **扫描器只计数、不判语法**：它按 JSON 的分隔符（引号/花括号/方括号/逗号）维护容器栈，不验证语法，
//! 合法 JSON 上的计数才是精确的。畸形输入上的计数没有意义，由同一解码路径上的语法判定负责（§2.4：
//! JSON 畸形 → `nodelink.protocol.invalid_json`）；两个判定的顺序与优先级写在
//! [`crate::envelope::Envelope::decode`] 的文档里。

/// JSON 嵌套深度上限（§2.5 的「JSON nesting depth」）。
///
/// 口径是**整帧文本**的嵌套层数：最外层容器（信封对象）算第 1 层，容器内每再嵌一层加一；对象与数组都计入。
pub const MAX_NESTING_DEPTH: usize = 64;

/// 单个对象的字段数上限（§2.5 的「单对象字段数」）。逐个对象独立计数，不跨对象累加。
pub const MAX_OBJECT_FIELDS: usize = 1_024;

/// 单个数组的元素数上限（§2.5 的「单数组元素数」）。逐个数组独立计数，不跨数组累加。
pub const MAX_ARRAY_ELEMENTS: usize = 10_000;

/// 超过 §2.5 三个固定上限中的某一个。适配器映射为 `nodelink.protocol.schema_invalid`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum StructureError {
    #[error("JSON 嵌套深度为 {found}，超过固定上限 {limit}")]
    NestingDepth { found: usize, limit: usize },
    #[error("单个对象有 {found} 个字段，超过固定上限 {limit}")]
    ObjectFields { found: usize, limit: usize },
    #[error("单个数组有 {found} 个元素，超过固定上限 {limit}")]
    ArrayElements { found: usize, limit: usize },
}

/// 已打开的容器：它自己已经数到多少个成员/元素，以及下一个记号的位置。
enum Frame {
    Object {
        members: usize,
        /// 下一个字符串出现在键的位置（即开始一个新成员）。
        expecting_key: bool,
    },
    Array {
        elements: usize,
        /// 下一个值开始一个新元素（标量与容器都算一个元素）。
        expecting_value: bool,
    },
}

/// 扫描整帧文本并核对三个固定上限；任何一项越界即返回该错误。
pub fn check(text: &str) -> Result<(), StructureError> {
    let mut stack: Vec<Frame> = Vec::new();
    let mut in_string = false;
    let mut escaped = false;

    for byte in text.bytes() {
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match byte {
            b'"' => {
                in_string = true;
                // 字符串在「键」位置开始一个对象成员，在「元素」位置是一个数组元素；值位置不计数。
                count_string(stack.last_mut())?;
            }
            b'{' | b'[' => {
                // 数组元素位置的容器先记一个元素；对象值位置的容器不额外计数（成员已在键上记过）。
                count_array_element(stack.last_mut())?;
                if stack.len() + 1 > MAX_NESTING_DEPTH {
                    return Err(StructureError::NestingDepth {
                        found: stack.len() + 1,
                        limit: MAX_NESTING_DEPTH,
                    });
                }
                stack.push(if byte == b'{' {
                    Frame::Object {
                        members: 0,
                        expecting_key: true,
                    }
                } else {
                    Frame::Array {
                        elements: 0,
                        expecting_value: true,
                    }
                });
            }
            // 容器结束：只出栈，不校验配对（配对属语法判定）。
            b'}' | b']' => {
                stack.pop();
            }
            b',' => match stack.last_mut() {
                Some(Frame::Object { expecting_key, .. }) => *expecting_key = true,
                Some(Frame::Array {
                    expecting_value, ..
                }) => *expecting_value = true,
                None => {}
            },
            // 标量（数字、`true`/`false`/`null`）只能出现在值的位置：首字节记一个数组元素，
            // 后续字节到达这里时 `expecting_value` 已是 `false`，因此一个标量只记一次。
            b' ' | b'\t' | b'\n' | b'\r' | b':' => {}
            _ => count_array_element(stack.last_mut())?,
        }
    }
    Ok(())
}

/// 字符串项：对象键位置的字符串是一个新成员；数组元素位置的字符串是一个新元素。
fn count_string(frame: Option<&mut Frame>) -> Result<(), StructureError> {
    match frame {
        Some(Frame::Object {
            members,
            expecting_key,
        }) if *expecting_key => {
            *members += 1;
            *expecting_key = false;
            check_object_fields(*members)
        }
        other => count_array_element(other),
    }
}

/// 数组元素位置的标量或容器：它是该数组的一个新元素（已在别处计数的位置不影响）。
fn count_array_element(frame: Option<&mut Frame>) -> Result<(), StructureError> {
    match frame {
        Some(Frame::Array {
            elements,
            expecting_value,
        }) if *expecting_value => {
            *elements += 1;
            *expecting_value = false;
            check_array_elements(*elements)
        }
        _ => Ok(()),
    }
}

fn check_object_fields(found: usize) -> Result<(), StructureError> {
    if found > MAX_OBJECT_FIELDS {
        return Err(StructureError::ObjectFields {
            found,
            limit: MAX_OBJECT_FIELDS,
        });
    }
    Ok(())
}

fn check_array_elements(found: usize) -> Result<(), StructureError> {
    if found > MAX_ARRAY_ELEMENTS {
        return Err(StructureError::ArrayElements {
            found,
            limit: MAX_ARRAY_ELEMENTS,
        });
    }
    Ok(())
}
