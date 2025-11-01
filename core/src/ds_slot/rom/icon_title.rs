use super::{header::Header, Contents};
use crate::utils::{mem_prelude::*, zeroed_box};
use std::array;

pub type Palette = [u16; 0x10];
pub type Pixels = [u8; 0x400];

fn decode_palette(offset: u32, rom_contents: &(impl Contents + ?Sized)) -> Palette {
    let mut data = Bytes::new([0; 0x20]);
    rom_contents.read_slice(offset, &mut *data);
    array::from_fn(|i| data.read_le(i << 1))
}

fn decode_pixels(offset: u32, rom_contents: &(impl Contents + ?Sized)) -> Pixels {
    let mut data = zeroed_box::<Bytes<0x200>>();
    rom_contents.read_slice(offset, &mut **data);

    let mut pixels = [0; 0x400];
    for src_tile_line_base in (0..0x200).step_by(4) {
        let src_line = data.read_le::<u32>(src_tile_line_base);
        let tile_y = src_tile_line_base >> 7;
        let tile_x = src_tile_line_base >> 5 & 3;
        let y_in_tile = src_tile_line_base >> 2 & 7;
        let dst_tile_line_base = tile_y << 8 | y_in_tile << 5 | tile_x << 3;
        for x_in_tile in 0..8 {
            pixels[dst_tile_line_base | x_in_tile] = (src_line >> (x_in_tile << 2)) as u8 & 0xF;
        }
    }
    pixels
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Version {
    Base = 1,
    Chinese = 2,
    Korean = 3,
    AnimatedIcon = 0x103,
}

pub struct VersionCrcData {
    pub version: Version,
    pub crc16_v1: u16,
    pub crc16_v2: u16,
    pub crc16_v3: u16,
    pub crc16_v103: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    OutOfBounds,
    UnknownVersion(u16),
}

impl VersionCrcData {
    pub fn decode_at_offset(
        icon_title_offset: u32,
        rom_contents: &(impl Contents + ?Sized),
    ) -> Result<Self, DecodeError> {
        if icon_title_offset as u64 + 20 > rom_contents.len() {
            return Err(DecodeError::OutOfBounds);
        }
        let mut version_crc_data = Bytes::new([0; 0x20]);
        rom_contents.read_slice(icon_title_offset, &mut *version_crc_data);

        let version = match version_crc_data.read_le::<u16>(0) {
            1 => Version::Base,
            2 => Version::Chinese,
            3 => Version::Korean,
            0x103 => Version::AnimatedIcon,
            version => return Err(DecodeError::UnknownVersion(version)),
        };

        let crc16_v1 = version_crc_data.read_le::<u16>(2);
        let crc16_v2 = version_crc_data.read_le::<u16>(4);
        let crc16_v3 = version_crc_data.read_le::<u16>(6);
        let crc16_v103 = version_crc_data.read_le::<u16>(8);

        Ok(VersionCrcData {
            version,
            crc16_v1,
            crc16_v2,
            crc16_v3,
            crc16_v103,
        })
    }
}

pub struct DefaultIcon {
    pub palette: Palette,
    pub pixels: Pixels,
}

impl DefaultIcon {
    pub fn decode_at_offset(
        icon_title_offset: u32,
        rom_contents: &(impl Contents + ?Sized),
    ) -> Option<Box<Self>> {
        if icon_title_offset as u64 + 0x240 > rom_contents.len() {
            return None;
        }

        let default_icon = Box::new(DefaultIcon {
            palette: decode_palette(icon_title_offset + 0x220, rom_contents),
            pixels: decode_pixels(icon_title_offset + 0x20, rom_contents),
        });

        Some(default_icon)
    }
}

pub type Title = Result<String, Box<[u8; 0x100]>>;

pub struct Titles {
    pub japanese: Title,
    pub english: Title,
    pub french: Title,
    pub german: Title,
    pub italian: Title,
    pub spanish: Title,
    pub chinese: Option<Title>,
    pub korean: Option<Title>,
}

impl Titles {
    pub fn decode_at_offset(
        icon_title_offset: u32,
        version: Version,
        rom_contents: &(impl Contents + ?Sized),
    ) -> Option<Self> {
        macro_rules! title {
            ($offset: expr) => {{
                if icon_title_offset as u64 + $offset > rom_contents.len() {
                    return None;
                }
                let mut bytes = [0; 0x100];
                rom_contents.read_slice(icon_title_offset + $offset, &mut bytes);
                let end_index = bytes
                    .chunks(2)
                    .position(|c| *c == [0; 2])
                    .unwrap_or(bytes.len())
                    << 1;
                String::from_utf16le(&bytes[..end_index]).map_err(|_| Box::new(bytes))
            }};
        }

        let japanese = title!(0x240);
        let english = title!(0x340);
        let french = title!(0x440);
        let german = title!(0x540);
        let italian = title!(0x640);
        let spanish = title!(0x740);
        let chinese = if version >= Version::Chinese {
            Some(title!(0x840))
        } else {
            None
        };
        let korean = if version >= Version::Korean {
            Some(title!(0x940))
        } else {
            None
        };

        Some(Titles {
            japanese,
            english,
            french,
            german,
            italian,
            spanish,
            chinese,
            korean,
        })
    }
}

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct AnimSequenceEntry(pub u16): Debug {
        pub frames: u8 @ 0..=7,
        pub bitmap: u8 @ 8..=10,
        pub palette: u8 @ 11..=13,
        pub h_flip: bool @ 14,
        pub v_flip: bool @ 15,
    }
}

