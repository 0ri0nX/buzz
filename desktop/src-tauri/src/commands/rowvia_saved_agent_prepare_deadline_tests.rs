use super::*;

struct TimedContext {
    inner: FakeContext,
    qualification_delay: Duration,
    revalidation_delay: Duration,
    refuse_save: bool,
    startups: usize,
    revalidation_calls: usize,
}

impl TimedContext {
    fn new() -> Self {
        Self {
            inner: FakeContext::new("success"),
            qualification_delay: Duration::ZERO,
            revalidation_delay: Duration::ZERO,
            refuse_save: true,
            startups: 0,
            revalidation_calls: 0,
        }
    }
}

impl WholeContext for TimedContext {
    type Boundary = FakeC2;

    fn preflight(&mut self, request: &OwnerSaveRequest) -> Result<ValidatedInvocation, ()> {
        std::thread::sleep(self.qualification_delay);
        self.inner.preflight(request)
    }

    fn save(
        &mut self,
        original: RowviaSavedAgentPreparation,
    ) -> Result<PreparedRowviaSavedAgent, ()> {
        let prepared = self.inner.save(original)?;
        if self.refuse_save {
            Err(())
        } else {
            Ok(prepared)
        }
    }

    fn startup(
        &mut self,
        original: &RowviaSavedAgentPreparation,
        prepared: &PreparedRowviaSavedAgent,
    ) -> Result<Vec<u8>, ()> {
        self.startups += 1;
        self.inner.startup(original, prepared)
    }

    fn boundary(&mut self) -> &mut FakeC2 {
        self.inner.boundary()
    }

    fn owned_descendants_settled(&mut self, leader: u32) -> bool {
        self.inner.owned_descendants_settled(leader)
    }

    fn revalidate_installation(&mut self, installation: &FixedInstallation) -> bool {
        self.revalidation_calls += 1;
        std::thread::sleep(self.revalidation_delay);
        self.inner.revalidate_installation(installation)
    }
}

#[test]
fn native_prepare_qualification_does_not_spend_post_save_budget() {
    let mut context = TimedContext::new();
    context.qualification_delay = Duration::from_millis(50);
    // Stop at the first effect; qualification takes much longer than the
    // operation budget but must not prevent this one Save from being attempted.
    assert_eq!(
        run_with_limits(
            &mut context,
            &request(),
            Duration::from_secs(5),
            Duration::from_millis(1),
        ),
        Err(PrepareFailure::Unknown)
    );
    assert_eq!(context.inner.saves, 1);
    assert_eq!(context.startups, 0);
    assert!(context.inner.boundary.calls.is_empty());
}

#[test]
fn native_prepare_expired_qualification_never_saves_or_dispatches() {
    let mut context = TimedContext::new();
    assert_eq!(
        run_with_limits(
            &mut context,
            &request(),
            Duration::ZERO,
            Duration::from_secs(5),
        ),
        Err(PrepareFailure::Unavailable)
    );
    assert_eq!(context.inner.saves, 0);
    assert_eq!(context.startups, 0);
    assert!(context.inner.boundary.calls.is_empty());
}

#[test]
fn native_prepare_expired_post_save_budget_is_unknown_without_replay() {
    let mut context = TimedContext::new();
    context.refuse_save = false;
    assert_eq!(
        run_with_limits(
            &mut context,
            &request(),
            Duration::from_secs(5),
            Duration::ZERO,
        ),
        Err(PrepareFailure::Unknown)
    );
    assert_eq!(context.inner.saves, 1);
    assert_eq!(context.startups, 0);
    assert!(context.inner.boundary.calls.is_empty());
}

#[test]
fn native_prepare_expired_final_installation_check_withholds_final_c2() {
    let mut context = TimedContext::new();
    context.refuse_save = false;
    context.revalidation_delay = Duration::from_millis(5100);
    assert_eq!(
        run_with_limits(
            &mut context,
            &request(),
            Duration::from_secs(5),
            Duration::from_secs(5),
        ),
        Err(PrepareFailure::Unknown)
    );
    assert_eq!(context.inner.saves, 1);
    assert_eq!(context.startups, 1);
    assert_eq!(context.revalidation_calls, 1);
    assert_eq!(
        context
            .inner
            .boundary
            .calls
            .iter()
            .map(|call| call.0)
            .collect::<Vec<_>>(),
        ["capture", "retain", "revalidate"]
    );
}
