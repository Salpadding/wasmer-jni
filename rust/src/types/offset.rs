const STACK_BASE_MASK: u64 = 0x7fffffff;
const STACK_BASE_SHIFTS: u32 = 0;
const LABEL_BASE_MASK: u64 = 0x7fffffff00000000;
const LABEL_BASE_SHIFTS: u32 = 32;

#[derive(Clone, Copy, Default)]
pub(crate) struct Offset(u64);

impl Offset {
    pub(crate) fn label_base(&self) -> u32 {
        ((self.0 & LABEL_BASE_MASK) >> LABEL_BASE_SHIFTS) as u32
    }
    pub(crate) fn stack_base(&self) -> u32 {
        ((self.0 & STACK_BASE_MASK) >> STACK_BASE_SHIFTS) as u32
    }
    pub(crate) fn new(label_base: u32, stack_base: u32) -> Self {
        Offset((label_base as u64) << 32 | (stack_base as u64))
    }
}
