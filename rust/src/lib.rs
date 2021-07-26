#![feature(unchecked_math)] // allow unchecked math
#![allow(warnings)]

#[macro_use]
extern crate lazy_static;
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
use jni::sys::{jbyteArray, jint, jlong, jlongArray, jobjectArray, jstring};
use parity_wasm::SerializationError;
use wasmer::{
    CompileError, ExportError, Exports, Features, Function, FunctionType, ImportObject, imports,
    Instance, InstantiationError, Module, RuntimeError, Store, Type, Value,
};
use wasmer_compiler_singlepass::Singlepass;
use wasmer_engine_universal::Universal;

use utils::{JNIUtil, ToVmType};

mod types;
mod hex;
mod utils;

const MAX_INSTANCES: usize = 1024;

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
    _signatures: jobjectArray,
) -> jint {
    jni_ret!(
        create_instance(env, _class, _module, _features, _hosts, _signatures),
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
    _id: jint,
    _method: jstring,
    _args: jlongArray,
) -> jlongArray {
    jni_ret!(execute(env, _id, _method, _args), env, null_mut())
}

#[no_mangle]
pub extern "system" fn Java_org_github_salpadding_wasmer_Natives_close(
    env: JNIEnv,
    // this is the class that owns our
    // static method. Not going to be
    // used, but still needs to have
    // an argument slot
    _class: JClass,
    _id: jint,
) {
    jni_ret!(close(env, _id), env, ())
}

#[no_mangle]
pub extern "system" fn Java_org_github_salpadding_wasmer_Natives_getMemory(
    env: JNIEnv,
    // this is the class that owns our
    // static method. Not going to be
    // used, but still needs to have
    // an argument slot
    _class: JClass,
    _id: jint,
    off: jint,
    len: jint,
) -> jbyteArray {
    jni_ret!(get_memory(env, _id, off, len), env, null_mut())
}

#[no_mangle]
pub extern "system" fn Java_org_github_salpadding_wasmer_Natives_setMemory(
    env: JNIEnv,
    // this is the class that owns our
    // static method. Not going to be
    // used, but still needs to have
    // an argument slot
    _class: JClass,
    _id: jint,
    off: jint,
    buf: jbyteArray,
) {
    jni_ret!(set_memory(env, _id, off, buf), env, ())
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

macro_rules! get_ins_by_id {
    ($id: expr) => {{
        INSTANCES
            .get($id as usize)
            .and_then(|x| x.as_ref())
            .ok_or(StringErr("instance not found".into()))?
    }};
}

fn create_instance(
    env: JNIEnv,
    _class: JClass,
    _module: jbyteArray,
    _features: jlong,
    _hosts: jobjectArray,
    _signatures: jobjectArray,
) -> Result<jint, StringErr> {
    unsafe {
        let hosts = env.jstring_array_to_vec(_hosts)?;
        let sigs = env.jbytes_array_to_vec(_signatures)?;

        let sigs: Vec<(Vec<Type>, Vec<Type>)> = {
            let mut r = Vec::new();

            for s in sigs {
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

        if INSTANCES.is_empty() {
            for _ in 0..MAX_INSTANCES {
                INSTANCES.push(None);
            }
        }

        for i in 0..MAX_INSTANCES {
            let m = &mut INSTANCES[i];

            match m {
                None => {
                    let descriptor = i;
                    // Use Singlepass compiler with the default settings
                    let compiler = Singlepass::default();
                    let mut features = Features::new();
                    let mask = _features as u64;

                    set_mask!(
                        mask, features,
                        threads, reference_types, simd, bulk_memory, multi_value,
                        tail_call, module_linking, multi_memory, memory64
                    );

                    // Create the store
                    let store = Store::new(&Universal::new(compiler).features(features).engine());
                    let bytes = env.convert_byte_array(_module)?;
                    let module = Module::new(&store, bytes)?;


                    let mut import_object = ImportObject::new();
                    let mut namespace = Exports::new();

                    for i in 0..hosts.len() {
                        let name = hosts[i].clone();
                        let n2 = name.clone();
                        let jvm = env.get_java_vm()?;
                        let s = sigs[i].clone();
                        let host_function_signature = FunctionType::new(s.clone().0, s.clone().1);
                        let host_function =
                            Function::new(&store, &host_function_signature, move |_args| {
                                let return_types = s.clone().1;
                                let env = as_rt!(jvm.get_env());
                                let jstr = as_rt!(env.new_string(name.clone()));
                                let v =
                                    as_i64_vec!(_args, RuntimeError::new("unexpected param type"));

                                let arr = env.call_static_method(
                                    "org/github/salpadding/wasmer/Natives",
                                    "onHostFunction",
                                    "(ILjava/lang/String;[J)[J",
                                    &[
                                        JValue::Int(descriptor as i32),
                                        JValue::Object(jstr.into()),
                                        JValue::Object(
                                            as_rt!(env.slice_to_jlong_array(&v)).into(),
                                        ),
                                    ],
                                );
                                let arr = as_rt!(arr);

                                let o = match arr {
                                    JValue::Object(o) => o,
                                    _ => return Err(RuntimeError::new("unexpected return type")),
                                };

                                let v = env.jlong_array_to_vec(o.into_inner());
                                let v = as_rt!(v);
                                return_types.convert(v)
                            });
                        namespace.insert(n2, host_function);
                    }

                    import_object.register("env", namespace);

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
        let ins = get_ins_by_id!(id as usize);

        let method = env.get_string(_method.into())?;
        let s = method.to_str()?;
        let fun = ins.exports.get_function(s)?;
        let sig = fun.get_vm_function().signature.clone();

        let a: Vec<i64> = env.jlong_array_to_vec(args)?;

        if sig.params().len() != a.len() {
            return Err(StringErr("invalid params length".into()));
        }

        let a = &sig.params().convert(a)?;
        let results = fun.call(&a)?;
        let results = as_i64_vec!(results, StringErr("unsupported return type".into()));

        return env.slice_to_jlong_array(&results);
    }
}

fn close(env: JNIEnv, descriptor: jint) -> Result<(), StringErr> {
    unsafe {
        if descriptor as usize > INSTANCES.len() {
            return Ok(());
        }
        INSTANCES[descriptor as usize] = None;
        Ok(())
    }
}

fn get_memory(
    env: JNIEnv,
    descriptor: jint,
    off: jint,
    len: jint,
) -> Result<jbyteArray, StringErr> {
    unsafe {
        let ins = get_ins_by_id!(descriptor as usize);

        let mut buf = vec![0u8; len as usize];
        let mem = ins.exports.get_memory("memory")?;
        if (off + len) as usize > mem.data_unchecked().len() {
            return Err(StringErr("memory access overflow".into()));
        }
        buf.copy_from_slice(&mem.data_unchecked()[(off as usize)..(off + len) as usize]);
        Ok(env.byte_array_from_slice(&buf)?)
    }
}

fn set_memory(
    env: JNIEnv,
    descriptor: jint,
    off: jint,
    buf: jbyteArray,
) -> Result<(), StringErr> {
    unsafe {
        let ins = get_ins_by_id!(descriptor as usize);
        let bytes = env.convert_byte_array(buf)?;
        let mem = ins.exports.get_memory("memory")?;
        if (off as usize + bytes.len()) as usize > mem.data_unchecked().len() {
            return Err(StringErr("memory access overflow".into()));
        }
        let mutable = mem.data_unchecked_mut();
        mutable[off as usize..off as usize + bytes.len()].copy_from_slice(&bytes);
        Ok(())
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
impl_from!(SerializationError);
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