//! The error vocabulary of the port.
//!
//! Every fallible entry point returns [`SpiceResult`]. The important variant is
//! [`SpiceError::NotYetPorted`]: the scaffold is honest about what is missing,
//! and names the C code the future implementation must match. `todo!()` and
//! `unimplemented!()` are denied by the workspace clippy configuration.

use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Result alias used throughout the port.
pub type SpiceResult<T> = Result<T, SpiceError>;

/// A location inside a source file.
///
/// The file name is shared through an [`Arc`] because a location is attached to
/// every token, and cloning a `PathBuf` per token would dominate the
/// tokenizer's cost.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLoc {
    /// The file the text came from.
    pub file: Arc<PathBuf>,
    /// 1-based line number.
    pub line: u32,
    /// 1-based byte column within the logical line.
    pub column: u32,
}

impl SourceLoc {
    /// Builds a location.
    #[must_use]
    pub fn new(file: impl Into<Arc<PathBuf>>, line: u32, column: u32) -> Self {
        Self {
            file: file.into(),
            line,
            column,
        }
    }

    /// The file this location points into.
    #[must_use]
    pub fn path(&self) -> &Path {
        self.file.as_path()
    }

    /// The same location, moved to a different column.
    #[must_use]
    pub fn at_column(&self, column: u32) -> Self {
        Self {
            file: Arc::clone(&self.file),
            line: self.line,
            column,
        }
    }
}

impl fmt::Display for SourceLoc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.file.display(), self.line, self.column)
    }
}

/// Everything that can go wrong in the port.
///
/// `Io` keeps the message rather than the [`std::io::Error`] so that the error
/// type stays `Clone` and comparable in tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpiceError {
    /// Reading or writing a file failed.
    Io {
        /// The file involved.
        path: PathBuf,
        /// The underlying error, rendered.
        message: String,
    },
    /// A deck, a card or a result file could not be understood.
    Parse {
        /// Where the problem was found.
        location: SourceLoc,
        /// What was wrong.
        message: String,
    },
    /// A device designator the port does not recognise.
    UnknownDevice {
        /// The instance name as written, e.g. `x1`.
        name: String,
        /// Its designator letter, lowercased.
        designator: char,
        /// Where it was found.
        location: SourceLoc,
    },
    /// A construct the port recognises but does not intend to support.
    Unsupported {
        /// What was encountered.
        feature: String,
        /// Where it was encountered, when known.
        location: Option<SourceLoc>,
    },
    /// A construct with a known C implementation that has not been ported yet.
    ///
    /// This is the expected error from the scaffold. `spice-cli` maps it to exit
    /// status 3 so that scripts can tell "not ported" apart from "failed".
    NotYetPorted {
        /// The behaviour that is missing.
        what: String,
        /// The C file (and function, when useful) that defines it.
        c_reference: String,
    },
    /// The solver failed, diverged, or produced a non-finite value.
    Numerical {
        /// Which part of the solution failed.
        context: String,
        /// What went wrong.
        message: String,
    },
    /// The circuit as a whole is inconsistent. Unlike [`SpiceError::Parse`] this
    /// is not tied to a location in a deck: it is raised while building the
    /// node/device graph, for instance for a duplicate instance name.
    Circuit {
        /// What is wrong with the circuit.
        message: String,
    },
}

impl SpiceError {
    /// Wraps an I/O error with the path it happened on.
    #[must_use]
    pub fn io(path: impl AsRef<Path>, error: &std::io::Error) -> Self {
        Self::Io {
            path: path.as_ref().to_path_buf(),
            message: error.to_string(),
        }
    }

    /// Builds a parse error.
    #[must_use]
    pub fn parse(location: SourceLoc, message: impl Into<String>) -> Self {
        Self::Parse {
            location,
            message: message.into(),
        }
    }

    /// Builds a [`SpiceError::NotYetPorted`].
    #[must_use]
    pub fn not_yet_ported(what: impl Into<String>, c_reference: impl Into<String>) -> Self {
        Self::NotYetPorted {
            what: what.into(),
            c_reference: c_reference.into(),
        }
    }

    /// True when this error only means "not implemented yet".
    #[must_use]
    pub fn is_not_yet_ported(&self) -> bool {
        matches!(self, Self::NotYetPorted { .. })
    }

    /// Builds a [`SpiceError::Circuit`].
    #[must_use]
    pub fn circuit(message: impl Into<String>) -> Self {
        Self::Circuit {
            message: message.into(),
        }
    }
}

impl fmt::Display for SpiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, message } => write!(f, "{}: {message}", path.display()),
            Self::Parse { location, message } => write!(f, "{location}: {message}"),
            Self::UnknownDevice {
                name,
                designator,
                location,
            } => write!(
                f,
                "{location}: unknown device designator '{designator}' in instance '{name}'"
            ),
            Self::Unsupported { feature, location } => match location {
                Some(location) => write!(f, "{location}: unsupported: {feature}"),
                None => write!(f, "unsupported: {feature}"),
            },
            Self::NotYetPorted { what, c_reference } => {
                write!(f, "not yet ported: {what} (see {c_reference})")
            }
            Self::Numerical { context, message } => write!(f, "{context}: {message}"),
            Self::Circuit { message } => write!(f, "circuit: {message}"),
        }
    }
}

impl Error for SpiceError {}

#[cfg(test)]
mod tests {
    use super::{SourceLoc, SpiceError};
    use std::path::PathBuf;

    fn loc() -> SourceLoc {
        SourceLoc::new(PathBuf::from("deck.cir"), 4, 7)
    }

    #[test]
    fn display_includes_the_location() {
        let error = SpiceError::parse(loc(), "expected a value");
        assert_eq!(error.to_string(), "deck.cir:4:7: expected a value");
    }

    #[test]
    fn not_yet_ported_names_the_c_reference() {
        let error =
            SpiceError::not_yet_ported("resistor stamping", "src/spicelib/devices/res/res.c");
        assert!(error.is_not_yet_ported());
        let message = error.to_string();
        assert!(
            message.contains("src/spicelib/devices/res/res.c"),
            "{message}"
        );
    }

    #[test]
    fn locations_compare_by_value() {
        assert_eq!(loc(), loc());
        assert_ne!(loc(), loc().at_column(8));
    }
}
