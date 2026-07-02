/*
seed = 987654321

[case]
program = "repro_loop_inclusive_singleton.aleo"
function = "main"
input = []
*/

program repro_loop_inclusive_singleton.aleo {
    fn main() -> bool {
        for i: u8 in 0u8..=0u8 {
            assert(false);
        }
        return true;
    }

    @noupgrade
    constructor() {}
}
