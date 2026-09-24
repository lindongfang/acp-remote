//! 不透明 JSON 文本载体（`EventPayload.view` / `PublicError.details` / `CommandResult` /
//! `ElicitationContentValue::Unknown`）。
//!
//! `docs/CORE_PORTS_AND_STORAGE.md` §2 明令 core 不得出现 `serde_json::Value`，因此这里定义 core 自己的
//! JSON 文本类型：构造时按 RFC 8259 校验**完整**文档，[`ViewJson`] 额外要求顶层是 object，之后**原样保留
//! 字节**（不做规范化、不重排键、不丢弃未知字段）。规范 JSON（ACPR-CJ1）与 wire DTO 属协议 crate，不在这里
//! 实现。
//!
//! 两个刻意的取舍：
//!
//! - **重复对象键**不做检查。`docs/SYNC_PROTOCOL.md` §3.3 要求 wire 文本不得含重复键，但那是 framing 层的
//!   判据（协议 crate 在解码时拒绝）；core 只面对已经过该层的文本，且逐键去重会强制为每个对象分配。
//!   （例外：需要按键取值的容器——`ElicitationValues`——在自己的解析里显式拒绝重复键。）
//! - **深度上限** `MAX_JSON_DEPTH`：校验器是递归下降，不设上限时攻击者（imported 资源事件的
//!   `payload.view`）可以用深嵌套输入耗尽栈。上限只拒绝畸形输入，正常视图与 ACP raw 远低于它。

use std::fmt;
use std::str::FromStr;

use super::InvalidValue;

/// `ViewJson` 允许的最大嵌套深度（容器层数，顶层 object 计 1）。
const MAX_JSON_DEPTH: u16 = 128;

/// 一个**已验证**的 JSON 文本。
///
/// `ViewJson::new` 保证：输入是合法 UTF-8、是良构 JSON 文档、顶层是 object、嵌套深度不超过
/// [`ViewJson::MAX_DEPTH`]。字节原样保存，`as_str()` 返回的文本与构造输入逐字节相同。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewJson {
    text: String,
    depth: u16,
}

impl ViewJson {
    /// 最大嵌套深度。
    pub const MAX_DEPTH: u16 = MAX_JSON_DEPTH;

    /// 校验并保留一个 JSON object 文本。
    pub fn new(text: &str) -> Result<Self, InvalidValue> {
        let depth = validate_document(text, true)?;
        Ok(Self {
            text: text.to_owned(),
            depth,
        })
    }

    /// 校验并保留字节形式的 JSON object（输入必须是合法 UTF-8）。
    pub fn from_slice(bytes: &[u8]) -> Result<Self, InvalidValue> {
        let text = std::str::from_utf8(bytes).map_err(|_| InvalidValue::Json)?;
        Self::new(text)
    }

    /// 空对象 `{}`，用于没有任何 details 的 `PublicError` 与空结果。
    pub fn empty_object() -> Self {
        Self {
            text: "{}".to_owned(),
            depth: 1,
        }
    }

    /// 原样文本。
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// 文本的 UTF-8 字节数。
    pub fn byte_len(&self) -> usize {
        self.text.len()
    }

    /// 嵌套深度（顶层 object 为 1）。
    pub fn depth(&self) -> u16 {
        self.depth
    }

    /// 取出底层文本。
    pub fn into_string(self) -> String {
        self.text
    }
}

impl fmt::Display for ViewJson {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

impl FromStr for ViewJson {
    type Err = InvalidValue;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

/// 任意 JSON **值**的原文载体（`ElicitationContentValue::Unknown` 等「未来 ACP 变体」位置）。
///
/// 与 [`ViewJson`] 同一套校验与保留口径，只把「顶层必须是 object」放宽为「任意 JSON 值」：构造时是良构
/// JSON 文档、嵌套深度不超过 [`JsonValueText::MAX_DEPTH`]，之后逐字节保留。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonValueText {
    text: String,
    depth: u16,
}

impl JsonValueText {
    /// 最大嵌套深度。
    pub const MAX_DEPTH: u16 = MAX_JSON_DEPTH;

    /// 校验并保留一个 JSON 值文本。
    pub fn new(text: &str) -> Result<Self, InvalidValue> {
        let depth = validate_document(text, false)?;
        Ok(Self {
            text: text.to_owned(),
            depth,
        })
    }

    /// 原样文本。
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// 文本的 UTF-8 字节数。
    pub fn byte_len(&self) -> usize {
        self.text.len()
    }

    /// 嵌套深度（标量记 0）。
    pub fn depth(&self) -> u16 {
        self.depth
    }

