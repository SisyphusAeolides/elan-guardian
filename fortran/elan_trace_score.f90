module elan_trace_model
  use, intrinsic :: iso_fortran_env, only: int64
  implicit none
  private

  integer, parameter, public :: class_inconclusive = 0
  integer, parameter, public :: class_healthy = 1
  integer, parameter, public :: class_transport = 2
  integer, parameter, public :: class_driver = 3
  integer, parameter, public :: class_consumer = 4

  public :: classify_trace

contains

  pure integer function classify_trace(irq_delta, event_bytes, expect_motion, cursor_stalled) result(kind)
    integer(int64), intent(in) :: irq_delta
    integer(int64), intent(in) :: event_bytes
    logical, intent(in) :: expect_motion
    logical, intent(in) :: cursor_stalled

    if (.not. expect_motion) then
      kind = class_inconclusive
    else if (event_bytes > 0_int64) then
      if (cursor_stalled) then
        kind = class_consumer
      else
        kind = class_healthy
      end if
    else if (irq_delta > 0_int64) then
      kind = class_driver
    else
      kind = class_transport
    end if
  end function classify_trace
end module elan_trace_model

program elan_trace_score
  use, intrinsic :: iso_fortran_env, only: error_unit, int64
  use elan_trace_model
  implicit none

  character(len=4096) :: path
  integer :: unit, status, kind
  integer(int64) :: irq_delta, event_bytes
  integer :: expect_motion_value, cursor_stalled_value

  if (command_argument_count() /= 1) then
    write(error_unit, '(a)') 'usage: elan-trace-score FEATURES.dat'
    stop 2
  end if
  call get_command_argument(1, path)
  open(newunit=unit, file=trim(path), status='old', action='read', iostat=status)
  if (status /= 0) then
    write(error_unit, '(a)') 'cannot open feature file'
    stop 2
  end if
  read(unit, *, iostat=status) irq_delta, event_bytes, expect_motion_value, cursor_stalled_value
  close(unit)
  if (status /= 0) then
    write(error_unit, '(a)') 'invalid feature file'
    stop 2
  end if

  kind = classify_trace(irq_delta, event_bytes, expect_motion_value /= 0, cursor_stalled_value /= 0)
  select case (kind)
  case (class_healthy)
    print '(a)', 'healthy'
  case (class_transport)
    print '(a)', 'transport-stalled'
  case (class_driver)
    print '(a)', 'driver-stalled'
  case (class_consumer)
    print '(a)', 'consumer-stalled'
  case default
    print '(a)', 'inconclusive'
  end select
end program elan_trace_score
