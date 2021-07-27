use std::io;
use std::io::Write;
use std::rc::Rc;

use parity_wasm::elements::Instruction;
use wasmer::wasmparser::Operator::Else;

use parity_wasm::elements::opcodes;
use crate::StringErr;
use crate::types::instance::{FunctionInstance, Instance, ToValueType, WASMFunction};
use crate::types::names;
use crate::types::frame_data::FuncBits;

macro_rules! v2_v1 {
    ($this: ident) => {
        {
            let v2 = $this.pop()? as i32;
            let v1 = $this.pop()? as i32;
            (v2, v1)
        }
    };
}

macro_rules! v2_v1_ {
    ($this: ident) => {
        {
            let v2 = $this.pop()? as i64;
            let v1 = $this.pop()? as i64;
            (v2, v1)
        }
    };
}

macro_rules! mem_off {
    ($this: ident, $ins: expr) => {{
        let l = $this.pop()? + $ins.payload() as u64;

        if l > i32::MAX as u64 {
            return Err(StringErr::new(format!("memory access overflow l > i32::MAX op = {}", names::name($ins.op_code()))));
        }

        l as u32
    }};
}

pub(crate) trait Runnable {
    fn run(&mut self) -> Result<u64, StringErr>;
}

fn cnt() -> u64 {
    unsafe { CNT }
}

#[cfg(test)]
static mut CNT: u64 = 0;

