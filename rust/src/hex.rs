use alloc::vec::*;
use alloc::string::*;

static CHARS: [char; 16] = ['0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c', 'd', 'e', 'f' ];
static CHARS_INV: [u8; 104] = [
    0,0,0,0,    0,0,0,0, // 0x08
    0,0,0,0,    0,0,0,0, // 0x10
    0,0,0,0,    0,0,0,0, // 0x18
    0,0,0,0,    0,0,0,0, // 0x20
    0,0,0,0,    0,0,0,0, // 0x28
    0,0,0,0,    0,0,0,0, // 0x30
    0,1,2,3,    4,5,6,7, // 0x38
    8,9,0,0,    0,0,0,0, // 0x40
    0,10,11,12, 13,14,15,0, // 0x48
    0,0,0,0,    0,0,0,0,  // 0x50
    0,0,0,0,    0,0,0,0, // 0x58
    0,0,0,0,    0,0,0,0, // 0x60
    0,10,11,12, 13,14,15,0 // 0x68
];


pub fn to_hex(data: &[u8]) -> String {
    let mut s = String::with_capacity(data.len() * 2);
    for i in data {
        s.push(CHARS[((i >> 4) & 0x0f) as usize]);
        s.push(CHARS[(i & 0x0f) as usize]);
    }
    s
}

pub fn decode_hex(data: &str) -> Vec<u8> {
    let ascii = data.as_bytes();
    if ascii.len() % 2 != 0 {
        panic!("odd hex string {}\n", data);
    }
    let mut r: Vec<u8> = Vec::with_capacity(data.len() / 2);
    let mut j: u8 = 0;
    let mut i: usize = 0;
    for x in ascii {
        if i % 2 != 0 {
            j = j | CHARS_INV[*x as usize];
            r.push(j);
            j = 0;
        } else {
            j = CHARS_INV[*x as usize] << 4;
        }
        i += 1;
    }
    r
}