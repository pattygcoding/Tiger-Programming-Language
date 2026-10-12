//! Parsing: precedence, statements, classes, and the syntax the parser rejects.
//!
//! A port of `pkg/parser/parser_test.go` and `pkg/parser/class_test.go`.

use tiger_ast::{Expr, Program, Stmt};

fn parse(source: &str) -> Program {
    tiger_parser::parse(source).unwrap_or_else(|error| panic!("parse({:?}): {}", source, error))
}

/// Rejects a program, returning its message.
#[track_caller]
fn reject(source: &str) -> String {
    match tiger_parser::parse(source) {
        Ok(_) => panic!("expected a syntax error: {:?}", source),
        Err(error) => error,
    }
}

/// Deep nesting needs a bigger stack than a test thread has. Only the outcome
/// crosses the thread boundary, because the AST holds `Rc`s.
fn parses_deep(source: String) -> bool {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || tiger_parser::parse(&source).is_ok())
        .expect("the parser thread must start")
        .join()
        .expect("the parser must not panic")
}

/// The value of the first statement's assignment.
fn assigned(source: &str) -> Expr {
    let program = parse(source);
    match &program.statements[0] {
        Stmt::Assign { value, .. } => value.clone(),
        other => panic!("expected an assignment, got {:?}", other),
    }
}

fn binary(expression: &Expr) -> (String, Expr, Expr) {
    match expression {
        Expr::Binary { operator, left, right, .. } => (operator.clone(), (**left).clone(), (**right).clone()),
        other => panic!("expected a binary expression, got {:?}", other),
    }
}

#[test]
fn precedence() {
    let (operator, _, right) = binary(&assigned("const answer = 1 + 2 * 3;"));
    assert_eq!(operator, "+");
    assert_eq!(binary(&right).0, "*");
}

#[test]
fn syntax() {
    parse(
        "const PI = 3.14;\n\
         function add(a, b) { return a + b; }\n\
         if PI > 3 { print(add(1, 2)); } elif PI == 3 { print(\"three\"); } else { print(false); }\n\
         while false { const value = 1; }\n\
         for item in [1, 2, \"apple\", true] { print(item); }\n\
         const config = {\"port\": 8080, \"host\": \"localhost\"};\n\
         config[\"port\"] = 80;\n\
         function empty() { return; };\n",
    );
}

#[test]
fn invalid_syntax() {
    for source in [
        "value = 1",
        "print(1)",
        "function bad() { return 1 }",
        "if true: { print(1); }",
        "while true print(1);",
        "for item in [1] print(item);",
        "const value;",
        "function bad(a, a) {}",
        "return 1;",
        "1 = 2;",
        "if true {",
        "[1, 2;",
        "{1 2};",
        "var value;",
        "var value = ;",
        "const value = ;",
        "var items[0] = 1;",
        "const this.value = 1;",
    ] {
        reject(source);
    }
}

#[test]
fn syntax_gaps() {
    let program = parse(
        "const pick = \"a\" if flag else \"b\";\n\
         outer: for item in [1, 2] {\n\
             if item == 1 { continue outer; }\n\
             break outer;\n\
         }\n\
         do { print(1); } while (false);\n\
         const window = [1, 2, 3][0:2];",
    );

    let Stmt::Assign { value, .. } = &program.statements[0] else { panic!("assignment") };
    let Expr::Conditional { condition, then, otherwise, .. } = value else { panic!("conditional expression did not parse: {:?}", value) };
    assert!(matches!(&**condition, Expr::Identifier { .. }));
    assert!(matches!(&**then, Expr::Literal { .. }));
    assert!(matches!(&**otherwise, Expr::Literal { .. }));

    let Stmt::For { label, body, .. } = &program.statements[1] else { panic!("for") };
    assert_eq!(label, "outer");
    let Stmt::If { branches, .. } = &body[0] else { panic!("if") };
    let Stmt::Control { kind, label, .. } = &branches[0].body[0] else { panic!("continue") };
    assert_eq!((kind.as_str(), label.as_str()), ("continue", "outer"));
    let Stmt::Control { kind, label, .. } = &body[1] else { panic!("break") };
    assert_eq!((kind.as_str(), label.as_str()), ("break", "outer"));

    assert!(matches!(program.statements[2], Stmt::DoWhile { .. }));
    let Stmt::Assign { value, .. } = &program.statements[3] else { panic!("slice assignment") };
    match value {
        Expr::Slice { start, stop, step, .. } => {
            assert!(start.is_some() && stop.is_some() && step.is_none());
        }
        other => panic!("slice did not parse: {:?}", other),
    }
}

