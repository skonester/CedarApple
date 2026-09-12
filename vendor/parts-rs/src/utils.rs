#[macro_export]
macro_rules! define_opcodes {
    ($($name:ident = $val:expr),* $(,)?) => {
        #[repr(u8)]
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum OpCode {
            $($name = $val),*
        }

        impl TryFrom<u8> for OpCode {
            type Error = u8;

            fn try_from(v: u8) -> Result<Self, Self::Error> {
                match v {
                    $($val => Ok(OpCode::$name),)*
                    _ => Err(v),
                }
            }
        }
    };
}

#[macro_export]
macro_rules! impl_binary_op {
    ($trait:ident, $method:ident, $op:tt) => {
        impl std::ops::$trait for Value {
            type Output = Result<Value, String>;

            fn $method(self, rhs: Self) -> Self::Output {
                match (self, rhs) {
                    (Value::Ref(_), _) | (_, Value::Ref(_)) =>
                        Err("got reference, expected value".to_string()),

                    (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a $op b)),
                    (Value::Double(a), Value::Double(b)) => Ok(Value::Double(a $op b)),

                    (Value::Int(a), Value::Double(b)) => Ok(Value::Double(a as f64 $op b)),
                    (Value::Double(a), Value::Int(b)) => Ok(Value::Double(a $op b as f64)),

                    (Value::Int(a), Value::Bool(b)) => Ok(Value::Int(a $op (if b { 1 } else { 0 }))),
                    (Value::Bool(a), Value::Int(b)) => Ok(Value::Int((if a { 1 } else { 0 }) $op b)),

                    (l, r) => Err(format!("Operation not supported: {:?} {} {:?}", l, stringify!($op), r)),
                }
            }
        }
    };
}

#[macro_export]
macro_rules! impl_compare_op {
    ($name:ident, $op:tt) => {
        impl Value {
            pub fn $name(&self, other: &Value) -> Result<Value, String> {
                if std::mem::discriminant(self) != std::mem::discriminant(other) {
                    return Ok(Value::Bool(false));
                }

                match (self, other) {
                    (Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a $op b)),
                    (Value::Double(a), Value::Double(b)) => Ok(Value::Bool(a $op b)),
                    (Value::String(a), Value::String(b)) => Ok(Value::Bool(a $op b)),
                    (Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(a $op b)),
                    _ => Err("Comparison not implemented for this type".to_string()),
                }
            }
        }
    };
}

#[macro_export]
macro_rules! impl_bitwise_op {
    ($trait:ident, $method:ident, $op:tt) => {
        impl std::ops::$trait for Value {
            type Output = Result<Value, String>;

            fn $method(self, rhs: Self) -> Self::Output {
                match (self, rhs) {
                    (Value::Ref(_), _) | (_, Value::Ref(_)) =>
                        Err("got reference, expected value".to_string()),

                    // Czyste liczby całkowite
                    (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a $op b)),

                    // Rzutowanie Double na i64 przed operacją bitową (kluczowe dla wyników z np. math.pow)
                    (Value::Double(a), Value::Double(b)) => Ok(Value::Int((a as i64) $op (b as i64))),
                    (Value::Int(a), Value::Double(b)) => Ok(Value::Int(a $op (b as i64))),
                    (Value::Double(a), Value::Int(b)) => Ok(Value::Int((a as i64) $op b)),

                    // Opcjonalna obsługa Bool
                    (Value::Int(a), Value::Bool(b)) => Ok(Value::Int(a $op (if b { 1 } else { 0 }))),
                    (Value::Bool(a), Value::Int(b)) => Ok(Value::Int((if a { 1 } else { 0 }) $op b)),
                    (Value::Bool(a), Value::Bool(b)) => Ok(Value::Int((if a { 1 } else { 0 }) $op (if b { 1 } else { 0 }))),

                    // Jeśli wpadnie np. Value::Hash(X), wyrzucimy ładny błąd zamiast panikowania maszyny wirtualnej!
                    (l, r) => Err(format!("Bitwise operation {} not supported for types: {:?} and {:?}", stringify!($op), l, r)),
                }
            }
        }
    };
}
