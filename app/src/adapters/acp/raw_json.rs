//! Lossless JSON with flat ownership: unknown subtrees never become a recursive value tree.
use std::{fmt, ops::Range, sync::Arc};

mod scan;

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RawJsonError {
    #[error("JSON is invalid")]
    InvalidJson,
    #[error("JSON contains a duplicate object field")]
    DuplicateField,
    #[error("JSON contains invalid Unicode")]
    InvalidUnicode,
    #[error("JSON value has a different type")]
    WrongType,
    #[error("JSON allocation failed")]
    Allocation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RawKind {
    Object,
    Array,
    String,
    Number,
    Boolean,
    Null,
}

struct Member {
    key: String,
    value: usize,
}

enum Content {
    Object(Vec<Member>),
    Array(Vec<usize>),
    String,
    Number,
    Boolean(bool),
    Null,
}

struct Node {
    range: Range<usize>,
    content: Content,
}

impl Node {
    const fn kind(&self) -> RawKind {
        match self.content {
            Content::Object(_) => RawKind::Object,
            Content::Array(_) => RawKind::Array,
            Content::String => RawKind::String,
            Content::Number => RawKind::Number,
            Content::Boolean(_) => RawKind::Boolean,
            Content::Null => RawKind::Null,
        }
    }
}

struct Inner {
    text: String,
    // Children are indices, never owned Nodes. Dropping this vector is not depth-recursive.
    nodes: Vec<Node>,
}

#[derive(Clone)]
pub struct RawDocument {
    inner: Arc<Inner>,
}

impl RawDocument {
    /// Validates one complete value while preserving every decoded member and raw fragment.
    /// # Errors
    /// Returns only static error categories; input and parser diagnostics are never attached.
    pub fn parse(bytes: Vec<u8>) -> Result<Self, RawJsonError> {
        let text = String::from_utf8(bytes).map_err(|_| RawJsonError::InvalidJson)?;
        let _: &serde_json::value::RawValue =
            serde_json::from_str(&text).map_err(|_| RawJsonError::InvalidJson)?;
        let nodes = scan::index(&text)?;
        Ok(Self {
            inner: Arc::new(Inner { text, nodes }),
        })
    }

    /// Checks the existing strict Node codecs without changing opaque ACP parsing.
    /// # Errors
    /// Rejects repeated decoded names or a failed allocation; no input is included.
    pub fn require_unique_fields(&self) -> Result<(), RawJsonError> {
        for node in &self.inner.nodes {
            if let Content::Object(members) = &node.content {
                let mut names = std::collections::HashSet::new();
                names
                    .try_reserve(members.len())
                    .map_err(|_| RawJsonError::Allocation)?;
                for member in members {
                    if !names.insert(member.key.as_str()) {
                        return Err(RawJsonError::DuplicateField);
                    }
                }
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        self.inner.text.as_bytes()
    }

    #[must_use]
    pub const fn root(&self) -> RawValueRef<'_> {
        RawValueRef {
            inner: &self.inner,
            index: 0,
        }
    }

    #[must_use]
    pub fn root_owned(&self) -> OwnedRawValue {
        self.root().to_owned()
    }
}

impl fmt::Debug for RawDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RawDocument")
            .field("bytes", &self.inner.text.len())
            .field("nodes", &self.inner.nodes.len())
            .finish_non_exhaustive()
    }
}

#[derive(Clone)]
pub struct OwnedRawValue {
    inner: Arc<Inner>,
    index: usize,
}

impl OwnedRawValue {
    #[must_use]
    pub const fn as_ref(&self) -> RawValueRef<'_> {
        RawValueRef {
            inner: &self.inner,
            index: self.index,
        }
    }
}

impl fmt::Debug for OwnedRawValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OwnedRawValue")
            .field("kind", &self.as_ref().kind())
            .field("bytes", &self.as_ref().fragment().len())
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy)]
pub struct RawValueRef<'a> {
    inner: &'a Arc<Inner>,
    index: usize,
}

