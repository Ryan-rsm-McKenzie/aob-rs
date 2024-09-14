#![warn(clippy::pedantic)]
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions
)]

mod error;
mod needle;
mod parsing;
mod pattern;
mod prefilter;

mod private {
    pub trait Sealed {}
}

pub use error::{
    Error,
    Reason,
};
pub use needle::{
    DynamicNeedle,
    Match,
    Needle,
    StaticNeedle,
};
#[doc(hidden)]
pub use prefilter::RawPrefilter;
use private::Sealed;
