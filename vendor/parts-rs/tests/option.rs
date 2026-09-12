#[cfg(test)]
mod tests {
    use parts::engine::Engine;
    use parts::value::Value;

    #[test]
    fn test_option_some_and_none_construction() {
        let engine = Engine::new();
        let source = "
            let some_val = Option::Some(100);
            let none_val = Option::None;
            return some_val.val;
        ";
        let result = engine.execute(source).unwrap();
        assert_eq!(result, Some(Value::Int(100)));
    }

    #[test]
    fn test_option_guards_is_some_is_none() {
        let engine = Engine::new();
        let source = "
            let std = import `@std/std.pts`;
            let opt_some = Option::Some(42);
            let opt_none = Option::None;
            
            let check_a = opt_some.is_some();
            let check_b = opt_some.is_none();
            let check_c = opt_none.is_some();
            let check_d = opt_none.is_none();

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
    fn test_option_unwrap_and_unwrap_or() {
        let engine = Engine::new();
        let source = "
            let std = import `@std/std.pts`;
            let opt_some = Option::Some(500);
            let opt_none = Option::None;

            let val_some = opt_some.unwrap();
            let val_fallback = opt_none.unwrap_or(999);

            return val_some + val_fallback;
        ";
        let result = engine.execute(source).unwrap();
        // 500 + 999 = 1499
        assert_eq!(result, Some(Value::Int(1499)));
    }

    #[test]
    fn test_option_expect() {
        let engine = Engine::new();
        let source = "
            let std = import `@std/std.pts`;
            let opt_some = Option::Some(77);

            let val = opt_some.expect(\"value should be present\");
            return val;
        ";
        let result = engine.execute(source).unwrap();
        assert_eq!(result, Some(Value::Int(77)));
    }

    #[test]
    fn test_option_ok_or_conversion_to_result() {
        let engine = Engine::new();
        let source = "
            let std = import `@std/std.pts`;
            let opt_some = Option::Some(100);
            let opt_none = Option::None;

            let res_ok = opt_some.ok_or(\"was none\");
            let res_err = opt_none.ok_or(\"was none\");

            let ok_check = res_ok.is_ok();
            let err_check = res_err.is_err();
            let val = res_ok.unwrap();
            let err_msg = res_err.unwrap_err();

            if ok_check & err_check & (val == 100) & (err_msg == \"was none\") {
                return 1;
            } else {
                return 0;
            }
        ";
        let result = engine.execute(source).unwrap();
        assert_eq!(result, Some(Value::Int(1)));
    }

    #[test]
    fn test_result_to_option_conversion() {
        let engine = Engine::new();
        let source = "
            let std = import `@std/std.pts`;
            let r_ok = Result::Ok(55);
            let r_err = Result::Err(\"failure\");

            let opt_from_ok = r_ok.ok();
            let opt_none_from_ok = r_ok.err();

            let opt_none_from_err = r_err.ok();
            let opt_from_err = r_err.err();

            let check_a = opt_from_ok.is_some() & (opt_from_ok.unwrap() == 55);
            let check_b = opt_none_from_ok.is_none();
            let check_c = opt_none_from_err.is_none();
            let check_d = opt_from_err.is_some() & (opt_from_err.unwrap() == \"failure\");

            if check_a & check_b & check_c & check_d {
                return 1;
            } else {
                return 0;
            }
        ";
        let result = engine.execute(source).unwrap();
        assert_eq!(result, Some(Value::Int(1)));
    }

    #[test]
    fn test_option_match_explicit_prefix() {
        let engine = Engine::new();
        let source = "
            let find_item(id) {
                if id == 1 {
                    return Option::Some(100);
                } else {
                    return Option::None;
                }
            }

            let r_some = find_item(1);
            let r_none = find_item(2);

            let out_a = match r_some {
                Option::Some(val) => val,
                Option::None => 0
            };

            let out_b = match r_none {
                Option::Some(val) => val,
                Option::None => 999
            };

            return out_a + out_b;
        ";
        let result = engine.execute(source).unwrap();
        // 100 + 999 = 1099
        assert_eq!(result, Some(Value::Int(1099)));
    }

    #[test]
    fn test_option_match_shorthand_some_none() {
        let engine = Engine::new();
        let source = "
            let get_opt(flag) {
                if flag {
                    return Option::Some(250);
                } else {
                    return Option::None;
                }
            }

            let s_val = get_opt(true);
            let n_val = get_opt(false);

            let out_a = match s_val {
                Some(v) => v * 2,
                None => 0
            };

            let out_b = match n_val {
                Some(v) => v * 2,
                None => 50
            };

            return out_a + out_b;
        ";
        let result = engine.execute(source).unwrap();
        // 500 + 50 = 550
        assert_eq!(result, Some(Value::Int(550)));
    }

    #[test]
    fn test_std_str_strip_prefix_and_suffix_option() {
        let engine = Engine::new();
        let source = "
            let std = import `@std/std.pts`;
            let text = \"0x1234abcd\";
            
            let opt_prefix = text.strip_prefix(\"0x\");
            let opt_no_match = text.strip_prefix(\"zzz\");

            let stripped = opt_prefix.unwrap_or(text);
            let is_none_match = opt_no_match.is_none();

            let opt_suffix = text.strip_suffix(\"abcd\");
            let stripped_suffix = opt_suffix.unwrap_or(text);

            if (stripped == \"1234abcd\") & is_none_match & (stripped_suffix == \"0x1234\") {
                return 1;
            } else {
                return 0;
            }
        ";
        let result = engine.execute(source).unwrap();
        assert_eq!(result, Some(Value::Int(1)));
    }

    #[test]
    fn test_both_result_and_option_combined_match() {
        let engine = Engine::new();
        let source = "
            let std = import `@std/std.pts`;

            let parse_int_hex(str_val) {
                let stripped_opt = str_val.strip_prefix(\"0x\");
                let clean_str = match stripped_opt {
                    Some(s) => s,
                    None => str_val
                };

                if clean_str == \"ff\" {
                    return Result::Ok(255);
                } else {
                    return Result::Err(\"unsupported\");
                }
            }

            let res_a = parse_int_hex(\"0xff\");
            let res_b = parse_int_hex(\"invalid\");

            let out_a = match res_a {
                Ok(v) => v,
                Err(e) => 0
            };

            let out_b = match res_b {
                Ok(v) => v,
                Err(e) => 1000
            };

            return out_a + out_b;
        ";
        let result = engine.execute(source).unwrap();
        // 255 + 1000 = 1255
        assert_eq!(result, Some(Value::Int(1255)));
    }

    #[test]
    fn test_option_display() {
        let some_val = Value::some(Value::Int(42));
        assert_eq!(format!("{}", some_val), "Some(42)");

        let none_val = Value::none();
        assert_eq!(format!("{}", none_val), "None");
    }
}
