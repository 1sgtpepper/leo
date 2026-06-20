#!/usr/bin/env bash
set +e
set -u -o pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
LEO="${LEO:-$ROOT/target/release/leo}"
WORK="${RUNNER_TEMP:-/tmp}/leo-candidate-repros"
LOG_DIR="$WORK/logs"
RESULTS="$WORK/repro-results.md"

VALID_SIG="sign1vck9vl9w0w9xh3z9dph5qmyqacqapyuve7jfqvgfyslglx92auqmr6965sdn7wp4n9nahysxps633j9d2g4rgs2zhxlz75sxm8af7qvr22qjwn4zc0pzv87twjygsz9m7ekljmuw4jpzf68rwuq99r0tp735vs6220q7tp60nr7llkwstcvu49wdhydx5x2s3sftjskzawhqvqcj6tl"
VALID_ADDR="aleo1rhgdu77hgyqd3xjj8ucu3jj9r2krwz6mnzyd80gncr5fxcwlh5rsvzp9px"

mkdir -p "$WORK" "$LOG_DIR"
rm -rf "$WORK"/projects
mkdir -p "$WORK"/projects

cat >"$RESULTS" <<EOF
# Leo Candidate Reproduction Results

- Repository commit: $(git -C "$ROOT" rev-parse HEAD)
- Leo binary: $LEO

| Candidate | Status | Evidence |
| --- | --- | --- |
EOF

record() {
    local candidate="$1"
    local status="$2"
    local evidence="$3"
    printf '| `%s` | %s | %s |\n' "$candidate" "$status" "$evidence" >>"$RESULTS"
    printf '\n[%s] %s - %s\n' "$candidate" "$status" "$evidence"
}

make_project() {
    local name="$1"
    local main_source="$2"
    local project="$WORK/projects/$name"

    mkdir -p "$WORK/projects"
    (cd "$WORK/projects" && "$LEO" new "$name" >"$LOG_DIR/$name.new.log" 2>&1)
    cat >"$project/src/main.leo" <<<"$main_source"
    printf '%s\n' "$project"
}

aleo_artifact() {
    local project="$1"
    find "$project/build" -type f -name '*.aleo' | sort | head -n 1
}

run_capture() {
    local log="$1"
    shift
    set +e
    "$@" >"$log" 2>&1
    local status=$?
    set +e
    return "$status"
}

case_signature_verify_dce() {
    local name="sig_dce_unused"
    local project
    project="$(make_project "$name" 'program sig_dce_unused.aleo {
    fn main(sig: signature, signer: address, msg: field) -> field {
        let ignored: bool = signature::verify(sig, signer, msg);
        return 1field;
    }

    @noupgrade
    constructor() {}
}')"

    local build_log="$LOG_DIR/$name.build.log"
    if ! run_capture "$build_log" bash -lc "cd '$project' && '$LEO' build"; then
        record "$name" "INCONCLUSIVE" "project did not build; see $build_log"
        return
    fi

    local artifact
    artifact="$(aleo_artifact "$project")"
    local invalid_log="$LOG_DIR/$name.invalid-run.log"
    run_capture "$invalid_log" bash -lc "cd '$project' && '$LEO' run main '$VALID_SIG' '$VALID_ADDR' 6field"
    local invalid_status=$?

    if ! grep -q 'sign.verify' "$artifact" && [ "$invalid_status" -eq 0 ] && grep -q '1field' "$invalid_log"; then
        record "$name" "CONFIRMED" "generated Aleo artifact contains no sign.verify, and invalid-message execution returned 1field; logs: $artifact, $invalid_log"
    elif grep -q 'sign.verify' "$artifact"; then
        record "$name" "NOT_CONFIRMED" "generated Aleo artifact preserved sign.verify; artifact: $artifact"
    else
        record "$name" "INCONCLUSIVE" "sign.verify was absent, but invalid run did not cleanly return 1field; see $invalid_log"
    fi
}

case_vector_push_order() {
    local name="push_len_semantics"
    local project
    project="$(make_project "$name" 'program push_len_semantics.aleo {
    storage values: [u32];

    fn push_literal(v: u32) -> Final {
        return final {
            values.push(v);
        };
    }

    fn push_len() -> Final {
        return final {
            values.push(values.len());
        };
    }

    @noupgrade
    constructor() {}
}')"

    mkdir -p "$project/tests"
    cat >"$project/tests/test_push_len_semantics.leo" <<'EOF'
