use std::io::{BufWriter, Read, Seek, Write};

use bzip2::bufread::BzDecoder;
use std::io::Result;
use xz::bufread::XzDecoder;
use zstd::Decoder as ZstdDecoder;

use super::PartitionExtent;
use crate::payload::update_metadata::install_operation::Type;

pub struct PartitionDecoder<W: Write + Seek> {
    destination: BufWriter<W>,
}

impl<W: Write + Seek> PartitionDecoder<W> {
    pub fn new(destination: W) -> Self {
        Self {
            destination: BufWriter::new(destination),
        }
    }

    pub fn write_extent(&mut self, extent: PartitionExtent) -> Result<()> {
        let operation_type = extent.operation_type();
        let start_offset = extent.start_block() * PartitionExtent::BLOCK_SIZE;
        let expected_size = extent.num_blocks() * PartitionExtent::BLOCK_SIZE;
        let blob = extent.into_raw();

        let decoded = match operation_type {
            Type::Zero => vec![0u8; expected_size as usize],
            Type::Replace => blob,
            Type::ReplaceXz | Type::ReplaceBz | Type::ReplaceZstd => {
                let mut buf = vec![0u8; expected_size as usize];
                let mut decoder: Box<dyn Read> = match operation_type {
                    Type::ReplaceXz => Box::new(XzDecoder::new(blob.as_slice())),
                    Type::ReplaceZstd => Box::new(ZstdDecoder::new(blob.as_slice()).unwrap()),
                    _ => Box::new(BzDecoder::new(blob.as_slice())),
                };

                decoder.read_exact(&mut buf)?;
                buf
            }
            _ => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Unsupported operation type: {:?}", operation_type),
                ))
            }
        };

        self.destination
            .seek(std::io::SeekFrom::Start(start_offset))
            .unwrap();

        self.destination.write_all(&decoded)?;
        Ok(())
    }
}
