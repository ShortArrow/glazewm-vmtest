#!/bin/sh
# Usage: run-live.sh <expectations file>
#
# Runs each live scenario on its own, one after another on the one guest, and checks its outcome against the
# expectations file: a scenario listed as `fails` has to fail, one listed as `varies` is not run, and every
# other scenario has to pass. Each line of the file is `<fails|varies> <scenario>`; `#` starts a comment.
set -u
expectations=$1

live() {
  cargo test --locked -p vmtest-adapters --test live -- --ignored "$@"
}

scenarios=$(live --list --format terse | sed -n 's/: test$//p')
listed=$(sed -e 's/#.*//' "$expectations" | awk 'NF { print $1, $2 }')
status=0

for name in $(printf '%s\n' "$listed" | awk 'NF { print $2 }'); do
  if ! printf '%s\n' "$scenarios" | grep -qx "$name"; then
    echo "UNKNOWN $name is listed in $expectations but is no scenario"
    status=1
  fi
done

for name in $scenarios; do
  expected=$(printf '%s\n' "$listed" | awk -v name="$name" '$2 == name { print $1 }')
  if [ "$expected" = varies ]; then
    echo "SKIPPED $name"
    continue
  fi
  if live --exact "$name"; then outcome=passes; else outcome=fails; fi
  if [ "$outcome" = "${expected:-passes}" ]; then
    echo "AS EXPECTED $name $outcome"
  else
    echo "UNEXPECTED $name $outcome, expected ${expected:-passes}"
    status=1
  fi
done

exit $status
