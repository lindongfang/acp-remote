//! ACPR-CJ1 规范 JSON（`docs/SYNC_PROTOCOL.md` §3.3）：`payloadDigest`/`snapshotDigest` 一类内容摘要
//! 的前像。
//!
//! [`canonicalize`] 把一段 JSON 文本变成**唯一的**字节形式：UTF-8、无 BOM、无前后空白、对象成员名按
//! UTF-16 code unit 升序、字符串最小转义、数字限定为 `|n| <= 2^53-1` 的整数、顶层是 object 或 array。
//! `base64url(SHA-256(canonicalize(payload)))` 是 `payloadDigest` 的值（哈希由调用方做，本 crate 没有
//! 密码学依赖）。
//!
//! # 为什么不经过 `serde_json`
//!
//! 两条硬要求让 `serde_json` 的公开 API 都用不上：
//!
//! - **重复成员名必须被拒绝，不得静默覆盖。** `serde_json::Value` 与 derive 出来的 struct 都是后到者
//!   胜（`MapAccess` 层的 `duplicate_field` 只能覆盖到 `#[serde(deny_unknown_fields)]` 的具名结构体，覆盖
//!   不了任意深度的开放对象）。
//! - **数字字面量必须与 JS 参考实现逐字面量同判。** workspace 打开了 `arbitrary_precision`，放不进
//!   `u64`/`i64` 的字面量（`1.0`、`1e2`、超出 u64 的大整数）会被 serde_json 伪装成一个只含私有键
//!   `$serde_json::private::Number` 的 map——一个真的形如 `{"$serde_json::private::Number":"1.0"}` 的对象
//!   与它在 serde 层无法区分，而摘要前像不能有这种歧义。
//!
//! 因此这里与 `core::model::json` 的 `Scanner` 同一路子：自带一份只覆盖 JSON 文法的递归下降扫描器，
//! 一次遍历直接写出规范形式（对象成员先缓冲再排序，其余边扫边写）。嵌套深度上限与 core 同值。
//!
//! # 与 JS 参考实现（`scripts/check-contract-assets.mjs` 的 `acprCj1`/`cj1String`）的差异
//!
//! 同一份语料（全部 `payloadDigest` fixture + 随机 JSON 文档，含对象与数组两种顶层）上两者逐字节一致，
//! 只有两处不同：
//!
//! - **重复成员名**：JS 侧的值来自 `JSON.parse`，重复键已被它静默覆盖（后到者胜），检测不出来；§3.3
//!   明确不允许重复键，这里报 [`Cj1Error::DuplicateKey`]。
//! - **顶层标量**：§3.3 要求顶层是 object 或 array，本实现照办；JS 的 `acprCj1` 对任何 JSON 值都成立，
//!   所以顶层是 string/number/bool/null 时 JS 会给出结果、这里报 [`Cj1Error::NotAContainer`]。
//!
//! 字符串转义（`"`、`\` 与小写 `\u00xx` 的控制字符）、孤立代理项、数字的字面量判定上**没有**差异：
//! 参考实现的 `cj1String`/`acprCj1` 与这里同一口径（`JSON.stringify` 的短转义已被它取代）。

use std::cmp::Ordering;
use std::fmt::{self, Display, Write as _};
use std::str::Utf8Error;

/// ACPR-CJ1 允许的最大嵌套深度（容器层数，顶层 object 记 1）。与 `core::model::json` 的
/// `MAX_JSON_DEPTH` 同值：扫描器是递归下降，不设上限时深嵌套输入能打爆栈。
pub const MAX_DEPTH: u16 = 128;

/// `|n| <= 2^53-1`（JS 的 `Number.MAX_SAFE_INTEGER`）。
const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

/// `\u00xx` 用的小写十六进制字母表。
const HEX_DIGITS: [char; 16] = [
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c', 'd', 'e', 'f',
];

/// JSON 值的类型。只用于 [`Cj1Error::NotAContainer`]：§3.3 允许顶层是 object 或 array
/// （「顶层必须是 object 或 array」），只有标量会被拒。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonKind {
    Object,
    Array,
    String,
    Number,
    Bool,
    Null,
}

