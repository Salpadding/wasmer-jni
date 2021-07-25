use core::panic;

use jni::{JNIEnv, sys::{jlongArray, jobjectArray}};
use wasmer::{RuntimeError, Type, Val, Value};

use crate::StringErr;

pub trait ToVmType {
    fn convert(&self, src: Vec<i64>) -> Result<Vec<Val>, RuntimeError>;
}


impl ToVmType for Vec<Type> {
    fn convert(&self, src: Vec<i64>) -> Result<Vec<Val>, RuntimeError> {
        let mut r: Vec<Val> = Vec::new();
        for i in 0..src.len() {
            let v = 
            match self[i] {
                Type::I32 => Value::I32(src[i] as u64 as u32 as i32),
                Type::F32 => Value::F32(f32::from_bits(src[i] as u64 as u32)),
                Type::I64 => Value::I64(src[i]),
                Type::F64 => Value::F64(f64::from_bits(src[i] as u64)),
                _ => return Err(RuntimeError::new("unexpected type"))
            };

            r.push(v);
        }

        Ok(r)
    }
}


pub trait JNIUtil {
    fn jlong_array_to_vec(&self, arr: jlongArray) -> Result<Vec<i64>, StringErr>;

    fn slice_to_jlong_array(&self, arr: &[i64]) -> Result<jlongArray, StringErr>;

    fn jstring_array_to_vec(&self, arr: jobjectArray) -> Result<Vec<String>, StringErr> ;
    

    fn jbytes_array_to_vec(&self, arr: jobjectArray) -> Result<Vec<Vec<u8>>, StringErr>;
}

impl JNIUtil for JNIEnv<'_> {
    fn jlong_array_to_vec(&self, arr: jlongArray) -> Result<Vec<i64>, StringErr> {
        if arr.is_null() {
            return Ok(Vec::new());
        }

        let arr = self.get_long_array_elements(arr, jni::objects::ReleaseMode::CopyBack)?;
                    
        let v: Vec<i64> = (0..arr.size()?).map(|i| unsafe { *arr.as_ptr().offset(i as isize) } ).collect();
        Ok(v)
    }

    fn slice_to_jlong_array(&self, slice: &[i64]) -> Result<jlongArray, StringErr> {
        let o = self.new_long_array(slice.len() as i32)?;
        self.set_long_array_region(o, 0, slice)?;        
        Ok(o)
    }

    fn jstring_array_to_vec(&self, arr: jobjectArray) -> Result<Vec<String>, StringErr>  {
        if arr.is_null() {
            return Ok(Vec::new());
        }


        let len = self.get_array_length(arr)?;
        let mut v: Vec<String> = Vec::with_capacity(len as usize);

        for i in 0..len {
            let o = self.get_object_array_element(arr, i)?;
            let s = self.get_string(o.into_inner().into())?;
            v.push(s.to_str()?.into());
        }

        Ok(v)
    }

    fn jbytes_array_to_vec(&self, arr: jobjectArray) -> Result<Vec<Vec<u8>>, StringErr> {
        if arr.is_null() {
            return Ok(Vec::new());
        }


        let len = self.get_array_length(arr)?;
        let mut v: Vec<Vec<u8>> = Vec::with_capacity(len as usize);

        for i in 0..len {
            let o = self.get_object_array_element(arr, i)?;
            let bytes = self.convert_byte_array(o.into_inner())?;
            v.push(bytes);
        }

        Ok(v)
    }

    
}


