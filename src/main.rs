include!(concat!(env!("OUT_DIR"), "/protos/mod.rs"));
use rayon::prelude::*;
use std::path::{Path, PathBuf};
mod payload;
use clap::Parser;

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

    #[arg(short = 'c', long = "num_threads", default_value = "4")]
    num_threads: usize,

    #[arg(short = 'q', long = "quiet")]
    quiet: bool,

    payload_path: PathBuf,
}

impl Arguments {
    fn payload_path(&self) -> PathBuf {
        Path::new(&self.payload_path).to_owned()
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
    let payload_dir = payload_path.parent().unwrap();
    let mut payload = payload::Payload::try_from(args.payload_path().as_path())?;

    payload._set_quiet(args.quiet);
    if !args.quiet {
        println!("Payload: {}", payload.header());
    }
    if args.list {
        payload.print_partitions();
        return Ok(());
    }

    let output_dir = args
        .output
        .map_or_else(|| generate_output_path(payload_dir), |path| path);
    std::fs::create_dir_all(&output_dir)?;

    let partitions = if args.partitions.is_empty() {
        payload.partition_list()
    } else {
        args.partitions
    };

    rayon::ThreadPoolBuilder::new()
        .num_threads(args.num_threads)
        .build_global()
        .unwrap();

    partitions
        .par_iter()
        .try_for_each(|partition| payload.extract(partition, output_dir.as_path()))?;
    Ok(())
}
