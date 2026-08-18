set pagination off
set print thread-events off
set debuginfod enabled off
set breakpoint pending on
set $trace_index = 0
break smolnesRuntimeWrappedCpuStepBegin
commands
  silent
  if $trace_index < 256
    set $trace_pc = PCH * 256 + PCL
    set $offset = $trace_pc - 0x8000
    set $op = rombuf[$offset + 16]
    printf "%03u pc=%04x op=%02x a=%02x x=%02x y=%02x p=%02x s=%02x scanline=%u dot=%u\n", $trace_index, $trace_pc, $op, (unsigned int)A, (unsigned int)X, (unsigned int)Y, (unsigned int)P, (unsigned int)S, (unsigned int)scany, (unsigned int)dot
    set $trace_index = $trace_index + 1
    continue
  end
  disable 1
  continue
end
run
