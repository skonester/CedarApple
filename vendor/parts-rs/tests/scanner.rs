#[cfg(test)]
mod tests {
    use parts::scanner::Scanner;
    use parts::scanner::ScannerError;
    use parts::scanner::Span;
    use parts::scanner::Token;
    use parts::scanner::TokenType;
    use parts::scanner_rules::ScannerRule;
    use rustc_hash::FxHashMap;

    #[test]
    fn check_operator() {
        let mut s = Scanner::new(ScannerRule::get_default_rules(), "@".to_string());

        let at_token = s.get_next();

        assert!(at_token.is_ok());
        assert_eq!(
            at_token.as_ref().unwrap().to_owned(),
            Token {
                kind: TokenType::Operator,
                lexeme: "AT".to_string(),
                span: Span { line: 0, column: 0 }
            }
        );

        let eof = s.get_next();

        assert!(eof.is_ok());
        assert_eq!(
            eof.as_ref().unwrap().to_owned(),
            Token {
                kind: TokenType::Special,
                lexeme: "EOF".to_string(),
                span: Span { line: 0, column: 0 }
            }
        )
    }

    #[test]
    fn check_operator_buffor() {
        let mut s = Scanner::new(ScannerRule::get_default_rules(), "@+".to_string());

        let at_token = s.get_next();

        assert!(at_token.is_ok());
        assert_eq!(
            at_token.as_ref().unwrap().to_owned(),
            Token {
                kind: TokenType::Operator,
                lexeme: "AT".to_string(),
                span: Span { line: 0, column: 0 }
            }
        );

        let plus_token = s.get_next();

        assert!(plus_token.is_ok());
        assert_eq!(
            plus_token.as_ref().unwrap().to_owned(),
            Token {
                kind: TokenType::Operator,
                lexeme: "PLUS".to_string(),
                span: Span { line: 0, column: 0 }
            }
        );

        let eof = s.get_next();

        assert!(eof.is_ok());
        assert_eq!(
            eof.as_ref().unwrap().to_owned(),
            Token {
                kind: TokenType::Special,
                lexeme: "EOF".to_string(),
                span: Span { line: 0, column: 0 }
            }
        )
    }

    #[test]
    fn invalid_token() {
        let mut s = Scanner::new(ScannerRule::get_default_rules(), "?".to_string());

        let at_token = s.get_next();

        assert!(at_token.is_err_and(|e| e == ScannerError::InvalidOperator("?".to_string())));
    }

    #[test]
    fn unknown_operator() {
        let mut s = Scanner::new(
            vec![ScannerRule {
                result: TokenType::Operator,
                base_rule: None,
                rule: None,
                process: Some(Box::new(|_mappings, _runs, _span| {
                    Err(ScannerError::UnknownToken('y'))
                })),
                skip: false,
                mappings: FxHashMap::default(),
                valid_chars: vec![],
            }],
            "x".to_string(),
        );

        let at_token = s.get_next();

        assert!(at_token.is_err_and(|e| e == ScannerError::UnknownToken('x')))
    }

    #[test]
    fn parse_number() {
        let mut s = Scanner::new(ScannerRule::get_default_rules(), "1".to_string());

        let num_token = s.get_next();

        assert!(num_token.is_ok());
        assert_eq!(
            num_token.as_ref().unwrap().to_owned(),
            Token {
                kind: TokenType::Number,
                lexeme: "1".to_string(),
                span: Span { line: 0, column: 0 }
            }
        );
    }

    #[test]
    fn parse_decimal() {
        let mut s = Scanner::new(ScannerRule::get_default_rules(), "3.14".to_string());

        let num_token = s.get_next();

        assert!(num_token.is_ok());
        assert_eq!(
            num_token.as_ref().unwrap().to_owned(),
            Token {
                kind: TokenType::Number,
                lexeme: "3.14".to_string(),
                span: Span { line: 0, column: 0 }
            }
        );
    }

    #[test]

    fn parse_keyword() {
        let mut s = Scanner::new(ScannerRule::get_default_rules(), "false".to_string());

        let false_token = s.get_next();

        assert!(false_token.is_ok());
        assert_eq!(
            false_token.as_ref().unwrap().to_owned(),
            Token {
                kind: TokenType::Keyword,
                lexeme: "FALSE".to_string(),
                span: Span { line: 0, column: 0 }
            }
        );
    }

    #[test]
    fn parse_identifier() {
        let mut s = Scanner::new(ScannerRule::get_default_rules(), "flse".to_string());

        let false_token = s.get_next();

        assert!(false_token.is_ok());
        assert_eq!(
            false_token.as_ref().unwrap().to_owned(),
            Token {
                kind: TokenType::Identifier,
                lexeme: "flse".to_string(),
                span: Span { line: 0, column: 0 }
            }
        );
    }

    #[test]
    fn parse_space() {
        let mut s = Scanner::new(ScannerRule::get_default_rules(), "  ".to_string());

        let eof = s.get_next();

        assert!(eof.is_ok());
        assert_eq!(
            eof.as_ref().unwrap().to_owned(),
            Token {
                kind: TokenType::Special,
                lexeme: "EOF".to_string(),
                span: Span { line: 0, column: 0 }
            }
        )
    }

    #[test]
    fn parse_string_tick() {
        let mut s = Scanner::new(ScannerRule::get_default_rules(), "``".to_string());

        let string_token = s.get_next();

        assert!(string_token.is_ok());
        assert_eq!(
            string_token.as_ref().unwrap().to_owned(),
            Token {
                kind: TokenType::String,
                lexeme: "".to_string(),
                span: Span { line: 0, column: 0 }
            }
        );

        let eof = s.get_next();

        assert!(eof.is_ok());
        assert_eq!(
            eof.as_ref().unwrap().to_owned(),
            Token {
                kind: TokenType::Special,
                lexeme: "EOF".to_string(),
                span: Span { line: 0, column: 0 }
            }
        )
    }

    #[test]
    fn parse_string_normal() {
        let mut s = Scanner::new(ScannerRule::get_default_rules(), "\"\"".to_string());

        let string_token = s.get_next();

        assert!(string_token.is_ok());
        assert_eq!(
            string_token.as_ref().unwrap().to_owned(),
            Token {
                kind: TokenType::String,
                lexeme: "".to_string(),
                span: Span { line: 0, column: 0 }
            }
        );

        let eof = s.get_next();

        assert!(eof.is_ok());
        assert_eq!(
            eof.as_ref().unwrap().to_owned(),
            Token {
                kind: TokenType::Special,
                lexeme: "EOF".to_string(),
                span: Span { line: 0, column: 0 }
            }
        )
    }

    #[test]
    fn parse_string_nested() {
        let mut s = Scanner::new(ScannerRule::get_default_rules(), "\"``\"".to_string());

        let string_token = s.get_next();

        assert!(string_token.is_ok());
        assert_eq!(
            string_token.as_ref().unwrap().to_owned(),
            Token {
                kind: TokenType::String,
                lexeme: "``".to_string(),
                span: Span { line: 0, column: 0 }
            }
        );

        let eof = s.get_next();

        assert!(eof.is_ok());
        assert_eq!(
            eof.as_ref().unwrap().to_owned(),
            Token {
                kind: TokenType::Special,
                lexeme: "EOF".to_string(),
                span: Span { line: 0, column: 0 }
            }
        )
    }
}