pub struct AnimatedIcon {
    pub palettes: [Box<Palette>; 8],
    pub pixels: [Box<Pixels>; 8],
    pub anim_sequence: Vec<AnimSequenceEntry>,
}

impl AnimatedIcon {
    pub fn decode_at_offset(
        icon_title_offset: u32,
        rom_contents: &(impl Contents + ?Sized),
    ) -> Option<Self> {
        if icon_title_offset as u64 + 0x23C0 > rom_contents.len() {
            return None;
        }

        let palettes: [Box<Palette>; 8] = array::from_fn(|i| {
            let offset = icon_title_offset + 0x1240 + i as u32 * 0x200;
            Box::new(decode_palette(offset, rom_contents))
        });
        let pixels: [Box<Pixels>; 8] = array::from_fn(|i| {
            let offset = icon_title_offset + 0x2240 + i as u32 * 0x20;
            Box::new(decode_pixels(offset, rom_contents))
        });

        let mut anim_sequence_data = Bytes::new([0; 0x80]);
        rom_contents.read_slice(icon_title_offset + 0x2340, &mut *anim_sequence_data);

        let mut anim_sequence = Vec::with_capacity(0x40);
        for i in 0..0x40 {
            let entry = AnimSequenceEntry(anim_sequence_data.read_le::<u16>(i * 2));
            if entry.frames() == 0 {
                break;
            }
            anim_sequence.push(entry);
        }

        Some(AnimatedIcon {
            palettes,
            pixels,
            anim_sequence,
        })
    }
}

pub struct IconTitle {
    pub version_crc_data: VersionCrcData,
    pub default_icon: Box<DefaultIcon>,
    pub titles: Titles,
    pub animated_icon: Option<AnimatedIcon>,
}

impl IconTitle {
    pub fn decode_at_offset(
        icon_title_offset: u32,
        rom_contents: &(impl Contents + ?Sized),
    ) -> Result<Self, DecodeError> {
        let version_crc_data = VersionCrcData::decode_at_offset(icon_title_offset, rom_contents)?;
        Ok(IconTitle {
            default_icon: DefaultIcon::decode_at_offset(icon_title_offset, rom_contents)
                .ok_or(DecodeError::OutOfBounds)?,
            titles: Titles::decode_at_offset(
                icon_title_offset,
                version_crc_data.version,
                rom_contents,
            )
            .ok_or(DecodeError::OutOfBounds)?,
            animated_icon: if version_crc_data.version >= Version::AnimatedIcon {
                Some(
                    AnimatedIcon::decode_at_offset(icon_title_offset, rom_contents)
                        .ok_or(DecodeError::OutOfBounds)?,
                )
            } else {
                None
            },
            version_crc_data,
        })
    }
}

pub fn read_icon_title_offset(rom_contents: &(impl Contents + ?Sized)) -> Option<u32> {
    if 0x170 > rom_contents.len() {
        return None;
    }
    let mut header_bytes = Bytes::new([0; 0x170]);
    rom_contents.read_header(&mut header_bytes);
    let header = Header::new(&header_bytes);
    Some(header.icon_title_offset())
}