impl<'a> RawValueRef<'a> {
    fn node(self) -> &'a Node {
        // Only the validated indexer constructs these private indices and UTF-8 ranges.
        &self.inner.nodes[self.index]
    }

    #[must_use]
    pub fn kind(self) -> RawKind {
        self.node().kind()
    }

    #[must_use]
    pub fn fragment(self) -> &'a str {
        &self.inner.text[self.node().range.clone()]
    }

    #[must_use]
    pub fn object(self) -> Option<RawObject<'a>> {
        match &self.node().content {
            Content::Object(members) => Some(RawObject {
                inner: self.inner,
                members,
            }),
            _ => None,
        }
    }

    #[must_use]
    pub fn array(self) -> Option<RawArray<'a>> {
        match &self.node().content {
            Content::Array(elements) => Some(RawArray {
                inner: self.inner,
                elements,
            }),
            _ => None,
        }
    }

    /// Decodes only this string, without parsing any containing object or unknown subtree.
    /// # Errors
    /// Rejects non-string values or a decoding failure without including the value.
    pub fn string(self) -> Result<String, RawJsonError> {
        if self.kind() != RawKind::String {
            return Err(RawJsonError::WrongType);
        }
        serde_json::from_str(self.fragment()).map_err(|_| RawJsonError::InvalidJson)
    }

    #[must_use]
    pub fn boolean(self) -> Option<bool> {
        match self.node().content {
            Content::Boolean(value) => Some(value),
            _ => None,
        }
    }

    #[must_use]
    pub fn integer(self) -> Option<i64> {
        (self.kind() == RawKind::Number)
            .then(|| self.fragment().parse().ok())
            .flatten()
    }

    #[must_use]
    pub fn unsigned(self) -> Option<u64> {
        (self.kind() == RawKind::Number && !self.fragment().starts_with('-'))
            .then(|| self.fragment().parse().ok())
            .flatten()
    }

    #[must_use]
    pub fn is_null(self) -> bool {
        self.kind() == RawKind::Null
    }

    #[must_use]
    pub fn to_owned(self) -> OwnedRawValue {
        OwnedRawValue {
            inner: Arc::clone(self.inner),
            index: self.index,
        }
    }

    /// Removes only JSON whitespace outside strings; original storage remains unchanged.
    /// # Errors
    /// Reports a checked allocation failure without including source text.
    pub fn compact(self) -> Result<String, RawJsonError> {
        let fragment = self.fragment();
        let mut result = Vec::new();
        result
            .try_reserve_exact(fragment.len())
            .map_err(|_| RawJsonError::Allocation)?;
        let mut in_string = false;
        let mut escaped = false;
        for byte in fragment.bytes() {
            if in_string {
                result.push(byte);
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'"' {
                    in_string = false;
                }
            } else if byte == b'"' {
                in_string = true;
                result.push(byte);
            } else if !scan::whitespace(byte) {
                result.push(byte);
            }
        }
        String::from_utf8(result).map_err(|_| RawJsonError::InvalidJson)
    }
}

impl fmt::Debug for RawValueRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RawValueRef")
            .field("kind", &self.kind())
            .field("bytes", &self.fragment().len())
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy)]
pub struct RawObject<'a> {
    inner: &'a Arc<Inner>,
    members: &'a [Member],
}

impl<'a> RawObject<'a> {
    #[must_use]
    pub const fn len(self) -> usize {
        self.members.len()
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.members.is_empty()
    }

    /// # Errors
    /// Rejects multiple occurrences of the queried decoded name.
    pub fn get(self, key: &str) -> Result<Option<RawValueRef<'a>>, RawJsonError> {
        let mut found = None;
        for member in self.members.iter().filter(|member| member.key == key) {
            if found.is_some() {
                return Err(RawJsonError::DuplicateField);
            }
            found = Some(RawValueRef {
                inner: self.inner,
                index: member.value,
            });
        }
        Ok(found)
    }

    #[must_use]
    pub fn iter(
        self,
    ) -> impl ExactSizeIterator<Item = (&'a str, RawValueRef<'a>)> + DoubleEndedIterator + 'a {
        self.members.iter().map(move |member| {
            (
                member.key.as_str(),
                RawValueRef {
                    inner: self.inner,
                    index: member.value,
                },
            )
        })
    }
}

impl fmt::Debug for RawObject<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RawObject")
            .field("members", &self.len())
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy)]
pub struct RawArray<'a> {
    inner: &'a Arc<Inner>,
    elements: &'a [usize],
}

impl<'a> RawArray<'a> {
    #[must_use]
    pub const fn len(self) -> usize {
        self.elements.len()
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.elements.is_empty()
    }

    #[must_use]
    pub fn get(self, index: usize) -> Option<RawValueRef<'a>> {
        self.elements.get(index).map(|index| RawValueRef {
            inner: self.inner,
            index: *index,
        })
    }

    #[must_use]
    pub fn iter(self) -> impl ExactSizeIterator<Item = RawValueRef<'a>> + DoubleEndedIterator + 'a {
        self.elements.iter().map(move |index| RawValueRef {
            inner: self.inner,
            index: *index,
        })
    }
}

impl fmt::Debug for RawArray<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RawArray")
            .field("elements", &self.len())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests;
