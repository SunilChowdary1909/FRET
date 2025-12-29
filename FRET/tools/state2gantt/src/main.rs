use hashbrown::HashMap;
use std::borrow::Cow;
use std::path::PathBuf;
use std::fs;
use fret::systemstate::{target_os::SystemTraceData, target_os::freertos::FreeRTOSTraceMetadata, target_os::SystemState, target_os::TaskControlBlock};
use std::io::Write;
use clap::Parser;
use itertools::Itertools;

#[derive(Parser)]
struct Config {
    /// Input Trace
    #[arg(short, long, value_name = "FILE")]
    input_trace: PathBuf,

    /// Output for activations
    #[arg(short, long, value_name = "FILE")]
    activation: Option<PathBuf>,

    /// Output for Release-Response intervals
    #[arg(short, long, value_name = "FILE")]
    response: Option<PathBuf>,

    /// Output abbs by task
    #[arg(short, long, value_name = "FILE")]
    per_task: Option<PathBuf>,

    /// Focussed Task
    #[arg(short, long, value_name = "TASK")]
    task: Option<String>,

    /// Translate times to microseconds
    #[arg(short, long)]
    micros: bool,
}

fn main() {
    // let args : Vec<String> = env::args().collect();
    let mut conf = Config::parse();

    let input_path = conf.input_trace;
    let raw_input = fs::read(input_path).expect("Can not read dumped traces");

    let activation_path = conf.activation;
    let instance_path = conf.response;
    let abb_path = conf.per_task;

    /* Write all execution intervals */
    let mut activation_file = activation_path.map(|x| std::fs::OpenOptions::new()
        .read(false)
        .write(true)
        .create(true)
        .append(false)
        .open(x).expect("Could not create file"));

    let mut level_per_task : HashMap<String, u32> = HashMap::new();


    // Store priority per task
    let trace : FreeRTOSTraceMetadata = ron::from_str(&String::from_utf8_lossy(&raw_input)).expect("Can not parse HashMap");
    // task_name -> (abb_addr -> (interval_count, exec_count, exec_time, woet))
    let mut abb_profile : HashMap<Cow<'static, str>, HashMap<u32, (usize, usize, u64, u64)>> = trace.select_abb_profile(conf.task.clone());
    for s in trace.intervals() {
        if s.level == 0 {
            let t = trace.states_map()[&s.start_state].current_task();
            level_per_task.insert(t.task_name().clone(),t.base_priority);
        }
    }

    // Range of longest selected job
    let limits = conf.task.as_ref().map(|task| trace.worst_jobs_per_task_by_response_time().get(task).map(|x| x.release..x.response)).flatten();
    if let Some(limits) = &limits {
        println!("Limits: {} - {}",limits.start,limits.end);
    }

    let mut intervals = trace.intervals().clone();
    activation_file.as_mut().map(|x| writeln!(x,"start,end,prio,name,state_id,state,abb").expect("Could not write to file"));
    for s in intervals.iter_mut() {
        if let Some(l) = &limits {
            if s.start_tick > l.end || s.end_tick < l.start {
                continue;
            }
            s.start_tick = s.start_tick.max(l.start);
            s.end_tick = s.end_tick.min(l.end);
        }
        let start_tick = if conf.micros {s.start_tick as f32 / fret::time::clock::QEMU_ISNS_PER_USEC} else {s.start_tick as f32};
        let end_tick = if conf.micros {s.end_tick as f32 / fret::time::clock::QEMU_ISNS_PER_USEC} else {s.end_tick as f32};
        let state = &trace.states_map()[&s.start_state];
        if s.level == 0 {
            activation_file.as_mut().map(|x| writeln!(x,"{},{},{},{},{:X},{},{}",start_tick,end_tick,trace.states_map()[&s.start_state].current_task().priority,trace.states_map()[&s.start_state].current_task().task_name, state.get_hash()>>48, state, s.abb.as_ref().map(|x| x.get_start()).unwrap_or(u32::MAX) ).expect("Could not write to file"));
        } else {
            activation_file.as_mut().map(|x| writeln!(x,"{},{},-{},{},{:X},{},{}",start_tick,end_tick,s.level,s.start_capture.1, state.get_hash()>>48, state, s.abb.as_ref().map(|x| x.get_start()).unwrap_or(u32::MAX)).expect("Could not write to file"));
        }
    }

    let mut jobs = trace.jobs().clone();
    /* Write all job instances from release to response */
    let instance_file = instance_path.map(|x| std::fs::OpenOptions::new()
        .read(false)
        .write(true)
        .create(true)
        .append(false)
        .open(x).expect("Could not create file"));

    if let Some(mut file) = instance_file {
        writeln!(file,"start,end,prio,name").expect("Could not write to file");
        for s in jobs.iter_mut() {
            if limits.as_ref().map(|x| !x.contains(&s.release) && !x.contains(&s.response) ).unwrap_or(false) {
                continue;
            }
            if let Some(l) = &limits {
                if s.release > l.end || s.response < l.start {
                    continue;
                }
                s.release = s.release.max(l.start);
                s.response = s.response.min(l.end);
            }
            writeln!(file,"{},{},{},{}",s.release,s.response,level_per_task[&s.name],s.name).expect("Could not write to file");
        }
    }

    /* Write all abbs per task */
    let abb_file = abb_path.map(|x| std::fs::OpenOptions::new()
        .read(false)
        .write(true)
        .create(true)
        .append(false)
        .open(x).expect("Could not create file"));

    if let Some(mut file) = abb_file {
        conf.micros = true;
        if abb_profile.is_empty() {
            return;
        }
        writeln!(file,"name,addr,active,finish,micros,woet").expect("Could not write to file");
        for (name, rest) in abb_profile.iter_mut().sorted_by_key(|x| x.0) {
            rest.iter().sorted_by_key(|x| x.0).for_each(|(addr, (active, finish, time, woet))| {
                writeln!(file,"{},{},{},{},{},{}",name,addr,active,finish,if conf.micros {*time as f64 / fret::time::clock::QEMU_ISNS_PER_USEC as f64} else {*time as f64}, if conf.micros {*woet as f64 / fret::time::clock::QEMU_ISNS_PER_USEC as f64} else {*woet as f64}).expect("Could not write to file");
            });
        }
    }
}
