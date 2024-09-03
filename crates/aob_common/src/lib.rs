#![warn(clippy::pedantic)]
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions
)]

mod error;
mod needle;
mod parsing;

mod private {
    pub trait Sealed {}
}

#[doc(hidden)]
pub use chumsky::{
    error::Simple,
    Parser,
};
pub use error::Error;
pub use needle::{
    DynamicNeedle,
    Needle,
    StaticNeedle,
};
#[doc(hidden)]
pub use parsing::ida_pattern;
use private::Sealed;
