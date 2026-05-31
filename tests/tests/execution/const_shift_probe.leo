/*
[case]
program = "test.aleo"
function = "const_overflow_u8"
input = []
[case]
program = "test.aleo"
function = "runtime_overflow_u8"
input = ["128u8"]
*/

program test.aleo {
    fn const_overflow_u8() -> u8 {
        return 128u8 << 1u8;
    }

    fn runtime_overflow_u8(x: u8) -> u8 {
        return x << 1u8;
    }

    @noupgrade
    constructor() {}
}
