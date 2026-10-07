"""Run all skill gates offline. Never submits prompts or verifies product runtime."""
from pathlib import Path
import subprocess
import sys

scripts = Path(__file__).resolve().parent
commands = [
    [sys.executable, str(scripts / "check_correct_path.py"), "self-test"],
    [sys.executable, str(scripts / "verify_runtime_evidence.py"), "self-test"],
    [sys.executable, str(scripts / "test_user_action_gate.py")],
    [sys.executable, str(scripts / "verify_independent_review.py"), "self-test"],
    [sys.executable, str(scripts / "test_effect_safety.py")],
]
for command in commands:
    result = subprocess.run(command)
    if result.returncode:
        raise SystemExit(result.returncode)
print("All Test Manager offline self-tests passed; product runtime remains unverified.")
