#!/usr/bin/env bash
set -u

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
. "$SCRIPT_DIR/common.sh"

show_tooling
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

log "else-block control"
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