impl JsonKind {
    /// 按 JSON 值类型各自唯一的首字节判定（调用方保证 `byte` 非空白）。
    fn of(byte: u8) -> Option<Self> {
        match byte {
            b'{' => Some(Self::Object),
            b'[' => Some(Self::Array),
            b'"' => Some(Self::String),
            b't' | b'f' => Some(Self::Bool),
            b'n' => Some(Self::Null),
            b'-' | b'0'..=b'9' => Some(Self::Number),
            _ => None,
        }
    }
}

impl Display for JsonKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Object => "object",
            Self::Array => "array",
            Self::String => "string",
            Self::Number => "number",
            Self::Bool => "boolean",
            Self::Null => "null",
        })
    }
}

/// ACPR-CJ1 规范化过程中发现的问题。
///
/// 除 [`Cj1Error::InvalidUtf8`] 外，所有变体都描述**文本**层面的拒绝，调用方不需要区分它们就能安全
/// 失败；变体只用于诊断与测试断言。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Cj1Error {
    #[error("输入不是有效的 UTF-8：{0}")]
    InvalidUtf8(Utf8Error),
    #[error("输入以 BOM 开头，ACPR-CJ1 不允许 BOM")]
    Bom,
    #[error("第 {offset} 字节处不是合法的 JSON")]
    Syntax { offset: usize },
    #[error("第 {offset} 字节处出现未转义的控制字符 U+{byte:02X}")]
    ControlCharacter { offset: usize, byte: u8 },
    #[error("第 {offset} 字节处的 \\u 转义不是有效 Unicode（孤立代理项或非法码位）")]
    InvalidUnicodeEscape { offset: usize },
    #[error("ACPR-CJ1 的顶层必须是 object 或 array，实际是 {found}")]
    NotAContainer { found: JsonKind },
    #[error("对象成员名重复：{name}")]
    DuplicateKey { name: String },
    #[error("数字必须是整数：{literal}")]
    NonInteger { literal: String },
    #[error("数字超出 |n| <= 2^53-1：{literal}")]
    IntegerOutOfRange { literal: String },
    #[error("嵌套深度超过 {max} 层")]
    TooDeep { max: u16 },
}

/// 把一段 JSON 文本规范化为 ACPR-CJ1 形式。
///
/// 数字按 JS 的**值**语义判定，与 `scripts/check-contract-assets.mjs` 的 `acprCj1` 逐字面量一致：
/// 字面量先按 JSON 数字文法严格校验，再取其 `f64` 值（`JSON.parse` 的语义），要求该值有限、无小数部分
/// 且 `|n| <= 2^53-1`，然后写成它的十进制整数形式。于是 `1.5`/`1e-1` 被拒（[`Cj1Error::NonInteger`]），
/// `1e400`/`9007199254740992` 被拒（[`Cj1Error::IntegerOutOfRange`]），而 `1.0`/`1e2`/`-0` 分别写成
/// `1`/`100`/`0`——这正是参考实现的行为（§3.3 约束的是值，不是字面量写法）。
///
/// 输入前后的空白可以存在（输出永远没有空白）；顶层必须是 object 或 array（§3.3 的「顶层必须是
/// object 或 array」），容器收尾之后除空白外的任何字节都是 [`Cj1Error::Syntax`]。
pub fn canonicalize(text: &str) -> Result<String, Cj1Error> {
    if text.starts_with(BOM) {
        return Err(Cj1Error::Bom);
    }
    let mut parser = Parser::new(text);
    parser.skip_ws();
    let Some(kind) = parser.peek().and_then(JsonKind::of) else {
        return Err(parser.syntax());
    };
    let mut out = String::with_capacity(text.len());
    match kind {
        JsonKind::Object => parser.write_object(&mut out)?,
        JsonKind::Array => parser.write_array(&mut out)?,
        _ => {
            // 顶层是标量。先确认文本本身是良构 JSON，再报类型错误：`tru` 是语法错误，不是类型错误。
            parser.write_value(&mut out)?;
            parser.finish()?;
            return Err(Cj1Error::NotAContainer { found: kind });
        }
    }
    parser.finish()?;
    Ok(out)
}

/// [`canonicalize`] 的字节入口：先按 UTF-8 校验（`payload_json` 一类字段以 BLOB 落地）。
pub fn canonicalize_bytes(bytes: &[u8]) -> Result<String, Cj1Error> {
    let text = std::str::from_utf8(bytes).map_err(Cj1Error::InvalidUtf8)?;
    canonicalize(text)
}

/// BOM（U+FEFF）。合法 JSON 文本里它只能出现在字符串内。
const BOM: char = '\u{feff}';

