use parity_wasm::elements::ResizableLimits;

use crate::StringErr;

#[derive(Default)]
pub struct Memory {
    pub(crate) data: Vec<u8>,
    pub(crate) initial: u32,
    pub(crate) maximum: Option<u32>,
    pub(crate) max_pages: u32,
    pub(crate) pages: u32,
}

macro_rules! min {
    ($x: expr, $y: expr) => {{
        if $x < $y {
            $x
        } else {
            $y
        }
    }};
}

macro_rules! memory_load_n {
    ($fn: ident, $t: ident, $bytes: expr) => {
        pub(crate) fn $fn(&self, off: u32) -> Result<$t, String> {
            if (off + $bytes) as usize > self.data.len() {
                return Err("memory access overflow".into());
            }

            let mut buf = [0u8; $bytes];
            buf.copy_from_slice(&self.data[off as usize..(off + $bytes) as usize]);
            Ok($t::from_le_bytes(buf))
        }
    };
}

macro_rules! memory_store_n {
    ($fn: ident, $t: ident, $bytes: expr) => {
        pub(crate) fn $fn(&mut self, off: u32, value: $t) -> Result<(), String> {
            if (off + $bytes) as usize > self.data.len() {
                return Err("memory access overflow".into());
            }

            let buf = value.to_le_bytes();
            self.data[off as usize..(off + $bytes) as usize].copy_from_slice(&buf);
            Ok(())
        }
    };
}

// validate: max_pages should <= 0xFFFF and not be 0
const MAX_PAGES: u32 = 0xFFFF;
const PAGE_SIZE: u32 = 64 * (1u32 << 10); // 64 KB

impl Memory {
    memory_load_n!(load_u64, u64, 8);
    memory_load_n!(load_u32, u32, 4);
    memory_load_n!(load_u16, u16, 2);
    memory_load_n!(load_u8, u8, 1);

    memory_store_n!(store_u64, u64, 8);
    memory_store_n!(store_u32, u32, 4);
    memory_store_n!(store_u16, u16, 2);
    memory_store_n!(store_u8, u8, 1);

    pub(crate) fn init(&mut self, limits: &ResizableLimits) -> Result<(), StringErr> {
        self.initial = limits.initial();
        if self.initial > MAX_PAGES {
            return Err(StringErr::new(format!("initial page too large: {}", self.initial)));
        }
        self.data = vec![0u8; (self.initial * PAGE_SIZE) as usize];
        self.maximum = limits.maximum();
        Ok(())
    }

    pub fn read(&self, off: usize, dst: &mut [u8]) -> Result<(), String> {
        if off + dst.len() > self.data.len() {
            return Err("Memory.read(): memory access overflow".into());
        }
        dst.copy_from_slice(&self.data[off..off + dst.len()]);
        Ok(())
    }

    pub fn write(&mut self, off: usize, dst: &[u8]) -> Result<(), String> {
        if off + dst.len() > self.data.len() {
            return Err("Memory.write(): memory access overflow".into());
        }
        self.data[off..off + dst.len()].copy_from_slice(dst);
        Ok(())
    }

    pub(crate) fn grow(&mut self, n: u32) -> Result<u32, String> {
        match self.maximum {
            None => {}
            Some(max) => {
                if self.pages + n > max {
                    return Ok(-1i32 as u32);
                }
            }
        }

        if self.pages + n > self.max_pages {
            return Err(format!(
                "memory overflow: cannot grow any more, current pages = {} n = {} maximum = {}",
                self.pages, n, self.max_pages
            ));
        }

        let mut v: Vec<u8> = vec![0; ((self.pages + n) * PAGE_SIZE) as usize];
        v[..self.data.len()].copy_from_slice(&self.data);
        self.data = v;
        let prev = self.pages;
        self.pages += n;
        Ok(prev)
    }
}
