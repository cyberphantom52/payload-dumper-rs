use crate::update_metadata::{
    install_operation::Type, DeltaArchiveManifest, PartitionUpdate, Signatures,
};
use bzip2::bufread::BzDecoder;
use indicatif::{HumanBytes, MultiProgress, ProgressBar, ProgressStyle};
use protobuf::Message;
use sha2::{Digest, Sha256};
use std::{fmt::Display, fs::File, io::Read, os::unix::fs::FileExt, path::Path};
use xz::bufread::XzDecoder;
use zstd::Decoder;

const PAYLOAD_HEADER_MAGIC: &str = "CrAU";
/// From: https://android.googlesource.com/platform/system/update_engine/+/refs/heads/main/update_engine.conf
const PAYLOAD_MAJOR_VERSION: u64 = 2;
const HEADER_SIZE: u64 = size_of::<Header>() as u64;
const BLOCK_SIZE: u64 = 4096;

#[derive(Debug)]
pub struct Header {
    /// Magic string “CrAU” identifying this is an update payload.
    magic_number: [u8; 4],
    /// Payload major version number.
    major_version: u64,
    /// Manifest size in bytes.
    manifest_size: u64,
    /// Manifest signature blob size in bytes (only in major version 2).
    manifest_signature_size: u32,
}

impl Display for Header {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Payload Version: {}\nPayload Manifest Size: {}\nPayload Manifest Signature Size: {}",
            self.major_version, self.manifest_size, self.manifest_signature_size
        )
    }
}

/// Reference: https://android.googlesource.com/platform/system/update_engine/#update-payload-file-specification
pub struct Payload {
    /// The header of the payload.
    header: Header,
    /// The list of operations to be performed.
    manifest: Box<DeltaArchiveManifest>,
    /// The signature of the first five fields. There could be multiple signatures if the key has changed.
    manifest_signature: Box<Signatures>,
    file: Box<File>,

    multi_progress: MultiProgress,
    quiet: bool,
}

impl TryFrom<&mut File> for Header {
    type Error = std::io::Error;

    fn try_from(file: &mut File) -> Result<Self, Self::Error> {
        // Read and validate version
        let major_version = {
            let mut buf = [0u8; 8];
            file.read_exact(&mut buf)?;
            let version = u64::from_be_bytes(buf);

            if version != PAYLOAD_MAJOR_VERSION {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Invalid payload version: {version}"),
                ));
            }
            version
        };

        // Read manifest and signature sizes
        let manifest_size = {
            let mut buf = [0u8; 8];
            file.read_exact(&mut buf)?;
            u64::from_be_bytes(buf)
        };

        let manifest_signature_size = {
            let mut buf = [0u8; 4];
            file.read_exact(&mut buf)?;
            u32::from_be_bytes(buf)
        };

        Ok(Header {
            magic_number: PAYLOAD_HEADER_MAGIC.as_bytes().try_into().unwrap(),
            major_version,
            manifest_size,
            manifest_signature_size,
        })
    }
}

impl TryFrom<&Path> for Payload {
    type Error = std::io::Error;
    fn try_from(path: &Path) -> Result<Self, Self::Error> {
        let mut file = File::open(path)?;

        // Validate magic number
        let magic = {
            let mut buffer = [0u8; 4];
            file.read_exact(&mut buffer)?;
            String::from_utf8_lossy(&buffer).to_string()
        };

        if magic != PAYLOAD_HEADER_MAGIC {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Invalid android payload magic number",
            ));
        }

        // Read header, manifest, and signature
        let header = Header::try_from(&mut file)?;

        let manifest = {
            let mut buf = vec![0u8; header.manifest_size as usize];
            file.read_exact(&mut buf)?;
            let mut manifest = DeltaArchiveManifest::parse_from_bytes(&buf)?;

            // Sort partitions by name for later binary search
            manifest
                .partitions
                .sort_by_key(|p| p.partition_name().to_owned());
            Box::new(manifest)
        };

        let manifest_signature = {
            let mut buf = vec![0u8; header.manifest_signature_size as usize];
            file.read_exact(&mut buf)?;
            Box::new(Signatures::parse_from_bytes(&buf)?)
        };

        Ok(Payload {
            header,
            manifest,
            manifest_signature,
            file: Box::new(file),
            multi_progress: MultiProgress::new(),
            quiet: false,
        })
    }
}

