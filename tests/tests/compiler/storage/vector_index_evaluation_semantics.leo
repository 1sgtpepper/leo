// The index expression is one source call. Its mapping write and random draw
// are observable, so lowering must materialize it before reusing it.
program vector_index_evaluation_semantics.aleo {
    storage values: [u32];
    mapping calls: u32 => u32;

    fn set_bound() -> Final {
        return final {
            let index: u32 = next_index();
            values.set(index, 99u32);
        };
    }

    fn set_inline() -> Final {
        return final {
            values.set(next_index(), 99u32);
        };
    }

    fn swap_remove_bound() -> Final {
        return final {
            let index: u32 = next_index();
            let removed: u32 = values.swap_remove(index);
        };
    }

    fn swap_remove_inline() -> Final {
        return final {
            let removed: u32 = values.swap_remove(next_index());
        };
    }

    final fn next_index() -> u32 {
        let count: u32 = calls.get_or_use(0u32, 0u32);
        calls.set(0u32, count + 1u32);
        return ChaCha::rand_u32();
    }

    @noupgrade
    constructor() {}
}
