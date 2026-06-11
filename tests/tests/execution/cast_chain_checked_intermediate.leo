/*
seed = 123456789

[case]
program = "test.aleo"
function = "store_roundtrip"
input = ["127u8"]

[case]
program = "test.aleo"
function = "store_roundtrip"
input = ["255u8"]
*/

program test.aleo {
    mapping observed: u8 => u8;

    fn store_roundtrip(x: u8) -> Final {
        let y: i8 = x as i8;
        let z: u8 = y as u8;

        return final { observed.set(0u8, z); };
    }

    @noupgrade
    constructor() {}
}