impl Payload {
    pub fn _set_quiet(&mut self, quiet: bool) {
        self.quiet = quiet;
        if quiet {
            self.multi_progress
                .set_draw_target(indicatif::ProgressDrawTarget::hidden());
        }
    }

    fn data_offset(&self) -> u64 {
        HEADER_SIZE + self.header.manifest_size + self.header.manifest_signature_size as u64
    }

    pub fn header(&self) -> &Header {
        &self.header
    }

    fn read_data_blob(&self, offset: u64, len: u64) -> Result<Vec<u8>, std::io::Error> {
        let mut buf = vec![0u8; len as usize];
        self.file
            .read_exact_at(&mut buf, self.data_offset() + offset)?;
        Ok(buf)
    }

    pub fn partition_list(&self) -> Vec<String> {
        self.partitions()
            .iter()
            .map(|p| p.partition_name().to_owned())
            .collect()
    }

    pub fn print_partitions(&self) {
        if self.quiet {
            return;
        }

        for partition in self.partitions() {
            let name = partition.partition_name();
            let size = HumanBytes(partition.new_partition_info.size() as u64);
            println!("{} ({})", name, size);
        }
    }

    pub fn partitions(&self) -> &[PartitionUpdate] {
        self.manifest.partitions.as_slice()
    }

    fn partition(&self, partition: &str) -> Result<&PartitionUpdate, usize> {
        self.partitions()
            .binary_search_by_key(&partition, |p| p.partition_name())
            .map(|idx| &self.partitions()[idx])
    }

    pub fn extract(&self, partition: &str, output_dir: &Path) -> Result<(), std::io::Error> {
        let partition = self.partition(partition).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Partition {} not found", partition),
            )
        })?;
        let name = partition.partition_name();
        let file = File::create(output_dir.join(format!("{}.img", name)))?;
        let progress_bar = self.multi_progress.add(
            ProgressBar::new(partition.new_partition_info.size() as u64)
                .with_message(name.to_owned())
                .with_style(
                    ProgressStyle::with_template(
                        "{msg:.bold} [{bar:40.cyan/blue}] {bytes:>10}/{total_bytes:>10} \n\
                        ETA: {eta} | Speed: {bytes_per_sec:.green}",
                    )
                    .unwrap(),
                ),
        );

        for operation in partition.operations.iter() {
            let dst_extent = operation.dst_extents.first().ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Invalid operation.dst_extents for the partition {name}"),
                )
            })?;

            let expected_size = dst_extent.num_blocks() * BLOCK_SIZE;
            let blob = self.read_data_blob(operation.data_offset(), operation.data_length())?;

            // Verify hash for non-zero operations
            if operation.type_() != Type::ZERO {
                let hash = hex::encode(Sha256::digest(&blob));
                let expected_hash = hex::encode(operation.data_sha256_hash());

                if hash != expected_hash {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("SHA256 hash mismatch. Expected: {expected_hash}, Got: {hash}"),
                    ));
                }
            }

            let decoded = match operation.type_() {
                Type::ZERO => vec![0u8; expected_size as usize],
                Type::REPLACE => blob,
                Type::REPLACE_XZ | Type::REPLACE_BZ => {
                    let mut decoder: Box<dyn Read> = match operation.type_() {
                        Type::REPLACE_XZ => Box::new(XzDecoder::new(blob.as_slice())),
                        _ => Box::new(BzDecoder::new(blob.as_slice())),
                    };
                    let mut decoded = vec![0u8; expected_size as usize];
                    decoder.read_exact(&mut decoded)?;
                    decoded
                }
                Type::REPLACE_ZSTD => {
                    let mut decoder = Decoder::new(blob.as_slice())?;
                    let mut decoded = vec![0u8; expected_size as usize];
                    decoder.read_exact(&mut decoded)?;
                    decoded
                }
                _ => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Invalid operation type: {:?}", operation.type_()),
                    ))
                }
            };

            file.write_all_at(&decoded, dst_extent.start_block() * BLOCK_SIZE)?;

            progress_bar.inc(decoded.len() as u64);
        }

        Ok(())
    }
}
