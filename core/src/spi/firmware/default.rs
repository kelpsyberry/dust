use super::crc16;
use crate::{
    utils::{BoxedByteSlice, ByteMutSlice},
    Model,
};

pub fn default(model: Model) -> BoxedByteSlice {
    let len = match model {
        Model::Dsi => 0x2_0000,
        Model::Ds | Model::Lite => 0x4_0000,
        Model::Ique | Model::IqueLite => 0x8_0000,
    };

    let mut firmware = BoxedByteSlice::new_zeroed(len);

    firmware.write_le(0x04, 0xDB00_u16);
    firmware.write_le(0x06, 0x0F1F_u16);

    for (i, byte) in [b'M', b'A', b'C', 0x68].into_iter().enumerate() {
        firmware[0x08 + i] = byte;
    }

    firmware.write_le(0x14, (len >> 17 << 12) as u16);

    for (i, byte) in [0x00, 0x00, 0x01, 0x01, 0x06].into_iter().enumerate() {
        firmware[0x18 + i] = byte;
    }

    firmware[0x1D] = match model {
        Model::Ds => 0xFF,
        Model::Lite => 0x20,
        Model::Ique => 0x57,
        Model::IqueLite => 0x43,
        Model::Dsi => 0x63,
    };

    firmware.write_le(0x1E, 0xFFFF_u16);

    firmware.write_le(0x20, ((len - 0x200) >> 3) as u16);
    firmware.write_le(0x22, 0x0B51_u16);
    firmware.write_le(0x24, 0x0DB3_u16);
    firmware.write_le(0x26, 0x4F5D_u16);

    firmware.write_le(0x28, 0xFFFF_u16);

    for (i, user_settings_bounds) in [(len - 0x200, len - 0x100), (len - 0x100, len)]
        .into_iter()
        .enumerate()
    {
        let mut user_settings =
            ByteMutSlice::new(&mut firmware[user_settings_bounds.0..user_settings_bounds.1]);

        user_settings.write_le(0x00, 5_u16);

        user_settings.write_le(0x02, (1 - i) as u8);
        user_settings[0x03] = 1;
        user_settings[0x04] = 1;
        for (i, char) in (*b"Dust").into_iter().enumerate() {
            user_settings.write_le(0x6 + (i << 1), char as u16);
        }
        user_settings.write_le(0x1A, 4_u16);
        user_settings.write_le(0x1C, b' ' as u16);
        user_settings.write_le(0x50, 1_u16);

        user_settings[0x52] = 6;
        user_settings[0x53] = 55;
        user_settings[0x56] = 0;
        user_settings[0x57] = 0;

        user_settings.write_le(0x58, 0x0000_u16);
        user_settings.write_le(0x5A, 0x0000_u16);
        user_settings.write_le(0x5C, 0x00_00_u16);
        user_settings.write_le(0x5E, 0x0FF0_u16);
        user_settings.write_le(0x60, 0x0BF0_u16);
        user_settings.write_le(0x62, 0xBF_FF_u16);

        user_settings.write_le(
            0x64,
            if matches!(model, Model::Ds | Model::Ique) {
                0xFC01_u16
            } else {
                0xFC11
            },
        );
        user_settings[0x66] = 6;
        user_settings[0x67] = 0x7F;
        user_settings.write_le(0x68, 0x0C44_DE1C_u32);
        user_settings.write_le(0x6C, 0_u32);

        user_settings.write_le(0x70, (1 + i) as u16);
        user_settings.write_le(0x72, crc16(0xFFFF, &user_settings[..0x70]));

        if matches!(model, Model::Ique | Model::IqueLite | Model::Dsi) {
            user_settings[0x74] = 1;
            user_settings[0x75] = 1;
            user_settings.write_le(
                0x76,
                match model {
                    Model::Ique => 0x7E,
                    Model::IqueLite => 0x42,
                    _ => 0x3E,
                },
            );
            user_settings[0x78..0xFE].fill(if model == Model::Dsi { 0x00 } else { 0xFF });
            user_settings.write_le(0xFE, crc16(0xFFFF, &user_settings[0x74..0xFE]));
        } else {
            user_settings[0x74..].fill(0xFF);
        }
    }

    firmware
}
