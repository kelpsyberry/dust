use dust_core::{
    ds_slot::rom::{self, Contents},
    utils::{mem_prelude::*, zeroed_box},
    Model,
};
use std::{any::Any, io, path::Path, sync::Arc};
use sync_file::{RandomAccessFile, ReadAt};

pub struct File {
    file: RandomAccessFile,
    len: u64,
    game_code: u32,
    header_bytes: Box<Bytes<0x170>>,
    secure_area_start: u32,
    secure_area_end: u64,
    secure_area: Option<Option<Box<Bytes<0x800>>>>,
    dldi_area_start: u32,
    dldi_area_end: u64,
    dldi_area: Option<Option<BoxedByteSlice>>,
}

impl Contents for File {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn len(&self) -> u64 {
        self.len.next_power_of_two()
    }

    fn game_code(&self) -> u32 {
        self.game_code
    }

    fn secure_area_mut(&mut self) -> Option<&mut [u8]> {
        self.secure_area
            .get_or_insert_with(|| {
                let mut buf = zeroed_box::<Bytes<0x800>>();
                self.file
                    .read_exact_at(&mut **buf, self.secure_area_start as u64)
                    .ok()
                    .map(|_| buf)
            })
            .as_mut()
            .map(|bytes| bytes.as_mut_slice())
    }

    fn dldi_area_mut(&mut self, addr: u32, len: usize) -> Option<&mut [u8]> {
        self.dldi_area
            .get_or_insert_with(|| {
                self.dldi_area_start = addr;
                self.dldi_area_end = (addr as u64) + len as u64;
                let mut buf = BoxedByteSlice::new_zeroed(len);
                self.file
                    .read_exact_at(&mut buf, self.dldi_area_start as u64)
                    .ok()
                    .map(|_| buf)
            })
            .as_mut()
            .map(|dldi_area| &mut **dldi_area)
    }

    fn read_header(&self, output: &mut Bytes<0x170>) {
        output.copy_from_slice(&**self.header_bytes)
    }

    fn read_slice(&self, addr: u32, output: &mut [u8]) {
        let read_len = (output.len() as u64).min(self.len.saturating_sub(addr as u64)) as usize;
        output[read_len..].fill(0);
        if read_len > 0 {
            self.file
                .read_exact_at(&mut output[..read_len], addr as u64)
                .expect("couldn't read DS slot ROM data");
        }

        macro_rules! apply_overlay {
            ($bytes: expr, $start: expr, $end: expr) => {
                if let Some(Some(bytes)) = $bytes {
                    if (addr as u64) < $end && addr as u64 + output.len() as u64 > $start as u64 {
                        let (start_src_i, start_dst_i) = if addr < $start {
                            (0, ($start - addr) as usize)
                        } else {
                            ((addr - $start) as usize, 0)
                        };
                        let len = output
                            .len()
                            .min(($end - $start as u64) as usize - start_src_i);
                        output[start_dst_i..start_dst_i + len]
                            .copy_from_slice(&bytes[start_src_i..start_src_i + len]);
                    }
                }
            };
        }
        apply_overlay!(
            &self.secure_area,
            self.secure_area_start,
            self.secure_area_end
        );
        apply_overlay!(&self.dldi_area, self.dldi_area_start, self.dldi_area_end);
    }
}

pub enum DsSlotRom {
    File(File),
    Memory(BoxedByteSlice),
}

pub enum CreationError {
    InvalidFileSize(u64),
    Io(io::Error),
}

impl From<io::Error> for CreationError {
    fn from(value: io::Error) -> Self {
        CreationError::Io(value)
    }
}

impl DsSlotRom {
    pub fn new(path: &Path, in_memory_max_size: u32, model: Model) -> Result<Self, CreationError> {
        let file = RandomAccessFile::open(path)?;
        let len = file.metadata()?.len();
        if !rom::is_valid_size(len.next_power_of_two(), model) {
            return Err(CreationError::InvalidFileSize(len));
        }
        let read_to_memory = len <= in_memory_max_size as u64;

        Ok(if read_to_memory {
            let mut bytes = BoxedByteSlice::new_zeroed(len.next_power_of_two() as usize);
            file.read_exact_at(&mut bytes[..len as usize], 0)?;
            DsSlotRom::Memory(bytes)
        } else {
            let mut header_bytes = zeroed_box::<Bytes<0x170>>();
            file.read_exact_at(&mut **header_bytes, 0)?;

            let game_code = header_bytes.read_le::<u32>(0x0C);
            let secure_area_start = header_bytes.read_le::<u32>(0x20);

            DsSlotRom::File(File {
                file,
                len,
                game_code,
                header_bytes,
                secure_area_start,
                secure_area_end: secure_area_start as u64 + 0x800,
                secure_area: None,
                dldi_area_start: 0,
                dldi_area_end: 0,
                dldi_area: None,
            })
        })
    }
}

macro_rules! forward_to_variants {
    ($ty: ident; $($variant: ident),*; $expr: expr, $f: ident $args: tt) => {
        match $expr {
            $(
                $ty::$variant(value) => value.$f $args,
            )*
        }
    }
}

impl Contents for DsSlotRom {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn len(&self) -> u64 {
        forward_to_variants!(DsSlotRom; File, Memory; self, len())
    }

    fn game_code(&self) -> u32 {
        forward_to_variants!(DsSlotRom; File, Memory; self, game_code())
    }

    fn secure_area_mut(&mut self) -> Option<&mut [u8]> {
        forward_to_variants!(DsSlotRom; File, Memory; self, secure_area_mut())
    }

    fn dldi_area_mut(&mut self, addr: u32, len: usize) -> Option<&mut [u8]> {
        forward_to_variants!(DsSlotRom; File, Memory; self, dldi_area_mut(addr, len))
    }

    fn read_header(&self, output: &mut Bytes<0x170>) {
        forward_to_variants!(DsSlotRom; File, Memory; self, read_header(output));
    }

    fn read_slice(&self, addr: u32, output: &mut [u8]) {
        forward_to_variants!(DsSlotRom; File, Memory; self, read_slice(addr, output));
    }
}

pub struct ArcDsSlotRom(pub Arc<DsSlotRom>);

impl Contents for ArcDsSlotRom {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn len(&self) -> u64 {
        self.0.len()
    }

    fn game_code(&self) -> u32 {
        self.0.game_code()
    }

    fn secure_area_mut(&mut self) -> Option<&mut [u8]> {
        Arc::get_mut(&mut self.0)
            // NOTE: This should only ever run on emulator initialization
            .expect("expected DS slot ROM to have no references")
            .secure_area_mut()
    }

    fn dldi_area_mut(&mut self, addr: u32, len: usize) -> Option<&mut [u8]> {
        Arc::get_mut(&mut self.0)
            // NOTE: This should only ever run on emulator initialization
            .expect("expected DS slot ROM to have no references")
            .dldi_area_mut(addr, len)
    }

    fn read_header(&self, output: &mut Bytes<0x170>) {
        self.0.read_header(output)
    }

    fn read_slice(&self, addr: u32, output: &mut [u8]) {
        self.0.read_slice(addr, output)
    }
}

impl From<DsSlotRom> for Box<dyn Contents> {
    fn from(rom: DsSlotRom) -> Self {
        Box::new(ArcDsSlotRom(Arc::new(rom)))
    }
}
