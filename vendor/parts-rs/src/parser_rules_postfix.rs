use crate::parser::BinaryOperator;
use crate::parser::Error as ParserError;
use crate::parser::Value;
use crate::parser::{Ast, Parser};
use crate::scanner::Span;
use crate::scanner::{Token, TokenType};
use std::fmt;
use std::sync::Arc;

type CheckFn = dyn Fn(&mut Parser) -> bool + Send + Sync;
type ParseFn = dyn Fn(&mut Parser, Ast) -> Result<Ast, ParserError> + Send + Sync;

#[derive(Clone)]
pub struct PostfixRule {
    pub id: String,
    pub advance_token: bool,
    pub rule: Arc<CheckFn>,
    pub parse: Arc<ParseFn>,
}

impl fmt::Debug for PostfixRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParserRule")
            .field("id", &self.id)
            .field("advance_token", &self.advance_token)
            .finish()
    }
}

impl PostfixRule {
    pub fn get_default_rules() -> Vec<PostfixRule> {
        vec![
            PostfixRule {
                id: "DotExpr".to_string(),
                advance_token: true,
                rule: Arc::new(|parser| parser.check_operator("DOT")),
                parse: Arc::new(|parser, ast| {
                    Ok(Ast::Dot {
                        accessor: Box::new(ast),
                        access: Box::new(parser.parse_rule("VarExpr")?),
                        resolve: false,
                    })
                }),
            },
            PostfixRule {
                id: "ArrIndex".to_string(),
                advance_token: true,
                rule: Arc::new(|parser| parser.check_operator("LEFT_BRACKET")),
                parse: Arc::new(|parser, ast| {
                    let access = parser.parse()?;

                    parser.expect_operator("RIGHT_BRACKET")?;

                    Ok(Ast::Dot {
                        accessor: Box::new(ast),
                        access: Box::new(access),
                        resolve: true,
                    })
                }),
            },
            PostfixRule {
                id: "PlusOp".to_string(),
                advance_token: true,
                rule: Arc::new(|parser| parser.check_operator("PLUS")),
                parse: Arc::new(|parser, ast| {
                    Ok(Ast::Binary {
                        left: Box::new(ast),
                        right: Box::new(parser.parse()?),
                        operator: BinaryOperator::Add,
                    })
                }),
            },
            PostfixRule {
                id: "MinusOp".to_string(),
                advance_token: true,
                rule: Arc::new(|parser| parser.check_operator("MINUS")),
                parse: Arc::new(|parser, ast| {
                    Ok(Ast::Binary {
                        left: Box::new(ast),
                        right: Box::new(parser.parse()?),
                        operator: BinaryOperator::Minus,
                    })
                }),
            },
            PostfixRule {
                id: "MulOp".to_string(),
                advance_token: true,
                rule: Arc::new(|parser| parser.check_operator("STAR")),
                parse: Arc::new(|parser, ast| {
                    Ok(Ast::Binary {
                        left: Box::new(ast),
                        right: Box::new(parser.parse()?),
                        operator: BinaryOperator::Multiply,
                    })
                }),
            },
            PostfixRule {
                id: "DivOp".to_string(),
                advance_token: true,
                rule: Arc::new(|parser| parser.check_operator("SLASH")),
                parse: Arc::new(|parser, ast| {
                    Ok(Ast::Binary {
                        left: Box::new(ast),
                        right: Box::new(parser.parse()?),
                        operator: BinaryOperator::Divide,
                    })
                }),
            },
            PostfixRule {
                id: "EqOp".to_string(),
                advance_token: true,
                rule: Arc::new(|parser| parser.check_operator("EQUALITY")),
                parse: Arc::new(|parser, ast| {
                    Ok(Ast::Binary {
                        left: Box::new(ast),
                        right: Box::new(parser.parse()?),
                        operator: BinaryOperator::Equals,
                    })
                }),
            },
            PostfixRule {
                id: "GtOp".to_string(),
                advance_token: true,
                rule: Arc::new(|parser| parser.check_operator("MORE_THAN")),
                parse: Arc::new(|parser, ast| {
                    Ok(Ast::Binary {
                        left: Box::new(ast),
                        right: Box::new(parser.parse()?),
                        operator: BinaryOperator::GreaterThan,
                    })
                }),
            },
            PostfixRule {
                id: "GtEqOp".to_string(),
                advance_token: true,
                rule: Arc::new(|parser| parser.check_operator("MORE_EQ")),
                parse: Arc::new(|parser, ast| {
                    Ok(Ast::Binary {
                        left: Box::new(ast),
                        right: Box::new(parser.parse()?),
                        operator: BinaryOperator::GreaterThanOrEqual,
                    })
                }),
            },
            PostfixRule {
                id: "LtOp".to_string(),
                advance_token: true,
                rule: Arc::new(|parser| parser.check_operator("LESS_THAN")),
                parse: Arc::new(|parser, ast| {
                    Ok(Ast::Binary {
                        left: Box::new(ast),
                        right: Box::new(parser.parse()?),
                        operator: BinaryOperator::LessThan,
                    })
                }),
            },
            PostfixRule {
                id: "LtEqOp".to_string(),
                advance_token: true,
                rule: Arc::new(|parser| parser.check_operator("LESS_EQ")),
                parse: Arc::new(|parser, ast| {
                    Ok(Ast::Binary {
                        left: Box::new(ast),
                        right: Box::new(parser.parse()?),
                        operator: BinaryOperator::LessThanOrEqual,
                    })
                }),
            },
            PostfixRule {
                id: "ModOp".to_string(),
                advance_token: true,
                rule: Arc::new(|parser| parser.check_operator("MOD")),
                parse: Arc::new(|parser, ast| {
                    Ok(Ast::Binary {
                        left: Box::new(ast),
                        right: Box::new(parser.parse()?),
                        operator: BinaryOperator::Modulo,
                    })
                }),
            },
            PostfixRule {
                id: "BitAndOp".to_string(),
                advance_token: true,
                rule: Arc::new(|parser| parser.check_operator("BIT_AND")),
                parse: Arc::new(|parser, ast| {
                    Ok(Ast::Binary {
                        left: Box::new(ast),
                        right: Box::new(parser.parse()?),
                        operator: BinaryOperator::BitAnd,
                    })
                }),
            },
            PostfixRule {
                id: "BitOrOp".to_string(),
                advance_token: true,
                rule: Arc::new(|parser| parser.check_operator("BIT_OR")),
                parse: Arc::new(|parser, ast| {
                    Ok(Ast::Binary {
                        left: Box::new(ast),
                        right: Box::new(parser.parse()?),
                        operator: BinaryOperator::BitOr,
                    })
                }),
            },
            PostfixRule {
                id: "BitXorOp".to_string(),
                advance_token: true,
                rule: Arc::new(|parser| parser.check_operator("BIT_XOR")),
                parse: Arc::new(|parser, ast| {
                    Ok(Ast::Binary {
                        left: Box::new(ast),
                        right: Box::new(parser.parse()?),
                        operator: BinaryOperator::BitXor,
                    })
                }),
            },
            PostfixRule {
                id: "BitShlOp".to_string(),
                advance_token: true,
                rule: Arc::new(|parser| parser.check_operator("LEFT_SHIFT")),
                parse: Arc::new(|parser, ast| {
                    Ok(Ast::Binary {
                        left: Box::new(ast),
                        right: Box::new(parser.parse()?),
                        operator: BinaryOperator::BitSHL,
                    })
                }),
            },
            PostfixRule {
                id: "BitShrOp".to_string(),
                advance_token: true,
                rule: Arc::new(|parser| parser.check_operator("RIGHT_SHIFT")),
                parse: Arc::new(|parser, ast| {
                    Ok(Ast::Binary {
                        left: Box::new(ast),
                        right: Box::new(parser.parse()?),
                        operator: BinaryOperator::BitSHR,
                    })
                }),
            },
            PostfixRule {
                id: "FunCall".to_string(),
                advance_token: true,
                rule: Arc::new(|parser| parser.check_operator("LEFT_PAREN")),
                parse: Arc::new(|parser, ast| {
                    let mut args: Vec<Ast> = vec![];

                    if !parser.check_operator("RIGHT_PAREN") {
                        loop {
                            args.push(parser.parse()?);

                            if !parser.match_operator("COMMA") {
                                break;
                            }
                        }
                    }

                    parser.expect_operator("RIGHT_PAREN")?;

                    Ok(Ast::Call {
                        what: Box::new(ast),
                        args,
                    })
                }),
            },
            PostfixRule {
                id: "SetOp".to_string(),
                advance_token: true,
                rule: Arc::new(|parser| parser.check_operator("EQUALS")),
                parse: Arc::new(|parser, ast| {
                    Ok(Ast::Set {
                        name: Box::new(ast),
                        value: Box::new(parser.parse()?),
                    })
                }),
            },
            PostfixRule {
                id: "MacroCall".to_string(),
                advance_token: true,
                rule: Arc::new(|parser| parser.check_operator("BANG")),
                parse: Arc::new(|parser, ast| {
                    let macro_name = if let Ast::Value(Value::Ref(name)) = ast {
                        name
                    } else {
                        return Err(ParserError::TokenMismatch(
                            Token {
                                kind: TokenType::Identifier,
                                lexeme: "".to_string(),
                                span: Span { line: 0, column: 0 },
                            },
                            parser.peek()?,
                        ));
                    };

                    if !parser.match_operator("LEFT_BRACE") {
                        return Err(ParserError::TokenMismatch(
                            Token {
                                kind: TokenType::Operator,
                                lexeme: "LEFT_BRACE".to_string(),
                                span: Span { line: 0, column: 0 },
                            },
                            parser.peek()?,
                        ));
                    }

                    let mut macro_call = vec![];

                    loop {
                        if parser.check_operator("RIGHT_BRACE") {
                            break;
                        }

                        if parser.match_operator("AT")
                            && parser.peek()?.kind == TokenType::Identifier
                        {
                            let mut temp = parser.advance()?;

                            temp.lexeme = format!("@{}", temp.lexeme);

                            macro_call.push(temp);
                            continue;
                        }

                        macro_call.push(parser.advance()?);
                    }

                    parser.handle_macro(&macro_name, macro_call)?;

                    let _ = parser.advance();

                    Ok(Ast::Ignore)
                }),
            },
            PostfixRule {
                id: "EnumFieldExpr".to_string(),
                advance_token: true,
                rule: Arc::new(|parser| parser.check_operator("DOUBLE_COLON")),
                parse: Arc::new(|parser, ast| {
                    if let Ast::Value(Value::Ref(name)) = ast {
                        let tag = parser.expect_kind(TokenType::Identifier)?.lexeme;

                        let mut fields = vec![];

                        if parser.check_operator("LEFT_PAREN") {
                            let _ = parser.advance();
                            loop {
                                fields.push(parser.parse()?);

                                if !parser.match_operator("COMMA") {
                                    break;
                                }
                            }

                            parser.expect_operator("RIGHT_PAREN")?;
                        }

                        Ok(Ast::Value(Value::EnumField { name, tag, fields }))
                    } else {
                        Err(ParserError::RuleNotFound)
                    }
                }),
            },
        ]
    }
}
