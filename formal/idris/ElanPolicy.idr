module ElanPolicy

%default total

public export
data FaultLayer = Healthy | Transport | Driver | Consumer | Inconclusive

public export
record Evidence where
  constructor MkEvidence
  expectedMotion : Bool
  cursorStalled : Bool
  irqDelta : Nat
  eventBytes : Nat
  irqKnown : Bool

public export
diagnose : Evidence -> FaultLayer
diagnose e =
  if not e.expectedMotion then Inconclusive
  else if e.eventBytes > 0 then
    if e.cursorStalled then Consumer else Healthy
  else if not e.irqKnown then Inconclusive
  else if e.irqDelta > 0 then Driver
  else Transport

public export
data DeviceState = Active | Faulted | Recovering

public export
data RecoveryAction = Observe | BeginRecovery | FinishRecovery | RecordFailure

public export
next : DeviceState -> RecoveryAction -> DeviceState
next Active Observe = Active
next Active BeginRecovery = Active
next Active FinishRecovery = Active
next Active RecordFailure = Faulted
next Faulted Observe = Faulted
next Faulted BeginRecovery = Recovering
next Faulted FinishRecovery = Faulted
next Faulted RecordFailure = Faulted
next Recovering Observe = Recovering
next Recovering BeginRecovery = Recovering
next Recovering FinishRecovery = Active
next Recovering RecordFailure = Faulted

public export
silentTransport : diagnose (MkEvidence True True 0 0 True) = Transport
silentTransport = Refl

public export
irqWithoutEvdev : diagnose (MkEvidence True True 1 0 True) = Driver
irqWithoutEvdev = Refl

public export
evdevWithoutCursor : diagnose (MkEvidence True True 1 24 True) = Consumer
evdevWithoutCursor = Refl

public export
successfulRecoveryIsActive : next Recovering FinishRecovery = Active
successfulRecoveryIsActive = Refl

public export
data WatchdogAction = Disarmed | ObserveHealth | RecoverInPlace

public export
record WatchdogEvidence where
  constructor MkWatchdogEvidence
  inputOpen : Bool
  transportProbeOk : Bool
  consecutiveReportErrors : Nat

public export
watchdog : WatchdogEvidence -> WatchdogAction
watchdog e =
  if not e.inputOpen then Disarmed
  else if not e.transportProbeOk then RecoverInPlace
  else if e.consecutiveReportErrors >= 3 then RecoverInPlace
  else ObserveHealth

public export
closedInputDisarms : watchdog (MkWatchdogEvidence False False 3) = Disarmed
closedInputDisarms = Refl

public export
transportProbeFailureRecovers :
  watchdog (MkWatchdogEvidence True False 0) = RecoverInPlace
transportProbeFailureRecovers = Refl

public export
reportFailureLimitRecovers :
  watchdog (MkWatchdogEvidence True True 3) = RecoverInPlace
reportFailureLimitRecovers = Refl

public export
healthyOpenInputIsObserved :
  watchdog (MkWatchdogEvidence True True 0) = ObserveHealth
healthyOpenInputIsObserved = Refl
