// Submodule items are NOT public surface (unless re-exported), so unused
// submodule fns/consts must still warn even in a library build.

// Used by `lib.leo::calls_submodule` — must NOT warn.
fn used_helper(x: u32) -> u32 {
    return x + 1u32;
}

// Submodule fn nobody calls — must warn.
fn dead_helper(x: u32) -> u32 {
    return x;
}

// Submodule fn with a dead local inside — fn warns AND the local warns.
fn dead_helper_with_dead_local(x: u32) -> u32 {
    let dead_sub_local: u32 = 0u32;
    return x;
}

// `_`-prefixed submodule fn — silenced (force-inlined, never reaches the VM).
fn _silent_sub_helper() -> u32 {
    return 0u32;
}

// Submodule const, used by `lib.leo::reads_sub_const` — must NOT warn.
const SUB_USED: u32 = 5u32;

// Submodule const, nobody references it — must warn.
const SUB_UNUSED: u32 = 6u32;
