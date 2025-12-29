//! A fuzzer using qemu in systemmode for binary-only coverage of kernels
//!
use core::time::Duration;
use std::{env, path::PathBuf, process::{self, abort}, io::{Read, Write}, fs::{self, OpenOptions}, cmp::{min, max}, mem::transmute_copy, ptr::addr_of_mut, ffi::OsStr};
use hashbrown::HashMap;
use libafl_bolts::{
core_affinity::Cores, ownedref::OwnedMutSlice, rands::StdRand, shmem::{ShMemProvider, StdShMemProvider}, tuples::tuple_list, AsSlice, SimpleStderrLogger
};
use libafl::{
common::{HasMetadata, HasNamedMetadata}, corpus::{Corpus, InMemoryCorpus, OnDiskCorpus}, events::{launcher::Launcher, EventConfig}, executors::ExitKind, feedback_or, feedback_or_fast, feedbacks::{CrashFeedback, MaxMapFeedback, TimeoutFeedback}, fuzzer::{Fuzzer, StdFuzzer}, inputs::{multi::MultipartInput, BytesInput, HasTargetBytes, Input}, monitors::MultiMonitor, observers::{CanTrack, VariableMapObserver}, prelude::{havoc_mutations, minimizer::TopRatedsMetadata, CorpusId, Generator, HitcountsMapObserver, RandBytesGenerator, SimpleEventManager, SimpleMonitor, SimplePrintingMonitor, SimpleRestartingEventManager, StdScheduledMutator}, schedulers::QueueScheduler, stages::StdMutationalStage, state::{HasCorpus, StdState}, Error, Evaluator
};
use libafl_qemu::{
elf::EasyElf, emu::Emulator, modules::{edges::{self}, EdgeCoverageModule, FilterList, StdAddressFilter, StdEdgeCoverageModule}, GuestAddr, GuestPhysAddr, QemuExecutor, QemuExitReason, QemuHooks, Regs
};
use libafl_targets::{edges_map_mut_ptr, EDGES_MAP_DEFAULT_SIZE, MAX_EDGES_FOUND};
use rand::{SeedableRng, StdRng, Rng};

#[cfg(feature = "freertos")]
use crate::systemstate::target_os::freertos::{config::get_range_groups, qemu_module::FreeRTOSSystemStateHelper, FreeRTOSSystem};
#[cfg(feature = "freertos")]
type TargetSystem = FreeRTOSSystem;
#[cfg(feature = "freertos")]
type SystemStateHelper = FreeRTOSSystemStateHelper;

#[cfg(feature = "osek")]
use crate::systemstate::target_os::osek::{config::get_range_groups, qemu_module::OSEKSystemStateHelper, OSEKSystem};
#[cfg(feature = "osek")]
type TargetSystem = OSEKSystem;
#[cfg(feature = "osek")]
type SystemStateHelper = OSEKSystemStateHelper;

use crate::{
    config::{get_target_ranges, get_target_symbols}, systemstate::{self, feedbacks::{DumpSystraceFeedback, SystraceErrorFeedback}, helpers::{get_function_range, input_bytes_to_interrupt_times, load_symbol, try_load_symbol}, mutational::{InterruptShiftStage, STGSnippetStage}, schedulers::{GenerationScheduler, LongestTraceScheduler}, stg::{stg_map_mut_slice, GraphMaximizerCorpusScheduler, STGEdge, STGNode, StgFeedback, MAX_STG_NUM}}, time::{
        clock::{ClockTimeFeedback, IcHist, QemuClockIncreaseFeedback, QemuClockObserver, FUZZ_START_TIMESTAMP, QEMU_ICOUNT_SHIFT, QEMU_ISNS_PER_MSEC, QEMU_ISNS_PER_USEC}, qemustate::QemuStateRestoreHelper, worst::{AlwaysTrueFeedback, ExecTimeIncFeedback, RateLimitedMonitor, TimeMaximizerCorpusScheduler, TimeProbMassScheduler, TimeStateMaximizerCorpusScheduler}
    }
};
use std::time::SystemTime;
use petgraph::dot::Dot;
use crate::systemstate::stg::STGFeedbackState;
use libafl::inputs::HasMutatorBytes;
use libafl_qemu::Qemu;
use crate::cli::Cli;
use crate::cli::Commands;
use crate::cli::set_env_from_config;
use clap::Parser;
use log;
use rand::RngCore;
use crate::templates;
use std::ops::Range;

