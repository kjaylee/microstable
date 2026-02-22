# Crimson Team PoC Suite

Hybrid Red+Purple campaign for Microstable (Blue v3 baseline).

## Files
- `compound_exploits.py` — chained/kill-chain scenarios
- `semantic_attacks.py` — logic/state-machine/economic semantics
- `numeric_edge_cases.py` — IEEE754/NaN/Inf/precision/staleness edge-cases
- `results.json` — aggregated machine-readable outcomes (27 attempts)

## Run
From repo root:

```bash
PYTHONPATH=. python3 tests/crimson-team/compound_exploits.py
PYTHONPATH=. python3 tests/crimson-team/semantic_attacks.py
PYTHONPATH=. python3 tests/crimson-team/numeric_edge_cases.py
```

Regenerate aggregated `results.json`:

```bash
python3 - <<'PY'
import json, runpy
from pathlib import Path

base = Path('tests/crimson-team')
run_c = runpy.run_path(str(base/'compound_exploits.py'))['run_attempts']
run_s = runpy.run_path(str(base/'semantic_attacks.py'))['run_attempts']
run_n = runpy.run_path(str(base/'numeric_edge_cases.py'))['run_attempts']

attempts = run_c() + run_s() + run_n()
success = [a for a in attempts if a['success']]
out = {
  'campaign': 'microstable-crimson-team',
  'total_attempts': len(attempts),
  'successful_exploits': len(success),
  'blocked_or_failed': len(attempts) - len(success),
  'success_rate': round(len(success) / len(attempts), 4),
  'attempts': attempts,
}
Path('tests/crimson-team/results.json').write_text(json.dumps(out, indent=2), encoding='utf-8')
print('wrote tests/crimson-team/results.json')
PY
```

## Notes
- `success=true` means exploit condition reproduced.
- `success=false` means defense held (or hypothesis failed).
- Some attempts are static Solana checks where this suite verifies guard presence in code paths.
