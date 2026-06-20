program vec_minimal.aleo {
    storage data: [u8];
    mapping result: bool => u8;

    fn read_one(idx: u32) -> Final {
        return final {
            let b: u8 = data.get(idx).unwrap_or(0u8);
            result.set(false, b);
        };
    }

    @noupgrade
    constructor() {}
}
