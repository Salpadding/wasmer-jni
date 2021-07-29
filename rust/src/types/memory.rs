use parity_wasm::elements::ResizableLimits;

use crate::StringErr;

#[derive(Default)]
pub struct Memory {
    pub(crate) data: Vec<u8>,
    pub(crate) initial: u32,
    pub(crate) maximum: Option<u32>,
    pub(crate) max_pages: u32,
    pub(crate) pages: u32,
    pub(crate) buf_8: [u8; 8],
    pub(crate) buf_4: [u8; 4],
    pub(crate) buf_2: [u8; 2],
}

macro_rules! dec_u64 {
    ($slice: expr, $off: expr) => {
        {
            ($slice[$off] as u64) |
            (($slice[$off + 1] as u64) << 8) |
            (($slice[$off + 2] as u64) << 16) |
            (($slice[$off + 3] as u64) << 24) |
            (($slice[$off + 4] as u64) << 32) |
            (($slice[$off + 5] as u64)) << 40 |
            (($slice[$off + 6] as u64)) << 48 |
            (($slice[$off + 7] as u64)) << 56
        }
    };
}

macro_rules! enc_u64 {
    ($slice: expr, $off: expr, $v: expr) => {
        {
            $slice[$off] = $v as u8;
            $slice[$off + 1] = ($v >> 8) as u8;
            $slice[$off + 2] = ($v >> 16) as u8;
            $slice[$off + 3] = ($v >> 24) as u8;
            $slice[$off + 4] = ($v >> 32) as u8;
            $slice[$off + 5] = ($v >> 40) as u8;
            $slice[$off + 6] = ($v >> 48) as u8;
            $slice[$off + 7] = ($v >> 56) as u8;
        }
    };
}

macro_rules! enc_u32 {
    ($slice: expr, $off: expr, $v: expr) => {
        {
            $slice[$off] = $v as u8;
            $slice[$off + 1] = ($v >> 8) as u8;
            $slice[$off + 2] = ($v >> 16) as u8;
            $slice[$off + 3] = ($v >> 24) as u8;
        }
    };
}

macro_rules! enc_u16 {
    ($slice: expr, $off: expr, $v: expr) => {
        {
            $slice[$off] = $v as u8;
            $slice[$off + 1] = ($v >> 8) as u8;
        }
    };
}

macro_rules! enc_u8 {
    ($slice: expr, $off: expr, $v: expr) => {
        {
            $slice[$off] = $v;
        }
    };
}

macro_rules! dec_u32 {
    ($slice: expr, $off: expr) => {
        {
            ($slice[$off] as u32) |
            (($slice[$off + 1] as u32) << 8) |
            (($slice[$off + 2] as u32) << 16) |
            (($slice[$off + 3] as u32) << 24)
        }
    };
}

macro_rules! dec_u16 {
    ($slice: expr, $off: expr) => {
        {
            ($slice[$off] as u16) |
            (($slice[$off + 1] as u16) << 8)
        }
    };
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
    ($fn: ident, $t: ident, $bytes: expr, $dec: ident) => {
        pub(crate) fn $fn(&self, off: u32) -> Result<$t, String> {
            if (off + $bytes) as usize > self.data.len() {
                return Err(format!("memory access overflow off = {} buf len = {} data len = {}", off, $bytes, self.data.len()));
            }

            Ok($dec!(self.data, off as usize))
        }
    };
}

macro_rules! memory_store_n {
    ($fn: ident, $t: ident, $bytes: expr, $enc: ident) => {
        pub(crate) fn $fn(&mut self, off: u32, value: $t) -> Result<(), String> {
            if (off + $bytes) as usize > self.data.len() {
                return Err(format!("memory access overflow off = {} buf len = {} data len = {}", off, $bytes, self.data.len()));
            }

            $enc!(self.data, off as usize, value);
            Ok(())
        }
    };
}

// validate: max_pages should <= 0xFFFF and not be 0
const MAX_PAGES: u32 = 0xFFFF;
const PAGE_SIZE: u32 = 64 * (1u32 << 10); // 64 KB

impl Memory {
    memory_load_n!(load_u64, u64, 8, dec_u64);
    memory_load_n!(load_u32, u32, 4, dec_u32);
    memory_load_n!(load_u16, u16, 2, dec_u16);


    memory_store_n!(store_u64, u64, 8, enc_u64);
    memory_store_n!(store_u32, u32, 4, enc_u32);
    memory_store_n!(store_u16, u16, 2, enc_u16);
    memory_store_n!(store_u8, u8, 1, enc_u8);

    pub(crate) fn init(&mut self, limits: &ResizableLimits) -> Result<(), StringErr> {
        self.initial = limits.initial();
        if self.initial > MAX_PAGES || self.initial > self.max_pages {
            return Err(StringErr::new(format!("initial page too large: {}", self.initial)));
        }
        self.data = vec![0u8; (self.initial * PAGE_SIZE) as usize];
        self.pages = self.initial;
        self.maximum = limits.maximum();
        Ok(())
    }

    pub(crate) fn load_u8(&self, off: u32) -> Result<u8, String> {
        if (off + 1) as usize > self.data.len() {
            return Err(format!("memory access overflow off = {} buf len = {} data len = {}", off, 1, self.data.len()));
        }
        Ok(self.data[off as usize])
    }

    pub fn read(&self, off: usize, dst: &mut [u8]) -> Result<(), String> {
        if off + dst.len() > self.data.len() {
            return Err(format!("Memory.read(): memory access overflow off = {} buf len = {} memory len = {}", off, dst.len(), self.data.len()));
        }
        dst.copy_from_slice(&self.data[off..off + dst.len()]);
        Ok(())
    }

    pub fn write(&mut self, off: usize, dst: &[u8]) -> Result<(), String> {
        if off + dst.len() > self.data.len() {
            return Err(format!("Memory.read(): memory access overflow off = {} buf len = {} memory len = {}", off, dst.len(), self.data.len()));
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

    pub(crate) fn clear(&mut self) {
        self.data.fill(0);
    }
}