// Constants ================================================================================

pub static mut RNG_SEED: u64 = 1;

pub const FIRST_INT : u32 = 200000;

pub const MAX_NUM_INTERRUPT: usize = 128;
pub const NUM_INTERRUPT_SOURCES: usize = 6; // Keep in sync with qemu-libafl-bridge/hw/timer/armv7m_systick.c:319 and  FreeRTOS/FreeRTOS/Demo/CORTEX_M3_MPS2_QEMU_GCC/init/startup.c:216
pub const DO_NUM_INTERRUPT: usize = 128;
pub static mut MAX_INPUT_SIZE: usize = 1024;

pub fn get_all_fn_symbol_ranges(elf: &EasyElf, range: std::ops::Range<GuestAddr>) -> HashMap<String,std::ops::Range<GuestAddr>> {
    let mut ret : HashMap<String,std::ops::Range<GuestAddr>> = HashMap::new();

    let gob = elf.goblin();

    let mut funcs : Vec<_> = gob.syms.iter().filter(|x| x.is_function() && range.contains(&x.st_value.try_into().unwrap())).collect();
    funcs.sort_unstable_by(|x,y| x.st_value.cmp(&y.st_value));

    for sym in &funcs {
        let sym_name = gob.strtab.get_at(sym.st_name);
        if let Some(sym_name) = sym_name {
            // if ISR_SYMBOLS.contains(&sym_name) {continue;}; // skip select symbols, which correspond to ISR-safe system calls
            if let Some(r) = get_function_range(elf, sym_name) {
                ret.insert(sym_name.to_string(), r);
            }
        }
    }

    return ret;
}

#[allow(unused)]
extern "C" {
static mut libafl_interrupt_offsets : [[u32; MAX_NUM_INTERRUPT]; NUM_INTERRUPT_SOURCES];
static mut libafl_num_interrupts : [u64; NUM_INTERRUPT_SOURCES];
}


/// Takes a state, cli and a suffix, writes out the current worst case
macro_rules! do_dump_case {
( $s:expr,$cli:expr, $c:expr) => {
    if ($cli.dump_cases) {
        let dump_path = $cli.dump_name.clone().unwrap().with_extension(if $c=="" {"case"} else {$c});
        println!("Dumping worst case to {:?}", &dump_path);
        let corpus = $s.corpus();
        let mut worst = Duration::new(0,0);
        let mut worst_input = None;
        for i in 0..corpus.count() {
            let tc = corpus.get(corpus.nth(i.into())).expect("Could not get element from corpus").borrow();
            if worst < tc.exec_time().expect("Testcase missing duration") {
                worst_input = Some(tc.input().as_ref().unwrap().clone());
                worst = tc.exec_time().expect("Testcase missing duration");
            }
        }
        if let Some(wi) = worst_input {
            wi.to_file(dump_path);
        }
    }
}
}

/// Takes a state, cli and a suffix, appends icount history
macro_rules! do_dump_times {
($state:expr, $cli:expr, $c:expr) => {
    if $cli.dump_times {
        let dump_path = $cli.dump_name.clone().unwrap().with_extension(if $c=="" {"time"} else {$c});
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .append(true)
            .open(dump_path).expect("Could not open timedump");
        if let Ok(ichist) = $state.metadata_mut::<IcHist>() {
            for i in ichist.0.drain(..) {
                writeln!(file, "{},{}", i.0, i.1).expect("Write to dump failed");
            }
        }
    }
};
}

