//| The [`MutationalStage`] is the default stage used during fuzzing.
//! For the current input, it will perform a range of random mutations, and then run them in the executor.

use core::marker::PhantomData;
use std::cmp::{max, min};

use hashbrown::HashMap;
use libafl_bolts::{rands::{
    random_seed, Rand, StdRand
}, Named};
use libafl::{
    common::{HasMetadata, HasNamedMetadata}, corpus::{self, Corpus, HasCurrentCorpusId, Testcase}, events::{Event, EventFirer, EventProcessor, LogSeverity}, fuzzer::Evaluator, inputs::{HasMutatorBytes, HasTargetBytes, Input, MultipartInput}, mark_feature_time, prelude::{new_hash_feedback, AggregatorOps, CorpusId, MutationResult, Mutator, UserStats, UserStatsValue, UsesInput}, stages::Stage, start_timer, state::{HasCorpus, HasRand, MaybeHasClientPerfMonitor, UsesState}, Error
};
use libafl::prelude::State;
use petgraph::{graph::NodeIndex, graph::{self, DiGraph}};
use crate::{time::clock::{IcHist, QEMU_ISNS_PER_USEC}, fuzzer::{DO_NUM_INTERRUPT, FIRST_INT, MAX_NUM_INTERRUPT}, systemstate::{stg::{STGFeedbackState, STGNodeMetadata}, CaptureEvent, ExecInterval}};
use libafl::state::HasCurrentTestcase;
use std::borrow::Cow;

use simple_moving_average::SMA;

use super::{helpers::{input_bytes_to_interrupt_times, interrupt_times_to_input_bytes}, stg::{STGEdge, STGNode}, target_os::TargetSystem, RTOSJob};

// pub static mut MINIMUM_INTER_ARRIVAL_TIME : u32 = 1000 /*us*/ * QEMU_ISNS_PER_USEC; 
// one isn per 2**4 ns
// virtual insn/sec 62500000 = 1/16 GHz
// 1ms = 62500 insn
// 1us = 62.5 insn



//======================= Custom mutator

fn is_interrupt_handler<SYS>(graph: &DiGraph<STGNode<SYS>, STGEdge>, node: NodeIndex) -> bool 
where
    SYS: TargetSystem,
{
    graph.edges_directed(node as NodeIndex, petgraph::Direction::Incoming).any(|x| x.weight().event == CaptureEvent::ISRStart)
}

fn has_interrupt_handler_non_systick<SYS>(graph: &DiGraph<STGNode<SYS>, STGEdge>, node: NodeIndex) -> bool 
where
    SYS: TargetSystem,
{
    graph.edges_directed(node as NodeIndex, petgraph::Direction::Outgoing).any(|x| x.weight().event == CaptureEvent::ISRStart && x.weight().name!="xPortSysTickHandler")
}

fn is_candidate_for_new_branches<SYS>(graph: &DiGraph<STGNode<SYS>, STGEdge>, node: NodeIndex) -> bool 
where
    SYS: TargetSystem,
{
    !has_interrupt_handler_non_systick(graph, node) && !is_interrupt_handler(graph, node)
}

