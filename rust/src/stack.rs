use alloc::borrow::ToOwned;
use alloc::collections::BTreeMap;
use alloc::string::*;
use alloc::vec::*;
use parity_wasm::elements::FuncBody;
use parity_wasm::elements::FunctionType;
use parity_wasm::elements::Instruction;
use parity_wasm::elements::Local;
use parity_wasm::elements::Module;
use parity_wasm::elements::Type;
use parity_wasm::elements::ValueType;

// label size (2byte) | local size (2byte) | stack size (2byte) | function index (2byte)
#[derive(Clone, Copy)]
struct FrameData(u64);

const LABEL_SIZE_MASK: u64 = 0xffff000000000000;
const LABEL_SIZE_SHIFTS: usize = 48;
const LOCAL_SIZE_MASK: u64 = 0x0000ffff00000000;
const LOCAL_SIZE_SHIFTS: usize = 32;
const STACK_SIZE_MASK: u64 = 0x00000000ffff0000;
const STACK_SIZE_SHIFTS: usize = 16;
const FUNCTION_INDEX_MASK: u64 = 0x000000000000ffff;
const FUNCTION_INDEX_SHIFTS: usize = 0;

const FN_INDEX_MASK: u16 = 0x7fff;
const IS_TABLE_MASK: u16 = 0x8000;

// validate: function size <= FN_INDEX_MASK
impl FrameData {
    fn label_size(&self) -> u16 {
        ((self.0 & LABEL_PC_MASK) >> LABEL_SIZE_SHIFTS) as u16
    }

    fn local_size(&self) -> u16 {
        ((self.0 & LOCAL_SIZE_MASK) >> LOCAL_SIZE_SHIFTS) as u16
    }

    fn stack_size(&self) -> u16 {
        ((self.0 & STACK_SIZE_MASK) >> STACK_SIZE_SHIFTS) as u16
    }

    fn func_index(&self) -> u16 {
        ((self.0 & FUNCTION_INDEX_MASK) >> FUNCTION_INDEX_SHIFTS) as u16
    }

    fn new(label_size: u16, local_size: u16, stack_size: u16, func_index: u16) -> Self {
        let n = ((label_size as u64) << LABEL_SIZE_SHIFTS)
            | ((local_size as u64) << LOCAL_SIZE_SHIFTS)
            | ((stack_size as u64) << STACK_SIZE_SHIFTS)
            | (func_index as u64);
        FrameData(n)
    }
}

// label data = stack pc (2byte) | label pc (2byte) | 0x00  | 0x00  | arity (1byte) | loop (1byte)
#[derive(Clone, Copy)]
struct LabelData(u64);

const STACK_PC_MASK: u64 = 0xffff000000000000;
const STACK_PC_SHIFTS: usize = 48;
const LABEL_PC_MASK: u64 = 0x0000ffff00000000;
const LABEL_PC_SHIFTS: usize = 32;
const ARITY_MASK: u64 = 0x00000000ffff0000;
const ARITY_SHIFTS: usize = 16;

const LOOP_MASK: u64 = 0x000000000000ffff;

impl LabelData {
    fn stack_pc(&self) -> u16 {
        ((self.0 & STACK_PC_MASK) >> STACK_PC_SHIFTS) as u16
    }

    fn label_pc(&self) -> u32 {
        ((self.0 & LABEL_PC_MASK) >> LABEL_PC_SHIFTS) as u32
    }

    fn arity(&self) -> bool {
        (self.0 & ARITY_MASK) != 0
    }

    fn is_loop(&self) -> bool {
        (self.0 & LOOP_MASK) != 0
    }

    fn new(stack_pc: u16, label_pc: u16, arity: bool, is_loop: bool) -> LabelData {
        let o = ((stack_pc as u64) << STACK_PC_SHIFTS)
            | ((label_pc as u64) << LABEL_PC_SHIFTS)
            | ((arity as u64) << ARITY_SHIFTS)
            | (is_loop as u64);
        LabelData(o)
    }
}

const MAX_SIGNED_INT: u64 = 0x7fffffff;
const STACK_BASE_MASK: u64 = 0x7fffffff;
const STACK_BASE_SHIFTS: u32 = 0;
const LABEL_BASE_MASK: u64 = 0x7fffffff00000000;
const LABEL_BASE_SHIFTS: u32 = 32;

#[derive(Clone, Copy)]
struct Offset(u64);

impl Offset {
    fn label_base(&self) -> u32 {
        ((self.0 & LABEL_BASE_MASK) >> LABEL_BASE_SHIFTS) as u32
    }
    fn stack_base(&self) -> u32 {
        ((self.0 & STACK_BASE_MASK) >> STACK_BASE_SHIFTS) as u32
    }
    fn new(label_base: u32, stack_base: u32) -> Self {
        Offset((label_base as u64) << 32 | (stack_base as u64))
    }
}

