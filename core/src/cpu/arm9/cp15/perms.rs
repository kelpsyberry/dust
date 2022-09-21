pub(super) type Perms = u8;
#[allow(clippy::module_inception)]
pub(super) mod perms {
    use super::Perms;

    pub const PRIV_R: Perms = 1 << 0;
    pub const PRIV_W: Perms = 1 << 1;
    pub const PRIV_X: Perms = 1 << 2;
    pub const PRIV_ALL: Perms = PRIV_R | PRIV_W | PRIV_X;

    pub const UNPRIV_R: Perms = 1 << 4;
    pub const UNPRIV_W: Perms = 1 << 5;
    pub const UNPRIV_X: Perms = 1 << 6;
    pub const UNPRIV_ALL: Perms = UNPRIV_R | UNPRIV_W | UNPRIV_X;

    pub const ALL: Perms = PRIV_ALL | UNPRIV_ALL;

    pub fn set_data_from_raw(perms: Perms, value: u8) -> Perms {
        match value {
            0 => perms & !(PRIV_R | PRIV_W | UNPRIV_R | UNPRIV_W),
            1 => (perms & !(UNPRIV_R | UNPRIV_W)) | (PRIV_R | PRIV_W),
            2 => (perms & !(UNPRIV_W)) | (PRIV_R | PRIV_W | UNPRIV_R),
            3 => perms | (PRIV_R | PRIV_W | UNPRIV_R | UNPRIV_W),
            5 => (perms & !(PRIV_W | UNPRIV_R | UNPRIV_W)) | PRIV_R,
            6 => (perms & !(PRIV_W | UNPRIV_W)) | (PRIV_R | UNPRIV_R),
            _ => unimplemented!("Unpredictable data access perms: {:#X}", value),
        }
    }

    pub fn set_code_from_raw(perms: Perms, value: u8) -> Perms {
        match value {
            0 => perms & !(PRIV_X | UNPRIV_X),
            1 | 5 => (perms & !UNPRIV_X) | PRIV_X,
            2 | 3 | 6 => perms | (PRIV_X | UNPRIV_X),
            _ => unimplemented!("Unpredictable code access perms: {:#X}", value),
        }
    }
}

pub struct PermMap([Perms; Self::ENTRIES]);

macro_rules! def_checks {
    ($($fn_ident: ident, $unpriv_mask_ident: ident);*$(;)?) => {
        $(
            #[inline]
            pub const fn $fn_ident(&self, addr: u32, privileged: bool) -> bool {
                self.0[(addr >> Self::PAGE_SHIFT) as usize]
                    & perms::$unpriv_mask_ident >> ((privileged as u8) << 2)
                    != 0
            }
        )*
    };
}

impl PermMap {
    pub const PAGE_SHIFT: usize = 12;
    pub const PAGE_SIZE: usize = 1 << Self::PAGE_SHIFT;
    pub const PAGE_MASK: u32 = (Self::PAGE_SIZE - 1) as u32;
    pub const ENTRIES: usize = 1 << (32 - Self::PAGE_SHIFT);

    def_checks! {
        read, UNPRIV_R;
        write, UNPRIV_W;
        execute, UNPRIV_X;
    }

    pub(super) fn set_range(&mut self, perms: Perms, (lower_bound, upper_bound): (u32, u32)) {
        debug_assert!(lower_bound & Self::PAGE_MASK == 0);
        debug_assert!(upper_bound & Self::PAGE_MASK == Self::PAGE_MASK);
        let lower_bound = (lower_bound >> Self::PAGE_SHIFT) as usize;
        let upper_bound = (upper_bound >> Self::PAGE_SHIFT) as usize;
        self.0[lower_bound..=upper_bound].fill(perms);
    }

    pub(super) fn set_all(&mut self, perms: Perms) {
        self.0.fill(perms);
    }
}
