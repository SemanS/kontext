#!/bin/sh
# Release check: what a release candidate changes for the people and applications that read it.
#
#   scripts/release-check.sh
#
# 1. Builds the candidate (`cargo build --release --locked`), or uses KONTEXT_CANDIDATE=<binary>.
# 2. With PEER_DIR=<Peer checkout> set, runs Peer's integration suites against the candidate
#    (KONTEXT_BIN=<candidate>): kontext.integration, knowledge/challenges.integration and
#    knowledge/keeping.integration. Skipped when PEER_DIR is unset. PEER_TEST_CMD overrides the runner
#    (default `vp test run`, run in $PEER_DIR/apps/server).
# 3. For each store in KONTEXT_STORES (repository paths separated by `:`), reads every entry through
#    `kontext api entries.list` (paged) and `entries.get` with the previous binary (KONTEXT_PREV,
#    default: `kontext` on PATH) and the candidate, and prints per entry what differs in status,
#    verification.kind, verification.verified, since.commits and the governing paths, with totals.
#
# Exit status: 0 when everything ran (differences are printed, not judged), 1 when a step failed.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
failed=0

say() { printf '\n== %s\n' "$*"; }

say "candidate"
if [ -n "${KONTEXT_CANDIDATE:-}" ]; then
  candidate="$KONTEXT_CANDIDATE"
else
  (cd "$root" && cargo build --release --locked)
  candidate="$root/target/release/kontext"
fi
[ -x "$candidate" ] || { echo "no candidate binary at $candidate" >&2; exit 1; }
echo "candidate: $("$candidate" --version) ($candidate)"

say "Peer integration suites"
if [ -z "${PEER_DIR:-}" ]; then
  echo "skipped: set PEER_DIR to a Peer checkout to run them"
else
  server="$PEER_DIR/apps/server"
  suites="src/peerHub/kontext.integration.test.ts src/peerHub/plugins/knowledge/challenges.integration.test.ts src/peerHub/plugins/knowledge/keeping.integration.test.ts"
  present=""
  for s in $suites; do
    if [ -f "$server/$s" ]; then present="$present $s"; else echo "missing in this Peer checkout: $s"; fi
  done
  if [ -z "$present" ]; then
    echo "none of the suites exist under $server" >&2
    failed=1
  else
    # shellcheck disable=SC2086 # the runner and the suite list are word lists
    if (cd "$server" && KONTEXT_BIN="$candidate" ${PEER_TEST_CMD:-vp test run} $present); then
      echo "Peer suites passed"
    else
      echo "Peer suites FAILED against $candidate" >&2
      failed=1
    fi
  fi
fi

say "stores"
if [ -z "${KONTEXT_STORES:-}" ]; then
  echo "skipped: set KONTEXT_STORES=/path/to/repo[:/other/repo] to compare entries"
else
  prev="${KONTEXT_PREV:-$(command -v kontext || true)}"
  [ -n "$prev" ] && [ -x "$prev" ] || { echo "no previous binary: set KONTEXT_PREV or put kontext on PATH" >&2; exit 1; }
  echo "previous: $("$prev" --version) ($prev)"
  python3 - "$prev" "$candidate" "$KONTEXT_STORES" <<'PY' || failed=1
import json, os, subprocess, sys

prev, cand, stores = sys.argv[1], sys.argv[2], [s for s in sys.argv[3].split(":") if s]
env = dict(os.environ, KONTEXT_SKIP="1")


def api(binary, store, op, payload):
    out = subprocess.run([binary, "api", op, json.dumps(payload)], cwd=store, env=env, capture_output=True, text=True)
    try:
        value = json.loads(out.stdout)
    except ValueError:
        raise SystemExit(f"{binary} api {op} in {store}: no JSON (exit {out.returncode}): {out.stderr.strip()[:300]}")
    if not value.get("ok"):
        return None, value.get("error", {})
    return value["result"], None


def listed(binary, store):
    entries, offset = {}, 0
    while True:
        result, err = api(binary, store, "entries.list", {"limit": 500, "offset": offset})
        if err:
            raise SystemExit(f"{binary} entries.list in {store}: {err}")
        for e in result.get("entries", []):
            entries[e["id"]] = e
        offset = result.get("next_offset")
        if offset is None:
            return entries


def facts(e):
    if e is None:
        return None
    v = e.get("verification") or {}
    s = e.get("since") or {}
    return {
        "status": e.get("status"),
        "verification.kind": v.get("kind"),
        "verification.verified": v.get("verified"),
        "since.commits": s.get("commits"),
        "paths": sorted(e.get("paths") or []),
    }


totals = {"entries": 0, "changed": 0, "added": 0, "removed": 0}
by_field = {}
for store in stores:
    print(f"\n-- {store}")
    old, new = listed(prev, store), listed(cand, store)
    for eid in sorted(set(old) | set(new)):
        totals["entries"] += 1
        if eid not in new:
            totals["removed"] += 1
            print(f"  {eid}: only in the previous binary's list")
            continue
        if eid not in old:
            totals["added"] += 1
            print(f"  {eid}: only in the candidate's list")
            continue
        diffs = []
        for source, a, b in (
            ("list", facts(old[eid]), facts(new[eid])),
            ("get", facts((api(prev, store, "entries.get", {"id": eid})[0] or {}).get("entry")),
             facts((api(cand, store, "entries.get", {"id": eid})[0] or {}).get("entry"))),
        ):
            if a is None or b is None:
                if a != b:
                    diffs.append(f"{source}: entries.get {'fails' if b is None else 'works'} in the candidate")
                continue
            for field in a:
                if a[field] != b[field]:
                    diffs.append(f"{source} {field}: {a[field]!r} -> {b[field]!r}")
                    by_field[field] = by_field.get(field, 0) + 1
        if diffs:
            totals["changed"] += 1
            print(f"  {eid}:")
            for d in diffs:
                print(f"    {d}")

print(
    f"\ntotal: {totals['entries']} entries, {totals['changed']} changed, "
    f"{totals['added']} only in the candidate, {totals['removed']} only in the previous binary"
)
for field, n in sorted(by_field.items()):
    print(f"  {field}: {n} difference(s)")
PY
fi

if [ "$failed" != 0 ]; then
  say "release check FAILED"
  exit 1
fi
say "release check done"
