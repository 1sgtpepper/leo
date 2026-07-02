/*
seed = 987654321

[case]
program = "repro_ssa_fallible_sub_erased.aleo"
function = "main"
input = ["0u8", "1u8"]
*/

program repro_ssa_fallible_sub_erased.aleo {
    fn main(x: u8, y: u8) -> u8 {
        let diff: u8 = x - y;
        return diff * 0u8;
    }

    @noupgrade
    constructor() {}
}
