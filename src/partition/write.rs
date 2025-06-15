use super::PartitionExtent;
use crate::update_metadata::install_operation::Type;
use bzip2::bufread::BzDecoder;
use std::io::Read;
use std::io::Result;
use xz::bufread::XzDecoder;
use zstd::Decoder as ZstdDecoder;

#[derive(Debug)]
pub struct PartitionDecoder<'a> {
    destination: &'a mut [u8],
}

impl<'a> PartitionDecoder<'a> {
    pub fn new(destination: &'a mut [u8]) -> Self {
        Self { destination }
    }

    pub fn write_extent(&mut self, extent: PartitionExtent) -> Result<()> {
        let operation_type = extent.operation_type();
        let expected_size = (extent.num_blocks() * PartitionExtent::BLOCK_SIZE) as usize;
        let start_offset = (extent.start_block() * PartitionExtent::BLOCK_SIZE) as usize;
        let end_offset = start_offset + expected_size as usize;
        let blob = extent.as_slice();

        match operation_type {
            Type::Zero => return Ok(()), // No data to write for zero operation
            Type::Replace => self.destination[start_offset..end_offset].copy_from_slice(blob),
            Type::ReplaceXz | Type::ReplaceBz | Type::ReplaceZstd => {
                let mut decoder: Box<dyn Read> = match operation_type {
                    Type::ReplaceXz => Box::new(XzDecoder::new(blob)),
                    Type::ReplaceZstd => Box::new(ZstdDecoder::new(blob)?),
                    _ => Box::new(BzDecoder::new(blob)),
                };

                decoder.read_exact(&mut self.destination[start_offset..end_offset])?;
            }
            _ => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Unsupported operation type: {:?}", operation_type),
                ))
            }
        };

        Ok(())
    }
}
