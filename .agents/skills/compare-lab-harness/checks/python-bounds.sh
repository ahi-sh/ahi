#!/bin/sh
# Host-side boundary tests; no dependencies or project test files to overwrite.
exec python3 -I -B - <<'PY'
import runpy
within = runpy.run_path('bounds.py')['within_window']
assert within(4, 5) and not within(5, 5) and not within(6, 5)
print('three boundary cases passed')
PY
