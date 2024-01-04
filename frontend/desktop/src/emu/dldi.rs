use dust_core::{
    dldi::Provider,
    utils::{BoxedByteSlice, Bytes},
};
use std::{
    collections::VecDeque,
    fs,
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};
use tempdir::TempDir;

struct LoadedChunk {
    index: u64,
    is_dirty: bool,
    contents: BoxedByteSlice,
}

impl LoadedChunk {
    fn path(base_path: &Path, index: u64) -> PathBuf {
        base_path.join(format!("{}.bin", index))
    }

    fn load(base_path: &Path, index: u64, chunk_size_shift: u8) -> io::Result<Self> {
        let mut contents = BoxedByteSlice::new_zeroed(1 << chunk_size_shift);
        match fs::File::open(Self::path(base_path, index)) {
            Ok(mut file) => file.read_exact(&mut contents[..])?,
            Err(err) => {
                if err.kind() != io::ErrorKind::NotFound {
                    return Err(err);
                }
            }
        }
        Ok(LoadedChunk {
            index,
            is_dirty: false,
            contents,
        })
    }

    fn writeback(&mut self, base_path: &Path) -> io::Result<()> {
        if !self.is_dirty {
            return Ok(());
        }

        fs::write(Self::path(base_path, self.index), &self.contents[..])?;
        self.is_dirty = false;
        Ok(())
    }
}

struct ChunkManager {
    temp_dir: TempDir,

    cur_addr: u64,
    fs_max_size: u64,

    chunk_size_shift: u8,
    chunk_size_mask: usize,
    loaded_chunks: VecDeque<LoadedChunk>,
    max_loaded_chunks: usize,
}

impl ChunkManager {
    fn find_or_load_chunk(&mut self, chunk_index: u64) -> io::Result<usize> {
        for (
            i,
            LoadedChunk {
                index: loaded_chunk_index,
                ..
            },
        ) in self.loaded_chunks.iter().enumerate()
        {
            if *loaded_chunk_index == chunk_index {
                return Ok(i);
            }
        }

        if self.loaded_chunks.len() == self.max_loaded_chunks {
            if let Some(mut chunk) = self.loaded_chunks.pop_front() {
                chunk.writeback(self.temp_dir.path())?;
            }
        }

        self.loaded_chunks.push_back(LoadedChunk::load(
            self.temp_dir.path(),
            chunk_index,
            self.chunk_size_shift,
        )?);
        Ok(0)
    }
}

impl Seek for ChunkManager {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        match pos {
            SeekFrom::Start(addr) => self.cur_addr = addr,
            _ => {
                let (base, offset) = match pos {
                    SeekFrom::Current(offset) => (self.cur_addr, offset),
                    SeekFrom::End(offset) => (self.fs_max_size, offset),
                    _ => unreachable!(),
                };
                let (res, overflowed) = base.overflowing_add_signed(offset);
                self.cur_addr = if overflowed {
                    if offset > 0 {
                        self.fs_max_size
                    } else {
                        0
                    }
                } else {
                    res.clamp(0, self.fs_max_size)
                };
            }
        }
        Ok(self.cur_addr)
    }
}

impl Read for ChunkManager {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.cur_addr >= self.fs_max_size || buf.is_empty() {
            return Ok(0);
        }
        let max_read_len = (buf.len() as u64).min(self.fs_max_size - self.cur_addr);

        let start_addr = self.cur_addr;
        let end_addr = self.cur_addr + max_read_len - 1;
        let mut buf_offset = 0;
        for chunk_index in start_addr >> self.chunk_size_shift..=end_addr >> self.chunk_size_shift {
            let chunk_queue_index = self.find_or_load_chunk(chunk_index)?;
            let chunk = &self.loaded_chunks[chunk_queue_index];

            let start_chunk_offset = (start_addr.max(chunk_index << self.chunk_size_shift))
                as usize
                & self.chunk_size_mask;
            let end_chunk_offset = (end_addr.min(((chunk_index + 1) << self.chunk_size_shift) - 1))
                as usize
                & self.chunk_size_mask;
            let transfer_len = end_chunk_offset - start_chunk_offset + 1;
            buf[buf_offset..buf_offset + transfer_len]
                .copy_from_slice(&chunk.contents[start_chunk_offset..=end_chunk_offset]);
            buf_offset += transfer_len;
            self.cur_addr += transfer_len as u64;
        }

