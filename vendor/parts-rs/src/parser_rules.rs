use crate::{
    parser::{
        Ast, EnumVariant, Error as ParserError, ImportType, MacroArm, MatchArm, MatchPattern,
        Parser, Value,
    },
    scanner::{Span, Token, TokenType},
};
use std::{fmt, sync::Arc};

type CheckFn = dyn Fn(&mut Parser) -> bool + Send + Sync;
type ParseFn = dyn Fn(&mut Parser) -> Result<Ast, ParserError> + Send + Sync;

#[derive(Clone)]
pub struct ParserRule {
    pub id: String,
    pub advance_token: bool,
    pub rule: Arc<CheckFn>,
    pub parse: Arc<ParseFn>,
}

impl fmt::Debug for ParserRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParserRule")
            .field("id", &self.id)
            .field("advance_token", &self.advance_token)
            .finish()
    }
}

impl ParserRule {
    pub fn get_default_rules() -> Vec<ParserRule> {
        vec![
            ParserRule {
                id: "ImportStmt".to_string(),
                advance_token: true,
                rule: Arc::new(|parser| parser.check_keyword("IMPORT")),
                parse: Arc::new(|parser| {
                    let mut import_type = ImportType::Regular;

                    if parser.match_keyword("SYNTAX") {
                        import_type = ImportType::Syntax;
                    } else if parser.match_keyword("TRANSLATION") {
                        import_type = ImportType::Translation;
                    }

                    let mut import_map = None;
                    if parser.match_operator("LEFT_BRACE") {
                        let mut identifiers = Vec::new();

                        if !parser.match_operator("RIGHT_BRACE") {
                            loop {
                                let identifier = parser.expect_kind(TokenType::Identifier)?;
                                identifiers.push(identifier.lexeme);

                                if !parser.match_operator("COMMA") {
                                    break;
                                }
                            }

                            parser.expect_operator("RIGHT_BRACE")?;
                        }

                        parser.expect_keyword("FROM")?;
                        import_map = Some(identifiers)
                    }

                    let source_token = parser.expect_kind(TokenType::String)?;

                    let mut alias = None;
                    if parser.match_keyword("AS") {
                        let alias_token = parser.expect_kind(TokenType::Identifier)?;
                        alias = Some(alias_token.lexeme.to_string());
                    }

                    Ok(Ast::Import {
                        import_type,
                        import_map,
                        source: source_token.lexeme,
                        alias,
                    })
                }),
            },
            ParserRule {
                id: "UseStmt".to_string(),
                advance_token: true,
                rule: Arc::new(|parser| parser.check_keyword("USE")),
                parse: Arc::new(|parser| {
                    let import_type = if parser.match_keyword("SYNTAX") {
                        ImportType::Syntax
                    } else if parser.match_keyword("TRANSLATION") {
                        ImportType::Translation
                    } else {
                        return Err(ParserError::TokenMismatch(
                            Token {
                                kind: TokenType::Keyword,
                                lexeme: "SYNTAX | TRANSLATION".to_string(),
                                span: Span { line: 0, column: 0 },
                            },
                            parser.peek()?,
                        ));
                    };

                    let source_token = parser.expect_kind(TokenType::String)?;

                    parser.expect_keyword("AS")?;

                    let alias_token = parser.expect_kind(TokenType::Identifier)?;

                    Ok(Ast::Import {
                        import_type,
                        import_map: None,
                        source: source_token.lexeme.to_string(),
                        alias: Some(alias_token.lexeme.to_string()),
                    })
                }),
            },
            ParserRule {
                id: "LetStmt".to_string(),
                advance_token: true,
                rule: Arc::new(|parser| parser.check_keyword("LET")),
                parse: Arc::new(|parser| {
                    let identifier = parser.expect_kind(TokenType::Identifier)?;

                    let initial_value = if parser.match_operator("LEFT_PAREN") {
                        let token = parser.peek()?;

                        let mut args = Vec::new();

                        if token.kind != TokenType::Operator && token.lexeme != "RIGHT_PAREN" {
                            loop {
                                let arg = parser.expect_kind(TokenType::Identifier)?;

                                args.push(arg.lexeme.to_string());

                                if parser.match_operator("COMMA") {
                                    continue;
                                }

                                if parser.check_operator("RIGHT_PAREN") {
                                    break;
                                }
                            }
                        }

                        parser.expect_operator("RIGHT_PAREN")?;

                        let body = if parser.match_operator("EQUALS") {
                            let value = Box::new(parser.parse()?);

                            Ast::Return { value }
                        } else {
                            parser.parse_rule("BlockExpr")?
                        };

                        Ast::Value(Value::Fun {
                            args,
                            body: Box::new(body),
                        })
                    } else {
                        parser.expect_operator("EQUALS")?;
                        parser.parse()?
                    };

                    Ok(Ast::Declare {
                        name: identifier.lexeme.to_string(),
                        value: Box::new(initial_value),
                    })
                }),
            },
            ParserRule {
                id: "FunExpr".to_string(),
                advance_token: true,
                rule: Arc::new(|parser| parser.check_keyword("FUN")),
                parse: Arc::new(|parser| {
                    parser.expect_operator("LEFT_PAREN")?;

                    let mut args: Vec<String> = vec![];

                    if !parser.check_operator("RIGHT_PAREN") {
                        loop {
                            let arg = parser.expect_kind(TokenType::Identifier)?;
                            args.push(arg.lexeme.to_string());

                            if !parser.match_operator("COMMA") {
                                break;
                            }
                        }
                    }

                    parser.expect_operator("RIGHT_PAREN")?;

                    let body = if parser.match_operator("EQUALS") {
                        Ast::Return {
                            value: Box::new(parser.parse()?),
                        }
                    } else {
                        parser.parse_rule("BlockExpr")?
                    };

                    Ok(Ast::Value(Value::Fun {
                        args,
                        body: Box::new(body),
                    }))
                }),
            },
            ParserRule {
                id: "IfExpr".to_string(),
                advance_token: true,
                rule: Arc::new(|parser| parser.check_keyword("IF")),
                parse: Arc::new(|parser| {
                    let condition = parser.parse()?;
                    let then_branch = parser.parse()?;
                    let mut else_branch = None;

                    if parser.match_keyword("ELSE") {
                        else_branch = Some(Box::new(parser.parse()?));
                    }

                    Ok(Ast::If {
                        condition: Box::new(condition),
                        then_branch: Box::new(then_branch),
                        else_branch,
                    })
                }),
            },
            ParserRule {
                id: "ForExpr".to_string(),
                advance_token: true,
                rule: Arc::new(|parser| parser.check_keyword("FOR")),
                parse: Arc::new(|parser| {
                    let condition = parser.parse()?;

                    if parser.match_keyword("IN") {
                        if let Ast::Value(Value::Ref(ref_val)) = condition {
                            let iter = parser.parse()?;
                            let body = parser.parse()?;

                            Ok(Ast::ForEach {
                                iterable: Box::new(iter),
                                var_name: ref_val,
                                body: Box::new(body),
                            })
                        } else {
                            Err(ParserError::TokenMismatch(
                                Token {
                                    kind: TokenType::Keyword,
                                    lexeme: "".to_string(),
                                    span: Span { column: 0, line: 0 },
                                },
                                parser.peek()?,
                            ))
                        }
                    } else {
                        let body = parser.parse()?;

                        Ok(Ast::For {
                            condition: Box::new(condition),
                            body: Box::new(body),
                        })
                    }
                }),
            },
            ParserRule {
                id: "MatchExpr".to_string(),
                advance_token: true,
                rule: Arc::new(|parser| parser.check_keyword("MATCH")),
                parse: Arc::new(|parser| {
                    let target = parser.parse()?;

                    parser.expect_operator("LEFT_BRACE")?;

                    let mut arms = Vec::new();

                    while !parser.check_operator("RIGHT_BRACE") {
                        let pattern = if parser.match_operator("AT") {
                            MatchPattern::Wildcard(None)
                        } else if parser.check_kind(TokenType::Number) {
                            let val_ast = parser.parse_rule("NumExpr")?;
                            if let Ast::Value(val) = val_ast {
                                MatchPattern::Value(val)
                            } else {
                                unreachable!()
                            }
                        } else if parser.check_kind(TokenType::String) {
                            let val_ast = parser.parse_rule("StringExpr")?;
                            if let Ast::Value(val) = val_ast {
                                MatchPattern::Value(val)
                            } else {
                                unreachable!()
                            }
                        } else if parser.check_keyword("TRUE") {
                            parser.advance()?;
                            MatchPattern::Value(Value::Bool(true))
                        } else if parser.check_keyword("FALSE") {
                            parser.advance()?;
                            MatchPattern::Value(Value::Bool(false))
                        } else if parser.check_kind(TokenType::Identifier) {
                            let ident = parser.advance()?;
                            if ident.lexeme == "_" {
                                MatchPattern::Wildcard(None)
                            } else if (ident.lexeme == "Ok" || ident.lexeme == "Err")
                                && parser.match_operator("LEFT_PAREN")
                            {
                                let mut fields = Vec::new();
                                while !parser.check_operator("RIGHT_PAREN") {
                                    let field_tok = parser.advance()?;
                                    fields.push(field_tok.lexeme);
                                    if !parser.match_operator("COMMA") {
                                        break;
                                    }
                                }
                                parser.expect_operator("RIGHT_PAREN")?;
                                MatchPattern::Enum {
                                    name: "Result".to_string(),
                                    tag: ident.lexeme,
                                    fields,
                                }
                            } else if ident.lexeme == "Some" && parser.match_operator("LEFT_PAREN")
                            {
                                let mut fields = Vec::new();
                                while !parser.check_operator("RIGHT_PAREN") {
                                    let field_tok = parser.advance()?;
                                    fields.push(field_tok.lexeme);
                                    if !parser.match_operator("COMMA") {
                                        break;
                                    }
                                }
                                parser.expect_operator("RIGHT_PAREN")?;
                                MatchPattern::Enum {
                                    name: "Option".to_string(),
                                    tag: "Some".to_string(),
                                    fields,
                                }
                            } else if ident.lexeme == "None" {
                                if parser.match_operator("LEFT_PAREN") {
                                    parser.expect_operator("RIGHT_PAREN")?;
                                }
                                MatchPattern::Enum {
                                    name: "Option".to_string(),
                                    tag: "None".to_string(),
                                    fields: vec![],
                                }
                            } else if parser.match_operator("DOUBLE_COLON") {
                                let tag = parser.expect_kind(TokenType::Identifier)?.lexeme;
                                let mut fields = Vec::new();
                                if parser.match_operator("LEFT_PAREN") {
                                    while !parser.check_operator("RIGHT_PAREN") {
                                        let field_tok = parser.advance()?;
                                        fields.push(field_tok.lexeme);
                                        if !parser.match_operator("COMMA") {
                                            break;
                                        }
                                    }
                                    parser.expect_operator("RIGHT_PAREN")?;
                                }
                                MatchPattern::Enum {
                                    name: ident.lexeme,
                                    tag,
                                    fields,
                                }
                            } else {
                                MatchPattern::Wildcard(Some(ident.lexeme))
                            }
                        } else {
                            return Err(ParserError::TokenMismatch(
                                Token {
                                    kind: TokenType::Identifier,
                                    lexeme: "pattern".to_string(),
                                    span: Span { line: 0, column: 0 },
                                },
                                parser.peek()?,
                            ));
                        };

                        parser.expect_operator("ARROW_LEFT")?;

                        let body = parser.parse()?;

                        parser.match_operator("COMMA");
                        parser.match_operator("SEMICOLON");

                        arms.push(MatchArm {
                            pattern,
                            body: Box::new(body),
                        });
                    }

                    parser.expect_operator("RIGHT_BRACE")?;

                    Ok(Ast::Match {
                        target: Box::new(target),
                        arms,
                    })
                }),
            },
            ParserRule {
                id: "BlockExpr".to_string(),
                advance_token: true,
                rule: Arc::new(|parser| parser.check_operator("LEFT_BRACE")),
                parse: Arc::new(|parser| {
                    let mut body: Vec<Ast> = vec![];

                    loop {
                        if parser.check_operator("RIGHT_BRACE") {
                            break;
                        }

                        let temp = parser.parse()?;
                        if temp != Ast::Ignore && !matches!(temp, Ast::Value(Value::String(_))) {
                            body.push(temp);
                        }
                    }

                    parser.expect_operator("RIGHT_BRACE")?;

                    Ok(Ast::Block { code: body })
                }),
            },
            ParserRule {
                id: "ReturnExpr".to_string(),
                advance_token: true,
                rule: Arc::new(|parser| parser.check_keyword("RETURN")),
                parse: Arc::new(|parser| {
                    Ok(Ast::Return {
                        value: Box::new(parser.parse()?),
                    })
                }),
            },
            ParserRule {
                id: "Break".to_string(),
                advance_token: true,
                rule: Arc::new(|parser| parser.check_keyword("BREAK")),
                parse: Arc::new(|_parser| Ok(Ast::BreakCode)),
            },
            ParserRule {
                id: "Continue".to_string(),
                advance_token: true,
                rule: Arc::new(|parser| parser.check_keyword("CONTINUE")),
                parse: Arc::new(|_parser| Ok(Ast::ContinueCode)),
            },
            ParserRule {
                id: "TrueExpr".to_string(),
                advance_token: true,
                rule: Arc::new(|parser| parser.check_keyword("TRUE")),
                parse: Arc::new(|_parser| Ok(Ast::Value(Value::Bool(true)))),
            },
            ParserRule {
                id: "FalseExpr".to_string(),
                advance_token: true,
                rule: Arc::new(|parser| parser.check_keyword("FALSE")),
                parse: Arc::new(|_parser| Ok(Ast::Value(Value::Bool(false)))),
            },
            ParserRule {
                id: "NumExpr".to_string(),
                advance_token: false,
                rule: Arc::new(|parser| parser.check_kind(TokenType::Number)),
                parse: Arc::new(|parser| {
                    let raw = parser.advance()?;

                    match raw.lexeme.chars().filter(|c| *c == '.').count() {
                        0 => {
                            let parsed: i64 = raw.lexeme.parse().map_err(|_err| {
                                ParserError::TokenMismatch(
                                    Token {
                                        kind: TokenType::Number,
                                        lexeme: "".to_string(),
                                        span: Span { line: 0, column: 0 },
                                    },
                                    parser.peek().unwrap(),
                                )
                            })?;
                            Ok(Ast::Value(Value::Int(parsed)))
                        }
                        1 => {
                            let parsed: f64 = raw.lexeme.parse().map_err(|_err| {
                                ParserError::TokenMismatch(
                                    Token {
                                        kind: TokenType::Number,
                                        lexeme: "".to_string(),
                                        span: Span { line: 0, column: 0 },
                                    },
                                    parser.peek().unwrap(),
                                )
                            })?;
                            Ok(Ast::Value(Value::Double(parsed)))
                        }
                        _ => Err(ParserError::TokenMismatch(
                            Token {
                                kind: TokenType::Number,
                                lexeme: "".to_string(),
                                span: Span { line: 0, column: 0 },
                            },
                            parser.peek()?,
                        )),
                    }
                }),
            },
            ParserRule {
                id: "StringExpr".to_string(),
                advance_token: false,
                rule: Arc::new(|parser| parser.check_kind(TokenType::String)),
                parse: Arc::new(|parser| Ok(Ast::Value(Value::String(parser.advance()?.lexeme)))),
            },
            ParserRule {
                id: "VarExpr".to_string(),
                advance_token: false,
                rule: Arc::new(|parser| parser.check_kind(TokenType::Identifier)),
                parse: Arc::new(|parser| {
                    Ok(Ast::Value(Value::Ref(parser.advance()?.lexeme.to_string())))
                }),
            },
            ParserRule {
                id: "GroupExpr".to_string(),
                advance_token: true,
                rule: Arc::new(|parser| parser.check_operator("LEFT_PAREN")),
                parse: Arc::new(|parser| {
                    let val = parser.parse()?;

                    parser.expect_operator("RIGHT_PAREN")?;

                    Ok(val)
                }),
            },
            ParserRule {
                id: "ObjExpr".to_string(),
                advance_token: true,
                rule: Arc::new(|parser| parser.check_operator("OBJ_START")),
                parse: Arc::new(|parser| {
                    let mut entries: Vec<(Ast, Ast)> = vec![];

                    if !parser.match_operator("OBJ_END") {
                        loop {
                            let key = parser.parse()?;

                            parser.expect_operator("COLON")?;

                            let val = parser.parse()?;

                            entries.push((key, val));

                            if !parser.match_operator("COMMA") {
                                break;
                            }
                        }

                        parser.expect_operator("OBJ_END")?;
                    }

                    Ok(Ast::Object(entries))
                }),
            },
            ParserRule {
                id: "MacroDefinition".to_string(),
                advance_token: true,
                rule: Arc::new(|parser| parser.check_keyword("PART")),
                parse: Arc::new(|parser| {
                    let macro_name = parser.expect_kind(TokenType::Identifier)?;

                    let mut arms: Vec<MacroArm> = vec![];

                    parser.expect_operator("LEFT_BRACE")?;

                    loop {
                        if parser.match_operator("RIGHT_BRACE") {
                            break;
                        }

                        let mut current_arm = MacroArm {
                            expansion: vec![],
                            pattern: vec![],
                        };

                        parser.expect_operator("LEFT_PAREN")?;

                        loop {
                            if parser.match_operator("AT") {
                                if parser.peek()?.kind == TokenType::Identifier {
                                    let mut temp = parser.advance()?;

                                    temp.lexeme = format!("@{}", temp.lexeme);

                                    current_arm.pattern.push(temp);
                                    continue;
                                } else {
                                    current_arm.pattern.push(Token {
                                        kind: TokenType::Operator,
                                        lexeme: "AT".to_string(),
                                        span: Span { line: 0, column: 0 },
                                    });
                                }
                            }

                            if parser.match_operator("RIGHT_PAREN") {
                                break;
                            }

                            current_arm.pattern.push(parser.advance()?);
                        }

                        parser.expect_operator("ARROW_LEFT")?;
                        parser.expect_operator("LEFT_BRACE")?;

                        let mut brace_depth = 0;

                        loop {
                            if parser.match_operator("RIGHT_BRACE") {
                                if brace_depth == 0 {
                                    break;
                                } else {
                                    brace_depth -= 1;
                                    current_arm.expansion.push(Token {
                                        kind: TokenType::Operator,
                                        lexeme: "RIGHT_BRACE".to_string(),
                                        span: Span { line: 0, column: 0 },
                                    });
                                    continue;
                                }
                            }

                            if parser.match_operator("LEFT_BRACE") {
                                brace_depth += 1;
                                current_arm.expansion.push(Token {
                                    kind: TokenType::Operator,
                                    lexeme: "LEFT_BRACE".to_string(),
                                    span: Span { line: 0, column: 0 },
                                });
                                continue;
                            }

                            if parser.match_operator("AT") {
                                if parser.peek()?.kind == TokenType::Identifier {
                                    let mut temp = parser.advance()?;

                                    temp.lexeme = format!("@{}", temp.lexeme);

                                    current_arm.expansion.push(temp);
                                    continue;
                                } else {
                                    current_arm.expansion.push(Token {
                                        kind: TokenType::Operator,
                                        lexeme: "AT".to_string(),
                                        span: Span { line: 0, column: 0 },
                                    });
                                    continue;
                                }
                            }

                            current_arm.expansion.push(parser.advance()?);
                        }

                        arms.push(current_arm);
                    }

                    parser.add_macro(macro_name, arms);

                    Ok(Ast::Ignore)
                }),
            },
            ParserRule {
                id: "EnumDefinition".to_string(),
                advance_token: true,
                rule: Arc::new(|parser| parser.check_keyword("ENUM")),
                parse: Arc::new(|parser| {
                    let enum_name = parser.expect_kind(TokenType::Identifier)?;
                    let mut variants = vec![];

                    if parser.match_operator("LEFT_BRACE") {
                        loop {
                            if parser.check_operator("RIGHT_BRACE") {
                                break;
                            }

                            let variant_name = parser.expect_kind(TokenType::Identifier)?;
                            let mut fields = vec![];

                            if parser.match_operator("LEFT_PAREN") {
                                loop {
                                    let field_name = parser.expect_kind(TokenType::Identifier)?;
                                    fields.push(field_name.lexeme);

                                    if !parser.match_operator("COMMA") {
                                        break;
                                    }
                                }

                                parser.expect_operator("RIGHT_PAREN")?;
                            }

                            variants.push(EnumVariant {
                                name: variant_name.lexeme,
                                fields,
                            });

                            if !parser.match_operator("COMMA") {
                                break;
                            }
                        }

                        parser.expect_operator("RIGHT_BRACE")?;
                    }

                    Ok(Ast::EnumDef {
                        name: enum_name.lexeme,
                        variants,
                    })
                }),
            },
            ParserRule {
                id: "SemicolonSkip".to_string(),
                advance_token: true,
                rule: Arc::new(|parser| parser.check_operator("SEMICOLON")),
                parse: Arc::new(|_parser| Ok(Ast::Ignore)),
            },
        ]
    }
}
