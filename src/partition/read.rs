use std::io::{BufReader, Read, Result, Seek, Write};

use sha2::{Digest, Sha256};

use super::PartitionExtent;
use crate::payload::update_metadata::{install_operation::Type, InstallOperation};

pub struct PartitionReader<R: Read + Seek> {
    source: BufReader<R>,
    data_offset: u64,
    operations: Vec<InstallOperation>,
    checksum: Option<Sha256>,
}

impl<R: Read + Seek> PartitionReader<R> {
    pub fn new(source: BufReader<R>, data_offset: u64, operations: Vec<InstallOperation>) -> Self {
        Self {
            source,
            data_offset,
            operations,
            checksum: Some(Sha256::new()),
        }
    }

    pub fn without_checksum(mut self) -> Self {
        self.checksum = None;
        self
    }
}

impl<R: Read + Seek> Iterator for PartitionReader<R> {
    type Item = Result<PartitionExtent>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.operations.is_empty() {
            return None;
        }

        let operation = self.operations.remove(0);
        let operation_type = operation.r#type();

        let dst_extent = operation.dst_extents.first()?;
        let mut buf = vec![0u8; operation.data_length() as usize];

        self.source
            .seek(std::io::SeekFrom::Start(
                self.data_offset + operation.data_offset(),
            ))
            .unwrap();
        self.source.read_exact(&mut buf).unwrap();

        if let Some(ref mut hasher) = self.checksum {
            if operation.r#type() != Type::Zero {
                hasher.update(&buf);
                let hash = hex::encode(hasher.finalize_reset());
                let expected_hash = hex::encode(operation.data_sha256_hash());

                if hash != expected_hash {
                    return Some(Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("SHA256 hash mismatch. Expected: {expected_hash}, Got: {hash}"),
                    )));
                }
            }
        }

        Some(Ok(PartitionExtent::new(buf, operation_type, dst_extent)))
    }
}
