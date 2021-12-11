pub use super::super::engines_common::*;

pub static COND_STRINGS: [&str; 16] = [
    "eq", "ne", "cs", "cc", "mi", "pl", "vs", "vc", "hi", "ls", "ge", "lt", "gt", "le", "", "nv",
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DpOpSpecialTy {
    Add,
    Cmp,
    Mov,
}
