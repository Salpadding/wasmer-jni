use jni::{JNIEnv, sys::jlongArray};

use crate::StringErr;

pub trait JNIUtil {
    fn jlong_array_to_vec(&self, arr: jlongArray) -> Result<Vec<i64>, StringErr>;

    fn slice_to_jlong_array(&self, arr: &[i64]) -> Result<jlongArray, StringErr>;
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
        let arr = self.get_long_array_elements(o, jni::objects::ReleaseMode::NoCopyBack)?;
        
        for i in 0..slice.len() {
            unsafe { *arr.as_ptr().offset(i as isize) = slice[i] };
        }

        Ok(o)
    }

    
}


