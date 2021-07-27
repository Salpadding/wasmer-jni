use std::fs::OpenOptions;
use std::io::{Cursor, Seek, SeekFrom};

use parity_wasm::elements::{BlockType, Instruction, opcodes, ValueType, VarUint32, Deserialize, VarInt32, VarUint64, VarInt64, Uint32, Uint64};

use crate::StringErr;
use std::collections::BTreeSet;

macro_rules! trait_var {
    ($t0: ident, $name: ident, $t1: ident) => {
        fn $name(&mut self) -> Result<$t0, StringErr>;
    };
}

macro_rules! impl_var {
    ($t0: ident, $name: ident, $t1: ident) => {
        fn $name(&mut self) -> Result<$t0, StringErr> {
            let v = $t1::deserialize(self)?;
            Ok(v.into())
        }
    };
}

trait ModuleCursor<T> {
    fn peek(&self) -> Result<T, StringErr>;

    fn next(&mut self) ->  Result<T, StringErr>;

    trait_var!(u32, var_u32, VarUint32);
    trait_var!(i32, var_i32, VarInt32);
    trait_var!(u64, var_u64, VarUint64);
    trait_var!(i64, var_i64, VarInt64);
    trait_var!(u32, u32, Uint32);
    trait_var!(u64, u64, Uint64);
}

pub const NULL: u64 = 0;



impl <T: AsRef<[u8]>> ModuleCursor<u8> for Cursor<T>  {
    fn peek(&self) -> Result<u8, StringErr> {
        let cur = self.position();
        let r = self.get_ref().as_ref();
        r.get(cur as usize).cloned().ok_or(StringErr::new("unexpected eof"))
    }

    fn next(&mut self) -> Result<u8, StringErr> {
        let cur = self.position();
        let r = self.get_ref().as_ref();
        let r = r.get(cur as usize).cloned();
        if r.is_some() {
            self.seek(SeekFrom::Current(1)).unwrap();
        }
        r.ok_or(StringErr::new("unexpected eof"))
    }

    impl_var!(u32, var_u32, VarUint32);
    impl_var!(i32, var_i32, VarInt32);
    impl_var!(u64, var_u64, VarUint64);
    impl_var!(i64, var_i64, VarInt64);
    impl_var!(u32, u32, Uint32);
    impl_var!(u64, u64, Uint64);
}

// allocation for runtime instruction
// operand (4byte) or zero | 0x00 | 0x01 | 0x00 | opcode for instruction with <= 1(i32, f32) operand, no branch
// operand position (4byte) | 0x00 | 0x01 | 0x00 | opcode for instruction with = 1(i64, f64) operand, no branch
// memory position | operand size (2byte) | result type (1byte) | opcode for instruction with branch and var length operands

// allocation for instruction branch and operands
// 8 byte  operands offset | branch1 size (2byte) | branch0 size (2byte)
// 8 byte  branch1 offset (4byte) | branch0 offset (4byte)
#[derive(Copy, Clone)]
pub(crate) struct InsBits(u64);

#[derive(Copy, Clone)]
pub(crate) struct InsVec(u64);

impl InsVec {
    pub(crate) fn is_null(&self) -> bool {
        self.0 == NULL
    }

    pub(crate) fn size(&self) -> usize {
        ((self.0 >> 32) & 0xffffffffu64) as usize
    }

    pub(crate) fn offset(&self) -> usize {
        (self.0 & 0xffffffffu64) as usize
    }

    pub(crate) fn null() -> InsVec {
        InsVec(NULL)
    }

    pub(crate) fn new(offset: u32, size: u32) -> InsVec {
        InsVec(((size as u64) << 32) | (offset as u64))
    }
}

impl InsBits {
    pub(crate) fn new(op: u8) -> Self {
        InsBits(op as u64)
    }

    pub(crate) fn op_code(&self) -> u8 {
        (self.0 & 0xff) as u8
    }

    // with opcode and null result type
    pub(crate) fn no_result(op: u8) -> InsBits {
        InsBits(op as u64 | 0xff00)
    }


    pub(crate) fn operand_size(&self) -> u16 {
        ((self.0 & 0xFFFF0000u64) >> 16) as u16
    }

    pub(crate) fn add_operand_size(&self, size: u16) -> Self {
        let bits = (self.0 & !(0xFFFF0000u64)) | ((size as u64) << 16);
        InsBits(bits)
    }

    // 1. operand
    // 2. offset of branch 0 or branch 1

    pub(crate) fn payload(&self) -> u32 {
        ((self.0 & 0xFFFFFFFF00000000) >> 32) as u32
    }

    pub(crate) fn add_payload(&self, operand: u32) -> Self {
        let bits = ((operand as u64 & 0xFFFFFFFFu64) << 32) | (self.0 & 0xFFFFFFFFu64);
        InsBits(bits)
    }

