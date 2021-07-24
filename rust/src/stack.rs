use std::any::Any;

use alloc::borrow::ToOwned;
use alloc::collections::BTreeMap;
use alloc::rc::Rc;
use alloc::string::*;
use alloc::vec::*;

use parity_wasm::elements::GlobalEntry;
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
    md: Option<Module>,
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
                                    fn_type: self.types.get(*i as usize).ok_or("err")?.clone(),
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
        self.push_label(self.result_type.is_some(), self.frame_body.clone(), false);

        while self.label_size != 0 {
            if self.label_pc as usize >= self.label_body.len() {
                self.pop_label();
                continue;
            }

            match self.label_body[self.label_pc as usize] {
                Instruction::Return => {
                    return self.ret();
                },
                Instruction::Nop | Instruction::I32ReinterpretF32 | Instruction::I64ReinterpretF64 | Instruction::I64ExtendUI32 => {},
            }

            self.label_pc += 1;
        }

        self.ret()
    }

    fn ret(&mut self) -> Result<u64, String>  {
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

                self.pop_frame();
                return Ok(res);
            }
        }
    }

    fn execute_expr(&mut self, value_type: ValueType) -> Result<u64, String> {
        self.push_expr(self.expr.clone(), Some(value_type));
        self.execute()
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

    fn push_frame(&mut self, func_index: u32, args: Option<Vec<u64>>) -> Result<(), String> {
        push_fr!(self);

        if func_index > u16::MAX as u32 {
            return Err("function index overflow".into());
        }

        self.func_index = func_index as u16;
        let func = self.get_func_by_bits(self.func_index)?;
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
            data.func_index(),
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

        match f_re? {
            &FunctionInstance::WasmFunction(ref x) => Ok(x.clone()),
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
        self.func_index = prev.func_index();

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
