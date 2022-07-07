use super::{super::BgControl, Engine2d, Role};
use crate::gpu::vram::Vram;
use core::mem::MaybeUninit;

#[repr(align(64))]
pub struct TextTiles([MaybeUninit<u16>; 64]);

impl TextTiles {
    pub fn new_uninit() -> Self {
        TextTiles(MaybeUninit::uninit_array())
    }
}

pub fn read_bg_text_tiles<'a, R: Role>(
    engine: &Engine2d<R>,
    tiles: &'a mut TextTiles,
    bg_control: BgControl,
    y: u32,
    vram: &Vram,
) -> &'a [u16] {
    let map_base = {
        let mut map_base = if R::IS_A {
            engine.control.a_map_base() | bg_control.map_base()
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

    unsafe {
        if R::IS_A {
            vram.read_a_bg_slice::<usize>(map_base, 64, tiles.0.as_mut_ptr() as *mut usize);
        } else {
            vram.read_b_bg_slice::<usize>(map_base, 64, tiles.0.as_mut_ptr() as *mut usize);
        }
        if bg_control.size_key() & 1 == 0 {
            MaybeUninit::slice_assume_init_ref(&tiles.0[..32])
        } else {
            if R::IS_A {
                vram.read_a_bg_slice::<usize>(
                    (map_base + 0x800) & R::BG_VRAM_MASK,
                    64,
                    tiles.0.as_mut_ptr().add(32) as *mut usize,
                );
            } else {
                vram.read_b_bg_slice::<usize>(
                    (map_base + 0x800) & R::BG_VRAM_MASK,
                    64,
                    tiles.0.as_mut_ptr().add(32) as *mut usize,
                );
            }
            MaybeUninit::slice_assume_init_ref(&tiles.0[..])
        }
    }
}
