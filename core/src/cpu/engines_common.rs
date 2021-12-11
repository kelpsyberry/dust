#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShiftTy {
    Lsl,
    Lsr,
    Asr,
    Ror,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShiftImmTy {
    Lsl,
    Lsr,
    Asr,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DpOpImm8Ty {
    Mov,
    Cmp,
    Add,
    Sub,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DpOpRegTy {
    And,
    Eor,
    Lsl,
    Lsr,
    Asr,
    Adc,
    Sbc,
    Ror,
    Tst,
    Neg,
    Cmp,
    Cmn,
    Orr,
    Mul,
    Bic,
    Mvn,
}

impl DpOpRegTy {
    pub const fn is_unary(self) -> bool {
        matches!(self, Self::Neg | Self::Mvn)
    }

    pub const fn is_shift(self) -> bool {
        matches!(self, Self::Lsl | Self::Lsr | Self::Asr | Self::Ror)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DpOpTy {
    And,
    Eor,
    Sub,
    Rsb,
    Add,
    Adc,
    Sbc,
    Rsc,
    Tst,
    Teq,
    Cmp,
    Cmn,
    Orr,
    Mov,
    Bic,
    Mvn,
}

impl DpOpTy {
    pub const fn is_test(self) -> bool {
        matches!(self, Self::Tst | Self::Teq | Self::Cmp | Self::Cmn)
    }

    pub const fn is_unary(self) -> bool {
        matches!(self, Self::Mov | Self::Mvn)
    }

    pub const fn sets_carry(self) -> bool {
        match self {
            Self::And
            | Self::Eor
            | Self::Tst
            | Self::Teq
            | Self::Orr
            | Self::Mov
            | Self::Bic
            | Self::Mvn => false,
            Self::Sub
            | Self::Rsb
            | Self::Add
            | Self::Adc
            | Self::Sbc
            | Self::Rsc
            | Self::Cmp
            | Self::Cmn => true,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DpOperand {
    Imm,
    Reg { shift_ty: ShiftTy, shift_imm: bool },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WbAddressing {
    Post,
    PostUser,
    PreNoWb,
    Pre,
}

impl WbAddressing {
    pub const fn preincrement(self) -> bool {
        matches!(self, WbAddressing::PreNoWb | WbAddressing::Pre)
    }

    pub const fn writeback(self) -> bool {
        !matches!(self, WbAddressing::PreNoWb)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WbOffTy {
    Imm,
    Reg(ShiftTy),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MiscAddressing {
    Post,
    PreNoWb,
    Pre,
}

impl MiscAddressing {
    pub const fn preincrement(self) -> bool {
        !matches!(self, MiscAddressing::Post)
    }

    pub const fn writeback(self) -> bool {
        !matches!(self, MiscAddressing::PreNoWb)
    }
}
