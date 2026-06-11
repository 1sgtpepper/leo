/*
seed = 123456789

[case]
program = "test.aleo"
function = "store_after_sub_check"
input = ["0u8"]

[case]
program = "test.aleo"
function = "store_after_sub_check"
input = ["1u8"]
*/

program test.aleo {
    mapping observed: u8 => u8;

    fn store_after_sub_check(amount: u8) -> Final {
        return final { finalize_store_after_sub_check(amount); };
    }

    @noupgrade
    constructor() {}
}

final fn finalize_store_after_sub_check(amount: u8) {
    let must_halt: u8 = 0u8 - amount;
    observed.set(0u8, amount);
}