// TODO: this can be much more efficient, if the graph stored snapshots of the state and input progress was tracked
/// Determines if a given node in the state transition graph (STG) is a candidate for introducing new branches.
pub fn try_force_new_branches<SYS>(interrupt_ticks : &[u32], fbs: &STGFeedbackState<SYS>, meta: &STGNodeMetadata, config: (usize, u32)) -> Option<Vec<u32>> 
where
    SYS: TargetSystem,
{
    let mut new = false;
    let mut new_interrupt_times = Vec::new();
    for (num,&interrupt_time) in interrupt_ticks.iter().enumerate() {
        let lower_bound = if num==0 {FIRST_INT} else {interrupt_ticks[num-1].saturating_add((config.1 as f32 * QEMU_ISNS_PER_USEC) as u32)};
        let next = if interrupt_ticks.len()>num+1 {interrupt_ticks[num+1]} else {u32::MAX};
        for exec_interval in meta.intervals().iter().filter(|x| x.start_tick >= lower_bound as u64 && x.start_tick < next as u64) {
            if !(exec_interval.start_capture.0==CaptureEvent::ISRStart) {  // shortcut to skip interrupt handers without node lookup
                let node_index = fbs.state_abb_hash_index.get(&exec_interval.get_hash_index()).unwrap();
                if !has_interrupt_handler_non_systick(&fbs.graph, node_index.clone()) {
                    let new_time  = exec_interval.start_tick.saturating_add((exec_interval.end_tick+exec_interval.start_tick)/4);
                    new_interrupt_times.push(new_time.try_into().expect("ticks > u32"));
                    if (new_time + config.1 as u64) < next as u64 { // the new interrupt is not too close to the next one
                        new_interrupt_times.extend(interrupt_ticks.iter().skip(num).cloned());
                    } else {    // the new interrupt is too close to the next one, skip the next one
                        new_interrupt_times.extend(interrupt_ticks.iter().skip(num+1).cloned());
                    }
                    new=true;
                    break;
                }
            }
        }
        if new {break;}
        new_interrupt_times.push(interrupt_time);
    }
    if new {Some(new_interrupt_times)} else {None}
}

/// The default mutational stage
#[derive(Clone, Debug)]
pub struct InterruptShiftStage<E, EM, Z, SYS> {
    #[allow(clippy::type_complexity)]
    phantom: PhantomData<(E, EM, Z, SYS)>,
    interrup_config: Vec<(usize,u32)>,
    success: simple_moving_average::SingleSumSMA<f32, f32, 50>
}

impl<E, EM, Z, SYS> InterruptShiftStage<E, EM, Z, SYS>
where
    E: UsesState<State = Z::State>,
    EM: UsesState<State = Z::State>,
    Z: Evaluator<E, EM>,
    Z::State: MaybeHasClientPerfMonitor + HasCorpus + HasRand,
{
    pub fn new(config : &Vec<(usize,u32)>) -> Self {
        Self { phantom: PhantomData, interrup_config: config.clone(), success: simple_moving_average::SingleSumSMA::from_zero(1.0) }
    }
}

static mut num_stage_execs : u64 = 0;
static mut sum_reruns : u64 = 0;
static mut sum_interesting_reruns : u64 = 0;

impl<E, EM, Z, I, SYS> InterruptShiftStage<E, EM, Z, SYS>
where
    E: UsesState<State = Z::State>,
    EM: UsesState<State = Z::State>,
    EM: EventFirer,
    Z: Evaluator<E, EM>,
    Z::State: MaybeHasClientPerfMonitor + HasCorpus + HasRand + HasMetadata + HasNamedMetadata,
    <Z::State as UsesInput>::Input: Input,
    Z::State: UsesInput<Input = MultipartInput<I>>,
    I: HasMutatorBytes + Default,
    SYS: TargetSystem,
{
    fn report_stats(&self, state: &mut <InterruptShiftStage<E, EM, Z, SYS> as libafl::state::UsesState>::State, manager: &mut EM) {
        unsafe {
            let _ = manager.fire(
                state,
                Event::UpdateUserStats {
                    name: Cow::from("InterruptShiftStage"),
                    value: UserStats::new(
                        UserStatsValue::String(Cow::from(format!("{} -> {}/{} {:.1}% ", num_stage_execs, sum_interesting_reruns, sum_reruns, sum_interesting_reruns as f32 * 100.0 / sum_reruns as f32))),
                        AggregatorOps::None,
                    ),
                    phantom: PhantomData,
                },
            );
        }
    }
}

