#!/bin/sh
# Regenerate the oracle side of crates/gdlint/tests/fixtures/lint: a tree dump for each script the cases lint,
# and what gdtoolkit 4.5.0's gdlint prints for each case in cases.txt. Usage: tools/oracle/gdlint_fixtures.sh
set -eu
venv=${GG_CORPUS:-$HOME/.cache/gg-corpus}/oracle-venv
repo=$(cd "$(dirname "$0")/../.." && pwd)
cd "$repo/crates/gdlint/tests/fixtures/lint"
rm -rf trees && mkdir trees
for script in default/*.gd custom/tree/*.gd; do
    "$venv/bin/python" "$repo/tools/oracle/dump_tree.py" "$script" > "trees/$(basename "$script" .gd).jsonl"
done
# Each line of cases.txt: a case name, the directory to run in, then gdlint's arguments.
while read -r name dir args; do
    out=$(mktemp) err=$(mktemp)
    status=0
    # shellcheck disable=SC2086 # the arguments are space-separated on purpose
    (cd "$dir" && "$venv/bin/gdlint" $args >"$out" 2>"$err") || status=$?
    { echo "exit $status"; echo "--- stdout"; cat "$out"; echo "--- stderr"; cat "$err"; } > "$name.expected"
    rm -f "$out" "$err"
done < cases.txt
