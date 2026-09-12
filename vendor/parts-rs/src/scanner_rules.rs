use core::fmt::Result as FmtResult;
use core::fmt::{Debug, Formatter};
use rustc_hash::FxHashMap;

use crate::scanner::{ScannerError, Span};
use crate::scanner::{Token, TokenType};

type BaseRuleFn = Box<dyn Fn(&char) -> bool>;
type RuleFn = Box<dyn Fn(&str) -> bool>;
type ProcessFn =
    Box<dyn Fn(&FxHashMap<String, String>, &str, Span) -> Result<Vec<Token>, ScannerError>>;

pub struct ScannerRule {
    pub result: TokenType,
    pub base_rule: Option<BaseRuleFn>,
    pub rule: Option<RuleFn>,
    pub process: Option<ProcessFn>,
    pub skip: bool,
    pub mappings: FxHashMap<String, String>,
    pub valid_chars: Vec<char>,
}

impl Debug for ScannerRule {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("ScannerRule")
            .field("result", &self.result)
            .field("base_rule_exists", &self.base_rule.is_some())
            .field("rule_exists", &self.rule.is_some())
            .field("process_exists", &self.process.is_some())
            .field("skip", &self.skip)
            .field("mappings", &self.mappings)
            .field("valid_chars", &self.valid_chars)
            .finish()
    }
}

//TODO str.len() returns bytes count, str.chars() - chars, for example 0xa0 counts as 2 bytes, but one char is making test fail if passed in a string

