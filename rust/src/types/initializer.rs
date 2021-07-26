use std::rc::Rc;

use parity_wasm::elements::{External, FuncBody, Internal, Module, Type, ValueType};

use crate::StringErr;
use crate::types::executable::Runnable;
use crate::types::frame_data::FN_INDEX_MASK;
use crate::types::instance::{FunctionInstance, HostFunction, Instance, WASMFunction};

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
                    self.expr = Rc::new(g.init_expr().code().to_vec());
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

                    let w = WASMFunction::new(
                        self.types[t as usize].clone(),
                        codes[i as usize].clone(),
                    );

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
                            self.expr = Rc::new(ex.code().to_vec());
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
                            self.expr = Rc::new(ex.code().to_vec());
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