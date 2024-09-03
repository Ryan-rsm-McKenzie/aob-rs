use aob_common::DynamicNeedle;
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
            _ => Err(syn::Error::new(value.span(), "expected `ida`")),
        }
    }
}

/// ```
/// aob! {
///     $VISIBILITY const $NAME = $METHOD("$PATTERN");
/// }
/// ```
struct Aob {
    visibility: Visibility,
    name: Ident,
    method: Method,
    pattern: String,
}

impl Aob {
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

    fn tokenize_needle(&self, bytes: &[Option<u8>]) -> TokenStream2 {
        let needle = DynamicNeedle::from_bytes(bytes);
        let table = needle.table_slice();
        let word = needle.word_slice();
        let wildcards = needle.wildcards_slice();
        let largest_offset = table
            .iter()
            .copied()
            .filter(|&x| x != usize::MAX)
            .min()
            .unwrap_or(0);
        let offset_type = match largest_offset {
            i if u8::try_from(i).is_ok() => quote::quote!(u8),
            i if u16::try_from(i).is_ok() => quote::quote!(u16),
            i if u32::try_from(i).is_ok() => quote::quote!(u32),
            i if u64::try_from(i).is_ok() => quote::quote!(u64),
            _ => std::unreachable!(
                "integer of type usize somehow doesn't fit into any fixed-width integer"
            ),
        };
        let table_len: UnsuffixedUsize = table.len().into();
        let word_len: UnsuffixedUsize = word.len().into();
        let wildcards_len: UnsuffixedUsize = wildcards.len().into();
        let table: TokenStream2 = table
            .iter()
            .map(|&x| {
                if x != usize::MAX {
                    let x = UnsuffixedUsize(x);
                    quote::quote!(#x,)
                } else {
                    quote::quote!(#offset_type::MAX,)
                }
            })
            .collect();
        let word: TokenStream2 = word
            .iter()
            .map(|&x| {
                let x = UnsuffixedU8(x);
                quote::quote!(#x,)
            })
            .collect();
        let wildcards: TokenStream2 = wildcards
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
            #visibility const #name: ::aob_common::StaticNeedle<#offset_type, #table_len, #word_len, #wildcards_len> = ::aob_common::StaticNeedle::new([#table], [#word], [#wildcards]);
        }
    }

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

#[proc_macro]
pub fn aob(input: TokenStream) -> TokenStream {
    parse_macro_input!(input as Aob).into_tokens().into()
}
