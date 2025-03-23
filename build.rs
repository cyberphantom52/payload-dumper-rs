use std::io::Result;

fn main() -> Result<()> {
    prost_build::compile_protos(&["protos/update_metadata.proto"], &["protos/"])?;
    Ok(())
}