    pub(crate) fn block_type(&self) -> Option<ValueType> {
        let rt = ((self.0 & 0xFF00u64) >> 8);
        if (rt & 0x80) != 0 {
            return None;
        }
        match rt {
            0 => Some(ValueType::I32),
            1 => Some(ValueType::I64),
            2 => Some(ValueType::F32),
            3 => Some(ValueType::F64),
            _ => panic!("unexpected block type")
        }
    }


    pub(crate) fn add_block_type(&self, t: BlockType) -> Self {
        let r = match t {
            BlockType::NoResult => {
                let bits = (self.0 & !(0xff00u64)) | 0x8000u64;
                return InsBits(bits);
            }
            BlockType::Value(ty) => {
                match ty {
                    ValueType::I32 => 0,
                    ValueType::I64 => 1,
                    ValueType::F32 => 2,
                    ValueType::F64 => 3,
                    _ => panic!("simd not supported")
                }
            }
        };

        let bits = (self.0 & !(0xff00u64)) | (r << 8);
        return InsBits(bits);
    }
}


pub struct InsPool {
    data: Vec<u64>,
}

impl InsPool {
    fn new() -> Self {
        // push a null pointer
        let mut r = InsPool { data: Vec::new() };
        r.data.push(NULL);
        r
    }
}


impl InsPool {
    // store an instruction
    fn alloc_ins(&mut self, ins: InsBits) -> usize {
        let r = self.data.len();
        self.data.push(ins.0);
        r
    }

    // create a linked list by insert the first element
    fn alloc_linked(&mut self, value: u32) -> usize {
        let r = self.data.len();
        self.data.push(value as u64);
        r
    }

    fn add_body_off(&self, ins: InsBits) -> InsBits {
        let sz = self.data.len();
        let r = ins.add_payload(sz as u32);
        r
    }

    fn push_linked(&mut self, prev: u32, value: u32) -> usize {
        let r = self.data.len();
        self.data.push(value as u64);
        self.data[prev as usize] = self.data[prev as usize] | ((r as u64) << 32);
        r
    }

    fn span_linked(&mut self, head: u32) -> Result<InsVec, StringErr> {
        let mut cnt: u32 = 0;
        let mut cur = self.data[head as usize];
        let start = self.data.len();

        loop {
            let next = (cur & 0x7fffffff00000000u64) >> 32;
            let val = cur & 0x7fffffff;
            self.data.push(self.data[val as usize]);
            cnt += 1;

            if next == 0 {
                break;
            }

            cur = self.data[next as usize];
        }

        if cnt > u16::MAX as u32{
            Err(StringErr::new("expression size overflow"))
        } else {
            Ok(InsVec::new(start as u32, cnt))
        }
    }
    
    fn operand(&self, ins: InsBits, i: usize) -> u64 {
        let off = ins.payload() as usize;
        self.data[off + i]
    }

    pub(crate) fn read_expr(&mut self, cursor: &mut Cursor<Vec<u8>>) -> Result<InsVec, StringErr> {
        let r = self.read_util(cursor, &[opcodes::END])?;
        cursor.next()?;
        Ok(r)
    }

    fn read_util(&mut self, cursor: &mut Cursor<Vec<u8>>, end: &'static [u8]) -> Result<InsVec, StringErr> {
        let mut cnt = -1;
        // head is a pointer
        let mut head: u32 = 0;
        let mut cur = 0;

        loop {
            cnt += 1;
            let ins = cursor.peek()?;
            if end.iter().any(|x| *x == ins) {
                if head == 0 {
                    return Ok(InsVec::null())
                }
                return self.span_linked(head);
            }

            let read = self.read_ins(cursor)?;
            match head {
                0 => {
                    let ptr = self.alloc_ins(read) as u32;
                    head = self.alloc_linked(ptr) as u32;
                    if head == 0 {
                        println!("alloc lined = 0")
                    }
                    cur = head;
                }
                _ => {
                    let ptr = self.alloc_ins(read) as u32;
                    cur = self.push_linked(cur, ptr) as u32;
                }
            }
        }
    }

    fn read_ins(&mut self, cursor: &mut Cursor<Vec<u8>>) -> Result<InsBits, StringErr> {
        let op = cursor.peek()?;

        match op {
            // control instructions
            opcodes::UNREACHABLE..=opcodes::CALLINDIRECT => self.read_ctl(cursor),
            // parametric
            opcodes::DROP..=opcodes::SELECT => Ok(InsBits::no_result(cursor.next()?)),
            opcodes::GETLOCAL..=opcodes::SETGLOBAL => self.read_var_ins(cursor),
            opcodes::I32LOAD..=opcodes::GROWMEMORY => self.read_mem_ins(cursor),
            _ => self.read_num_ins(cursor)
        }
    }

    fn push_labels(&mut self, cursor: &mut Cursor<Vec<u8>>) -> Result<u32, StringErr> {
        let len: u32 = cursor.var_u32()?;

        for i in 0..len {
            let lb: u32 = cursor.var_u32()?;
            self.data.push(lb as u64);
        }
        Ok(len)
    }