import push_len_semantics.aleo;

program test_push_len_semantics.aleo {
    @test
    fn empty_pre_len() -> Final {
        let f: Final = push_len_semantics.aleo::push_len();
        return final {
            f.run();
            assert(push_len_semantics.aleo::values.len() == 1u32);
            assert(push_len_semantics.aleo::values.get(0u32).unwrap() == 0u32);
        };
    }

    @test
    fn nonempty_pre_len() -> Final {
        let f1: Final = push_len_semantics.aleo::push_literal(7u32);
        let f2: Final = push_len_semantics.aleo::push_len();
        return final {
            f1.run();
            f2.run();
            assert(push_len_semantics.aleo::values.len() == 2u32);
            assert(push_len_semantics.aleo::values.get(1u32).unwrap() == 1u32);
        };
    }

    @noupgrade
    constructor() {}
}
EOF

    local test_log="$LOG_DIR/$name.test.log"
    if run_capture "$test_log" bash -lc "cd '$project' && '$LEO' test"; then
        record "$name" "NOT_CONFIRMED" "storage post-state matched source expectation in leo test; see $test_log"
    else
        record "$name" "CONFIRMED" "leo test failed the pre-push-length storage invariant; see $test_log"
    fi
}

case_ssa_array_index() {
    local read_name="ssa_array_index_read"
    local read_project
    read_project="$(make_project "$read_name" 'program ssa_array_index_read.aleo {
    fn main() -> u8 {
        let arr: [u8; 2] = [11u8, 22u8];
        let i: u32 = 0u32;
        i = 1u32;
        return arr[i];
    }

    @noupgrade
    constructor() {}
}')"

    local write_name="ssa_array_index_write"
    local write_project
    write_project="$(make_project "$write_name" 'program ssa_array_index_write.aleo {
    fn main() -> u8 {
        let arr: [u8; 2] = [11u8, 22u8];
        let i: u32 = 0u32;
        i = 1u32;
        arr[i] = 33u8;
        return arr[1u32];
    }

    @noupgrade
    constructor() {}
}')"

    local read_log="$LOG_DIR/$read_name.run.log"
    run_capture "$read_log" bash -lc "cd '$read_project' && '$LEO' run main"
    local read_status=$?

    local write_log="$LOG_DIR/$write_name.run.log"
    run_capture "$write_log" bash -lc "cd '$write_project' && '$LEO' run main"
    local write_status=$?

    if [ "$read_status" -eq 0 ] && grep -q '22u8' "$read_log" && [ "$write_status" -eq 0 ] && grep -q '33u8' "$write_log"; then
        record "ssa_array_index_const_miss" "NOT_CONFIRMED" "reassigned local array index folded correctly for read and write variants; logs: $read_log, $write_log"
    else
        record "ssa_array_index_const_miss" "CONFIRMED_OR_CONTRACT_GAP" "reassigned local array index did not produce expected outputs; verify whether this source form is intended to be const-foldable; logs: $read_log, $write_log"
    fi
}

case_vector_get_expr_stmt() {
    local name="vector_get_stmt_drop"
    local project
    project="$(make_project "$name" 'program vector_get_stmt_drop.aleo {
    storage id_numbers: [u64];

    fn dropped_get_key(public x: u32) -> Final {
        return final {
            id_numbers.get(1u32 / x);
        };
    }

    @noupgrade
    constructor() {}
}')"

    mkdir -p "$project/tests"
    cat >"$project/tests/test_vector_get_stmt_drop.leo" <<'EOF'
import vector_get_stmt_drop.aleo;

