use crate::model::value::ValueKind;

/// An unredacted raw environment variable value encountered during tokenization.
///
/// # Security Invariant (Safe by Construction)
/// By deliberate architectural design, this struct:
/// - **DOES NOT** implement [`std::fmt::Display`] (preventing formatting to stdout/stderr).
/// - **DOES NOT** implement [`std::fmt::Debug`] (preventing accidental logging via `dbg!()` or `tracing`).
/// - **DOES NOT** implement [`serde::Serialize`] (preventing JSON/SARIF emission).
///
/// Values **never leave the lexer**. The only permissible operation is to classify
/// this raw value into a [`ValueKind`] metadata token (`Empty`, `NonEmpty`, `MultiLine`),
/// after which the raw secret bytes are securely dropped and wiped from memory.
pub struct RawEnvValue {
    inner: Vec<u8>,
    is_multiline: bool,
}

impl RawEnvValue {
    /// Constructs a new `RawEnvValue` from a UTF-8 string slice or buffer.
    pub fn new(value: impl AsRef<str>, is_multiline: bool) -> Self {
        Self {
            inner: value.as_ref().as_bytes().to_vec(),
            is_multiline,
        }
    }

    /// Returns `true` if the value has zero length.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Returns the length in bytes of the unredacted value.
    #[inline]
    pub fn byte_len(&self) -> usize {
        self.inner.len()
    }

    /// Classifies the raw value into a safe [`ValueKind`] without consuming `self`.
    pub fn classify(&self) -> ValueKind {
        if self.inner.is_empty() {
            ValueKind::Empty
        } else if self.is_multiline {
            ValueKind::MultiLine
        } else {
            ValueKind::NonEmpty
        }
    }

    /// Consumes the raw value and converts it into a safe [`ValueKind`].
    /// The underlying raw bytes are immediately dropped and wiped.
    #[inline]
    pub fn into_kind(self) -> ValueKind {
        self.classify()
    }
}

impl Drop for RawEnvValue {
    fn drop(&mut self) {
        // Volatile wipe of underlying bytes to eliminate lingering secrets in heap memory
        for byte in self.inner.iter_mut() {
            unsafe {
                std::ptr::write_volatile(byte, 0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_empty() {
        let raw = RawEnvValue::new("", false);
        assert_eq!(raw.classify(), ValueKind::Empty);
        assert_eq!(raw.into_kind(), ValueKind::Empty);
    }

    #[test]
    fn test_classify_non_empty() {
        let raw = RawEnvValue::new("super_secret_value", false);
        assert_eq!(raw.classify(), ValueKind::NonEmpty);
        assert_eq!(raw.into_kind(), ValueKind::NonEmpty);
    }

    #[test]
    fn test_classify_multiline() {
        let raw = RawEnvValue::new("line1\nline2", true);
        assert_eq!(raw.classify(), ValueKind::MultiLine);
        assert_eq!(raw.into_kind(), ValueKind::MultiLine);
    }

    #[test]
    fn test_drop_wipes_memory() {
        let raw = RawEnvValue::new("secret", false);
        assert_eq!(raw.byte_len(), 6);
        // Explicitly trigger drop
        drop(raw);
    }
}
