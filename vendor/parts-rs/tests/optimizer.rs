#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use parts::engine::Engine;
    use parts::value::Value;

    thread_local! {
        static SIDE_EFFECT_COUNTER: Cell<i64> = const { Cell::new(0) };
    }

    fn make_engine_with_counter() -> Engine {
        SIDE_EFFECT_COUNTER.with(|c| c.set(0));
        let mut engine = Engine::new();
        engine.register_native("bump", 0, |_args| {
            let v = SIDE_EFFECT_COUNTER.with(|c| {
                let n = c.get() + 1;
                c.set(n);
                n
            });
            Ok(Value::Int(v))
        });
        engine.register_native("peek", 0, |_args| {
            Ok(Value::Int(SIDE_EFFECT_COUNTER.with(|c| c.get())))
        });
        engine.register_native("negbump", 0, |_args| {
            let v = SIDE_EFFECT_COUNTER.with(|c| {
                let n = c.get() + 1;
                c.set(n);
                n
            });
            Ok(Value::Int(-v))
        });
        engine
    }

    fn exec_int(engine: &Engine, source: &str) -> i64 {
        match engine.execute(source) {
            Ok(Some(Value::Int(n))) => n,
            Ok(Some(other)) => panic!("expected Int, got {:?}", other),
            Ok(None) => panic!("expected Int, got None"),
            Err(e) => panic!("execution failed: {}", e),
        }
    }

    fn exec_bool(engine: &Engine, source: &str) -> bool {
        match engine.execute(source) {
            Ok(Some(Value::Bool(b))) => b,
            Ok(Some(other)) => panic!("expected Bool, got {:?}", other),
            Ok(None) => panic!("expected Bool, got None"),
            Err(e) => panic!("execution failed: {}", e),
        }
    }

    // --- Modulo on negative dividends ---

    #[test]
    fn mod_pow2_neg_divisor_4() {
        let engine = Engine::new();
        assert_eq!(exec_int(&engine, "let x = 0 - 5; return x % 4;"), -1);
    }

    #[test]
    fn mod_pow2_neg_divisor_8() {
        let engine = Engine::new();
        assert_eq!(exec_int(&engine, "let x = 0 - 7; return x % 8;"), -7);
    }

    #[test]
    fn mod_pow2_neg_divisor_16() {
        let engine = Engine::new();
        assert_eq!(exec_int(&engine, "let x = 0 - 17; return x % 16;"), -1);
    }

    #[test]
    fn mod_pow2_neg_divisor_64() {
        let engine = Engine::new();
        assert_eq!(exec_int(&engine, "let x = 0 - 100; return x % 64;"), -36);
    }

    #[test]
    fn mod_pow2_neg_divisor_1024_large() {
        let engine = Engine::new();
        assert_eq!(
            exec_int(&engine, "let x = 0 - 1000000; return x % 1024;"),
            -576
        );
    }

    #[test]
    fn mod_pow2_neg_one_dividend() {
        let engine = Engine::new();
        assert_eq!(exec_int(&engine, "let x = 0 - 1; return x % 2;"), -1);
    }

    #[test]
    fn mod_pow2_i64_min_dividend() {
        let engine = Engine::new();
        let src = "
            let min = 0 - 9223372036854775807 - 1;
            let x = min + 1;
            return x % 2;
        ";
        assert_eq!(exec_int(&engine, src), -1);
    }

    #[test]
    fn mod_pow2_in_loop_with_negatives() {
        let engine = Engine::new();
        let src = "
            let sum = 0;
            let i = 0;
            for i < 16 {
                let x = i - 8;
                sum = sum + (x % 4);
                i = i + 1;
            }
            return sum;
        ";
        assert_eq!(exec_int(&engine, src), 0);
    }

    #[test]
    fn mod_pow2_after_inlining_neg_arg() {
        let engine = Engine::new();
        let src = "
            let rem(a) = { return a % 4; }
            let x = 0 - 5;
            return rem(x);
        ";
        assert_eq!(exec_int(&engine, src), -1);
    }

    #[test]
    fn mod_pow2_positive_still_correct() {
        let engine = Engine::new();
        assert_eq!(exec_int(&engine, "let x = 5; return x % 4;"), 1);
        assert_eq!(exec_int(&engine, "let x = 7; return x % 8;"), 7);
        assert_eq!(exec_int(&engine, "let x = 100; return x % 64;"), 36);
        assert_eq!(exec_int(&engine, "return 0 % 8;"), 0);
    }

    // --- Multiply to ShiftLeft ---

    #[test]
    fn mul_pow2_positive_simple() {
        let engine = Engine::new();
        assert_eq!(exec_int(&engine, "let x = 5; return x * 2;"), 10);
        assert_eq!(exec_int(&engine, "let x = 5; return x * 4;"), 20);
        assert_eq!(exec_int(&engine, "let x = 5; return x * 8;"), 40);
        assert_eq!(exec_int(&engine, "let x = 5; return x * 1024;"), 5120);
    }

    #[test]
    fn mul_pow2_negative_simple() {
        let engine = Engine::new();
        assert_eq!(exec_int(&engine, "let x = 0 - 5; return x * 2;"), -10);
        assert_eq!(exec_int(&engine, "let x = 0 - 5; return x * 4;"), -20);
        assert_eq!(exec_int(&engine, "let x = 0 - 5; return x * 8;"), -40);
        assert_eq!(
            exec_int(&engine, "let x = 0 - 1000000; return x * 8;"),
            -8_000_000
        );
    }

    #[test]
    fn mul_pow2_left_is_one() {
        let engine = Engine::new();
        assert_eq!(exec_int(&engine, "let x = 42; return 1 * x;"), 42);
        assert_eq!(exec_int(&engine, "let x = 0 - 7; return 1 * x;"), -7);
    }

    // --- Algebraic simplifications ---

    #[test]
    fn add_zero() {
        let engine = Engine::new();
        assert_eq!(exec_int(&engine, "let x = 5; return x + 0;"), 5);
        assert_eq!(exec_int(&engine, "let x = 0 - 5; return x + 0;"), -5);
        assert_eq!(exec_int(&engine, "let x = 5; return 0 + x;"), 5);
    }

    #[test]
    fn sub_zero() {
        let engine = Engine::new();
        assert_eq!(exec_int(&engine, "let x = 5; return x - 0;"), 5);
        assert_eq!(exec_int(&engine, "let x = 0 - 5; return x - 0;"), -5);
    }

    #[test]
    fn mul_by_one() {
        let engine = Engine::new();
        assert_eq!(exec_int(&engine, "let x = 5; return x * 1;"), 5);
        assert_eq!(exec_int(&engine, "let x = 0 - 5; return x * 1;"), -5);
    }

    #[test]
    fn mul_by_zero() {
        let engine = Engine::new();
        assert_eq!(exec_int(&engine, "let x = 5; return x * 0;"), 0);
        assert_eq!(exec_int(&engine, "let x = 0 - 5; return x * 0;"), 0);
    }

    #[test]
    fn div_by_one() {
        let engine = Engine::new();
        assert_eq!(exec_int(&engine, "let x = 5; return x / 1;"), 5);
        assert_eq!(exec_int(&engine, "let x = 0 - 5; return x / 1;"), -5);
    }

    // --- Constant folding ---

    #[test]
    fn fold_int_arithmetic() {
        let engine = Engine::new();
        assert_eq!(exec_int(&engine, "return 2 + 3;"), 5);
        assert_eq!(exec_int(&engine, "return 6 * 7;"), 42);
        assert_eq!(exec_int(&engine, "return (0 - 5) + 3;"), -2);
    }

    #[test]
    fn fold_modulo_non_power_of_two() {
        let engine = Engine::new();
        assert_eq!(exec_int(&engine, "return 10 % 3;"), 1);
        assert_eq!(exec_int(&engine, "return (0 - 10) % 3;"), -1);
    }

    #[test]
    fn fold_bitops() {
        let engine = Engine::new();
        assert_eq!(exec_int(&engine, "return 255 & 15;"), 15);
        assert_eq!(exec_int(&engine, "return 240 | 15;"), 255);
        assert_eq!(exec_int(&engine, "return 255 ^ 15;"), 240);
        assert_eq!(exec_int(&engine, "return 1 << 4;"), 16);
        assert_eq!(exec_int(&engine, "return 256 >> 3;"), 32);
    }

    #[test]
    fn fold_comparisons() {
        let engine = Engine::new();
        assert!(exec_bool(&engine, "return 1 == 1;"));
        assert!(!exec_bool(&engine, "return 1 == 2;"));
        assert!(exec_bool(&engine, "return 3 > 2;"));
        assert!(exec_bool(&engine, "return 2 < 3;"));
        assert!(exec_bool(&engine, "return 3 >= 3;"));
        assert!(exec_bool(&engine, "return 2 <= 2;"));
    }

    // --- Control flow & dead branches ---

    #[test]
    fn if_true_picks_then() {
        let engine = Engine::new();
        let src = "if true { return 1; } else { return 2; }";
        assert_eq!(engine.execute(src).unwrap(), Some(Value::Int(1)));
    }

    #[test]
    fn if_false_picks_else() {
        let engine = Engine::new();
        let src = "if false { return 1; } else { return 2; }";
        assert_eq!(engine.execute(src).unwrap(), Some(Value::Int(2)));
    }

    #[test]
    fn if_true_no_else() {
        let engine = Engine::new();
        let src = "
            let x = 0;
            if true { x = 1; }
            return x;
        ";
        assert_eq!(engine.execute(src).unwrap(), Some(Value::Int(1)));
    }

    // --- Dead code & inlining ---

    #[test]
    fn dead_code_after_return() {
        let engine = Engine::new();
        let src = "
            return 42;
            let x = 999;
            return x;
        ";
        assert_eq!(engine.execute(src).unwrap(), Some(Value::Int(42)));
    }

    #[test]
    fn inline_simple() {
        let engine = Engine::new();
        let src = "
            let add(a, b) = { return a + b; }
            return add(3, 4);
        ";
        assert_eq!(engine.execute(src).unwrap(), Some(Value::Int(7)));
    }

    #[test]
    fn inline_with_negative_result() {
        let engine = Engine::new();
        let src = "
            let neg(a) = { return 0 - a; }
            return neg(5);
        ";
        assert_eq!(engine.execute(src).unwrap(), Some(Value::Int(-5)));
    }

    // --- IR optimizer passes ---

    #[test]
    fn ir_copy_propagation_arithmetic() {
        let engine = Engine::new();
        let src = "
            let a = 7;
            let b = a;
            let c = b;
            return c * 6;
        ";
        assert_eq!(engine.execute(src).unwrap(), Some(Value::Int(42)));
    }

    #[test]
    fn ir_dead_store_keeps_visible_writes() {
        let engine = Engine::new();
        let src = "
            let x = 0;
            if true {
                x = 1;
            } else {
                x = 2;
            }
            return x;
        ";
        assert_eq!(engine.execute(src).unwrap(), Some(Value::Int(1)));
    }

    #[test]
    fn ir_unreachable_code_after_jump() {
        let engine = Engine::new();
        let src = "
            let x = 1;
            for true {
                x = x + 1;
                if x == 5 { return x; }
            }
        ";
        assert_eq!(engine.execute(src).unwrap(), Some(Value::Int(5)));
    }

    #[test]
    fn ir_inc_dec_preserves_semantics() {
        let engine = Engine::new();
        let src = "
            let x = 10;
            let y = 0 - 1;
            x = x + 1;
            x = x - 1;
            x = x + y;
            return x;
        ";
        assert_eq!(engine.execute(src).unwrap(), Some(Value::Int(9)));
    }

    // --- Type preservation ---

    #[test]
    fn double_arithmetic_preserved() {
        let engine = Engine::new();
        let src = "return 1.5 + 2.5;";
        match engine.execute(src).unwrap() {
            Some(Value::Double(v)) => {
                assert_eq!(v.to_bits(), 4.0_f64.to_bits());
            }
            other => panic!("expected Double(4.0), got {:?}", other),
        }
    }

    #[test]
    fn string_concat_preserved() {
        let engine = Engine::new();
        let src = "return \"foo\" + \"bar\";";
        match engine.execute(src).unwrap() {
            Some(Value::String(s)) => assert_eq!(&*s, "foobar"),
            other => panic!("expected String(\"foobar\"), got {:?}", other),
        }
    }

    // --- Side-effect preservation ---

    #[test]
    fn side_effect_mul_by_zero_right() {
        let engine = make_engine_with_counter();
        let result = exec_int(&engine, "let x = bump(); return x * 0;");
        assert_eq!(result, 0);
        assert_eq!(exec_int(&engine, "return peek();"), 1);
    }

    #[test]
    fn side_effect_mul_by_zero_left() {
        let engine = make_engine_with_counter();
        let result = exec_int(&engine, "return 0 * bump();");
        assert_eq!(result, 0);
        assert_eq!(exec_int(&engine, "return peek();"), 1);
    }

    #[test]
    fn side_effect_bitand_zero_right() {
        let engine = make_engine_with_counter();
        let result = exec_int(&engine, "let x = bump(); return x & 0;");
        assert_eq!(result, 0);
        assert_eq!(exec_int(&engine, "return peek();"), 1);
    }

    #[test]
    fn side_effect_bitand_zero_left() {
        let engine = make_engine_with_counter();
        let result = exec_int(&engine, "return 0 & bump();");
        assert_eq!(result, 0);
        assert_eq!(exec_int(&engine, "return peek();"), 1);
    }

    #[test]
    fn side_effect_add_zero_right() {
        let engine = make_engine_with_counter();
        let result = exec_int(&engine, "let x = bump(); return x + 0;");
        assert_eq!(result, 1);
        assert_eq!(exec_int(&engine, "return peek();"), 1);
    }

    #[test]
    fn side_effect_add_zero_left() {
        let engine = make_engine_with_counter();
        let result = exec_int(&engine, "return 0 + bump();");
        assert_eq!(result, 1);
        assert_eq!(exec_int(&engine, "return peek();"), 1);
    }

    #[test]
    fn side_effect_sub_zero_right() {
        let engine = make_engine_with_counter();
        let result = exec_int(&engine, "let x = bump(); return x - 0;");
        assert_eq!(result, 1);
        assert_eq!(exec_int(&engine, "return peek();"), 1);
    }

    #[test]
    fn side_effect_mul_by_one_right() {
        let engine = make_engine_with_counter();
        let result = exec_int(&engine, "let x = bump(); return x * 1;");
        assert_eq!(result, 1);
        assert_eq!(exec_int(&engine, "return peek();"), 1);
    }

    #[test]
    fn side_effect_div_by_one_right() {
        let engine = make_engine_with_counter();
        let result = exec_int(&engine, "let x = bump(); return x / 1;");
        assert_eq!(result, 1);
        assert_eq!(exec_int(&engine, "return peek();"), 1);
    }

    #[test]
    fn side_effect_modulo_pow2_on_call() {
        let engine = make_engine_with_counter();
        let result = exec_int(&engine, "let x = bump(); return x % 4;");
        assert_eq!(result, 1);
        assert_eq!(exec_int(&engine, "return peek();"), 1);
    }

    #[test]
    fn side_effect_modulo_pow2_on_neg_call() {
        let engine = make_engine_with_counter();
        let result = exec_int(&engine, "let x = negbump(); return x % 4;");
        assert_eq!(result, -1);
        assert_eq!(exec_int(&engine, "return peek();"), 1);
    }

    #[test]
    fn side_effect_modulo_pow2_inline_neg_call() {
        let engine = make_engine_with_counter();
        let src = "
            let mod4(a) = { return a % 4; }
            return mod4(negbump());
        ";
        let result = exec_int(&engine, src);
        assert_eq!(result, -1);
        assert_eq!(exec_int(&engine, "return peek();"), 1);
    }

    #[test]
    fn side_effect_inlined_function_with_add_zero() {
        let engine = make_engine_with_counter();
        let src = "
            let addzero(a) = { return a + 0; }
            return addzero(bump());
        ";
        let result = exec_int(&engine, src);
        assert_eq!(result, 1);
        assert_eq!(exec_int(&engine, "return peek();"), 1);
    }

    #[test]
    fn side_effect_bump_called_twice_not_elided() {
        let engine = make_engine_with_counter();
        let result = exec_int(&engine, "return bump() + bump();");
        assert_eq!(result, 3);
        assert_eq!(exec_int(&engine, "return peek();"), 2);
    }

    #[test]
    fn side_effect_modulo_within_compound_expr() {
        let engine = make_engine_with_counter();
        let result = exec_int(&engine, "return bump() + (negbump() % 4);");
        let calls = exec_int(&engine, "return peek();");
        assert_eq!(result, -1, "unexpected result (calls={})", calls);
        assert_eq!(calls, 2, "expected 2 calls, got {}", calls);
    }

    #[test]
    fn side_effect_two_native_calls_in_add() {
        let engine = make_engine_with_counter();
        let result = exec_int(&engine, "return bump() + negbump();");
        assert_eq!(result, -1);
        assert_eq!(exec_int(&engine, "return peek();"), 2);
    }

    #[test]
    fn side_effect_bump_plus_bitand_of_negbump() {
        let engine = make_engine_with_counter();
        let result = exec_int(&engine, "return bump() + (negbump() & 3);");
        let calls = exec_int(&engine, "return peek();");
        assert_eq!(result, 3, "unexpected result (calls={})", calls);
        assert_eq!(calls, 2);
    }

    #[test]
    fn ir_constant_folding_binary() {
        use parts::compiler::IrOp;
        use parts::optimize::IrOptimizer;
        use parts::parser::BinaryOperator;

        let ir = vec![
            IrOp::LoadInt { dest: 0, val: 15 },
            IrOp::LoadInt { dest: 1, val: 25 },
            IrOp::Binary {
                dest: 2,
                op: BinaryOperator::Add,
                left: 0,
                right: 1,
            },
            IrOp::Return { value: 2 },
        ];

        let optimized = IrOptimizer::optimize(ir);
        assert!(optimized.contains(&IrOp::LoadInt { dest: 2, val: 40 }));
    }

    #[test]
    fn ir_dead_store_elimination_pure_ops() {
        use parts::compiler::IrOp;
        use parts::optimize::IrOptimizer;
        let ir = vec![
            IrOp::LoadDouble { dest: 0, val: 3.5 },
            IrOp::LoadDouble { dest: 0, val: 2.5 },
            IrOp::Return { value: 0 },
        ];

        let optimized = IrOptimizer::optimize(ir);
        // The first LoadDouble(3.14) into dest: 0 was immediately overwritten and never read
        assert_eq!(optimized.len(), 2);
        assert_eq!(optimized[0], IrOp::LoadDouble { dest: 0, val: 2.5 });
    }
}
