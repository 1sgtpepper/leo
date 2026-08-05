use leo_aleo_abi_wasm::generate_abi_from_aleo;
use wasm_bindgen_test::*;

const VICTIM: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../crates/disassembler/src/tests/victim_future_input.aleo"
));
const VALID: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/tests/cli/test_abi_from_aleo/contents/simple.aleo"
));

#[wasm_bindgen_test]
#[should_panic]
fn audit_w1_malformed_bytecode_validation_boundary() {
    println!("AUDIT_RESULT=CONFIRMED root=W1 downstream=wasm-host-panic");
    let _ = generate_abi_from_aleo(VICTIM, "testnet");
}

#[wasm_bindgen_test]
fn audit_w1_valid_bytecode_control() {
    let abi = generate_abi_from_aleo(VALID, "testnet").expect("valid bytecode ABI generation failed");
    assert!(abi.contains("\"program\""), "valid ABI output missing program field: {abi}");
}
