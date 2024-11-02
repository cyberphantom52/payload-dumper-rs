/*
    Reference: https://android.googlesource.com/platform/system/update_engine/#update-payload-file-specification
    Reference: https://github.com/ssut/payload-dumper-go/blob/main/payload.go
*/
use crate::update_metadata::{DeltaArchiveManifest, Signatures};
use std::fs::File;

#[derive(Debug)]
pub struct Header {
    magic_number: [u8; 4],
    major_version: u64,
    manifest_size: u64,
    manifest_signature_size: u32,
}

#[derive(Debug)]
pub struct Payload {
    header: Header,
    manifest: Box<DeltaArchiveManifest>,
    manifest_signature: Box<Signatures>,
    metadata_size: u64,
    data_offset: u64,
    file: Box<File>,
}
