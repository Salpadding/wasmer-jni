#![feature(unchecked_math)] // allow unchecked math

#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate wasmer;

use instance::StringErr;
use utils::JNIUtil;
use wasmer::{CompileError, ExportError, Exports, Features, Function, ImportObject, Instance, InstantiationError, Module, RuntimeError, Store, Type, Value, imports};
use wasmer_compiler_singlepass::Singlepass;
use wasmer_engine_universal::Universal;

mod hex;
mod instance;
mod utils;
mod handlers;

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
use std::ptr::null_mut;
use std::str::Utf8Error;

const MAX_INSTANCES: usize = 1024;

use std::sync::{PoisonError};


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
pub extern "system" fn Java_org_github_salpadding_wasmer_Natives_createInstance(
    env: JNIEnv,
    // this is the class that owns our
    // static method. Not going to be
    // used, but still needs to have
    // an argument slot
    _class: JClass,
    _module: jbyteArray,
    _features: jlong,
    _hosts: jobjectArray,
) -> jint {
    jni_ret!(create_instance(env, _module, _features, _hosts), env, 0)
}

#[no_mangle]
pub extern "system" fn Java_org_github_salpadding_wasmer_Natives_execute(
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


impl From<jni::errors::Error> for StringErr {
    fn from(e: jni::errors::Error) -> Self {
        StringErr(format!("{:?}", e))
    }
}


impl <T> From<PoisonError<T>> for StringErr {
    fn from(e: PoisonError<T>) -> Self {
        StringErr("lock error".into())
    }
}

impl From<Utf8Error> for StringErr {
    fn from(e: Utf8Error) -> Self {
        StringErr(format!("{:?}", e))
    }
}

impl From<CompileError> for StringErr {
    fn from(e: CompileError) -> Self {
        StringErr(format!("{:?}", e))
    }
}

impl From<InstantiationError> for StringErr {
    fn from(e: InstantiationError) -> Self {
        StringErr(format!("{:?}", e))
    }
}

impl From<ExportError> for StringErr {
    fn from(e: ExportError) -> Self {
        StringErr(format!("{:?}", e))
    }
}

impl From<RuntimeError> for StringErr {
    fn from(e: RuntimeError) -> Self {
        StringErr(format!("{:?}", e))
    }
}



mod features_enum {
        /// Threads proposal should be enabled
        pub const threads: u64 = 1;
        /// Reference Types proposal should be enabled
        pub const reference_types: u64 = 1 << 1;
        /// SIMD proposal should be enabled
        pub const simd: u64 = 1 << 2;
        /// Bulk Memory proposal should be enabled
        pub const bulk_memory: u64 = 1 << 3;
        /// Multi Value proposal should be enabled
        pub const multi_value: u64 = 1 << 4;
        /// Tail call proposal should be enabled
        pub const tail_call: u64 = 1 << 5;
        /// Module Linking proposal should be enabled
        pub const module_linking: u64 = 1 << 6;
        /// Multi Memory proposal should be enabled
        pub const multi_memory: u64 = 1 << 7;
        /// 64-bit Memory proposal should be enabled
        pub const memory64: u64 = 1 << 8;
}


macro_rules! set_mask {
    ($mask: expr, $feature: expr, $opt: ident) => {
        $feature.$opt($mask & features_enum::$opt != 0);
    };
}

fn create_instance(
    env: JNIEnv,
    _module: jbyteArray,
    _features: jlong,
    _hosts: jobjectArray,
) -> Result<jint, StringErr> {

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
                    let mask = _features as u64;

                    set_mask!(mask, features, threads);
                    set_mask!(mask, features, reference_types);
                    set_mask!(mask, features, simd);
                    set_mask!(mask, features, bulk_memory);
                    set_mask!(mask, features, multi_value);
                    set_mask!(mask, features, tail_call);
                    set_mask!(mask, features, module_linking);
                    set_mask!(mask, features, multi_memory);
                    set_mask!(mask, features, memory64);

                    // Create the store
                    let store = Store::new(&Universal::new(compiler).features(features).engine());                    
                    let bytes = env.convert_byte_array(_module)?;
                    let module = Module::new(&store, bytes)?;

                    fn multiply(a: i32) -> i32 {
                        println!("Calling `multiply_native`...");
                        let result = a * 3;
                
                        println!("Result of `multiply_native`: {:?}", result);
                
                        result
                    }


                    let multiply_native = Function::new_native(&store, multiply);
                    
                    let import_object = imports! {
                        "env" => {
                            "multiply_native" => multiply_native,
                        }
                    };


                    let instance = Instance::new(&module, &import_object)?;
    
                    *m = Some(instance);
                    return Ok(i as jint);
                }
                Some(_) => {
                    continue;
                }
            };
    
        }
        Err(StringErr("instance descriptor overflows".into()))        
    }
}

fn execute(
    env: JNIEnv,
    id: jint,
    _method: jstring,
    args: jlongArray,
) -> Result<jlongArray, StringErr> {

    unsafe {
        let r = INSTANCES.get(id as usize).and_then(|x| x.as_ref());
        match r {
            Some(ins) => {
                let method = env.get_string(_method.into())?;
                let s = method.to_str()?;
                let fun = ins.exports.get_function(s)?;                
                let sig = fun.get_vm_function().signature.clone();


                let a: Vec<i64> = env.jlong_array_to_vec(args)?;

                if sig.params().len() != a.len() {
                    return Err(StringErr("invalid params length".into()))
                }

                let a = {
                    let mut v: Vec<Value> = Vec::new();

                    for i in 0..a.len() {
                        let t = sig.params()[i];
                        let j = a[i];

                        let k = match t {
                            Type::I32 => Value::I32(j as i32),
                            /// Signed 64 bit integer.
                            Type::I64 => Value::I64(j),
                            /// Floating point 32 bit integer.
                            Type::F32 => Value::F32(f32::from_bits((j as i32) as u32)),
                            /// Floating point 64 bit integer.
                            Type::F64 => Value::F64(f64::from_bits(j as u64)),
                            _ => return Err(StringErr("unsupoorted param type".into())),
                        };

                        v.push(k);
                    }

                    v
                };


                let results = fun.call(&a)?.to_vec();
                let results = {
                    let mut v: Vec<i64> = Vec::new();

                    for x in results.iter() {
                        let y = match x {
                            Value::I32(x) => *x as u32 as i64,
                            Value::I64(x) => *x,
                            Value::F32(x) => x.to_bits() as u64 as i64,
                            Value::F64(x) => x.to_bits() as i64,
                            _ => return Err(StringErr("unsupported return type".into()))
                        };

                        v.push(y);
                    }

                    v
                };
         

                return env.slice_to_jlong_array(&results);
                    
                
            },
            None => return Err(StringErr("instance nof found".into()))         
        }   
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn test() {}
}
