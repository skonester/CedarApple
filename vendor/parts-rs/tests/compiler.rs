#[cfg(test)]
mod tests {

    use std::cell::RefCell;
    use std::rc::Rc;

    use parts::compiler::Compiler;
    use parts::compiler::IrOp;
    use parts::emitter::Emitter;
    use parts::parser::Ast;
    use parts::parser::BinaryOperator;
    use parts::parser::Value as ParserValue;
    use parts::value::{Function, Value};
    use rustc_hash::FxHashMap;

    fn assert_compile(src: Vec<Ast>, expected: Vec<IrOp>) {
        let mut c = Compiler::new("./".into());
        let res = c.compile_all(src).expect("Error during AST compilation");

        assert_eq!(res, expected, "Bytecode mismatch");
    }

    fn assert_compile_constants(
        src: Vec<Ast>,
        expected: Vec<IrOp>,
        expected_pool_entries: Vec<(usize, Value)>,
    ) {
        let mut c = Compiler::new("./".into());
        let res = c.compile_all(src).expect("Error during AST compilation");

        assert_eq!(res, expected, "Bytecode mismatch");

        println!("Printing pool");
        for x in &c.constant_pool {
            println!("{:?}", x);
        }

        for (index, expected_value) in expected_pool_entries {
            let actual_value = c
                .constant_pool
                .get(index)
                .unwrap_or_else(|| panic!("Constant {} missing", index));

            assert_eq!(
                actual_value, &expected_value,
                "Index at {} doesn't match!",
                index
            );
        }
    }

    #[test]
    fn check_empty() {
        assert_compile(vec![], vec![]);
    }

    #[test]
    fn check_declaration() {
        assert_compile(
            vec![Ast::Declare {
                name: "x".to_string(),
                value: Box::new(Ast::Value(ParserValue::Bool(false))),
            }],
            vec![IrOp::LoadBool {
                dest: 0,
                val: false,
            }],
        );
    }

    #[test]
    fn check_inline_value() {
        assert_compile(
            vec![Ast::Value(ParserValue::Int(0))],
            vec![IrOp::LoadInt { dest: 0, val: 0 }],
        );
    }

    #[test]
    fn check_string_value() {
        assert_compile_constants(
            vec![Ast::Value(ParserValue::String("parts".to_string()))],
            vec![IrOp::LoadConst { dest: 0, idx: 0 }],
            vec![(0, Value::String(Rc::new("parts".to_string())))],
        );
    }

    #[test]
    fn check_duplicate_string_value() {
        assert_compile_constants(
            vec![
                Ast::Value(ParserValue::String("parts".to_string())),
                Ast::Value(ParserValue::String("parts".to_string())),
            ],
            vec![
                IrOp::LoadConst { dest: 0, idx: 0 },
                IrOp::LoadConst { dest: 0, idx: 0 },
            ],
            vec![(0, Value::String(Rc::new("parts".to_string())))],
        );
    }

    #[test]
    fn check_multi_string_value() {
        assert_compile_constants(
            vec![
                Ast::Value(ParserValue::String("parts".to_string())),
                Ast::Value(ParserValue::String("rust".to_string())),
                Ast::Value(ParserValue::String("parts".to_string())),
            ],
            vec![
                IrOp::LoadConst { dest: 0, idx: 0 },
                IrOp::LoadConst { dest: 0, idx: 1 },
                IrOp::LoadConst { dest: 0, idx: 0 },
            ],
            vec![
                (0, Value::String(Rc::new("parts".to_string()))),
                (1, Value::String(Rc::new("rust".to_string()))),
            ],
        );
    }

    #[test]
    fn check_ref() {
        assert_compile(
            vec![
                Ast::Declare {
                    name: "x".to_string(),
                    value: Box::new(Ast::Value(ParserValue::Bool(false))),
                },
                Ast::Declare {
                    name: "y".to_string(),
                    value: Box::new(Ast::Value(ParserValue::Ref("x".to_string()))),
                },
            ],
            vec![
                IrOp::LoadBool {
                    dest: 0,
                    val: false,
                },
                IrOp::LoadReg { dest: 1, src: 0 },
            ],
        );
    }

