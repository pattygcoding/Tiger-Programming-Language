//! Import collection finds every literal `import`, however deeply nested.
//!
//! A port of `pkg/ast/imports_test.go`.

use tiger_ast::imports;

#[test]
fn imports_inside_do_while() {
    let program =
        tiger_parser::parse("outer: do { import \"a.tg\" as a; if true { import \"b.tg\" as b; } } while (false);").expect("parse");
    let found = imports(&program);
    assert_eq!(found.len(), 2, "{:?}", found);
    assert_eq!(found[0].0, "a.tg");
    assert_eq!(found[1].0, "b.tg");
}

#[test]
fn imports_inside_constructor() {
    let program =
        tiger_parser::parse("class Box { Box() { import \"a.tg\" as a; } function run() { import \"b.tg\" as b; } }").expect("parse");
    let found = imports(&program);
    assert_eq!(found.len(), 2, "{:?}", found);
    assert_eq!(found[0].0, "a.tg");
    assert_eq!(found[1].0, "b.tg");
}

#[test]
fn import_declarations_carry_positions() {
    let program = tiger_parser::parse("\nimport \"a.tg\" as a;").expect("parse");
    let declarations = tiger_ast::import_decls(&program);
    assert_eq!(declarations.len(), 1);
    assert_eq!(declarations[0].path, "a.tg");
    assert_eq!(declarations[0].name, "a");
    assert_eq!(declarations[0].base.line, 2);
}
