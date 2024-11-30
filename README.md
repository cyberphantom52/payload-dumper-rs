# payload-dumper-rs

An Android OTA Payload Dumper written in Rust.

This tool allows you to extract partition images from an Android OTA `payload.bin` file.

### What is an Android OTA Payload?

Android devices receive software updates in the form of Over-The-Air (OTA) packages. These OTA packages often contain a file named `payload.bin`, which includes the binary diffs of the partitions that need to be updated on the device.

The `payload.bin` file follows a specific format as defined by the Android Update Engine. It contains:

The payload file allows for efficient updates by only including the differences (deltas) between the current and new versions of partitions, rather than the full partition images. This reduces the size of the OTA update package.

**Reference Documentation:**
- [Android OTA Updates](https://source.android.com/devices/tech/ota/ab/ab_ota_payload)
- [Update Engine Payload Generation](https://android.googlesource.com/platform/system/update_engine/+/refs/heads/main/README.md#file-format)

## Features

- List partitions available in the payload.
- Extract partitions from Android OTA payload files.
- Extract multiple partitions in parallel.
- Progress bars to show extraction progress.

## Requirements

- **Rust**.
- **Protocol Buffers Compiler** (`protoc`).

## Installation

### Install Rust

If you don't have Rust installed, you can install it using [rustup](https://rustup.rs/):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Clone and Build the Project

Clone the repository and build the project using Cargo:

```bash
git clone https://github.com/cyberphantom52/payload-dumper-rs.git
cd payload-dumper-rs
cargo build --release
```

This will generate the executable in the `target/release` directory.

## Usage

```bash
payload-dumper-rs [OPTIONS] <PAYLOAD_PATH>
```

### Options

- `-l`, `--list`
  List the available partitions in the payload.

- `-p`, `--partitions <PARTITIONS>`
  Comma-separated list of partitions to extract.

- `-o`, `--output <OUTPUT>`
  Output directory for the extracted images.

- `-c`, `--num_threads <NUM_THREADS>`
  Number of threads to use for extraction (default: 4).

### Examples

#### List Available Partitions

To list the partitions available in the payload:

```bash
payload-dumper-rs -l payload.bin
```

Example output:

```
Payload Version: 2
Payload Manifest Size: 123456
Payload Manifest Signature Size: 789
system (1.5 GiB)
vendor (500 MiB)
boot (64 MiB)
```

#### Extract All Partitions

To extract all partitions:

```bash
payload-dumper-rs payload.bin
```

By default, the extracted images will be saved in a directory named `extracted_<timestamp>` in the same directory as the `payload.bin`.

#### Extract Specific Partitions

To extract specific partitions (e.g., `system`, `boot`):

```bash
payload-dumper-rs -p system,boot payload.bin
```

#### Specify Output Directory

To specify an output directory:

```bash
payload-dumper-rs -o /path/to/output payload.bin
```

#### Set Number of Threads

To set the number of threads for extraction:

```bash
payload-dumper-rs -c 8 payload.bin
```

## Notes

- The tool will create the output directory if it does not exist.
- The tool verifies the SHA256 hash of each data blob before extraction.
- Supports payloads with major version 2 (as per the Android OTA update payload specification).

## Development

### Dependencies

- [clap](https://crates.io/crates/clap)
  For command-line argument parsing.

- [indicatif](https://crates.io/crates/indicatif)
  For progress bars and user-friendly output.

- [protobuf](https://crates.io/crates/protobuf)
  For handling Protocol Buffers serialization/deserialization.

- [rayon](https://crates.io/crates/rayon)
  For parallel extraction using multi-threading.

- [sha2](https://crates.io/crates/sha2)
  For SHA256 hashing of data blobs.

- [xz2](https://crates.io/crates/xz2) and [bzip2](https://crates.io/crates/bzip2)
  For decompressing data blobs compressed with xz or bzip2.

### Building the Project

The project uses a build script (`build.rs`) to generate Rust code from the Protocol Buffers definition. Ensure that the `protoc` compiler is installed and accessible in your `PATH`.

```bash
cargo build --release
```

## License

This project is licensed under the [GPL-3.0 License](LICENSE).


## Credits

- **Inam Ul Haq** - Author
- Inspired by the [`payload-dumper-go`](https://github.com/ssut/payload-dumper-go) by [ssut](https://github.com/ssut).

## Contributing

Contributions are welcome! If you have suggestions, bug reports, or patches, feel free to contribute to the project.
