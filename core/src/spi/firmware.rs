mod default;
pub use default::default;

use super::Model;
use crate::utils::mem_prelude::*;
use core::ops::Range;

static CRC16_VALUES: [u16; 8] = [
    0xC0C1, 0xC181, 0xC301, 0xC601, 0xCC01, 0xD801, 0xF001, 0xA001,
];

fn crc16(init: u16, bytes: &[u8]) -> u16 {
    let mut result = init as u32;
    for &byte in bytes {
        result ^= byte as u32;
        for (i, crc) in CRC16_VALUES.iter().enumerate() {
            let carry = result & 1 != 0;
            result >>= 1;
            if carry {
                result ^= (*crc as u32) << (i ^ 7);
            }
        }
    }
    result as u16
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerificationRegion {
    Wifi,
    Ap1,
    Ap2,
    Ap3,
    User0,
    User0IQue,
    User1,
    User1IQue,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerificationError {
    IncorrectSize {
        expected: usize,
        got: usize,
    },
    IncorrectCrc16 {
        region: VerificationRegion,
        calculated: u16,
        expected: u16,
    },
}

fn check_crc(
    firmware: &[u8],
    region: VerificationRegion,
    init_value: u16,
    range: Range<usize>,
    expected_value_pos: usize,
) -> Result<(), VerificationError> {
    let calculated = crc16(init_value, &firmware[range]);
    let expected = firmware.read_le(expected_value_pos);
    if calculated == expected {
        Ok(())
    } else {
        Err(VerificationError::IncorrectCrc16 {
            region,
            calculated,
            expected,
        })
    }
}

/// # Errors
/// - [`VerificationError::IncorrectSize`](VerificationError::IncorrectSize): the firmware's size
//    is not among real consoles' firmware sizes (a power of two between 0x20000 and 0x80000 bytes).
//  - [`VerificationError::IncorrectCrc16`](VerificationError::IncorrectCrc16): the specified
//    region's CRC16 checksum does not match with the one stored in the firmware.
pub fn verify(firmware: &[u8], model: Model) -> Result<(), VerificationError> {
    let has_ique_regions = matches!(model, Model::Ique | Model::IqueLite | Model::Dsi);
    let expected_len = match model {
        Model::Dsi => 0x2_0000,
        Model::Ds | Model::Lite => 0x4_0000,
        Model::Ique | Model::IqueLite => 0x8_0000,
    };
    if firmware.len() != expected_len {
        return Err(VerificationError::IncorrectSize {
            expected: expected_len,
            got: firmware.len(),
        });
    }

    let mask = firmware.len() - 1;
    let wifi_len = firmware.read_le::<u16>(0x2C) as usize;
    check_crc(
        firmware,
        VerificationRegion::Wifi,
        0,
        0x2C..0x2C + wifi_len,
        0x2A,
    )?;
    check_crc(
        firmware,
        VerificationRegion::Ap1,
        0,
        0x7_FA00 & mask..0x7_FAFE & mask,
        0x7_FAFE & mask,
    )?;
    check_crc(
        firmware,
        VerificationRegion::Ap2,
        0,
        0x7_FB00 & mask..0x7_FBFE & mask,
        0x7_FBFE & mask,
    )?;
    check_crc(
        firmware,
        VerificationRegion::Ap3,
        0,
        0x7_FC00 & mask..0x7_FCFE & mask,
        0x7_FCFE & mask,
    )?;
    check_crc(
        firmware,
        VerificationRegion::User0,
        0xFFFF,
        0x7_FE00 & mask..0x7_FE70 & mask,
        0x7_FE72 & mask,
    )?;
    if has_ique_regions {
        check_crc(
            firmware,
            VerificationRegion::User0IQue,
            0xFFFF,
            0x7_FE00 & mask..0x7_FEFD & mask,
            0x7_FE72 & mask,
        )?;
    }
    check_crc(
        firmware,
        VerificationRegion::User1,
        0xFFFF,
        0x7_FF00 & mask..0x7_FF70 & mask,
        0x7_FF72 & mask,
    )?;
    if has_ique_regions {
        check_crc(
            firmware,
            VerificationRegion::User1IQue,
            0xFFFF,
            0x7_FF00 & mask..0x7_FF70 & mask,
            0x7_FF72 & mask,
        )?;
    }
    Ok(())
}

pub fn id_for_model(model: Model) -> [u8; 20] {
    let mut id = [0; 20];
    id[..3].copy_from_slice(&match model {
        Model::Ds => [0x20, 0x40, 0x12],
        Model::Lite => [0x20, 0x50, 0x12],
        // TODO: What's the ID for the iQue Lite?
        Model::Ique | Model::IqueLite => [0x20, 0x80, 0x13],
        Model::Dsi => [0x20, 0x40, 0x11],
    });
    id
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelDetectionError {
    IncorrectSize,
}

pub fn is_valid_size(firmware_len: usize) -> bool {
    firmware_len.is_power_of_two() && (0x2_0000..=0x8_0000).contains(&firmware_len)
}

/// # Errors
/// - [`DetectionError::IncorrectSize`](DetectionError::IncorrectSize): the firmware's size is not
///   among real consoles' firmware sizes (a power of two between 0x20000 and 0x80000 bytes).
/// - [`DetectionError::UnknownModel`](DetectionError::UnknownModel): the DS model could not be
///   detected based on the contents of the firmware.
pub fn detect_model(firmware: &[u8]) -> Result<Option<Model>, ModelDetectionError> {
    if !is_valid_size(firmware.len()) {
        return Err(ModelDetectionError::IncorrectSize);
    }
    match firmware[0x1D] {
        0xFF => Ok(Some(Model::Ds)),
        0x20 => Ok(Some(Model::Lite)),
        0x43 => Ok(Some(Model::Ique)),
        0x63 => Ok(Some(Model::IqueLite)),
        0x57 => Ok(Some(Model::Dsi)),
        _ => Ok(None),
    }
}

pub fn newest_user_settings(firmware: &[u8]) -> &[u8] {
    let user_settings_offset = (firmware.read_le::<u16>(0x20) as usize) << 3;
    let count_0 = firmware.read_le::<u16>(user_settings_offset + 0x70);
    let count_1 = firmware.read_le::<u16>(user_settings_offset + 0x170);
    if count_1 == (count_0 + 1) & 0x7F {
        &firmware[user_settings_offset + 0x100..user_settings_offset + 0x200]
    } else {
        &firmware[user_settings_offset..user_settings_offset + 0x100]
    }
}
