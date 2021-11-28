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

use crate::utils::JNIUtil;
use crate::rp::Rp;
use crate::{StringErr, ToVmType};

pub fn get_memory(
    env: JNIEnv,
    descriptor: jlong,
    off: jint,
    len: jint,
) -> Result<jbyteArray, StringErr> {
    unsafe {
        let ins = crate::get_ins_by_id(descriptor as usize);
        let mem = ins.exports.get_memory("memory")?;
        if (off + len) as u64 > mem.data_size() || off < 0 || len < 0 {
            return Err(StringErr("memory access overflow".into()));
        }
        let slice = &mem.data_unchecked()[(off as usize)..(off + len) as usize];
        Ok(env.byte_array_from_slice(slice)?)
    }
}

pub fn set_memory(env: JNIEnv, descriptor: jlong, off: jint, buf: jbyteArray) -> Result<(), StringErr> {
    unsafe {
        let ins = crate::get_ins_by_id(descriptor as usize);
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

pub fn close(env: JNIEnv, descriptor: jlong) -> Result<(), StringErr> {
    unsafe {
        let mut ins: Rp<Instance> = (descriptor as usize).into();

        if ins.is_null() {
            return Ok(());
        }
        ins.drop();
    }
    Ok(())
}


pub fn create_host(store: &wasmer::Store, sig: (Vec<Type>, Vec<Type>), jvm: jni::JavaVM, ins: jint, host_id: jint) -> Function {
    let host_function_signature = FunctionType::new(sig.0.clone(), sig.1.clone());
    Function::new(store, &host_function_signature, move |_args| {
        let ret_types = sig.1.clone();
        let env: JNIEnv = as_rt!(jvm.get_env());
        let v = as_i64_vec!(_args, RuntimeError::new("unexpected param type"));
        let arr = env.call_static_method("com/github/salpadding/wasmer/Natives", "onHostFunction", "(II[J)[J", &[
            JValue::Int(ins),
            JValue::Int(host_id),
            JValue::Object(as_rt!(env.slice_to_jlong_array(&v)).into()),
        ],
        );

        let arr = as_rt!(arr);
        let o = match arr {
            JValue::Object(o) => o,
            _ => return Err(RuntimeError::new("unexpected return type")),
        };

        let v = env.jlong_array_to_vec(o.into_inner());
        let v = as_rt!(v);
        ret_types.convert(v)
    })
}

pub fn execute(
    env: JNIEnv,
    id: jlong,
    _method: jstring,
    args: jlongArray,
) -> Result<jlongArray, StringErr> {
    unsafe {
        let ins = crate::get_ins_by_id(id as usize);

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