type Body = Vec<Instruction>;

pub struct WASMFunction {
    fn_type: FunctionType,
    body: Vec<Instruction>,
    locals: Vec<Local>,
}

pub struct Memory{
    data: Vec<u8>,
	initial: u32,
	maximum: Option<u32>,
    max_pages: u32,
    pages: u32,
}

macro_rules! min {
    ($x: expr, $y: expr) => {
       {
           if $x < $y { $x } else { $y }
       } 
    };
}

macro_rules! memory_load_n {
    ($fn: ident, $t: ident, $bytes: expr) => {
        fn $fn(&self, off: u32) -> Result<$t, String> {
            if (off + $bytes) as usize > self.data.len()  {
                return Err("memory access overflow".into())
            }
    
            let mut buf = [0u8; $bytes];
            buf.copy_from_slice(&self.data[off as usize ..(off+$bytes) as usize]);
            Ok($t::from_le_bytes(buf))
        }
    };
}

macro_rules! memory_store_n {
    ($fn: ident, $t: ident, $bytes: expr) => {
        fn $fn(&mut self, off: u32, value: $t) -> Result<(), String> {
            if (off + $bytes) as usize > self.data.len()  {
                return Err("memory access overflow".into())
            }
    
            let buf = value.to_le_bytes();
            self.data[off as usize..(off+$bytes) as usize].copy_from_slice(&buf);
            Ok(())
        }
    };
}

// validate: max_pages should <= 0xFFFF and not be 0 
const MAX_PAGES: u32 = 0xFFFF;
const PAGE_SIZE: u32 = 64 * (1u32 << 10); // 64 KB

impl Memory {
    memory_load_n!(load_u64, u64, 8);
    memory_load_n!(load_u32, u32, 4);
    memory_load_n!(load_u16, u16, 2);
    memory_load_n!(load_u8,  u8,  1);

    memory_store_n!(store_u64, u64, 8);    
    memory_store_n!(store_u32, u32, 4); 
    memory_store_n!(store_u16, u16, 2);     
    memory_store_n!(store_u8,  u8,  1);      
    
    fn read(&self, off: usize, dst: &mut [u8]) -> Result<(), String> {
        if off + dst.len() > self.data.len() {
            return Err("Memory.read(): memory access overflow".into());
        }
        dst.copy_from_slice(&self.data[off..off+dst.len()]);
        Ok(())
    }

    fn write(&mut self, off: usize, dst: &[u8]) -> Result<(), String> {
        if off + dst.len() > self.data.len() {
            return Err("Memory.write(): memory access overflow".into());
        }
        self.data[off..off+dst.len()].copy_from_slice(dst);
        Ok(())
    }    

    fn grow(&mut self, n: u32) -> Result<u32, String>  {
        match self.maximum {
            None => {}
            Some(max) => {
                if self.pages + n > max {
                    return Ok(-1i32 as u32);
                }
            }
        }

        if self.pages + n > self.max_pages {
            return Err(format!("memory overflow: cannot grow any more, current pages = {} n = {} maximum = {}", self.pages, n, self.max_pages));
        }

        let mut v: Vec<u8> = vec![0; ((self.pages + n) * PAGE_SIZE) as usize];
        v[..self.data.len()].copy_from_slice(&self.data);
        self.data = v;
        let prev = self.pages;
        self.pages += n;
        Ok(prev)
    }
}

impl WASMFunction {
    fn new(fn_type: FunctionType, body: FuncBody) -> WASMFunction {
        WASMFunction {
            fn_type: fn_type,
            body: body.code().elements().to_owned(),
            locals: body.locals().to_owned(),
        }
    }
}

pub struct Instance<'a> {
    md: &'a Module,
    // frames counter
    count: u16,

    max_stacks: u32,
    max_frames: u16,
    max_labels: u32,
    max_pages: usize, 

    stack_data: Vec<u64>,
    frame_data: Vec<FrameData>,
    label_data: Vec<LabelData>,
    labels: Vec<&'a [Instruction]>,

    offsets: Vec<Offset>,

    // current frame
    label_size: u16,
    stack_size: u16,
    local_size: u16,
    func_index: u16,
    frame_body: Body,

    stack_base: u32,
    label_base: u32,

    result_type: Option<ValueType>,

    // current label
    label_pc: u16,
    arity: bool,
    is_loop: bool,
    stack_pc: u16,
    label_body: &'a [Instruction],

    functions: Vec<WASMFunction>,
    exports: BTreeMap<String, &'a WASMFunction>,
    types: Vec<FunctionType>,
}

macro_rules! current_frame {
    ($this: ident) => {{
        ($this.count - 1) as usize
    }};
}