/// 递归下降的 ACPR-CJ1 扫描器：一次遍历直接写出规范形式。
///
/// `pos` 始终落在字符边界上：结构字节都是 ASCII，遇到非 ASCII 的结构位置直接报 [`Cj1Error::Syntax`]
/// 而不前进；字符串内部则按 [`char`] 前进。
struct Parser<'a> {
    text: &'a str,
    bytes: &'a [u8],
    pos: usize,
    depth: u16,
}

impl<'a> Parser<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            text,
            bytes: text.as_bytes(),
            pos: 0,
            depth: 0,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    /// 位置无关的错误：`pos` 就是当前出问题的字节。
    fn syntax(&self) -> Cj1Error {
        Cj1Error::Syntax { offset: self.pos }
    }

    /// 跳过 JSON 的四种空白（空格、`\t`、`\n`、`\r`）；其余空白字符不是 JSON 空白。
    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    /// 跳过尾随空白，并要求输入已结束（§3.3 不允许尾随内容）。
    fn finish(&mut self) -> Result<(), Cj1Error> {
        self.skip_ws();
        if self.pos == self.bytes.len() {
            Ok(())
        } else {
            Err(self.syntax())
        }
    }

    fn enter(&mut self) -> Result<(), Cj1Error> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            Err(Cj1Error::TooDeep { max: MAX_DEPTH })
        } else {
            Ok(())
        }
    }

    fn leave(&mut self) {
        self.depth -= 1;
    }

    /// 写出一个 JSON 值（调用时 `pos` 指向该值的首字节）。
    fn write_value(&mut self, out: &mut String) -> Result<(), Cj1Error> {
        match self.peek() {
            Some(b'{') => self.write_object(out),
            Some(b'[') => self.write_array(out),
            Some(b'"') => {
                out.push('"');
                self.scan_string(|ch| write_escaped_char(ch, out))?;
                out.push('"');
                Ok(())
            }
            Some(b't') => self.write_keyword(b"true", "true", out),
            Some(b'f') => self.write_keyword(b"false", "false", out),
            Some(b'n') => self.write_keyword(b"null", "null", out),
            Some(b'-' | b'0'..=b'9') => self.write_number(out),
            _ => Err(self.syntax()),
        }
    }

    /// 匹配一个 ASCII 关键字（`true`/`false`/`null`），逐字节比较以便把错误指到第一个不同处。
    fn write_keyword(
        &mut self,
        keyword: &[u8],
        literal: &str,
        out: &mut String,
    ) -> Result<(), Cj1Error> {
        for expected in keyword {
            if self.peek() != Some(*expected) {
                return Err(self.syntax());
            }
            self.pos += 1;
        }
        out.push_str(literal);
        Ok(())
    }

    /// 写一个 JSON object：成员先各自规范化到缓冲区，再按 UTF-16 code unit 序排序、查重、拼接。
    fn write_object(&mut self, out: &mut String) -> Result<(), Cj1Error> {
        self.pos += 1;
        self.enter()?;
        let mut members: Vec<(String, String)> = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.pos += 1;
        } else {
            loop {
                self.skip_ws();
                if self.peek() != Some(b'"') {
                    return Err(self.syntax());
                }
                let mut name = String::new();
                self.scan_string(|ch| name.push(ch))?;
                self.skip_ws();
                if self.peek() != Some(b':') {
                    return Err(self.syntax());
                }
                self.pos += 1;
                self.skip_ws();
                let mut value = String::new();
                self.write_value(&mut value)?;
                members.push((name, value));
                self.skip_ws();
                match self.peek() {
                    Some(b',') => self.pos += 1,
                    Some(b'}') => {
                        self.pos += 1;
                        break;
                    }
                    _ => return Err(self.syntax()),
                }
            }
        }
        self.leave();
        members.sort_unstable_by(|left, right| utf16_cmp(&left.0, &right.0));
        // 排序后相邻相等即重复成员名；成员名的转义写法（`"a"` 与 `"\u0061"`）在这里已经解码，能相互比较。
        if let Some(pair) = members.windows(2).find(|pair| pair[0].0 == pair[1].0) {
            return Err(Cj1Error::DuplicateKey {
                name: pair[0].0.clone(),
            });
        }
        out.push('{');
        for (index, (name, value)) in members.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            write_escaped(name, out);
            out.push(':');
            out.push_str(value);
        }
        out.push('}');
        Ok(())
    }

    /// 写一个 JSON array：元素顺序即规范顺序，可以边扫边写。
    fn write_array(&mut self, out: &mut String) -> Result<(), Cj1Error> {
        self.pos += 1;
        self.enter()?;
        out.push('[');
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.pos += 1;
            self.leave();
            out.push(']');
            return Ok(());
        }
        loop {
            self.skip_ws();
            self.write_value(out)?;
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                    out.push(',');
                }
                Some(b']') => {
                    self.pos += 1;
                    break;
                }
                _ => return Err(self.syntax()),
            }
        }
        self.leave();
        out.push(']');
        Ok(())
    }

    /// 解析一个 JSON 字符串字面量（调用时 `pos` 指向开引号），把解码后的字符逐个交给 `sink`。
    ///
    /// 成员名需要解码后的文本用于排序与查重，字符串值需要按最小转义写出——两条路径共用这一份扫描。
    fn scan_string(&mut self, mut sink: impl FnMut(char)) -> Result<(), Cj1Error> {
        self.pos += 1;
        loop {
            let Some(ch) = self.text[self.pos..].chars().next() else {
                return Err(self.syntax());
            };
            match ch {
                '"' => {
                    self.pos += 1;
                    return Ok(());
                }
                '\\' => {
                    let offset = self.pos;
                    self.pos += 1;
                    match self.text[self.pos..].chars().next() {
                        Some('"') => {
                            self.pos += 1;
                            sink('"');
                        }
                        Some('\\') => {
                            self.pos += 1;
                            sink('\\');
                        }
                        Some('/') => {
                            self.pos += 1;
                            sink('/');
                        }
                        Some('b') => {
                            self.pos += 1;
                            sink('\u{0008}');
                        }
                        Some('f') => {
                            self.pos += 1;
                            sink('\u{000c}');
                        }
                        Some('n') => {
                            self.pos += 1;
                            sink('\n');
                        }
                        Some('r') => {
                            self.pos += 1;
                            sink('\r');
                        }
                        Some('t') => {
                            self.pos += 1;
                            sink('\t');
                        }
                        Some('u') => {
                            self.pos += 1;
                            sink(self.unicode_escape(offset)?);
                        }
                        _ => return Err(Cj1Error::Syntax { offset }),
                    }
                }
                // JSON 字符串里不允许裸控制字符，转义后的（`\u0001`）才合法。
                control if control < '\u{20}' => {
                    return Err(Cj1Error::ControlCharacter {
                        offset: self.pos,
                        byte: control as u8,
                    });
                }
                normal => {
                    sink(normal);
                    self.pos += normal.len_utf8();
                }
            }
        }
    }

    /// 解码 `\u` 转义（调用时 `pos` 指向 4 位十六进制的第一位）；代理项必须成对（§3.3 不允许无效
    /// Unicode）。`offset` 是 `\` 的位置，只用于报错。
    fn unicode_escape(&mut self, offset: usize) -> Result<char, Cj1Error> {
        let unit = self.hex4()?;
        let code = if (0xd800..=0xdbff).contains(&unit) {
            if self.peek() != Some(b'\\') || self.bytes.get(self.pos + 1) != Some(&b'u') {
                return Err(Cj1Error::InvalidUnicodeEscape { offset });
            }
            self.pos += 2;
            let low = self.hex4()?;
            if !(0xdc00..=0xdfff).contains(&low) {
                return Err(Cj1Error::InvalidUnicodeEscape { offset });
            }
            0x1_0000 + ((u32::from(unit) - 0xd800) << 10) + (u32::from(low) - 0xdc00)
        } else if (0xdc00..=0xdfff).contains(&unit) {
            return Err(Cj1Error::InvalidUnicodeEscape { offset });
        } else {
            u32::from(unit)
        };
        char::from_u32(code).ok_or(Cj1Error::InvalidUnicodeEscape { offset })
    }

    fn hex4(&mut self) -> Result<u16, Cj1Error> {
        let mut value: u16 = 0;
        for _ in 0..4 {
            let byte = self.peek().ok_or_else(|| self.syntax())?;
            let nibble = match byte {
                b'0'..=b'9' => byte - b'0',
                b'a'..=b'f' => byte - b'a' + 10,
                b'A'..=b'F' => byte - b'A' + 10,
                _ => return Err(self.syntax()),
            };
            self.pos += 1;
            value = (value << 4) | u16::from(nibble);
        }
        Ok(value)
    }

    /// 写一个 JSON 数字：严格按 JSON 文法取字面量，再按 JS 的值语义判定并写成十进制整数。
    fn write_number(&mut self, out: &mut String) -> Result<(), Cj1Error> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        match self.peek() {
            // 前导零之后不能再有数字（`01` 不是 JSON 数字）。
            Some(b'0') => {
                self.pos += 1;
                if matches!(self.peek(), Some(b'0'..=b'9')) {
                    return Err(self.syntax());
                }
            }
            Some(b'1'..=b'9') => {
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.pos += 1;
                }
            }
            _ => return Err(self.syntax()),
        }
        if self.peek() == Some(b'.') {
            self.pos += 1;
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(self.syntax());
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(self.syntax());
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        let literal = &self.text[start..self.pos];
        // 上面的文法与 `f64` 的解析域重合（超长数字/指数只会溢出成无穷，不会失败），这里的映射只为
        // 不出现 panic 路径：`1e999…9` 与 `1e-999…9` 分别得到 `inf` 与 `0.0`，与 `JSON.parse` 相同。
        let value = literal.parse::<f64>().map_err(|_| self.syntax())?;
        if value.abs() > MAX_SAFE_INTEGER {
            return Err(Cj1Error::IntegerOutOfRange {
                literal: literal.to_owned(),
            });
        }
        if !value.is_finite() || value.fract() != 0.0 {
            return Err(Cj1Error::NonInteger {
                literal: literal.to_owned(),
            });
        }
        // `|n| <= 2^53-1` 的整数转换是精确的；`-0.0` 写成 `0`，与 JS 的 `String(-0)` 一致。
        let _ = write!(out, "{}", value as i64);
        Ok(())
    }
}

