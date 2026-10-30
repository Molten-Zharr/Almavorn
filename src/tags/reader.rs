use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    path::Path,
};

/// Present an empty ID3v1 year as four digits to Lofty's strict reader only.
/// The underlying file remains read-only and its original tail is retained.
pub(super) struct Reader {
    file: File,
    position: u64,
    year_offset: u64,
    pub preserved_id3v1: Option<[u8; 128]>,
}

impl Reader {
    pub fn open(path: &Path, mpeg: bool) -> io::Result<Self> {
        let mut file = File::open(path)?;
        let length = file.metadata()?.len();
        let mut preserved_id3v1 = None;
        if mpeg && length >= 128 {
            file.seek(SeekFrom::End(-128))?;
            let mut tail = [0; 128];
            file.read_exact(&mut tail)?;
            if &tail[..3] == b"TAG" && tail[93..97] == [0; 4] {
                preserved_id3v1 = Some(tail);
            }
        }
        file.rewind()?;
        Ok(Self {
            file,
            position: 0,
            year_offset: length.saturating_sub(35),
            preserved_id3v1,
        })
    }
}

impl Read for Reader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let length = self.file.read(buffer)?;
        if self.preserved_id3v1.is_some() {
            let first = self.position.max(self.year_offset);
            let end = (self.position + length as u64).min(self.year_offset + 4);
            if first < end {
                buffer[(first - self.position) as usize..(end - self.position) as usize].fill(b'0');
            }
        }
        self.position += length as u64;
        Ok(length)
    }
}

impl Seek for Reader {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.position = self.file.seek(position)?;
        Ok(self.position)
    }
}
