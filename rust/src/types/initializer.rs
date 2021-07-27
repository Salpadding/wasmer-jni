use std::rc::Rc;

use parity_wasm::elements::{External, FuncBody, Internal, Module, Type, ValueType, Serialize, Instructions, InitExpr};

use crate::StringErr;
use crate::types::executable::Runnable;
use crate::types::frame_data::FN_INDEX_MASK;
use crate::types::instance::{FunctionInstance, HostFunction, Instance, WASMFunction};
use std::io::Cursor;
use crate::types::ins_pool::InsVec;

macro_rules! read_expr {
    ($this: expr, $code: expr) => {
        {
            let mut vec: Vec<u8> = Vec::new();
            $code.serialize(&mut vec)?;
            let mut cur = Cursor::new(vec);
            $this.pool.read_expr(&mut cur)?
        }
    };
}

pub(crate) trait InitFromModule {
    fn init(&mut self, md: Module) -> Result<(), StringErr>;
}

impl InitFromModule for Instance {
    fn init(&mut self, md: Module) -> Result<(), StringErr> {
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

        let exprs: Vec<InsVec> = {
            let mut v = Vec::new();
            for x in codes.iter() {
                let r = x.code().clone();
                v.push(read_expr!(self, r));
            }
            v
        };

        match md.import_section() {
            None => {}
            Some(sec) => {
                for imp in sec.entries().iter() {
                    match imp.external() {
                        External::Function(i) => {
                            let h = HostFunction {
                                module: imp.module().into(),
                                field: imp.field().into(),
                                fn_type: get_or_err!(
                                        self.types,
                                        *i as usize,
                                        "function not found"
                                    ).clone(),
                            };
                            self.functions
                                .push(FunctionInstance::HostFunction(Rc::new(h)))
                        }
                        _ => {
                            let msg = format!("unsupported import type {:?}", imp.external());
                            return Err(StringErr::new(msg));
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
                    self.expr = read_expr!(self, g.init_expr().clone());
                    self.globals[i] = self.execute_expr(g.global_type().content_type())?.unwrap();
                }
            }
        };

        match md.function_section() {
            None => {}
            Some(sec) => {
                if sec.entries().len() > FN_INDEX_MASK as usize {
                    let msg = format!(
                        "function section overflow, too much functions {} > {}",
                        sec.entries().len(),
                        FN_INDEX_MASK
                    );
                    return Err(StringErr::new(msg));
                }
                for i in 0..sec.entries().len() {
                    let t = sec.entries()[i].type_ref();
                    if t as usize > self.types.len() || i as usize > codes.len() {
                        let msg = format!("type entry or code entry not found func entry = {}, type entires = {}, code entries = {}", i, self.types.len(), codes.len());
                        return Err(StringErr::new(msg));
                    }

                    let w = WASMFunction {
                        fn_type: self.types[t as usize].clone(),
                        body: exprs[i as usize],
                        locals: codes[i as usize].locals().to_vec(),
                    };

                    self.functions
                        .push(FunctionInstance::WasmFunction(Rc::new(w)))
                }
            }
        };

        match md.elements_section() {
            Some(sec) => {
                for e in sec.entries() {
                    let off = match e.offset() {
                        Some(ex) => {
                            self.expr = read_expr!(self, ex.clone());
                            self.execute_expr(ValueType::I32)?
                        }
                        _ => Some(0)
                    };
                    self.table.put_elements(off.unwrap() as usize, &[self.functions[e.index() as usize].clone()])
                }
            }
            _ => {}
        }

        match md.memory_section() {
            Some(sec) => {
                if sec.entries().len() > 1 {
                    return Err(StringErr::new("multi memory"));
                }
                self.memory.init(&sec.entries()[0].limits());
            }
            _ => {  }
        }

        match md.data_section() {
            Some(sec) => {
                for seg in sec.entries() {
                    let off: u64 = match seg.offset() {
                        None => 0,
                        Some(ex) => {
                            self.expr = read_expr!(self, ex.clone());
                            self.execute_expr(ValueType::I32)?.unwrap()
                        }
                    };
                    self.memory.write(off as usize, seg.value());
                }
            }
            _ => {}
        }

        match md.start_section() {
            Some(i) => {
                let start = get_or_err!(self.functions, i as usize, "start function not found");
                match start {
                    FunctionInstance::HostFunction(_) => {
                        return Err(StringErr::new("start function cannot be host"));
                    }
                    FunctionInstance::WasmFunction(w) => {
                        self.push_frame(w.clone(), Some(Vec::new()));
                        self.run();
                    }
                }
            }
            _ => {}
        }

        match md.export_section() {
            Some(sec) => {
                for e in sec.entries() {
                    match e.internal() {
                        Internal::Function(i) => {
                            let i = *i;
                            self.exports.insert(e.field().to_string(), {
                                match get_or_err!(self.functions, i as usize, "func not found") {
                                    FunctionInstance::WasmFunction(w) => {
                                        w.clone()
                                    }
                                    _ => return Err(StringErr::new("export shouldn't be host function"))
                                }
                            });
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }
}