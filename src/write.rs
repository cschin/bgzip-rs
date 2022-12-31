use crate::header;
use flate2::write::DeflateEncoder;
use flate2::Crc;
use std::convert::TryInto;
use std::io::{self, Write};

/// A BGZF writer
pub struct BGZFWriter<W: io::Write> {
    writer: W,
    buffer: Vec<u8>,
    compressed_buffer: Vec<u8>,
    compress_block_unit: usize,
    level: flate2::Compression,
    closed: bool,
}

/// Default BGZF block size.
pub const DEFAULT_COMPRESS_BLOCK_UNIT: usize = 65280;

impl<W: io::Write> BGZFWriter<W> {
    /// Create new BGZF writer from [`std::io::Write`]
    pub fn new(writer: W, level: flate2::Compression) -> Self {
        BGZFWriter {
            writer,
            buffer: Vec::new(),
            compressed_buffer: Vec::new(),
            compress_block_unit: DEFAULT_COMPRESS_BLOCK_UNIT,
            level,
            closed: false,
        }
    }

    /// Cerate new BGZF writer with block size.
    pub fn with_block_size(writer: W, level: flate2::Compression, block_size: usize) -> Self {
        BGZFWriter {
            writer,
            buffer: Vec::new(),
            compressed_buffer: Vec::new(),
            compress_block_unit: block_size,
            level,
            closed: false,
        }
    }

    fn write_block(&mut self) -> io::Result<()> {
        self.compressed_buffer.clear();
        let uncompressed_block_size = self.compress_block_unit.min(self.buffer.len());
        let mut encoder = DeflateEncoder::new(&mut self.compressed_buffer, self.level);
        encoder.write_all(&self.buffer[..uncompressed_block_size])?;
        encoder.finish()?;

        let mut crc = Crc::new();
        crc.update(&self.buffer[..uncompressed_block_size]);

        let header =
            header::BGZFHeader::new(true, 0, self.compressed_buffer.len().try_into().unwrap());
        header.write(&mut self.writer)?;
        self.writer.write_all(&self.compressed_buffer)?;
        self.buffer.drain(..uncompressed_block_size);
        self.writer.write_all(&crc.sum().to_le_bytes())?;
        self.writer
            .write_all(&(uncompressed_block_size as u32).to_le_bytes())?;

        Ok(())
    }

    /// Write end-of-file marker and close BGZF.
    ///
    /// Explicitly call of this method is not required. Drop trait will write end-of-file marker automatically.
    /// If you need to handle I/O errors while closing, please use this method.
    pub fn close(mut self) -> io::Result<()> {
        if !self.closed {
            self.flush()?;
            self.writer.write_all(FOOTER_BYTES)?;
            self.closed = true;
        }
        Ok(())
    }
}

impl<W: io::Write> io::Write for BGZFWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buffer.extend_from_slice(buf);
        while self.compress_block_unit < self.buffer.len() {
            self.write_block()?;
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        while !self.buffer.is_empty() {
            self.write_block()?;
        }
        Ok(())
    }
}

const FOOTER_BYTES: &[u8] = &[
    0x1f, 0x8b, 0x08, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0x06, 0x00, 0x42, 0x43, 0x02, 0x00,
    0x1b, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

impl<W: io::Write> Drop for BGZFWriter<W> {
    fn drop(&mut self) {
        if !self.closed {
            self.flush().unwrap();
            self.writer.write_all(FOOTER_BYTES).unwrap();
            self.closed = true;
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::fs;
    use std::io::Write;

    #[test]
    fn test_vcf() -> io::Result<()> {
        let mut writer = BGZFWriter::new(
            fs::File::create("target/test.vcf.gz")?,
            flate2::Compression::default(),
        );
        let mut reader = flate2::read::MultiGzDecoder::new(fs::File::open(
            "testfiles/common_all_20180418_half.vcf.gz",
        )?);
        io::copy(&mut reader, &mut writer)?;
        Ok(())
    }

    #[test]
    fn test_simple() -> io::Result<()> {
        let mut writer = BGZFWriter::new(
            fs::File::create("target/simple1.txt.gz")?,
            flate2::Compression::default(),
        );
        writer.write_all(b"1234")?;
        Ok(())
    }
}
