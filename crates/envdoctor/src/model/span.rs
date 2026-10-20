use crate::model::path::RelPath;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Represents a source code span tracking line, column, and byte offsets.
///
/// Lines and columns are 1-based human-readable coordinates.
/// Offsets are 0-based byte indices into UTF-8 source text.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Span {
    /// 1-based line index.
    pub line: u32,
    /// 1-based UTF-8 character column index.
    pub column: u32,
    /// 0-based starting byte offset.
    pub start_offset: u32,
    /// 0-based ending byte offset (exclusive).
    pub end_offset: u32,
}

impl Span {
    /// Constructs a new `Span` with explicit coordinates.
    pub const fn new(line: u32, column: u32, start_offset: u32, end_offset: u32) -> Self {
        Self {
            line,
            column,
            start_offset,
            end_offset,
        }
    }

    /// Constructs a zero-width point span at given line and column.
    pub const fn point(line: u32, column: u32, offset: u32) -> Self {
        Self {
            line,
            column,
            start_offset: offset,
            end_offset: offset,
        }
    }

    /// Byte offset where this span begins.
    #[inline]
    pub const fn offset(&self) -> u32 {
        self.start_offset
    }

    /// Byte length of this span.
    #[inline]
    pub const fn len(&self) -> u32 {
        self.end_offset.saturating_sub(self.start_offset)
    }

    /// Returns `true` if this span has length 0.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl fmt::Debug for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Span({}:{}-{}: [{}..{}])",
            self.line, self.column, self.line, self.start_offset, self.end_offset
        )
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.column)
    }
}

/// A combined source location containing relative file path and span.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Location {
    pub path: RelPath,
    pub span: Span,
}

impl Location {
    /// Constructs a new `Location`.
    pub fn new(path: impl Into<RelPath>, span: Span) -> Self {
        Self {
            path: path.into(),
            span,
        }
    }
}

impl fmt::Debug for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Location({}:{})", self.path, self.span)
    }
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.path, self.span)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_metrics() {
        let span = Span::new(10, 4, 100, 115);
        assert_eq!(span.line, 10);
        assert_eq!(span.column, 4);
        assert_eq!(span.offset(), 100);
        assert_eq!(span.len(), 15);
        assert!(!span.is_empty());

        let pt = Span::point(1, 1, 0);
        assert!(pt.is_empty());
        assert_eq!(pt.len(), 0);
    }

    #[test]
    fn test_location_display() {
        let loc = Location::new("src/index.ts", Span::new(42, 5, 200, 210));
        assert_eq!(loc.to_string(), "src/index.ts:42:5");
    }
}

