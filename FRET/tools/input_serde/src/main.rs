use either::Either::{self, Left, Right};
use hashbrown::HashMap;
use rand::rngs::StdRng;
use std::path::PathBuf;
use std::{env,fs};
use fret::systemstate::{ExecInterval, RTOSJob, target_os::SystemTraceData, target_os::freertos::FreeRTOSTraceMetadata, target_os::SystemState, target_os::TaskControlBlock, helpers::interrupt_times_to_input_bytes};
use libafl::inputs::multi::MultipartInput;
use libafl::inputs::{BytesInput, Input};
use std::io::Write;
use clap::Parser;
use itertools::{assert_equal, join, Itertools};
use rand::RngCore;
use libafl::inputs::HasMutatorBytes;

const MAX_NUM_INTERRUPT: usize = 128;
const NUM_INTERRUPT_SOURCES: usize = 6; // Keep in sync with qemu-libafl-bridge/hw/timer/armv7m_systick.c:319 and  FreeRTOS/FreeRTOS/Demo/CORTEX_M3_MPS2_QEMU_GCC/init/startup.c:216
pub const QEMU_ICOUNT_SHIFT: u32 = 5;
pub const QEMU_ISNS_PER_SEC: u32 = u32::pow(10, 9) / u32::pow(2, QEMU_ICOUNT_SHIFT);
pub const QEMU_ISNS_PER_USEC: f32 = QEMU_ISNS_PER_SEC as f32 / 1000000.0;

#[derive(Parser)]
struct Config {
    /// Input Case
    #[arg(short, long, value_name = "FILE")]
    case: PathBuf,

    /// Input format
    #[arg(short, long, value_name = "FORMAT")]
    input_format: Option<String>,

    /// Output format
    #[arg(short, long, value_name = "FORMAT", default_value = "edit")]
    format: String,
}

/// Setup the interrupt inputs. Noop if interrupts are not fuzzed
fn setup_interrupt_inputs(mut input : MultipartInput<BytesInput>) -> MultipartInput<BytesInput> {
    for i in 0..MAX_NUM_INTERRUPT {
        let name = format!("isr_{}_times",i);
        if input.parts_by_name(&name).next().is_none() {
            input.add_part(name, BytesInput::new([0; MAX_NUM_INTERRUPT*4].to_vec()));
        }
    }
    input
}

fn unfold_input(input : &MultipartInput<BytesInput>) -> HashMap<String,Either<Vec<u8>,Vec<u32>>> {
    let mut res = HashMap::new();
    for (name, part) in input.iter() {
        if name == "bytes" {
            res.insert(name.to_string(),Left(part.bytes().to_vec()));
        } else {
            // let times = unsafe{std::mem::transmute::<&[u8], &[u32]>(&part.bytes()[0..4*(part.bytes().len()/4)])}.to_vec();
            eprintln!("name {} len {}", name, part.bytes().len());
            let mut times = part.bytes().chunks(4).filter(|x| x.len()==4).map(|x| u32::from_le_bytes(x.try_into().unwrap())).collect::<Vec<_>>();
            times.sort_unstable();
            res.insert(name.to_string(),Right(times));
        }
    }
    res
}

fn fold_input(input : HashMap<String,Either<Vec<u8>,Vec<u32>>>) -> MultipartInput<BytesInput> {
    let mut res = MultipartInput::new();
    for (name, data) in input {
        match data {
            Left(x) => res.add_part(name, BytesInput::new(x)),
            Right(x) => res.add_part(name, BytesInput::new(interrupt_times_to_input_bytes(&x))),
        }
    }
    res
}


fn main() {
    let conf = Config::parse();
    let show_input = match conf.input_format {
        Some(x) => {
            match x.as_str() {
                "case" => {
                    eprintln!("Interpreting input file as multipart input");
                    MultipartInput::from_file(conf.case.as_os_str()).unwrap()
                },
                "edit" => {
                    let bytes = fs::read(conf.case).expect("Can not read input file");
                    let input_str = String::from_utf8_lossy(&bytes);
                    eprintln!("Interpreting input file as custom edit input");
                    fold_input(ron::from_str::<HashMap<String,Either<Vec<u8>,Vec<u32>>>>(&input_str).expect("Failed to parse input"))
                },
                "ron" => {
                    let bytes = fs::read(conf.case).expect("Can not read input file");
                    let input_str = String::from_utf8_lossy(&bytes);
                    eprintln!("Interpreting input file as raw ron input");
                    ron::from_str::<MultipartInput<BytesInput>>(&input_str).expect("Failed to parse input")
                },
                "raw" => {
                    let bytes = fs::read(conf.case).expect("Can not read input file");
                    setup_interrupt_inputs(MultipartInput::from([("bytes",BytesInput::new(bytes))]))
                },
                x => panic!("Unknown input format: {}", x),
            }
        }
        Option::None => match MultipartInput::from_file(conf.case.as_os_str()) {
            Ok(x) => {
                eprintln!("Interpreting input file as multipart input");
                x
            },
            Err(_) => {
                let bytes = fs::read(conf.case).expect("Can not read input file");
                let input_str = String::from_utf8_lossy(&bytes);
                match ron::from_str::<HashMap<String,Either<Vec<u8>,Vec<u32>>>>(&input_str) {
                    Ok(x) => {
                        eprintln!("Interpreting input file as custom edit input");
                        fold_input(x)
                    },
                    Err(_) => {
                        match ron::from_str::<MultipartInput<BytesInput>>(&input_str) {
                            Ok(x) => {
                                eprintln!("Interpreting input file as raw ron input");
                                x
                            },
                            Err(_) => {
                                eprintln!("Interpreting input file as raw input");
                                setup_interrupt_inputs(MultipartInput::from([("bytes",BytesInput::new(bytes))]))
                            }
                        }
                    }
                }
            }
        }
    };
    // let uf = unfold_input(&show_input);
    // println!("{:?}", show_input);
    match conf.format.as_str() {
        "edit" => {
            let output = ron::to_string(&unfold_input(&show_input)).expect("Could not serialize input");
            println!("{}", output);
        },
        "ron" => {
            let output = ron::to_string(&show_input).expect("Could not serialize input");
            println!("{}", output);
        },
        "case" => {
            let output = postcard::to_allocvec(&show_input).expect("Could not serialize input");
            std::io::stdout().write_all(&output).expect("Could not write output");
        },
        _ => panic!("Unknown format")
    }
}
