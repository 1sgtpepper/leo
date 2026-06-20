program test.aleo {
    @noupgrade
    constructor() {}

    fn absorb_true_arm(cond: bool, value: u8, fallback: u8) -> u8 {
        let inner: u8 = cond ? value : fallback;
        return cond ? inner : fallback;
    }

    fn absorb_false_arm(cond: bool, value: u8, fallback: u8) -> u8 {
        let inner: u8 = cond ? value : fallback;
        return cond ? value : inner;
    }

    fn keep_different_fallback(cond: bool, value: u8, first: u8, second: u8) -> u8 {
        let inner: u8 = cond ? value : first;
        return cond ? inner : second;
    }
}