impl<'a> Instance<'a> {
    fn init(&mut self) -> Result<(), String> {
        self.types = match self.md.type_section() {
            None => Vec::new(),
            Some(sec) => sec
                .types()
                .to_vec()
                .into_iter()
                .map(|x| match x {
                    Type::Function(y) => y,
                })
                .collect(),
        };

        let codes: Vec<FuncBody> = match self.md.code_section() {
            None => Vec::new() ,
            Some(sec) => {
                sec.bodies().to_vec()
            }
        };

        match self.md.function_section() {
            None => {}
            Some(sec) => {
                if sec.entries().len() > FN_INDEX_MASK as usize {
                    return Err(format!("function section overflow, too much functions {} > {}", sec.entries().len(), FN_INDEX_MASK));
                }
                for f in sec.entries().iter().map(|x| x.type_ref()) {
                    if f as usize > self.types.len() || f as usize > codes.len() {
                        return Err(format!("type entry or code entry not found func entry = {}, type entires = {}, code entries = {}", f, self.types.len(), codes.len()));
                    }

                    let w = WASMFunction::new(self.types[f as usize].clone(), codes[f as usize].clone());
                    self.functions.push(w)
                }
            }
        };

        Ok(())
    }

    fn store_current_frame(&mut self) {
        let data = FrameData::new(
            self.label_size,
            self.local_size,
            self.stack_size,
            self.func_index,
        );
        let off = Offset::new(self.label_base, self.stack_base);
        self.frame_data[current_frame!(self)] = data;
        self.offsets[current_frame!(self)] = off;
    }

    fn store_current_label(&mut self) {
        let p = self.label_base + (self.label_size as u32) - 1;
        self.labels[p as usize] = self.label_body;
        let data = LabelData::new(self.stack_pc, self.label_pc, self.arity, self.is_loop);
        self.label_data[p as usize] = data;
    }

    fn push(&mut self, value: u64) -> Result<(), String> {
        let base = self.stack_base + (self.local_size as u32);
        let index = (base + (self.stack_size as u32)) as usize;
        if index as usize >= self.stack_data.len() {
            return Err("stack overflow".into());
        }
        self.stack_data[index] = value;
        Ok(())
    }

    fn pop(&mut self) -> Result<u64, String> {
        let base = self.stack_base + (self.local_size as u32);
        if self.stack_size == 0 {
            return Err("stack underflow".into());
        }

        let v = self.stack_data[(base + (self.stack_size as u32) - 1) as usize];
        self.stack_size -= 1;
        Ok(v)
    }

    #[inline]
    fn pop_i32(&mut self) -> Result<i32, String> {
        Ok(self.pop()? as i32)
    }

    #[inline]
    fn clear_frame_data(&mut self) {
        self.label_size = 0;
        self.stack_size = 0;
        self.func_index = 0;
        self.local_size = 0;
    }

    fn push_expr(&mut self, expr_body: Body, result_type: Option<ValueType>) -> Result<(), String> {
        if self.count == self.max_frames {
            return Err("frame overflow".into());
        }

        if self.count != 0 {
            self.store_current_frame();
        }

        if self.label_size != 0 {
            self.store_current_label();
        }

        let mut new_stack_base = 0;
        let mut new_label_base = 0;

        if self.count != 0 {
            new_stack_base = self.stack_base + (self.local_size as u32) + (self.stack_size as u32);
            new_label_base = self.label_base + (self.label_size as u32);
        }

        self.clear_frame_data();
        self.stack_base = new_stack_base;
        self.label_base = new_label_base;
        self.result_type = result_type;
        self.frame_body = expr_body;
        self.count += 1;
        Ok(())
    }

    fn set_local(&mut self, index: u32, value: u64) -> Result<(), String> {
        if index >= self.local_size as u32 {
            return Err("local variable access overflow".into());
        }

        self.stack_data[(self.stack_base + index) as usize] = value;
        Ok(())
    }

    fn get_local(&mut self, index: u32) -> Result<u64, String> {
        if index >= self.local_size as u32 {
            return Err("local variable access overflow".into());
        }

        Ok(self.stack_data[(self.stack_base + index) as usize])
    }

    fn pop_frame(&mut self) {
        self.count -= 1;

        if self.count == 0 {
            return;
        }

        let prev = self.frame_data[current_frame!(self)];
        self.stack_size = prev.stack_size();
        self.local_size = prev.local_size();
        self.label_size = prev.label_size();
        self.func_index = prev.func_index();

        let prev_off = self.offsets[current_frame!(self)];
        self.stack_base = prev_off.stack_base();
        self.label_base = prev_off.label_base();
    }

    fn get_by_func_index(&mut self, func_index: u16) {
        let is_table = (func_index & IS_TABLE_MASK) != 0;
        let index = (func_index & FN_INDEX_MASK) as usize;
    }
}
