/*
seed = 123456789
min_height = 16

[case]
program = "vector_index_evaluation_semantics.aleo"
function = "set_bound"
input = []

[case]
program = "vector_index_evaluation_semantics.aleo"
function = "set_inline"
input = []

[case]
program = "vector_index_evaluation_semantics.aleo"
function = "swap_remove_bound"
input = []

[case]
program = "vector_index_evaluation_semantics.aleo"
function = "swap_remove_inline"
input = []
*/

program vector_index_evaluation_semantics.aleo {
    storage values: [u32];
    mapping calls: u32 => u32;

    fn set_bound() -> Final {
        return final {
            reset();
            let index: u32 = next_index();
            values.set(index, 99u32);
            assert(calls.get_or_use(0u32, 0u32) == 1u32);
        };
    }

    fn set_inline() -> Final {
        return final {
            reset();
            values.set(next_index(), 99u32);
            assert(calls.get_or_use(0u32, 0u32) == 1u32);
        };
    }

    fn swap_remove_bound() -> Final {
        return final {
            reset();
            let index: u32 = next_index();
            let removed: u32 = values.swap_remove(index);
            assert(removed == 10u32 || removed == 20u32);
            assert(calls.get_or_use(0u32, 0u32) == 1u32);
        };
    }

    fn swap_remove_inline() -> Final {
        return final {
            reset();
            let removed: u32 = values.swap_remove(next_index());
            assert(removed == 10u32 || removed == 20u32);
            assert(calls.get_or_use(0u32, 0u32) == 1u32);
        };
    }

    final fn reset() {
        values.clear();
        values.push(10u32);
        values.push(20u32);
        calls.set(0u32, 0u32);
    }

    final fn next_index() -> u32 {
        let count: u32 = calls.get_or_use(0u32, 0u32);
        calls.set(0u32, count + 1u32);
        return ChaCha::rand_u32() & 1u32;
    }

    @noupgrade
    constructor() {}
}
