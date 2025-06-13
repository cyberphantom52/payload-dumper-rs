use crate::update_metadata::{install_operation::Type, Extent};

pub struct PartitionExtent<'a> {
    blob: &'a [u8],
    operation_type: Type,
    start_block: u64,
    num_blocks: u64,
}

impl<'a> PartitionExtent<'a> {
    pub const BLOCK_SIZE: u64 = 4096;

    pub fn new(blob: &'a [u8], operation_type: Type, extent: &Extent) -> Self {
        let start_block = extent.start_block();
        let num_blocks = extent.num_blocks();

        Self {
            blob,
            start_block,
            num_blocks,
            operation_type,
        }
    }

    pub fn operation_type(&self) -> Type {
        self.operation_type
    }

    pub fn start_block(&self) -> u64 {
        self.start_block
    }

    pub fn num_blocks(&self) -> u64 {
        self.num_blocks
    }

    pub fn as_slice(&self) -> &'a [u8] {
        self.blob
    }

    pub fn into_raw(self) -> Vec<u8> {
        self.blob.to_vec()
    }
}
