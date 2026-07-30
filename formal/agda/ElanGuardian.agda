{-# OPTIONS --safe #-}

module ElanGuardian where

open import Agda.Builtin.Nat using (Nat; zero; suc)
open import Agda.Builtin.Equality using (_≡_; refl)

data DriverState : Set where
  active : DriverState
  suspending : DriverState
  suspended : DriverState
  waking : DriverState
  faulted : DriverState
  recovering : DriverState

data Signal : Set where
  beginSuspend : Signal
  suspendComplete : Signal
  beginWake : Signal
  wakeComplete : Signal
  observeFault : Signal
  beginRecovery : Signal
  recoveryComplete : Signal
  recoveryFailed : Signal

step : DriverState → Signal → DriverState
step active beginSuspend = suspending
step suspending suspendComplete = suspended
step suspended beginWake = waking
step waking wakeComplete = active
step active observeFault = faulted
step faulted beginRecovery = recovering
step recovering recoveryComplete = active
step recovering recoveryFailed = faulted
step state signal = state

data IrqState : Set where
  enabled : IrqState
  disabled : IrqState

data InputUse : Set where
  closed : InputUse
  opened : InputUse

data ProbeState : Set where
  reachable : ProbeState
  unreachable : ProbeState

data ReportHealth : Set where
  reportsHealthy : ReportHealth
  reportFailureLimit : ReportHealth

data WatchdogAction : Set where
  disarm : WatchdogAction
  observe : WatchdogAction
  recoverInPlace : WatchdogAction

watchdog : InputUse → ProbeState → ReportHealth → WatchdogAction
watchdog closed probe reports = disarm
watchdog opened unreachable reports = recoverInPlace
watchdog opened reachable reportFailureLimit = recoverInPlace
watchdog opened reachable reportsHealthy = observe

record Machine : Set where
  constructor machine
  field
    state : DriverState
    irq : IrqState
    attempts : Nat

transition : Machine → Signal → Machine
transition (machine active enabled zero) observeFault = machine faulted enabled zero
transition (machine faulted enabled count) beginRecovery = machine recovering disabled (suc count)
transition (machine recovering disabled count) recoveryComplete = machine active enabled count
transition (machine recovering disabled count) recoveryFailed = machine faulted enabled count
transition (machine state irq count) signal = machine (step state signal) irq count

recovery-enables-irq : ∀ count →
  transition (machine recovering disabled count) recoveryComplete ≡
  machine active enabled count
recovery-enables-irq count = refl

failed-recovery-enables-irq : ∀ count →
  transition (machine recovering disabled count) recoveryFailed ≡
  machine faulted enabled count
failed-recovery-enables-irq count = refl

recovery-counts-attempt : ∀ count →
  transition (machine faulted enabled count) beginRecovery ≡
  machine recovering disabled (suc count)
recovery-counts-attempt count = refl

wake-returns-active : step waking wakeComplete ≡ active
wake-returns-active = refl

closed-input-disarms : ∀ probe reports → watchdog closed probe reports ≡ disarm
closed-input-disarms probe reports = refl

transport-failure-recovers : ∀ reports →
  watchdog opened unreachable reports ≡ recoverInPlace
transport-failure-recovers reports = refl

report-failure-limit-recovers :
  watchdog opened reachable reportFailureLimit ≡ recoverInPlace
report-failure-limit-recovers = refl

healthy-open-input-is-not-reset :
  watchdog opened reachable reportsHealthy ≡ observe
healthy-open-input-is-not-reset = refl
