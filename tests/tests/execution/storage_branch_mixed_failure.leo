/*
seed = 123456789
min_height = 16

[case]
program = "storage_branch_mixed_failure.aleo"
function = "run"
input = ["true", "0u32"]

[case]
program = "storage_branch_mixed_failure.aleo"
function = "run"
input = ["true", "2u32"]

[case]
program = "storage_branch_mixed_failure.aleo"
function = "run"
input = ["false", "2u32"]

[case]
program = "storage_branch_mixed_failure.aleo"
function = "run"
input = ["false", "0u32"]
*/

program storage_branch_mixed_failure.aleo {
    storage values: [u32];

    fn run(public take_storage: bool, public denominator: u32) -> Final {
        return final {
            values.clear();
            values.push(11u32);

            let selected: u32 = take_storage
                ? values.pop().unwrap_or(0u32)
                : 10u32 / denominator;

            if take_storage {
                assert(selected == 11u32);
                assert(values.len() == 0u32);
            } else {
                assert(selected == 5u32);
                assert(values.len() == 1u32);
            }
        };
    }

    @noupgrade
    constructor() {}
}
