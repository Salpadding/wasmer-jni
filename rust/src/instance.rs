use alloc::borrow::ToOwned;
use alloc::collections::BTreeMap;
use alloc::rc::Rc;
use alloc::string::*;
use alloc::vec::*;

use parity_wasm::elements::BlockType;
use parity_wasm::elements::{
    External, FuncBody, FunctionType, GlobalType, Instruction, Local, Module, Type, ValueType,
};

// label size (2byte) | local size (2byte) | stack size (2byte) | function index (2byte)
#[derive(Clone, Copy)]
struct FrameData(u64);

const LABEL_SIZE_MASK: u64 = 0xffff000000000000;
const LABEL_SIZE_SHIFTS: usize = 48;
const LOCAL_SIZE_MASK: u64 = 0x0000ffff00000000;
const LOCAL_SIZE_SHIFTS: usize = 32;
const STACK_SIZE_MASK: u64 = 0x00000000ffff0000;
const STACK_SIZE_SHIFTS: usize = 16;
const FUNCTION_BITS_MASK: u64 = 0x000000000000ffff;
const FUNCTION_BITS_SHIFTS: usize = 0;

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

    fn func_bits(&self) -> u16 {
        ((self.0 & FUNCTION_BITS_MASK) >> FUNCTION_BITS_SHIFTS) as u16
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

    fn label_pc(&self) -> u16 {
        ((self.0 & LABEL_PC_MASK) >> LABEL_PC_SHIFTS) as u16
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

pub struct WASMFunction {
    fn_type: FunctionType,
    body: Rc<Vec<Instruction>>,
    locals: Vec<Local>,
}

impl WASMFunction {
    fn body(&self) -> Rc<Vec<Instruction>> {
        self.body.clone()
    }
}

#[derive(Default)]
pub struct Memory {
    data: Vec<u8>,
    initial: u32,
    maximum: Option<u32>,
    max_pages: u32,
    pages: u32,
}

macro_rules! min {
    ($x: expr, $y: expr) => {{
        if $x < $y {
            $x
        } else {
            $y
        }
    }};
}

macro_rules! memory_load_n {
    ($fn: ident, $t: ident, $bytes: expr) => {
        fn $fn(&self, off: u32) -> Result<$t, String> {
            if (off + $bytes) as usize > self.data.len() {
                return Err("memory access overflow".into());
            }

            let mut buf = [0u8; $bytes];
            buf.copy_from_slice(&self.data[off as usize..(off + $bytes) as usize]);
            Ok($t::from_le_bytes(buf))
        }
    };
}

macro_rules! memory_store_n {
    ($fn: ident, $t: ident, $bytes: expr) => {
        fn $fn(&mut self, off: u32, value: $t) -> Result<(), String> {
            if (off + $bytes) as usize > self.data.len() {
                return Err("memory access overflow".into());
            }

            let buf = value.to_le_bytes();
            self.data[off as usize..(off + $bytes) as usize].copy_from_slice(&buf);
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
    memory_load_n!(load_u8, u8, 1);

    memory_store_n!(store_u64, u64, 8);
    memory_store_n!(store_u32, u32, 4);
    memory_store_n!(store_u16, u16, 2);
    memory_store_n!(store_u8, u8, 1);

    fn read(&self, off: usize, dst: &mut [u8]) -> Result<(), String> {
        if off + dst.len() > self.data.len() {
            return Err("Memory.read(): memory access overflow".into());
        }
        dst.copy_from_slice(&self.data[off..off + dst.len()]);
        Ok(())
    }

    fn write(&mut self, off: usize, dst: &[u8]) -> Result<(), String> {
        if off + dst.len() > self.data.len() {
            return Err("Memory.write(): memory access overflow".into());
        }
        self.data[off..off + dst.len()].copy_from_slice(dst);
        Ok(())
    }

    fn grow(&mut self, n: u32) -> Result<u32, String> {
        match self.maximum {
            None => {}
            Some(max) => {
                if self.pages + n > max {
                    return Ok(-1i32 as u32);
                }
            }
        }

        if self.pages + n > self.max_pages {
            return Err(format!(
                "memory overflow: cannot grow any more, current pages = {} n = {} maximum = {}",
                self.pages, n, self.max_pages
            ));
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
            body: Rc::new(body.code().elements().to_owned()),
            locals: body.locals().to_owned(),
        }
    }
}

pub struct HostFunction {
    module: String,
    field: String,
    fn_type: FunctionType,
}

enum FunctionInstance {
    HostFunction(HostFunction),
    WasmFunction(Rc<WASMFunction>),
}

#[derive(Default)]
pub struct Instance {
    // frames counter
    count: u16,

    // limitation
    max_stacks: u32,
    max_frames: u16,
    max_labels: u32,
    max_pages: usize,

    // bitmaps
    stack_data: Vec<u64>,
    frame_data: Vec<FrameData>,
    label_data: Vec<LabelData>,
    labels: Vec<Rc<Vec<Instruction>>>,
    offsets: Vec<Offset>,

    // memory
    memory: Memory,

    // current frame
    label_size: u16,
    stack_size: u16,
    local_size: u16,
    func_index: u16,
    frame_body: Rc<Vec<Instruction>>,

    stack_base: u32,
    label_base: u32,

    result_type: Option<ValueType>,

    // current label
    label_pc: u16,
    arity: bool,
    is_loop: bool,
    stack_pc: u16,
    label_body: Rc<Vec<Instruction>>,

    // static region
    functions: Vec<FunctionInstance>,
    exports: BTreeMap<String, Rc<WASMFunction>>,
    types: Vec<FunctionType>,
    table: Vec<Option<FunctionInstance>>,

    // runtime
    globals: Vec<u64>,
    global_types: Vec<GlobalType>,

    // expr
    expr: Rc<Vec<Instruction>>,
}

macro_rules! current_frame {
    ($this: ident) => {{
        ($this.count - 1) as usize
    }};
}

pub struct StringErr(pub String);

impl From<StringErr> for String {
    fn from(e: StringErr) -> Self {
        e.0
    }
}

impl From<parity_wasm::elements::Error> for StringErr {
    fn from(e: parity_wasm::elements::Error) -> Self {
        StringErr(format!("{:?}", e))
    }
}

macro_rules! mem_off {
    ($this: ident, $var: expr) => {{
        let l = $this.pop()? + $var as u64;

        if l > i32::MAX as u64 {
            return Err("memory access overflow".into());
        }

        l as u32
    }};
}

macro_rules! get_or_err {
    ($v: expr, $id: expr, $msg: expr) => {
        $v.get($id).ok_or::<String>($msg.into())?
    };
}

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

impl Instance {
    fn new(bin: &[u8]) -> Result<Instance, String> {
        let mut r = Instance::default();
        let md: Result<Module, StringErr> = Module::from_bytes(bin).map_err(|x| x.into());
        r.init(md?)?;
        Ok(r)
    }

    fn init(&mut self, md: Module) -> Result<(), String> {
        // save type section
        self.types = match md.type_section() {
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

        let codes: Vec<FuncBody> = match md.code_section() {
            None => Vec::new(),
            Some(sec) => sec.bodies().to_vec(),
        };

        match md.import_section() {
            None => {}
            Some(sec) => {
                for imp in sec.entries().iter() {
                    match imp.external() {
                        External::Function(i) => {
                            self.functions
                                .push(FunctionInstance::HostFunction(HostFunction {
                                    module: imp.module().into(),
                                    field: imp.field().into(),
                                    fn_type: get_or_err!(
                                        self.types,
                                        *i as usize,
                                        "function not found"
                                    )
                                    .clone(),
                                }))
                        }
                        _ => {
                            return Err(format!("unsupported import type {:?}", imp.external()));
                        }
                    }
                }
            }
        };

        match md.global_section() {
            None => {}
            Some(sec) => {
                self.globals = vec![0u64; sec.entries().len()];
                self.global_types = sec
                    .entries()
                    .iter()
                    .map(|x| x.global_type().clone())
                    .collect();

                for i in 0..sec.entries().len() {
                    let g = &sec.entries()[i];
                    self.expr = Rc::new(g.init_expr().code().to_vec());
                    self.globals[i] = self.execute_expr(g.global_type().content_type())?;
                }
            }
        }

        match md.function_section() {
            None => {}
            Some(sec) => {
                if sec.entries().len() > FN_INDEX_MASK as usize {
                    return Err(format!(
                        "function section overflow, too much functions {} > {}",
                        sec.entries().len(),
                        FN_INDEX_MASK
                    ));
                }
                for f in sec.entries().iter().map(|x| x.type_ref()) {
                    if f as usize > self.types.len() || f as usize > codes.len() {
                        return Err(format!("type entry or code entry not found func entry = {}, type entires = {}, code entries = {}", f, self.types.len(), codes.len()));
                    }

                    let w = WASMFunction::new(
                        self.types[f as usize].clone(),
                        codes[f as usize].clone(),
                    );
                    self.functions
                        .push(FunctionInstance::WasmFunction(Rc::new(w)))
                }
            }
        };

        Ok(())
    }

    fn execute(&mut self) -> Result<u64, String> {
        self.push_label(self.result_type.is_some(), self.frame_body.clone(), false)?;

        while self.label_size != 0 {
            if self.label_pc as usize >= self.label_body.len() {
                self.pop_label()?;
                continue;
            }

            let cloned = self.label_body.clone();
            let ins = &cloned[self.label_pc as usize];

            match ins {
                Instruction::Return => {
                    return self.ret();
                }
                Instruction::Nop
                | Instruction::I32ReinterpretF32
                | Instruction::I64ReinterpretF64
                | Instruction::I64ExtendUI32 => {}
                Instruction::Block(t) => {
                    self.push_label(t.to_value_type().is_some(), Rc::new(Vec::new()), false)?;
                }
                Instruction::Loop(_) => {
                    self.push_label(false, Rc::new(Vec::new()), true)?;
                }
                Instruction::If(t) => {
                    let arity = t.to_value_type().is_some();
                    let c = self.pop()?;

                    if c != 0 {
                        self.push_label(arity, Rc::new(Vec::new()), false)?;
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
                    let tb = &data.table;
                    let sz = data.table.len();
                    let i = self.pop()? as u32;

                    if sz == 0 {
                        return Err("invalid empty br table data".into());
                    }
                    let n = if (i as usize) < sz - 1 {
                        tb[i as usize]
                    } else {
                        tb[sz - 1]
                    };
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
                                return Err(format!("call failed, invalid function index {}", n))
                            }
                            Some(fun) => match fun {
                                FunctionInstance::HostFunction(_) => {
                                    return Err("host function is not supported yet".into());
                                }
                                FunctionInstance::WasmFunction(w) => w.clone(),
                            },
                        }
                    };

                    self.push_frame(f.clone(), None)?;
                    let res = self.execute()?;

                    if !&f.fn_type.results().is_empty() {
                        self.push(res)?;
                    }
                }
                Instruction::CallIndirect(n, m) => {
                    let index = self.pop()? as usize;

                    let f = {
                        match &self.table[index] {
                            None => return Err("function not found in table".into()),
                            Some(f) => match f {
                                FunctionInstance::HostFunction(_) => {
                                    return Err("host function is not supported".into())
                                }
                                FunctionInstance::WasmFunction(w) => w.clone(),
                            },
                        }
                    };

                    self.push_frame(f.clone(), None)?;
                    let r = self.execute()?;

                    if !f.fn_type.results().is_empty() {
                        self.push(r)?
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
                    let v = self.pop()?;
                    self.set_local(*n, v)?
                }
                Instruction::TeeLocal(n) => {
                    let v = self.pop()?;
                    self.push(v)?;
                    self.push(v)?;
                    let v1 = self.pop()?;
                    self.set_local(*n, v1)?;
                }
                Instruction::GetGlobal(n) => {
                    let v = get_or_err!(self.globals, *n as usize, "access global overflow");
                    self.push(*v)?;
                }
                Instruction::SetGlobal(n) => {
                    let t = get_or_err!(self.global_types, *n as usize, "access global overflow");
                    if t.is_mutable() {
                        return Err("modify global failed: immutable".into());
                    }
                    let v = self.pop()?;
                    self.globals[*n as usize] = v;
                }
                Instruction::I32Load(x, _) => {
                    let off = mem_off!(self, *x);
                    self.push(self.memory.load_u32(off)? as u64)?;
                }
                Instruction::I64Load32U(x, _) => {
                    let off = mem_off!(self, *x);
                    self.push(self.memory.load_u32(off)? as u64)?;
                }

                Instruction::I64Load(x, _) => {
                    let off = mem_off!(self, *x);
                    self.push(self.memory.load_u64(off)?)?;
                }
                Instruction::I32Load8S(x, _) => {
                    let off = mem_off!(self, *x);
                    self.push(self.memory.load_u8(off)? as i8 as i32 as u32 as u64)?;
                }
                Instruction::I64Load8S(x, _) => {
                    let off = mem_off!(self, *x);
                    self.push(self.memory.load_u8(off)? as i8 as i64 as u64)?;
                }
                Instruction::I32Load8U(x, _) => {
                    let off = mem_off!(self, *x);
                    self.push(self.memory.load_u8(off)? as u64)?;
                }
                Instruction::I64Load8U(x, _) => {
                    let off = mem_off!(self, *x);
                    self.push(self.memory.load_u8(off)? as u64)?;
                }
                Instruction::I32Load16S(x, _) => {
                    let off = mem_off!(self, *x);
                    self.push(self.memory.load_u16(off)? as i16 as i32 as u32 as u64)?;
                }
                Instruction::I64Load16S(x, _) => {
                    let off = mem_off!(self, *x);
                    self.push(self.memory.load_u16(off)? as i16 as i64 as u64)?;
                }
                Instruction::I32Load16U(x, _) => {
                    let off = mem_off!(self, *x);
                    self.push(self.memory.load_u16(off)? as u64)?;
                }
                Instruction::I64Load16U(x, _) => {
                    let off = mem_off!(self, *x);
                    self.push(self.memory.load_u16(off)? as u64)?;
                }
                Instruction::I64Load32S(x, _) => {
                    let off = mem_off!(self, *x);
                    self.push(self.memory.load_u32(off)? as i32 as i64 as u64)?;
                }
                Instruction::I32Store8(x, _) => {
                    let off = mem_off!(self, *x);
                    let v = self.pop()? as u8;
                    self.memory.store_u8(off, v)?;
                }
                Instruction::I64Store8(x, _) => {
                    let off = mem_off!(self, *x);
                    let v = self.pop()? as u8;
                    self.memory.store_u8(off, v)?;
                }
                Instruction::I32Store16(x, _) => {
                    let off = mem_off!(self, *x);
                    let v = self.pop()? as u16;
                    self.memory.store_u16(off, v)?;
                }
                Instruction::I64Store16(x, _) => {
                    let off = mem_off!(self, *x);
                    let v = self.pop()? as u16;
                    self.memory.store_u16(off, v)?;
                }
                Instruction::I32Store(x, _) => {
                    let off = mem_off!(self, *x);
                    let v = self.pop()? as u32;
                    self.memory.store_u32(off, v)?;
                }
                Instruction::I64Store32(x, _) => {
                    let off = mem_off!(self, *x);
                    let v = self.pop()? as u32;
                    self.memory.store_u32(off, v)?;
                }
                Instruction::I64Store(x, _) => {
                    let off = mem_off!(self, *x);
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
                },
                Instruction::I32Const(x) => {
                    self.push(*x as u32 as u64)?;
                }
                Instruction::I64Const(x) => {
                    self.push(*x as u64)?;
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
                    self.push( unsafe { v2.unchecked_add(v1) } as u32 as u64)?;
                }     
                Instruction::I32Mul => {
                    let (v2, v1) = v2_v1!(self);
                    self.push(unsafe { v2.unchecked_mul(v1) } as u32 as u64)?;
                }         
                Instruction::I32DivS => {
                    let (v2, v1) = v2_v1!(self);
                    if v1 == 0 {
                        return Err("divided by zero".into());
                    }
                    if v1 == (0x80000000u32 as i32) && v2 == -1 {
                        return Err("math over flow: divide i32.min_value by -1".into());
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
                        return Err("divided by zero".into());
                    }                    
                    self.push((v1 % v2) as u32 as u64)?;
                }        
                Instruction::I32RemU => {
                    let (v2, v1) = v2_v1!(self);
                    if v1 == 0 {
                        return Err("divided by zero".into());
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
                    self.push(unsafe { v1.unchecked_add(v2)} as u64)?;
                }        
                Instruction::I64Sub => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push(unsafe { v1.unchecked_sub(v2)} as u64)?;
                }         
                Instruction::I64Mul => {
                    let (v2, v1) = v2_v1_!(self);
                    self.push(unsafe { v1.unchecked_mul(v2)} as u64)?;
                }     
                Instruction::I64DivS => {
                    let (v2, v1) = v2_v1_!(self);
                    if v2 == 0 {
                        return Err("divided by zero".into());
                    }
                    if v1 == (0x8000000000000000u64 as i64) && v2 == -1 {
                        return Err("math overflow: divide i64.min_value by -1".into());
                    }
                    self.push((v1 / v2) as u64)?;
                }     
                Instruction::I64DivU => {
                    let (v2, v1) = v2_v1_!(self);
                    if v2 == 0 {
                        return Err("divided by zero".into());
                    }                    
                    self.push(v1 as u64 / v2 as u64)?;
                }       
                Instruction::I64RemS => {
                    let (v2, v1) = v2_v1_!(self);
                    if v2 == 0 {
                        return Err("divided by zero".into());
                    }                        
                    self.push((v1 % v2) as u64)?;
                }                   
                Instruction::I64RemU => {
                    let (v2, v1) = v2_v1_!(self);
                    if v2 == 0 {
                        return Err("divided by zero".into());
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
                _ => return Err(format!("unsupported op {}", ins)),
            }
            self.label_pc += 1;
        }

        self.ret()
    }

    fn ret(&mut self) -> Result<u64, String> {
        match self.result_type {
            None => {
                self.pop_frame()?;
                return Ok(0);
            }
            Some(t) => {
                let mut res = self.pop()?;
                match t {
                    ValueType::F32 | ValueType::I32 => {
                        res = res & (u32::MAX as u64);
                    }
                    _ => {}
                };

                self.pop_frame()?;
                return Ok(res);
            }
        }
    }

    fn execute_expr(&mut self, value_type: ValueType) -> Result<u64, String> {
        self.push_expr(self.expr.clone(), Some(value_type))?;
        self.execute()
    }

    fn branch(&mut self, l: u32) -> Result<(), String> {
        if self.label_size == 0 || ((self.label_size - 1) as u32) < l {
            return Err("branch failed: label underflow".into());
        }

        let idx = self.label_size as u32 - 1 - l;
        self.label_size = idx as u16;

        let p = self.label_base + self.label_size as u32;

        if l != 0 {
            let data: LabelData = self.label_data[p as usize];
            self.is_loop = data.is_loop();
            self.label_body = self.labels[p as usize].clone();
            self.arity = data.arity();
            self.stack_pc = data.stack_pc();
        }

        let val = if self.arity { self.pop()? } else { 0 };
        self.stack_size = self.stack_pc;

        if self.arity {
            self.push(val)?;
        }

        if self.label_base + self.label_size as u32 == self.max_labels {
            return Err("branch failed: label overflow".into());
        }

        self.label_size += 1;
        self.label_pc = if self.is_loop {
            0
        } else {
            self.label_body.len() as u16
        };
        Ok(())
    }

    fn save_frame(&mut self) {
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

    fn save_label(&mut self) {
        let p = self.label_base + (self.label_size as u32) - 1;
        self.labels[p as usize] = self.label_body.clone();
        let data = LabelData::new(self.stack_pc, self.label_pc, self.arity, self.is_loop);
        self.label_data[p as usize] = data;
    }

    fn push(&mut self, value: u64) -> Result<(), String> {
        let base = self.stack_base + (self.local_size as u32);
        let index = (base + (self.stack_size as u32)) as usize;
        if index as usize >= self.max_stacks as usize {
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
    fn clear_frame(&mut self) {
        self.label_size = 0;
        self.stack_size = 0;
        self.func_index = 0;
        self.local_size = 0;
    }
}

macro_rules! push_fr {
    ($this: ident) => {
        if $this.count == $this.max_frames {
            return Err("frame overflow".into());
        }

        if $this.count != 0 {
            $this.save_frame();
        }

        if $this.label_size != 0 {
            $this.save_label();
        }

        let mut new_stack_base = 0;
        let mut new_label_base = 0;

        if $this.count != 0 {
            new_stack_base =
                $this.stack_base + ($this.local_size as u32) + ($this.stack_size as u32);
            new_label_base = $this.label_base + ($this.label_size as u32);
        }

        $this.stack_base = new_stack_base;
        $this.label_base = new_label_base;
        $this.clear_frame();
    };
}

trait VecUtils {
    type Item;

    fn self_copy(&mut self, src: usize, dst: usize, len: usize);
    fn fill_from(&mut self, src: usize, len: usize, elem: Self::Item);
}

impl<T> VecUtils for Vec<T>
where
    T: Sized + Copy,
{
    type Item = T;

    fn self_copy(&mut self, src: usize, dst: usize, len: usize) {
        for i in 0..len {
            self[src + i] = self[dst + i];
        }
    }

    fn fill_from(&mut self, src: usize, len: usize, elem: T) {
        self[src..src + len].fill(elem);
    }
}

trait ToValueType {
    fn to_value_type(&self) -> Option<ValueType>;
}

impl ToValueType for BlockType {
    fn to_value_type(&self) -> Option<ValueType> {
        match self {
            BlockType::NoResult => None,
            BlockType::Value(v) => Some(v.clone()),
        }
    }
}

impl Instance {
    fn push_expr(
        &mut self,
        expr: Rc<Vec<Instruction>>,
        result_type: Option<ValueType>,
    ) -> Result<(), String> {
        push_fr!(self);

        self.result_type = result_type;
        self.frame_body = expr;
        self.count += 1;
        Ok(())
    }

    fn push_frame(&mut self, func: Rc<WASMFunction>, args: Option<Vec<u64>>) -> Result<(), String> {
        push_fr!(self);

        let local_len = func.locals.len() + func.fn_type.params().len();
        if local_len > u16::MAX as usize {
            return Err(format!(
                "push frame: function require too much locals {}",
                local_len
            ));
        }

        self.result_type = func.fn_type.results().get(0).map(|x| x.clone());
        self.local_size = local_len as u16;
        self.frame_body = func.body();
        self.count += 1;

        match args {
            None => {
                let c = current_frame!(self);
                if c == 0 {
                    return Err("unexpected empty frame".into());
                }
                self.push_args(func.fn_type.params().len())?;
            }
            Some(args) => {
                self.stack_data
                    .fill_from(self.stack_base as usize, local_len, 0);
                self.stack_data[self.stack_base as usize..self.stack_base as usize + args.len()]
                    .copy_from_slice(&args);
            }
        }

        Ok(())
    }

    // pop n params from stack of prev frame into current frame
    fn push_args(&mut self, params: usize) -> Result<(), String> {
        let length = params as usize;
        let frame = current_frame!(self) - 1;
        let data = self.frame_data[frame];
        let off = self.offsets[frame];
        let stack_size = data.stack_size();
        if (stack_size as usize) < length {
            return Err("push_args: stack underflow".into());
        }
        let top = off.stack_base() as usize + data.local_size() as usize + stack_size as usize;
        self.frame_data[frame] = FrameData::new(
            data.label_size(),
            data.local_size(),
            stack_size - length as u16,
            data.func_bits(),
        );

        self.stack_data
            .fill_from(self.stack_base as usize, self.local_size as usize, 0);
        self.stack_data
            .self_copy(top - params, self.stack_base as usize, params);
        Ok(())
    }

    fn get_func_by_bits(&self, bits: u16) -> Result<Rc<WASMFunction>, String> {
        let is_table = (bits & IS_TABLE_MASK) != 0;
        let index = bits & FN_INDEX_MASK;

        let o: Option<&FunctionInstance> = if is_table {
            let o = self.table.get(index as usize);
            match o {
                None => None,
                Some(x) => x.as_ref(),
            }
        } else {
            self.functions.get(index as usize)
        };

        let f_re: Result<&FunctionInstance, String> =
            o.ok_or("get_func_byt_bits: function index overflow".into());

        match &f_re? {
            &FunctionInstance::WasmFunction(x) => Ok(x.clone()),
            _ => Err("expect wasm function, while host function found".into()),
        }
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

    fn pop_frame(&mut self) -> Result<(), String> {
        self.count -= 1;

        if self.count == 0 {
            return Ok(());
        }

        let prev = self.frame_data[current_frame!(self)];
        self.stack_size = prev.stack_size();
        self.local_size = prev.local_size();
        self.label_size = prev.label_size();
        self.func_index = prev.func_bits();

        let prev_off = self.offsets[current_frame!(self)];

        self.stack_base = prev_off.stack_base();
        self.label_base = prev_off.label_base();
        self.reset_body(self.func_index)?;

        if self.label_size != 0 {
            self.load_label();
        }

        Ok(())
    }

    fn get_by_func_index(&mut self, func_index: u16) {
        let is_table = (func_index & IS_TABLE_MASK) != 0;
        let index = (func_index & FN_INDEX_MASK) as usize;
    }

    fn load_label(&mut self) {
        let p = self.label_base + self.label_size as u32 - 1;
        self.label_body = self.labels[p as usize].clone();
        let data = self.label_data[p as usize];
        self.label_pc = data.label_pc();
        self.stack_pc = data.stack_pc();
        self.arity = data.arity();
        self.is_loop = data.is_loop();
    }

    fn reset_body(&mut self, func_bits: u16) -> Result<(), String> {
        let fun = self.get_func_by_bits(func_bits)?;
        self.frame_body = fun.body();
        self.result_type = fun.fn_type.results().get(0).map(|x| x.clone());
        Ok(())
    }

    fn push_label(
        &mut self,
        arity: bool,
        body: Rc<Vec<Instruction>>,
        is_loop: bool,
    ) -> Result<(), String> {
        if self.label_size != 0 {
            self.save_label();
        }

        self.arity = arity;
        self.is_loop = is_loop;
        self.stack_pc = self.stack_size;
        self.label_body = body;
        self.label_pc = 0;

        if self.label_base + self.label_size as u32 == self.max_labels {
            return Err("push label failed: label size overflow".into());
        }
        self.label_size += 1;
        Ok(())
    }

    fn pop_label(&mut self) -> Result<(), String> {
        if self.label_size == 0 {
            return Err("pop label failed: label underflow".into());
        }
        self.label_size -= 1;
        if self.label_size != 0 {
            self.load_label();
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::{env, fs, fs::File, io::Read};

    #[test]
    fn test() {
        let filename = "src/testdata/basic.wasm";
        let mut f = File::open(filename).expect("no file found");
        let metadata = fs::metadata(filename).expect("unable to read metadata");
        let mut buffer = vec![0; metadata.len() as usize];
        f.read(&mut buffer).expect("buffer overflow");

        super::Instance::new(&buffer).unwrap();
    }
}
