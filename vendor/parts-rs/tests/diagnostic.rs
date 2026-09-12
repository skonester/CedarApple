use parts::{
    diagnostic::{Diagnostic, DiagnosticLevel, Report},
    engine::Engine,
    parser::Parser,
    scanner::{Scanner, ScannerError, Span, Token, TokenType},
    tools::LanguageTools,
};

#[test]
fn test_diagnostic_builder_and_render_plain() {
    let source = "let x = 10\nlet y = 20;";
    let diag = Diagnostic::error("expected ';', found keyword 'let'")
        .with_file("test.pts")
        .with_location(2, 1)
        .with_length(3)
        .with_label("unexpected keyword")
        .with_note("statements should be terminated with a semicolon")
        .with_help("add ';' at the end of the previous statement");

    let rendered = diag.render_plain(Some(source), Some("test.pts"));

    assert!(rendered.contains("error: expected ';', found keyword 'let'"));
    assert!(rendered.contains("--> test.pts:2:1"));
    assert!(rendered.contains("2 | let y = 20;"));
    assert!(rendered.contains("^^^ unexpected keyword"));
    assert!(rendered.contains("= note: statements should be terminated with a semicolon"));
    assert!(rendered.contains("= help: add ';' at the end of the previous statement"));
}

#[test]
fn test_diagnostic_level_names_and_colors() {
    assert_eq!(DiagnosticLevel::Error.name(), "error");
    assert_eq!(DiagnosticLevel::Warning.name(), "warning");
    assert_eq!(DiagnosticLevel::Note.name(), "note");
    assert_eq!(DiagnosticLevel::Help.name(), "help");
    assert_eq!(DiagnosticLevel::Info.name(), "info");

    assert!(DiagnosticLevel::Error.ansi_color().contains("\x1b[1;31m"));
}

#[test]
fn test_report_collection() {
    let mut report = Report::new();
    assert!(report.is_empty());
    assert!(!report.has_errors());

    report.add(Diagnostic::warning("unused variable 'foo'").with_location(1, 5));
    assert_eq!(report.warning_count(), 1);
    assert_eq!(report.error_count(), 0);
    assert!(!report.has_errors());

    report.add(Diagnostic::error("type mismatch").with_location(2, 10));
    assert_eq!(report.error_count(), 1);
    assert!(report.has_errors());

    let rendered = report.render_plain(None, None);
    assert!(rendered.contains("warning: unused variable 'foo'"));
    assert!(rendered.contains("error: type mismatch"));
}

#[test]
fn test_scanner_error_diagnostics() {
    let source = "let s = \"unterminated string";
    let mut scanner = Scanner::new(
        parts::scanner_rules::ScannerRule::get_default_rules(),
        source.to_string(),
    );
    let err = loop {
        match scanner.get_next() {
            Ok(tok) if tok.kind == TokenType::Special && tok.lexeme == "EOF" => break None,
            Ok(_) => continue,
            Err(e) => break Some(e),
        }
    };
    assert_eq!(err, Some(ScannerError::UnterminatedString));
    let report = LanguageTools::check(source, Some("test_scan.pts"));
    assert!(report.has_errors());
    let rendered = report.render_plain(Some(source), Some("test_scan.pts"));
    assert!(rendered.contains("error: unterminated string literal"));
}

#[test]
fn test_parser_error_diagnostics_token_mismatch() {
    let source = "let = 10;";
    let mut parser = Parser::new(source.to_string());
    let res = parser.parse_all();
    assert!(res.is_err());

    let err = res.unwrap_err();
    let diag = err.to_diagnostic(Some(source), Some("test_syntax.pts"));
    let rendered = diag.render_plain(Some(source), Some("test_syntax.pts"));

    assert!(rendered.contains("error:"));
    assert!(rendered.contains("test_syntax.pts"));
    assert!(rendered.contains("let = 10;"));
    assert!(rendered.contains("^"));
}

#[test]
fn test_token_friendly_names() {
    let semi_tok = Token {
        kind: TokenType::Operator,
        lexeme: "SEMICOLON".to_string(),
        span: Span { line: 1, column: 5 },
    };
    assert_eq!(semi_tok.user_friendly_name(), "';'");

    let plus_tok = Token {
        kind: TokenType::Operator,
        lexeme: "PLUS".to_string(),
        span: Span { line: 1, column: 5 },
    };
    assert_eq!(plus_tok.user_friendly_name(), "'+'");

    let kw_tok = Token {
        kind: TokenType::Keyword,
        lexeme: "let".to_string(),
        span: Span { line: 1, column: 1 },
    };
    assert_eq!(kw_tok.user_friendly_name(), "keyword 'let'");

    let ident_tok = Token {
        kind: TokenType::Identifier,
        lexeme: "foo".to_string(),
        span: Span { line: 1, column: 5 },
    };
    assert_eq!(ident_tok.user_friendly_name(), "identifier 'foo'");

    let eof_tok = Token {
        kind: TokenType::Special,
        lexeme: "EOF".to_string(),
        span: Span {
            line: 1,
            column: 10,
        },
    };
    assert_eq!(eof_tok.user_friendly_name(), "end of file");
}

#[test]
fn test_compiler_error_diagnostics_break_outside_loop() {
    let source = "break;";
    let report = LanguageTools::check(source, Some("bad_break.pts"));
    assert!(report.has_errors());
    let rendered = report.render_plain(Some(source), Some("bad_break.pts"));
    assert!(rendered.contains("cannot use 'break' outside of a loop"));
}

#[test]
fn test_compiler_error_diagnostics_continue_outside_loop() {
    let source = "continue;";
    let report = LanguageTools::check(source, Some("bad_continue.pts"));
    assert!(report.has_errors());
    let rendered = report.render_plain(Some(source), Some("bad_continue.pts"));
    assert!(rendered.contains("cannot use 'continue' outside of a loop"));
}

#[test]
fn test_compiler_error_nonexistent_enum() {
    let source = "let x = NonExistent::Variant(1);";
    let report = LanguageTools::check(source, Some("bad_enum.pts"));
    assert!(report.has_errors());
    let rendered = report.render_plain(Some(source), Some("bad_enum.pts"));
    assert!(rendered.contains("referencing enum definition that doesn't exist"));
}

#[test]
fn test_engine_check_and_error_reporting() {
    let engine = Engine::new();
    let report = engine.check("let x = 1 + 2; return x;");
    assert!(!report.has_errors());

    let bad_report = engine.check("break;");
    assert!(bad_report.has_errors());

    let run_err = engine.run("let = 10;").unwrap_err();
    assert!(run_err.contains("Parser error"));
}
