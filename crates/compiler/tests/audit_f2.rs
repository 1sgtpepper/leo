use std::rc::Rc;

use leo_ast::{NetworkName, NodeBuilder};
use leo_compiler::{Compiler, CompilerOptions};
use leo_errors::Handler;
use leo_span::source_map::FileName;

fn compile(source: &str) -> Result<String, String> {
    let (handler, emitter) = Handler::new_with_buf();
    let mut compiler = Compiler::new(
        Some("test.aleo".to_string()),
        false,
        handler,
        Rc::new(NodeBuilder::default()),
        Some(CompilerOptions::default()),
        indexmap::IndexMap::new(),
        NetworkName::TestnetV0,
    );

    compiler
        .compile(source, FileName::Custom("audit_f2.leo".into()), &Vec::new())
        .map(|compiled| compiled.primary.bytecode)
        .map_err(|error| format!("{error}; diagnostics: {:?}", emitter.extract_errs()))
}

const BASE: &str = r#"
export interface Base {
    fn get_value() -> u64;
}

export interface Extended: Base {
    fn double_val(x: u64) -> u64;
}

program test.aleo: Extended {
    fn get_value() -> u64 {
        return 0u64;
    }

    fn double_val(x: u64) -> u64 {
        return x * 2u64;
    }

    fn main(target: field, x: u64) -> u64 {
        return Extended@(target)::double_val(x);
    }

    @noupgrade
    constructor() {}
}
"#;

#[test]
fn audit_f2_inherited_dynamic_member_lookup() {
    let child_control = BASE;
    let parent_control = BASE.replace(
        "Extended@(target)::double_val(x)",
        "Base@(target)::get_value()",
    );
    let target = BASE.replace(
        "Extended@(target)::double_val(x)",
        "Extended@(target)::get_value()",
    );

    let child = compile(child_control).unwrap_or_else(|error| panic!("child control failed: {error}"));
    let parent = compile(&parent_control).unwrap_or_else(|error| panic!("parent control failed: {error}"));
    assert!(child.contains("call.dynamic") && child.contains("'double_val'"));
    assert!(parent.contains("call.dynamic") && parent.contains("'get_value'"));

    match compile(&target) {
        Err(error) if error.contains("get_value") && error.contains("not found") => {
            println!("AUDIT_RESULT=CONFIRMED root=F2 downstream=dynamic-call-rejected");
        }
        Ok(bytecode) if bytecode.contains("call.dynamic") && bytecode.contains("'get_value'") => {
            println!("AUDIT_RESULT=DISPROVED root=F2 downstream=dynamic-call-emitted");
        }
        Ok(bytecode) => panic!(
            "AUDIT_RESULT=INCONCLUSIVE root=F2 unexpected successful bytecode: {bytecode}"
        ),
        Err(error) => panic!("AUDIT_RESULT=INCONCLUSIVE root=F2 unrelated diagnostic: {error}"),
    }
}
