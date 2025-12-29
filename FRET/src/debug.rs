use libafl::*;
use libafl_bolts::*;
use std::borrow::Cow;
use serde::*;
use serde::ser::Serialize;
use libafl::prelude::Feedback;
use libafl::prelude::Testcase;
use libafl::prelude::*;
use std::marker::PhantomData;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct DebugMetadata {
    pub val: bool
}
libafl_bolts::impl_serdeany!(DebugMetadata);

//==================================================================================================

/// The [`DebugFeedback`] reports the same value, always.
/// It can be used to enable or disable feedback results through composition.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum DebugFeedback {
    /// Always returns `true`
    True,
    /// Alsways returns `false`
    False,
}

static mut counter : usize = 10;

impl<EM, I, OT, S> Feedback<EM, I, OT, S> for DebugFeedback
where
    S: State,
{
    #[inline]
    #[allow(clippy::wrong_self_convention)]
    fn is_interesting(
        &mut self,
        _state: &mut S,
        _manager: &mut EM,
        _input: &I,
        _observers: &OT,
        _exit_kind: &ExitKind,
    ) -> Result<bool, Error>
    where
        EM: EventFirer<State = S>,
        OT: ObserversTuple<I, S>,
    {
        if unsafe { counter } > 0 {
            unsafe { counter -= 1; }
            return Ok(true);
        } else {
            return Ok(false);
        }
        Ok((*self).into())
    }

    #[cfg(feature = "track_hit_feedbacks")]
    fn last_result(&self) -> Result<bool, Error> {
        Ok((*self).into())
    }

    fn append_metadata(
        &mut self,
        state: &mut S,
        _manager: &mut EM,
        _observers: &OT,
        testcase: &mut Testcase<<S>::Input>,
    ) -> Result<(), Error>
    where
        OT: ObserversTuple<I, S>,
        EM: EventFirer<State = S>,
    {
        testcase.metadata_map_mut().insert(DebugMetadata { val: true });
        eprintln!("Attach: {:?}",testcase.metadata::<DebugMetadata>());
        Ok(())
    }
}

impl Named for DebugFeedback {
    #[inline]
    fn name(&self) -> &Cow<'static, str> {
        static NAME: Cow<'static, str> = Cow::Borrowed("DebugFeedback");
        &NAME
    }
}

impl DebugFeedback {
    /// Creates a new [`DebugFeedback`] from the given boolean
    #[must_use]
    pub fn new(val: bool) -> Self {
        Self::from(val)
    }
}

impl From<bool> for DebugFeedback {
    fn from(val: bool) -> Self {
        if val {
            Self::True
        } else {
            Self::False
        }
    }
}

impl From<DebugFeedback> for bool {
    fn from(value: DebugFeedback) -> Self {
        match value {
            DebugFeedback::True => true,
            DebugFeedback::False => false,
        }
    }
}

//==================================================================================================

/// The default mutational stage
#[derive(Clone, Debug, Default)]
pub struct DebugStage<E, OT> {
    #[allow(clippy::type_complexity)]
    phantom: PhantomData<(E, OT)>,
}

impl<E, OT> UsesState for DebugStage<E, OT>
where
    E: UsesState,
{
    type State = E::State;
}

impl<E, OT> DebugStage<E, OT>
{
    pub fn new() -> Self {
        Self { phantom: PhantomData}
    }
}

impl<E, EM, OT, Z> Stage<E, EM, Z> for DebugStage<E, OT>
where
    E: Executor<EM, Z> + HasObservers<Observers = OT>,
    EM: EventFirer<State = Self::State>,
    OT: ObserversTuple<Self::State>,
    Self::State: HasCorpus + HasMetadata + HasNamedMetadata + HasExecutions,
    Z: Evaluator<E, EM, State = Self::State>,
{
    fn perform(
        &mut self,
        fuzzer: &mut Z,
        executor: &mut E,
        state: &mut Self::State,
        manager: &mut EM
    ) -> Result<(), Error> {
        // eprintln!("DebugStage {:?}", state.current_testcase());
        let testcase = state.current_testcase()?;
        eprintln!("Stage: {:?}",testcase.metadata::<DebugMetadata>());

        Ok(())
    }

    fn restart_progress_should_run(&mut self, state: &mut Self::State) -> Result<bool, Error> {
        Ok(true)
    }

    fn clear_restart_progress(&mut self, state: &mut Self::State) -> Result<(), Error> {
        Ok(())
    }
}