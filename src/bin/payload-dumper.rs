use clap::Parser;
use memmap2::Mmap;
use payload_dumper_rs::Payload;
use rayon::prelude::*;
use std::{
    fs::File,
    path::{Path, PathBuf},
};

#[derive(Parser, Debug)]
#[command(
    author = "Inam Ul Haq",
    version = "1.0",
    about = "Android OTA payload dumper"
)]
struct Arguments {
    #[arg(short = 'l', long = "list")]
    list: bool,

    #[arg(short = 'p', long = "partitions", value_delimiter = ',')]
    partitions: Vec<String>,

    #[arg(short = 'o', long = "output")]
    output: Option<PathBuf>,

    #[arg(short = 'c', long = "num_threads")]
    num_threads: Option<usize>,

    #[arg(short = 'q', long = "quiet")]
    quiet: bool,

    #[arg(long = "no-verify")]
    no_verify: bool,

    payload_path: PathBuf,
}

impl Arguments {
    fn payload_path(&self) -> &Path {
        self.payload_path.as_path()
    }
}

fn generate_output_path(base_dir: &Path) -> PathBuf {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap();
    let dir_name = format!("extracted_{}", now.as_secs());
    base_dir.join(dir_name)
}

fn main() -> Result<(), std::io::Error> {
    let args: Arguments = Arguments::parse();

    let payload_path = args.payload_path();
    /* Default Path to use if output path is not provided */
    let default_path = generate_output_path(payload_path.parent().unwrap());
    let file = File::open(payload_path)?;
    // TODO: Is it necessary to memmap the payload file?
    let memmap = unsafe { Mmap::map(&file)? };

    let payload = Payload::try_from(&memmap[..])?;

    if !args.quiet {
        println!("Payload: {}", payload.header());
    }
    if args.list {
        payload.print_partitions();
        return Ok(());
    }

    let output_dir = args.output.unwrap_or_else(|| default_path);
    std::fs::create_dir_all(&output_dir)?;

    let partitions = if args.partitions.is_empty() {
        payload.partition_list()
    } else {
        args.partitions
            .iter()
            .map(|p| p.as_str())
            .collect::<Vec<&str>>()
    };

    let num_threads = args.num_threads.unwrap_or_else(|| {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
    });

    rayon::ThreadPoolBuilder::new()
        .num_threads(num_threads)
        .build_global()
        .unwrap();

    partitions
        .par_iter()
        .try_for_each(|partition| payload.extract(partition, output_dir.as_path()))?;
    Ok(())
}
