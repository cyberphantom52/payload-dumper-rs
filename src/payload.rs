use crate::update_metadata::{DeltaArchiveManifest, Signatures};
use protobuf::Message;
use std::{fs::File, io::Read, path::Path};

const PAYLOAD_HEADER_MAGIC: &str = "CrAU";
const PAYLOAD_MAJOR_VERSION: u64 = 2;

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

/// Reference: https://android.googlesource.com/platform/system/update_engine/#update-payload-file-specification
#[derive(Debug)]
pub struct Payload {
    /// The header of the payload.
    header: Header,
    /// The list of operations to be performed.
    manifest: Box<DeltaArchiveManifest>,
    /// The signature of the first five fields. There could be multiple signatures if the key has changed.
    manifest_signature: Box<Signatures>,
    file: Box<File>,
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
        })
    }
}
