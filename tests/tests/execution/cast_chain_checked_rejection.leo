/*
seed = 123456789

[case]
program = "test.aleo"
function = "roundtrip_u8_i8_u8"
input = ["127u8"]

[case]
program = "test.aleo"
function = "roundtrip_u8_i8_u8"
input = ["255u8"]

[case]
program = "test.aleo"
function = "roundtrip_i8_u8_i8"
input = ["-1i8"]
*/

program test.aleo {
    fn roundtrip_u8_i8_u8(x: u8) -> u8 {
        return (x as i8) as u8;
    }

    fn roundtrip_i8_u8_i8(x: i8) -> i8 {
        return (x as u8) as i8;
    }

    @noupgrade
    constructor() {}
}
