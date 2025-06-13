use std::io::Result;

use rayon::prelude::*;

use super::PartitionExtent;
use crate::update_metadata::InstallOperation;

pub struct PartitionReader<'a> {
    source: &'a [u8],
    data_offset: u64,
    operations: &'a [InstallOperation],
    op_index: usize,
}

impl<'a> PartitionReader<'a> {
    pub fn new(source: &'a [u8], data_offset: u64, operations: &'a [InstallOperation]) -> Self {
        Self {
            source,
            data_offset,
            operations,
            op_index: 0,
        }
    }

    pub fn par_iter(&'a self) -> impl ParallelIterator<Item = Result<PartitionExtent>> + 'a {
        self.operations.par_iter().map(move |operation| {
            let operation_type = operation.r#type();
            let dst_extent = operation.dst_extents.first().ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "No destination extent")
            })?;
            let start = (self.data_offset + operation.data_offset()) as usize;
            let end = start + operation.data_length() as usize;

            if end > self.source.len() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "Operation exceeds source bounds",
                ));
            }

            let buf = &self.source[start..end];

            Ok(PartitionExtent::new(
                buf.to_vec(),
                operation_type,
                dst_extent,
            ))
        })
    }
}

impl Iterator for PartitionReader<'_> {
    type Item = Result<PartitionExtent>;

    fn next(&mut self) -> Option<Self::Item> {
        let operation = self.operations.get(self.op_index)?;
        self.op_index += 1;
        let operation_type = operation.r#type();

        let dst_extent = operation.dst_extents.first()?;
        let start = (self.data_offset + operation.data_offset()) as usize;
        let end = start + operation.data_length() as usize;

        if end > self.source.len() {
            return Some(Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "Operation exceeds source bounds",
            )));
        }

        let buf = &self.source[start..end];

        Some(Ok(PartitionExtent::new(
            buf.to_vec(),
            operation_type,
            dst_extent,
        )))
    }
}