impl ScannerRule {
    pub fn get_default_rules() -> Vec<ScannerRule> {
        vec![
            ScannerRule {
                result: TokenType::Operator,
                base_rule: None,
                rule: None,
                process: Some(Box::new(|mappings, runs, span| {
                    let token_value: String = runs.chars().collect();

                    if let Some(name) = mappings.get(&token_value) {
                        return Ok(vec![Token {
                            kind: TokenType::Operator,
                            lexeme: name.clone(),
                            span,
                        }]);
                    }

                    let mut ret_tokens = Vec::new();
                    let mut offset = 0;
                    let mut temp_slice = &token_value[..];
                    let mut current_span = span;

                    while offset != token_value.len() {
                        if temp_slice.is_empty() {
                            return Err(ScannerError::InvalidOperator(
                                token_value[offset..].to_string(),
                            ));
                        }

                        if let Some(name) = mappings.get(temp_slice) {
                            offset += temp_slice.len();
                            ret_tokens.push(Token {
                                kind: TokenType::Operator,
                                lexeme: name.clone(),
                                span: current_span, // replace with actual length
                            });
                            temp_slice = &token_value[offset..];
                            current_span.column += temp_slice.chars().count();
                        } else {
                            temp_slice = &temp_slice[..temp_slice.len() - 1];
                        }
                    }

                    Ok(ret_tokens)
                })),
                skip: false,
                mappings: [
                    ("+", "PLUS"),
                    ("-", "MINUS"),
                    ("/", "SLASH"),
                    ("*", "STAR"),
                    ("%", "MOD"),
                    (";", "SEMICOLON"),
                    (":", "COLON"),
                    (".", "DOT"),
                    (",", "COMMA"),
                    ("(", "LEFT_PAREN"),
                    (")", "RIGHT_PAREN"),
                    ("{", "LEFT_BRACE"),
                    ("}", "RIGHT_BRACE"),
                    ("[", "LEFT_BRACKET"),
                    ("]", "RIGHT_BRACKET"),
                    ("@", "AT"),
                    ("!", "BANG"),
                    ("=", "EQUALS"),
                    ("&", "BIT_AND"),
                    ("|", "BIT_OR"),
                    ("|>", "OBJ_START"),
                    ("<|", "OBJ_END"),
                    ("==", "EQUALITY"),
                    ("<", "LESS_THAN"),
                    (">", "MORE_THAN"),
                    ("<=", "LESS_EQ"),
                    (">=", "MORE_EQ"),
                    ("=>", "ARROW_LEFT"),
                    ("^", "BIT_XOR"),
                    ("<<", "LEFT_SHIFT"),
                    (">>", "RIGHT_SHIFT"),
                    ("::", "DOUBLE_COLON"),
                ]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
                valid_chars: vec![
                    '+', '-', '/', '*', ';', '[', ']', '(', ')', '{', '}', '.', ':', ',', '|', '&',
                    '>', '<', '!', '#', '-', '=', '?', '%', '@', '^',
                ],
            }, // --- TokenNumber Rule ---
            ScannerRule {
                result: TokenType::Number,
                base_rule: Some(Box::new(|r| *r >= '0' && *r <= '9' || *r == '.')),
                rule: Some(Box::new(|r| r.chars().filter(|&c| c == '.').count() <= 1)),
                process: None,
                skip: false,
                mappings: FxHashMap::default(),
                valid_chars: vec![],
            },
            // --- TokenKeyword Rule ---
            ScannerRule {
                result: TokenType::Keyword,
                base_rule: Some(Box::new(|r| {
                    (*r >= 'a' && *r <= 'z')
                        || (*r >= 'A' && *r <= 'Z')
                        || *r == '_'
                        || (*r >= '0' && *r <= '9')
                })),
                rule: None,
                process: Some(Box::new(|mappings, runs, span| {
                    let mut token = Token {
                        kind: TokenType::Keyword,
                        lexeme: runs.to_string(),
                        span,
                    };

                    if mappings.contains_key(&token.lexeme) {
                        token.lexeme = token.lexeme.to_uppercase();
                    } else {
                        token.kind = TokenType::Identifier;
                    }

                    Ok(vec![token])
                })),
                skip: false,
                mappings: [
                    ("false", ""),
                    ("if", ""),
                    ("let", ""),
                    ("true", ""),
                    ("fun", ""),
                    ("return", ""),
                    ("else", ""),
                    ("for", ""),
                    ("import", ""),
                    ("from", ""),
                    ("as", ""),
                    ("syntax", ""),
                    ("use", ""),
                    ("raise", ""),
                    ("break", ""),
                    ("continue", ""),
                    ("translation", ""),
                    ("in", ""),
                    ("part", ""),
                    ("enum", ""),
                    ("match", ""),
                ]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
                valid_chars: vec![],
            },
            // --- TokenSpace Rule ---
            ScannerRule {
                result: TokenType::Space,
                base_rule: Some(Box::new(|r| r.is_whitespace())),
                rule: None,
                process: None,
                skip: true,
                mappings: FxHashMap::default(),
                valid_chars: vec![],
            },
            // --- TokenString Rule ---
            ScannerRule {
                result: TokenType::String,
                base_rule: Some(Box::new(|_r| true)),
                rule: Some(Box::new(|runs| {
                    let bytes = runs.as_bytes();
                    bytes.len() <= 1 || bytes[0] != bytes[bytes.len() - 1]
                })),
                process: Some(Box::new(|_mappings, runs, span| {
                    if runs.len() < 2 {
                        return Err(ScannerError::UnterminatedString);
                    }

                    let bytes = runs.as_bytes();
                    let left_side = bytes[0] as char;
                    let right_side = bytes[bytes.len() - 1] as char;

                    if (left_side == '"' && right_side != '"')
                        || (left_side == '`' && right_side != '`')
                    {
                        return Err(ScannerError::UnterminatedString);
                    }

                    if left_side != '"' && left_side != '`' {
                        return Err(ScannerError::UnknownToken(left_side));
                    }

                    let content = &runs[1..runs.len() - 1];
                    let mut unescaped = String::with_capacity(content.len());
                    let mut chars = content.chars();

                    while let Some(c) = chars.next() {
                        if c == '\\' {
                            match chars.next() {
                                Some('"') => unescaped.push('"'),
                                Some('\\') => unescaped.push('\\'),
                                Some('n') => unescaped.push('\n'),
                                Some('t') => unescaped.push('\t'),
                                Some('r') => unescaped.push('\r'),
                                Some('0') => unescaped.push('\0'),
                                Some('b') => unescaped.push('\x08'),
                                Some('f') => unescaped.push('\x0C'),
                                _ => return Err(ScannerError::InvalidEscape),
                            }
                        } else {
                            unescaped.push(c);
                        }
                    }

                    Ok(vec![Token {
                        kind: TokenType::String,
                        lexeme: unescaped,
                        span,
                    }])
                })),
                skip: false,
                mappings: FxHashMap::default(),
                valid_chars: vec![],
            },
        ]
    }
}
