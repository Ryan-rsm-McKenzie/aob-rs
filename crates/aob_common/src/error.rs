use chumsky::error::SimpleReason;
use std::{
    fmt::{
        self,
        Display,
        Formatter,
    },
    ops::Range,
};

#[derive(Clone, Debug)]
pub enum Reason {
    Unexpected,
    Unclosed,
    Custom(String),
}

impl Reason {
    pub(crate) fn new<I, S>(reason: &SimpleReason<I, S>) -> Self {
        match reason {
            SimpleReason::Unexpected => Self::Unexpected,
            SimpleReason::Unclosed {
                span: _,
                delimiter: _,
            } => Self::Unclosed,
            SimpleReason::Custom(custom) => Self::Custom(custom.clone()),
        }
    }
}

impl Display for Reason {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let what = match self {
            Self::Unexpected => "unexpected input",
            Self::Unclosed => "unclosed delimiter",
            Self::Custom(custom) => custom,
        };
        write!(f, "{what}")
    }
}

/// Describes errors encountered when parsing custom pattern syntax
#[derive(Clone, Debug)]
pub struct Error<'a> {
    pub(crate) source: &'a str,
    pub(crate) span: Range<usize>,
    pub(crate) reason: Reason,
}

impl<'a> Error<'a> {
    #[must_use]
    pub fn span(&self) -> Range<usize> {
        self.span.clone()
    }

    #[must_use]
    pub fn reason(&self) -> &Reason {
        &self.reason
    }
}

impl Display for Error<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let reason = &self.reason;
        let start = self.span.start;
        let end = self.span.end;
        let pattern = &self.source[start..end];
        write!(
            f,
            "'{reason}' while parsing pattern \"{pattern}\" in range [{start}, {end})",
        )
    }
}

impl std::error::Error for Error<'_> {}
