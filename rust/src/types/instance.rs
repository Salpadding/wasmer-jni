use std::collections::BTreeMap;
use std::rc::Rc;

use parity_wasm::elements::{BlockType, ResizableLimits};
use parity_wasm::elements::{
    External, FuncBody, FunctionType, GlobalType, Instruction, Local, Module, Type, ValueType,
};

use crate::StringErr;
use crate::types::executable::Runnable;
use crate::types::frame_data::{FrameData, FunctionBits};
use crate::types::initializer::InitFromModule;
use crate::types::label_data::LabelData;
use crate::types::memory::Memory;
use crate::types::offset::Offset;
use crate::types::table::Table;
use crate::utils::VecUtils;
use crate::types::ins_pool::{InsVec, InsPool};

const MAX_SIGNED_INT: u64 = 0x7fffffff;

#[derive(Debug)]
pub(crate) struct WASMFunction {
    pub(crate) fn_type: FunctionType,
    pub(crate) body: InsVec,
    pub(crate) locals: Vec<Local>,
}

impl WASMFunction {
    fn body(&self) -> InsVec {
        self.body
    }

    pub(crate) fn local_len(&self) -> u32 {
        self.locals.iter().map(|x| x.count()).sum()
    }

}

#[derive(Clone)]
pub struct HostFunction {
    pub(crate) module: String,
    pub(crate) field: String,
    pub(crate) fn_type: FunctionType,
}

#[derive(Clone)]
pub(crate) enum FunctionInstance {
    HostFunction(Rc<HostFunction>),
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
    offsets: Vec<Offset>,

    // memory
    pub(crate) memory: Memory,

    // current frame
    pub(crate) label_size: u16,
    stack_size: u16,
    local_size: u16,
    func_bits: FunctionBits,

    pub(crate) frame_body: InsVec,
    pub(crate) label_body: InsVec,

    stack_base: u32,
    label_base: u32,

    pub(crate) result_type: Option<ValueType>,

    // current label
    pub(crate) label_pc: u16,

    pub(crate) labels: Vec<InsVec>,

    arity: bool,
    is_loop: bool,
    stack_pc: u16,



    // static region
    pub(crate) functions: Vec<FunctionInstance>,
    pub(crate) exports: BTreeMap<String, Rc<WASMFunction>>,
    pub(crate) types: Vec<FunctionType>,
    pub(crate) table: Table,
    table_limit: Option<ResizableLimits>,

    // runtime
    pub(crate) globals: Vec<u64>,
    pub(crate) global_types: Vec<GlobalType>,

    // expr
    pub(crate) expr: InsVec,

    pub(crate) pool: InsPool,
}

macro_rules! current_frame {
    ($this: ident) => {{
        ($this.count - 1) as usize
    }};
}

impl Instance {
    pub fn new(bin: &[u8], max_frames: u16, max_stacks: u32, max_labels: u32) -> Result<Instance, StringErr> {
        let mut r = Instance::default();
        r.max_frames = max_frames;
        r.max_stacks = max_stacks;
        r.max_labels = max_labels;
        r.offsets = vec![Offset::default(); r.max_frames as usize];
        r.frame_data = vec![FrameData(0); r.max_frames as usize];
        r.stack_data = vec![0u64; r.max_stacks as usize];
        r.label_data = vec![LabelData(0); r.max_labels as usize];
        r.labels = vec![InsVec::null(); r.max_labels as usize];
        r.pool = InsPool::new();


        let md = Module::from_bytes(bin)?;
        r.init(md)?;
        Ok(r)
    }


    pub(crate) fn ret(&mut self) -> Result<Option<u64>, StringErr> {
        match self.result_type {
            None => {
                self.pop_frame()?;
                Ok(None)
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
                Ok(Some(res))
            }
        }
    }


    pub fn execute(&mut self, name: &str, args: &[u64]) -> Result<Vec<u64>, StringErr> {
        let fun = get_or_err!(self.exports, name, "function not found");
        self.push_frame(fun.clone(), Some(args.to_vec()));
        Ok(opt_to_vec!(self.run()?))
    }

    pub(crate) fn execute_expr(&mut self, value_type: ValueType) -> Result<Option<u64>, StringErr> {
        self.push_expr(self.expr, Some(value_type))?;
        self.run()
    }

    pub(crate) fn branch(&mut self, l: u32) -> Result<(), String> {
        if self.label_size == 0 || ((self.label_size - 1) as u32) < l {
            return Err("branch failed: label underflow".into());
        }

        let idx = self.label_size as u32 - 1 - l;
        self.label_size = idx as u16;

        let p = self.label_base + self.label_size as u32;

        if l != 0 {
            let data: LabelData = self.label_data[p as usize];
            self.is_loop = data.is_loop();
            self.label_body = self.labels[p as usize];
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
           self.label_body.size() as u16
        };
        Ok(())
    }

    fn save_frame(&mut self) {
        let data = FrameData::new(
            self.label_size,
            self.local_size,
            self.stack_size,
            self.func_bits,
        );
        let off = Offset::new(self.label_base, self.stack_base);
        self.frame_data[current_frame!(self)] = data;
        self.offsets[current_frame!(self)] = off;
    }

    fn save_label(&mut self) {
        let p = self.label_base + (self.label_size as u32) - 1;
        let data = LabelData::new(self.stack_pc, self.label_pc,  self.arity, self.is_loop);
        self.label_data[p as usize] = data;
    }

    pub(crate) fn push(&mut self, value: u64) -> Result<(), String> {
        let base = self.stack_base + (self.local_size as u32);
        let index = (base + (self.stack_size as u32)) as usize;
        if index >= self.max_stacks as usize {
            return Err("stack overflow".into());
        }
        self.stack_data[index] = value;
        self.stack_size += 1;
        Ok(())
    }

