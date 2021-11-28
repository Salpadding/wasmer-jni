#![feature(unchecked_math)] // allow unchecked math
#![allow(warnings)]

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

macro_rules! set_mask {
    ($mask: expr, $feature: expr, $( $opt:ident ),*) => {
        $($feature.$opt($mask & features_enum::$opt != 0);)*
    };
}

macro_rules! as_i64_vec {
    ($re: expr, $err: expr) => {{
        let mut v: Vec<i64> = Vec::new();

        for x in $re.iter() {
            let y = match x {
                Value::I32(x) => *x as u32 as i64,
                Value::I64(x) => *x,
                Value::F32(x) => x.to_bits() as u64 as i64,
                Value::F64(x) => x.to_bits() as i64,
                _ => return Err($err),
            };

            v.push(y);
        }

        v
    };};
}

macro_rules! u8_to_type {
    ($e: expr) => {{
        match $e {
            0 => Some(Type::I32),
            1 => Some(Type::I64),
            2 => Some(Type::F32),
            3 => Some(Type::F64),
            _ => None,
        }
    }};
}

macro_rules! as_rt {
    ($x: expr) => {{
        $x.map_err(|x| RuntimeError::new(format!("{:?}", x)))?
    }};
}

macro_rules! decode_sig {
    ($sigs: expr) => {
        {
            let mut r = Vec::new();
            for s in $sigs {
                let ret = u8_to_type!(s[0]);
                let pair: (Vec<Type>, Vec<Type>) = (
                    // signature passed from java side is valid
                    s[1..].iter().map(|x| u8_to_type!(*x).unwrap()).collect(),
                    ret.map(|f| vec![f]).unwrap_or(Vec::new()),
                );
                r.push(pair);
            }
            r
        };
    };
}

#[macro_use]
extern crate lazy_static;
#[cfg(test)]
#[macro_use]
extern crate serde;
#[macro_use]
extern crate wasmer;

use std::error::Error;
use std::fmt::{Debug, Display, Formatter, Write};
use std::ops::Deref;
use std::ptr::null_mut;
use std::str::Utf8Error;
use std::sync::PoisonError;

// This is the interface to the JVM that we'll
// call the majority of our methods on.
use jni::JNIEnv;
// These objects are what you should use as arguments to your native function.
// They carry extra lifetime information to prevent them escaping this context
// and getting used after being GC'd.
use jni::objects::{GlobalRef, JClass, JObject, JString, JValue, TypeArray};
// This is just a pointer. We'll be returning it from our function.
// We can't return one of the objects with lifetime information because the
// lifetime checker won't let us.
use jni::sys::{_jobject, jbyteArray, jint, jlong, jlongArray, jobject, jobjectArray, jstring};
use wasmer::{
    CompileError, ExportError, Exports, Features, Function, FunctionType, ImportObject, imports,
    Instance, InstantiationError, Module, RuntimeError, Store, Type, Value,
};
use wasmer::wasmparser::Operator;
use wasmer_compiler_singlepass::Singlepass;
use wasmer_engine_universal::Universal;
use wasmer_compiler_cranelift::Cranelift;

use utils::{JNIUtil, ToVmType};

use crate::rp::Rp;

mod hex;
mod utils;
mod rp;
mod instance;


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
    _options: jlong,
    _ins: jint,
    _host_names: jobjectArray,
    _signatures: jobjectArray,
) -> jlong {
    jni_ret!(
        create_instance(env, _class, _module, _options, _ins, _host_names, _signatures),
        env,
        0
    )
}

#[no_mangle]
pub extern "system" fn Java_org_github_salpadding_wasmer_Natives_execute(
    env: JNIEnv,
    // this is the class that owns our
    // static method. Not going to be
    // used, but still needs to have
    // an argument slot
    _class: JClass,
    _id: jlong,
    _method: jstring,
    _args: jlongArray,
) -> jlongArray {
    jni_ret!(crate::instance::execute(env, _id, _method, _args), env, null_mut())
}

#[no_mangle]
pub extern "system" fn Java_org_github_salpadding_wasmer_Natives_close(
    env: JNIEnv,
    // this is the class that owns our
    // static method. Not going to be
    // used, but still needs to have
    // an argument slot
    _class: JClass,
    _id: jlong,
) {
    jni_ret!(crate::instance::close(env, _id), env, ())
}

#[no_mangle]
pub extern "system" fn Java_org_github_salpadding_wasmer_Natives_getMemory(
    env: JNIEnv,
    // this is the class that owns our
    // static method. Not going to be
    // used, but still needs to have
    // an argument slot
    _class: JClass,
    _id: jlong,
    off: jint,
    len: jint,
) -> jbyteArray {
    jni_ret!(crate::instance::get_memory(env, _id, off, len), env, null_mut())
}

#[no_mangle]
pub extern "system" fn Java_org_github_salpadding_wasmer_Natives_setMemory(
    env: JNIEnv,
    // this is the class that owns our
    // static method. Not going to be
    // used, but still needs to have
    // an argument slot
    _class: JClass,
    _id: jlong,
    off: jint,
    buf: jbyteArray,
) {
    jni_ret!(crate::instance::set_memory(env, _id, off, buf), env, ())
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


#[inline]
fn get_ins_by_id(id: usize) -> Rp<Instance> {
    id.into()
}

fn create_instance(
    env: JNIEnv,
    _class: JClass,
    _module: jbyteArray,
    _options: jlong,
    ins: jint,
    _host_names: jobjectArray,
    _signatures: jobjectArray,
) -> Result<jlong, StringErr> {
    unsafe {
        let host_names = env.jstring_array_to_vec(_host_names)?;
        let sigs = env.jbytes_array_to_vec(_signatures)?;
        let sigs: Vec<(Vec<Type>, Vec<Type>)> = decode_sig!(sigs);
        let mut features = Features::new();
        let mask = _options as u64;

        set_mask!(
                mask,
                features,
                threads,
                reference_types,
                simd,
                bulk_memory,
                multi_value,
                tail_call,
                module_linking,
                multi_memory,
                memory64
            );


        // Create the store
        let store = Store::new(&Universal::new(Cranelift::default()).features(features).engine());
        let bytes = env.convert_byte_array(_module)?;
        let module = Module::new(&store, bytes)?;

        let mut import_object = ImportObject::new();
        let mut namespace = Exports::new();

        for i in 0..host_names.len() {
            let name = host_names[i].clone();
            let jvm = env.get_java_vm()?;
            let s = sigs[i].clone();
            let host_function = crate::instance::create_host(&store, s, jvm, ins, i as jint);
            namespace.insert(name, host_function);
        }

        import_object.register("env", namespace);

        let instance = Instance::new(&module, &import_object)?;

        let i = Rp::new(instance).ptr();
        return Ok(i as jlong);
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn test() {}
}

macro_rules! impl_from {
    ($debug: ty) => {
        impl From<$debug> for StringErr {
            fn from(e: $debug) -> StringErr {
                StringErr(format!("{:?}", e))
            }
        }
    };
}

impl_from!(RuntimeError);
impl_from!(jni::errors::Error);
impl_from!(Utf8Error);
impl_from!(ExportError);
impl_from!(InstantiationError);
impl_from!(CompileError);
impl_from!(String);

// Error handling utils
pub struct StringErr(pub String);

impl StringErr {
    fn new<T: Deref<Target=str>>(t: T) -> Self {
        StringErr(t.to_string())
    }
}

impl Debug for StringErr {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self)
    }
}

impl Deref for StringErr {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
