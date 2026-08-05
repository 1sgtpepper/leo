use std::rc::Rc;

use leo_ast::{NetworkName, NodeBuilder};
use leo_compiler::{Compiler, CompilerOptions};
use leo_errors::Handler;
use leo_span::{create_session_if_not_set_then, source_map::FileName};
use serde_json::Value;

fn compile(source: &str, module_source: &str) -> Result<Value, String> {
    create_session_if_not_set_then(|_| {
        let (handler, emitter) = Handler::new_with_buf();
        let mut compiler = Compiler::new(
            Some("abi_path.aleo".to_string()),
            false,
            handler,
            Rc::new(NodeBuilder::default()),
            Some(CompilerOptions::default()),
            indexmap::IndexMap::new(),
            NetworkName::TestnetV0,
        );
        let modules = vec![(module_source, FileName::Custom("types.leo".into()))];

        compiler
            .compile(source, FileName::Custom("main.leo".into()), &modules)
            .map(|compiled| serde_json::to_value(compiled.primary.abi).expect("ABI is serializable"))
            .map_err(|error| format!("{error}; diagnostics: {:?}", emitter.extract_errs()))
    })
}

fn echo_input(abi: &Value) -> &Value {
    abi["functions"]
        .as_array()
        .and_then(|functions| functions.iter().find(|function| function["name"] == "echo"))
        .and_then(|function| function["inputs"].as_array())
        .and_then(|inputs| inputs.first())
        .unwrap_or_else(|| panic!("echo input missing from ABI: {abi}"))
}

fn path(ty: &Value) -> Option<&Vec<Value>> {
    ty.get("Plaintext")?.get("ty")?.get("Struct")?.get("path")?.as_array()
}

const PROGRAM: &str = r#"
record Token {
    owner: address,
}

program abi_path.aleo {
    fn echo(v: types::Token) -> types::Token {
        return v;
    }

    @noupgrade
    constructor() {}
}
"#;

const TYPES: &str = r#"
export struct Token {
    value: u32,
}
"#;

#[test]
fn audit_a1_composite_path_identity() {
    let target = compile(PROGRAM, TYPES).unwrap_or_else(|error| panic!("target failed: {error}"));
    let target_input = echo_input(&target);
    let target_path = path(target_input);

    let unique_types = TYPES.replace("Token", "Point");
    let unique_program = PROGRAM.replace("types::Token", "types::Point");
    let control = compile(&unique_program, &unique_types)
        .unwrap_or_else(|error| panic!("unique-path control failed: {error}"));
    let control_input = echo_input(&control);
    assert_eq!(
        path(control_input),
        Some(&vec![Value::String("types".into()), Value::String("Point".into())]),
        "unique module struct was not retained as a struct: {control}"
    );

    match target_path {
        Some(path) if path == &vec![Value::String("Token".into())] && target_input.get("Record").is_some() => {
            println!("AUDIT_RESULT=CONFIRMED root=A1 downstream=abi-labels-module-struct-as-record");
        }
        Some(path)
            if path == &vec![Value::String("types".into()), Value::String("Token".into())]
                && target_input.get("Plaintext").is_some() =>
        {
            println!("AUDIT_RESULT=DISPROVED root=A1 downstream=abi-preserves-struct-path");
        }
        _ => panic!("AUDIT_RESULT=INCONCLUSIVE root=A1 unexpected ABI: {target}"),
    }
}
