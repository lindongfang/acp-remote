use super::{Content, Member, Node, RawJsonError};

struct ObjectFrame {
    pending_key: Option<String>,
    expect_key: bool,
}

struct Frame {
    node: usize,
    object: Option<ObjectFrame>,
}

struct Scanner<'a> {
    text: &'a str,
    cursor: usize,
    nodes: Vec<Node>,
    stack: Vec<Frame>,
}

pub(super) fn index(text: &str) -> Result<Vec<Node>, RawJsonError> {
    Scanner {
        text,
        cursor: 0,
        nodes: Vec::new(),
        stack: Vec::new(),
    }
    .run()
}

pub(super) const fn whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}

fn push<T>(values: &mut Vec<T>, value: T) -> Result<(), RawJsonError> {
    values
        .try_reserve(1)
        .map_err(|_| RawJsonError::Allocation)?;
    values.push(value);
    Ok(())
}

impl Scanner<'_> {
    fn advance(&mut self, length: usize) -> Result<(), RawJsonError> {
        self.cursor = self
            .cursor
            .checked_add(length)
            .ok_or(RawJsonError::Allocation)?;
        Ok(())
    }

    fn run(mut self) -> Result<Vec<Node>, RawJsonError> {
        while let Some(byte) = self.text.as_bytes().get(self.cursor).copied() {
            match byte {
                b'{' | b'[' => self.open(byte == b'{')?,
                b'}' | b']' => self.close(byte == b'}')?,
                b'"' => self.string()?,
                b',' => {
                    if let Some(object) = self
                        .stack
                        .last_mut()
                        .and_then(|frame| frame.object.as_mut())
                    {
                        object.expect_key = true;
                    }
                    self.advance(1)?;
                }
                b':' | b' ' | b'\t' | b'\r' | b'\n' => self.advance(1)?,
                _ => self.primitive(byte)?,
            }
        }
        if self.nodes.is_empty() || !self.stack.is_empty() {
            return Err(RawJsonError::InvalidJson);
        }
        Ok(self.nodes)
    }

    fn attach(&mut self, node: Node) -> Result<usize, RawJsonError> {
        let index = self.nodes.len();
        if let Some(parent) = self.stack.last_mut() {
            let content = &mut self
                .nodes
                .get_mut(parent.node)
                .ok_or(RawJsonError::InvalidJson)?
                .content;
            match content {
                Content::Object(members) => {
                    let key = parent
                        .object
                        .as_mut()
                        .and_then(|object| object.pending_key.take())
                        .ok_or(RawJsonError::InvalidJson)?;
                    push(members, Member { key, value: index })?;
                }
                Content::Array(elements) => push(elements, index)?,
                _ => return Err(RawJsonError::InvalidJson),
            }
        } else if !self.nodes.is_empty() {
            return Err(RawJsonError::InvalidJson);
        }
        push(&mut self.nodes, node)?;
        Ok(index)
    }

    fn open(&mut self, object: bool) -> Result<(), RawJsonError> {
        let content = if object {
            Content::Object(Vec::new())
        } else {
            Content::Array(Vec::new())
        };
        let node = self.attach(Node {
            range: self.cursor..self.cursor,
            content,
        })?;
        push(
            &mut self.stack,
            Frame {
                node,
                object: object.then_some(ObjectFrame {
                    pending_key: None,
                    expect_key: true,
                }),
            },
        )?;
        self.advance(1)
    }

    fn close(&mut self, object: bool) -> Result<(), RawJsonError> {
        let frame = self.stack.pop().ok_or(RawJsonError::InvalidJson)?;
        if frame.object.is_some() != object {
            return Err(RawJsonError::InvalidJson);
        }
        self.advance(1)?;
        self.nodes
            .get_mut(frame.node)
            .ok_or(RawJsonError::InvalidJson)?
            .range
            .end = self.cursor;
        Ok(())
    }

    fn string(&mut self) -> Result<(), RawJsonError> {
        let start = self.cursor;
        self.cursor = string_end(self.text.as_bytes(), start)?;
        if let Some(object) = self
            .stack
            .last_mut()
            .and_then(|frame| frame.object.as_mut())
            && object.expect_key
        {
            let raw = self
                .text
                .get(start..self.cursor)
                .ok_or(RawJsonError::InvalidJson)?;
            let key: String = serde_json::from_str(raw).map_err(|_| RawJsonError::InvalidJson)?;
            object.pending_key = Some(key);
            object.expect_key = false;
        } else {
            self.attach(Node {
                range: start..self.cursor,
                content: Content::String,
            })?;
        }
        Ok(())
    }

    fn primitive(&mut self, first: u8) -> Result<(), RawJsonError> {
        let start = self.cursor;
        while let Some(byte) = self.text.as_bytes().get(self.cursor).copied() {
            if whitespace(byte) || matches!(byte, b',' | b']' | b'}') {
                break;
            }
            self.advance(1)?;
        }
        let content = match first {
            b't' => Content::Boolean(true),
            b'f' => Content::Boolean(false),
            b'n' => Content::Null,
            b'-' | b'0'..=b'9' => Content::Number,
            _ => return Err(RawJsonError::InvalidJson),
        };
        self.attach(Node {
            range: start..self.cursor,
            content,
        })?;
        Ok(())
    }
}

