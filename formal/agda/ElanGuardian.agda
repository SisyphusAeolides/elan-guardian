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