impl Runnable for Instance {
    fn run(&mut self) -> Result<u64, StringErr> {
        self.push_label(self.result_type.is_some(), self.frame_body, false)?;

        while self.label_size != 0 {
            if self.label_pc as usize >= self.label_body.size() {
                self.pop_label()?;
                continue;
            }

            let ins = self.pool.ins_in_vec(self.label_body, self.label_pc as usize);

            if ins.op_code() == opcodes::RETURN {
                return self.ret();
            }

            self.label_pc += 1;

            // self.print_stack();

            #[cfg(test)] {
                unsafe { CNT += 1 };
                // println!("count = {} code = {}", unsafe { CNT }, names::name(ins.op_code()));

                if unsafe { CNT % 10000 == 0} {
                    println!("cnt = {}", unsafe { CNT } );
                }

                if unsafe { CNT == 98000000 } {
                    return Err(StringErr::new("unexpected"))
                }
            }

            match ins.op_code() {
                opcodes::NOP
                | opcodes::I32REINTERPRETF32
                | opcodes::I64REINTERPRETF64
                | opcodes::I64EXTENDUI32
                => {}
                opcodes::UNREACHABLE => {
                    return Err(StringErr::new("wasm: unreachable()"));
                }
                opcodes::BLOCK => {
                    self.push_label(ins.block_type().is_some(), self.pool.branch0(ins),  false)?;
                }
                opcodes::LOOP => {
                    self.push_label(false, self.pool.branch0(ins),  true)?;
                }
                opcodes::IF => {
                    let arity = ins.block_type().is_some();
                    let c = self.pop()?;
                    if c != 0 {
                        self.push_label(arity, self.pool.branch0(ins), false)?;
                        continue;
                    } else {
                        let else_branch = self.pool.branch1(ins);

                        if else_branch.is_null() {
                            continue;
                        }
                        self.push_label(arity, else_branch, false)?;
                    }
                }
                opcodes::BR => {
                    self.branch(ins.payload())?;
                }
                opcodes::BRIF => {
                    let c = self.pop()?;
                    if c != 0 {
                        self.branch(ins.payload())?;
                    }
                }
                opcodes::BRTABLE => {
                    let n = {
                        let sz = ins.operand_size();
                        let i = self.peek()? as u32;

                        if sz == 0 {
                            return Err(StringErr::new("invalid empty br table data"));
                        }

                        if i < sz as u32 - 1 {
                            self.pool.operand(ins, i as usize)
                        } else {
                            self.pool.operand(ins, sz as usize - 1)
                        }
                    };

                    self.drop_unchecked();
                    self.branch(n as u32)?;
                }
                opcodes::DROP => {
                    self.pop()?;
                }
                opcodes::CALL => {
                    let n = ins.payload();
                    self.push_frame(FuncBits::normal(n as u16), None)?;
                    let arity = self.result_type.is_some();
                    let res = self.run()?;
                    if arity {
                        self.push(res)?;
                    }
                }
                opcodes::CALLINDIRECT  => {
                    let index = self.pop()? as usize;
                    self.push_frame(FuncBits::table(index as u16), None)?;
                    let arity = self.result_type.is_some();
                    let r = self.run()?;
                    if arity {
                        self.push(r)?
                    }
                }
                opcodes::SELECT => {
                    let c = self.pop()?;
                    let val2 = self.pop()?;
                    let val1 = self.pop()?;
                    if c != 0 {
                        self.push(val1)?;
                    } else {
                        self.push(val2)?;
                    }
                }

                opcodes::GETLOCAL => {
                    let loc = self.get_local(ins.payload())?;
                    self.push(loc)?
                }
                opcodes::SETLOCAL => {
                    let v = self.pop()?;
                    self.set_local(ins.payload(), v)?
                }
                opcodes::TEELOCAL => {
                    let v = self.pop()?;
                    self.push(v)?;
                    self.push(v)?;
                    let v1 = self.pop()?;
                    self.set_local(ins.payload(), v1)?;
                }
                opcodes::GETGLOBAL => {
                    let v = get_or_err!(self.globals, ins.payload() as usize, "access global overflow");
                    self.push(*v)?;
                }
                opcodes::SETGLOBAL => {
                    let t = get_or_err!(self.global_types, ins.payload() as usize, "access global overflow");
                    if !t.is_mutable() {
                        return Err(StringErr::new("modify global failed: immutable"));
                    }
                    let v = self.pop()?;
                    self.globals[ins.payload() as usize] = v;
                }
                opcodes::I32LOAD | opcodes::I64LOAD32U => {
                    let off = mem_off!(self, ins);
                    let loaded = self.memory.load_u32(off)? as u64;
                    self.push(loaded)?;
                }
                opcodes::I64LOAD => {
                    let off = mem_off!(self, ins);
                    self.push(self.memory.load_u64(off)?)?;
                }
                opcodes::I32LOAD8S => {
                    let off = mem_off!(self, ins);
                    self.push(self.memory.load_u8(off)? as i8 as i32 as u32 as u64)?;
                }
                opcodes::I64LOAD8S => {
                    let off = mem_off!(self, ins);
                    self.push(self.memory.load_u8(off)? as i8 as i64 as u64)?;
                }
                opcodes::I32LOAD8U | opcodes::I64LOAD8U=> {
                    let off = mem_off!(self, ins);
                    self.push(self.memory.load_u8(off)? as u64)?;
                }
                opcodes::I32LOAD16S => {
                    let off = mem_off!(self, ins);
                    self.push(self.memory.load_u16(off)? as i16 as i32 as u32 as u64)?;
                }
                opcodes::I64LOAD16S => {
                    let off = mem_off!(self, ins);
                    self.push(self.memory.load_u16(off)? as i64 as u64)?;
                }
                opcodes::I32LOAD16U | opcodes::I64LOAD16U=> {
                    let off = mem_off!(self, ins);
                    self.push(self.memory.load_u16(off)? as u64)?;
                }
                opcodes::I64LOAD32S => {
                    let off = mem_off!(self, ins);
                    self.push(self.memory.load_u32(off)? as i32 as i64 as u64)?;
                }
                opcodes::I32STORE8 | opcodes::I64STORE8 => {
                    let v = self.pop()? as u8;
                    let off = mem_off!(self, ins);
                    self.memory.store_u8(off, v)?;
                }
                opcodes::I32STORE16 |opcodes::I64STORE16 => {
                    let v = self.pop()? as u16;
                    let off = mem_off!(self, ins);
                    self.memory.store_u16(off, v)?;
                }
                opcodes::I32STORE | opcodes::I64STORE32 => {
                    let v = self.pop()? as u32;
                    if v > u16::max as u32{
                        println!("i32store v = {}", v);
                    }
                    let off = mem_off!(self, ins);
                    self.memory.store_u32(off, v)?;
                }
                opcodes::I64STORE => {
                    let v = self.pop()?;
                    let off = mem_off!(self, ins);
                    self.memory.store_u64(off, v)?;
                }
                opcodes::CURRENTMEMORY => {
                    let p = self.memory.pages;
                    self.push(p as u64)?;
                }
                opcodes::GROWMEMORY => {
                    let n = self.pop()?;
                    let grow_result = self.memory.grow(n as u32)?;
                    println!("grow memory n = {}", n);

                    self.push(grow_result as u64)?;
                }
                opcodes::I32CONST => {
                    self.push(ins.payload() as u32 as u64)?;
                }
                opcodes::I64CONST => {
                    self.push(self.pool.operand(ins, 0))?;
                }
                opcodes::I32CLZ => {
                    let n = self.pop()? as u32;
                    self.push(n.leading_ones() as u64)?;
                }
                opcodes::I32CTZ => {
                    let n = self.pop()? as u32;
                    self.push(n.trailing_zeros() as u64)?;
                }
                opcodes::I32POPCNT => {
                    let n = self.pop()? as u32;
                    self.push(n.count_ones() as u64)?;
                }
                opcodes::I32ADD => {
                    let (v2, v1) = v2_v1!(self);
                    self.push(unsafe { v2.unchecked_add(v1) } as u32 as u64)?;
                }
                opcodes::I32MUL => {
                    let (v2, v1) = v2_v1!(self);
                    self.push(unsafe { v2.unchecked_mul(v1) } as u32 as u64)?;
                }
                opcodes::I32DIVS => {
                    let (v2, v1) = v2_v1!(self);
                    if v1 == 0 {
                        return Err(StringErr::new("divided by zero"));
                    }
                    if v1 == (0x80000000u32 as i32) && v2 == -1 {
                        return Err(StringErr::new("math over flow: divide i32.min_value by -1"));
                    }
                    self.push((v2 / v1) as u32 as u64)?;
                }
                opcodes::I32DIVU => {
                    let (v2, v1) = v2_v1!(self);
                    self.push((v2 as u32 / v1 as u32) as u64)?;
                }
                opcodes::I32REMS => {
                    let (v2, v1) = v2_v1!(self);
                    if v1 == 0 {
                        return Err(StringErr::new("divided by zero"));
                    }
                    self.push((v1 % v2) as u32 as u64)?;
                }
                opcodes::I32REMU => {
                    let (v2, v1) = v2_v1!(self);
                    if v1 == 0 {
                        return Err(StringErr::new("divided by zero"));
                    }
                    self.push((v1 as u32 % v2 as u32) as u64)?;
                }
                opcodes::I32SUB => {
                    let (v2, v1) = v2_v1!(self);
                    self.push(unsafe { v1.unchecked_sub(v2) } as u32 as u64)?;
                }
                opcodes::I32AND => {
                    let (v2, v1) = v2_v1!(self);
                    self.push((v1 & v2) as u32 as u64)?;
                }
                opcodes::I32OR => {
                    let (v2, v1) = v2_v1!(self);
                    self.push((v1 | v2) as u32 as u64)?;
                }
                opcodes::I32XOR => {
                    let (v2, v1) = v2_v1!(self);
                    self.push((v1 ^ v2) as u32 as u64)?;
                }
                opcodes::I32SHL => {
                    let (v2, v1) = v2_v1!(self);
                    self.push((v1 << (v2 as usize)) as u32 as u64)?;
                }
                opcodes::I32SHRU => {
                    let (v2, v1) = v2_v1!(self);
                    self.push(((v1 as u32) >> (v2 as usize)) as u32 as u64)?;
                }
                opcodes::I32SHRS => {
                    let (v2, v1) = v2_v1!(self);
                    self.push((v1 >> (v2 as usize)) as u32 as u64)?;
                }
                opcodes::I32ROTL => {
                    let (v2, v1) = v2_v1!(self);
                    self.push(v1.rotate_left(v2 as u32) as u32 as u64)?;
                }
                opcodes::I32ROTR => {
                    let (v2, v1) = v2_v1!(self);
                    self.push(v1.rotate_right(v2 as u32) as u32 as u64)?;
                }
                opcodes::I32LES => {
                    let (v2, v1) = v2_v1!(self);
                    self.push((v1 <= v2) as u64)?;
                }
                opcodes::I32LEU => {
                    let (v2, v1) = v2_v1!(self);
                    self.push(((v1 as u32) <= (v2 as u32)) as u64)?;
                }
                opcodes::I32LTS => {
                    let (v2, v1) = v2_v1!(self);
                    self.push((v1 < v2) as u64)?;
                }
                opcodes::I32LTU => {
                    let (v2, v1) = v2_v1!(self);
                    self.push(((v1 as u32) < (v2 as u32)) as u64)?;
                }
                opcodes::I32GTS => {
                    let (v2, v1) = v2_v1!(self);
                    self.push((v1 > v2) as u64)?;
                }
                opcodes::I32GTU => {
                    let (v2, v1) = v2_v1!(self);
                    self.push(((v1 as u32) > (v2 as u32)) as u64)?;
                }
                opcodes::I32GES => {
                    let (v2, v1) = v2_v1!(self);
                    self.push((v1 >= v2) as u64)?;
                }
                opcodes::I32GEU => {
                    let (v2, v1) = v2_v1!(self);
                    self.push(((v1 as u32) >= (v2 as u32)) as u64)?;
                }
                opcodes::I32EQZ => {
                    let v = self.pop()? as u32;
                    self.push((v == 0) as u64)?;
                }
                opcodes::I32EQ => {
                    let (v2, v1) = v2_v1!(self);
                    self.push(((v1 as u32) == (v2 as u32)) as u64)?;
                }
                opcodes::I32NE => {
                    let (v2, v1) = v2_v1!(self);
                    self.push(((v1 as u32) != (v2 as u32)) as u64)?;
                }
                opcodes::I64CLZ => {
                    let v = self.pop()?;
                    self.push(v.leading_zeros() as u64)?;
                }
                opcodes::I64CTZ => {
                    let v = self.pop()?;
                    self.push(v.trailing_zeros() as u64)?;
                }
                opcodes::I64POPCNT => {
                    let v = self.pop()?;
                    self.push(v.count_ones() as u64)?;
                }
                opcodes::I64ADD => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push(unsafe { v1.unchecked_add(v2) } as u64)?;
                }
                opcodes::I64SUB => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push(unsafe { v1.unchecked_sub(v2) } as u64)?;
                }
                opcodes::I64MUL => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push(unsafe { v1.unchecked_mul(v2) } as u64)?;
                }
                opcodes::I64DIVS => {
                    let (v2, v1) = v2_v1_!(self);
                    if v2 == 0 {
                        return Err(StringErr::new("divided by zero"));
                    }
                    if v1 == (0x8000000000000000u64 as i64) && v2 == -1 {
                        return Err(StringErr::new("math overflow: divide i64.min_value by -1"));
                    }
                    self.push((v1 / v2) as u64)?;
                }
                opcodes::I64DIVU => {
                    let (v2, v1) = v2_v1_!(self);
                    if v2 == 0 {
                        return Err(StringErr::new("divided by zero"));
                    }
                    self.push(v1 as u64 / v2 as u64)?;
                }
                opcodes::I64REMS => {
                    let (v2, v1) = v2_v1_!(self);
                    if v2 == 0 {
                        return Err(StringErr::new("divided by zero"));
                    }
                    self.push((v1 % v2) as u64)?;
                }
                opcodes::I64REMU => {
                    let (v2, v1) = v2_v1_!(self);
                    if v2 == 0 {
                        return Err(StringErr::new("divided by zero"));
                    }
                    self.push(v1 as u64 % v2 as u64)?;
                }
                opcodes::I64AND => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push((v1 & v2) as u64)?;
                }
                opcodes::I64OR => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push((v1 | v2) as u64)?;
                }
                opcodes::I64XOR => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push((v1 ^ v2) as u64)?;
                }
                opcodes::I64SHL => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push((v1 << (v2 as u64)) as u64)?;
                }
                opcodes::I64SHRS => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push((v1 >> (v2 as u64)) as u64)?;
                }
                opcodes::I64SHRU => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push(((v1 as u64) >> (v2 as u64)) as u64)?;
                }
                opcodes::I64ROTL => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push(v1.rotate_left(v2 as u32) as u64)?;
                }
                opcodes::I64ROTR => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push(v1.rotate_right(v2 as u32) as u64)?;
                }
                opcodes::I64EQ => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push((v1 == v2) as u64)?;
                }
                opcodes::I64EQZ => {
                    let v = self.pop()?;
                    self.push((v == 0) as u64)?;
                }
                opcodes::I64NE => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push((v1 != v2) as u64)?;
                }
                opcodes::I64LTS => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push((v1 < v2) as u64)?;
                }
                opcodes::I64LTU => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push(((v1 as u64) < (v2 as u64)) as u64)?;
                }
                opcodes::I64GTS => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push((v1 > v2) as u64)?;
                }
                opcodes::I64GTU => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push(((v1 as u64) > (v2 as u64)) as u64)?;
                }
                opcodes::I64LEU => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push(((v1 as u64) <= (v2 as u64)) as u64)?;
                }
                opcodes::I64LES => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push((v1 <= v2) as u64)?;
                }
                opcodes::I64GES => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push((v1 >= v2) as u64)?;
                }
                opcodes::I64GEU => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push(((v1 as u64) >= (v2 as u64)) as u64)?;
                }
                opcodes::I32WRAPI64 => {
                    let n = self.pop()?;
                    self.push(n as u32 as u64)?;
                }
                opcodes::I64EXTENDSI32 => {
                    let n = self.pop()? as u32 as i32;
                    let m = n as i64;
                    self.push(m as u64)?;
                }
                _ => return Err(StringErr::new(format!("unsupported op {}", ins.op_code()))),
            }
        }

        self.ret()
    }
}