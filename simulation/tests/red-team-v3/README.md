# Red Team v3 PoC Suite

Run the full exploit campaign:

```bash
PYTHONPATH=. python3 tests/red-team-v3/exploit_campaign.py
```

Artifacts generated:
- `tests/red-team-v3/results.json`
- `tests/red-team-v3/results.md`

The harness executes 36 attempts (PT-001 ~ PT-027 bypass attempts + Solana integration vectors) and marks each as:
- `SUCCESS` (exploit reproduced)
- `BLOCKED` (defense held)