    /// 取出底层文本。
    pub fn into_string(self) -> String {
        self.text
    }
}

impl fmt::Display for JsonValueText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

impl FromStr for JsonValueText {
    type Err = InvalidValue;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

/// 校验 JSON 文本；`require_object` 为真时要求顶层是 object，否则接受任意 JSON 值。
///
/// 返回最大嵌套深度（顶层标量记 0）。供 `AcpRaw.raw_json` 复用（ACP 原文同样是 JSON document）。
pub(crate) fn validate_document(text: &str, require_object: bool) -> Result<u16, InvalidValue> {
    Scanner {
        bytes: text.as_bytes(),
        pos: 0,
        depth: 0,
        max_depth: 0,
    }
    .scan_document(require_object)
}

/// 解析顶层 object 的成员：返回（解码后的键，值的原文切片），保持文本顺序。
///
/// 调用方负责按自己的语义校验成员（例如拒绝重复键或限定值的形状）。输入必须已经过
/// [`validate_document`]；本函数重走一遍以保证成员级切片的边界正确。
pub(crate) fn object_members(text: &str) -> Result<Vec<(String, &str)>, InvalidValue> {
    let mut scanner = Scanner {
        bytes: text.as_bytes(),
        pos: 0,
        depth: 0,
        max_depth: 0,
    };
    scanner.skip_ws();
    if scanner.peek() != Some(b'{') {
        return Err(InvalidValue::Json);
    }
    scanner.pos += 1;
    scanner.depth += 1;
    if scanner.depth > MAX_JSON_DEPTH {
        return Err(InvalidValue::Depth {
            max: MAX_JSON_DEPTH,
        });
    }
    let mut members = Vec::new();
    scanner.skip_ws();
    if scanner.peek() == Some(b'}') {
        scanner.pos += 1;
        scanner.depth -= 1;
    } else {
        loop {
            scanner.skip_ws();
            let key_start = scanner.pos;
            if scanner.peek() != Some(b'"') {
                return Err(InvalidValue::Json);
            }
            scanner.scan_string()?;
            let key =
                decode_json_string(&text[key_start..scanner.pos]).ok_or(InvalidValue::Json)?;
            scanner.skip_ws();
            if scanner.peek() != Some(b':') {
                return Err(InvalidValue::Json);
            }
            scanner.pos += 1;
            scanner.skip_ws();
            let value_start = scanner.pos;
            scanner.scan_value()?;
            members.push((key, &text[value_start..scanner.pos]));
            scanner.skip_ws();
            match scanner.peek() {
                Some(b',') => scanner.pos += 1,
                Some(b'}') => {
                    scanner.pos += 1;
                    scanner.depth -= 1;
                    break;
                }
                _ => return Err(InvalidValue::Json),
            }
        }
    }
    scanner.skip_ws();
    if scanner.pos != scanner.bytes.len() {
        return Err(InvalidValue::Json);
    }
    Ok(members)
}

/// 顶层 object 单个成员的取值（注入前的一致性判定）。
///
/// 非字符串取值不保留原文：冲突路径的错误信息刻意不带取值，避免把适配器数据带进协议错误。
#[derive(Debug)]
pub(crate) enum MemberValue {
    /// 键不存在。
    Absent,
    /// 键存在且值是 JSON 字符串（解码后的取值）。
    Text(String),
    /// 键存在但值不是 JSON 字符串。
    NonText,
}

/// 读取顶层 object 的一个成员。
///
/// 与 [`object_members`] 同一口径：输入必须已经过 [`validate_document`]。
pub(crate) fn top_level_member(text: &str, key: &str) -> Result<MemberValue, InvalidValue> {
    let members = object_members(text)?;
    Ok(match members.iter().find(|(name, _)| name == key) {
        None => MemberValue::Absent,
        Some((_, raw)) => match decode_json_string(raw) {
            Some(value) => MemberValue::Text(value),
            None => MemberValue::NonText,
        },
    })
}

/// 在顶层 object 的**最前面**插入一个成员；`value_json` 必须是 JSON 字符串字面量（含两侧引号）。
///
/// 只做前置插入：既有成员的字节与顺序原样保留（不规范化、不重排键、不丢未知字段）。目标键已存在时
/// 返回 [`InvalidValue::Field`]（调用方必须先用 [`top_level_member`] 判定冲突，避免写出重复键）；插入
/// 结果仍需交 [`ViewJson::new`] 重新校验（深度可能 +1，仍受 `MAX_JSON_DEPTH` 约束）。
pub(crate) fn insert_string_member_front(
    text: &str,
    key: &str,
    value_json: &str,
) -> Result<String, InvalidValue> {
    let members = object_members(text)?;
    if members.iter().any(|(name, _)| name == key) {
        return Err(InvalidValue::Field);
    }
    if decode_json_string(value_json).is_none() {
        return Err(InvalidValue::Json);
    }
    let open = text.find('{').ok_or(InvalidValue::Json)?;
    let (head, rest) = text.split_at(open + 1);
    let mut out = String::with_capacity(text.len() + key.len() + value_json.len() + 6);
    out.push_str(head);
    out.push_str(&encode_json_string(key));
    out.push(':');
    out.push_str(value_json);
    if !members.is_empty() {
        out.push(',');
    }
    out.push_str(rest);
    Ok(out)
}

/// 若 `raw` 是「全部元素都是 JSON 字符串」的数组，返回解码后的元素（空数组返回空 `Vec`）；否则 `None`。
pub(crate) fn string_array_items(raw: &str) -> Option<Vec<String>> {
    let mut scanner = Scanner {
        bytes: raw.as_bytes(),
        pos: 0,
        depth: 0,
        max_depth: 0,
    };
    scanner.skip_ws();
    if scanner.peek() != Some(b'[') {
        return None;
    }
    scanner.pos += 1;
    let mut items = Vec::new();
    scanner.skip_ws();
    if scanner.peek() == Some(b']') {
        return Some(items);
    }
    loop {
        scanner.skip_ws();
        let start = scanner.pos;
        if scanner.peek() != Some(b'"') || scanner.scan_string().is_err() {
            return None;
        }
        items.push(decode_json_string(&raw[start..scanner.pos])?);
        scanner.skip_ws();
        match scanner.peek() {
            Some(b',') => scanner.pos += 1,
            Some(b']') => return Some(items),
            _ => return None,
        }
    }
}

/// 解码一个 JSON 字符串字面量（含两侧引号）；非法转义或代理对不完整时返回 `None`。
pub(crate) fn decode_json_string(raw: &str) -> Option<String> {
    let inner = raw.strip_prefix('"')?.strip_suffix('"')?;
    let bytes = inner.as_bytes();
    let mut out = String::with_capacity(inner.len());
    let mut pos = 0;
    while pos < bytes.len() {
        if bytes[pos] != b'\\' {
            let ch = inner[pos..].chars().next()?;
            out.push(ch);
            pos += ch.len_utf8();
            continue;
        }
        pos += 1;
        let escaped = *bytes.get(pos)?;
        pos += 1;
        match escaped {
            b'"' => out.push('"'),
            b'\\' => out.push('\\'),
            b'/' => out.push('/'),
            b'b' => out.push('\u{0008}'),
            b'f' => out.push('\u{000c}'),
            b'n' => out.push('\n'),
            b'r' => out.push('\r'),
            b't' => out.push('\t'),
            b'u' => {
                let unit = hex4(bytes, pos)?;
                pos += 4;
                let code = if (0xd800..=0xdbff).contains(&unit) {
                    if bytes.get(pos) != Some(&b'\\') || bytes.get(pos + 1) != Some(&b'u') {
                        return None;
                    }
                    pos += 2;
                    let low = hex4(bytes, pos)?;
                    pos += 4;
                    if !(0xdc00..=0xdfff).contains(&low) {
                        return None;
                    }
                    0x1_0000 + ((u32::from(unit) - 0xd800) << 10) + (u32::from(low) - 0xdc00)
                } else if (0xdc00..=0xdfff).contains(&unit) {
                    return None;
                } else {
                    u32::from(unit)
                };
                out.push(char::from_u32(code)?);
            }
            _ => return None,
        }
    }
    Some(out)
}

/// 编码一个 JSON 字符串字面量（含两侧引号），控制字符转义。
pub(crate) fn encode_json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000c}' => out.push_str("\\f"),
            ch if (ch as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", ch as u32));
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn hex4(bytes: &[u8], pos: usize) -> Option<u16> {
    let mut value: u16 = 0;
    for index in 0..4 {
        let byte = *bytes.get(pos + index)?;
        let nibble = match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            b'A'..=b'F' => byte - b'A' + 10,
            _ => return None,
        };
        value = (value << 4) | u16::from(nibble);
    }
    Some(value)
}

struct Scanner<'a> {
    bytes: &'a [u8],
    pos: usize,
    depth: u16,
    max_depth: u16,
}

