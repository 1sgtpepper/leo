/*
seed = 987654321
min_height = 16

[case]
program = "repro_storage_ternary_pop_unselected.aleo"
function = "main"
input = []
private_key = "APrivateKey1zkpH5Ne1Xfd79t61VhK7b6yaYz92yW5dbuVkiFheR7rwCDE"
*/

program repro_storage_ternary_pop_unselected.aleo {
    storage vec: [u32];

    fn main() -> Final {
        return final {
            vec.push(99u32);
            let popped: u32? = false ? vec.pop() : none;
            assert(popped == none);
            assert(vec.len() == 1u32);
            let remaining: u32 = vec.pop().unwrap();
            assert(remaining == 99u32);
        };
    }

    @noupgrade
    constructor() {}
}