program test_vector_get_stmt_drop.aleo {
    @test
    @should_fail
    fn divzero_get() -> Final {
        let f: Final = vector_get_stmt_drop.aleo::dropped_get_key(0u32);
        return final {
            f.run();
        };
    }

    @noupgrade
    constructor() {}
}
EOF

    local build_log="$LOG_DIR/$name.build.log"
    run_capture "$build_log" bash -lc "cd '$project' && '$LEO' build"
    local artifact=""
    artifact="$(aleo_artifact "$project" 2>/dev/null || true)"

    local test_log="$LOG_DIR/$name.test.log"
    if run_capture "$test_log" bash -lc "cd '$project' && '$LEO' test"; then
        record "$name" "NOT_CONFIRMED" "@should_fail test failed as expected, so division-by-zero behavior was preserved; artifact: $artifact"
    else
        if [ -n "$artifact" ] && ! grep -Eq '\\bdiv\\b|/ x| r[0-9]+ 0u32' "$artifact"; then
            record "$name" "CONFIRMED" "@should_fail test did not observe a failure and generated artifact appears to omit division; logs: $test_log, $artifact"
        else
            record "$name" "INCONCLUSIVE" "leo test failed, but artifact/log needs inspection to distinguish bug from compile/test issue; logs: $test_log, $artifact"
        fi
    fi
}

case_finalizer_phi() {
    local name="finalizer_phi_undef"
    local project
    project="$(make_project "$name" 'program finalizer_phi_undef.aleo {
    mapping seen: u8 => u32;

    fn choose(public flag: bool) -> Final {
        return final {
            let y: u32 = 0u32;

            if flag {
                y = block.height + 1u32;
            } else {
                y = block.height + 2u32;
            }

            seen.set(0u8, y);
        };
    }

    @noupgrade
    constructor() {}
}')"

    mkdir -p "$project/tests"
    cat >"$project/tests/test_finalizer_phi_undef.leo" <<'EOF'
import finalizer_phi_undef.aleo;

program test_finalizer_phi_undef.aleo {
    @test
    fn true_branch_runs_and_writes() -> Final {
        let f: Final = finalizer_phi_undef.aleo::choose(true);
        return final {
            f.run();
            assert(finalizer_phi_undef.aleo::seen.get_or_use(0u8, 0u32) > 0u32);
        };
    }

    @test
    fn false_branch_runs_and_writes() -> Final {
        let f: Final = finalizer_phi_undef.aleo::choose(false);
        return final {
            f.run();
            assert(finalizer_phi_undef.aleo::seen.get_or_use(0u8, 0u32) > 0u32);
        };
    }

    @noupgrade
    constructor() {}
}
EOF

    local build_log="$LOG_DIR/$name.build.log"
    if ! run_capture "$build_log" bash -lc "cd '$project' && '$LEO' build"; then
        if grep -q 'cannot reassign `y` from a conditional scope to an outer scope in a final block' "$build_log"; then
            record "$name" "NOT_CONFIRMED" "current Leo rejects the source form before codegen, so no wrong artifact is produced; see $build_log"
        else
            record "$name" "INCONCLUSIVE" "project did not build; see $build_log"
        fi
        return
    fi

    local artifact
    artifact="$(aleo_artifact "$project")"
    local test_log="$LOG_DIR/$name.test.log"
    run_capture "$test_log" bash -lc "cd '$project' && '$LEO' test"
    local test_status=$?

    if grep -q 'ternary' "$artifact" && grep -q 'branch.eq' "$artifact"; then
        if [ "$test_status" -eq 0 ]; then
            record "$name" "ARTIFACT_SUSPICIOUS" "compiled finalizer contains branch plus post-branch ternary shape, but both branch tests passed; artifact/logs: $artifact, $test_log"
        else
            record "$name" "CONFIRMED" "branch tests failed and artifact contains branch plus ternary merge shape; logs: $artifact, $test_log"
        fi
    elif [ "$test_status" -eq 0 ]; then
        record "$name" "NOT_CONFIRMED" "no suspicious ternary merge shape found and both branch tests passed; logs: $artifact, $test_log"
    else
        record "$name" "INCONCLUSIVE" "tests failed without the expected artifact shape; logs: $artifact, $test_log"
    fi
}

main() {
    if [ ! -x "$LEO" ]; then
        echo "Leo binary is not executable: $LEO" >&2
        exit 2
    fi

    "$LEO" --version

    case_signature_verify_dce
    case_vector_push_order
    case_ssa_array_index
    case_vector_get_expr_stmt
    case_finalizer_phi

    printf '\n===== SUMMARY =====\n'
    cat "$RESULTS"
    printf '\nDetailed logs: %s\n' "$LOG_DIR"
}

main "$@"
