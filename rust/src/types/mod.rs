macro_rules! get_or_err {
    ($v: expr, $id: expr, $msg: expr) => {
        $v.get($id).ok_or::<String>($msg.into())?
    };
}

macro_rules! ok_or_err {
    ($v: expr, $msg: expr) => {
        $v.ok_or::<String>($msg.into())?
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
mod names;

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

        let mut ins = Instance::new(&buffer, 16000, 16000 * 16, 16000 * 16, 64).unwrap();
        ins.execute("bench", &[]).unwrap();
    }

    fn read_file(path: &str) -> Vec<u8> {
        let mut f = File::open(path).expect("no file found");
        let metadata = fs::metadata(path).expect("unable to read metadata");
        let mut buffer = vec![0; metadata.len() as usize];
        f.read(&mut buffer).expect("buffer overflow");
        buffer
    }

    fn from_expect(s: &str) -> u64 {
        let split: Vec<String> = s.split(":").map(|x| x.to_string()).collect();
        let t = &split[0];
        let v = &split[1];

        match t.as_str() {
            "f32" => {
                let x: f32 = v.parse().unwrap();
                x.to_bits() as u64
            }
            "i32" => {
                let x: i32 = v.parse().unwrap();
                x as u32 as u64
            }
            "i64" => {
                let x: i64 = v.parse().unwrap();
                x as u64
            }
            "f64" => {
                let x: f64 = v.parse().unwrap();
                x.to_bits()
            }
            _ => 0
        }
    }

    macro_rules! test_md {
        ($test_fn: ident, $md: expr) => {
            #[test]
            fn $test_fn () {
                let json_file = "src/testdata/modules.json";
                let module_file = $md;


                let json_str = String::from_utf8(read_file(json_file)).unwrap();
                let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();
                let obj = json.as_array().unwrap()
                        .iter().find(|x| x.as_object().unwrap().get("file").unwrap().as_str().unwrap() == module_file).unwrap();

                let tests = obj.get("tests").unwrap().as_array().unwrap();
                let path = format!("src/testdata/{}", module_file);

                let mut ins = Instance::new(&read_file(&path), 16000, 16000 * 16, 16000 * 16, 64).unwrap();

                for test in tests {
                    let func_name = test.as_object().unwrap().get("function").unwrap().as_str().unwrap();
                    let expect = test.as_object().unwrap().get("return").unwrap().as_str().unwrap();
                    let expect = from_expect(expect);
                    assert_eq!(ins.execute(func_name, &[]).unwrap(), expect)
                }
                println!("test passed for module {}", $md);
            }

        };
    }

    test_md!(test_br, "br.wasm");
    test_md!(test_bug_49, "bug-49.wasm");
    test_md!(test_br_if, "brif.wasm");
    test_md!(test_br_if_loop, "brif-loop.wasm");
    test_md!(test_expr_block, "expr-block.wasm");
}