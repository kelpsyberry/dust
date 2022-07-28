use dust_core::{
    ds_slot::rom::Contents,
    utils::{zeroed_box, ByteMutSlice, Bytes},
};
use std::{
    fs,
    io::{self, Read, Seek, SeekFrom},
    path::Path,
};

pub struct DsSlotRom {
    file: fs::File,
    len: usize,
    game_code: u32,
    secure_area_start: usize,
    secure_area_end: usize,
    secure_area: Option<Option<Box<Bytes<0x800>>>>,
}

impl DsSlotRom {
    pub fn new(path: &Path) -> io::Result<Self> {
        let mut file = fs::File::open(path)?;

        let len = file.metadata()?.len() as usize;

        let mut header_bytes = Bytes::new([0; 0x170]);
        file.read_exact(&mut header_bytes[..])?;

        let game_code = header_bytes.read_le::<u32>(0x0C);
        let secure_area_start = header_bytes.read_le::<u32>(0x20) as usize;

        Ok(DsSlotRom {
            file,
            len,
            game_code,
            secure_area_start,
            secure_area_end: secure_area_start + 0x800,
            secure_area: None,
        })
    }
}

impl Contents for DsSlotRom {
    fn len(&self) -> usize {
        self.len
    }

    fn game_code(&self) -> u32 {
        self.game_code
    }

    fn secure_area_mut(&mut self) -> Option<ByteMutSlice> {
        self.secure_area
            .get_or_insert_with(|| {
                let mut buf = zeroed_box::<Bytes<0x800>>();
                self.file
                    .seek(SeekFrom::Start(self.secure_area_start as u64))
                    .and_then(|_| self.file.read_exact(&mut buf[..]))
                    .ok()
                    .map(|_| buf)
            })
            .as_mut()
            .map(|secure_area| ByteMutSlice::new(&mut secure_area[..]))
    }

    fn read_header(&mut self, buf: &mut Bytes<0x170>) {
        self.file
            .seek(SeekFrom::Start(0))
            .and_then(|_| self.file.read_exact(&mut buf[..]))
            .expect("couldn't read DS slot ROM header");
    }

    fn read_slice(&mut self, addr: usize, mut output: ByteMutSlice) {
        self.file
            .seek(SeekFrom::Start(addr as u64))
            .and_then(|_| self.file.read_exact(&mut output[..]))
            .expect("couldn't read DS slot ROM data");
        if let Some(Some(secure_area)) = &self.secure_area {
            if addr < self.secure_area_end && addr + output.len() > self.secure_area_start {
                let (start_src_i, start_dst_i) = if addr < self.secure_area_start {
                    (0, self.secure_area_start - addr)
                } else {
                    (addr - self.secure_area_start, 0)
                };
                let len = output.len().min(0x800 - start_src_i);
                output[start_dst_i..start_dst_i + len]
                    .copy_from_slice(&secure_area[start_src_i..start_src_i + len]);
            }
        }
    }
}
