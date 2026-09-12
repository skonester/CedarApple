use crate::parser::BinaryOperator;
use rustc_hash::FxHashMap;

use crate::define_opcodes;

pub struct Emitter {}

define_opcodes! {
    // Direct Constant & Register Loads
    LoadInt       = 0x00,
    LoadDouble    = 0x01,
    LoadBool      = 0x02,
    LoadConst     = 0x03,
    LoadReg       = 0x04,
    LoadFun       = 0x05,
    LoadObject    = 0x06,
    LoadEnum      = 0x07,
    LoadIntSmall  = 0x08,

    // Flow Control
    Return        = 0x10,
    Call          = 0x11,
    Binary        = 0x12,
    Jump          = 0x13,
    JumpNot       = 0x16,
    TailCall      = 0x17,
    Inc           = 0x18,
    Dec           = 0x19,
    // Property & Global Access
    GetProperty    = 0x21,
    SetProperty    = 0x22,
    GetPropertyDyn = 0x23,
    SetPropertyDyn = 0x24,
    LoadNative     = 0x25,
    LoadGlobal     = 0x26,
    MatchEnum      = 0x27
}

impl Emitter {
    fn get_byte_length(op: &IrOp) -> usize {
        match op {
            IrOp::LoadConst { .. } => 4,
            IrOp::LoadInt { val, .. } => {
                if (-128..=127).contains(val) {
                    3
                } else {
                    10
                }
            }
            IrOp::LoadDouble { .. } => 10,
            IrOp::LoadBool { .. } => 3,
            IrOp::LoadReg { .. } => 3,
            IrOp::LoadNative { .. } => 4,
            IrOp::LoadGlobal { .. } => 3,
            IrOp::LoadFun { .. } => 4,
            IrOp::LoadObject { .. } => 4,
            IrOp::GetProperty { .. } => 5,
            IrOp::GetPropertyDyn { .. } => 4,
            IrOp::SetProperty { .. } => 5,
            IrOp::SetPropertyDyn { .. } => 4,
            IrOp::Return { .. } => 2,
            IrOp::Call { args, .. } => 4 + args.len(),
            IrOp::TailCall { args, .. } => 3 + args.len(),
            IrOp::Binary { .. } => 5,
            IrOp::JumpNot { .. } => 4,
            IrOp::Jump { .. } => 3,
            IrOp::Label(_) => 0,
            IrOp::Inc { .. } => 2,
            IrOp::Dec { .. } => 2,
            IrOp::LoadEnumField { args, .. } => 6 + 9 * args.len(),
            IrOp::MatchEnum { .. } => 6,
        }
    }

