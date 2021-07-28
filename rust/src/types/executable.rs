use std::cmp::PartialOrd;
use std::io;
use std::io::Write;
use std::ops::{Add, BitAnd, BitOr, BitXor, Div, Neg, Rem, Shl, Shr, Sub, Mul};
use std::rc::Rc;

use parity_wasm::elements::Instruction;
use parity_wasm::elements::opcodes;
use wasmer::wasmparser::Operator::Else;

use crate::StringErr;
use crate::types::frame_data::FuncBits;
use crate::types::ins_pool::InsBits;
use crate::types::instance::{FunctionInstance, Instance, ToValueType, WASMFunction};
use crate::types::names;

trait IsZero {
    fn is_zero(&self) -> bool;
}

trait Math {
    fn nearest(&self) -> Self;
}

macro_rules! imp_is_zero {
    ($t: ident) => {
        impl IsZero for $t {
            fn is_zero(&self) -> bool {
                *self == 0 as $t
            }
        }
    };
}

macro_rules! imp_math {
    ($t: ident, $u: ident) => {
        impl Math for $t {
            fn nearest(&self) -> Self {
                (self + (0.5 as $t).copysign(*self)) as $u as $t
            }
        }
    };
}

imp_math!(f32, u32);
imp_math!(f64, u64);

imp_is_zero!(u32);
imp_is_zero!(u64);
imp_is_zero!(f32);
imp_is_zero!(f64);

macro_rules! trunc_as {
    ($this: expr, $f: ident, $f_l: ty, $t1: ty, $t2: ty) => {
        let t = $this.top()?;
        *t = $f::from_bits(*t as $f_l).trunc() as $t1 as $t2 as u64;
    };
}


macro_rules! bin_cast {
    ($this: ident, $t: ident, $op: ident) => {
        {
            let (v2, v1) = $this.top_2()?;
            *v1 = (*v1 as $t).$op(v2 as $t) as u64;
            $this.drop_unchecked();
        }
    };
}

macro_rules! bin_cmp_cast {
    ($this: ident, $t: ident, $op: ident) => {
        {
            let (v2, v1) = $this.top_2()?;
            *v1 = (*v1 as $t).$op(&(v2 as $t)) as u64;
            $this.drop_unchecked();
        }
    };
}

macro_rules! bin_cmp {
    ($this: ident, $op: ident) => {
        {
            let (v2, v1) = $this.top_2()?;
            *v1 = (*v1).$op(&v2) as u64;
            $this.drop_unchecked();
        }
    };
}

macro_rules! bin_cast_2 {
    ($this: ident, $t1: ident, $t2: ident, $op: ident) => {
        {
            let (v2, v1) = $this.top_2()?;
            *v1 = (*v1 as $t1).$op(v2 as $t2) as u64;
            $this.drop_unchecked();
        }
    };
}