/// Takes a state and a bool, writes out the current graph
macro_rules! do_dump_stg {
($state:expr, $cli:expr, $c:expr) => {
    #[cfg(feature = "trace_stg")]
    if $cli.dump_graph {
        let dump_path = $cli.dump_name.clone().unwrap().with_extension(if $c=="" {"dot"} else {$c});
        println!("Dumping graph to {:?}", &dump_path);
        if let Ok(md) = $state.metadata_mut::<STGFeedbackState<TargetSystem>>() {
            let out = md.graph.map(|_i,x| x.color_print(&md.systemstate_index), |_i,x| x.color_print());
            let outs = Dot::with_config(&out, &[]).to_string();
            let outs = outs.replace("\\\"","\"");
            let outs = outs.replace(';',"\\n");
            fs::write(dump_path,outs).expect("Failed to write graph");
        }
    }
};
}

/// Takes a state and a bool, writes out top rated inputs
macro_rules! do_dump_toprated {
($state:expr, $cli:expr, $c:expr) => {
    if $cli.dump_cases {
        {
            let dump_path = $cli.dump_name.clone().unwrap().with_extension(if $c=="" {"toprated"} else {$c});
            println!("Dumping toprated to {:?}", &dump_path);
            if let Some(md) = $state.metadata_map_mut().get_mut::<TopRatedsMetadata>() {
                let mut uniq: Vec<CorpusId> = md.map.values().map(|x| x.clone()).collect();
                uniq.sort();
                uniq.dedup();
                fs::write(dump_path,ron::to_string(&md.map).expect("Failed to serialize metadata")).expect("Failed to write graph");
            }
        }
    }
};
}


// Fuzzer setup ================================================================================

