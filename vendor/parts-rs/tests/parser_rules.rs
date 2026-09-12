#[cfg(test)]
mod tests {
    use parts::parser::{Ast, EnumVariant, Parser, Value};

    #[test]
    fn test_semicolon() {
        let mut p = Parser::new(";".to_string());

        let res = p.parse_all();

        let val = res.unwrap();

        assert_eq!(val.len(), 0);
    }

    #[test]
    fn test_object_empty() {
        let mut p = Parser::new("|> <|".to_string());
        let res = p.parse_all();

        let val = res.unwrap();

        assert_eq!(val.len(), 1);

        if let Some(Ast::Object(inner_obj)) = val.first() {
            assert_eq!(inner_obj.len(), 0);
        } else {
            panic!("Expected Ast::Object at 0, got {:?}", val.first());
        }
    }

    #[test]
    fn test_object_one_entry() {
        let mut p = Parser::new("|> expectFalse: false <|".to_string());
        let res = p.parse_all();

        let val = res.unwrap();
        assert_eq!(val.len(), 1);

        if let Some(Ast::Object(inner_obj)) = val.first() {
            assert_eq!(
                inner_obj.clone(),
                vec![(
                    Ast::Value(Value::Ref("expectFalse".to_string())),
                    Ast::Value(Value::Bool(false))
                )]
            );
        } else {
            panic!("Expected Ast::Object at 0, got {:?}", val.first());
        }
    }

    #[test]
    fn test_object_two_entries() {
        let mut p = Parser::new("|> expectFalse: false, expectTrue: true <|".to_string());
        let res = p.parse_all();

        let val = res.unwrap();
        assert_eq!(val.len(), 1);

        if let Some(Ast::Object(inner_obj)) = val.first() {
            assert_eq!(
                inner_obj.clone(),
                vec![
                    (
                        Ast::Value(Value::Ref("expectFalse".to_string())),
                        Ast::Value(Value::Bool(false))
                    ),
                    (
                        Ast::Value(Value::Ref("expectTrue".to_string())),
                        Ast::Value(Value::Bool(true))
                    )
                ]
            );
        } else {
            panic!("Expected Ast::Object at 0, got {:?}", val.first());
        }
    }

    #[test]
    fn test_group() {
        let mut p = Parser::new("(false)".to_string());
        let res = p.parse_all();

        assert_eq!(res.unwrap(), vec![Ast::Value(Value::Bool(false))]);
    }

    #[test]
    fn test_var() {
        let mut p = Parser::new("x".to_string());
        let res = p.parse_all();

        assert_eq!(res.unwrap(), vec![Ast::Value(Value::Ref("x".to_string()))]);
    }

    #[test]
    fn test_string() {
        let mut p = Parser::new("`x`".to_string());
        let res = p.parse_all();

        assert_eq!(res.unwrap(), vec![]);
    }

    #[test]
    fn test_number() {
        let mut p = Parser::new("0".to_string());
        let res = p.parse_all();

        assert_eq!(res.unwrap(), vec![Ast::Value(Value::Int(0))]);
    }

    #[test]
    fn test_decimal() {
        let mut p = Parser::new("0.1".to_string());
        let res = p.parse_all();

        assert_eq!(res.unwrap(), vec![Ast::Value(Value::Double(0.1))]);
    }

    #[test]
    fn test_false() {
        let mut p = Parser::new("false".to_string());
        let res = p.parse_all();

        assert_eq!(res.unwrap(), vec![Ast::Value(Value::Bool(false))]);
    }

    #[test]
    fn test_true() {
        let mut p = Parser::new("true".to_string());
        let res = p.parse_all();

        assert_eq!(res.unwrap(), vec![Ast::Value(Value::Bool(true))]);
    }

    #[test]
    fn test_continue() {
        let mut p = Parser::new("continue".to_string());
        let res = p.parse_all();

        assert_eq!(res.unwrap(), vec![Ast::ContinueCode]);
    }

    #[test]
    fn test_break() {
        let mut p = Parser::new("break".to_string());
        let res = p.parse_all();

        assert_eq!(res.unwrap(), vec![Ast::BreakCode]);
    }