impl<S, E, EM, Z, I, SYS> Stage<E, EM, Z> for InterruptShiftStage<E, EM, Z, SYS>
where
    E: UsesState<State = S>,
    EM: UsesState<State = S>,
    Z: Evaluator<E, EM, State = S>,
    S: State<Input = MultipartInput<I>> + HasRand + HasCorpus + HasCurrentTestcase + HasMetadata + HasNamedMetadata,
    <<Self as UsesState>::State as HasCorpus>::Corpus: Corpus<Input = Self::Input>, //delete me
    EM: EventFirer,
    I: Default + Input + HasMutatorBytes,
    SYS: TargetSystem,
{
    fn perform(
        &mut self,
        fuzzer: &mut Z,
        executor: &mut E,
        state: &mut Self::State,
        manager: &mut EM
    ) -> Result<(), Error>
    where <Z as UsesState>::State: HasCorpus {
        if self.interrup_config.len() == 0 {return Ok(());} // configuration implies no interrupts
        let mut myrand = StdRand::new();
        myrand.set_seed(state.rand_mut().next());
        unsafe {num_stage_execs+=1;}


        let mut rerun_count = 0;    // count how many times we rerun the executor
        let mut interesting_rerun_count = 0;    // count how many reruns were interesting
        // Try many times to find a mutation that is not already in the corpus
        let loopbound = max(1, (self.success.get_average()*100.0) as usize);
        for _ in 0..loopbound {
            // Choose which isr to mutate
            let interrup_config = match myrand.choose(&self.interrup_config) {
                Some(s) => s,
                Option::None => {
                    self.report_stats(state, manager);
                    return Ok(())
                }
            };
            let name = format!("isr_{}_times", interrup_config.0);
            // manager.log(state, LogSeverity::Info, format!("Mutation {}/{}", loopbound, loopcount))?;

            let curr_case : std::cell::Ref<Testcase<MultipartInput<_>>> = state.current_testcase()?;
            let curr_input = curr_case.input().as_ref().unwrap();

            let mut new_input : MultipartInput<I> = curr_input.clone();
            let new_interrupt_part : &mut I = if new_input.parts_by_name(&name).next().is_some() {
                new_input.parts_by_name_mut(&name).next().unwrap()
            } else {
                new_input.add_part(String::from(&name), I::default()); new_input.parts_by_name_mut(&name).next().unwrap()
            }.1;
            let old_interrupt_times = input_bytes_to_interrupt_times(new_interrupt_part.bytes(), *interrup_config);
            let mut new_interrupt_times = Vec::with_capacity(MAX_NUM_INTERRUPT);
            let mut do_rerun = false;
            // if state.rand_mut().between(1, 100) <= 50 // only attempt the mutation half of the time
            {
                #[cfg(feature = "mutate_stg")]
                {
                    let metadata = state.metadata_map();
                    let maxtick = {metadata.get::<IcHist>().unwrap().1.0};
                    drop(new_interrupt_part.drain(..).collect::<Vec<u8>>());
                    {
                        let choice = myrand.between(1,100);
                        if choice <= 25 || *old_interrupt_times.get(0).unwrap_or(&u32::MAX) as u64 > maxtick {  // 0.5*0.25 = 12.5% of the time fully randomize all interrupts
                            do_rerun = true;
                            let hist = metadata.get::<IcHist>().unwrap();
                            let maxtick : u64 = hist.1.0;
                            // let maxtick : u64 = (_input.exec_time().expect("No duration found").as_nanos() >> 4).try_into().unwrap();
                            for _ in 0..myrand.between(0,min(MAX_NUM_INTERRUPT, (maxtick as usize * 3) / (interrup_config.1 as usize * QEMU_ISNS_PER_USEC as usize * 2))) {
                                new_interrupt_times.push(myrand.between(0, min(maxtick, u32::MAX as u64) as usize).try_into().expect("ticks > u32"));
                            }
                        }
                        else if choice <= 75 { // 0.5 * 0.25 = 12.5% of cases
                            let feedbackstate = match state
                                .metadata::<STGFeedbackState<SYS>>() {
                                    Ok(s) => s,
                                    Error => {
                                        panic!("STGfeedbackstate not visible")
                                    }
                                };
                            if let Some(meta) = curr_case.metadata_map().get::<STGNodeMetadata>() {
                                if let Some(t) = try_force_new_branches(&old_interrupt_times, feedbackstate, meta, *interrup_config) {
                                    do_rerun = true;
                                    new_interrupt_times=t;
                                }
                            }
                        //     let tmp = current_case.metadata_map().get::<STGNodeMetadata>();
                        //     if tmp.is_some() {
                        //         let trace = tmp.expect("STGNodeMetadata not found");
                        //         let mut node_indices = vec![];
                        //         for i in (0..trace.intervals.len()).into_iter() {
                        //             if let Some(abb) = &trace.intervals[i].abb {
                        //                 if let Some(idx) = feedbackstate.state_abb_hash_index.get(&(trace.intervals[i].start_state,abb.get_hash())) {
                        //                     node_indices.push(Some(idx));
                        //                     continue;
                        //                 }
                        //             }
                        //             node_indices.push(None);
                        //         }
                        //         // let mut marks : HashMap<u32, usize>= HashMap::new(); // interrupt -> block hit
                        //         // for i in 0..trace.intervals.len() {
                        //         //     let curr = &trace.intervals[i];
                        //         //     let m = interrupt_offsets[0..num_interrupts].iter().filter(|x| (curr.start_tick..curr.end_tick).contains(&((**x) as u64)));
                        //         //     for k in m {
                        //         //         marks.insert(*k,i);
                        //         //     }
                        //         // }
                        //         // walk backwards trough the trace and try moving the interrupt to a block that does not have an outgoing interrupt edge or ist already hit by a predecessor
                        //         for i in (0..num_interrupts).rev() {
                        //             let mut lb = FIRST_INT;
                        //             let mut ub : u32 = trace.intervals[trace.intervals.len()-1].end_tick.try_into().expect("ticks > u32");
                        //             if i > 0 {
                        //                 lb = u32::saturating_add(interrupt_offsets[i-1],unsafe{MINIMUM_INTER_ARRIVAL_TIME});
                        //             }
                        //             if i < num_interrupts-1 {
                        //                 ub = u32::saturating_sub(interrupt_offsets[i+1],unsafe{MINIMUM_INTER_ARRIVAL_TIME});
                        //             }
                        //             let alternatives : Vec<_> = (0..trace.intervals.len()).filter(|x|
                        //                 node_indices[*x].is_some() &&
                        //                 (trace.intervals[*x].start_tick < (lb as u64) && (lb as u64) < trace.intervals[*x].end_tick
                        //                 || trace.intervals[*x].start_tick > (lb as u64) && trace.intervals[*x].start_tick < (ub as u64))
                        //             ).collect();
                        //             let not_yet_hit : Vec<_> = alternatives.iter().filter(
                        //                 |x| feedbackstate.graph.edges_directed(*node_indices[**x].unwrap(), petgraph::Direction::Outgoing).any(|y| y.weight().event != CaptureEvent::ISRStart)).collect();
                        //             if not_yet_hit.len() > 0 {
                        //                 let replacement = &trace.intervals[*myrand.choose(not_yet_hit).unwrap()];
                        //                 interrupt_offsets[i] = (myrand.between(replacement.start_tick as usize,
                        //                     replacement.end_tick as usize)).try_into().expect("ticks > u32");
                        //                 // println!("chose new alternative, i: {} {} -> {}",i,tmp, interrupt_offsets[i]);
                        //                 do_rerun = true;
                        //                 break;
                        //             }
                        //         }
                        //     }
                        }
                        else {    // old version of the alternative search
                            new_interrupt_times = old_interrupt_times.clone();
                            let tmp = curr_case.metadata_map().get::<STGNodeMetadata>();
                            if tmp.is_some() {
                                let trace = tmp.expect("STGNodeMetadata not found");

                                // calculate hits and identify snippets
                                let mut last_m = false;
                                let mut marks : Vec<(&ExecInterval, usize, usize)>= vec![]; // 1: got interrupted, 2: interrupt handler
                                for i in 0..trace.intervals().len() {
                                    let curr = &trace.intervals()[i];
                                    let m = old_interrupt_times.iter().any(|x| (curr.start_tick..curr.end_tick).contains(&(*x as u64)));
                                    if m {
                                        marks.push((curr, i, 1));
                                        // println!("1: {}",curr.current_task.0.task_name);
                                    } else if last_m {
                                        marks.push((curr, i, 2));
                                        // println!("2: {}",curr.current_task.0.task_name);
                                    } else {
                                        marks.push((curr, i, 0));
                                    }
                                    last_m = m;
                                }
                                for i in 0..old_interrupt_times.len() {
                                    // bounds based on minimum inter-arrival time
                                    let mut lb = FIRST_INT;
                                    let mut ub : u32 = trace.intervals()[trace.intervals().len()-1].end_tick.try_into().expect("ticks > u32");
                                    if i > 0 {
                                        // use the new times, because changes to preceding timings are not accounted for yet
                                        lb = u32::saturating_add(new_interrupt_times[i-1], (interrup_config.1 as f32 * QEMU_ISNS_PER_USEC) as u32); 
                                    }
                                    if i < old_interrupt_times.len()-1 {
                                        ub = u32::saturating_sub(new_interrupt_times[i+1], (interrup_config.1 as f32 * QEMU_ISNS_PER_USEC) as u32);
                                    }
                                    // get old hit and handler
                                    let old_hit = marks.iter().filter(
                                        |x| x.0.start_tick < (old_interrupt_times[i] as u64) && (old_interrupt_times[i] as u64) < x.0.end_tick
                                    ).next();
                                    let old_handler = match old_hit {
                                        Some(s) => if s.1 < old_interrupt_times.len()-1 && s.1 < marks.len()-1 {
                                            Some(marks[s.1+1])
                                        } else {None},
                                        None => None
                                    };
                                    // find reachable alternatives
                                    let alternatives : Vec<_> = marks.iter().filter(|x|
                                        x.2 != 2 &&
                                        (
                                        x.0.start_tick < (lb as u64) && (lb as u64) < x.0.end_tick
                                        || x.0.start_tick > (lb as u64) && x.0.start_tick < (ub as u64))
                                    ).collect();
                                    // in cases there are no alternatives
                                    if alternatives.len() == 0 {
                                        if old_hit.is_none() {
                                            // choose something random
                                            let untouched : Vec<_> = marks.iter().filter(
                                                |x| x.2 == 0
                                            ).collect();
                                            if untouched.len() > 0 {
                                                let tmp = old_interrupt_times[i];
                                                let choice = myrand.choose(untouched).unwrap();
                                                new_interrupt_times[i] = myrand.between(choice.0.start_tick as usize, choice.0.end_tick as usize)
                                                    .try_into().expect("tick > u32");
                                                do_rerun = true;
                                            }
                                            // println!("no alternatives, choose random i: {} {} -> {}",i,tmp,interrupt_offsets[i]);
                                            continue;
                                        } else {
                                            // do nothing
                                            // println!("no alternatives, do nothing i: {} {}",i,interrupt_offsets[i]);
                                            continue;
                                        }
                                    }
                                    let replacement = myrand.choose(alternatives).unwrap();
                                    if (old_hit.map_or(false, |x| x == replacement)) {
                                        // use the old value
                                        // println!("chose old value, do nothing i: {} {}",i,interrupt_offsets[i]);
                                        continue;
                                    } else {
                                        let extra = if (old_hit.map_or(false, |x| x.1 < replacement.1)) {
                                            // move futher back, respect old_handler
                                            old_handler.map_or(0, |x| x.0.end_tick - x.0.start_tick)
                                        } else { 0 };
                                        // let tmp = new_interrupt_times[i];
                                        new_interrupt_times[i] = (myrand.between(replacement.0.start_tick as usize,
                                            replacement.0.end_tick as usize) + extra as usize).try_into().expect("ticks > u32");
                                        // println!("chose new alternative, i: {} {} -> {}",i,tmp, interrupt_offsets[i]);
                                        do_rerun = true;
                                    }
                                }
                                // println!("Mutator: {:?}", numbers);
                                // let mut start : u32 = 0;
                                // for i in 0..numbers.len() {
                                //     let tmp = numbers[i];
                                //     numbers[i] = numbers[i]-start;
                                //     start = tmp;
                                // }
                                new_interrupt_part.extend(&interrupt_times_to_input_bytes(&new_interrupt_times));
                            }
                        }
                    }
                }
                #[cfg(not(feature = "mutate_stg"))]
                {
                    if myrand.between(1,100) <= 25 {  // we have no hint if interrupt times will change anything
                        do_rerun = true;
                        let metadata = state.metadata_map();
                        let maxtick = {metadata.get::<IcHist>().unwrap().1.0};
                        new_interrupt_times = Vec::with_capacity(MAX_NUM_INTERRUPT);
                        for i in 0..myrand.between(0,min(MAX_NUM_INTERRUPT, (maxtick as usize * 3) / (interrup_config.1 as usize * QEMU_ISNS_PER_USEC as usize * 2))) {
                            new_interrupt_times.push(myrand.between(0, min(maxtick, u32::MAX as u64) as usize).try_into().expect("ticks > u32"));
                        }
                    }
                }
                new_interrupt_part.extend(&interrupt_times_to_input_bytes(&new_interrupt_times));
            }
            drop(curr_case);
            if do_rerun {
                rerun_count+=1;
                let (_, corpus_idx) = fuzzer.evaluate_input(state, executor, manager, new_input)?;
                if corpus_idx.is_some() { unsafe{interesting_rerun_count+=1;}} else
                if corpus_idx.is_none() && loopbound<=0 { break;}
            } else {if loopbound<=0 {break;}}
        }
        unsafe {
            sum_reruns+=rerun_count;
            sum_interesting_reruns+=interesting_rerun_count;
            if rerun_count>0 {self.success.add_sample(interesting_rerun_count as f32 / rerun_count as f32);}
        }
        self.report_stats(state, manager);
        Ok(())
    }
    
    fn should_restart(&mut self, state: &mut Self::State) -> Result<bool, Error> {
        Ok(true)
    }
    
    fn clear_progress(&mut self, state: &mut Self::State) -> Result<(), Error> {
        Ok(())
    }
}

