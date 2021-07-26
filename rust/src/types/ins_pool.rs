use std::fs::OpenOptions;
use std::io::{Cursor, Seek, SeekFrom};

use parity_wasm::elements::{BlockType, Instruction, opcodes, ValueType, VarUint32, Deserialize};

use crate::StringErr;
use std::collections::BTreeSet;

trait Peekable<T> {
    fn peek(&self) -> Option<T>;

    fn next(&mut self) -> Option<T>;
}

pub const NULL: u64 = 0xFFFFFFFFFFFFFFFFu64;

impl <T: AsRef<[u8]>> Peekable<u8> for Cursor<T>  {
    fn peek(&self) -> Option<u8> {
        let cur = self.position();
        let r = self.get_ref().as_ref();
        r.get(cur as usize).cloned()
    }

    fn next(&mut self) -> Option<u8> {
        let cur = self.position();
        let r = self.get_ref().as_ref();
        let r = r.get(cur as usize).cloned();
        if r.is_some() {
            self.seek(SeekFrom::Current(1)).unwrap();
        }
        r
    }
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

    pub(crate) fn with_operand_size(&self, size: u16) -> Self {
        let bits = (self.0 & !(0xFFFF0000u64)) | ((size as u64) << 16);
        InsBits(bits)
    }

    pub(crate) fn operand(&self) -> u32 {
        ((self.0 & 0xFFFFFFFF00000000) >> 32) as u32
    }

    pub(crate) fn with_operand(&self, operand: u32) -> Self {
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
            _ => panic!("unreachable")
        }
    }


    pub(crate) fn with_block_type(&self, t: BlockType) -> Self {
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
    pub(crate) fn push(&mut self, ops: &[Instruction]) -> InsVec {
        InsVec(0)
    }

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

    fn push_with_body_off(&mut self, ins: InsBits) -> InsBits {
        let sz = self.data.len();
        let r = ins.with_operand(sz as u32 + 1);
        self.data.push(r.0);
        r
    }

    fn push_linked(&mut self, prev: u32, value: u32) -> usize {
        let r = self.data.len();
        self.data.push(value as u64);
        self.data[prev as usize] = self.data[prev as usize] | ((r as u64) << 32);
        r
    }

    fn span_linked(&mut self, head: u32) -> InsVec {
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
        InsVec(((cnt as u64) << 32) | (start as u64))
    }

    fn read_util<'b>(&mut self, cursor: &'b mut Cursor<Vec<u8>>, end: &[u8]) -> Result<InsVec, StringErr> {
        let mut s: BTreeSet<u8> = BTreeSet::new();
        for x in end {
            s.insert(*x);
        }

        Err(StringErr::new("TODO"))
    }

    fn push_labels(&mut self, cursor: &mut Cursor<Vec<u8>>) -> Result<u32, StringErr> {
        let len: u32 = VarUint32::deserialize(cursor)?.into();

        for i in 0..len {
            let lb: u32 = VarUint32::deserialize(cursor)?.into();
            self.data.push(lb as u64);
        }
        Ok(len)
    }


    fn push_ctl(&mut self, cursor: &mut Cursor<Vec<u8>>) -> Result<InsBits, StringErr> {
        let op: u8 = ok_or_err!(cursor.next(), "unexpected eof");
        let mut bits = InsBits::no_result(op);
        match op {
            opcodes::UNREACHABLE | opcodes::NOP | opcodes::RETURN => {
                return Ok(bits);
            }
            opcodes::BR | opcodes::BRIF | opcodes::CALL => {
                let n: u32 = VarUint32::deserialize(cursor)?.into();
                bits = bits.with_operand(n).with_operand_size(1);
                return Ok(bits);
            }

            opcodes::BLOCK | opcodes::LOOP | opcodes::IF => {
                let t: BlockType = BlockType::deserialize(cursor)?;
                bits = bits.with_block_type(t);
                let branch_0 = self.read_util(cursor, &[opcodes::END, opcodes::ELSE])?;
                let mut branch_1 = InsVec(NULL);

                if ok_or_err!(cursor.peek(), "unreachable") == opcodes::ELSE {
                    // skip 0x05
                    ok_or_err!(cursor.next(), "unreachable");
                    branch_1 = self.read_util(cursor, &[opcodes::END, opcodes::ELSE])?;
                }

                // skip 0x05
                ok_or_err!(cursor.next(), "unreachable");
                bits = self.push_with_body_off(bits);
                self.data.push(branch_0.0);
                self.data.push(branch_1.0);
                return Ok(bits);
            }
            opcodes::BRTABLE => {
                let ptr = self.data.len();
                bits = self.push_with_body_off(bits);
                let operand_size = self.push_labels(cursor)?;
                self.data.push({
                    let x: u32 = VarUint32::deserialize(cursor)?.into();
                    x as u64
                });
                bits = bits.with_operand_size((operand_size + 1) as u16);
                self.data[ptr] = bits.0;
                return Ok(bits)
            }
            _ => {
                return Err(StringErr::new("unreachable"))
            }
        }
    }
}