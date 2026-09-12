#[cfg(test)]
mod tests {
    use parts::engine::Engine;
    use parts::value::Value;

    #[test]
    fn test_result_ok_and_err_construction() {
        let engine = Engine::new();
        let source = "
            let ok_val = Result::Ok(100);
            let err_val = Result::Err(\"something went wrong\");
            return ok_val.val;
        ";
        let result = engine.execute(source).unwrap();
        assert_eq!(result, Some(Value::Int(100)));
    }

    #[test]
    fn test_result_guards_is_ok_is_err() {
        let engine = Engine::new();
        let source = "
            let std = import `@std/std.pts`;
            let res_ok = Result::Ok(42);
            let res_err = Result::Err(\"failure\");
            
            let check_a = res_ok.is_ok();
            let check_b = res_ok.is_err();
            let check_c = res_err.is_ok();
            let check_d = res_err.is_err();

            if check_a & (check_b == false) & (check_c == false) & check_d {
                return 1;
            } else {
                return 0;
            }
        ";
        let result = engine.execute(source).unwrap();
        assert_eq!(result, Some(Value::Int(1)));
    }

    #[test]
    fn test_result_unwrap_and_unwrap_or() {
        let engine = Engine::new();
        let source = "
            let std = import `@std/std.pts`;
            let res_ok = Result::Ok(500);
            let res_err = Result::Err(\"failed\");

            let val_ok = res_ok.unwrap();
            let val_fallback = res_err.unwrap_or(999);

            return val_ok + val_fallback;
        ";
        let result = engine.execute(source).unwrap();
        // 500 + 999 = 1499
        assert_eq!(result, Some(Value::Int(1499)));
    }

    #[test]
    fn test_result_expect_and_unwrap_err() {
        let engine = Engine::new();
        let source = "
            let std = import `@std/std.pts`;
            let res_ok = Result::Ok(77);
            let res_err = Result::Err(\"bad input\");

            let val = res_ok.expect(\"should be present\");
            let err_msg = res_err.unwrap_err();

            if (val == 77) & (err_msg == \"bad input\") {
                return 1;
            } else {
                return 0;
            }
        ";
        let result = engine.execute(source).unwrap();
        assert_eq!(result, Some(Value::Int(1)));
    }

    #[test]
    fn test_result_match_with_result_prefix() {
        let engine = Engine::new();
        let source = "
            let divide(a, b) {
                if b == 0 {
                    return Result::Err(\"division by zero\");
                } else {
                    return Result::Ok(a / b);
                }
            }

            let r_ok = divide(100, 4);
            let r_err = divide(100, 0);

            let out_a = match r_ok {
                Result::Ok(val) => val,
                Result::Err(e) => 0 - 1
            };

            let out_b = match r_err {
                Result::Ok(val) => val,
                Result::Err(e) => 999
            };

            return out_a + out_b;
        ";
        let result = engine.execute(source).unwrap();
        // 25 + 999 = 1024
        assert_eq!(result, Some(Value::Int(1024)));
    }

    #[test]
    fn test_result_match_shorthand_ok_err() {
        let engine = Engine::new();
        let source = "
            let get_value(flag) {
                if flag {
                    return Result::Ok(123);
                } else {
                    return Result::Err(\"disabled\");
                }
            }

            let res = get_value(true);
            return match res {
                Ok(v) => v * 2,
                Err(e) => 0
            };
        ";
        let result = engine.execute(source).unwrap();
        assert_eq!(result, Some(Value::Int(246)));
    }

    #[test]
    fn test_std_env_graceful_handling() {
        let engine = Engine::new();
        let source = "
            let std = import `@std/std.pts`;
            \"Lookup non-existent environment variable\"
            let res = std.sys.env(\"PARTS_NON_EXISTENT_VAR_12345\");
            
            let is_failure = res.is_err();
            let default_val = res.unwrap_or(\"fallback_value\");

            if is_failure & (default_val == \"fallback_value\") {
                return 1;
            } else {
                return 0;
            }
        ";
        let result = engine.execute(source).unwrap();
        assert_eq!(result, Some(Value::Int(1)));
    }

    #[test]
    fn test_std_exec_graceful_handling() {
        let engine = Engine::new();
        let source = "
            let std = import `@std/std.pts`;
            \"Execute non-existent command\"
            let res = std.sys.exec(\"non_existent_command_xyz_123\");
            
            let failed = res.is_err();
            if failed {
                return 1;
            } else {
                return 0;
            }
        ";
        let result = engine.execute(source).unwrap();
        assert_eq!(result, Some(Value::Int(1)));
    }

    #[test]
    fn test_std_str_byte_at_graceful_handling() {
        let engine = Engine::new();
        let source = "
            let std = import `@std/std.pts`;
            let text = \"hello\";
            
            let valid = std.str.byte_at(text, 0);
            let out_of_bounds = std.str.byte_at(text, 100);

            let v_byte = valid.unwrap();
            let is_oob_err = out_of_bounds.is_err();
            let fallback_byte = out_of_bounds.unwrap_or(0);

            if (v_byte == 104) & is_oob_err & (fallback_byte == 0) {
                return 1;
            } else {
                return 0;
            }
        ";
        let result = engine.execute(source).unwrap();
        assert_eq!(result, Some(Value::Int(1)));
    }

    #[test]
    fn test_std_result_module_import() {
        let engine = Engine::new();
        let source = "
            let std = import `@std/std.pts`;

            let ok_val = std.Ok(888);
            let err_val = std.Err(\"custom error\");

            let ok_check = ok_val.is_ok();
            let unwrapped = ok_val.unwrap();
            let fallback = err_val.unwrap_or(111);

            return (unwrapped + fallback);
        ";
        let result = engine.execute(source).unwrap();
        // 888 + 111 = 999
        assert_eq!(result, Some(Value::Int(999)));
    }
}