impl<E, EM, Z, SYS> UsesState for InterruptShiftStage<E, EM, Z, SYS>
where
    E: UsesState<State = Z::State>,
    EM: UsesState<State = Z::State>,
    Z: Evaluator<E, EM>,
    Z::State: MaybeHasClientPerfMonitor + HasCorpus + HasRand,
{
    type State = Z::State;
}


pub fn try_worst_snippets<SYS>(bytes : &[u8], fbs: &STGFeedbackState<SYS>, meta: &STGNodeMetadata) -> Option<Vec<u8>> 
where
    SYS: TargetSystem,
{
    let mut new = false;
    let mut ret = Vec::new();
    for (num,interval) in meta.intervals().iter().enumerate() {
        todo!();
    }
    if new {Some(ret)} else {None}
}


static mut num_snippet_stage_execs : u64 = 0;
static mut num_snippet_rerun : u64 = 0;
static mut num_snippet_success : u64 = 0;

/// The default mutational stage
#[derive(Clone, Debug, Default)]
pub struct STGSnippetStage<E, EM, Z, SYS> {
    #[allow(clippy::type_complexity)]
    phantom: PhantomData<(E, EM, Z, SYS)>,
    input_addr: u32
}

impl<E, EM, Z, SYS> STGSnippetStage<E, EM, Z, SYS>
where
    E: UsesState<State = Z::State>,
    EM: UsesState<State = Z::State>,
    Z: Evaluator<E, EM>,
    Z::State: MaybeHasClientPerfMonitor + HasCorpus + HasRand,
    SYS: TargetSystem,
{
    pub fn new(input_addr: u32) -> Self {
        Self { phantom: PhantomData, input_addr }
    }
}