    pub fn emit(&mut self, ir: Vec<IrOp>) -> Vec<u8> {
        let mut current_byte_offset = 0;
        let mut label_offsets = FxHashMap::default();

        for op in &ir {
            match op {
                IrOp::Label(id) => {
                    label_offsets.insert(*id, current_byte_offset);
                }
                _ => current_byte_offset += Self::get_byte_length(op),
            }
        }

        let mut buff = Vec::with_capacity(current_byte_offset);

        for op in ir {
            match op {
                IrOp::LoadConst { dest, idx } => {
                    buff.push(OpCode::LoadConst as u8);
                    buff.push(dest);
                    buff.extend_from_slice(&(idx as u16).to_le_bytes());
                }
                IrOp::LoadInt { dest, val } => {
                    if (-128..=127).contains(&val) {
                        buff.push(OpCode::LoadIntSmall as u8);
                        buff.push(dest);
                        buff.push(val as i8 as u8);
                    } else {
                        buff.push(OpCode::LoadInt as u8);
                        buff.push(dest);
                        buff.extend(val.to_le_bytes());
                    }
                }
                IrOp::LoadDouble { dest, val } => {
                    buff.push(OpCode::LoadDouble as u8);
                    buff.push(dest);
                    buff.extend(val.to_le_bytes());
                }
                IrOp::LoadBool { dest, val } => {
                    buff.push(OpCode::LoadBool as u8);
                    buff.push(dest);
                    buff.push(val as u8);
                }
                IrOp::LoadReg { dest, src } => {
                    buff.push(OpCode::LoadReg as u8);
                    buff.push(dest);
                    buff.push(src);
                }
                IrOp::LoadNative { dest, src } => {
                    buff.push(OpCode::LoadNative as u8);
                    buff.push(dest);
                    buff.extend_from_slice(&(src as u16).to_le_bytes());
                }
                IrOp::LoadGlobal { dest, src } => {
                    buff.push(OpCode::LoadGlobal as u8);
                    buff.push(dest);
                    buff.push(src);
                }
                IrOp::LoadFun { dest, src } => {
                    buff.push(OpCode::LoadFun as u8);
                    buff.push(dest);
                    buff.extend_from_slice(&(src as u16).to_le_bytes());
                }
                IrOp::LoadObject { dest, src } => {
                    buff.push(OpCode::LoadObject as u8);
                    buff.push(dest);
                    buff.extend_from_slice(&(src as u16).to_le_bytes());
                }
                IrOp::SetProperty { key, val, obj } => {
                    buff.push(OpCode::SetProperty as u8);
                    buff.push(obj);
                    buff.extend_from_slice(&(key as u16).to_le_bytes());
                    buff.push(val);
                }
                IrOp::SetPropertyDyn { key, val, obj } => {
                    buff.push(OpCode::SetPropertyDyn as u8);
                    buff.push(obj);
                    buff.push(key);
                    buff.push(val);
                }
                IrOp::GetProperty { dest, key, obj } => {
                    buff.push(OpCode::GetProperty as u8);
                    buff.push(dest);
                    buff.push(obj);
                    buff.extend_from_slice(&(key as u16).to_le_bytes());
                }
                IrOp::GetPropertyDyn { dest, key, obj } => {
                    buff.push(OpCode::GetPropertyDyn as u8);
                    buff.push(dest);
                    buff.push(obj);
                    buff.push(key);
                }
                IrOp::Return { value } => {
                    buff.push(OpCode::Return as u8);
                    buff.push(value)
                }
                IrOp::Call { dest, what, args } => {
                    buff.push(OpCode::Call as u8);
                    buff.push(dest);
                    buff.push(what);
                    buff.push(args.len() as u8);

                    buff.extend_from_slice(&args);
                }
                IrOp::TailCall { what, args } => {
                    buff.push(OpCode::TailCall as u8);
                    buff.push(what);
                    buff.push(args.len() as u8);

                    buff.extend_from_slice(&args);
                }
                IrOp::Binary {
                    dest,
                    op,
                    left,
                    right,
                } => {
                    buff.push(OpCode::Binary as u8);

                    let op_code = match op {
                        crate::parser::BinaryOperator::Add => 0,
                        crate::parser::BinaryOperator::Minus => 1,
                        crate::parser::BinaryOperator::Multiply => 2,
                        crate::parser::BinaryOperator::Divide => 3,
                        crate::parser::BinaryOperator::Equals => 4,
                        crate::parser::BinaryOperator::GreaterThan => 5,
                        crate::parser::BinaryOperator::LessThan => 6,
                        crate::parser::BinaryOperator::GreaterThanOrEqual => 7,
                        crate::parser::BinaryOperator::LessThanOrEqual => 8,
                        crate::parser::BinaryOperator::Modulo => 9,
                        crate::parser::BinaryOperator::BitAnd => 10,
                        crate::parser::BinaryOperator::BitOr => 11,
                        crate::parser::BinaryOperator::BitXor => 12,
                        crate::parser::BinaryOperator::BitSHL => 13,
                        crate::parser::BinaryOperator::BitSHR => 14,
                    };

                    buff.push(op_code);
                    buff.push(dest);
                    buff.push(left);
                    buff.push(right);
                }
                IrOp::Label(_) => (),
                IrOp::JumpNot { condition, target } => {
                    let target_offset = label_offsets[&target];

                    buff.push(OpCode::JumpNot as u8);
                    buff.push(condition);
                    buff.extend_from_slice(&(target_offset as u16).to_le_bytes());
                }
                IrOp::Jump { target } => {
                    let target_offset = label_offsets[&target];

                    buff.push(OpCode::Jump as u8);
                    buff.extend_from_slice(&(target_offset as u16).to_le_bytes());
                }
                IrOp::Inc { target } => {
                    buff.push(OpCode::Inc as u8);
                    buff.push(target);
                }

                IrOp::Dec { target } => {
                    buff.push(OpCode::Dec as u8);
                    buff.push(target);
                }
                IrOp::LoadEnumField {
                    enum_idx,
                    tag,
                    args,
                    dest,
                } => {
                    buff.push(OpCode::LoadEnum as u8);
                    buff.push(dest);
                    buff.extend_from_slice(&(enum_idx as u16).to_le_bytes());
                    buff.push(tag);
                    buff.push(args.len().try_into().unwrap());

                    for arg in args {
                        buff.extend_from_slice(&arg.0.to_le_bytes());
                        buff.push(arg.1);
                    }
                }
                IrOp::MatchEnum {
                    dest,
                    src,
                    enum_idx,
                    tag,
                } => {
                    buff.push(OpCode::MatchEnum as u8);
                    buff.push(dest);
                    buff.push(src);
                    buff.extend_from_slice(&(enum_idx as u16).to_le_bytes());
                    buff.push(tag);
                }
            }
        }

        buff
    }
}

