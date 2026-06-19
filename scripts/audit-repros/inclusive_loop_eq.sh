#!/usr/bin/env bash
set -u

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
. "$SCRIPT_DIR/common.sh"

show_tooling
log "inclusive equal-bound loop"

new_project inclusive_loop_eq
write_main inclusive_loop_eq <<'LEO'
program inclusive_loop_eq.aleo {
    @noupgrade
    constructor() {}

    fn main() -> u32 {
        let acc: u32 = 0u32;

        for i: u32 in 5u32..=5u32 {
            acc += 1u32;
        }

        return acc;
    }
}
LEO

out="$ROOT/inclusive_loop_eq.out"
(cd "$ROOT/inclusive_loop_eq" && run_cmd "$out" "$LEO_BIN" run main)
status=$?
show_output "$out"

if [ "$status" -eq 0 ] && grep -q '0u32' "$out"; then
  result inclusive_loop_eq confirmed "inclusive 5u32..=5u32 ran as zero iterations"
elif [ "$status" -eq 0 ] && grep -q '1u32' "$out"; then
  result inclusive_loop_eq fixed "inclusive equal-bound loop returned 1u32"
else
  result inclusive_loop_eq inconclusive "unexpected status=$status"
fi
