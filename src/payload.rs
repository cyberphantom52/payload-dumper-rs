use std::{fmt::Display, fs::OpenOptions, path::Path};

use crate::update_metadata::{DeltaArchiveManifest, PartitionUpdate, Signatures};
use indicatif::HumanBytes;
use memmap2::MmapMut;
use prost::Message;
use rayon::iter::ParallelIterator;

use crate::{PartitionDecoder, PartitionReader};

const PAYLOAD_HEADER_MAGIC: &str = "CrAU";
/// From: https://android.googlesource.com/platform/system/update_engine/+/refs/heads/main/update_engine.conf
const PAYLOAD_MAJOR_VERSION: u64 = 2;
const HEADER_SIZE: u64 = 24;

#[derive(Debug)]
pub struct Header<'a> {
    /// Magic string “CrAU” identifying this is an update payload.
    _magic_number: &'a [u8],
    /// Payload major version number.
    major_version: u64,
    /// Manifest size in bytes.
    manifest_size: u64,
    /// Manifest signature blob size in bytes (only in major version 2).
    manifest_signature_size: u32,
}

impl Display for Header<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Payload Version: {}", self.major_version)?;
        writeln!(f, "Payload Manifest Size: {}", self.manifest_size)?;
        write!(
            f,
            "Payload Manifest Signature Size: {}",
            self.manifest_signature_size
        )
    }
}

/// Reference: https://android.googlesource.com/platform/system/update_engine/#update-payload-file-specification
pub struct Payload<'a> {
    /// The header of the payload.
    header: Header<'a>,
    /// The list of operations to be performed.
    manifest: DeltaArchiveManifest,
    /// The signature of the first five fields. There could be multiple signatures if the key has changed.
    _manifest_signature: Signatures,

    data: &'a [u8],
}

impl<'a> TryFrom<&'a [u8]> for Header<'a> {
    type Error = std::io::Error;

    fn try_from(buffer: &'a [u8]) -> Result<Self, Self::Error> {
        // Validate magic number
        let magic = &buffer[..4];
        let magic_str = String::from_utf8_lossy(&magic).to_string();

        if magic_str != PAYLOAD_HEADER_MAGIC {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Invalid android payload magic number",
            ));
        }

        // Read and validate version
        let major_version = {
            let buf: [u8; 8] = buffer[4..12].try_into().map_err(|_| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Invalid version buffer length",
                )
            })?;
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
            let buf: [u8; 8] = buffer[12..20].try_into().map_err(|_| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Invalid manifest size buffer length",
                )
            })?;
            u64::from_be_bytes(buf)
        };

        let manifest_signature_size = {
            let buf: [u8; 4] = buffer[20..24].try_into().map_err(|_| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Invalid manifest signature size buffer length",
                )
            })?;
            u32::from_be_bytes(buf)
        };

        Ok(Header {
            _magic_number: magic,
            major_version,
            manifest_size,
            manifest_signature_size,
        })
    }
}

impl<'a> TryFrom<&'a [u8]> for Payload<'a> {
    type Error = std::io::Error;
    fn try_from(buffer: &'a [u8]) -> Result<Self, Self::Error> {
        let header = Header::try_from(buffer)?;
        let mut start = HEADER_SIZE as usize;

        let manifest = {
            let end = start + header.manifest_size as usize;
            let buf = &buffer[start..end];
            start = end;

            let mut manifest = DeltaArchiveManifest::decode(&buf[..])?;
            // Sort partitions by name for later binary search
            manifest
                .partitions
                .sort_by_key(|p| p.partition_name.to_owned());
            manifest
        };

        let _manifest_signature = {
            let end = start + header.manifest_signature_size as usize;
            let buf = &buffer[start..end];

            Signatures::decode(&buf[..])?
        };

        Ok(Payload {
            header,
            manifest,
            _manifest_signature,
            data: buffer,
        })
    }
}

impl<'a> Payload<'a> {
    fn data_offset(&self) -> u64 {
        HEADER_SIZE + self.header.manifest_size + self.header.manifest_signature_size as u64
    }

    pub fn header(&self) -> &'a Header {
        &self.header
    }

    pub fn partitions(&'a self) -> &'a [PartitionUpdate] {
        self.manifest.partitions.as_slice()
    }

    pub fn partition_list(&'a self) -> Vec<&'a str> {
        self.partitions()
            .iter()
            .map(|p| p.partition_name.as_str())
            .collect()
    }

    pub fn print_partitions(&self) {
        for partition in self.partitions().iter() {
            let name = partition.partition_name.as_str();
            let size = HumanBytes(partition.new_partition_info.as_ref().unwrap().size() as u64);
            println!("{} ({})", name, size);
        }
    }

    fn partition(&self, partition: &str) -> Result<&PartitionUpdate, usize> {
        self.partitions()
            .binary_search_by_key(&partition, |p| p.partition_name.as_str())
            .map(|idx| &self.partitions()[idx])
    }

    pub fn extract(&self, partition: &str, output_dir: &Path) -> std::io::Result<()> {
        let partition = if let Ok(partition) = self.partition(partition) {
            partition
        } else {
            println!("Partition not found: {partition}");
            return Ok(());
        };

        let total_bytes = partition
            .new_partition_info
            .as_ref()
            .map(|info| info.size() as u64)
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Partition info is missing or invalid",
                )
            })?;

        let name = partition.partition_name.as_str();
        let file_path = output_dir.join(name).with_extension("img");

        /* TODO: Enable progress bar
        let progress_bar = self.multi_progress.add(
            ProgressBar::new(total_bytes as u64)
                .with_message(name.to_owned())
                .with_style(
                    ProgressStyle::with_template(
                        "{msg:.bold} [{bar:40.cyan/blue}] {bytes:>10}/{total_bytes:>10} \n\
                            ETA: {eta} | Speed: {bytes_per_sec:.green}",
                    )
                    .unwrap(),
                ),
        );
        */

        let operations = partition.operations.as_slice();
        let reader = PartitionReader::new(self.data, self.data_offset(), operations);

        /* TODO: Enable checksum verification
        if !self.verify {
            reader = reader.without_checksum();
        }
        */

        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(true)
            .open(&file_path)?;
        file.set_len(total_bytes)?;
        let mmapp = unsafe { MmapMut::map_mut(&file)? };

        reader.par_iter().for_each(|extent| {
            let ptr = mmapp.as_ptr();
            let len = mmapp.len();
            let mut_slice = unsafe { std::slice::from_raw_parts_mut(ptr as *mut u8, len) };
            let mut decoder = PartitionDecoder::new(mut_slice);
            let extent = extent.unwrap();
            decoder.write_extent(extent).unwrap();
        });

        mmapp.flush()
    }
}