    #[test]
    fn check_fun_no_args() {
        assert_compile_constants(
            vec![Ast::Value(ParserValue::Fun {
                args: vec![],
                body: Box::new(Ast::Return {
                    value: Box::new(Ast::Value(ParserValue::Bool(false))),
                }),
            })],
            vec![IrOp::LoadConst { dest: 0, idx: 0 }],
            vec![(
                0,
                Value::Fun(Rc::new(Function {
                    arity: 0,
                    frame_size: 1,
                    code: Emitter {}
                        .emit(vec![
                            IrOp::LoadBool {
                                dest: 0,
                                val: false,
                            },
                            IrOp::Return { value: 0 },
                        ])
                        .into(),
                })),
            )],
        );
    }

    #[test]
    fn check_fun_one_arg() {
        assert_compile_constants(
            vec![Ast::Value(ParserValue::Fun {
                args: vec!["n".to_string()],
                body: Box::new(Ast::Return {
                    value: Box::new(Ast::Value(ParserValue::Bool(false))),
                }),
            })],
            vec![IrOp::LoadConst { dest: 0, idx: 0 }],
            vec![(
                0,
                Value::Fun(Rc::new(Function {
                    arity: 1,
                    frame_size: 2,
                    code: Emitter {}
                        .emit(vec![
                            IrOp::LoadBool {
                                dest: 1,
                                val: false,
                            },
                            IrOp::Return { value: 1 },
                        ])
                        .into(),
                })),
            )],
        );
    }

    #[test]
    fn check_fun_multiple_args() {
        assert_compile_constants(
            vec![Ast::Value(ParserValue::Fun {
                args: vec!["n".to_string(), "i".to_string()],
                body: Box::new(Ast::Return {
                    value: Box::new(Ast::Value(ParserValue::Bool(false))),
                }),
            })],
            vec![IrOp::LoadConst { dest: 0, idx: 0 }],
            vec![(
                0,
                Value::Fun(Rc::new(Function {
                    arity: 2,
                    frame_size: 3,
                    code: Emitter {}
                        .emit(vec![
                            IrOp::LoadBool {
                                dest: 2,
                                val: false,
                            },
                            IrOp::Return { value: 2 },
                        ])
                        .into(),
                })),
            )],
        );
    }

    #[test]
    fn check_return() {
        assert_compile(
            vec![Ast::Return {
                value: Box::new(Ast::Value(ParserValue::Bool(false))),
            }],
            vec![
                IrOp::LoadBool {
                    dest: 0,
                    val: false,
                },
                IrOp::Return { value: 0 },
            ],
        );
    }

    #[test]
    fn check_call() {
        assert_compile(
            vec![Ast::Call {
                what: Box::new(Ast::Value(ParserValue::String("fib".to_string()))),
                args: vec![Ast::Value(ParserValue::Int(0))],
            }],
            vec![
                IrOp::LoadConst { dest: 0, idx: 0 },
                IrOp::LoadInt { dest: 1, val: 0 },
                IrOp::Call {
                    what: 0,
                    dest: 2,
                    args: vec![1],
                },
            ],
        );
    }

    #[test]
    fn check_binary() {
        assert_compile(
            vec![Ast::Binary {
                left: Box::new(Ast::Value(ParserValue::Bool(false))),
                right: Box::new(Ast::Value(ParserValue::Bool(false))),
                operator: BinaryOperator::Add,
            }],
            vec![
                IrOp::LoadBool {
                    dest: 0,
                    val: false,
                },
                IrOp::LoadBool {
                    dest: 1,
                    val: false,
                },
                IrOp::Binary {
                    dest: 2,
                    op: BinaryOperator::Add,
                    left: 0,
                    right: 1,
                },
            ],
        );
    }

