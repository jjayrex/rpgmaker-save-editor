use std::fmt;

/// Everything that can go wrong while reading or writing a Ruby `Marshal` stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarshalError {
    /// Stream ended in the middle of a value.
    UnexpectedEof { pos: usize },
    /// Stream does not start with the 4.8 version header.
    BadMagic { major: u8, minor: u8 },
    /// A type byte we do not know how to decode.
    UnknownType { byte: u8, pos: usize },
    /// A symlink / object link pointing outside the table built so far.
    BadLink { index: i64, pos: usize },
    /// A length or index that cannot be represented on this platform.
    BadLength { value: i64, pos: usize },
    /// Structurally valid Marshal that this codec deliberately does not support.
    Unsupported { what: &'static str, pos: usize },
    /// `I` (instance-variable) prefix applied to a value that cannot carry ivars.
    IvarsOnImmediate { pos: usize },
}

impl fmt::Display for MarshalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEof { pos } => {
                write!(f, "unexpected end of marshal data at byte {pos}")
            }
            Self::BadMagic { major, minor } => write!(
                f,
                "not a Ruby Marshal stream: expected version 4.8, found {major}.{minor}"
            ),
            Self::UnknownType { byte, pos } => write!(
                f,
                "unknown marshal type byte {byte:#04x} ({:?}) at byte {pos}",
                *byte as char
            ),
            Self::BadLink { index, pos } => {
                write!(f, "marshal link {index} at byte {pos} is out of range")
            }
            Self::BadLength { value, pos } => {
                write!(f, "invalid marshal length {value} at byte {pos}")
            }
            Self::Unsupported { what, pos } => {
                write!(f, "unsupported marshal construct {what} at byte {pos}")
            }
            Self::IvarsOnImmediate { pos } => write!(
                f,
                "instance variables attached to a value that cannot hold them, at byte {pos}"
            ),
        }
    }
}

impl std::error::Error for MarshalError {}

pub type Result<T> = std::result::Result<T, MarshalError>;
