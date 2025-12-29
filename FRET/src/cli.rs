use clap::{Parser, Subcommand};
use std::path::PathBuf;

// Argument parsing ================================================================================

#[derive(Parser,Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Kernel Image
    #[arg(short, long, value_name = "FILE")]
    pub kernel: PathBuf,

    /// Sets a custom config file
    #[arg(short, long, value_name = "FILE")]
    pub config: PathBuf,

    /// Sets the prefix of dumed files
    #[arg(short='n', long, value_name = "FILENAME")]
    pub dump_name: Option<PathBuf>,

    /// do time dumps
    #[arg(short='t', long)]
    pub dump_times: bool,

    /// do worst-case dumps
    #[arg(short='a', long)]
    pub dump_cases: bool,

    /// do trace dumps (if supported)
    #[arg(short='r', long)]
    pub dump_traces: bool,

    /// do graph dumps (if supported)
    #[arg(short='g', long)]
    pub dump_graph: bool,

    /// select a task for measurments
    #[arg(short='s', long)]
    pub select_task: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}
#[derive(Subcommand,Clone,Debug)]
pub enum Commands {
    /// run a single input
    Showmap {
        /// take this input
        #[arg(short, long)]
        input: PathBuf,
    },
    /// start fuzzing campaign
    Fuzz {
        /// disable heuristic
        #[arg(short, long)]
        random: bool,
        /// seed for randomness
        #[arg(short, long)]
        seed: Option<u64>,
        /// runtime in seconds
        #[arg(short, long)]
        time: Option<u64>,
    }
}

pub fn set_env_from_config(kernel : &PathBuf, path : &PathBuf) {
    let is_csv = path.as_path().extension().map_or(false, |x| x=="csv");
    if !is_csv {
        let lines = std::fs::read_to_string(path).expect("Config file not found");
        let lines = lines.lines().filter(
            |x| x.len()>0
        );
        for l in lines {
            let pair = l.split_once('=').expect("Non VAR=VAL line in config");
            std::env::set_var(pair.0, pair.1);
        }
    } else {
        let mut reader = csv::Reader::from_path(path).expect("CSV read from config failed");
        let p = kernel.as_path();
        let stem = p.file_stem().expect("Kernel filename error").to_str().unwrap();
        let mut found = false;
        for r in reader.records() {
            let rec = r.expect("CSV entry error");
            if stem == &rec[0] {
                println!("Config from file {:?}", rec);
                found = true;
                std::env::set_var("FUZZ_MAIN", &rec[1]);
                std::env::set_var("FUZZ_INPUT", &rec[2]);
                std::env::set_var("FUZZ_INPUT_LEN", &rec[3]);
                std::env::set_var("BREAKPOINT", &rec[4]);
                break;
            }
        }
        if !found {
            eprintln!("No config found for kernel {:?}", stem);
        }
    }
}

pub fn get_interrupt_config(kernel : &PathBuf, path : &PathBuf) -> Vec<(usize,u32)>{
    let is_csv = path.as_path().extension().map_or(false, |x| x=="csv");
    if !is_csv {
        panic!("Interrupt config must be inside a CSV file");
    } else {
        let mut reader = csv::Reader::from_path(path).expect("CSV read from config failed");
        let p = kernel.as_path();
        let stem = p.file_stem().expect("Kernel filename error").to_str().unwrap();
        for r in reader.records() {
            let rec = r.expect("CSV entry error");
            if stem == &rec[0] {
                let ret = rec[6].split(';').filter(|x| x != &"").map(|x| {
                    let pair = x.split_once('#').expect("Interrupt config error");
                    (pair.0.parse().expect("Interrupt config error"), pair.1.parse().expect("Interrupt config error"))
                }).collect();
                println!("Interrupt config {:?}", ret);
                return ret;
            }
        }
    }
    return Vec::new();
}