#[test]
fn syntax_gap_errors() {
    for source in [
        "print(1 if true);",
        "print(1 if true else);",
        "break missing;",
        "outer: for item in [1] { continue missing; }",
        "outer: while true { function inner() { break outer; } }",
        "do { print(1); } while;",
        "do { print(1); } while true",
        "for item in [1] { continue 5; }",
    ] {
        reject(source);
    }
}

#[test]
fn nesting_limit() {
    assert!(!parses_deep(format!("{}1{};", "(".repeat(600), ")".repeat(600))));
    assert!(!parses_deep(format!("{}1{};", "f\"{".repeat(600), "}\"".repeat(600))));
    assert!(!parses_deep(format!("{}1{}", "switch 1 { case 1: ".repeat(600), "}".repeat(600))));
}

#[test]
fn declarations() {
    let program = parse("const limit = 3; var count = 0; count = 1; count += 2;");
    for (index, statement) in program.statements.iter().enumerate() {
        let Stmt::Assign { declaration, constant, .. } = statement else { panic!("assignment") };
        assert_eq!(*declaration, index < 2, "declaration flag at {}", index);
        assert_eq!(*constant, index == 0, "constant flag at {}", index);
    }
    let Stmt::Assign { operator, .. } = &program.statements[3] else { panic!("compound") };
    assert_eq!(operator, "+=");
}

#[test]
fn keyword_arguments() {
    let program = parse("print(\"working\", end=\"\");");
    let Stmt::Expression { value, .. } = &program.statements[0] else { panic!("call") };
    let Expr::Call { arguments, keywords, .. } = value else { panic!("call") };
    assert_eq!(arguments.len(), 1);
    assert_eq!(keywords.len(), 1);
    assert_eq!(keywords[0].name, "end");
    reject("print(end=\"\", 1);");
    reject("print(end=\"\", end=\"!\");");
}

#[test]
fn variadic_and_spread_syntax() {
    for source in [
        "function collect(first, *rest, **options) { return rest; }",
        "function only(*rest) { return rest; }",
        "function only(**options) { return options; }",
        "function keyword_first(**options) {}",
        "function mixed(a, b, *rest, **options) {}",
        "class Item { function method(*rest, **options) {} }",
        "const numbers = [1, 2]; collect(*numbers);",
        "collect(**{\"a\": 1});",
        "collect(1, *numbers, key=\"value\", **{\"other\": 2});",
    ] {
        parse(source);
    }
    for source in [
        "function bad(*rest, *more) {}",
        "function bad(**options, **more) {}",
        "function bad(**options, *rest) {}",
        "function bad(first, *first) {}",
        "function bad(a, *rest, extra) {}",
        "function bad(*) {}",
        "function bad(**) {}",
        "class Item { function method(this, *rest) {} }",
        "collect(*numbers, key=\"value\", 2);",
        "collect(key=\"value\", *numbers);",
    ] {
        reject(source);
    }
}

#[test]
fn imports() {
    let program = parse("import \"lib/math.tg\" as math; print(math.square(3));");
    let Stmt::Import { path, name, .. } = &program.statements[0] else { panic!("import") };
    assert_eq!((path.as_str(), name.as_str()), ("lib/math.tg", "math"));
    for source in [
        "import \"math.tg\";",
        "import math as math;",
        "import \"math.tg\" as;",
        "import f\"math.tg\" as math;",
        "import \"math.tg\" as math",
    ] {
        reject(source);
    }
}

