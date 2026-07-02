/*
seed = 987654321

[case]
program = "repro_cast_equal_width_signedness.aleo"
function = "main"
input = ["-1i8"]
*/

program repro_cast_equal_width_signedness.aleo {
    fn main(x: i8) -> i8 {
        let y: i8 = (x as u8) as i8;
        return y;
    }

    @noupgrade
    constructor() {}
}
