use crate::ds_slot::RomControl;
use crate::utils::{bounded_int_lit, ByteSlice};

#[derive(Clone, Copy)]
pub struct Header<'a>(ByteSlice<'a>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitCode {
    Ds = 0,
    DsAndDsi = 2,
    Dsi = 3,
}

bounded_int_lit!(pub struct EncryptionSeed(u8), max 7);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    Normal = 0,
    Korea = 0x40,
    China = 0x80,
}

impl<'a> Header<'a> {
    #[inline]
    pub fn new(bytes: ByteSlice<'a>) -> Option<Self> {
        if bytes.len() < 0x170 {
            return None;
        }
        Some(Header(bytes))
    }

    #[inline]
    pub fn game_title(&self) -> Option<&str> {
        let mut title_bytes = &self.0[0..0xC];
        if let Some(first_nul_pos) = title_bytes.iter().position(|b| *b == 0) {
            title_bytes = &title_bytes[..first_nul_pos];
        }
        core::str::from_utf8(title_bytes).ok()
    }

    #[inline]
    pub fn game_code(&self) -> (u32, Option<&str>) {
        let code = self.0.read_le::<u32>(0xC);
        (code, core::str::from_utf8(&self.0[0xC..0x10]).ok())
    }

    #[inline]
    pub fn maker_code(&self) -> (u16, Option<&str>) {
        let code = self.0.read_le::<u16>(0x10);
        (code, core::str::from_utf8(&self.0[0x10..0x12]).ok())
    }

    #[inline]
    pub fn unit_code(&self) -> Result<UnitCode, u8> {
        match self.0[0x12] {
            0 => Ok(UnitCode::Ds),
            2 => Ok(UnitCode::DsAndDsi),
            3 => Ok(UnitCode::Dsi),
            other => Err(other),
        }
    }

    #[inline]
    pub fn encryption_seed(&self) -> Result<EncryptionSeed, u8> {
        match self.0[0x13] {
            seed @ 0..=7 => Ok(EncryptionSeed::new(seed)),
            other => Err(other),
        }
    }

    #[inline]
    pub fn capacity(&self) -> (u8, Option<usize>) {
        let shift = self.0[0x14];
        (shift, 1_usize.checked_shl(17 + shift as u32))
    }

    #[inline]
    pub fn region(&self) -> Result<Region, u8> {
        match self.0[0x1D] {
            0 => Ok(Region::Normal),
            0x40 => Ok(Region::Korea),
            0x80 => Ok(Region::China),
            other => Err(other),
        }
    }

    #[inline]
    pub fn version(&self) -> u8 {
        self.0[0x1E]
    }

    #[inline]
    pub fn auto_start(&self) -> bool {
        self.0[0x1F] & 1 << 2 != 0
    }

    #[inline]
    pub fn arm9_rom_offset(&self) -> u32 {
        self.0.read_le::<u32>(0x20)
    }

    #[inline]
    pub fn arm9_entry_addr(&self) -> u32 {
        self.0.read_le::<u32>(0x24)
    }

    #[inline]
    pub fn arm9_ram_addr(&self) -> u32 {
        self.0.read_le::<u32>(0x28)
    }

    #[inline]
    pub fn arm9_size(&self) -> u32 {
        self.0.read_le::<u32>(0x2C)
    }

    #[inline]
    pub fn arm7_rom_offset(&self) -> u32 {
        self.0.read_le::<u32>(0x30)
    }

    #[inline]
    pub fn arm7_entry_addr(&self) -> u32 {
        self.0.read_le::<u32>(0x34)
    }

    #[inline]
    pub fn arm7_ram_addr(&self) -> u32 {
        self.0.read_le::<u32>(0x38)
    }

    #[inline]
    pub fn arm7_size(&self) -> u32 {
        self.0.read_le::<u32>(0x3C)
    }

    #[inline]
    pub fn fnt_offset(&self) -> u32 {
        self.0.read_le::<u32>(0x40)
    }

    #[inline]
    pub fn fnt_size(&self) -> u32 {
        self.0.read_le::<u32>(0x44)
    }

    #[inline]
    pub fn fat_offset(&self) -> u32 {
        self.0.read_le::<u32>(0x48)
    }

    #[inline]
    pub fn fat_size(&self) -> u32 {
        self.0.read_le::<u32>(0x4C)
    }

    #[inline]
    pub fn arm9_overlay_offset(&self) -> u32 {
        self.0.read_le::<u32>(0x50)
    }

    #[inline]
    pub fn arm9_overlay_size(&self) -> u32 {
        self.0.read_le::<u32>(0x54)
    }

    #[inline]
    pub fn arm7_overlay_offset(&self) -> u32 {
        self.0.read_le::<u32>(0x58)
    }

    #[inline]
    pub fn arm7_overlay_size(&self) -> u32 {
        self.0.read_le::<u32>(0x5C)
    }

    #[inline]
    pub fn rom_control_normal(&self) -> RomControl {
        RomControl(self.0.read_le::<u32>(0x60))
    }

    #[inline]
    pub fn rom_control_key1(&self) -> RomControl {
        RomControl(self.0.read_le::<u32>(0x64))
    }

    #[inline]
    pub fn icon_title_offset(&self) -> u32 {
        self.0.read_le::<u32>(0x68)
    }

    #[inline]
    pub fn secure_area_crc(&self) -> u16 {
        self.0.read_le::<u16>(0x6C)
    }

    #[inline]
    pub fn used_rom_size(&self) -> u32 {
        self.0.read_le::<u32>(0x80)
    }

    #[inline]
    pub fn header_size(&self) -> u32 {
        self.0.read_le::<u32>(0x84)
    }

    #[inline]
    pub fn nand_raw_rom_end(&self) -> u16 {
        self.0.read_le::<u16>(0x94)
    }

    #[inline]
    pub fn nand_raw_rw_start(&self) -> u16 {
        self.0.read_le::<u16>(0x98)
    }

    #[inline]
    pub fn header_crc(&self) -> u16 {
        self.0.read_le::<u16>(0x15E)
    }
}