impl Scanner<'_> {
    fn scan_document(mut self, require_object: bool) -> Result<u16, InvalidValue> {
        self.skip_ws();
        if require_object && self.peek() != Some(b'{') {
            return Err(InvalidValue::Json);
        }
        self.scan_value()?;
        self.skip_ws();
        if self.pos != self.bytes.len() {
            return Err(InvalidValue::Json);
        }
        Ok(self.max_depth)
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while let Some(b) = self.peek() {
            match b {
                b' ' | b'\t' | b'\n' | b'\r' => self.pos += 1,
                _ => break,
            }
        }
    }

    fn scan_value(&mut self) -> Result<(), InvalidValue> {
        match self.peek().ok_or(InvalidValue::Json)? {
            b'{' => self.scan_container(b'{', b'}'),
            b'[' => self.scan_container(b'[', b']'),
            b'"' => self.scan_string(),
            b't' => self.scan_literal(b"true"),
            b'f' => self.scan_literal(b"false"),
            b'n' => self.scan_literal(b"null"),
            b'-' | b'0'..=b'9' => self.scan_number(),
            _ => Err(InvalidValue::Json),
        }
    }

    fn scan_container(&mut self, open: u8, close: u8) -> Result<(), InvalidValue> {
        self.pos += 1;
        self.depth += 1;
        if self.depth > MAX_JSON_DEPTH {
            return Err(InvalidValue::Depth {
                max: MAX_JSON_DEPTH,
            });
        }
        self.max_depth = self.max_depth.max(self.depth);
        self.skip_ws();
        if self.peek() == Some(close) {
            self.pos += 1;
            self.depth -= 1;
            return Ok(());
        }
        loop {
            self.skip_ws();
            if open == b'{' {
                if self.peek() != Some(b'"') {
                    return Err(InvalidValue::Json);
                }
                self.scan_string()?;
                self.skip_ws();
                if self.peek() != Some(b':') {
                    return Err(InvalidValue::Json);
                }
                self.pos += 1;
                self.skip_ws();
            }
            self.scan_value()?;
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b) if b == close => {
                    self.pos += 1;
                    self.depth -= 1;
                    return Ok(());
                }
                _ => return Err(InvalidValue::Json),
            }
        }
    }

    fn scan_number(&mut self) -> Result<(), InvalidValue> {
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        match self.peek() {
            Some(b'0') => self.pos += 1,
            Some(b'1'..=b'9') => {
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.pos += 1;
                }
            }
            _ => return Err(InvalidValue::Json),
        }
        if self.peek() == Some(b'.') {
            self.pos += 1;
            if self.take_digits() == 0 {
                return Err(InvalidValue::Json);
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            if self.take_digits() == 0 {
                return Err(InvalidValue::Json);
            }
        }
        Ok(())
    }

    fn take_digits(&mut self) -> usize {
        let start = self.pos;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        self.pos - start
    }

    fn scan_literal(&mut self, literal: &[u8]) -> Result<(), InvalidValue> {
        let end = self.pos + literal.len();
        if self.bytes.get(self.pos..end) == Some(literal) {
            self.pos = end;
            Ok(())
        } else {
            Err(InvalidValue::Json)
        }
    }

    fn scan_string(&mut self) -> Result<(), InvalidValue> {
        if self.peek() != Some(b'"') {
            return Err(InvalidValue::Json);
        }
        self.pos += 1;
        loop {
            match self.peek().ok_or(InvalidValue::Json)? {
                b'"' => {
                    self.pos += 1;
                    return Ok(());
                }
                b'\\' => {
                    self.pos += 1;
                    match self.peek().ok_or(InvalidValue::Json)? {
                        b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't' => self.pos += 1,
                        b'u' => {
                            self.pos += 1;
                            self.scan_escaped_code_point()?;
                        }
                        _ => return Err(InvalidValue::Json),
                    }
                }
                0x00..=0x1f => return Err(InvalidValue::Json),
                // 多字节 UTF-8 序列由调用方保证合法（输入是 `&str`），逐字节跳过即可：
                // 续字节都 >= 0x80，不可能与 ASCII 定界符混淆。
                _ => self.pos += 1,
            }
        }
    }

    /// 读取 `\uXXXX`，并要求代理对完整（与 `serde_json` 的拒绝行为一致）。
    fn scan_escaped_code_point(&mut self) -> Result<(), InvalidValue> {
        let unit = self.scan_hex4()?;
        if (0xd800..=0xdbff).contains(&unit) {
            if self.peek() != Some(b'\\') {
                return Err(InvalidValue::Json);
            }
            self.pos += 1;
            if self.peek() != Some(b'u') {
                return Err(InvalidValue::Json);
            }
            self.pos += 1;
            let low = self.scan_hex4()?;
            if !(0xdc00..=0xdfff).contains(&low) {
                return Err(InvalidValue::Json);
            }
        } else if (0xdc00..=0xdfff).contains(&unit) {
            return Err(InvalidValue::Json);
        }
        Ok(())
    }

    fn scan_hex4(&mut self) -> Result<u16, InvalidValue> {
        let mut value: u16 = 0;
        for _ in 0..4 {
            let byte = self.peek().ok_or(InvalidValue::Json)?;
            let nibble = match byte {
                b'0'..=b'9' => byte - b'0',
                b'a'..=b'f' => byte - b'a' + 10,
                b'A'..=b'F' => byte - b'A' + 10,
                _ => return Err(InvalidValue::Json),
            };
            value = (value << 4) | u16::from(nibble);
            self.pos += 1;
        }
        Ok(value)
    }
}
