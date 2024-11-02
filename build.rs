fn main() {
    println!("cargo:rerun-if-changed=protos/update_metadata.proto");

    protobuf_codegen::Codegen::new()
        .protoc()
        .protoc_path(&protoc_bin_vendored::protoc_bin_path().unwrap())
        .include("protos")
        .input("protos/update_metadata.proto")
        .cargo_out_dir("protos")
        .run_from_script();
}
