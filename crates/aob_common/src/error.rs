use std::{
    fmt::{
        self,
        Display,
    },
    ops::Range,
};

#[derive(Clone, Copy, Debug)]
pub(crate) struct Span {
    pub(crate) start: usize,
    pub(crate) end: usize,
}

impl From<Range<usize>> for Span {
    fn from(value: Range<usize>) -> Self {
        Self {
            start: value.start,
            end: value.end,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Error<'a> {
    pub(crate) source: &'a str,
    pub(crate) span: Span,
}

impl Display for Error<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "error while parsing pattern \"{}\" in range [{}, {})",
            &self.source[self.span.start..self.span.end],
            self.span.start,
            self.span.end,
        )
    }
}

impl std::error::Error for Error<'_> {}