    pub(crate) fn peek(&self) -> Result<u64, String> {
        let base = self.stack_base + (self.local_size as u32);
        if self.stack_size == 0 {
            return Err("stack underflow".into());
        }

        let v = self.stack_data[(base + (self.stack_size as u32) - 1) as usize];
        Ok(v)
    }

    pub(crate) fn drop_unchecked(&mut self) {
        self.stack_size -= 1
    }

    pub(crate) fn pop(&mut self) -> Result<u64, String> {
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
        self.func_bits = FunctionBits::default();
        self.local_size = 0;
    }
}

macro_rules! push_fr {
    ($this: ident) => {
        if $this.count == $this.max_frames {
            return Err(StringErr::new("frame overflow"));
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

pub(crate) trait ToValueType {
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
        expr: InsVec,
        result_type: Option<ValueType>,
    ) -> Result<(), StringErr> {
        push_fr!(self);

        self.result_type = result_type;
        self.frame_body = expr;
        self.count += 1;
        Ok(())
    }

    pub(crate) fn push_frame(&mut self, func: Rc<WASMFunction>, args: Option<Vec<u64>>) -> Result<(), StringErr> {
        push_fr!(self);

        let local_len = func.local_len() as usize + func.fn_type.params().len();

        if local_len > u16::MAX as usize {
            let msg = format!(
                "push frame: function require too much locals {}",
                local_len
            );
            return Err(StringErr::new(msg));
        }

        self.result_type = func.fn_type.results().get(0).map(|x| x.clone());
        self.local_size = local_len as u16;
        self.frame_body = func.body();
        self.count += 1;

        match args {
            None => {
                let c = current_frame!(self);
                if c == 0 {
                    return Err(StringErr::new("unexpected empty frame"));
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

    fn get_func_by_bits(&self, bits: FunctionBits) -> Result<Rc<WASMFunction>, String> {
        let o: Option<&FunctionInstance> = if bits.is_table() {
            let o = self.table.functions.get(bits.fn_index() as usize);
            match o {
                None => None,
                Some(x) => x.as_ref(),
            }
        } else {
            self.functions.get(bits.fn_index() as usize)
        };

        let f_re: Result<&FunctionInstance, String> =
            o.ok_or("get_func_byt_bits: function index overflow".into());

        match &f_re? {
            &FunctionInstance::WasmFunction(x) => Ok(x.clone()),
            _ => Err("expect wasm function, while host function found".into()),
        }
    }

    pub(crate) fn set_local(&mut self, index: u32, value: u64) -> Result<(), String> {
        if index >= self.local_size as u32 {
            return Err("local variable access overflow".into());
        }

        self.stack_data[(self.stack_base + index) as usize] = value;
        Ok(())
    }

    pub(crate) fn get_local(&self, index: u32) -> Result<u64, String> {
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
        self.func_bits = prev.func_bits();

        let prev_off = self.offsets[current_frame!(self)];

        self.stack_base = prev_off.stack_base();
        self.label_base = prev_off.label_base();
        self.reset_body(self.func_bits)?;

        if self.label_size != 0 {
            self.load_label();
        }

        Ok(())
    }


    fn load_label(&mut self) {
        let p = self.label_base + self.label_size as u32 - 1;
        let data = self.label_data[p as usize];
        self.label_pc = data.label_pc();
        self.stack_pc = data.stack_pc();
        self.arity = data.arity();
        self.is_loop = data.is_loop();
    }

    fn reset_body(&mut self, func_bits: FunctionBits) -> Result<(), String> {
        let fun = self.get_func_by_bits(func_bits)?;
        self.frame_body = fun.body();
        self.result_type = fun.fn_type.results().get(0).map(|x| x.clone());
        Ok(())
    }

    pub(crate) fn print_stack(&self) {
        let mut stack_base = self.stack_base + self.local_size as u32;
        print!("stack = {}", '[');
        for i in stack_base..stack_base+self.stack_size as u32{
            print!("{}", self.stack_data[i as usize]);
            print!("{}", ',');
        }
        print!("{}", "]\n");

        stack_base = self.stack_base;
        print!("local = {}", '[');
        for i in stack_base..stack_base+self.local_size as u32{
            print!("{}", self.stack_data[i as usize]);
            print!("{}", ',');
        }
        print!("{}", "]\n");

        // let label_base = self.label_base;
        // print!("labels = {}", '[');
        // for i in label_base..label_base+self.label_size as u32 - 1{
        //     let pc = self.label_data[i as usize].label_pc();
        //     print!("{}", self.frame_body[pc as usize - 1]);
        //
        //     print!("{}", ',');
        // }
        // print!("{}", "]\n");

        // print!("next = {}", '[');
        // let mut i = 0;
        //
        // while self.label_pc + i < self.frame_body.len() as u16 && i < 16 {
        //     print!("{}", self.frame_body[(self.label_pc + i) as usize]);
        //     print!(",");
        //     i += 1;
        // }
        //
        // print!("{}", "]\n");

    }

    pub(crate) fn push_label(
        &mut self,
        arity: bool,
        body: InsVec,
        is_loop: bool,
    ) -> Result<(), String> {
        if self.label_size != 0 {
            self.save_label();
            // since new label created, mark the start of new label
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

    pub(crate) fn pop_label(&mut self) -> Result<(), String> {
        if self.label_size == 0 {
            return Err("pop label failed: label underflow".into());
        }
        self.label_size -= 1;
        if self.label_size != 0 {
            self.load_label();
        }

        println!("pop label");
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

        super::Instance::new(&buffer, 16000, 16000 * 16, 16000 * 16).unwrap();
    }
}
