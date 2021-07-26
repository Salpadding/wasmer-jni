// label size (2byte) | local size (2byte) | stack size (2byte) | function index (2byte)
#[derive(Clone, Copy)]
pub(crate) struct FrameData(pub(crate) u64);

#[derive(Clone, Copy)]
pub(crate) struct FunctionBits(pub(crate) u16);

impl Default for FunctionBits {
    fn default() -> Self {
        FunctionBits(0)
    }
}


pub(crate) const FN_INDEX_MASK: u16 = 0x7fff;
const IS_TABLE_MASK: u16 = 0x8000;

impl FunctionBits {
    pub(crate) fn is_table(&self) -> bool {
        self.0 & IS_TABLE_MASK != 0
    }

    pub(crate) fn fn_index(&self) -> u16 {
        self.0 & FN_INDEX_MASK
    }
}

const LABEL_SIZE_MASK: u64 = 0xffff000000000000;
const LABEL_SIZE_SHIFTS: usize = 48;
const LOCAL_SIZE_MASK: u64 = 0x0000ffff00000000;
const LOCAL_SIZE_SHIFTS: usize = 32;
const STACK_SIZE_MASK: u64 = 0x00000000ffff0000;
const STACK_SIZE_SHIFTS: usize = 16;
const FUNCTION_BITS_MASK: u64 = 0x000000000000ffff;
const FUNCTION_BITS_SHIFTS: usize = 0;

// validate: function size <= FN_INDEX_MASK
impl FrameData {
    pub(crate) fn label_size(&self) -> u16 {
        ((self.0 & LABEL_SIZE_MASK) >> LABEL_SIZE_SHIFTS) as u16
    }

    pub(crate) fn local_size(&self) -> u16 {
        ((self.0 & LOCAL_SIZE_MASK) >> LOCAL_SIZE_SHIFTS) as u16
    }

    pub(crate) fn stack_size(&self) -> u16 {
        ((self.0 & STACK_SIZE_MASK) >> STACK_SIZE_SHIFTS) as u16
    }

    pub(crate) fn func_bits(&self) -> FunctionBits {
        FunctionBits(((self.0 & FUNCTION_BITS_MASK) >> FUNCTION_BITS_SHIFTS) as u16)
    }

    pub(crate) fn new(label_size: u16, local_size: u16, stack_size: u16, func_bits: FunctionBits) -> Self {
        let n = ((label_size as u64) << LABEL_SIZE_SHIFTS)
            | ((local_size as u64) << LOCAL_SIZE_SHIFTS)
            | ((stack_size as u64) << STACK_SIZE_SHIFTS)
            | (func_bits.0 as u64);
        FrameData(n)
    }
}