#[test]
fn control_syntax() {
    for source in [
        "cfor (var count = 0; count < 3; ++count) { if count == 1 { continue; } } else { print(\"done\"); }",
        "cfor (;;) { break; }",
        "while false {} else { print(\"empty\"); } for item in [] {} else { print(\"empty\"); }",
        "switch 1 { case 1: print(\"one\"); break; default: print(\"other\"); }",
        "var count = 0; print(++count, count++, --count, count--);",
    ] {
        parse(source);
    }
    for source in [
        "break;",
        "continue;",
        "switch 1 { case 1: continue; }",
        "while true { function invalid() { break; } }",
        "switch 1 { default: break; default: break; }",
        "++1;",
        "print(1)++;",
        "cfor (; true; var value = 1) {}",
        "try { print(1); }",
        "try {} catch {}",
        "try {} catch (error, other) {}",
        "throw;",
        "private var value = 1;",
        "class Item { public private var value = 1; }",
        "class Item { var value = 1; function value() {} }",
    ] {
        reject(source);
    }
}

#[test]
fn c_style_conditions() {
    let program = parse("cif (a && b || c) { print(1); } celse cif (d) {} celse { print(2); }");
    let Stmt::If { branches, else_block, .. } = &program.statements[0] else { panic!("if") };
    assert_eq!(branches.len(), 2);
    assert_eq!(else_block.len(), 1);
    let (operator, left, _) = binary(&branches[0].condition);
    assert_eq!(operator, "or");
    assert_eq!(binary(&left).0, "and");

    for (source, message) in [
        ("print(a && b);", "&& is only allowed in cif and cfor headers; use and"),
        ("if a || b {}", "|| is only allowed in cif and cfor headers; use or"),
        ("cif (a) {} else {}", "use celse or celse cif after a cif block"),
        ("if a {} celse {}", "use elif or else after an if block"),
        ("cif (a) { print(a && b); }", "&& is only allowed"),
        ("cfor (;;) { print(a || b); }", "|| is only allowed"),
    ] {
        let error = reject(source);
        assert!(error.contains(message), "{}: expected {:?}, got {}", source, message, error);
    }
}

#[test]
fn power_and_bitwise_precedence() {
    let (operator, _, right) = binary(&assigned("const power = 2 ** 3 ** 2;"));
    assert_eq!(operator, "**");
    assert_eq!(binary(&right).0, "**", "** must be right-associative");

    let Expr::Unary { operator, right, .. } = assigned("const unary = -2 ** 2;") else { panic!("unary") };
    assert_eq!(operator, "-");
    assert_eq!(binary(&right).0, "**", "unary minus must bind looser than **");

    let (operator, _, right) = binary(&assigned("const bits = 1 | 2 & 3;"));
    assert_eq!(operator, "|");
    assert_eq!(binary(&right).0, "&", "& must bind tighter than |");

    let (operator, left, _) = binary(&assigned("const shift = 1 + 2 << 3;"));
    assert_eq!(operator, "<<");
    assert_eq!(binary(&left).0, "+", "+ must bind tighter than <<");

    let (operator, left, _) = binary(&assigned("const compare = 1 | 2 == 3;"));
    assert_eq!(operator, "==");
    assert_eq!(binary(&left).0, "|", "| must bind tighter than ==");

    let (operator, left, _) = binary(&assigned("const floor = 6 div 3 * 2;"));
    assert_eq!(operator, "*");
    assert_eq!(binary(&left).0, "div", "div must be left-associative with *");
}

