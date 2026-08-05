use leo_ast::{NetworkName, NodeBuilder};
use leo_compiler::{Compiler, CompilerOptions, run};
use leo_errors::Handler;
use leo_span::source_map::FileName;

fn compile_and_run(source: &str) -> Result<String, String> {
    let (handler, emitter) = Handler::new_with_buf();
    let mut compiler = Compiler::new(
        Some("name_only.aleo".to_string()),
        false,
        handler,
        std::rc::Rc::new(NodeBuilder::default()),
        Some(CompilerOptions::default()),
        indexmap::IndexMap::new(),
        NetworkName::TestnetV0,
    );
    let modules = vec![
        (
            "export fn add(X: u32) -> u32 { return X + nested::X; }",
            FileName::Custom("base.leo".into()),
        ),
        ("export const X: u32 = 7u32;", FileName::Custom("base/nested.leo".into())),
    ];

    let compiled = compiler
        .compile(source, FileName::Custom("name_only.leo".into()), &modules)
        .map_err(|error| format!("compile error: {error}; diagnostics: {:?}", emitter.extract_errs()))?;

    let config = run::Config {
        seed: 1234567890,
        start_height: None,
        programs: vec![run::Program { bytecode: compiled.primary.bytecode, name: compiled.primary.name }],
        skip_proving: true,
    };
    let case = run::Case {
        program_name: "name_only.aleo".into(),
        function: "main".into(),
        input: vec!["5u32".into()],
        ..Default::default()
    };
    let outcome = run::run_without_ledger(&config, &[case])
        .map_err(|error| format!("downstream evaluation failed to start: {error}"))?
        .into_iter()
        .next()
        .ok_or_else(|| "downstream evaluator returned no outcome".to_string())?;
    if !matches!(outcome.status, run::EvaluationStatus::Success) {
        return Err(format!("downstream evaluation status: {}", outcome.status));
    }
    Ok(outcome.output().to_string())
}

fn audit_case(label: &str, source: &str) -> Result<String, String> {
    compile_and_run(source).map_err(|error| format!("{label}: {error}"))
}

#[test]
fn audit_m1_qualified_path_substitution() {
    let target = r#"
program name_only.aleo {
    fn main(v: u32) -> u32 {
        return base::add(v);
    }

    @noupgrade
    constructor() {}
}
"#;
    let control = target.replace("nested::X", "nested::Y").replace("const X", "const Y");

    let control_output = audit_case("control", &control).expect("control must compile and evaluate");
    assert_eq!(control_output, "12u32", "control downstream oracle changed");

    match audit_case("target", target) {
        Ok(output) if output == "10u32" => {
            println!("AUDIT_RESULT=CONFIRMED root=M1 target_output={output} control_output={control_output}");
        }
        Ok(output) if output == "12u32" => {
            println!("AUDIT_RESULT=DISPROVED root=M1 target_output={output} control_output={control_output}");
        }
        Ok(output) => panic!(
            "AUDIT_RESULT=INCONCLUSIVE root=M1 unexpected_target_output={output} control_output={control_output}"
        ),
        Err(error) => panic!("AUDIT_RESULT=INCONCLUSIVE root=M1 {error}"),
    }
}