#[allow(unused)]
pub fn fuzz() {
log::set_max_level(log::LevelFilter::Info);
SimpleStderrLogger::set_logger().unwrap();
let cli = Cli::parse();
dbg!(&cli);
set_env_from_config(&cli.kernel, &cli.config);
let interrupt_config = crate::cli::get_interrupt_config(&cli.kernel, &cli.config);
unsafe {FUZZ_START_TIMESTAMP = SystemTime::now();}
if cli.dump_name.is_none() && (cli.dump_times || cli.dump_cases || cli.dump_traces || cli.dump_graph) {
    panic!("Dump name not give but dump is requested");
}
let mut starttime = std::time::Instant::now();
// Hardcoded parameters
let timeout = Duration::from_secs(10);
let broker_port = 1337;
let cores = Cores::from_cmdline("1").unwrap();
let corpus_dirs = [PathBuf::from("./corpus")];
let objective_dir = PathBuf::from(cli.dump_name.clone().map(|x| x.with_extension("crashes")).unwrap_or("./crashes".try_into().unwrap()));

let mut elf_buffer = Vec::new();
let elf = EasyElf::from_file(
    &cli.kernel,
    &mut elf_buffer,
)
.unwrap();

let TARGET_SYMBOLS: HashMap<&'static str, GuestAddr> = get_target_symbols(&elf);
let TARGET_RANGES: HashMap<&'static str, Range<GuestAddr>> = get_target_ranges(&elf, &TARGET_SYMBOLS);
let TARGET_GROUPS: HashMap<&'static str, HashMap<String, Range<GuestAddr>>> = get_range_groups(&elf, &TARGET_SYMBOLS, &TARGET_RANGES);

unsafe {
    libafl_num_interrupts = [0; NUM_INTERRUPT_SOURCES];
}

if let Ok(input_len) = env::var("FUZZ_INPUT_LEN") {
    unsafe {MAX_INPUT_SIZE = str::parse::<usize>(&input_len).expect("FUZZ_INPUT_LEN was not a number");}
}
unsafe {dbg!(MAX_INPUT_SIZE);}

if let Ok(seed) = env::var("SEED_RANDOM") {
    unsafe {RNG_SEED = str::parse::<u64>(&seed).expect("SEED_RANDOM must be an integer.");}
}


let denylist: Vec<_> = TARGET_GROUPS["ISR_FN"].values().map(|x| x.clone()).collect();
let denylist = StdAddressFilter::deny_list(denylist); // do not count isr jumps, which are useless

/// Setup the interrupt inputs. Noop if interrupts are not fuzzed
fn setup_interrupt_inputs(mut input : MultipartInput<BytesInput>, interrupt_config : &Vec<(usize,u32)>, mut random: Option<&mut StdRng>) -> MultipartInput<BytesInput> {
    #[cfg(feature = "fuzz_int")]
    for (i,_) in interrupt_config {
        let name = format!("isr_{}_times",i);
        if input.parts_by_name(&name).next().is_none() {
            if let Some(random) = random.as_mut() {
                input.add_part(name, BytesInput::new((0..MAX_NUM_INTERRUPT).map(|_| (random.next_u32()%(100*QEMU_ISNS_PER_MSEC)).to_le_bytes()).flatten().collect()));
            } else {
                input.add_part(name, BytesInput::new([0; MAX_NUM_INTERRUPT*4].to_vec()));
            }
        }
    }
    input
}

// Client setup ================================================================================

let run_client = |state: Option<_>, mut mgr, _core_id| {
    // Initialize QEMU
    let args: Vec<String> = vec![
        "target/debug/fret",
        "-icount",
        &format!("shift={},align=off,sleep=off", QEMU_ICOUNT_SHIFT),
        "-machine",
        "mps2-an385",
        "-cpu",
        "cortex-m3",
        "-monitor",
        "null",
        "-kernel",
        &cli.kernel.as_os_str().to_str().expect("kernel path is not a string"),
        "-serial",
        "null",
        "-nographic",
        "-S",
        // "-semihosting",
        // "--semihosting-config",
        // "enable=on,target=native",
        #[cfg(not(feature = "snapshot_fast"))]
        "-snapshot",
        #[cfg(not(feature = "snapshot_fast"))]
        "-drive",
        #[cfg(not(feature = "snapshot_fast"))]
        "if=none,format=qcow2,file=dummy.qcow2",
    ].into_iter().map(String::from).collect();
    let env: Vec<(String, String)> = env::vars().collect();
    let qemu = Qemu::init(&args).expect("Emulator creation failed");

    if let Some(&main_addr) = TARGET_SYMBOLS.get("FUZZ_MAIN") {
        qemu.set_breakpoint(main_addr);
        unsafe {
            match qemu.run() {
                Ok(QemuExitReason::Breakpoint(_)) => {}
                _ => panic!("Unexpected QEMU exit."),
            }
        }
        qemu.remove_breakpoint(main_addr);
    }

    qemu.set_breakpoint(TARGET_SYMBOLS["BREAKPOINT"]); // BREAKPOINT

    let devices = qemu.list_devices();
    println!("Devices = {devices:?}");

    #[cfg(feature = "snapshot_fast")]
    let initial_snap = Some(qemu.create_fast_snapshot(true));
    #[cfg(not(feature = "snapshot_fast"))]
    let initial_snap = None;

    let harness_input_addr = TARGET_SYMBOLS["FUZZ_INPUT"];
    let harness_input_length_ptr = TARGET_SYMBOLS.get("FUZZ_LENGTH").copied();
    let harness_breakpoint = TARGET_SYMBOLS["BREAKPOINT"];

    // The wrapped harness function, calling out to the LLVM-style harness
    let mut harness = |emulator: &mut Emulator<_, _, _, _, _>, state: &mut _, input: &MultipartInput<BytesInput>| {
        unsafe {
            #[cfg(feature = "fuzz_int")]
            {
                libafl_interrupt_offsets=[[0;MAX_NUM_INTERRUPT];NUM_INTERRUPT_SOURCES];
                for &c in &interrupt_config {
                    let (i,_) = c;
                    let name = format!("isr_{}_times",i);
                    let input_bytes = input.parts_by_name(&name).next().map(|x| x.1.bytes()).unwrap_or(&[]);
                    let t = input_bytes_to_interrupt_times(input_bytes, c);
                    for j in 0..t.len() {libafl_interrupt_offsets[i][j]=t[j];}
                    libafl_num_interrupts[i]=t.len() as u64;
                }

                // println!("Load: {:?}", libafl_interrupt_offsets[0..libafl_num_interrupts].to_vec());
            }

            let mut bytes = input.parts_by_name("bytes").next().unwrap().1.bytes();
            let mut len = bytes.len();
            if len > MAX_INPUT_SIZE {
                bytes = &bytes[0..MAX_INPUT_SIZE];
                len = MAX_INPUT_SIZE;
            }

            // Note: I could not find a difference between write_mem and write_phys_mem for my usecase
            qemu.write_mem(harness_input_addr, bytes);
            if let Some(s) = harness_input_length_ptr {
                qemu.write_mem(s, &(len as u32).to_le_bytes());
            }

            qemu.run();

            // If the execution stops at any point other then the designated breakpoint (e.g. a breakpoint on a panic method) we consider it a crash
            let mut pcs = (0..qemu.num_cpus())
                .map(|i| qemu.cpu_from_index(i))
                .map(|cpu| -> Result<u32, _> { cpu.read_reg(Regs::Pc) });
            match pcs
                .find(|pc| (harness_breakpoint..harness_breakpoint + 5).contains(pc.as_ref().unwrap_or(&0)))
            {
                Some(_) => ExitKind::Ok,
                Option::None => ExitKind::Crash,
            }
        }
    };

        // Create an observation channel to keep track of the execution time
        let clock_time_observer = QemuClockObserver::new("clocktime", &cli.select_task); // if cli.dump_times {cli.dump_name.clone().map(|x| x.with_extension("time"))} else {None}

        // Create an observation channel using the coverage map
        #[cfg(feature = "observe_edges")]
        let mut edges_observer = unsafe { VariableMapObserver::from_mut_slice(
            "edges",
            OwnedMutSlice::from_raw_parts_mut(edges_map_mut_ptr(), EDGES_MAP_DEFAULT_SIZE),
            addr_of_mut!(MAX_EDGES_FOUND),
        )};
        #[cfg(feature = "observe_hitcounts")]
        let mut edges_observer = HitcountsMapObserver::new(edges_observer);
        #[cfg(feature = "observe_edges")]
        let mut edges_observer = edges_observer.track_indices();

        #[cfg(feature = "observe_systemstate")]
        let stg_coverage_observer = unsafe { VariableMapObserver::from_mut_slice(
            "stg",
            stg_map_mut_slice(),
            addr_of_mut!(MAX_STG_NUM)
        )}.track_indices();

        // Feedback to rate the interestingness of an input
        // This one is composed by two Feedbacks in OR
        let mut feedback = feedback_or!(
            // Time feedback, this one does not need a feedback state
            ClockTimeFeedback::<TargetSystem>::new_with_observer(&clock_time_observer, &cli.select_task, if cli.dump_times {cli.dump_name.clone().map(|x| x.with_extension("time"))} else {None})
        );
        #[cfg(feature = "feed_genetic")]
        let mut feedback = feedback_or!(
            feedback,
            AlwaysTrueFeedback::new()
        );
        #[cfg(feature = "feed_afl")]
        let mut feedback = feedback_or!(
            feedback,
            // New maximization map feedback linked to the edges observer and the feedback state
            MaxMapFeedback::new(&edges_observer)
        );
        #[cfg(feature = "feed_longest")]
        let mut feedback = feedback_or!(
            // afl feedback needs to be activated first for MapIndexesMetadata
            feedback,
            // Feedback to reward any input which increses the execution time
            ExecTimeIncFeedback::<TargetSystem>::new()
        );
        #[cfg(all(feature = "observe_systemstate"))]
        let mut feedback = feedback_or!(
            feedback,
            DumpSystraceFeedback::<TargetSystem>::with_dump(if cli.dump_traces {cli.dump_name.clone()} else {None})
        );
        #[cfg(feature = "trace_stg")]
        let mut feedback = feedback_or!(
            feedback,
            StgFeedback::<TargetSystem>::new(cli.select_task.clone(), if cli.dump_graph {cli.dump_name.clone()} else {None})
        );
        #[cfg(feature = "feed_stg_edge")]
        let mut feedback = feedback_or!(
            feedback,
            MaxMapFeedback::new(&stg_coverage_observer)
        );

        // A feedback to choose if an input is producing an error
        let mut objective = feedback_or_fast!(CrashFeedback::new(), TimeoutFeedback::new(), SystraceErrorFeedback::<TargetSystem>::new(matches!(cli.command, Commands::Fuzz{..}), Some(10)));

        // If not restarting, create a State from scratch
        let mut state = state.unwrap_or_else(|| {
            StdState::new(
                // RNG
                unsafe {StdRand::with_seed(RNG_SEED) },
                // Corpus that will be evolved, we keep it in memory for performance
                InMemoryCorpus::new(),
                // Corpus in which we store solutions (crashes in this example),
                // on disk so the user can get them after stopping the fuzzer
                OnDiskCorpus::new(objective_dir.clone()).unwrap(),
                // States of the feedbacks.
                // The feedbacks can report the data that should persist in the State.
                &mut feedback,
                // Same for objective feedbacks
                &mut objective,
            )
            .unwrap()
        });

        // A minimization+queue policy to get testcasess from the corpus
        #[cfg(not(any(feature = "sched_afl", feature = "sched_stg", feature = "sched_genetic")))]
        let scheduler = QueueScheduler::new();  // fallback
        #[cfg(feature = "sched_afl",)]
        let scheduler = TimeMaximizerCorpusScheduler::new(&edges_observer,TimeProbMassScheduler::new());
        #[cfg(feature = "sched_stg")]
        let mut scheduler = GraphMaximizerCorpusScheduler::non_metadata_removing(&stg_coverage_observer,TimeProbMassScheduler::new());
        #[cfg(feature = "sched_stg")]
        {
            scheduler.skip_non_favored_prob = 0.8;
        }
        #[cfg(feature = "sched_genetic")]
        let scheduler = GenerationScheduler::new();

        // A fuzzer with feedbacks and a corpus scheduler
        let mut fuzzer = StdFuzzer::new(scheduler, feedback, objective);

        let qhelpers = tuple_list!();
        #[cfg(feature = "observe_systemstate")]
        let qhelpers = (SystemStateHelper::new(&TARGET_SYMBOLS,&TARGET_RANGES,&TARGET_GROUPS), qhelpers);
        #[cfg(feature = "observe_edges")]
        let qhelpers = (
            StdEdgeCoverageModule::builder()
            .map_observer(edges_observer.as_mut())
            .address_filter(denylist)
            .build()
            .unwrap(), qhelpers);//StdEdgeCoverageModule::new(denylist, FilterList::None), qhelpers);
        let qhelpers = (QemuStateRestoreHelper::with_fast(initial_snap), qhelpers);

        let emulator = Emulator::empty().qemu(qemu).modules(qhelpers).build().unwrap();

        let observer_list = tuple_list!();
        #[cfg(feature = "observe_systemstate")]
        let observer_list = (stg_coverage_observer, observer_list);  // must come after clock
        #[cfg(feature = "observe_edges")]
        let observer_list = (edges_observer, observer_list);
        let observer_list = (clock_time_observer, observer_list);

        // Create a QEMU in-process executor
        let mut executor = QemuExecutor::new(
            emulator,
            &mut harness,
            observer_list,
            &mut fuzzer,
            &mut state,
            &mut mgr,
            timeout
        )
        .expect("Failed to create QemuExecutor");

        executor.break_on_timeout();

        let mutations = havoc_mutations();
        // Setup an havoc mutator with a mutational stage
        let mutator = StdScheduledMutator::new(mutations);

        let stages = (systemstate::report::SchedulerStatsStage::default(),());
        let stages = (StdMutationalStage::new(mutator), stages);
        #[cfg(feature = "mutate_stg")]
        let mut stages = (STGSnippetStage::<_,_,_,TargetSystem>::new(TARGET_SYMBOLS["FUZZ_INPUT"]), stages);
        #[cfg(feature = "fuzz_int")]
        let mut stages = (InterruptShiftStage::<_,_,_,TargetSystem>::new(&interrupt_config), stages);

        if let Commands::Showmap { input } = cli.command.clone() {
            let s = input.as_os_str();
            // let show_input = BytesInput::new(if s=="-" {
            //         let mut buf = Vec::<u8>::new();
            //         std::io::stdin().read_to_end(&mut buf).expect("Could not read Stdin");
            //         buf
            //     } else if s=="$" {
            //         env::var("SHOWMAP_TEXTINPUT").expect("SHOWMAP_TEXTINPUT not set").as_bytes().to_owned()
            //     } else {
            //         // fs::read(s).expect("Input file for DO_SHOWMAP can not be read")
            //     });
            let show_input = match MultipartInput::from_file(input.as_os_str()) {
                Ok(x) => x,
                Err(_) => {
                    println!("Interpreting input file as raw input");
                    setup_interrupt_inputs(MultipartInput::from([("bytes",BytesInput::new(fs::read(input).expect("Can not read input file")))]), &interrupt_config, None)
                }
            };
            fuzzer.evaluate_input(&mut state, &mut executor, &mut mgr, show_input)
                .unwrap();
            do_dump_times!(state, &cli, "");
            do_dump_stg!(state, &cli, "");
        } else if let Commands::Fuzz { random, time, seed } = cli.command {
            if let Some(se) = seed {
                unsafe {
                    let mut rng = StdRng::seed_from_u64(se);
                    let bound = 10000;
                    #[cfg(feature = "shortcut")]
                    let bound = 100;
                    for _ in 0..bound {
                        let inp2 = BytesInput::new((0..MAX_INPUT_SIZE).map(|_| rng.gen::<u8>()).collect());
                        let inp = setup_interrupt_inputs(MultipartInput::from([("bytes",inp2)]), &interrupt_config, Some(&mut rng));
                        fuzzer.evaluate_input(&mut state, &mut executor, &mut mgr, inp).unwrap();
                    }
                }
            }
            else if let Ok(sf) = env::var("SEED_DIR") {
                state
                    .load_initial_inputs(&mut fuzzer, &mut executor, &mut mgr, &[PathBuf::from(&sf)])
                    .unwrap_or_else(|_| {
                        println!("Failed to load initial corpus at {:?}", &corpus_dirs);
                        process::exit(0);
                    });
                println!("We imported {} inputs from seedfile.", state.corpus().count());
            } else if state.corpus().count() < 1 {
                state
                    .load_initial_inputs(&mut fuzzer, &mut executor, &mut mgr, &corpus_dirs)
                    .unwrap_or_else(|_| {
                        println!("Failed to load initial corpus at {:?}", &corpus_dirs);
                        process::exit(0);
                    });
                println!("We imported {} inputs from disk.", state.corpus().count());
            }

            match time {
                Option::None => {
                    fuzzer
                        .fuzz_loop(&mut stages, &mut executor, &mut state, &mut mgr)
                        .unwrap();
                },
                Some(t) => {
                    println!("Iterations {}",t);
                    let num = t;
                    if random { unsafe {
                        println!("Random Fuzzing, ignore corpus");
                        // let mut generator = RandBytesGenerator::new(MAX_INPUT_SIZE);
                        let target_duration = Duration::from_secs(num);
                        let start_time = std::time::Instant::now();
                        let mut rng = StdRng::seed_from_u64(RNG_SEED);
                        while start_time.elapsed() < target_duration {
                            // let inp = generator.generate(&mut state).unwrap();
                            // libafl's generator is too slow
                            let inp2 = BytesInput::new((0..MAX_INPUT_SIZE).map(|_| rng.gen::<u8>()).collect());
                            let inp = setup_interrupt_inputs(MultipartInput::from([("bytes",inp2)]), &interrupt_config, Some(&mut rng));
                            fuzzer.evaluate_input(&mut state, &mut executor, &mut mgr, inp).unwrap();
                        }
                    }} else {
                        // fuzzer
                        //     .fuzz_loop_for_duration(&mut stages, &mut executor, &mut state, &mut mgr, Duration::from_secs(num))
                        //     .unwrap();
                        fuzzer
                            .fuzz_loop_until(&mut stages, &mut executor, &mut state, &mut mgr, starttime.checked_add(Duration::from_secs(num)).unwrap())
                            .unwrap();
                        #[cfg(feature = "run_until_saturation")]
                        {
                            let mut dumper = |marker : String| {
                                let d = format!("{}.case",marker);
                                do_dump_case!(state, &cli, &d);
                                let _d = format!("{}.dot",marker);
                                do_dump_stg!(state, &cli, &_d);
                                let d = format!("{}.toprated",marker);
                                do_dump_toprated!(state, &cli, &d);
                            };

                            dumper(format!(".iter_{}",t));
                            do_dump_times!(state, &cli, "");

                            println!("Start running until saturation");
                            let mut last = state.metadata_map().get::<IcHist>().unwrap().1;
                            while SystemTime::now().duration_since(unsafe {FUZZ_START_TIMESTAMP}).unwrap().as_millis() < last.1 + Duration::from_secs(10800).as_millis() {
                                starttime=starttime.checked_add(Duration::from_secs(30)).unwrap();
                                fuzzer
                                    .fuzz_loop_until(&mut stages, &mut executor, &mut state, &mut mgr, starttime)
                                    .unwrap();
                                let after = state.metadata_map().get::<IcHist>().unwrap().1;
                                if after.0 > last.0 {
                                    last=after;
                                }
                                do_dump_case!(state, &cli, "");
                                do_dump_stg!(state, &cli, "");
                                do_dump_toprated!(state, &cli, "");
                            }
                        }
                    }
                    do_dump_times!(state, &cli, "");
                    do_dump_case!(state, &cli, "");
                    do_dump_stg!(state, &cli, "");
                    do_dump_toprated!(state, &cli, "");
                },
            }
        }
        #[cfg(not(feature = "singlecore"))]
        return Ok(());
    };

    // Special case where no fuzzing happens, but standard input is dumped
    if let Ok(input_dump) = env::var("DUMP_SEED") {
        // Initialize QEMU
        let args: Vec<String> = env::args().collect();
        let env: Vec<(String, String)> = env::vars().collect();
        let emu = Qemu::init(&args).expect("Emu creation failed");

        if let Some(&main_addr) = TARGET_SYMBOLS.get("FUZZ_MAIN") {
            emu.set_breakpoint(main_addr); // BREAKPOINT 
        }
        unsafe {
            emu.run();

            let mut buf = [0u8].repeat(MAX_INPUT_SIZE);
            emu.read_mem(TARGET_SYMBOLS["FUZZ_INPUT"], buf.as_mut_slice());

            let dir = env::var("SEED_DIR").map_or("./corpus".to_string(), |x| x);
            let filename = if input_dump == "" {"input"} else {&input_dump};
            println!("Dumping input to: {}/{}",&dir,filename);
            fs::write(format!("{}/{}",&dir,filename), buf).expect("could not write input dump");
        }
        return
}

    #[cfg(feature = "singlecore")]
    {
        let monitor = RateLimitedMonitor::new();
        #[cfg(not(feature = "restarting"))]
        {
            let mgr = SimpleEventManager::new(monitor);
            run_client(None, mgr, 0);
        }

        #[cfg(feature = "restarting")]
        {
            let mut shmem_provider = StdShMemProvider::new().unwrap();
            let (state, mgr) = match SimpleRestartingEventManager::launch(monitor, &mut shmem_provider)
            {
                // The restarting state will spawn the same process again as child, then restarted it each time it crashes.
                Ok(res) => res,
                Err(err) => match err {
                    Error::ShuttingDown => {
                        return;
                    }
                    _ => {
                        panic!("Failed to setup the restarter: {}", err);
                    }
                },
            };
            run_client(state, mgr, 0);
        }
    }
    // else -> multicore
    #[cfg(not(feature = "singlecore"))]
    {
        // The shared memory allocator
        let shmem_provider = StdShMemProvider::new().expect("Failed to init shared memory");

        // The stats reporter for the broker
        let monitor = MultiMonitor::new(|s| println!("{}", s));

        // Build and run a Launcher
        match Launcher::builder()
            .shmem_provider(shmem_provider)
            .broker_port(broker_port)
            .configuration(EventConfig::from_build_id())
            .monitor(monitor)
            .run_client(&mut run_client)
            .cores(&cores)
            // .stdout_file(Some("/dev/null"))
            .build()
            .launch()
        {
            Ok(()) => (),
            Err(Error::ShuttingDown) => println!("Fuzzing stopped by user. Good bye."),
            Err(err) => panic!("Failed to run launcher: {:?}", err),
        }
    }
}
