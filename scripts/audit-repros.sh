#!/usr/bin/env bash
set -u

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

"$SCRIPT_DIR/audit-repros/inclusive_loop_eq.sh"
"$SCRIPT_DIR/audit-repros/const_ternary_fallible.sh"
"$SCRIPT_DIR/audit-repros/dce_dynamic_array_index.sh"
"$SCRIPT_DIR/audit-repros/option_else_if_unwrap.sh"
