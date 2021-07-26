use std::io;
use std::io::Write;
use std::rc::Rc;

use parity_wasm::elements::Instruction;
use wasmer::wasmparser::Operator::Else;

use crate::StringErr;
use crate::types::instance::{FunctionInstance, Instance, ToValueType, WASMFunction};

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
    ($this: ident, $var: expr) => {{
        let l = $this.pop()? + $var as u64;

        if l > i32::MAX as u64 {
            return Err(StringErr::new("memory access overflow"));
        }

        l as u32
    }};
}

pub(crate) trait Runnable {
    fn run(&mut self) -> Result<Option<u64>, StringErr>;
}

static mut CNT: u64 = 0;

impl Runnable for Instance {
    fn run(&mut self) -> Result<Option<u64>, StringErr> {
        self.push_label(self.result_type.is_some(), 0, 0, false)?;

        while self.label_size != 0 {
            if self.label_pc as usize >= self.frame_body.len() {
                self.pop_label()?;
                continue;
            }

            let ins = &self.frame_body[self.label_pc as usize];
            println!("{:?}", ins);
            self.label_pc += 1;

            unsafe { CNT += 1 };

            if unsafe { CNT == 1000 } {
                return Err(StringErr::new("limited"));
            }

            match ins {
                Instruction::Return => {
                    return Ok(self.ret()?);
                }
                Instruction::Nop
                | Instruction::I32ReinterpretF32
                | Instruction::I64ReinterpretF64
                | Instruction::I64ExtendUI32
                | Instruction::Else
                => {}
                Instruction::End => {
                    self.pop_label()?;
                    continue;
                },
                Instruction::Block(t) => {
                    self.push_label(t.to_value_type().is_some(), self.label_pc, self.label_pc, false)?;
                }
                Instruction::Loop(_) => {
                    self.push_label(false, self.label_pc, self.label_pc, true)?;
                }
                Instruction::If(t) => {
                    let arity = t.to_value_type().is_some();
                    let mut else_pc = self.label_pc;

                    while self.frame_body[else_pc as usize] != Instruction::Else {
                        else_pc += 1;
                    }

                    let c = self.pop()?;

                    if c != 0 {
                        self.push_label(arity, self.label_pc, self.label_pc, false)?;
                    } else {
                        self.push_label(arity, else_pc, else_pc, false)?;
                    }
                }
                Instruction::Br(n) => {
                    self.branch(*n)?;
                }
                Instruction::BrIf(n) => {
                    let m = *n;
                    let c = self.pop()?;
                    if c != 0 {
                        self.branch(m)?;
                    }
                }
                Instruction::BrTable(data) => {
                    let n = {
                        let tb = &data.table;
                        let i = self.peek()? as u32;
                        let sz = tb.len();

                        if sz == 0 {
                            return Err(StringErr::new("invalid empty br table data"));
                        }
                        if (i as usize) < sz - 1 {
                            tb[i as usize]
                        } else {
                            tb[sz - 1]
                        }
                    };

                    self.drop_unchecked();
                    self.branch(n)?;
                }
                Instruction::Drop => {
                    self.pop()?;
                }
                Instruction::Call(n) => {
                    let n = *n;

                    let f: Rc<WASMFunction> = {
                        match self.functions.get(n as usize) {
                            None => {
                                let msg = format!("call failed, invalid function index {}", n);
                                return Err(StringErr::new(msg));
                            }
                            Some(fun) => match fun {
                                FunctionInstance::HostFunction(_) => {
                                    return Err(StringErr::new("host function is not supported yet"));
                                }
                                FunctionInstance::WasmFunction(w) => w.clone(),
                            },
                        }
                    };

                    self.push_frame(f.clone(), None)?;
                    let res = self.run()?;

                    if !&f.fn_type.results().is_empty() {
                        self.push(res.unwrap())?;
                    }
                }
                Instruction::CallIndirect(n, m) => {
                    let index = self.pop()? as usize;

                    let f = {
                        match &self.table.functions[index] {
                            None => return Err(StringErr::new("function not found in table")),
                            Some(f) => match f {
                                FunctionInstance::HostFunction(_) => {
                                    return Err(StringErr::new("host function is not supported"));
                                }
                                FunctionInstance::WasmFunction(w) => w.clone(),
                            },
                        }
                    };

                    self.push_frame(f.clone(), None)?;
                    let r = self.run()?;

                    if !f.fn_type.results().is_empty() {
                        self.push(r.unwrap())?
                    }
                }
                Instruction::Select => {
                    let c = self.pop()?;
                    let val2 = self.pop()?;
                    let val1 = self.pop()?;
                    if c != 0 {
                        self.push(val1)?;
                    } else {
                        self.push(val2)?;
                    }
                }

                Instruction::GetLocal(n) => {
                    let loc = self.get_local(*n)?;
                    self.push(loc)?
                }
                Instruction::SetLocal(n) => {
                    let n = *n;
                    let v = self.pop()?;
                    self.set_local(n, v)?
                }
                Instruction::TeeLocal(n) => {
                    let n = *n;
                    let v = self.pop()?;
                    self.push(v)?;
                    self.push(v)?;
                    let v1 = self.pop()?;
                    self.set_local(n, v1)?;
                }
                Instruction::GetGlobal(n) => {
                    let v = get_or_err!(self.globals, *n as usize, "access global overflow");
                    self.push(*v)?;
                }
                Instruction::SetGlobal(n) => {
                    let t = get_or_err!(self.global_types, *n as usize, "access global overflow");
                    if !t.is_mutable() {
                        return Err(StringErr::new("modify global failed: immutable"));
                    }
                    let n = *n;
                    let v = self.pop()?;
                    self.globals[n as usize] = v;
                }
                Instruction::I32Load(x, _) => {
                    let x = *x;
                    let off = mem_off!(self, x);
                    self.push(self.memory.load_u32(off)? as u64)?;
                }
                Instruction::I64Load32U(x, _) => {
                    let x = *x;
                    let off = mem_off!(self, x);
                    self.push(self.memory.load_u32(off)? as u64)?;
                }

                Instruction::I64Load(x, _) => {
                    let x = *x;
                    let off = mem_off!(self, x);
                    self.push(self.memory.load_u64(off)?)?;
                }
                Instruction::I32Load8S(x, _) => {
                    let x = *x;
                    let off = mem_off!(self, x);
                    self.push(self.memory.load_u8(off)? as i8 as i32 as u32 as u64)?;
                }
                Instruction::I64Load8S(x, _) => {
                    let x = *x;
                    let off = mem_off!(self, x);
                    self.push(self.memory.load_u8(off)? as i8 as i64 as u64)?;
                }
                Instruction::I32Load8U(x, _) => {
                    let x = *x;
                    let off = mem_off!(self, x);
                    self.push(self.memory.load_u8(off)? as u64)?;
                }
                Instruction::I64Load8U(x, _) => {
                    let x = *x;
                    let off = mem_off!(self, x);
                    self.push(self.memory.load_u8(off)? as u64)?;
                }
                Instruction::I32Load16S(x, _) => {
                    let x = *x;
                    let off = mem_off!(self, x);
                    self.push(self.memory.load_u16(off)? as i16 as i32 as u32 as u64)?;
                }
                Instruction::I64Load16S(x, _) => {
                    let x = *x;
                    let off = mem_off!(self, x);
                    self.push(self.memory.load_u16(off)? as i16 as i64 as u64)?;
                }
                Instruction::I32Load16U(x, _) => {
                    let x = *x;
                    let off = mem_off!(self, x);
                    self.push(self.memory.load_u16(off)? as u64)?;
                }
                Instruction::I64Load16U(x, _) => {
                    let x = *x;
                    let off = mem_off!(self, x);
                    self.push(self.memory.load_u16(off)? as u64)?;
                }
                Instruction::I64Load32S(x, _) => {
                    let x = *x;
                    let off = mem_off!(self, x);
                    self.push(self.memory.load_u32(off)? as i32 as i64 as u64)?;
                }
                Instruction::I32Store8(x, _) => {
                    let x = *x;
                    let off = mem_off!(self, x);
                    let v = self.pop()? as u8;
                    self.memory.store_u8(off, v)?;
                }
                Instruction::I64Store8(x, _) => {
                    let x = *x;
                    let off = mem_off!(self, x);
                    let v = self.pop()? as u8;
                    self.memory.store_u8(off, v)?;
                }
                Instruction::I32Store16(x, _) => {
                    let x = *x;
                    let off = mem_off!(self, x);
                    let v = self.pop()? as u16;
                    self.memory.store_u16(off, v)?;
                }
                Instruction::I64Store16(x, _) => {
                    let x = *x;
                    let off = mem_off!(self, x);
                    let v = self.pop()? as u16;
                    self.memory.store_u16(off, v)?;
                }
                Instruction::I32Store(x, _) => {
                    let x = *x;
                    let off = mem_off!(self, x);
                    let v = self.pop()? as u32;
                    self.memory.store_u32(off, v)?;
                }
                Instruction::I64Store32(x, _) => {
                    let x = *x;
                    let off = mem_off!(self, x);
                    let v = self.pop()? as u32;
                    self.memory.store_u32(off, v)?;
                }
                Instruction::I64Store(x, _) => {
                    let x = *x;
                    let off = mem_off!(self, x);
                    let v = self.pop()?;
                    self.memory.store_u64(off, v)?;
                }
                Instruction::CurrentMemory(_) => {
                    let p = self.memory.pages;
                    self.push(p as u64)?;
                }
                Instruction::GrowMemory(_) => {
                    let n = self.pop()?;
                    let grow_result = self.memory.grow(n as u32)?;
                    self.push(grow_result as u64)?;
                }
                Instruction::I32Const(x) => {
                    let x = *x;
                    self.push(x as u32 as u64)?;
                }
                Instruction::I64Const(x) => {
                    let x = *x;
                    self.push(x as u64)?;
                }
                Instruction::I32Clz => {
                    let n = self.pop()? as u32;
                    self.push(n.leading_ones() as u64)?;
                }
                Instruction::I32Ctz => {
                    let n = self.pop()? as u32;
                    self.push(n.trailing_zeros() as u64)?;
                }
                Instruction::I32Popcnt => {
                    let n = self.pop()? as u32;
                    self.push(n.count_ones() as u64)?;
                }
                Instruction::I32Add => {
                    let (v2, v1) = v2_v1!(self);
                    self.push(unsafe { v2.unchecked_add(v1) } as u32 as u64)?;
                }
                Instruction::I32Mul => {
                    let (v2, v1) = v2_v1!(self);
                    self.push(unsafe { v2.unchecked_mul(v1) } as u32 as u64)?;
                }
                Instruction::I32DivS => {
                    let (v2, v1) = v2_v1!(self);
                    if v1 == 0 {
                        return Err(StringErr::new("divided by zero"));
                    }
                    if v1 == (0x80000000u32 as i32) && v2 == -1 {
                        return Err(StringErr::new("math over flow: divide i32.min_value by -1"));
                    }
                    self.push((v2 / v1) as u32 as u64)?;
                }
                Instruction::I32DivU => {
                    let (v2, v1) = v2_v1!(self);
                    self.push((v2 as u32 / v1 as u32) as u64)?;
                }
                Instruction::I32RemS => {
                    let (v2, v1) = v2_v1!(self);
                    if v1 == 0 {
                        return Err(StringErr::new("divided by zero"));
                    }
                    self.push((v1 % v2) as u32 as u64)?;
                }
                Instruction::I32RemU => {
                    let (v2, v1) = v2_v1!(self);
                    if v1 == 0 {
                        return Err(StringErr::new("divided by zero"));
                    }
                    self.push((v1 as u32 % v2 as u32) as u64)?;
                }
                Instruction::I32Sub => {
                    let (v2, v1) = v2_v1!(self);
                    self.push(unsafe { v1.unchecked_sub(v2) } as u32 as u64)?;
                }
                Instruction::I32And => {
                    let (v2, v1) = v2_v1!(self);
                    self.push((v1 & v2) as u32 as u64)?;
                }
                Instruction::I32Or => {
                    let (v2, v1) = v2_v1!(self);
                    self.push((v1 | v2) as u32 as u64)?;
                }
                Instruction::I32Xor => {
                    let (v2, v1) = v2_v1!(self);
                    self.push((v1 ^ v2) as u32 as u64)?;
                }
                Instruction::I32Shl => {
                    let (v2, v1) = v2_v1!(self);
                    self.push((v1 << (v2 as usize)) as u32 as u64)?;
                }
                Instruction::I32ShrU => {
                    let (v2, v1) = v2_v1!(self);
                    self.push(((v1 as u32) >> (v2 as usize)) as u32 as u64)?;
                }
                Instruction::I32ShrS => {
                    let (v2, v1) = v2_v1!(self);
                    self.push((v1 >> (v2 as usize)) as u32 as u64)?;
                }
                Instruction::I32Rotl => {
                    let (v2, v1) = v2_v1!(self);
                    self.push(v1.rotate_left(v2 as u32) as u32 as u64)?;
                }
                Instruction::I32Rotr => {
                    let (v2, v1) = v2_v1!(self);
                    self.push(v1.rotate_right(v2 as u32) as u32 as u64)?;
                }
                Instruction::I32LeS => {
                    let (v2, v1) = v2_v1!(self);
                    self.push((v1 <= v2) as u64)?;
                }
                Instruction::I32LeU => {
                    let (v2, v1) = v2_v1!(self);
                    self.push(((v1 as u32) <= (v2 as u32)) as u64)?;
                }
                Instruction::I32LtS => {
                    let (v2, v1) = v2_v1!(self);
                    self.push((v1 < v2) as u64)?;
                }
                Instruction::I32LtU => {
                    let (v2, v1) = v2_v1!(self);
                    self.push(((v1 as u32) < (v2 as u32)) as u64)?;
                }
                Instruction::I32GtS => {
                    let (v2, v1) = v2_v1!(self);
                    self.push((v1 > v2) as u64)?;
                }
                Instruction::I32GtU => {
                    let (v2, v1) = v2_v1!(self);
                    self.push(((v1 as u32) > (v2 as u32)) as u64)?;
                }
                Instruction::I32GeS => {
                    let (v2, v1) = v2_v1!(self);
                    self.push((v1 >= v2) as u64)?;
                }
                Instruction::I32GeU => {
                    let (v2, v1) = v2_v1!(self);
                    self.push(((v1 as u32) >= (v2 as u32)) as u64)?;
                }
                Instruction::I32Eqz => {
                    let v = self.pop()? as u32;
                    self.push((v == 0) as u64)?;
                }
                Instruction::I32Eq => {
                    let (v2, v1) = v2_v1!(self);
                    self.push(((v1 as u32) == (v2 as u32)) as u64)?;
                }
                Instruction::I32Ne => {
                    let (v2, v1) = v2_v1!(self);
                    self.push(((v1 as u32) != (v2 as u32)) as u64)?;
                }
                Instruction::I64Clz => {
                    let v = self.pop()?;
                    self.push(v.leading_zeros() as u64)?;
                }
                Instruction::I64Ctz => {
                    let v = self.pop()?;
                    self.push(v.trailing_zeros() as u64)?;
                }
                Instruction::I64Popcnt => {
                    let v = self.pop()?;
                    self.push(v.count_ones() as u64)?;
                }
                Instruction::I64Add => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push(unsafe { v1.unchecked_add(v2) } as u64)?;
                }
                Instruction::I64Sub => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push(unsafe { v1.unchecked_sub(v2) } as u64)?;
                }
                Instruction::I64Mul => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push(unsafe { v1.unchecked_mul(v2) } as u64)?;
                }
                Instruction::I64DivS => {
                    let (v2, v1) = v2_v1_!(self);
                    if v2 == 0 {
                        return Err(StringErr::new("divided by zero"));
                    }
                    if v1 == (0x8000000000000000u64 as i64) && v2 == -1 {
                        return Err(StringErr::new("math overflow: divide i64.min_value by -1"));
                    }
                    self.push((v1 / v2) as u64)?;
                }
                Instruction::I64DivU => {
                    let (v2, v1) = v2_v1_!(self);
                    if v2 == 0 {
                        return Err(StringErr::new("divided by zero"));
                    }
                    self.push(v1 as u64 / v2 as u64)?;
                }
                Instruction::I64RemS => {
                    let (v2, v1) = v2_v1_!(self);
                    if v2 == 0 {
                        return Err(StringErr::new("divided by zero"));
                    }
                    self.push((v1 % v2) as u64)?;
                }
                Instruction::I64RemU => {
                    let (v2, v1) = v2_v1_!(self);
                    if v2 == 0 {
                        return Err(StringErr::new("divided by zero"));
                    }
                    self.push(v1 as u64 % v2 as u64)?;
                }
                Instruction::I64And => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push((v1 & v2) as u64)?;
                }
                Instruction::I64Or => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push((v1 | v2) as u64)?;
                }
                Instruction::I64Xor => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push((v1 ^ v2) as u64)?;
                }
                Instruction::I64Shl => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push((v1 << (v2 as u64)) as u64)?;
                }
                Instruction::I64ShrU => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push(((v1 as u64) >> (v2 as u64)) as u64)?;
                }
                Instruction::I64ShrS => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push((v1 >> (v2 as u64)) as u64)?;
                }
                Instruction::I64Rotl => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push(v1.rotate_left(v2 as u32) as u64)?;
                }
                Instruction::I64Rotr => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push(v1.rotate_right(v2 as u32) as u64)?;
                }
                Instruction::I64LeS => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push((v1 <= v2) as u64)?;
                }
                Instruction::I64LeU => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push(((v1 as u64) <= (v2 as u64)) as u64)?;
                }
                Instruction::I64LtS => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push((v1 < v2) as u64)?;
                }
                Instruction::I64LtU => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push(((v1 as u64) < (v2 as u64)) as u64)?;
                }
                Instruction::I64GtS => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push((v1 > v2) as u64)?;
                }
                Instruction::I64GtU => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push(((v1 as u64) > (v2 as u64)) as u64)?;
                }
                Instruction::I64GeS => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push((v1 >= v2) as u64)?;
                }
                Instruction::I64GeU => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push(((v1 as u64) >= (v2 as u64)) as u64)?;
                }
                Instruction::I64Eqz => {
                    let v = self.pop()?;
                    self.push((v == 0) as u64)?;
                }
                Instruction::I64Eq => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push((v1 == v2) as u64)?;
                }
                Instruction::I64Ne => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push((v1 != v2) as u64)?;
                }
                _ => return Err(StringErr::new(format!("unsupported op {}", ins))),
            }
        }

        self.ret()
    }
}