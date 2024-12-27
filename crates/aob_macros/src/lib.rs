#![warn(clippy::pedantic)]

use aob_common::{
    DynamicNeedle,
    Error as AobError,
    Needle as _,
    RawPrefilter,
};
use ariadne::{
    Config,
    Label,
    Report,
    ReportKind,
    Source,
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
    Attribute,
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

struct AobDecl {
    attributes: Vec<Attribute>,
    visibility: Visibility,
    name: Ident,
    method: Method,
    pattern: String,
}

impl AobDecl {
    #[must_use]
    fn into_tokens(self) -> TokenStream2 {
        let parse_result = match self.method {
            Method::Ida => DynamicNeedle::from_ida(self.pattern.as_str()),
        };

        match parse_result {
            Ok(needle) => self.tokenize_needle(&needle),
            Err(error) => self.tokenize_error(&error),
        }
    }

    #[must_use]
    fn tokenize_needle(&self, needle: &DynamicNeedle) -> TokenStream2 {
        let needle_len: UnsuffixedUsize = needle.len().into();
        let prefilter = match needle.serialize_prefilter() {
            RawPrefilter::Length { len } => quote::quote! {
                ::aob_common::RawPrefilter::Length {
                    len: #len
                }
            },
            RawPrefilter::Prefix {
                prefix,
                prefix_offset,
            } => quote::quote! {
                ::aob_common::RawPrefilter::Prefix {
                    prefix: #prefix,
                    prefix_offset: #prefix_offset,
                }
            },
            RawPrefilter::PrefixPostfix {
                prefix,
                prefix_offset,
                postfix,
                postfix_offset,
            } => quote::quote! {
                ::aob_common::RawPrefilter::PrefixPostfix {
                    prefix: #prefix,
                    prefix_offset: #prefix_offset,
                    postfix: #postfix,
                    postfix_offset: #postfix_offset,
                }
            },
        };

        let tokenize_slice = |slice: &[u8]| {
            slice
                .iter()
                .map(|&x| {
                    let x = UnsuffixedU8(x);
                    quote::quote!(#x,)
                })
                .collect::<TokenStream2>()
        };
        let buffer_len = needle.serialize_word().len();
        let word = tokenize_slice(needle.serialize_word());
        let mask = tokenize_slice(needle.serialize_mask());

        let Self {
            attributes, visibility, name, ..
        } = self;

        quote::quote! {
            #(#attributes)*
            #visibility const #name: ::aob_common::StaticNeedle<#needle_len, #buffer_len> =
                ::aob_common::StaticNeedle::new(#prefilter, [#word], [#mask]);
        }
    }

    #[must_use]
    fn tokenize_error(&self, error: &AobError) -> TokenStream2 {
        let mut buffer = Vec::new();
        Report::build(ReportKind::Error, (), error.span().start)
            .with_config(Config::default().with_color(false))
            .with_message(error.to_string())
            .with_label(Label::new(error.span()).with_message(error.reason().to_string()))
            .finish()
            .write(Source::from(&self.pattern), &mut buffer)
            .unwrap();
        let error_message = String::from_utf8(buffer).unwrap();
        quote::quote_spanned!(Span::call_site() => compile_error!(#error_message))
    }
}

impl Parse for AobDecl {
    fn parse(input: ParseStream) -> ParseResult<Self> {
        let attributes = input.call(Attribute::parse_outer)?;
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
            attributes,
            visibility,
            name,
            method,
            pattern,
        })
    }
}

struct AobDecls {
    decls: Vec<AobDecl>,
}

impl AobDecls {
    fn into_tokens(self) -> TokenStream2 {
        let mut tokens = TokenStream2::new();
        for decl in self.decls {
            tokens.extend(decl.into_tokens());
        }
        tokens
    }
}

impl Parse for AobDecls {
    fn parse(input: ParseStream) -> ParseResult<Self> {
        let mut decls = Vec::new();
        decls.push(input.parse()?);
        while let Ok(decl) = input.parse() {
            decls.push(decl);
        }
        Ok(Self { decls })
    }
}

/// Parses, validates, and constructs a [`Needle`](aob_common::Needle) at compile-time.
///
/// ## Syntax
/// ```ignore
/// aob! {
///     [pub] const NAME_1 = METHOD_1("PATTERN_1");
///     [pub] const NAME_2 = METHOD_2("PATTERN_2");
///     ...
///     [pub] const NAME_N = METHOD_N("PATTERN_N");
/// }
/// ```
/// Expects syntax of the form: `#[$ATTRIBUTES]* $VISIBILITY? const $IDENTIFIER = $METHOD("$PATTERN");`
///
/// With the following rules:
/// * `$ATTRIBUTES` is zero or more valid [Attributes](<https://doc.rust-lang.org/reference/attributes.html>).
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
    parse_macro_input!(input as AobDecls).into_tokens().into()
}
