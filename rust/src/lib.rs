#![feature(unchecked_math)] // allow unchecked math

#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate wasmer;

use wasmer::{CompileError, ExportError, Features, Instance, InstantiationError, Module, RuntimeError, Store, Value, imports};
use wasmer_compiler_singlepass::Singlepass;
use wasmer_engine_universal::Universal;

mod hex;
mod instance;

// This is the interface to the JVM that we'll
// call the majority of our methods on.
use jni::JNIEnv;

// These objects are what you should use as arguments to your native function.
// They carry extra lifetime information to prevent them escaping this context
// and getting used after being GC'd.
use jni::objects::{GlobalRef, JClass, JObject, JString, TypeArray};

// This is just a pointer. We'll be returning it from our function.
// We can't return one of the objects with lifetime information because the
// lifetime checker won't let us.
use jni::sys::{jbyteArray, jint, jlong, jlongArray, jobjectArray, jstring};

use std::cell::{Cell, RefCell, RefMut};
use std::ptr::null_mut;
use std::str::Utf8Error;
use std::{sync::mpsc, thread, time::Duration};

struct StaticErr(&'static str);

const MAX_INSTANCES: usize = 1024;

use std::sync::{Arc, Mutex, PoisonError};


static mut INSTANCES: Vec<Option<Instance>> = vec![];

macro_rules! jni_ret {
    ($ex: expr, $env: ident, $default: expr) => {
        match $ex {
            Ok(r) => r,
            Err(e) => {
                $env.throw_new("java/lang/RuntimeException", e.0);
                $default
            }
        }
    };
}

// This keeps rust from "mangling" the name and making it unique for this crate.
#[no_mangle]
pub extern "system" fn Java_org_github_salpadding_tinywasm_Natives_createInstance(
    env: JNIEnv,
    // this is the class that owns our
    // static method. Not going to be
    // used, but still needs to have
    // an argument slot
    _class: JClass,
    _module: jbyteArray,
    _hosts: jobjectArray,
) -> jint {
    jni_ret!(create_instance(env, _module, _hosts), env, 0)
}

#[no_mangle]
pub extern "system" fn Java_org_github_salpadding_tinywasm_Natives_execute(
    env: JNIEnv,
    // this is the class that owns our
    // static method. Not going to be
    // used, but still needs to have
    // an argument slot
    _class: JClass,
    _id: jint,
    _method: jstring,
    _args: jlongArray,
) -> jlongArray {
    jni_ret!(execute(env, _id, _method, _args), env, null_mut())
}


impl From<jni::errors::Error> for StaticErr {
    fn from(_: jni::errors::Error) -> Self {
        StaticErr("jni error")
    }
}


impl <T> From<PoisonError<T>> for StaticErr {
    fn from(e: PoisonError<T>) -> Self {
        StaticErr("lock error")
    }
}

impl From<Utf8Error> for StaticErr {
    fn from(e: Utf8Error) -> Self {
        StaticErr("utf8 error")
    }
}

impl From<CompileError> for StaticErr {
    fn from(e: CompileError) -> Self {
        StaticErr("compiler error")
    }
}

impl From<InstantiationError> for StaticErr {
    fn from(e: InstantiationError) -> Self {
        StaticErr("InstantiationError")
    }
}

impl From<ExportError> for StaticErr {
    fn from(e: ExportError) -> Self {
        StaticErr("ExportError")
    }
}

impl From<RuntimeError> for StaticErr {
    fn from(e: RuntimeError) -> Self {
        StaticErr("RuntimeError")
    }
}


fn create_instance(
    env: JNIEnv,
    _module: jbyteArray,
    _hosts: jobjectArray,
) -> Result<jint, StaticErr> {

    unsafe {
        if INSTANCES.is_empty() {
            for _ in 0..MAX_INSTANCES {
                INSTANCES.push(None);
            }
        }

        for i in 0..MAX_INSTANCES {
            let m = &mut INSTANCES[i] ;
    
            match m {
                None => {
                    // Use Singlepass compiler with the default settings
                    let compiler = Singlepass::default();
                    let mut features = Features::new();

                    // Create the store
                    let store = Store::new(&Universal::new(compiler).engine());                    
                    let bytes = env.convert_byte_array(_module)?;
                    let module = Module::new(&store, bytes)?;
                    let import_object = imports! {};                    
                    let instance = Instance::new(&module, &import_object)?;

    
                    *m = Some(instance);
                    return Ok(i as jint);
                }
                Some(_) => {
                    continue;
                }
            };
    
        }
        Err(StaticErr("instance descriptor overflows"))        
    }
}

trait AsVec {
    fn to_vec<'a, T: From<(JNIEnv<'a>, JObject<'a>)>>(&self, env: JNIEnv<'a> ) -> Result<Option<Vec<T>>, StaticErr>;
}

fn execute(
    env: JNIEnv,
    id: jint,
    _method: jstring,
    args: jlongArray,
) -> Result<jlongArray, StaticErr> {

    unsafe {
        let r = INSTANCES.get(id as usize).and_then(|x| x.as_ref());
        match r {
            Some(ins) => {
                let a: Vec<Value> = if args.is_null() { Vec::new() } else {
                    let arr = env.get_long_array_elements(args, jni::objects::ReleaseMode::CopyBack)?;
                    
                    (0..arr.size()?).map(|i| *arr.as_ptr().offset(i as isize)).map(|x| Value::I64(x)).collect()
                };
                let method = env.get_string(_method.into())?;
                let s = method.to_str()?;

                let fun = ins.exports.get_function(s)?;
                let results = fun.call(&a)?.to_vec();

                
                let o = env.new_long_array(results.len() as i32)?;
                let arr = env.get_long_array_elements(o, jni::objects::ReleaseMode::NoCopyBack)?;
                
                for i in 0..results.len() {
                    *arr.as_ptr().offset(i as isize) = results[i].i64().unwrap_or(0);
                }
                return Ok(o);
                    
                
            },
            None => return Err(StaticErr("instance nof found"))         
        }   
    }

    Err(StaticErr(""))
}

#[cfg(test)]
mod test {
    #[test]
    fn test() {}
}