    #[test]
    fn test_return_no_value() {
        let mut p = Parser::new("return;".to_string());
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![Ast::Return {
                value: Box::new(Ast::Ignore)
            }]
        );
    }

    #[test]
    fn test_return_with_value() {
        let mut p = Parser::new("return 0".to_string());
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![Ast::Return {
                value: Box::new(Ast::Value(Value::Int(0)))
            }]
        );
    }

    #[test]
    fn test_block_empty() {
        let mut p = Parser::new("{}".to_string());
        let res = p.parse_all();

        assert_eq!(res.unwrap(), vec![Ast::Block { code: vec![] }]);
    }

    #[test]
    fn test_block_with_code() {
        let mut p = Parser::new("{ return 0 }".to_string());
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![Ast::Block {
                code: vec![Ast::Return {
                    value: Box::new(Ast::Value(Value::Int(0)))
                }]
            }]
        );
    }

    #[test]
    fn test_for() {
        let mut p = Parser::new("for true { return 0 }".to_string());
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![Ast::For {
                condition: Box::new(Ast::Value(Value::Bool(true))),
                body: Box::new(Ast::Block {
                    code: vec![Ast::Return {
                        value: Box::new(Ast::Value(Value::Int(0)))
                    }]
                })
            }]
        );
    }

    #[test]
    fn test_for_without_braces() {
        let mut p = Parser::new("for true return 0".to_string());
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![Ast::For {
                condition: Box::new(Ast::Value(Value::Bool(true))),
                body: Box::new(Ast::Return {
                    value: Box::new(Ast::Value(Value::Int(0)))
                })
            }]
        );
    }

    #[test]
    fn test_for_each() {
        let mut p = Parser::new("for x in y { return 0 }".to_string());
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![Ast::ForEach {
                body: Box::new(Ast::Block {
                    code: vec![Ast::Return {
                        value: Box::new(Ast::Value(Value::Int(0)))
                    }]
                }),
                iterable: Box::new(Ast::Value(Value::Ref("y".to_string()))),
                var_name: "x".to_string()
            }]
        );
    }

    #[test]
    fn test_if() {
        let mut p = Parser::new("if true { return 0 } else {return 1}".to_string());
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![Ast::If {
                condition: Box::new(Ast::Value(Value::Bool(true))),
                then_branch: Box::new(Ast::Block {
                    code: vec![Ast::Return {
                        value: Box::new(Ast::Value(Value::Int(0)))
                    }]
                }),
                else_branch: Some(Box::new(Ast::Block {
                    code: vec![Ast::Return {
                        value: Box::new(Ast::Value(Value::Int(1)))
                    }]
                })),
            }]
        );
    }

    #[test]
    fn test_if_no_else() {
        let mut p = Parser::new("if true { return 0 }".to_string());
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![Ast::If {
                condition: Box::new(Ast::Value(Value::Bool(true))),
                then_branch: Box::new(Ast::Block {
                    code: vec![Ast::Return {
                        value: Box::new(Ast::Value(Value::Int(0)))
                    }]
                }),
                else_branch: None,
            }]
        );
    }

    #[test]
    fn test_fun_no_arguments_short() {
        let mut p = Parser::new("fun () = 0;".to_string());
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![Ast::Value(Value::Fun {
                args: vec![],
                body: Box::new(Ast::Return {
                    value: Box::new(Ast::Value(Value::Int(0)))
                })
            })]
        );
    }

    #[test]
    fn test_fun_no_arguments() {
        let mut p = Parser::new("fun () {return 0}".to_string());
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![Ast::Value(Value::Fun {
                args: vec![],
                body: Box::new(Ast::Block {
                    code: vec![Ast::Return {
                        value: Box::new(Ast::Value(Value::Int(0)))
                    }]
                })
            })]
        );
    }

    #[test]
    fn test_fun_one_argument_short() {
        let mut p = Parser::new("fun (x) = 0;".to_string());
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![Ast::Value(Value::Fun {
                args: vec!["x".to_string()],
                body: Box::new(Ast::Return {
                    value: Box::new(Ast::Value(Value::Int(0)))
                })
            })]
        );
    }

    #[test]
    fn test_fun_one_argument() {
        let mut p = Parser::new("fun (x) {return 0}".to_string());
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![Ast::Value(Value::Fun {
                args: vec!["x".to_string()],
                body: Box::new(Ast::Block {
                    code: vec![Ast::Return {
                        value: Box::new(Ast::Value(Value::Int(0)))
                    }]
                })
            })]
        );
    }

    #[test]
    fn test_fun_two_arguments_short() {
        let mut p = Parser::new("fun (x, y) = 0;".to_string());
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![Ast::Value(Value::Fun {
                args: vec!["x".to_string(), "y".to_string()],
                body: Box::new(Ast::Return {
                    value: Box::new(Ast::Value(Value::Int(0)))
                })
            })]
        );
    }

    #[test]
    fn test_fun_two_arguments() {
        let mut p = Parser::new("fun (x, y) {return 0}".to_string());
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![Ast::Value(Value::Fun {
                args: vec!["x".to_string(), "y".to_string()],
                body: Box::new(Ast::Block {
                    code: vec![Ast::Return {
                        value: Box::new(Ast::Value(Value::Int(0)))
                    }]
                })
            })]
        );
    }

    #[test]
    fn test_let() {
        let mut p = Parser::new("let x = 0".to_string());
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![Ast::Declare {
                name: "x".to_string(),
                value: Box::new(Ast::Value(Value::Int(0)))
            }]
        );
    }

    #[test]
    fn test_let_function_no_args_short() {
        let mut p = Parser::new("let x() = 0".to_string());
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![Ast::Declare {
                name: "x".to_string(),
                value: Box::new(Ast::Value(Value::Fun {
                    args: vec![],
                    body: Box::new(Ast::Return {
                        value: Box::new(Ast::Value(Value::Int(0)))
                    })
                }))
            }]
        );
    }

    #[test]
    fn test_let_function_no_args() {
        let mut p = Parser::new("let x() { return 0 }".to_string());
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![Ast::Declare {
                name: "x".to_string(),
                value: Box::new(Ast::Value(Value::Fun {
                    args: vec![],
                    body: Box::new(Ast::Block {
                        code: vec![Ast::Return {
                            value: Box::new(Ast::Value(Value::Int(0)))
                        }]
                    })
                }))
            }]
        );
    }

    #[test]
    fn test_let_function_one_arg_short() {
        let mut p = Parser::new("let x(x) = 0".to_string());
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![Ast::Declare {
                name: "x".to_string(),
                value: Box::new(Ast::Value(Value::Fun {
                    args: vec!["x".to_string()],
                    body: Box::new(Ast::Return {
                        value: Box::new(Ast::Value(Value::Int(0)))
                    })
                }))
            }]
        );
    }

    #[test]
    fn test_let_function_one_arg() {
        let mut p = Parser::new("let x(x) { return 0 }".to_string());
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![Ast::Declare {
                name: "x".to_string(),
                value: Box::new(Ast::Value(Value::Fun {
                    args: vec!["x".to_string()],
                    body: Box::new(Ast::Block {
                        code: vec![Ast::Return {
                            value: Box::new(Ast::Value(Value::Int(0)))
                        }]
                    })
                }))
            }]
        );
    }

    #[test]
    fn test_let_function_two_args_short() {
        let mut p = Parser::new("let x(x, y) = 0".to_string());
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![Ast::Declare {
                name: "x".to_string(),
                value: Box::new(Ast::Value(Value::Fun {
                    args: vec!["x".to_string(), "y".to_string()],
                    body: Box::new(Ast::Return {
                        value: Box::new(Ast::Value(Value::Int(0)))
                    })
                }))
            }]
        );
    }

    #[test]
    fn test_let_function_two_args() {
        let mut p = Parser::new("let x(x, y) { return 0 }".to_string());
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![Ast::Declare {
                name: "x".to_string(),
                value: Box::new(Ast::Value(Value::Fun {
                    args: vec!["x".to_string(), "y".to_string()],
                    body: Box::new(Ast::Block {
                        code: vec![Ast::Return {
                            value: Box::new(Ast::Value(Value::Int(0)))
                        }]
                    })
                }))
            }]
        );
    }

    #[test]
    fn test_enum_declaration_no_variants() {
        let mut p = Parser::new("enum A".to_string());
        let res = p.parse_all();
        println!("{:?}", res);

        assert_eq!(
            res.unwrap(),
            vec![Ast::EnumDef {
                name: "A".to_string(),
                variants: vec![],
            }]
        )
    }

    #[test]
    fn test_enum_declaration_no_variants_other() {
        let mut p = Parser::new("enum A {}".to_string());
        let res = p.parse_all();
        println!("{:?}", res);

        assert_eq!(
            res.unwrap(),
            vec![Ast::EnumDef {
                name: "A".to_string(),
                variants: vec![],
            }]
        )
    }

    #[test]
    fn test_enum_declaration_one_variant() {
        let mut p = Parser::new("enum A { A }".to_string());
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![Ast::EnumDef {
                name: "A".to_string(),
                variants: vec![EnumVariant {
                    name: "A".to_string(),
                    fields: vec![],
                }],
            }]
        )
    }

    #[test]
    fn test_enum_declaration_one_variant_with_fields() {
        let mut p = Parser::new("enum A { A(A, B) }".to_string());
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![Ast::EnumDef {
                name: "A".to_string(),
                variants: vec![EnumVariant {
                    name: "A".to_string(),
                    fields: vec!["A".to_string(), "B".to_string()],
                }],
            }]
        )
    }

    #[test]
    fn test_enum_declaration_one_variant_with_field() {
        let mut p = Parser::new("enum A { A(A) }".to_string());
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![Ast::EnumDef {
                name: "A".to_string(),
                variants: vec![EnumVariant {
                    name: "A".to_string(),
                    fields: vec!["A".to_string()],
                }],
            }]
        )
    }

    #[test]
    fn test_enum_declaration_multiplee_variants_with_fields() {
        let mut p = Parser::new("enum A {A(A), B(A)}".to_string());
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![Ast::EnumDef {
                name: "A".to_string(),
                variants: vec![
                    EnumVariant {
                        name: "A".to_string(),
                        fields: vec!["A".to_string()],
                    },
                    EnumVariant {
                        name: "B".to_string(),
                        fields: vec!["A".to_string()],
                    }
                ],
            }]
        )
    }

    #[test]
    fn test_match_parser_enum_arms() {
        use parts::parser::{MatchArm, MatchPattern};
        let mut p = Parser::new("match d { Direction::North(p) => p * 2, _ => 0 }".to_string());
        let res = p.parse_all().unwrap();

        assert_eq!(
            res,
            vec![Ast::Match {
                target: Box::new(Ast::Value(Value::Ref("d".to_string()))),
                arms: vec![
                    MatchArm {
                        pattern: MatchPattern::Enum {
                            name: "Direction".to_string(),
                            tag: "North".to_string(),
                            fields: vec!["p".to_string()],
                        },
                        body: Box::new(Ast::Binary {
                            left: Box::new(Ast::Value(Value::Ref("p".to_string()))),
                            right: Box::new(Ast::Value(Value::Int(2))),
                            operator: parts::parser::BinaryOperator::Multiply,
                        }),
                    },
                    MatchArm {
                        pattern: MatchPattern::Wildcard(None),
                        body: Box::new(Ast::Value(Value::Int(0))),
                    },
                ],
            }]
        );
    }

    #[test]
    fn test_match_parser_unit_and_block_arm() {
        use parts::parser::{MatchArm, MatchPattern};
        let mut p =
            Parser::new("match s { Status::Active => { return 1 }, other => other }".to_string());
        let res = p.parse_all().unwrap();

        assert_eq!(
            res,
            vec![Ast::Match {
                target: Box::new(Ast::Value(Value::Ref("s".to_string()))),
                arms: vec![
                    MatchArm {
                        pattern: MatchPattern::Enum {
                            name: "Status".to_string(),
                            tag: "Active".to_string(),
                            fields: vec![],
                        },
                        body: Box::new(Ast::Block {
                            code: vec![Ast::Return {
                                value: Box::new(Ast::Value(Value::Int(1))),
                            }],
                        }),
                    },
                    MatchArm {
                        pattern: MatchPattern::Wildcard(Some("other".to_string())),
                        body: Box::new(Ast::Value(Value::Ref("other".to_string()))),
                    },
                ],
            }]
        );
    }
}
