/*
seed = 123456789
min_height = 16

[case]
program = "tuple_assert_neq_semantics.aleo"
function = "binary_partial_difference"
input = []

[case]
program = "tuple_assert_neq_semantics.aleo"
function = "assert_neq_partial_difference"
input = []

[case]
program = "tuple_assert_neq_semantics.aleo"
function = "assert_neq_all_different"
input = []

[case]
program = "tuple_assert_neq_semantics.aleo"
function = "assert_neq_equal"
input = []
*/

program tuple_assert_neq_semantics.aleo {
    fn binary_partial_difference() -> Final {
        return final {
            assert((1u32, 2u32) != (1u32, 3u32));
        };
    }

    fn assert_neq_partial_difference() -> Final {
        return final {
            assert_neq((1u32, 2u32), (1u32, 3u32));
        };
    }

    fn assert_neq_all_different() -> Final {
        return final {
            assert_neq((1u32, 2u32), (3u32, 4u32));
        };
    }

    fn assert_neq_equal() -> Final {
        return final {
            assert_neq((1u32, 2u32), (1u32, 2u32));
        };
    }

    @noupgrade
    constructor() {}
}
