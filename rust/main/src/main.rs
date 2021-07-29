use std::fs::File;
use std::fs;
use std::time::SystemTime;
use std::io::Read;
use wasmer_jni::types::instance::Instance;

fn main() {
    let filename = "src/testdata/main.wasm";
    let mut f = File::open(filename).expect("no file found");
    let metadata = fs::metadata(filename).expect("unable to read metadata");
    let mut buffer = vec![0; metadata.len() as usize];
    f.read(&mut buffer).expect("buffer overflow");

    let mut ins = Instance::new(&buffer, 16000, 16000 * 16, 16000 * 16, 64).unwrap();

    let loops = 1;

    let start = SystemTime::now();

    for _ in 0..loops {
        ins.execute("bench", &[]).unwrap();
        ins.clear();
    }

    let end = SystemTime::now();
    let dur = end.duration_since(start);
    println!("ops = {} ", (loops as f64) * 1000.0 / (dur.unwrap().as_millis() as f64))
}