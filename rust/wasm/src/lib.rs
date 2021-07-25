extern "C" {
    fn alert(x: u64);
    fn __peek(off: u32, len: u32);
}

#[no_mangle]
pub fn init(x: u64, y: u64) {
    unsafe {
        let i: Vec<u8> = (0xffff0000u32).to_be_bytes().to_vec();
        let raw = i.as_ptr();
        let ret = (raw as usize) as u64;
        std::mem::forget(i);        

        // let host modify the memory
        __peek(raw as usize as u32, 4u32);

        let i = unsafe {
            let raw = raw as *mut u8;
            Vec::from_raw_parts(raw, 4, 4)
        };
        
        let mut be = [0u8; 4];
        be.copy_from_slice(&i);
        alert(u32::from_be_bytes(be) as u64);
        alert(0xffff0000u64);
        alert(919191919);
    }
}