macro_rules! bin {
    ($this: ident, $op: ident) => {
        {
            let (v2, v1) = $this.top_2()?;
            *v1 = (*v1).$op(v2) ;
            $this.drop_unchecked();
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

macro_rules! op_u32 {
    ($this: expr, $fun: ident) => {
        let top = $this.top()?;
        *top = ((*top as u32).$fun()) as u64;
    };
}


macro_rules! op_u64 {
    ($this: expr, $fun: ident) => {
        let top = $this.top()?;
        *top = (*top).$fun() as u64;
    };
}


macro_rules! op_f32 {
    ($this: expr, $fun: ident) => {
        let top = $this.top()?;
        *top = (f32::from_bits(*top as u32)).$fun().to_bits() as u64;
    };
}

macro_rules! op_f64 {
    ($this: expr, $fun: ident) => {
        let top = $this.top()?;
        *top = (f64::from_bits(*top)).$fun().to_bits();
    };
}

macro_rules! bin_f32 {
    ($this: ident, $op: ident) => {
        {
            let (v2, v1) = $this.top_2()?;
            *v1 = (f32::from_bits(*v1 as u32).$op(f32::from_bits(v2 as u32))).to_bits() as u64;
            $this.drop_unchecked();
        }
    };
}

macro_rules! bin_f64 {
    ($this: ident, $op: ident) => {
        {
            let (v2, v1) = $this.top_2()?;
            *v1 = (f64::from_bits(*v1).$op(f64::from_bits(v2))).to_bits();
            $this.drop_unchecked();
        }
    };
}

macro_rules! bin_cmp_f32 {
    ($this: ident, $op: ident) => {
        {
            let (v2, v1) = $this.top_2()?;
            *v1 = (f32::from_bits(*v1 as u32).$op(&f32::from_bits(v2 as u32))) as u64;
            $this.drop_unchecked();
        }
    };
}

macro_rules! bin_cmp_f64 {
    ($this: ident, $op: ident) => {
        {
            let (v2, v1) = $this.top_2()?;
            *v1 = f64::from_bits(*v1).$op(&f64::from_bits(v2)) as u64;
            $this.drop_unchecked();
        }
    };
}

pub(crate) trait Runnable {
    fn run(&mut self) -> Result<u64, StringErr>;

    fn invoke(&mut self, ins: InsBits) -> Result<(), StringErr>;
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
            let pc = self.label_pc;
            let body = self.label_body;

            if pc as usize >= body.size() {
                self.pop_label()?;
                continue;
            }

            let ins = self.pool.ins_in_vec(body, pc as usize);

            if ins.op_code() == opcodes::RETURN {
                return self.ret();
            }

            self.label_pc = pc + 1;
            self.invoke(ins)?;
        }
        self.ret()
    }

    fn invoke(&mut self, ins: InsBits) -> Result<(), StringErr> {
        match ins.op_code() {
            opcodes::NOP
            | opcodes::I32REINTERPRETF32
            | opcodes::I64REINTERPRETF64
            | opcodes::I64EXTENDUI32
            | opcodes::F32REINTERPRETI32
            | opcodes::F64REINTERPRETI64
            => {}
            opcodes::UNREACHABLE => {
                return Err(StringErr::new("wasm: unreachable()"));
            }
            opcodes::BLOCK => {
                self.push_label(ins.block_type().is_some(), self.pool.branch0(ins), false)?;
            }
            opcodes::LOOP => {
                self.push_label(false, self.pool.branch0(ins), true)?;
            }
            opcodes::IF => {
                let arity = ins.block_type().is_some();
                let c = self.pop()?;
                if c != 0 {
                    self.push_label(arity, self.pool.branch0(ins), false)?;
                    return Ok(());
                }
                let else_branch = self.pool.branch1(ins);
                if !else_branch.is_null() {
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
            opcodes::CALLINDIRECT => {
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
            opcodes::I32LOAD | opcodes::I64LOAD32U | opcodes::F32LOAD => {
                let off = mem_off!(self, ins);
                let loaded = self.memory.load_u32(off)? as u64;
                self.push(loaded)?;
            }
            opcodes::I64LOAD | opcodes::F64LOAD => {
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
            opcodes::I32LOAD8U | opcodes::I64LOAD8U => {
                let off = mem_off!(self, ins);
                self.push(self.memory.load_u8(off)? as u64)?;
            }
            opcodes::I32LOAD16S => {
                let off = mem_off!(self, ins);
                self.push(self.memory.load_u16(off)? as i16 as i32 as u32 as u64)?;
            }
            opcodes::I64LOAD16S => {
                let off = mem_off!(self, ins);
                self.push(self.memory.load_u16(off)? as i16 as i64 as u64)?;
            }
            opcodes::I32LOAD16U | opcodes::I64LOAD16U => {
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
            opcodes::I32STORE16 | opcodes::I64STORE16 => {
                let v = self.pop()? as u16;
                let off = mem_off!(self, ins);
                self.memory.store_u16(off, v)?;
            }
            opcodes::I32STORE | opcodes::I64STORE32 | opcodes::F32STORE => {
                let v = self.pop()? as u32;
                let off = mem_off!(self, ins);
                self.memory.store_u32(off, v)?;
            }
            opcodes::I64STORE | opcodes::F64STORE => {
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
                self.push(grow_result as u64)?;
            }
            opcodes::I32CONST | opcodes::F32CONST => {
                self.push(ins.payload() as u32 as u64)?;
            }
            opcodes::I64CONST | opcodes::F64CONST => {
                self.push(self.pool.operand(ins, 0))?;
            }
            opcodes::I32CLZ => {
                op_u32!(self, leading_zeros);
            }
            opcodes::I32CTZ => {
                op_u32!(self, trailing_zeros);
            }
            opcodes::I32POPCNT => {
                op_u32!(self, count_ones);
            }
            opcodes::I32ADD => {
                unsafe {
                    bin_cast!(self, u32, unchecked_add);
                }
            }
            opcodes::I32MUL => {
                unsafe {
                    bin_cast!(self, u32, unchecked_mul);
                }
            }
            opcodes::I32DIVS => {
                bin_cast!(self, i32, div);
            }
            opcodes::I32DIVU => {
                bin_cast!(self, u32, div);
            }
            opcodes::I32REMS => {
                bin_cast!(self, i32, rem);
            }
            opcodes::I32REMU => {
                bin_cast!(self, u32, rem);
            }
            opcodes::I32SUB => {
                unsafe { bin_cast!(self, u32, unchecked_sub); }
            }
            opcodes::I32AND => {
                bin_cast!(self, u32, bitand);
            }
            opcodes::I32OR => {
                bin_cast!(self, u32, bitor);
            }
            opcodes::I32XOR => {
                bin_cast!(self, u32, bitxor);
            }
            opcodes::I32SHL => {
                bin_cast!(self, u32, shl);
            }
            opcodes::I32SHRU => {
                bin_cast!(self, u32, shr);
            }
            opcodes::I32SHRS => {
                bin_cast_2!(self, i32, u32, shr);
            }
            opcodes::I32ROTL => {
                bin_cast!(self, u32, rotate_left);
            }
            opcodes::I32ROTR => {
                bin_cast!(self, u32, rotate_right);
            }
            opcodes::I32LES => {
                bin_cmp_cast!(self, i32, le);
            }
            opcodes::I32LEU => {
                bin_cmp_cast!(self, u32, le);
            }
            opcodes::I32LTS => {
                bin_cmp_cast!(self, i32, lt);
            }
            opcodes::I32LTU => {
                bin_cmp_cast!(self, u32, lt);
            }
            opcodes::I32GTS => {
                bin_cmp_cast!(self, i32, gt);
            }
            opcodes::I32GTU => {
                bin_cmp_cast!(self, u32, gt);
            }
            opcodes::I32GES => {
                bin_cmp_cast!(self, i32, ge);
            }
            opcodes::I32GEU => {
                bin_cmp_cast!(self, u32, ge);
            }
            opcodes::I32EQZ => {
                op_u32!(self, is_zero);
            }
            opcodes::I32EQ => {
                bin_cmp_cast!(self, u32, eq);
            }
            opcodes::I32NE => {
                bin_cmp_cast!(self, u32, ne);
            }
            opcodes::I64CLZ => {
                op_u64!(self, leading_zeros);
            }
            opcodes::I64CTZ => {
                op_u64!(self, trailing_zeros);
            }
            opcodes::I64POPCNT => {
                op_u64!(self, count_ones);
            }
            opcodes::I64ADD => {
                unsafe { bin!(self, unchecked_add) };
            }
            opcodes::I64SUB => {
                unsafe { bin!(self, unchecked_sub) };
            }
            opcodes::I64MUL => {
                unsafe { bin!(self, unchecked_mul) };
            }
            opcodes::I64DIVS => {
                bin_cast!(self, i64, div);
            }
            opcodes::I64DIVU => {
                bin!(self, div);
            }
            opcodes::I64REMS => {
                bin_cast!(self, i64, rem);
            }
            opcodes::I64REMU => {
                bin!(self, rem);
            }
            opcodes::I64AND => {
                bin!(self, bitand);
            }
            opcodes::I64OR => {
                bin!(self, bitor);
            }
            opcodes::I64XOR => {
                bin!(self, bitxor);
            }
            opcodes::I64SHL => {
                bin!(self, shl);
            }
            opcodes::I64SHRS => {
                bin_cast_2!(self, i64, u64, shr);
            }
            opcodes::I64SHRU => {
                bin!(self, shr);
            }
            opcodes::I64ROTL => {
                bin_cast_2!(self, u64, u32, rotate_left);
            }
            opcodes::I64ROTR => {
                bin_cast_2!(self, u64, u32, rotate_right);
            }
            opcodes::I64EQ => {
                bin_cmp!(self, eq);
            }
            opcodes::I64EQZ => {
                op_u64!(self, is_zero);
            }
            opcodes::I64NE => {
                bin_cmp!(self, ne);
            }
            opcodes::I64LTS => {
                bin_cmp_cast!(self, i64, lt);
            }
            opcodes::I64LTU => {
                bin_cmp!(self, lt);
            }
            opcodes::I64GTS => {
                bin_cmp_cast!(self, i64, gt);
            }
            opcodes::I64GTU => {
                bin_cmp!(self, gt);
            }
            opcodes::I64LEU => {
                bin_cmp!(self, le);
            }
            opcodes::I64LES => {
                bin_cmp_cast!(self, i64, le);
            }
            opcodes::I64GES => {
                bin_cmp_cast!(self, i64, ge);
            }
            opcodes::I64GEU => {
                bin_cmp!(self, ge);
            }
            opcodes::I32WRAPI64 => {
                let p = self.top()?;
                *p = *p as u32 as u64;
            }
            opcodes::F32ABS => {
                op_f32!(self, abs);
            }
            opcodes::F32NEG => {
                op_f32!(self, neg);
            }
            opcodes::F32CEIL => {
                op_f32!(self, ceil);
            }
            opcodes::F32FLOOR => {
                op_f32!(self, floor);
            }
            opcodes::F32TRUNC => {
                op_f32!(self, trunc);
            }
            opcodes::F32NEAREST => {
                op_f32!(self, nearest);
            }
            opcodes::F32SQRT => {
                op_f32!(self, sqrt);
            }
            opcodes::F32ADD => {
                bin_f32!(self, add);
            }
            opcodes::F32SUB => {
                bin_f32!(self, sub);
            }
            opcodes::F32MUL => {
                bin_f32!(self, mul);
            }
            opcodes::F32DIV => {
                bin_f32!(self, div);
            }
            opcodes::F32MIN => {
                bin_f32!(self, min);
            }
            opcodes::F32MAX => {
                bin_f32!(self, max);
            }
            opcodes::F32COPYSIGN => {
                bin_f32!(self, copysign);
            }
            opcodes::F32EQ => {
                bin_cmp_f32!(self, eq);
            }
            opcodes::F32NE => {
                bin_cmp_f32!(self, ne);
            }
            opcodes::F32LT => {
                bin_cmp_f32!(self, lt);
            }
            opcodes::F32GT => {
                bin_cmp_f32!(self, gt);
            }
            opcodes::F32LE => {
                bin_cmp_f32!(self, le);
            }
            opcodes::F32GE => {
                bin_cmp_f32!(self, ge);
            }
            opcodes::F64ABS => {
                op_f64!(self, abs);
            }
            opcodes::F64NEG => {
                op_f64!(self, neg);
            }
            opcodes::F64CEIL => {
                op_f64!(self, ceil);
            }
            opcodes::F64FLOOR => {
                op_f64!(self, floor);
            }
            opcodes::F64TRUNC => {
                op_f64!(self, trunc);
            }
            opcodes::F64NEAREST => {
                op_f64!(self, nearest);
            }
            opcodes::F64SQRT => {
                op_f64!(self, sqrt);
            }
            opcodes::F64ADD => {
                bin_f64!(self, add);
            }
            opcodes::F64SUB => {
                bin_f64!(self, sub);
            }
            opcodes::F64MUL => {
                bin_f64!(self, mul);
            }
            opcodes::F64DIV => {
                bin_f64!(self, div);
            }
            opcodes::F64MIN => {
                bin_f64!(self, min);
            }
            opcodes::F64MAX => {
                bin_f64!(self, max);
            }
            opcodes::F64COPYSIGN => {
                bin_f64!(self, copysign);
            }
            opcodes::F64EQ => {
                bin_cmp_f64!(self, eq);
            }
            opcodes::F64NE => {
                bin_cmp_f64!(self, ne);
            }
            opcodes::F64LT => {
                bin_cmp_f64!(self, lt);
            }
            opcodes::F64GT => {
                bin_cmp_f64!(self, gt);
            }
            opcodes::F64LE => {
                bin_cmp_f64!(self, le);
            }
            opcodes::F64GE => {
                bin_cmp_f64!(self, ge);
            }
            opcodes::I32TRUNCSF32 => {
                trunc_as!(self, f32, u32, i32, u32);
            }
            opcodes::I32TRUNCSF64 => {
                trunc_as!(self, f64, u64, i32, u32);
            }
            opcodes::I32TRUNCUF32 => {
                trunc_as!(self, f32, u32, u32, u32);
            }
            opcodes::I32TRUNCUF64 => {
                trunc_as!(self, f64, u64, u32, u32);
            }
            opcodes::I64EXTENDSI32 => {
                let t = self.top()?;
                *t = *t as u32 as i32 as i64 as u64;
            }
            opcodes::I64TRUNCSF32 => {
                trunc_as!(self, f32, u32, i64, u64);
            }
            opcodes::I64TRUNCUF32 => {
                trunc_as!(self, f32, u32, u64, u64);
            }
            opcodes::I64TRUNCUF64 => {
                trunc_as!(self, f64, u64, u64, u64);
            }
            opcodes::I64TRUNCSF64 => {
                trunc_as!(self, f64, u64, i64, u64);
            }
            opcodes::F32CONVERTSI32 => {
                let t = self.top()?;
                *t = (*t as u32 as i32 as f32).to_bits() as u64;
            }
            opcodes::F32CONVERTUI32 => {
                let t = self.top()?;
                *t = (*t as u32 as f32).to_bits() as u64;
            }
            opcodes::F32CONVERTSI64 => {
                let t = self.top()?;
                *t = (*t as i64 as f32).to_bits() as u64;
            }
            opcodes::F32CONVERTUI64 => {
                let t = self.top()?;
                *t = (*t as u64 as f32).to_bits() as u64;
            }
            opcodes::F32DEMOTEF64 => {
                let t = self.top()?;
                *t = (f64::from_bits(*t) as f32).to_bits() as u64;
            }
            opcodes::F64CONVERTSI32 => {
                let t = self.top()?;
                *t = ((*t as i32) as f64).to_bits();
            }
            opcodes::F64CONVERTUI32 => {
                let t = self.top()?;
                *t = ((*t as u32) as f64).to_bits();
            }
            opcodes::F64CONVERTSI64 => {
                let t = self.top()?;
                *t = (*t as i64 as f64).to_bits();
            }
            opcodes::F64CONVERTUI64 => {
                let t = self.top()?;
                *t = (*t as u64 as f64).to_bits();
            }
            opcodes::F64PROMOTEF32 => {
                let t = self.top()?;
                *t = (f32::from_bits(*t as u32) as f64).to_bits();
            }
            _ => return Err(StringErr::new(format!("unsupported op {}", ins.op_code())))
        }
        Ok(())
    }
}