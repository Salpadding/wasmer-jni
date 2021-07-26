// label data = stack pc (2byte) | label pc (2byte) | 0x00  | 0x00  | arity (1byte) | loop (1byte)
#[derive(Clone, Copy)]
pub(crate) struct LabelData(pub(crate) u64);

const STACK_PC_MASK: u64 = 0xffff000000000000;
const STACK_PC_SHIFTS: usize = 48;

const LABEL_PC_MASK: u64 = 0x0000ffff00000000;
const LABEL_PC_SHIFTS: usize = 32;

const START_PC_MASK: u64 = 0x00000000ffff0000;
const START_PC_SHIFTS: usize = 16;

const ARITY_MASK: u64 = 2;
const ARITY_SHIFTS: usize = 1;

const LOOP_MASK: u64 = 1;

impl LabelData {
    pub(crate) fn stack_pc(&self) -> u16 {
        ((self.0 & STACK_PC_MASK) >> STACK_PC_SHIFTS) as u16
    }

    pub(crate) fn label_pc(&self) -> u16 {
        ((self.0 & LABEL_PC_MASK) >> LABEL_PC_SHIFTS) as u16
    }

    pub(crate) fn start_pc(&self) -> u16 {
        ((self.0 & START_PC_MASK) >> START_PC_SHIFTS) as u16
    }

    pub(crate) fn arity(&self) -> bool {
        (self.0 & ARITY_MASK) != 0
    }

    pub(crate) fn is_loop(&self) -> bool {
        (self.0 & LOOP_MASK) != 0
    }

    pub(crate) fn new(stack_pc: u16, label_pc: u16, start_pc: u16, arity: bool, is_loop: bool) -> LabelData {
        let o = ((stack_pc as u64) << STACK_PC_SHIFTS)
            | ((label_pc as u64) << LABEL_PC_SHIFTS)
            | ((start_pc as u64) << START_PC_SHIFTS)
            | ((arity as u64) << ARITY_SHIFTS)
            | (is_loop as u64);
        LabelData(o)
    }
}