#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub enum IrOp {
    LoadConst {
        dest: u8,
        idx: usize,
    },
    LoadReg {
        dest: u8,
        src: u8,
    },
    LoadNative {
        dest: u8,
        src: usize,
    },
    LoadGlobal {
        dest: u8,
        src: u8,
    },
    LoadFun {
        dest: u8,
        src: usize,
    },
    LoadInt {
        dest: u8,
        val: i64,
    },
    LoadDouble {
        dest: u8,
        val: f64,
    },
    LoadBool {
        dest: u8,
        val: bool,
    },
    LoadObject {
        dest: u8,
        src: usize,
    },
    GetProperty {
        dest: u8,
        obj: u8,
        key: usize,
    },
    GetPropertyDyn {
        dest: u8,
        obj: u8,
        key: u8,
    },
    SetProperty {
        obj: u8,
        key: usize,
        val: u8,
    },
    SetPropertyDyn {
        obj: u8,
        key: u8,
        val: u8,
    },
    Return {
        value: u8,
    },
    Call {
        dest: u8,
        what: u8,
        args: Vec<u8>,
    },
    TailCall {
        what: u8,
        args: Vec<u8>,
    },
    Binary {
        dest: u8,
        op: BinaryOperator,
        left: u8,
        right: u8,
    },
    JumpNot {
        target: usize,
        condition: u8,
    },
    Jump {
        target: usize,
    },
    Inc {
        target: u8,
    },
    Dec {
        target: u8,
    },
    LoadEnumField {
        dest: u8,
        enum_idx: usize,
        tag: u8,
        args: Vec<(u64, u8)>,
    },
    MatchEnum {
        dest: u8,
        src: u8,
        enum_idx: usize,
        tag: u8,
    },
    Label(usize),
}