/// ACPR-CJ1 的成员名顺序：按 UTF-16 code unit 升序（`docs/SYNC_PROTOCOL.md` §3.3）。
///
/// 与码位序（等价于 UTF-8 字节序）不同：U+10000 以上字符的首个 code unit 落在 `0xD800..=0xDBFF`，
/// 因此它们排在 U+E000..=U+FFFF 的 BMP 字符**之前**。
fn utf16_cmp(left: &str, right: &str) -> Ordering {
    left.encode_utf16().cmp(right.encode_utf16())
}

/// 写一个 JSON 字符串字面量（含两侧引号）。
fn write_escaped(text: &str, out: &mut String) {
    out.push('"');
    for ch in text.chars() {
        write_escaped_char(ch, out);
    }
    out.push('"');
}

/// 写单个字符的 ACPR-CJ1 最小转义形式：`"`、`\` 与 `< U+0020` 的控制字符（小写十六进制 `\u00xx`），
/// 其余字符原样输出（非 ASCII 就是它的 UTF-8 字节）。
fn write_escaped_char(ch: char, out: &mut String) {
    match ch {
        '"' => out.push_str("\\\""),
        '\\' => out.push_str("\\\\"),
        control if control < '\u{20}' => {
            let code = control as usize;
            out.push_str("\\u00");
            out.push(HEX_DIGITS[code >> 4]);
            out.push(HEX_DIGITS[code & 0xf]);
        }
        normal => out.push(normal),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 规范化成功时返回输出；失败时把错误交给调用方断言。
    fn ok(text: &str) -> String {
        match canonicalize(text) {
            Ok(out) => out,
            Err(error) => panic!("expected {text} to canonicalize, got {error}"),
        }
    }

    #[test]
    fn sorts_members_and_strips_whitespace_at_any_depth() {
        assert_eq!(ok("{}"), "{}");
        assert_eq!(ok(" \t\r\n{ }  "), "{}");
        assert_eq!(
            ok("{ \"z\" : [ 1 , true , null ] , \"a\" : { \"y\" : 2 , \"x\" : 3 } }"),
            r#"{"a":{"x":3,"y":2},"z":[1,true,null]}"#
        );
        assert_eq!(
            ok(r#"{"a":[{"b":[]},[{"c":1}]]}"#),
            r#"{"a":[{"b":[]},[{"c":1}]]}"#
        );
    }

    /// §3.3 的顺序是 **UTF-16 code unit** 序，不是码位序：U+10000（代理对 D800 DC00）排在 U+E000 之前。
    /// 期望值是把 `scripts/check-contract-assets.mjs` 里的 `acprCj1` 原样取出来、在同一输入上跑出的结果
    /// （U+10000 在 Rust 字面量里写作 `\u{10000}`）。
    #[test]
    fn orders_members_by_utf16_code_unit_not_code_point() {
        let expected = "{\"\u{10000}\":2,\"\u{e000}\":1}";
        assert_eq!(ok(r#"{"\ue000":1,"\ud800\udc00":2}"#), expected);
        // 输入顺序不影响输出顺序。
        assert_eq!(ok(r#"{"\ud800\udc00":2,"\ue000":1}"#), expected);
        // 对照：码位序会把 U+E000 排在前面，而 U+E000 之后的 BMP 字符（U+FFFF）同样在代理对之后。
        assert_eq!(
            ok(r#"{"\uffff":1,"\ud800\udc00":2}"#),
            "{\"\u{10000}\":2,\"\u{ffff}\":1}"
        );
    }

    /// 与参考实现的 `cj1String` 同一口径（`"`、`\` 与小写 `\u00xx` 的控制字符，其余原样输出；
    /// 未转义的控制字符属于语法错误）。期望值是把脚本里的 `cj1String`/`acprCj1` 原样取出来、在同一输入上
    /// 跑出的结果。
    #[test]
    fn escapes_minimally_and_keeps_non_ascii_utf8() {
        // `"`、`\` 转义；`\t`(U+0009)、U+0001、`\n`(U+000A)、U+0000 一律小写 `\u00xx`（§3.3 的措辞：
        // 控制字符写作小写十六进制）；U+007F 与所有非 ASCII 不属于「< U+0020」，原样输出。
        assert_eq!(
            ok(r#"{"a":"\"\\\u0001\t\n\u007f\u4e2d\ud83d\ude00\u0000"}"#),
            "{\"a\":\"\\\"\\\\\\u0001\\u0009\\u000a\u{7f}\u{4e2d}\u{1f600}\\u0000\"}"
        );
        // 成员名用同一套转义。
        assert_eq!(ok(r#"{"\u0001\"":1}"#), r#"{"\u0001\"":1}"#);
        // 合法代理对解码成非 ASCII 原样输出；`\/` 不是控制字符，按最小转义不保留反斜杠。
        assert_eq!(ok(r#"{"a":"\ud83d\ude00\/"}"#), "{\"a\":\"\u{1f600}/\"}");
    }

    /// §3.3 的「顶层必须是 object 或 array」：数组是合法顶层，元素照常规范化（含嵌套对象的成员排序）。
    #[test]
    fn accepts_array_top_level_and_normalizes_elements() {
        assert_eq!(ok("[]"), "[]");
        assert_eq!(ok("  [ ]  "), "[]");
        assert_eq!(
            ok(r#"[ { "b" : 1 , "a" : 2 } , [ 3 , { "d" : true , "c" : null } ] , "x" , 1.0 ]"#),
            r#"[{"a":2,"b":1},[3,{"c":null,"d":true}],"x",1]"#
        );
        // 顶层数组里的成员名同样按 UTF-16 code unit 排序，嵌套结构也一并规范化。
        assert_eq!(
            ok(r#"[{"\ue000":1,"\ud800\udc00":2}]"#),
            "[{\"\u{10000}\":2,\"\u{e000}\":1}]"
        );
        // 幂等：数组顶层同样稳定。
        let once = ok(r#"[{"b":1,"a":[{"d":1,"c":2}]}]"#);
        assert_eq!(once, r#"[{"a":[{"c":2,"d":1}],"b":1}]"#);
        assert_eq!(ok(&once), once);
    }

    #[test]
    fn rejects_scalar_top_level_but_only_after_the_text_parses() {
        for (text, found) in [
            ("1", JsonKind::Number),
            ("42", JsonKind::Number),
            ("1e2", JsonKind::Number),
            (r#""x""#, JsonKind::String),
            ("null", JsonKind::Null),
            ("true", JsonKind::Bool),
            ("false", JsonKind::Bool),
        ] {
            match canonicalize(text) {
                Err(Cj1Error::NotAContainer { found: actual }) => {
                    assert_eq!(actual, found, "{text}")
                }
                other => panic!("{text}: expected NotAContainer, got {other:?}"),
            }
        }
        // 不是良构 JSON 的顶层先报语法错误（类型错误只描述良构输入）。
        assert_eq!(canonicalize("[1,"), Err(Cj1Error::Syntax { offset: 3 }));
        assert_eq!(canonicalize("nul"), Err(Cj1Error::Syntax { offset: 3 }));
        // 浮点顶层仍然先按数字规则被拒，而不是「不是容器」。
        assert_eq!(
            canonicalize("1.5"),
            Err(Cj1Error::NonInteger {
                literal: "1.5".into()
            })
        );
    }

    #[test]
    fn rejects_duplicate_member_names_at_any_depth() {
        assert_eq!(
            canonicalize(r#"{"a":1,"a":2}"#),
            Err(Cj1Error::DuplicateKey { name: "a".into() })
        );
        // 值相同也不算合法：不得静默覆盖。
        assert_eq!(
            canonicalize(r#"{"a":1,"a":1}"#),
            Err(Cj1Error::DuplicateKey { name: "a".into() })
        );
        assert_eq!(
            canonicalize(r#"{"o":{"p":{"b":1,"b":2}}}"#),
            Err(Cj1Error::DuplicateKey { name: "b".into() })
        );
        assert_eq!(
            canonicalize(r#"{"a":[{"c":1,"c":2},{"d":1}]}"#),
            Err(Cj1Error::DuplicateKey { name: "c".into() })
        );
        // 转义写法与直写是同一个成员名。
        assert_eq!(
            canonicalize(r#"{"\u0061":1,"a":2}"#),
            Err(Cj1Error::DuplicateKey { name: "a".into() })
        );
        // 大小写不同是不同的成员名。
        assert_eq!(ok(r#"{"a":1,"A":2}"#), r#"{"A":2,"a":1}"#);
    }

    #[test]
    fn rejects_floats_and_numbers_outside_the_safe_integer_range() {
        for text in [
            "1.5",
            "-0.5",
            "[0.1]",
            r#"{"a":1e-1}"#,
            "[1.5,2]",
            "1.0000001e1",
            "0.01",
        ] {
            match canonicalize(text) {
                Err(Cj1Error::NonInteger { .. }) => {}
                other => panic!("{text}: expected NonInteger, got {other:?}"),
            }
        }
        for text in [
            "9007199254740992",
            "9007199254740993",
            "-9007199254740992",
            "18446744073709551616",
            "100000000000000000000000",
            "1e400",
            "1e309",
            "1e21",
        ] {
            match canonicalize(text) {
                Err(Cj1Error::IntegerOutOfRange { .. }) => {}
                other => panic!("{text}: expected IntegerOutOfRange, got {other:?}"),
            }
        }
        // 上界本身与常见的整数字面量都接受。
        assert_eq!(ok(r#"{"a":9007199254740991}"#), r#"{"a":9007199254740991}"#);
        assert_eq!(
            ok(r#"{"a":-9007199254740991}"#),
            r#"{"a":-9007199254740991}"#
        );
        assert_eq!(ok(r#"{"a":[0,-0,0]}"#), r#"{"a":[0,0,0]}"#);
    }

    /// 数字按**值**判定，与 JS 参考实现逐字面量一致：`1.0`/`1e2` 写成整数、`-0` 写成 `0`。期望值是把
    /// `scripts/check-contract-assets.mjs` 里的 `acprCj1` 原样取出来、在同一输入上跑出的结果。
    #[test]
    fn normalizes_integral_number_spellings_like_the_js_reference() {
        assert_eq!(
            ok(r#"{"a":1.0,"b":1e2,"c":-0,"d":-0.0,"e":2e0,"f":1e+2}"#),
            r#"{"a":1,"b":100,"c":0,"d":0,"e":2,"f":100}"#
        );
    }

    #[test]
    fn rejects_malformed_numbers_and_structures() {
        for (text, offset) in [
            ("01", 1),
            ("1.", 2),
            (".5", 0),
            ("+1", 0),
            ("1e", 2),
            ("-", 1),
            ("1e+", 3),
            ("1ee2", 2),
            ("[1,]", 3),
            ("[,1]", 1),
            ("{,] ", 1),
            (r#"{"a":}"#, 5),
            (r#"{"a" 1}"#, 5),
            (r#"{"a":1"#, 6),
            (r#"{"a":1,"#, 7),
            (r#"{"a":1}}"#, 7),
            ("{}{}", 2),
            (r#"{"a":1}x"#, 7),
            ("{]", 1),
            ("tru", 3),
            ("falsy", 4),
        ] {
            match canonicalize(text) {
                Err(Cj1Error::Syntax { offset: actual }) => assert_eq!(actual, offset, "{text}"),
                other => panic!("{text}: expected Syntax, got {other:?}"),
            }
        }
    }

    #[test]
    fn rejects_bad_escapes_and_lone_surrogates() {
        for text in [
            r#"{"a":"\x"}"#,
            r#"{"a":"\u00g0"}"#,
            r#"{"a":"\u"}"#,
            r#"{"a":"\u00"}"#,
            r#"{"a":"\U0041"}"#,
            r#"{"a":"\ud800"}"#,
            r#"{"a":"\udc00"}"#,
            r#"{"a":"\ud800\u0041"}"#,
            r#"{"a":"\ud800\u0400"}"#,
            r#"{"a":"#,
            r#"{"a":1"#,
        ] {
            assert!(
                canonicalize(text).is_err(),
                "{text} must be rejected as invalid JSON or invalid Unicode"
            );
        }
        // 孤立代理项与「转义写坏了」是两个不同的变体。
        assert_eq!(
            canonicalize(r#"{"a":"\ud800"}"#),
            Err(Cj1Error::InvalidUnicodeEscape { offset: 6 })
        );
        assert_eq!(
            canonicalize(r#"{"a":"\u00g0"}"#),
            Err(Cj1Error::Syntax { offset: 10 })
        );
    }

    #[test]
    fn rejects_unescaped_control_characters_in_strings() {
        assert_eq!(
            canonicalize("{\"a\":\"\u{1}\"}"),
            Err(Cj1Error::ControlCharacter { offset: 6, byte: 1 })
        );
        // 裸制表符在 JSON 字符串里同样非法（`\t` 只在字符串外算空白）。
        assert_eq!(
            canonicalize("{\"a\":\"\t\"}"),
            Err(Cj1Error::ControlCharacter { offset: 6, byte: 9 })
        );
        // 转义写法合法，并按小写 `\u00xx` 写回。
        assert_eq!(ok(r#"{"a":"\u0009"}"#), r#"{"a":"\u0009"}"#);
    }

    #[test]
    fn rejects_bom_and_invalid_utf8() {
        assert_eq!(canonicalize("\u{feff}{}"), Err(Cj1Error::Bom));
        // 字符串内部的 U+FEFF 不是 BOM（只有文本开头那个才是）。
        assert_eq!(ok("{\"a\":\"\u{feff}\"}"), "{\"a\":\"\u{feff}\"}");
        assert!(matches!(
            canonicalize_bytes(b"\xef\xbb\xbf{}"),
            Err(Cj1Error::Bom)
        ));
        assert!(matches!(
            canonicalize_bytes(b"{\xff}"),
            Err(Cj1Error::InvalidUtf8(_))
        ));
        assert_eq!(
            canonicalize_bytes(r#"{"a":"处理中"}"#.as_bytes()).unwrap(),
            r#"{"a":"处理中"}"#
        );
    }

    #[test]
    fn caps_nesting_depth() {
        let nested =
            |depth: usize| format!(r#"{{"a":{}{}}}"#, "[".repeat(depth), "]".repeat(depth));
        assert!(canonicalize(&nested(MAX_DEPTH as usize - 1)).is_ok());
        assert_eq!(
            canonicalize(&nested(MAX_DEPTH as usize)),
            Err(Cj1Error::TooDeep { max: MAX_DEPTH })
        );
    }

    #[test]
    fn is_idempotent() {
        for text in [
            r#"{"b":[1,{"d":true,"c":null}],"a":"文本","\u10000":"\ud800\udc00"}"#,
            r#"{}"#,
            r#"{"a":[]}"#,
            r#"{"a":1,"b":100,"c":0}"#,
            r#"{"a":"\"\\\u0001\u0009\u000a\u007f"}"#,
        ] {
            let once = canonicalize(text).unwrap_or_else(|error| panic!("{text}: {error}"));
            let twice = canonicalize(&once).unwrap_or_else(|error| panic!("{once}: {error}"));
            assert_eq!(once, twice, "canonicalizing {text} twice must be stable");
            assert_eq!(
                canonicalize_bytes(once.as_bytes()),
                Ok(once.clone()),
                "{once} must be valid UTF-8 on the byte entry point too"
            );
        }
    }
}
