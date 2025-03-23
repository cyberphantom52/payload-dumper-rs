pub mod update_metadata {
    include!(concat!(env!("OUT_DIR"), "/chromeos_update_engine.rs"));
}
use indicatif::{HumanBytes, MultiProgress, ProgressBar, ProgressStyle};
use prost::Message;
use std::{
    fmt::Display,
    fs::File,
    io::{BufReader, Read},
    path::{Path, PathBuf},
};
use update_metadata::{DeltaArchiveManifest, PartitionUpdate, Signatures};

use crate::{PartitionDecoder, PartitionExtent, PartitionReader};

const PAYLOAD_HEADER_MAGIC: &str = "CrAU";
/// From: https://android.googlesource.com/platform/system/update_engine/+/refs/heads/main/update_engine.conf
const PAYLOAD_MAJOR_VERSION: u64 = 2;
const HEADER_SIZE: u64 = size_of::<Header>() as u64;

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
    manifest: DeltaArchiveManifest,
    /// The signature of the first five fields. There could be multiple signatures if the key has changed.
    _manifest_signature: Signatures,

    file_path: PathBuf,
    multi_progress: MultiProgress,
    quiet: bool,
    verify: bool,
}

impl TryFrom<&mut File> for Header {
    type Error = std::io::Error;

    fn try_from(file: &mut File) -> Result<Self, Self::Error> {
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
            magic_number: magic.as_bytes().try_into().unwrap(),
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

        let header = Header::try_from(&mut file)?;

        let manifest = {
            let mut buf = vec![0u8; header.manifest_size as usize];
            file.read_exact(&mut buf)?;
            
            let mut manifest = DeltaArchiveManifest::decode(&buf[..])?;
            // Sort partitions by name for later binary search
            manifest
                .partitions
                .sort_by_key(|p| p.partition_name.to_owned());
            manifest
        };

        let _manifest_signature = {
            let mut buf = vec![0u8; header.manifest_signature_size as usize];
            file.read_exact(&mut buf)?;
            Signatures::decode(&buf[..])?
        };

        Ok(Payload {
            header,
            manifest,
            _manifest_signature,
            file_path: path.to_owned(),
            multi_progress: MultiProgress::new(),
            quiet: false,
            verify: true,
        })
    }
}

impl Payload {
    pub fn quiet(mut self) -> Self {
        self.quiet = true;
        self.multi_progress
            .set_draw_target(indicatif::ProgressDrawTarget::hidden());
        self
    }

    pub fn no_verify(mut self) -> Self {
        self.verify = false;
        self
    }

    fn data_offset(&self) -> u64 {
        HEADER_SIZE + self.header.manifest_size + self.header.manifest_signature_size as u64
    }

    pub fn header(&self) -> &Header {
        &self.header
    }

    pub fn partition_list(&self) -> Vec<String> {
        self.partitions()
            .iter()
            .map(|p| p.partition_name.to_owned())
            .collect()
    }

    pub fn print_partitions(&self) {
        if self.quiet {
            return;
        }

        for partition in self.partitions().iter() {
            let name = partition.partition_name.as_str();
            let size = HumanBytes(partition.new_partition_info.as_ref().unwrap().size() as u64);
            println!("{} ({})", name, size);
        }
    }

    pub fn partitions(&self) -> &[PartitionUpdate] {
        self.manifest.partitions.as_slice()
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

        let name = partition.partition_name.as_str();
        let file = File::create(output_dir.join(format!("{}.img", name)))?;

        let total_bytes = partition
            .new_partition_info
            .as_ref()
            .map(|info| info.size() as u64)
            .unwrap_or(0);

        let progress_bar = if self.quiet {
            None
        } else {
            Some(
                self.multi_progress.add(
                    ProgressBar::new(total_bytes as u64)
                        .with_message(name.to_owned())
                        .with_style(
                            ProgressStyle::with_template(
                                "{msg:.bold} [{bar:40.cyan/blue}] {bytes:>10}/{total_bytes:>10} \n\
                                    ETA: {eta} | Speed: {bytes_per_sec:.green}",
                            )
                            .unwrap(),
                        ),
                ),
            )
        };

        let operations = partition.operations.clone();
        let reader = PartitionReader::new(
            BufReader::new(File::open(&self.file_path).unwrap()),
            self.data_offset(),
            operations,
        );

        let mut decoder = PartitionDecoder::new(file);
        for extent in reader {
            let extent = extent?;
            let bytes_written = extent.num_blocks() * PartitionExtent::BLOCK_SIZE;
            decoder.write_extent(extent)?;

            if let Some(ref pb) = progress_bar {
                pb.inc(bytes_written);
            }
        }

        Ok(())
    }
}
