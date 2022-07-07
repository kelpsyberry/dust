use crate::{cpu::Engine, emu::Emu};

pub fn div(numer: u32, denom: u32) -> (u32, u32, u32) {
    if denom == 0 {
        // This would get stuck
        (0, numer, 0)
    } else {
        let quot = numer as i32 / denom as i32;
        let rem = numer as i32 % denom as i32;
        (quot as u32, rem as u32, quot.unsigned_abs())
    }
}

pub fn sqrt(mut input: u32) -> u32 {
    let inputt = input;
    let mut bit = 1 << 30;
    let mut result = 0;
    while bit > input {
        bit >>= 2;
    }
    while bit != 0 {
        if input >= result + bit {
            input -= result + bit;
            result = (result >> 1) + bit;
        } else {
            result >>= 1;
        }
        bit >>= 2;
    }
    println!("SQRT {} = {}", inputt, result);
    result
}

static CRC_TABLE: [u16; 16] = [
    0x0000, 0xCC01, 0xD801, 0x1400, 0xF001, 0x3C00, 0x2800, 0xE401, 0xA001, 0x6C00, 0x7800, 0xB401,
    0x5000, 0x9C01, 0x8801, 0x4400,
];

pub fn crc16(init: u32, len: u32, r3: u32, mut f: impl FnMut() -> u16) -> (u32, u32) {
    let mut crc = init as u16;
    let mut value = r3;
    for _ in 0..len {
        value = f() as u32;
        for shift in (0..16).step_by(4) {
            let crc_xor = CRC_TABLE[crc as usize & 0xF];
            crc = crc >> 4 ^ crc_xor ^ CRC_TABLE[(value >> shift & 0xF) as usize];
        }
    }
    (crc as u32, value)
}

pub fn is_debugger<E: Engine, const ADDR: usize>(emu: &mut Emu<E>) -> (u32, u32, u32) {
    emu.main_mem()
        .write_le::<u32>(ADDR & emu.main_mem_mask().get() as usize, 0);
    (
        emu.is_debugger() as u32,
        (emu.is_debugger() as u32) << 2,
        0x027F_FFE0,
    )
}