impl<E, EM, Z, I, SYS> STGSnippetStage<E, EM, Z, SYS>
where
    E: UsesState<State = Z::State>,
    EM: UsesState<State = Z::State>,
    EM: EventFirer,
    Z: Evaluator<E, EM>,
    Z::State: MaybeHasClientPerfMonitor + HasCorpus + HasRand + HasMetadata + HasNamedMetadata,
    <Z::State as UsesInput>::Input: Input,
    Z::State: UsesInput<Input = MultipartInput<I>>,
    I: HasMutatorBytes + Default,
    SYS: TargetSystem,
{
    fn report_stats(&self, state: &mut <STGSnippetStage<E, EM, Z, SYS> as UsesState>::State, manager: &mut EM) {
        unsafe {
            let _ = manager.fire(
                state,
                Event::UpdateUserStats {
                    name: Cow::from("STGSnippetStage"),
                    value: UserStats::new(
                        UserStatsValue::String(Cow::from(format!("{} -> {}/{} {:.1}% ", num_snippet_stage_execs, num_snippet_success, num_snippet_rerun, num_snippet_success as f32 * 100.0 / num_snippet_rerun as f32))),
                        AggregatorOps::None,
                    ),
                    phantom: PhantomData,
                },
            );
        }
    }
}

impl<E, EM, Z, I, SYS> Stage<E, EM, Z> for STGSnippetStage<E, EM, Z, SYS>
where
    E: UsesState<State = Z::State>,
    EM: UsesState<State = Z::State>,
    EM: EventFirer,
    Z: Evaluator<E, EM>,
    Z::State: MaybeHasClientPerfMonitor + HasCorpus + HasRand + HasMetadata + HasNamedMetadata,
    <Z::State as UsesInput>::Input: Input,
    Z::State: UsesInput<Input = MultipartInput<I>>,
    I: HasMutatorBytes + Default,
    Z::State: HasCurrentTestcase+HasCorpus+HasCurrentCorpusId,
    <Z::State as HasCorpus>::Corpus: Corpus<Input = MultipartInput<I>>,
    SYS: TargetSystem,
{
    fn perform(
        &mut self,
        fuzzer: &mut Z,
        executor: &mut E,
        state: &mut Self::State,
        manager: &mut EM
    ) -> Result<(), Error> {
        let mut myrand = StdRand::new();
        myrand.set_seed(state.rand_mut().next());

        let mut do_rerun = false;

        let current_case = state.current_testcase()?;
        let old_input = current_case.input().as_ref().unwrap();
        let mut new_input : MultipartInput<I> = old_input.clone();
        let new_bytes = new_input.parts_by_name_mut("bytes").next().expect("bytes not found in multipart input").1.bytes_mut();
        // dbg!(current_case.metadata_map());
        // eprintln!("Run mutator {}", current_case.metadata_map().get::<STGNodeMetadata>().is_some());
        if let Some(meta) = current_case.metadata_map().get::<STGNodeMetadata>() {
            let feedbackstate = match state
                .metadata::<STGFeedbackState<SYS>>() {
                    Ok(s) => s,
                    Error => {
                        panic!("STGfeedbackstate not visible")
                    }
                };
            // Maximize all snippets
            // dbg!(meta.jobs().len());
            for jobinst in meta.jobs().iter() {
                match feedbackstate.worst_task_jobs.get(&jobinst.get_hash_cached()) {
                    Some(worst) => {
                        let new = worst.map_bytes_onto(jobinst, Some(self.input_addr));
                        do_rerun |= new.len() > 0;
                        for (addr, byte) in new {
                            if (addr as usize) < new_bytes.len() {
                                new_bytes[addr as usize] = byte;
                            }
                        }
                    },
                    Option::None => {}
                }
            }
        }
        drop(current_case);
        unsafe {num_snippet_stage_execs+=1;}
        if do_rerun {
            unsafe {num_snippet_rerun+=1;}
            let (_, corpus_idx) = fuzzer.evaluate_input(state, executor, manager, new_input)?;
            if corpus_idx.is_some() { unsafe{num_snippet_success+=1};}
            
        }
        self.report_stats(state, manager);
        Ok(())
    }
    
    fn should_restart(&mut self, state: &mut Self::State) -> Result<bool, Error> {
        Ok(true)
    }
    
    fn clear_progress(&mut self, state: &mut Self::State) -> Result<(), Error> {
        Ok(())
    }
}

impl<E, EM, Z, SYS> UsesState for STGSnippetStage<E, EM, Z, SYS>
where
    E: UsesState<State = Z::State>,
    EM: UsesState<State = Z::State>,
    Z: Evaluator<E, EM>,
    Z::State: MaybeHasClientPerfMonitor + HasCorpus + HasRand,
    SYS: TargetSystem,
{
    type State = Z::State;
}