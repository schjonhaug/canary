#!/usr/bin/env python3
"""Install a single administrator TOTP factor without printing its secret."""
import argparse
import json
import os
from pathlib import Path
import re
import stat
import tempfile


def fail(message):
    raise SystemExit(message)


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--user-id", required=True)
parser.add_argument("--seed-file", required=True,
                    help="root-only file containing one base32 seed")
parser.add_argument("--output", default="/run/secrets/canary-admin-mfa.json")
args = parser.parse_args()

if os.geteuid() != 0:
    fail("Run as root; do not use a shell history value for the seed.")
if not re.fullmatch(r"[0-9a-fA-F-]{36}", args.user_id):
    fail("The user ID must be an immutable UUID.")
seed_path = Path(args.seed_file)
seed_info = seed_path.lstat()
if not stat.S_ISREG(seed_info.st_mode) or stat.S_ISLNK(seed_info.st_mode) or seed_info.st_mode & 0o077:
    fail("The seed file must be a private, non-symlink regular file.")
seed = seed_path.read_text(encoding="ascii").strip().upper()
if not re.fullmatch(r"[A-Z2-7]+=*", seed) or len(seed.rstrip("=")) < 32:
    fail("The seed must be a base32 value with at least 160 bits of entropy.")
output = Path(args.output)
if output.exists() and output.is_symlink():
    fail("The factors output must not be a symlink.")
entries = json.loads(output.read_text()) if output.exists() else {}
if not isinstance(entries, dict):
    fail("The factors output must contain a JSON object.")
entries[args.user_id] = seed
output.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
with tempfile.NamedTemporaryFile("w", dir=output.parent, prefix=".admin-mfa-", delete=False) as temporary:
    json.dump(entries, temporary, separators=(",", ":"))
    temporary.write("\n")
    temporary.flush()
    os.fchmod(temporary.fileno(), 0o600)
    os.fsync(temporary.fileno())
    name = temporary.name
os.replace(name, output)
os.chown(output, 0, 0)
os.chmod(output, 0o600)
print("Factor installed. Deliver the source seed only through the approved out-of-band channel.")