    #[test]
    fn check_if() {
        assert_compile(
            vec![Ast::If {
                then_branch: Box::new(Ast::Value(ParserValue::Bool(true))),
                else_branch: Some(Box::new(Ast::Value(ParserValue::Bool(false)))),
                condition: Box::new(Ast::Value(ParserValue::Bool(false))),
            }],
            vec![
                IrOp::LoadBool {
                    dest: 0,
                    val: false,
                },
                IrOp::JumpNot {
                    target: 0,
                    condition: 0,
                },
                IrOp::LoadBool { dest: 1, val: true },
                IrOp::Jump { target: 1 },
                IrOp::Label(0),
                IrOp::LoadBool {
                    dest: 2,
                    val: false,
                },
                IrOp::Label(1),
            ],
        );
    }

    #[test]
    fn check_if_no_else() {
        assert_compile(
            vec![Ast::If {
                then_branch: Box::new(Ast::Value(ParserValue::Bool(true))),
                else_branch: None,
                condition: Box::new(Ast::Value(ParserValue::Bool(false))),
            }],
            vec![
                IrOp::LoadBool {
                    dest: 0,
                    val: false,
                },
                IrOp::JumpNot {
                    target: 0,
                    condition: 0,
                },
                IrOp::LoadBool { dest: 1, val: true },
                IrOp::Label(0),
            ],
        );
    }

    #[test]
    fn check_continue() {
        assert_compile(
            vec![Ast::For {
                condition: Box::new(Ast::Value(ParserValue::Bool(false))),
                body: (Box::new(Ast::ContinueCode)),
            }],
            vec![
                IrOp::Label(0),
                IrOp::LoadBool {
                    dest: 0,
                    val: false,
                },
                IrOp::JumpNot {
                    target: 1,
                    condition: 0,
                },
                IrOp::Jump { target: 0 },
                IrOp::Jump { target: 0 },
                IrOp::Label(1),
            ],
        );
    }

    #[test]
    fn check_break() {
        assert_compile(
            vec![Ast::For {
                condition: Box::new(Ast::Value(ParserValue::Bool(false))),
                body: (Box::new(Ast::BreakCode)),
            }],
            vec![
                IrOp::Label(0),
                IrOp::LoadBool {
                    dest: 0,
                    val: false,
                },
                IrOp::JumpNot {
                    target: 1,
                    condition: 0,
                },
                IrOp::Jump { target: 1 },
                IrOp::Jump { target: 0 },
                IrOp::Label(1),
            ],
        );
    }

    #[test]
    fn check_obj_no_entries() {
        assert_compile_constants(
            vec![Ast::Value(ParserValue::Object(vec![]))],
            vec![IrOp::LoadConst { dest: 0, idx: 0 }],
            vec![(
                0,
                Value::Object(Rc::new(RefCell::new(FxHashMap::default()))),
            )],
        );
    }

    #[test]
    fn check_obj_with_entries() {
        let mut m = FxHashMap::default();

        m.insert(Value::Int(100).get_hash(), Value::Bool(false));

        assert_compile_constants(
            vec![Ast::Value(ParserValue::Object(vec![(
                ParserValue::Int(100),
                ParserValue::Bool(false),
            )]))],
            vec![IrOp::LoadConst { dest: 0, idx: 0 }],
            vec![(0, Value::Object(Rc::new(RefCell::new(m))))],
        );
    }

    #[test]
    fn check_assign_no_declaration() {
        assert_compile(
            vec![Ast::Set {
                name: Box::from(Ast::Value(ParserValue::Ref("x".to_string()))),
                value: Box::from(Ast::Value(ParserValue::Bool(false))),
            }],
            vec![IrOp::LoadBool {
                dest: 0,
                val: false,
            }],
        );
    }