impl IrOp {
    pub fn for_each_reg(&self, mut f: impl FnMut(u8)) {
        match self {
            IrOp::LoadConst { dest, .. }
            | IrOp::LoadNative { dest, .. }
            | IrOp::LoadFun { dest, .. }
            | IrOp::LoadInt { dest, .. }
            | IrOp::LoadDouble { dest, .. }
            | IrOp::LoadBool { dest, .. }
            | IrOp::LoadObject { dest, .. } => f(*dest),

            IrOp::LoadReg { dest, src }
            | IrOp::LoadGlobal { dest, src }
            | IrOp::MatchEnum { dest, src, .. } => {
                f(*dest);
                f(*src);
            }

            IrOp::GetProperty { dest, obj, .. } => {
                f(*dest);
                f(*obj);
            }
            IrOp::GetPropertyDyn { dest, obj, key } => {
                f(*dest);
                f(*obj);
                f(*key);
            }
            IrOp::SetProperty { obj, val, .. } => {
                f(*obj);
                f(*val);
            }
            IrOp::SetPropertyDyn { obj, key, val } => {
                f(*obj);
                f(*key);
                f(*val);
            }
            IrOp::Return { value } => f(*value),
            IrOp::Call { dest, what, args } => {
                f(*dest);
                f(*what);
                for a in args {
                    f(*a);
                }
            }
            IrOp::TailCall { what, args } => {
                f(*what);
                for a in args {
                    f(*a);
                }
            }
            IrOp::Binary {
                dest, left, right, ..
            } => {
                f(*dest);
                f(*left);
                f(*right);
            }
            IrOp::JumpNot { condition, .. } => f(*condition),
            IrOp::Inc { target } | IrOp::Dec { target } => f(*target),
            IrOp::LoadEnumField { dest, args, .. } => {
                f(*dest);
                for (_, a) in args {
                    f(*a);
                }
            }
            IrOp::Jump { .. } | IrOp::Label(_) => {}
        }
    }
    pub fn dest(&self) -> Option<u8> {
        match self {
            IrOp::LoadInt { dest, .. }
            | IrOp::LoadGlobal { dest, .. }
            | IrOp::LoadDouble { dest, .. }
            | IrOp::LoadBool { dest, .. }
            | IrOp::LoadConst { dest, .. }
            | IrOp::LoadObject { dest, .. }
            | IrOp::LoadNative { dest, .. }
            | IrOp::LoadFun { dest, .. }
            | IrOp::LoadReg { dest, .. }
            | IrOp::Binary { dest, .. }
            | IrOp::GetProperty { dest, .. }
            | IrOp::GetPropertyDyn { dest, .. }
            | IrOp::Call { dest, .. }
            | IrOp::LoadEnumField { dest, .. }
            | IrOp::MatchEnum { dest, .. } => Some(*dest),
            _ => None,
        }
    }

    pub fn for_each_read(&self, mut f: impl FnMut(u8)) {
        match self {
            IrOp::Binary { left, right, .. } => {
                f(*left);
                f(*right);
            }
            IrOp::GetProperty { obj, .. } => f(*obj),
            IrOp::GetPropertyDyn { obj, key, .. } => {
                f(*obj);
                f(*key);
            }
            IrOp::SetProperty { obj, val, .. } => {
                f(*obj);
                f(*val);
            }
            IrOp::SetPropertyDyn { obj, key, val } => {
                f(*obj);
                f(*key);
                f(*val);
            }
            IrOp::LoadReg { src, .. }
            | IrOp::LoadGlobal { src, .. }
            | IrOp::MatchEnum { src, .. } => {
                f(*src);
            }
            IrOp::Call { what, args, .. } => {
                f(*what);
                for a in args {
                    f(*a);
                }
            }
            IrOp::TailCall { what, args } => {
                f(*what);
                for a in args {
                    f(*a);
                }
            }
            IrOp::LoadEnumField { args, .. } => {
                for (_, a) in args {
                    f(*a);
                }
            }
            IrOp::Return { value } => f(*value),
            IrOp::JumpNot { condition, .. } => f(*condition),
            IrOp::Inc { target } | IrOp::Dec { target } => f(*target),
            _ => {}
        }
    }

    pub fn reads_reg(&self, reg: u8) -> bool {
        let mut found = false;
        self.for_each_read(|r| {
            if r == reg {
                found = true;
            }
        });
        found
    }

    pub fn has_side_effects(&self) -> bool {
        matches!(
            self,
            IrOp::Call { .. }
                | IrOp::TailCall { .. }
                | IrOp::SetProperty { .. }
                | IrOp::SetPropertyDyn { .. }
                | IrOp::Return { .. }
                | IrOp::Jump { .. }
                | IrOp::JumpNot { .. }
                | IrOp::Label(_)
        )
    }

    pub fn is_terminator(&self) -> bool {
        matches!(
            self,
            IrOp::Return { .. } | IrOp::TailCall { .. } | IrOp::Jump { .. }
        )
    }

    pub fn compute_frame_size(arity: u8, ir: &[IrOp]) -> u8 {
        let mut max_reg = arity.saturating_sub(1);
        for op in ir {
            op.for_each_reg(|r| {
                if r > max_reg {
                    max_reg = r;
                }
            });
        }
        max_reg.saturating_add(1).max(arity).max(1)
    }
}