        Ok(max_read_len as usize)
    }
}

impl Write for ChunkManager {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.cur_addr >= self.fs_max_size || buf.is_empty() {
            return Ok(0);
        }
        let max_write_len = (buf.len() as u64).min(self.fs_max_size - self.cur_addr);

        let start_addr = self.cur_addr;
        let end_addr = self.cur_addr + max_write_len - 1;
        let mut buf_offset = 0;
        for chunk_index in start_addr >> self.chunk_size_shift..=end_addr >> self.chunk_size_shift {
            let chunk_queue_index = self.find_or_load_chunk(chunk_index)?;
            let chunk = &mut self.loaded_chunks[chunk_queue_index];
            chunk.is_dirty = true;

            let start_chunk_offset = (start_addr.max(chunk_index << self.chunk_size_shift))
                as usize
                & self.chunk_size_mask;
            let end_chunk_offset = (end_addr.min(((chunk_index + 1) << self.chunk_size_shift) - 1))
                as usize
                & self.chunk_size_mask;
            let transfer_len = end_chunk_offset - start_chunk_offset + 1;
            chunk.contents[start_chunk_offset..=end_chunk_offset]
                .copy_from_slice(&buf[buf_offset..buf_offset + transfer_len]);
            buf_offset += transfer_len;
            self.cur_addr += transfer_len as u64;
        }

        Ok(max_write_len as usize)
    }

    fn flush(&mut self) -> io::Result<()> {
        for chunk in &mut self.loaded_chunks {
            chunk.writeback(self.temp_dir.path())?;
        }
        Ok(())
    }
}

pub struct FsProvider {
    chunk_manager: ChunkManager,
}

impl FsProvider {
    pub fn new(root_path: &Path, skip_path: &Path) -> io::Result<Self> {
        let mut chunk_manager = ChunkManager {
            temp_dir: TempDir::new("dust")?,
            cur_addr: 0,
            fs_max_size: 1 << 30,
            chunk_size_shift: 22,
            chunk_size_mask: (1 << 22) - 1,
            loaded_chunks: VecDeque::with_capacity(4),
            max_loaded_chunks: 4,
        };
        fatfs::format_volume(
            &mut chunk_manager,
            fatfs::FormatVolumeOptions::new()
                .fat_type(fatfs::FatType::Fat16)
                .volume_label(*b"Dust DLDI  "),
        )?;
        let fs = fatfs::FileSystem::new(&mut chunk_manager, fatfs::FsOptions::new())?;
        Self::construct_dir(fs.root_dir(), root_path, skip_path)?;
        drop(fs);
        Ok(FsProvider { chunk_manager })
    }

    fn construct_dir(
        dir: fatfs::Dir<&mut ChunkManager>,
        path: &Path,
        skip_path: &Path,
    ) -> io::Result<()> {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if entry.path() == skip_path {
                continue;
            }
            if let Some(name) = entry.file_name().to_str() {
                let path = entry.path();
                if file_type.is_dir() {
                    let dir = dir.create_dir(name)?;
                    Self::construct_dir(dir, &path, skip_path)?;
                } else if file_type.is_file() {
                    let mut file = dir.create_file(name)?;
                    let contents = std::fs::read(path)?;
                    file.write_all(&contents)?;
                }
            }
        }
        Ok(())
    }
}

impl Provider for FsProvider {
    fn supports_writes(&self) -> bool {
        true
    }

    fn read_sector(&mut self, sector: u32, buffer: &mut Bytes<0x200>) -> bool {
        self.chunk_manager
            .seek(SeekFrom::Start((sector as u64) << 9))
            .is_ok()
            && self.chunk_manager.read_exact(&mut buffer[..]).is_ok()
    }

    fn write_sector(&mut self, sector: u32, buffer: &Bytes<0x200>) -> bool {
        self.chunk_manager
            .seek(SeekFrom::Start((sector as u64) << 9))
            .is_ok()
            && self.chunk_manager.write_all(&buffer[..]).is_ok()
    }
}