    #[test]
    fn check_assign_with_declaration() {
        assert_compile(
            vec![
                Ast::Declare {
                    name: "x".to_string(),
                    value: Box::new(Ast::Value(ParserValue::Bool(true))),
                },
                Ast::Set {
                    name: Box::from(Ast::Value(ParserValue::Ref("x".to_string()))),
                    value: Box::from(Ast::Value(ParserValue::Bool(false))),
                },
            ],
            vec![
                IrOp::LoadBool { dest: 0, val: true },
                IrOp::LoadBool {
                    dest: 0,
                    val: false,
                },
            ],
        );
    }
    #[test]
    fn check_enum_definition_and_load() {
        use parts::parser::EnumVariant;
        assert_compile(
            vec![
                Ast::EnumDef {
                    name: "Direction".to_string(),
                    variants: vec![EnumVariant {
                        name: "North".to_string(),
                        fields: vec!["power".to_string()],
                    }],
                },
                Ast::Declare {
                    name: "d".to_string(),
                    value: Box::new(Ast::Value(ParserValue::EnumField {
                        name: "Direction".to_string(),
                        tag: "North".to_string(),
                        fields: vec![Ast::Value(ParserValue::Int(100))],
                    })),
                },
            ],
            vec![
                IrOp::LoadInt { dest: 1, val: 100 },
                IrOp::LoadEnumField {
                    dest: 0,
                    enum_idx: 2,
                    tag: 0,
                    args: vec![(Value::String(Rc::new("power".to_string())).get_hash(), 1)],
                },
            ],
        );
    }

    #[test]
    fn check_enum_direct_value_and_return() {
        use parts::parser::EnumVariant;
        assert_compile(
            vec![
                Ast::EnumDef {
                    name: "Result".to_string(),
                    variants: vec![EnumVariant {
                        name: "Ok".to_string(),
                        fields: vec!["val".to_string()],
                    }],
                },
                Ast::Return {
                    value: Box::new(Ast::Value(ParserValue::EnumField {
                        name: "Result".to_string(),
                        tag: "Ok".to_string(),
                        fields: vec![Ast::Value(ParserValue::Int(42))],
                    })),
                },
            ],
            vec![
                IrOp::LoadInt { dest: 1, val: 42 },
                IrOp::LoadEnumField {
                    dest: 0,
                    enum_idx: 0,
                    tag: 0,
                    args: vec![(Value::String(Rc::new("val".to_string())).get_hash(), 1)],
                },
                IrOp::Return { value: 0 },
            ],
        );
    }

    #[test]
    fn check_compile_match_enum() {
        use parts::parser::{EnumVariant, MatchArm, MatchPattern};
        let mut c = Compiler::new("./".into());
        let ast = vec![
            Ast::EnumDef {
                name: "Direction".to_string(),
                variants: vec![EnumVariant {
                    name: "North".to_string(),
                    fields: vec!["power".to_string()],
                }],
            },
            Ast::Declare {
                name: "d".to_string(),
                value: Box::new(Ast::Value(ParserValue::EnumField {
                    name: "Direction".to_string(),
                    tag: "North".to_string(),
                    fields: vec![Ast::Value(ParserValue::Int(100))],
                })),
            },
            Ast::Match {
                target: Box::new(Ast::Value(ParserValue::Ref("d".to_string()))),
                arms: vec![
                    MatchArm {
                        pattern: MatchPattern::Enum {
                            name: "Direction".to_string(),
                            tag: "North".to_string(),
                            fields: vec!["p".to_string()],
                        },
                        body: Box::new(Ast::Value(ParserValue::Ref("p".to_string()))),
                    },
                    MatchArm {
                        pattern: MatchPattern::Wildcard(None),
                        body: Box::new(Ast::Value(ParserValue::Int(0))),
                    },
                ],
            },
        ];

        let ir = c.compile_all(ast).expect("compilation failed");
        assert!(ir.iter().any(|op| matches!(op, IrOp::MatchEnum { .. })));
    }

    #[test]
    fn check_comments_removed_from_const_pool() {
        use parts::parser::Parser;
        let source = r#"
            "Module documentation comment"
            let a = "actual string 1";
            let add(x) = {
                "Inside function doc"
                return x;
            }
            "End of file comment"
        "#;
        let mut p = Parser::new(source.to_string());
        let ast = p.parse_all().unwrap();
        let mut c = Compiler::new("./".into());
        let _ = c.compile_all(ast).unwrap();

        for val in &c.constant_pool {
            if let Value::String(s) = val {
                assert_ne!(s.as_str(), "Module documentation comment");
                assert_ne!(s.as_str(), "Inside function doc");
                assert_ne!(s.as_str(), "End of file comment");
            }
        }
    }
}
