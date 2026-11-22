#!/bin/sh
# Host compiler only: no linking, device access, flashing or cross toolchain.
exec cc -std=c11 -x c -fsyntax-only -I . - <<'C'
#include "sensor.h"
_Static_assert(SENSOR_READY_MASK == (1u << 4), "wrong ready bit");
_Static_assert(SENSOR_IDLE_MASK == (1u << 1), "idle mask changed");
C
