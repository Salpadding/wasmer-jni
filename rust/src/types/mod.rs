macro_rules! get_or_err {
    ($v: expr, $id: expr, $msg: expr) => {
        $v.get($id).ok_or::<String>($msg.into())?
    };
}

macro_rules! opt_to_vec {
    ($op: expr) => {
        {
            match $op {
                Some(x) => vec![x],
                _ => Vec::new()
            }
        }
    };
}

pub mod frame_data;
pub mod offset;
pub mod label_data;
pub mod memory;
pub mod executable;
pub mod instance;
mod table;
mod initializer;
mod ins_pool;


#[cfg(test)]
mod test {
    use std::fs;
    use std::fs::File;
    use std::io::Read;

    use crate::types::instance::Instance;

    #[test]
    fn test() {
        let filename = "src/testdata/main.wasm";
        let mut f = File::open(filename).expect("no file found");
        let metadata = fs::metadata(filename).expect("unable to read metadata");
        let mut buffer = vec![0; metadata.len() as usize];
        f.read(&mut buffer).expect("buffer overflow");

        let mut ins = Instance::new(&buffer, 16000, 16000 * 16, 16000 * 16).unwrap();
        ins.execute("bench", &[]).unwrap();
    }
}