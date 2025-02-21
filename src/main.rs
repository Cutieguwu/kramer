mod recovery;
mod mapping;

use clap::Parser;
use libc::O_DIRECT;
use mapping::MapFile;
use recovery::Recover;
use std::{
    fs::{File, OpenOptions},
    io::{self, Seek, SeekFrom},
    os::unix::fs::OpenOptionsExt,
    path::PathBuf,
};


const FB_SECTOR_SIZE: u16 = 2048;


#[derive(Parser, Debug)]
struct Args {
    /// Path to source file or block device
    #[arg(short, long, value_hint = clap::ValueHint::DirPath)]
    input: PathBuf,

    /// Path to output file. Defaults to {input}.iso
    #[arg(short, long, value_hint = clap::ValueHint::DirPath)]
    output: Option<PathBuf>,

    /// Path to rescue map. Defaults to {input}.map
    #[arg(short, long, value_hint = clap::ValueHint::DirPath)]
    map: Option<PathBuf>,

    /// Max number of consecutive sectors to test as a group
    #[arg(short, long, default_value_t = 128)]
    cluster_length: u16,

    /// Number of brute force read passes
    #[arg(short, long, default_value_t = 2)]
    brute_passes: usize,

    /// Sector size
    #[arg(short, long, default_value_t = FB_SECTOR_SIZE)]
    sector_size: u16,
}


fn main() {
    let config = Args::parse();

    // Live with it, prefer to use expect() here.
    // I'm lazy and don't want to mess around with comparing error types.
    // Thus, any error in I/O here should be treated as fatal.

    let mut input: File = {
        match OpenOptions::new()
            .custom_flags(O_DIRECT)
            .read(true)
            .write(false)
            .append(false)
            .create(false)
            .open(&config.input.as_path())
        {
            Ok(f) => f,
            Err(err) => panic!("Failed to open input file: {:?}", err)
        }
    };

    let mut output: File = {
        // Keep this clean, make a short-lived binding.
        let path = get_path(
            &config.output,
            &config.input.to_str().unwrap(),
            "iso"
        );

        match OpenOptions::new()
            .custom_flags(O_DIRECT)
            .read(true)
            .write(true)
            .create(true)
            .open(path)
        {
            Ok(f) => f,
            Err(err) => panic!("Failed to open/create output file. {:?}", err)
        }
    };

    // Check if output file is shorter than input.
    // If so, autoextend the output file.
    {
        let input_len = get_stream_length(&mut input)
            .expect("Failed to get the length of the input data.");
        let output_len = get_stream_length(&mut output)
            .expect("Failed to get the length of the output file.");

        if output_len < input_len {
            output.set_len(input_len)
                .expect("Failed to autofill output file.")
        }
    }

    let map: MapFile = {
        let path = get_path(
            &config.output,
            &config.input.to_str().unwrap(),
            "map"
        );

        let file = match OpenOptions::new()
            .read(true)
            .create(true)
            .open(path)
        {
            Ok(f) => f,
            Err(err) => panic!("Failed to open/create mapping file. {:?}", err)
        }; 
        
        if let Ok(map) = MapFile::try_from(file) {
            map
        } else {
            MapFile::new(config.sector_size)
        }
    };

    let recover_tool  = Recover::new(config, input, output, map);

    recover_tool.run_full();

    todo!("Recovery, Map saving, and closure of all files.");
}

/// Generates a file path if one not provided.
/// source_name for fallback name.
fn get_path(
    output: &Option<PathBuf>,
    source_name: &str,
    extention: &str
) -> PathBuf {
    if let Some(f) = output {
        f.to_owned()
    } else {
        PathBuf::from(format!(
            "{:?}.{}",
            source_name,
            extention,
        ))
        .as_path()
        .to_owned()
    }
}

/// Get length of data stream.
/// Physical length of data stream in bytes
/// (multiple of sector_size, rather than actual).
fn get_stream_length<S: Seek>(file: &mut S) -> io::Result<u64> {
    let len = file.seek(SeekFrom::End(0))?;

    let _ = file.seek(SeekFrom::Start(0));

    Ok(len)
}