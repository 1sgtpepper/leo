/*
seed = 123456789

[case]
program = "test.aleo"
function = "main"
input = ["0u8"]

[case]
program = "test.aleo"
function = "main"
input = ["1u8"]
*/

program test.aleo {
    fn main(x: u8) -> bool {
        let must_halt: u8 = 0u8 - x;
        return true;
    }

    @noupgrade
    constructor() {}
}
