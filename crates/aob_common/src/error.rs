use std::{
    fmt::{
        self,
        Display,
        Formatter,
    },
    ops::Range,
};

/// A [`Reason`] gives more context about why parsing failed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Reason {
    /// Encountered an unexpected input in the stream.
    Unexpected,
    /// Encountered an unclosed set of delimiters.
    Unclosed,
    /// The given character is not a valid hexdigit.
    InvalidHexdigit(char),
}

impl Display for Reason {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unexpected => write!(f, "unexpected input"),
            Self::Unclosed => write!(f, "unclosed delimiter"),
            Self::InvalidHexdigit(c) => write!(f, "'{c}' is not a hexdigit"),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SimpleError {
    pub(crate) span: Range<usize>,
    pub(crate) reason: Reason,
}

impl SimpleError {
    pub(crate) fn invalid_hexdigit(span: Range<usize>, found: char) -> Self {
        Self {
            span,
            reason: Reason::InvalidHexdigit(found),
        }
    }
}

impl chumsky::Error<char> for SimpleError {
    type Span = Range<usize>;
    type Label = &'static str;

    fn expected_input_found<Iter: IntoIterator<Item = Option<char>>>(
        span: Self::Span,
        _expected: Iter,
        _found: Option<char>,
    ) -> Self {
        Self {
            span,
            reason: Reason::Unexpected,
        }
    }

    fn with_label(self, _label: Self::Label) -> Self {
        self
    }

    fn merge(self, _other: Self) -> Self {
        self
    }

    fn unclosed_delimiter(
        _unclosed_span: Self::Span,
        _unclosed: char,
        span: Self::Span,
        _expected: char,
        _found: Option<char>,
    ) -> Self {
        Self {
            span,
            reason: Reason::Unclosed,
        }
    }
}

/// Describes errors encountered when parsing custom pattern syntax.
#[derive(Clone, Debug)]
pub struct Error<'a> {
    pub(crate) source: &'a str,
    pub(crate) inner: SimpleError,
}

impl<'a> Error<'a> {
    /// The span over which the error was encountered.
    ///
    /// ```
    /// # use aob_common::DynamicNeedle;
    /// let error = DynamicNeedle::from_ida("12 3_ 56").unwrap_err();
    /// assert_eq!(error.span(), 4..5);
    /// ```
    #[must_use]
    pub fn span(&self) -> Range<usize> {
        self.inner.span.clone()
    }

    /// A human readable reason describing why the error occurred.
    ///
    /// ```
    /// # use aob_common::{DynamicNeedle, Reason};
    /// let error = DynamicNeedle::from_ida("12 3_ 56").unwrap_err();
    /// assert_eq!(error.reason(), &Reason::InvalidHexdigit('_'));
    /// ```
    #[must_use]
    pub fn reason(&self) -> &Reason {
        &self.inner.reason
    }
}

impl Display for Error<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let start = self.inner.span.start;
        let end = self.inner.span.end;
        let span = &self.source[start..end];
        write!(
            f,
            "error while parsing token \"{span}\" in range [{start}, {end})",
        )
    }
}

impl std::error::Error for Error<'_> {}
