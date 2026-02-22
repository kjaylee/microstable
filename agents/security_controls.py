from __future__ import annotations

import json
import os
from typing import Any, Dict, Tuple

ROOT_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
STATE_DIR = os.path.join(ROOT_DIR, ".state", "agents")
STATE_PATH = os.path.join(STATE_DIR, "security_state.json")


def _default_state() -> Dict[str, Any]:
    return {
        "keeper": {"last_rebalance_tick": None},
        "watchdog": {"cb_last_activation_tick": {}},
        "consensus": {
            "proposal_nonce": 0,
            "queued": {},
            "last_params": {"cr_target": 1.20, "mint_fee": 0.002},
        },
    }


def load_state() -> Dict[str, Any]:
    try:
        with open(STATE_PATH, "r", encoding="utf-8") as f:
            raw = json.load(f)
        if isinstance(raw, dict):
            return raw
    except Exception:
        pass
    return _default_state()


def save_state(state: Dict[str, Any]) -> None:
    os.makedirs(STATE_DIR, exist_ok=True)
    with open(STATE_PATH, "w", encoding="utf-8") as f:
        json.dump(state, f, indent=2, sort_keys=True)


def check_min_interval(last_tick: Any, tick: int, min_interval: int) -> Tuple[bool, int]:
    if last_tick is None:
        return True, 0
    elapsed = int(tick) - int(last_tick)
    if elapsed >= int(min_interval):
        return True, 0
    return False, int(min_interval) - elapsed