// Syntax is already validated. This pass also validates scalar escapes in every unknown value.
fn string_end(bytes: &[u8], start: usize) -> Result<usize, RawJsonError> {
    let mut cursor = start.checked_add(1).ok_or(RawJsonError::Allocation)?;
    loop {
        match bytes.get(cursor) {
            Some(b'"') => return cursor.checked_add(1).ok_or(RawJsonError::Allocation),
            Some(b'\\') => {
                cursor = cursor.checked_add(1).ok_or(RawJsonError::Allocation)?;
                if bytes.get(cursor) == Some(&b'u') {
                    cursor = unicode_escape(bytes, cursor)?;
                } else {
                    cursor = cursor.checked_add(1).ok_or(RawJsonError::Allocation)?;
                }
            }
            Some(_) => cursor = cursor.checked_add(1).ok_or(RawJsonError::Allocation)?,
            None => return Err(RawJsonError::InvalidJson),
        }
    }
}

fn hex_quad(bytes: &[u8], start: usize) -> Result<u16, RawJsonError> {
    let end = start.checked_add(4).ok_or(RawJsonError::Allocation)?;
    bytes
        .get(start..end)
        .ok_or(RawJsonError::InvalidJson)?
        .iter()
        .try_fold(0u16, |value, byte| {
            let digit = match byte {
                b'0'..=b'9' => u16::from(byte - b'0'),
                b'a'..=b'f' => u16::from(byte - b'a') + 10,
                b'A'..=b'F' => u16::from(byte - b'A') + 10,
                _ => return Err(RawJsonError::InvalidJson),
            };
            value
                .checked_mul(16)
                .and_then(|value| value.checked_add(digit))
                .ok_or(RawJsonError::InvalidJson)
        })
}

fn unicode_escape(bytes: &[u8], u_position: usize) -> Result<usize, RawJsonError> {
    let digits = u_position.checked_add(1).ok_or(RawJsonError::Allocation)?;
    let first = hex_quad(bytes, digits)?;
    let end = digits.checked_add(4).ok_or(RawJsonError::Allocation)?;
    match first {
        0xD800..=0xDBFF => {
            let pair_digits = end.checked_add(2).ok_or(RawJsonError::Allocation)?;
            if bytes.get(end..pair_digits) != Some(b"\\u") {
                return Err(RawJsonError::InvalidUnicode);
            }
            let second = hex_quad(bytes, pair_digits)?;
            if !(0xDC00..=0xDFFF).contains(&second) {
                return Err(RawJsonError::InvalidUnicode);
            }
            pair_digits.checked_add(4).ok_or(RawJsonError::Allocation)
        }
        0xDC00..=0xDFFF => Err(RawJsonError::InvalidUnicode),
        _ => Ok(end),
    }
}
