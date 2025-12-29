use libafl::{
    common::HasMetadata,
    executors::ExitKind,
    feedbacks::Feedback,
    observers::ObserversTuple,
    prelude::{State, UsesInput},
    state::{HasCorpus, MaybeHasClientPerfMonitor},
    Error,
    corpus::Corpus,
    inputs::Input,
};
use libafl::events::EventFirer;
use libafl_bolts::Named;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::target_os::TargetSystem;
use std::borrow::Cow;
use std::marker::PhantomData;

use crate::systemstate::target_os::*;
use libafl::prelude::StateInitializer;

//=========================== Debugging Feedback
/// A [`Feedback`] meant to dump the system-traces for debugging. Depends on [`QemuSystemStateObserver`]
#[derive(Debug)]
pub struct DumpSystraceFeedback<SYS>
where
    SYS: TargetSystem,
{
    name: Cow<'static, str>,
    dumpfile: Option<PathBuf>,
    phantom: PhantomData<SYS>,
    init_time: Instant,
    last_dump: Option<Instant>,
}

impl<S, SYS> StateInitializer<S> for DumpSystraceFeedback<SYS> where SYS: TargetSystem {}

impl<EM, I, OT, S, SYS> Feedback<EM, I, OT, S> for DumpSystraceFeedback<SYS>
where
    S: State + UsesInput + MaybeHasClientPerfMonitor + HasMetadata + HasCorpus<Corpus: Corpus<Input=I>>,
    EM: EventFirer<State = S>,
    OT: ObserversTuple<I, S>,
    SYS: TargetSystem,
    I: Input,
{
    fn is_interesting(
        &mut self,
        state: &mut S,
        _manager: &mut EM,
        _input: &I,
        _observers: &OT,
        _exit_kind: &ExitKind,
    ) -> Result<bool, Error>
where {
        match &self.dumpfile {
            Some(s) => {
                let time_has_come = self.last_dump.map(|t| Instant::now()-t > Duration::from_secs(600)).unwrap_or(true);
                if time_has_come {
                    self.last_dump = Some(Instant::now());
                    // Try dumping the worst case
                    let casename = s.with_file_name(&(s.file_stem().unwrap().to_str().unwrap().to_owned()+&format!("_at_{}h", (Instant::now()-self.init_time).as_secs()/3600))).with_extension("case");
                    let corpus = state.corpus();
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
                        wi.to_file(casename).expect("Could not dump testcase");
                    }

                    // Try dumping the current case
                    let tracename = s.with_extension("trace.ron");
                    let trace = state
                        .metadata::<SYS::TraceData>()
                        .expect("TraceData not found");
                    std::fs::write(
                        tracename,
                        ron::to_string(trace)
                            .expect("Error serializing hashmap"),
                    )
                    .expect("Can not dump to file");
                }
            }
            Option::None => {
                ()
            }
        };
        Ok(false)
    }
}

impl<SYS> Named for DumpSystraceFeedback<SYS>
where
    SYS: TargetSystem,
{
    #[inline]
    fn name(&self) -> &Cow<'static, str> {
        &self.name
    }
}

impl<SYS> DumpSystraceFeedback<SYS>
where
    SYS: TargetSystem,
{
    /// Creates a new [`DumpSystraceFeedback`]
    #[allow(unused)]
    pub fn new() -> Self {
        Self {
            name: Cow::from("Dumpsystemstate".to_string()),
            dumpfile: None,
            phantom: PhantomData,
            init_time: std::time::Instant::now(),
            last_dump: None,
        }
    }
    #[allow(unused)]
    pub fn with_dump(dumpfile: Option<PathBuf>) -> Self {
        Self {
            name: Cow::from("Dumpsystemstate".to_string()),
            dumpfile: dumpfile,
            phantom: PhantomData,
            init_time: std::time::Instant::now(),
            last_dump: None,
        }
    }
}

#[derive(Debug, Default)]
pub struct SystraceErrorFeedback<SYS>
where
    SYS: TargetSystem,
{
    name: Cow<'static, str>,
    dump_case: bool,
    max_reports: Option<usize>,
    phantom: std::marker::PhantomData<SYS>,
}

impl<S, SYS> StateInitializer<S> for SystraceErrorFeedback<SYS> where SYS: TargetSystem {}

impl<EM, I, OT, S, SYS> Feedback<EM, I, OT, S> for SystraceErrorFeedback<SYS>
where
    S: State + UsesInput + MaybeHasClientPerfMonitor + HasMetadata,
    EM: EventFirer<State = S>,
    OT: ObserversTuple<I, S>,
    SYS: TargetSystem,
{
    fn is_interesting(
        &mut self,
        state: &mut S,
        _manager: &mut EM,
        _input: &I,
        _observers: &OT,
        _exit_kind: &ExitKind,
    ) -> Result<bool, Error>
where {
        #[cfg(feature = "trace_stg")]
        {
            if let Some(m) = self.max_reports {
                if m <= 0 {
                    return Ok(false);
                }
                let need_to_debug = state
                    .metadata::<SYS::TraceData>()
                    .expect("TraceData not found")
                    .need_to_debug();
                if need_to_debug {
                    self.max_reports = Some(m - 1);
                }
                return Ok(self.dump_case && need_to_debug);
            } else {
                return Ok(false);
            }
        }
        #[cfg(not(feature = "trace_stg"))]
        {
            return Ok(false);
        }
    }
}

impl<SYS> Named for SystraceErrorFeedback<SYS>
where
    SYS: TargetSystem,
{
    #[inline]
    fn name(&self) -> &Cow<'static, str> {
        &self.name
    }
}

impl<SYS> SystraceErrorFeedback<SYS>
where
    SYS: TargetSystem,
{
    #[must_use]
    pub fn new(dump_case: bool, max_reports: Option<usize>) -> Self {
        Self {
            name: Cow::from(String::from("SystraceErrorFeedback")),
            dump_case,
            max_reports,
            phantom: std::marker::PhantomData,
        }
    }
}
