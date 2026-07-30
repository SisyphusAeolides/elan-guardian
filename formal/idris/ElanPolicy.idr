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
