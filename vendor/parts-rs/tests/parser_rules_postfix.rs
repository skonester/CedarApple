#[cfg(test)]
mod tests {
    use parts::parser::{Ast, BinaryOperator, EnumVariant, Parser, Value};

    #[test]
    fn test_dot() {
        let mut p = Parser::new("a.b".to_string());
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![Ast::Dot {
                accessor: Box::new(Ast::Value(Value::Ref("a".to_string()))),
                access: Box::new(Ast::Value(Value::Ref("b".to_string()))),
                resolve: false
            }]
        );
    }

    #[test]
    fn test_arr_index() {
        let mut p = Parser::new("a[b]".to_string());
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![Ast::Dot {
                accessor: Box::new(Ast::Value(Value::Ref("a".to_string()))),
                access: Box::new(Ast::Value(Value::Ref("b".to_string()))),
                resolve: true
            }]
        );
    }

    #[test]
    fn test_add() {
        let mut p = Parser::new("a + b".to_string());
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![Ast::Binary {
                left: Box::new(Ast::Value(Value::Ref("a".to_string()))),
                right: Box::new(Ast::Value(Value::Ref("b".to_string()))),
                operator: BinaryOperator::Add
            }]
        );
    }

    #[test]
    fn test_minus() {
        let mut p = Parser::new("a - b".to_string());
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![Ast::Binary {
                left: Box::new(Ast::Value(Value::Ref("a".to_string()))),
                right: Box::new(Ast::Value(Value::Ref("b".to_string()))),
                operator: BinaryOperator::Minus
            }]
        );
    }

    #[test]
    fn test_multiply() {
        let mut p = Parser::new("a * b".to_string());
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![Ast::Binary {
                left: Box::new(Ast::Value(Value::Ref("a".to_string()))),
                right: Box::new(Ast::Value(Value::Ref("b".to_string()))),
                operator: BinaryOperator::Multiply
            }]
        );
    }

    #[test]
    fn test_divide() {
        let mut p = Parser::new("a / b".to_string());
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![Ast::Binary {
                left: Box::new(Ast::Value(Value::Ref("a".to_string()))),
                right: Box::new(Ast::Value(Value::Ref("b".to_string()))),
                operator: BinaryOperator::Divide
            }]
        );
    }

    #[test]
    fn test_equals() {
        let mut p = Parser::new("a == b".to_string());
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![Ast::Binary {
                left: Box::new(Ast::Value(Value::Ref("a".to_string()))),
                right: Box::new(Ast::Value(Value::Ref("b".to_string()))),
                operator: BinaryOperator::Equals
            }]
        );
    }

    #[test]
    fn test_greater_than() {
        let mut p = Parser::new("a > b".to_string());
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![Ast::Binary {
                left: Box::new(Ast::Value(Value::Ref("a".to_string()))),
                right: Box::new(Ast::Value(Value::Ref("b".to_string()))),
                operator: BinaryOperator::GreaterThan
            }]
        );
    }

    #[test]
    fn test_greater_than_or_equal() {
        let mut p = Parser::new("a >= b".to_string());
        let res = p.parse_all();

        let a = Box::new(Ast::Value(Value::Ref("a".to_string())));
        let b = Box::new(Ast::Value(Value::Ref("b".to_string())));

        assert_eq!(
            res.unwrap(),
            vec![Ast::Binary {
                left: a,
                right: b,
                operator: BinaryOperator::GreaterThanOrEqual
            }]
        );
    }

    #[test]
    fn test_less_than() {
        let mut p = Parser::new("a < b".to_string());
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![Ast::Binary {
                left: Box::new(Ast::Value(Value::Ref("a".to_string()))),
                right: Box::new(Ast::Value(Value::Ref("b".to_string()))),
                operator: BinaryOperator::LessThan
            }]
        );
    }

    #[test]
    fn test_less_than_or_equal() {
        let mut p = Parser::new("a <= b".to_string());
        let res = p.parse_all();

        let a = Box::new(Ast::Value(Value::Ref("a".to_string())));
        let b = Box::new(Ast::Value(Value::Ref("b".to_string())));

        assert_eq!(
            res.unwrap(),
            vec![Ast::Binary {
                left: a,
                right: b,
                operator: BinaryOperator::LessThanOrEqual
            }]
        );
    }

    #[test]
    fn test_modulo() {
        let mut p = Parser::new("a % b".to_string());
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![Ast::Binary {
                left: Box::new(Ast::Value(Value::Ref("a".to_string()))),
                right: Box::new(Ast::Value(Value::Ref("b".to_string()))),
                operator: BinaryOperator::Modulo
            }]
        );
    }

    #[test]
    fn test_call_no_args() {
        let mut p = Parser::new("a()".to_string());
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![Ast::Call {
                what: Box::new(Ast::Value(Value::Ref("a".to_string()))),
                args: vec![]
            }]
        )
    }

    #[test]
    fn test_call_one_arg() {
        let mut p = Parser::new("a(1)".to_string());
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![Ast::Call {
                what: Box::new(Ast::Value(Value::Ref("a".to_string()))),
                args: vec![Ast::Value(Value::Int(1))]
            }]
        )
    }

    #[test]
    fn test_call_two_args() {
        let mut p = Parser::new("a(1,2)".to_string());
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![Ast::Call {
                what: Box::new(Ast::Value(Value::Ref("a".to_string()))),
                args: vec![Ast::Value(Value::Int(1)), Ast::Value(Value::Int(2))]
            }]
        )
    }

    #[test]
    fn test_ufcs_parse() {
        let mut p = Parser::new("a.f(1)".to_string());
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![Ast::Call {
                what: Box::new(Ast::Dot {
                    accessor: Box::new(Ast::Value(Value::Ref("a".to_string()))),
                    access: Box::new(Ast::Value(Value::Ref("f".to_string()))),
                    resolve: false
                }),
                args: vec![Ast::Value(Value::Int(1))]
            }]
        );
    }

    #[test]
    fn test_set() {
        let mut p = Parser::new("x = 0".to_string());
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![Ast::Set {
                name: Box::new(Ast::Value(Value::Ref("x".to_string()))),
                value: Box::new(Ast::Value(Value::Int(0)))
            }]
        );
    }

    #[test]
    fn test_enum_field_access() {
        let mut p = Parser::new(
            "
            enum A { A };   
            A::A
            "
            .to_string(),
        );
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![
                Ast::EnumDef {
                    name: "A".to_string(),
                    variants: vec![EnumVariant {
                        name: "A".to_string(),
                        fields: vec![],
                    }],
                },
                Ast::Value(Value::EnumField {
                    name: "A".to_string(),
                    tag: "A".to_string(),
                    fields: vec![]
                })
            ]
        )
    }

    #[test]
    fn test_enum_field_access_with_field() {
        let mut p = Parser::new(
            "
            enum A { A(a) }   
            A::A(0)
            "
            .to_string(),
        );
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![
                Ast::EnumDef {
                    name: "A".to_string(),
                    variants: vec![EnumVariant {
                        name: "A".to_string(),
                        fields: vec!["a".to_string()],
                    }],
                },
                Ast::Value(Value::EnumField {
                    name: "A".to_string(),
                    tag: "A".to_string(),
                    fields: vec![Ast::Value(Value::Int(0))]
                })
            ]
        )
    }

    #[test]
    fn test_enum_field_access_with_fields() {
        let mut p = Parser::new(
            "
            enum A { A(a, b) }   
            A::A(0, 1)
            "
            .to_string(),
        );
        let res = p.parse_all();

        assert_eq!(
            res.unwrap(),
            vec![
                Ast::EnumDef {
                    name: "A".to_string(),
                    variants: vec![EnumVariant {
                        name: "A".to_string(),
                        fields: vec!["a".to_string(), "b".to_string()],
                    }],
                },
                Ast::Value(Value::EnumField {
                    name: "A".to_string(),
                    tag: "A".to_string(),
                    fields: vec![Ast::Value(Value::Int(0)), Ast::Value(Value::Int(1))]
                })
            ]
        )
    }
}
