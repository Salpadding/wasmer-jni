macro_rules! get_or_err {
    ($v: expr, $id: expr, $msg: expr) => {
        match $v.get($id) {
            None => return Err(StringErr::new($msg)),
            Some(o) => o
        }
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

    trait ToBits {
        fn to_bits(&self) -> u64;
    }

    impl ToBits for String {
        fn to_bits(&self) -> u64 {
            from_expect(&self)
        }
    }


    #[derive(Serialize, Deserialize)]
    struct TestFunction {
        function: String,
        r#return: String,
        #[serde(default)]
        args: Vec<String>
    }

    #[derive(Serialize, Deserialize)]
    struct TestFile {
        file: String,
        tests: Vec<TestFunction>,
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
                if v.starts_with("-") {
                    let x: i32 = v.parse().unwrap();
                    x as u32 as u64
                } else {
                    let x: u32 = v.parse().unwrap();
                    x as u64
                }
            }
            "i64" => {
                if v.starts_with("-") {
                    let x: i64 = v.parse().unwrap();
                    x as u64
                } else {
                    let x: u64 = v.parse().unwrap();
                    x
                }
            }
            "f64" => {
                let x: f64 = v.parse().unwrap();
                x.to_bits()
            }
            _ => 0
        }
    }

    fn test_no_spec(module_file: &'static str) {
        test_wasm_file("src/testdata", "src/testdata/modules.json", module_file);
    }

    fn test_wasm_file(dir: &'static str, json_file: &'static str, module_file: &'static str) {
        let json_str = String::from_utf8(read_file(json_file)).unwrap();
        let json: Vec<TestFile> = serde_json::from_str(&json_str).unwrap();

        let obj = json
            .iter().find(|x| &x.file == module_file).unwrap();

        let path = format!("{}/{}", dir, module_file);

        let mut ins = Instance::new(&read_file(&path), 16000, 16000 * 16, 16000 * 16, 64).unwrap();

        for test in &obj.tests {
            let args: Vec<u64> = test.args.iter().map(|x| x.to_bits()).collect();
            assert_eq!(
                ins.execute(&test.function, &args).unwrap(),
                test.r#return.to_bits(),
                "test failed for file: {} func: {}", &obj.file, &test.function
            )
        }
    }

    #[test]
    fn test_basic(){
        test_no_spec("basic.wasm");
    }

    #[test]
    fn test_binary(){
        test_no_spec("binary.wasm");
    }

    #[test]
    fn test_brif_loop(){
        test_no_spec("brif-loop.wasm");
    }

    #[test]
    fn test_brif(){
        test_no_spec("brif.wasm");
    }

    #[test]
    fn test_br(){
        test_no_spec("br.wasm");
    }

    #[test]
    fn test_call(){
        test_no_spec("call.wasm");
    }

    #[test]
    fn test_call_zero_args(){
        test_no_spec("call-zero-args.wasm");
    }

    #[test]
    fn test_call_indirect(){
        test_no_spec("callindirect.wasm");
    }

    #[test]
    fn test_cast(){
        test_no_spec("cast.wasm");
    }

    #[test]
    fn test_compare(){
        test_no_spec("compare.wasm");
    }

    #[test]
    fn test_convert(){
        test_no_spec("convert.wasm");
    }

    #[test]
    fn test_expr_block(){
        test_no_spec("expr-block.wasm");
    }

    #[test]
    fn test_expr_brif(){
        test_no_spec("expr-brif.wasm");
    }

    #[test]
    fn test_expr_br(){
        test_no_spec("expr-br.wasm");
    }

    #[test]
    fn test_expr_if(){
        test_no_spec("expr-if.wasm");
    }

    #[test]
    fn test_if(){
        test_no_spec("if.wasm");
    }

    #[test]
    fn test_load(){
        test_no_spec("load.wasm");
    }

    #[test]
    fn test_loop(){
        test_no_spec("loop.wasm");
    }

    #[test]
    fn test_nested_if(){
        test_no_spec("nested-if.wasm");
    }

    #[test]
    fn test_return(){
        test_no_spec("return.wasm");
    }

    #[test]
    fn test_select(){
        test_no_spec("select.wasm");
    }

    #[test]
    fn test_start(){
        test_no_spec("start.wasm");
    }

    #[test]
    fn test_store(){
        test_no_spec("store.wasm");
    }

    #[test]
    fn test_unary(){
        test_no_spec("unary.wasm");
    }

    #[test]
    fn test_bug_49(){
        test_no_spec("bug-49.wasm");
    }

    #[test]
    fn test_rs_basic(){
        test_no_spec("rust-basic.wasm");
    }
}