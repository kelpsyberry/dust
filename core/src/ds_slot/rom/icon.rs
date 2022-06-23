use crate::utils::ByteSlice;

#[inline]
pub fn decode(offset: u32, rom: ByteSlice) -> Option<[u32; 32 * 32]> {
    let icon_data = ByteSlice::new(rom.get(offset as usize + 0x20..offset as usize + 0x240)?);

    let mut palette = [0; 16];
    for (i, color) in palette.iter_mut().enumerate() {
        let raw_color = icon_data.read_le::<u16>(0x200 | i << 1) as u32;
        let rgb6 = (raw_color << 1 & 0x3E)
        | (raw_color << 4 & 0x3E00)
        | (raw_color << 7 & 0x3E_0000);
        *color = 0xFF00_0000
            | rgb6 << 2 | (rgb6 >> 4 & 0x03_0303);
    }

    let mut pixels = [0; 32 * 32];
    for src_tile_line_base in (0..0x200).step_by(4) {
        let src_line = icon_data.read_le::<u32>(src_tile_line_base);
        let tile_y = src_tile_line_base >> 7;
        let tile_x = src_tile_line_base >> 5 & 3;
        let y_in_tile = src_tile_line_base >> 2 & 7;
        let dst_tile_line_base = tile_y << 8 | y_in_tile << 5 | tile_x << 3;
        for x_in_tile in 0..8 {
            pixels[dst_tile_line_base | x_in_tile] =
                palette[(src_line >> (x_in_tile << 2)) as usize & 0xF];
        }
    }
    Some(pixels)
}
