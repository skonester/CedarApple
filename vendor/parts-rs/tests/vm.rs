#[cfg(test)]
mod tests {
    use parts::emitter::OpCode;
    use parts::value::{Function, Value};
    use parts::vm::VM;
    use std::rc::Rc;

    #[test]
    fn check_double_return() {
        let code = [OpCode::Return as u8, 0, OpCode::Return as u8, 0];

        let mut vm = VM::new(code.to_vec(), vec![]);

        let res = vm.run();

        assert!(res.is_ok_and(|val| val == Some(Value::Int(0))))
    }

    #[test]
    fn check_call_no_args() {
        let code = vec![
            OpCode::LoadFun as u8,
            0,
            0,
            0,
            OpCode::Call as u8,
            1,
            0,
            0,
            OpCode::Return as u8,
            1,
        ];

        let constants = vec![Value::Fun(Rc::new(Function {
            arity: 0,
            frame_size: 1,
            code: vec![OpCode::LoadBool as u8, 0, 1, OpCode::Return as u8, 0].into(),
        }))];

        let mut vm = VM::new(code, constants);

        let res = vm.run();

        assert!(res.is_ok_and(|val| val.is_some_and(|deep| deep == Value::Bool(true))))
    }

    #[test]
    fn check_call_one_arg() {
        let code = vec![
            OpCode::LoadFun as u8,
            0,
            0,
            0,
            OpCode::LoadBool as u8,
            1,
            0,
            OpCode::Call as u8,
            2,
            0,
            1,
            1,
            OpCode::Return as u8,
            2,
        ];

        let constants = vec![Value::Fun(Rc::new(Function {
            arity: 0,
            frame_size: 1,
            code: vec![OpCode::Return as u8, 0].into(),
        }))];

        let mut vm = VM::new(code, constants);

        let res = vm.run();

        assert!(res.is_ok_and(|val| val.is_some_and(|deep| deep == Value::Bool(false))))
    }

    #[test]
    fn check_test_binary() {
        let code = vec![
            OpCode::LoadIntSmall as u8,
            0,
            1,
            OpCode::LoadIntSmall as u8,
            1,
            1,
            OpCode::Binary as u8,
            0,
            2,
            0,
            1,
            OpCode::Return as u8,
            2,
        ];

        let constants = vec![];

        let mut vm = VM::new(code, constants);

        assert!(
            vm.run()
                .is_ok_and(|val| val.is_some_and(|deep| deep == Value::Int(2)))
        )
    }
    #[test]
    fn check_enum_in_function_frame_and_return() {
        use std::rc::Rc;
        let power_hash = Value::String(Rc::new("power".to_string())).get_hash();

        // Function body (runs at fp = 256):
        // Load reg 0 with Double 42.0
        // Load reg 1 with LoadEnum(enum 0, tag 0, 1 field: power_hash from reg 0)
        // Return reg 1
        let mut func_body = vec![OpCode::LoadDouble as u8, 0];
        func_body.extend(42.0f64.to_le_bytes());
        func_body.extend(vec![
            OpCode::LoadEnum as u8,
            1,
            0, // enum_idx lo
            0, // enum_idx hi
            0, // tag
            1, // count
        ]);
        func_body.extend(power_hash.to_le_bytes());
        func_body.push(0); // reg 0 in callee frame
        func_body.extend(vec![OpCode::Return as u8, 1]);

        // Main code (runs at fp = 0):
        // Load reg 0 with func (constant 0)
        // Call reg 0 with 0 args, return into reg 1
        // GetProperty reg 2 from obj reg 1, key const 1 (hash)
        // Return reg 2
        let main_code = vec![
            OpCode::LoadFun as u8,
            0,
            0,
            0,
            OpCode::Call as u8,
            1, // return into reg 1
            0, // func is at reg 0
            0, // 0 args
            OpCode::GetProperty as u8,
            2, // dest reg 2
            1, // obj reg 1
            1, // const idx lo
            0, // const idx hi
            OpCode::Return as u8,
            2,
        ];

        let constants = vec![
            Value::Fun(Rc::new(Function {
                arity: 0,
                frame_size: 2,
                code: func_body.into(),
            })),
            Value::Hash(power_hash),
        ];

        let mut vm = VM::new(main_code, constants);
        let res = vm.run();
        assert_eq!(res, Ok(Some(Value::Double(42.0))));
    }
}