    fn read_ctl(&mut self, cursor: &mut Cursor<Vec<u8>>) -> Result<InsBits, StringErr> {
        let op: u8 = cursor.next()?;
        let mut bits = InsBits::no_result(op);
        return match op {
            opcodes::UNREACHABLE | opcodes::NOP | opcodes::RETURN => {
                Ok(bits)
            }
            opcodes::BR | opcodes::BRIF | opcodes::CALL => {
                let n: u32 = cursor.var_u32()?;
                bits = bits.add_payload(n).add_operand_size(1);
                Ok(bits)
            }

            opcodes::BLOCK | opcodes::LOOP | opcodes::IF => {
                let t: BlockType = BlockType::deserialize(cursor)?;
                bits = bits.add_block_type(t);
                let branch_0 = self.read_util(cursor, &[opcodes::END, opcodes::ELSE])?;
                let mut branch_1 = InsVec::null();

                if cursor.peek()? == opcodes::ELSE {
                    // skip 0x05
                    cursor.next()?;
                    branch_1 = self.read_util(cursor, &[opcodes::END])?;
                }

                // skip 0x05
                cursor.next()?;
                bits = self.add_body_off(bits);
                self.data.push(branch_0.0);
                self.data.push(branch_1.0);
                Ok(bits)
            }
            opcodes::BRTABLE => {
                bits = self.add_body_off(bits);
                let operand_size = self.push_labels(cursor)?;
                self.data.push( cursor.var_u32()? as u64);
                bits = bits.add_operand_size((operand_size + 1) as u16);
                Ok(bits)
            }
            opcodes::CALLINDIRECT => {
                let t = cursor.var_u32()?;
                if cursor.next()? != 0 {
                    Err(StringErr::new("invalid operand of call indirect"))
                } else {
                    bits = bits.add_operand_size(1).add_payload(t);
                    Ok(bits)
                }
            }
            _ => {
                Err(StringErr::new(format!("unexpected op {}", op)))
            }
        }
    }

    fn read_var_ins(&self, cur: &mut Cursor<Vec<u8>>) -> Result<InsBits, StringErr> {
        let op = cur.next()?;
        let bits = InsBits::no_result(op).add_payload(cur.var_u32()?);
        Ok(bits)
    }

    fn read_mem_ins(&self, cur: &mut Cursor<Vec<u8>>) -> Result<InsBits, StringErr> {
        let op = cur.next()?;
        let bits = InsBits::no_result(op);

        if op == opcodes::CURRENTMEMORY || op == opcodes::GROWMEMORY {
            if cur.next()? != 0 {
                return Err(StringErr::new("invalid terminator"));
            }
            return Ok(bits);
        }

        let align = cur.var_u32()?;
        Ok(
            bits.add_payload(cur.var_u32()?).add_operand_size(1)
        )
    }

    fn ins_in_vec(&self, vec: InsVec, i: usize) -> InsBits {
        InsBits(self.data[vec.offset() + i])
    }

    fn branch0(&self, ins: InsBits) -> InsVec {
        let off = ins.payload();
        InsVec(self.data[off])
    }

    fn branch1(&self, ins: InsBits) -> InsVec {
        let off = ins.payload();
        InsVec(self.data[off + 1])
    }

    fn read_num_ins(&mut self, cur: &mut Cursor<Vec<u8>>) -> Result<InsBits, StringErr> {
        let op = cur.next()?;
        let bits = InsBits::no_result(op);
        let bits =
        match op {
            opcodes::I32CONST => bits.add_payload(cur.var_i32()? as u32).add_operand_size(1),
            opcodes::I64CONST => {
                let r = self.add_body_off(bits).add_operand_size(1);
                self.data.push(cur.var_i64()? as u64);
                r
            }
            opcodes::F32CONST => {
                bits.add_payload(cur.u32()?).add_operand_size(1)
            }
            opcodes::F64CONST => {
                let r = self.add_body_off(bits).add_operand_size(1);
                self.data.push(cur.u64()?);
                r
            }
            _ => bits
        };
        Ok(bits)
    }
}

#[cfg(test)]
mod test{
    use crate::types::ins_pool::InsPool;
    use std::fs::File;
    use std::fs;
    use std::io::{Read, Cursor};
    use parity_wasm::elements::{Deserialize, Serialize};
    use crate::StringErr;


    fn test() -> Result<(), StringErr>{
        let mut pool = InsPool::new();
        let filename = "src/testdata/main.wasm";
        let mut f = File::open(filename).expect("no file found");
        let m = parity_wasm::elements::Module::deserialize(&mut f)?;

        let code_sec = m.code_section().unwrap();

        for bd in code_sec.bodies() {
            let mut serialized: Vec<u8> = Vec::new();
            bd.code().to_owned().serialize(&mut serialized)?;
            let mut cur = Cursor::new(serialized);
            pool.read_expr(&mut cur)?;
        }

        Ok(())
    }

    #[test]
    fn test_1() {
        test().unwrap()
    }
}