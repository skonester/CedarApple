#[cfg(test)]
mod tests {
    use parts::engine::Engine;
    use parts::value::Value;

    #[test]
    fn test_execute() {
        let engine = Engine::new();
        let result = engine.execute("return 1 + 2;").unwrap();
        assert_eq!(result, Some(Value::Int(3)));
    }

    #[test]
    fn test_iterative() {
        let engine = Engine::new();
        let source = "
            let a = 0;
            let b = 1;
            let i = 0;
            for i < 10 {
                let temp = a + b;
                a = b;
                b = temp;
                i = i + 1;
            }
            return a;
        ";
        let result = engine.execute(source).unwrap();
        assert_eq!(result, Some(Value::Int(55)));
    }

    #[test]
    fn test_call_function() {
        let engine = Engine::new();
        let source = "
            let add(a, b) = {
                return a + b;
            }
            return add;
        ";
        let res = engine.run(source).unwrap();
        let func = res.value.unwrap();

        let call_res = func
            .call(vec![Value::Int(10), Value::Int(20)], res.constants)
            .unwrap();

        assert_eq!(call_res, Some(Value::Int(30)));
    }

    #[test]
    fn test_custom_native() {
        let mut engine = Engine::new();
        engine.register_native("rust_add", 2, |args| match (&args[0], &args[1]) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
            _ => Err("Expected two integers".to_string()),
        });

        let source = "return rust_add(5, 7);";
        let result = engine.execute(source).unwrap();
        assert_eq!(result, Some(Value::Int(12)));
    }
}