#[test]
fn function_ergonomics_parsing() {
    let named = parse("function f(a, b = 2) { return a; }");
    let Stmt::Function(declaration) = &named.statements[0] else { panic!("function") };
    assert_eq!(declaration.parameters.len(), 2);
    assert!(declaration.defaults[0].is_none());
    assert!(declaration.defaults[1].is_some());

    let anonymous = parse("const f = function(x) { return x; };");
    let Stmt::Assign { value, .. } = &anonymous.statements[0] else { panic!("assign") };
    let Expr::Function(declaration) = value else { panic!("anonymous function") };
    assert_eq!(declaration.name, "");

    let multiple = parse("function f() { return 1, 2; }");
    let Stmt::Function(declaration) = &multiple.statements[0] else { panic!("function") };
    let Stmt::Return { value, .. } = &declaration.body[0] else { panic!("return") };
    let Some(Expr::List { elements, .. }) = value else { panic!("list return") };
    assert_eq!(elements.len(), 2);

    let destructured = parse("const [a, b] = pair();");
    let Stmt::Destructure { lookup, names, constant, .. } = &destructured.statements[0] else { panic!("destructure") };
    assert!(!*lookup);
    assert_eq!(names.len(), 2);
    assert!(*constant);

    let record = parse("var {x, y} = record;");
    let Stmt::Destructure { lookup, constant, .. } = &record.statements[0] else { panic!("destructure") };
    assert!(*lookup);
    assert!(!*constant);

    reject("function bad(a = 1, b) { return a; }");
}

#[test]
fn try_catches_and_finally() {
    let program = parse("try { print(1); } catch (error) if isinstance(error, \"error\") { print(2); } finally { print(3); }");
    let Stmt::Try { catches, finally, .. } = &program.statements[0] else { panic!("try") };
    assert_eq!(catches.len(), 1);
    assert!(catches[0].condition.is_some());
    assert!(!finally.is_empty());

    parse("try { } finally { }");
    reject("try { }");

    let chained = parse("try { } catch (error) if error[\"kind\"] == \"x\" { } catch (other) { }");
    let Stmt::Try { catches, .. } = &chained.statements[0] else { panic!("try") };
    assert_eq!(catches.len(), 2);
}

#[test]
fn class_syntax() {
    let program = parse(
        "class Animal {\n\
             Animal(name) { this.name = name; }\n\
             function speak() { return this.name; }\n\
         }\n\
         class Dog extends Animal {\n\
             Dog(name) { super(name); }\n\
             function speak() { return super.speak() + \"!\"; }\n\
         }\n\
         const pet = Dog(\"Rex\");\n\
         pet.friend = Animal(\"Cat\");\n\
         print(pet.friend.speak(), [pet][0].name);\n",
    );
    let Stmt::Class(derived) = &program.statements[1] else { panic!("class") };
    assert_eq!(derived.parent_name.as_deref(), Some("Animal"));
    assert_eq!(derived.methods.len(), 1);
    let constructor = derived.constructor.as_ref().expect("constructor");
    assert_eq!(constructor.name, "Dog");

    let Stmt::Assign { target, .. } = &program.statements[3] else { panic!("assign") };
    let Expr::Property { name, .. } = target else { panic!("property") };
    assert_eq!(name, "friend");
}

#[test]
fn invalid_class_syntax() {
    for source in [
        "class Animal: {}",
        "class Animal extends Parent, Other {}",
        "class Animal extends Animal {}",
        "class Animal(Parent) {}",
        "class Animal extends {}",
        "class Animal extends 1 {}",
        "class Animal { function speak(this) {} }",
        "class Animal { function speak(name, this) {} }",
        "class Animal { function speak() {} function speak() {} }",
        "class Animal { value = 1; }",
        "class Animal { Animal() { this.name = \"Rex\" } }",
        "class Animal { Animal(this) {} }",
        "class Animal { Animal(name, this) {} }",
        "class Animal { Animal() {} Animal(name) {} }",
        "class Animal { function Animal() {} }",
        "class Animal { Other() {} }",
        "super.;",
        "class Animal { const legs; }",
        "class Animal { var legs }",
        "class Animal { var legs = ; }",
        "class Animal { function speak() { return 1 } }",
        "class Animal {",
        "pet.name = 1",
        "pet.speak()",
        "super;",
        "pet.;",
    ] {
        reject(source);
    }
}

#[test]
fn parenthesized_inheritance_hint() {
    let error = reject("class Dog(Animal) {}");
    assert!(error.contains("use 'class Dog extends Parent'"), "{}", error);
}
