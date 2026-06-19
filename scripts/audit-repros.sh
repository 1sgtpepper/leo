#!/usr/bin/env bash
set -u

LEO_BIN="${LEO_BIN:-$(pwd)/target/debug/leo}"
ROOT="$(mktemp -d)"
trap 'rm -rf "$ROOT"' EXIT

log() {
  printf '\n## %s\n' "$1"
}

result() {
  printf 'RESULT %-28s %-14s %s\n' "$1" "$2" "$3"
}

run_cmd() {
  local outfile="$1"
  shift
  "$@" >"$outfile" 2>&1
  return $?
}

new_project() {
  local name="$1"
  (cd "$ROOT" && "$LEO_BIN" new "$name" >/dev/null)
}

write_main() {
  local name="$1"
  shift
  cat > "$ROOT/$name/src/main.leo"
}

show_output() {
  local file="$1"
  sed -n '1,220p' "$file"
}

log "Tooling"
printf 'LEO_BIN=%s\n' "$LEO_BIN"
"$LEO_BIN" --version || true

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

log "constant ternary fallible arm"
new_project const_ternary_bug
write_main const_ternary_bug <<'LEO'
program const_ternary_bug.aleo {
    @noupgrade
    constructor() {}

    fn main(x: u8) -> u8 {
        let y: u8 = true ? x : x.div_wrapped(0u8);
        return y;
    }
}
LEO
out="$ROOT/const_ternary_bug.out"
(cd "$ROOT/const_ternary_bug" && run_cmd "$out" "$LEO_BIN" run main 7u8)
const_status=$?
show_output "$out"

new_project dynamic_ternary_probe
write_main dynamic_ternary_probe <<'LEO'
program dynamic_ternary_probe.aleo {
    @noupgrade
    constructor() {}

    fn main(cond: bool, x: u8) -> u8 {
        let y: u8 = cond ? x : x.div_wrapped(0u8);
        return y;
    }
}
LEO
dyn_out="$ROOT/dynamic_ternary_probe.out"
(cd "$ROOT/dynamic_ternary_probe" && run_cmd "$dyn_out" "$LEO_BIN" run main true 7u8)
dyn_status=$?
show_output "$dyn_out"
if [ "$const_status" -eq 0 ] && grep -q '7u8' "$out" && [ "$dyn_status" -ne 0 ]; then
  result const_ternary_fallible confirmed "constant fold returns while equivalent dynamic ternary halts"
elif [ "$const_status" -eq 0 ] && grep -q '7u8' "$out"; then
  result const_ternary_fallible partial "constant fold returns 7u8; dynamic control did not fail"
else
  result const_ternary_fallible inconclusive "constant case status=$const_status dynamic status=$dyn_status"
fi

log "dynamic array index liveness"
new_project dce_arr_dead
write_main dce_arr_dead <<'LEO'
program dce_arr_dead.aleo {
    @noupgrade
    constructor() {}

    fn main(i: u32) -> u8 {
        let arr: [u8; 2] = [1u8, 2u8];
        let dead: u8 = arr[i];
        return 9u8;
    }
}
LEO
dead_out="$ROOT/dce_arr_dead.out"
(cd "$ROOT/dce_arr_dead" && run_cmd "$dead_out" "$LEO_BIN" build)
dead_status=$?
show_output "$dead_out"

new_project dce_arr_live
write_main dce_arr_live <<'LEO'
program dce_arr_live.aleo {
    @noupgrade
    constructor() {}

    fn main(i: u32) -> u8 {
        let arr: [u8; 2] = [1u8, 2u8];
        let dead: u8 = arr[i];
        return dead;
    }
}
LEO
live_out="$ROOT/dce_arr_live.out"
(cd "$ROOT/dce_arr_live" && run_cmd "$live_out" "$LEO_BIN" build)
live_status=$?
show_output "$live_out"
if [ "$dead_status" -eq 0 ] && [ "$live_status" -ne 0 ]; then
  result dce_dynamic_array_index confirmed "dead dynamic index builds; live dynamic index fails"
elif [ "$dead_status" -ne 0 ] && [ "$live_status" -ne 0 ]; then
  result dce_dynamic_array_index fixed "both variants reject/fail before artifact success"
else
  result dce_dynamic_array_index inconclusive "dead status=$dead_status live status=$live_status"
fi

log "option lowering else-if unwrap placement"
new_project opt_else_if_hoist
write_main opt_else_if_hoist <<'LEO'
program opt_else_if_hoist.aleo {
    @noupgrade
    constructor() {}

    fn main(take_first: bool) -> u8 {
        let missing: u8? = none;

        if take_first {
            return 1u8;
        } else if missing.unwrap() == 0u8 {
            return 2u8;
        } else {
            return 3u8;
        }
    }
}
LEO
elseif_out="$ROOT/opt_else_if_hoist.out"
(cd "$ROOT/opt_else_if_hoist" && run_cmd "$elseif_out" "$LEO_BIN" run main true)
elseif_status=$?
show_output "$elseif_out"

new_project opt_else_block_ctrl
write_main opt_else_block_ctrl <<'LEO'
program opt_else_block_ctrl.aleo {
    @noupgrade
    constructor() {}

    fn main(take_first: bool) -> u8 {
        let missing: u8? = none;

        if take_first {
            return 1u8;
        } else {
            if missing.unwrap() == 0u8 {
                return 2u8;
            } else {
                return 3u8;
            }
        }
    }
}
LEO
elseblock_out="$ROOT/opt_else_block_ctrl.out"
(cd "$ROOT/opt_else_block_ctrl" && run_cmd "$elseblock_out" "$LEO_BIN" run main true)
elseblock_status=$?
show_output "$elseblock_out"
if [ "$elseif_status" -ne 0 ] && [ "$elseblock_status" -eq 0 ] && grep -q '1u8' "$elseblock_out"; then
  result option_else_if_unwrap confirmed "else-if fails while equivalent else-block returns 1u8"
elif [ "$elseif_status" -ne 0 ] && [ "$elseblock_status" -ne 0 ]; then
  result option_else_if_unwrap downgraded "both spellings fail; not an else-if-specific placement delta"
elif [ "$elseif_status" -eq 0 ] && grep -q '1u8' "$elseif_out"; then
  result option_else_if_unwrap fixed "else-if spelling returns 1u8"
else
  result option_else_if_unwrap inconclusive "else-if status=$elseif_status else-block status=$elseblock_status"
fi
