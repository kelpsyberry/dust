use crate::Vram;
use core::{
    mem::{self, MaybeUninit},
    ptr,
};
use dust_core::gpu::engine_2d::{BgControl, Control, Role};

#[repr(align(64))]
pub struct TextTiles([MaybeUninit<u16>; 64]);

impl TextTiles {
    pub fn new_uninit() -> Self {
        TextTiles(MaybeUninit::uninit_array())
    }
}

#[inline(always)]
pub fn read_bg_text_tiles<'a, R: Role, V: Vram<R>>(
    tiles: &'a mut TextTiles,
    control: Control,
    bg_control: BgControl,
    y: u32,
    vram: &V,
) -> &'a [u16]
where
    [(); R::BG_VRAM_LEN]: Sized,
{
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
        ptr::copy_nonoverlapping(
            bg_vram.as_ptr().add(map_base as usize) as *const usize,
            tiles.0.as_mut_ptr() as *mut usize,
            64 / mem::size_of::<usize>(),
        );
        if bg_control.size_key() & 1 == 0 {
            MaybeUninit::slice_assume_init_ref(&tiles.0[..32])
        } else {
            ptr::copy_nonoverlapping(
                bg_vram
                    .as_ptr()
                    .add(((map_base + 0x800) & R::BG_VRAM_MASK) as usize)
                    as *const usize,
                tiles.0.as_mut_ptr().add(32) as *mut usize,
                64 / mem::size_of::<usize>(),
            );
            MaybeUninit::slice_assume_init_ref(&tiles.0[..])
        }
    }
}
