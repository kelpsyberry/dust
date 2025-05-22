use crate::Vram;
use core::mem::{self, MaybeUninit};
use dust_core::gpu::engine_2d::{BgControl, Control, Role};

#[repr(align(64))]
pub struct TextTiles([MaybeUninit<u16>; 64]);

impl TextTiles {
    pub fn new_uninit() -> Self {
        TextTiles([MaybeUninit::uninit(); 64])
    }
}

#[inline(always)]
pub fn read_bg_text_tiles<
    'a,
    R: Role,
    V: Vram<R, BG_VRAM_LEN, OBJ_VRAM_LEN>,
    const BG_VRAM_LEN: usize,
    const OBJ_VRAM_LEN: usize,
>(
    tiles: &'a mut TextTiles,
    control: Control,
    bg_control: BgControl,
    y: u32,
    vram: &V,
) -> &'a [u16] {
    let map_base = {
        let mut map_base = if R::IS_A {
            control.a_map_base() | bg_control.map_base()
        } else {
            bg_control.map_base()
        };
        match bg_control.size_key() {
            0 | 1 => {
                map_base |= (y & 0xF8) << 3;
            }
            2 => {
                map_base += (y & 0x1F8) << 3;
                if R::IS_A {
                    map_base &= R::BG_VRAM_MASK;
                }
            }
            _ => {
                map_base |= (y & 0xF8) << 3;
                map_base += (y & 0x100) << 4;
                if R::IS_A {
                    map_base &= R::BG_VRAM_MASK;
                }
            }
        }
        map_base
    };

    let bg_vram = vram.bg();
    unsafe {
        (tiles.0.as_mut_ptr() as *mut usize).copy_from_nonoverlapping(
            bg_vram.as_ptr().add(map_base as usize) as *const usize,
            64 / mem::size_of::<usize>(),
        );
        if bg_control.size_key() & 1 == 0 {
            tiles.0[..32].assume_init_ref()
        } else {
            (tiles.0.as_mut_ptr().add(32) as *mut usize).copy_from_nonoverlapping(
                bg_vram
                    .as_ptr()
                    .add(((map_base + 0x800) & R::BG_VRAM_MASK) as usize)
                    as *const usize,
                64 / mem::size_of::<usize>(),
            );
            tiles.0.assume_init_ref()
        }
    }
}
