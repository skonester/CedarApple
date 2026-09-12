use crate::emitter::OpCode;
use crate::value::Value;

pub fn disassemble(code: &[u8], constants: &[Value]) {
    println!("--- Disassembly ---");
    let mut offset = 0;
    while offset < code.len() {
        offset = disassemble_instruction(code, offset, constants);
    }
}

fn disassemble_instruction(code: &[u8], offset: usize, constants: &[Value]) -> usize {
    print!("{:04} ", offset);
    let byte = code[offset];

    let opcode = match OpCode::try_from(byte) {
        Ok(op) => op,
        Err(_) => {
            println!("Unknown OpCode: {:#04X}", byte);
            return offset + 1;
        }
    };

    match opcode {
        OpCode::LoadInt => {
            let reg = code[offset + 1];
            let raw_bytes: [u8; 8] = code[offset + 2..offset + 10].try_into().unwrap();
            let val = i64::from_le_bytes(raw_bytes);
            println!("{:-12} Reg: {:<3} Val: {}", "LOAD_INT", reg, val);
            offset + 10
        }
        OpCode::LoadIntSmall => {
            let reg = code[offset + 1];
            let val = code[offset + 2] as i8;
            println!("{:-12} Reg: {:<3} Val: {}", "LOAD_INT_S", reg, val);
            offset + 3
        }
        OpCode::LoadDouble => {
            let reg = code[offset + 1];
            let raw_bytes: [u8; 8] = code[offset + 2..offset + 10].try_into().unwrap();
            let val = f64::from_le_bytes(raw_bytes);
            println!("{:-12} Reg: {:<3} Val: {}", "LOAD_DBL", reg, val);
            offset + 10
        }
        OpCode::LoadBool => {
            let reg = code[offset + 1];
            let val = code[offset + 2] != 0;
            println!("{:-12} Reg: {:<3} Val: {}", "LOAD_BOOL", reg, val);
            offset + 3
        }
        OpCode::LoadConst | OpCode::LoadFun | OpCode::LoadObject => {
            let reg = code[offset + 1];
            let idx = u16::from_le_bytes([code[offset + 2], code[offset + 3]]) as usize;
            let name = match opcode {
                OpCode::LoadFun => "LOAD_FUN",
                OpCode::LoadObject => "LOAD_OBJ",
                _ => "LOAD_CONST",
            };
            println!(
                "{:-12} Reg: {:<3} ConstIdx: {} ({:?})",
                name,
                reg,
                idx,
                constants
                    .get(idx)
                    .map(|v| v.to_string())
                    .unwrap_or_default()
            );
            offset + 4
        }
        OpCode::LoadReg => {
            let dest = code[offset + 1];
            let src = code[offset + 2];
            println!("{:-12} Dest: {:<3} Src: {:<3}", "LOAD_REG", dest, src);
            offset + 3
        }
        OpCode::LoadEnum => {
            let reg = code[offset + 1];
            let enum_idx = u16::from_le_bytes([code[offset + 2], code[offset + 3]]);
            let tag = code[offset + 4];
            let args_count = code[offset + 5];
            let mut args = vec![];
            for i in 0..args_count {
                args.push(code[offset + 6 + (i as usize) * 9 + 8]);
            }
            println!(
                "LOAD_ENUM    Reg: {}, Enum: {}, tag: {} fields: {:?}",
                reg, enum_idx, tag, args
            );
            offset + 6 + 9 * args_count as usize
        }
        OpCode::Call => {
            let ret = code[offset + 1];
            let func = code[offset + 2];
            let args_count = code[offset + 3];
            let mut args = vec![];

            for i in 0..args_count {
                args.push(code[offset + 4 + i as usize]);
            }

            println!(
                "{:-12} RetReg: {:<3} FuncReg: {:<3} Args: {:?}",
                "CALL", ret, func, args
            );
            offset + 4 + args_count as usize
        }
        OpCode::TailCall => {
            let func = code[offset + 1];
            let args_count = code[offset + 2];
            let mut args = vec![];

            for i in 0..args_count {
                args.push(code[offset + 3 + i as usize]);
            }

            println!("{:-12} FuncReg: {:<3} Args: {:?}", "TAIL_CALL", func, args);
            offset + 3 + args_count as usize
        }
        OpCode::LoadNative => {
            let reg = code[offset + 1];
            let idx = u16::from_le_bytes([code[offset + 2], code[offset + 3]]) as usize;
            println!(
                "{:-12} Reg: {:<3} NativeIdx: {} ({})",
                "LOAD_NATIVE",
                reg,
                idx,
                constants
                    .get(idx)
                    .map(|v| v.to_string())
                    .unwrap_or_default()
            );
            offset + 4
        }
        OpCode::Binary => {
            let op = code[offset + 1];
            let dest = code[offset + 2];
            let left = code[offset + 3];
            let right = code[offset + 4];
            println!(
                "{:-12} Op: {} Dest: {} L: {} R: {}",
                "BINARY", op, dest, left, right
            );
            offset + 5
        }
        OpCode::Return => {
            println!("{:-12} Reg: {}", "RETURN", code[offset + 1]);
            offset + 2
        }
        OpCode::Jump => {
            let jump_offset = u16::from_le_bytes([code[offset + 1], code[offset + 2]]) as usize;
            let mut target = jump_offset;
            let name = if opcode == OpCode::Jump {
                target += offset + 3;
                "JUMP"
            } else {
                "JUMP_BY"
            };
            println!("{:-12} {:04} (offset: {})", name, target, jump_offset);
            offset + 3
        }
        OpCode::JumpNot => {
            let reg = code[offset + 1];
            let jump_offset = u16::from_le_bytes([code[offset + 2], code[offset + 3]]) as usize;
            let target = offset + 4 + jump_offset;
            let name = "JUMP_NOT";
            println!("{:-12} Reg: {:<3} Target: {:04}", name, reg, target);
            offset + 4
        }
        OpCode::GetProperty => {
            let dest = code[offset + 1];
            let src = code[offset + 2];
            let idx = code[offset + 3] as usize;
            println!(
                "{:-12} DestReg: {:<3} SrcReg: {:<3} ConstIdx: {} ({})",
                "GET_PROP",
                dest,
                src,
                idx,
                constants
                    .get(idx)
                    .map(|v| v.to_string())
                    .unwrap_or_default()
            );
            offset + 4
        }
        OpCode::SetProperty => {
            let obj = code[offset + 1];
            let idx = u16::from_le_bytes([code[offset + 2], code[offset + 3]]) as usize;
            let src = code[offset + 4];
            println!(
                "{:-12} ObjReg: {:<3} ConstIdx: {} ({}) SrcReg: {:<3}",
                "SET_PROP",
                obj,
                idx,
                constants
                    .get(idx)
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
                src
            );
            offset + 5
        }
        OpCode::GetPropertyDyn => {
            let dest = code[offset + 1];
            let src = code[offset + 2];
            let idx = code[offset + 3] as usize;
            println!(
                "{:-12} DestReg: {:<3} SrcReg: {:<3} ConstIdx: {} ({})",
                "GET_PROPD",
                dest,
                src,
                idx,
                constants
                    .get(idx)
                    .map(|v| v.to_string())
                    .unwrap_or_default()
            );
            offset + 4
        }
        OpCode::SetPropertyDyn => {
            let obj = code[offset + 1];
            let idx = code[offset + 2] as usize;
            let src = code[offset + 3];
            println!(
                "{:-12} ObjReg: {:<3} ConstIdx: {} ({}) SrcReg: {:<3}",
                "SET_PROPD",
                obj,
                idx,
                constants
                    .get(idx)
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
                src
            );
            offset + 4
        }
        OpCode::LoadGlobal => {
            let reg = code[offset + 1];
            let dest = code[offset + 2];
            println!("{:-12} Reg: {:<3} Dest: {:<3}", "LOAD", reg, dest);

            offset + 3
        }
        OpCode::Inc => {
            let src = code[offset + 1];
            println!("{:-12} Src: {:<3}", "INC", src);

            offset + 2
        }
        OpCode::Dec => {
            let src = code[offset + 1];
            println!("{:-12} Src: {:<3}", "DEC", src);

            offset + 2
        }
        OpCode::MatchEnum => {
            let dest = code[offset + 1];
            let src = code[offset + 2];
            let enum_idx = u16::from_le_bytes([code[offset + 3], code[offset + 4]]);
            let tag = code[offset + 5];
            println!(
                "{:-12} Dest: {} Src: {} EnumIdx: {} Tag: {}",
                "MATCH_ENUM", dest, src, enum_idx, tag
            );
            offset + 6
        }
    }
}
