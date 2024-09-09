#![warn(clippy::pedantic)]

use aob_common::{
    DynamicNeedle,
    Needle as _,
};
use ariadne::{
    Config,
    Label,
    Report,
    ReportKind,
    Source,
};
use chumsky::{
    error::{
        Simple,
        SimpleReason,
    },
    primitive::end,
    Parser,
};
use proc_macro::TokenStream;
use proc_macro2::{
    Literal,
    Span,
    TokenStream as TokenStream2,
};
use quote::{
    ToTokens,
    TokenStreamExt as _,
};
use syn::{
    parenthesized,
    parse::{
        Parse,
        ParseStream,
        Result as ParseResult,
    },
    parse_macro_input,
    Ident,
    LitStr,
    Token,
    Visibility,
};

macro_rules! unsuffixed_primitive {
    ($type:ident: $primitive:ident => $method:ident) => {
        struct $type($primitive);

        impl ToTokens for $type {
            fn to_tokens(&self, tokens: &mut TokenStream2) {
                tokens.append(Literal::$method(self.0))
            }
        }

        impl From<$primitive> for $type {
            fn from(value: $primitive) -> Self {
                Self(value)
            }
        }
    };
}

unsuffixed_primitive!(UnsuffixedUsize: usize => usize_unsuffixed);
unsuffixed_primitive!(UnsuffixedU8: u8 => u8_unsuffixed);

enum Method {
    Ida,
}

impl TryFrom<Ident> for Method {
    type Error = syn::Error;

    fn try_from(value: Ident) -> Result<Self, Self::Error> {
        match value.to_string().as_str() {
            "ida" => Ok(Self::Ida),
            _ => Err(syn::Error::new(value.span(), "expected one of: `ida`")),
        }
    }
}

struct Aob {
    visibility: Visibility,
    name: Ident,
    method: Method,
    pattern: String,
}

impl Aob {
    #[must_use]
    fn into_tokens(self) -> TokenStream2 {
        let parse_result = match self.method {
            Method::Ida => aob_common::ida_pattern()
                .then_ignore(end())
                .parse(self.pattern.as_str()),
        };

        match parse_result {
            Ok(bytes) => self.tokenize_needle(&bytes),
            Err(errors) => self.tokenize_errors(&errors),
        }
    }

    #[must_use]
    fn tokenize_needle(&self, bytes: &[Option<u8>]) -> TokenStream2 {
        let needle = DynamicNeedle::from_bytes(bytes);
        let needle_len: UnsuffixedUsize = needle.len().into();
        let dfa = needle.serialize_dfa_with_target_endianness();
        let dfa_len: UnsuffixedUsize = dfa.len().into();
        let dfa: TokenStream2 = dfa
            .iter()
            .map(|&x| {
                let x = UnsuffixedU8(x);
                quote::quote!(#x,)
            })
            .collect();
        let Self {
            visibility, name, ..
        } = self;
        quote::quote! {
            #visibility const #name: ::aob_common::StaticNeedle<#dfa_len> = ::aob_common::StaticNeedle::new([#dfa], #needle_len);
        }
    }

    #[must_use]
    fn tokenize_errors(&self, errors: &[Simple<char>]) -> TokenStream2 {
        let error = errors.first().unwrap();
        let mut buffer = Vec::new();
        Report::build(ReportKind::Error, (), error.span().start)
            .with_config(Config::default().with_color(false))
            .with_message(error.to_string())
            .with_label(Label::new(error.span()).with_message(match error.reason() {
                SimpleReason::Unexpected => "unexpected input",
                SimpleReason::Unclosed {
                    span: _,
                    delimiter: _,
                } => "unclosed delimiter",
                SimpleReason::Custom(custom) => custom.as_str(),
            }))
            .finish()
            .write(Source::from(&self.pattern), &mut buffer)
            .unwrap();
        let error_message = String::from_utf8(buffer).unwrap();
        quote::quote_spanned!(Span::call_site() => compile_error!(#error_message))
    }
}

impl Parse for Aob {
    fn parse(input: ParseStream) -> ParseResult<Self> {
        let visibility = input.parse()?;
        input.parse::<Token![const]>()?;
        let name = input.parse()?;
        input.parse::<Token![=]>()?;
        let method = input.parse::<Ident>()?.try_into()?;
        let pattern = {
            let content;
            parenthesized!(content in input);
            content.parse::<LitStr>()?.value()
        };
        input.parse::<Token![;]>()?;
        Ok(Self {
            visibility,
            name,
            method,
            pattern,
        })
    }
}

/// Parses, validates, and constructs a [`Needle`](aob_common::Needle) at compile-time.
///
/// ## Syntax
/// Expects syntax of the form: `$VISIBILITY? const $IDENTIFIER = $METHOD("$PATTERN");`
///
/// With the following rules:
/// * `$VISIBILITY` is a valid [Visibility](<https://doc.rust-lang.org/reference/visibility-and-privacy.html>) token, or nothing.
/// * `$IDENTIFIER` is a valid [Identifier](<https://doc.rust-lang.org/reference/identifiers.html>) token.
/// * `$METHOD` is one of:
///   * `ida`.
/// * `$PATTERN` is a valid pattern whose syntax depends on the chosen `$METHOD`.
///
/// ## Example
/// ```
/// # use aob_macros::aob;
/// # use aob_common::Needle as _;
/// aob! {
///     const NEEDLE = ida("78 ? BC");
/// }
/// let haystack = [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE];
/// let matched = NEEDLE.find(&haystack).unwrap();
/// assert_eq!(matched.as_bytes(), [0x78, 0x9A, 0xBC]);
/// ```
#[proc_macro]
pub fn aob(input: TokenStream) -> TokenStream {
    parse_macro_input!(input as Aob).into_tokens().into()
}
