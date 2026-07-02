/*
seed = 987654321

[case]
program = "repro_flattening_untaken_division.aleo"
function = "main"
input = ["false", "0u32"]
*/

program repro_flattening_untaken_division.aleo {
    fn main(take_bad: bool, denom: u32) -> u32 {
        if take_bad {
            let value: u32 = 1u32 / denom;
            return value;
        }
        return 0u32;
    }

    @noupgrade
    constructor() {}
}
