use chumsky::{
    error::Simple,
    primitive::{
        choice,
        filter,
        filter_map,
        just,
    },
    Parser,
};

#[must_use]
pub(crate) fn ida_pattern() -> impl Parser<char, Vec<Option<u8>>, Error = Simple<char>> {
    let whitespace = filter(|c: &char| c.is_whitespace()).repeated();
    let wildcard = just("?").repeated().at_least(1).at_most(2).to(None);
    let byte = filter_map(|span, c: char| {
        if c.is_ascii_hexdigit() {
            Ok(c as u8)
        } else {
            Err(Simple::custom(span, format!("'{c}' is not a hexdigit")))
        }
    })
    .repeated()
    .exactly(2)
    .map(|digits| {
        let digits = String::from_utf8(digits).unwrap();
        Some(u8::from_str_radix(&digits, 16).unwrap())
    });

    choice((wildcard, byte))
        .separated_by(whitespace.at_least(1))
        .collect()
        .padded_by(whitespace)
}

#[cfg(test)]
mod tests {
    use chumsky::{
        primitive::end,
        Parser as _,
    };

    #[test]
    fn test_success() {
        let parser = super::ida_pattern().then_ignore(end());
        assert_eq!(
            parser.parse("AA ? BB").unwrap(),
            [Some(0xAA), None, Some(0xBB)]
        );
        assert_eq!(
            parser.parse("AA ?? BB").unwrap(),
            [Some(0xAA), None, Some(0xBB)]
        );
        assert_eq!(
            parser.parse("AA    ? BB").unwrap(),
            [Some(0xAA), None, Some(0xBB)]
        );
        assert_eq!(
            parser.parse(" AA ? BB").unwrap(),
            [Some(0xAA), None, Some(0xBB)]
        );
        assert_eq!(
            parser.parse("AA ? BB ").unwrap(),
            [Some(0xAA), None, Some(0xBB)]
        );
    }

    #[test]
    fn test_error() {
        let parser = super::ida_pattern().then_ignore(end());
        assert!(parser.parse("A ? BB").is_err());
        assert!(parser.parse("AAA ? BB").is_err());
        assert!(parser.parse("AA ??? BB").is_err());
        assert!(parser.parse("Ax ? BB").is_err());
        assert!(parser.parse("\"AA ? BB\"").is_err());
    }
}
