use std::cmp::max;

use parity_wasm::elements::ResizableLimits;

use crate::types::instance::FunctionInstance;

#[derive(Default)]
pub(crate) struct Table {
    pub(crate) functions: Vec<Option<FunctionInstance>>,
}


// TODO: limit table size
impl Table {
    pub(crate) fn put_elements(&mut self, off: usize, functions: &[FunctionInstance]) {
        for i in 0..functions.len() {
            let index = off + i;

            if index >= self.functions.len() {
                let mut new_vec = vec![None; max(self.functions.len() * 2, index + 1)];
                for i in 0..self.functions.len() {
                    new_vec[i] = self.functions[i].clone()
                }
                self.functions = new_vec;
            }
            self.functions[index] = Some(functions[i].clone());
        }
    }
}