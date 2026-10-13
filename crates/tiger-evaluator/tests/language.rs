//! The language core: programs, scoping, control flow, exceptions, formatted
//! strings, and variadics.
//!
//! A port of `pkg/evaluator/evaluator_test.go`.

mod common;

use common::{expect_errors, expect_outputs, run, run_failing_output, run_with_steps};

#[test]
fn programs() {
    expect_outputs(&[
        (
            "const MAX_RUNS = 5;\n\
             function factorial(n) { if n <= 1 { return 1; } return n * factorial(n - 1); }\n\
             var i = 1; while i <= MAX_RUNS { const val = factorial(i); print(\"factorial(\" + str(i) + \") = \" + str(val)); i = i + 1; }",
            "factorial(1) = 1\nfactorial(2) = 2\nfactorial(3) = 6\nfactorial(4) = 24\nfactorial(5) = 120\n",
        ),
        (
            "for x in [-1, 0, 1] { if x > 0 { print(\"Positive\"); } elif x == 0 { print(\"Zero\"); } else { print(\"Negative\"); } }",
            "Negative\nZero\nPositive\n",
        ),
        (
            "const items = [1, 2, \"apple\", true]; items[0] = 9; const config = {\"port\": 8080, \"host\": \"localhost\"}; config[\"port\"] = 80; print(items[-1], config[\"port\"], len(items)); for key in config { print(key); }",
            "true 80 4\nport\nhost\n",
        ),
        (
            "function counter() { var count = 0; function next() { count = count + 1; return count; } return next; } const next = counter(); print(next(), next());",
            "1 2\n",
        ),
        (
            "var count = 0; while count < 3 { const local = 1; count = count + local; } for item in [1, 2] { count = count + item; } print(count);",
            "6\n",
        ),
        ("const x = 3; function add(x) { x = x + 1; return x; } print(add(9), x);", "10 3\n"),
        (
            "function find() { for item in [1, 2, 3] { while true { if item == 2 { return item; } return 1; } } } function empty() {} function explicit() { return; } print(find(), empty(), explicit());",
            "1 null null\n",
        ),
        (
            "print(1 + 2 * 3, (1 + 2) * 3, -5 % 3, 5 % -3, 7 / 2); print(not 1 == 2, false and missing, true or missing, \"a\" < \"b\");",
            "7 9 1 -1 3.5\ntrue false true true\n",
        ),
        (
            "var value = 10; value += 5; value -= 3; value *= 2; value %= 7; print(value); var calls = 0; function index() { calls += 1; return 0; } const items = [5]; items[index()] *= 3; print(items, calls);",
            "3\n[15] 1\n",
        ),
        (
            "print(\"loading\", end=\"\"); print(\"...\", end=\" done\\n\"); print(end=\"\"); print(\"next\");",
            "loading... done\nnext\n",
        ),
        (
            "const items = [1, 2, 3]; const text = \"T🐯\"; print(items.size(), items.length(), text.size(), text.length(), len(text));",
            "3 3 2 2 2\n",
        ),
        (
            "const nums = [3, 1, 2]; print(nums.sort(), nums); nums.sort(reverse=true); print(nums); const words = [\"bb\", \"a\", \"cc\", \"d\"]; function size(word) { return len(word); } words.sort(key=size); print(words); words.sort(key=size, reverse=true); print(words); const empty = []; empty.sort(key=null); print(empty);",
            "null [1, 2, 3]\n[3, 2, 1]\n[\"a\", \"d\", \"bb\", \"cc\"]\n[\"bb\", \"cc\", \"a\", \"d\"]\n[]\n",
        ),
        (
            "print(2 in [1, 2], \"port\" in {\"port\": 80}, \"ell\" in \"hello\", [] == [], {\"a\": 1} == {\"a\": 1});",
            "true true true true true\n",
        ),
        (
            "const items = [null]; items[0] = items; const other = [null]; other[0] = other; print(items, items == other); const config = {}; config[\"self\"] = config; print(config);",
            "[[...]] true\n{\"self\": {...}}\n",
        ),
        ("const items = [1]; items[0] = 2; print(items);", "[2]\n"),
        (
            "const config = {1: \"number\", \"1\": \"string\", true: \"bool\", -0: \"zero\"}; print(config[1], config[\"1\"], config[true], config[0]);",
            "number string bool zero\n",
        ),
        ("for char in \"hi\" { print(char); } print(\"hello\"[-1], len(\"hello\"));", "h\ni\no 5\n"),
        ("const left = [1]; const both = left + [2]; both[0] = 9; print(left, both);", "[1] [9, 2]\n"),
    ]);
}

#[test]
fn runtime_errors() {
    expect_errors(&[
        ("const PI = 3; PI = 4;", "cannot reassign const"),
        ("const PI = 3; if true { PI = 4; }", "cannot reassign const"),
        ("const PI = 3; function change() { PI = 4; } change();", "cannot reassign const"),
        ("const PI = 3; const PI = 4;", "already declared"),
        ("if true { const local = 1; } print(local);", "undefined name"),
        ("for item in [1] {} print(item);", "undefined name"),
        ("function local() { const hidden = 1; } local(); print(hidden);", "undefined name"),
        ("print(1 / 0);", "division by zero"),
        ("print(1 % 0);", "division by zero"),
        ("print(1 + \"x\");", "not supported"),
        ("function add(a, b) { return a + b; } add(1);", "expects 2 arguments"),
        ("str(1, 2);", "str expects at most 1 argument, got 2"),
        ("len(1);", "does not accept number"),
        ("print(\"x\", end=1);", "end must be a string"),
        ("print(\"x\", unknown=\"\");", "does not accept keyword argument"),
        ("\"text\".size(1);", "expects 0 arguments"),
        ("[1].unknown();", "has no property"),
        ("[2, \"a\"].sort();", "cannot compare"),
        ("[2, 1].sort(true);", "no positional arguments"),
        ("[2, 1].sort(order=1);", "does not accept keyword argument"),
        ("1();", "not callable"),
        ("print([1][2]);", "index out of range"),
        ("print([1][0.5]);", "index must be an integer"),
        ("print({}[\"missing\"]);", "key not found"),
        ("const config = {[]: 1};", "cannot be a dictionary key"),
        ("for item in 1 {}", "not iterable"),
        ("const text = \"hi\"; text[0] = \"a\";", "cannot assign an index"),
        ("print(1e308 * 1e308);", "non-finite"),
        ("function forever() { forever(); } forever();", "maximum evaluation depth"),
    ]);
}

#[test]
fn execution_limit() {
    let error = run_with_steps("while true {}", 20).unwrap_err();
    assert!(error.contains("step limit"), "{}", error);
}

#[test]
fn explicit_declarations() {
    expect_outputs(&[
        ("var count = 0; function next() { count = count + 1; } next(); print(count);", "1\n"),
        ("const value = 1; if true { var value = 2; value = 3; print(value); } print(value);", "3\n1\n"),
        ("const values = [1]; values[0] = 2; print(values);", "[2]\n"),
        ("/* multiline\ncomment */ const text = \"# // /* */\"; print(text); // end", "# // /* */\n"),
    ]);
    for (source, message) in [
        ("missing = 1;", "declare it with const or var"),
        ("function create() { missing = 1; } create();", "declare it with const or var"),
        ("var value = 1; var value = 2;", "already declared"),
        ("var value = 1; const value = 2;", "already declared"),
        ("const value = 1; value = 2;", "cannot reassign const"),
        ("def old() {}", "expected"),
        ("# old comment", "unexpected character"),
    ] {
        let error = common::failure(source);
        assert!(error.contains(message), "{}: expected {:?}, got {}", source, message, error);
    }
}

#[test]
fn extended_control_flow() {
    expect_outputs(&[
        ("var count = 1; print(count++, ++count, count--, --count, count);", "1 3 3 1 1\n"),
        (
            "var calls = 0; function index() { calls++; return 0; } const items = [4]; print(items[index()]++, ++items[index()], calls, items);",
            "4 6 2 [6]\n",
        ),
        ("const values = {\"n\": 2}; print(--values[\"n\"], values[\"n\"]--, values[\"n\"]);", "1 1 0\n"),
        (
            "var total = 0; for (var index = 0; index < 5; ++index) { if index == 2 { continue; } total = total + index; } else { print(total); }",
            "8\n",
        ),
        (
            "for (;;) { break; } else { print(\"wrong\"); } while false {} else { print(\"empty\"); } for item in [] {} else { print(\"done\"); }",
            "empty\ndone\n",
        ),
        (
            "for item in [1, 2] { if item == 1 { continue; } break; } else { print(\"wrong\"); } print(\"done\");",
            "done\n",
        ),
        (
            "switch 2 { case 1: print(\"wrong\"); break; case 2: print(\"two\"); case 3: print(\"three\"); break; default: print(\"wrong\"); }",
            "two\nthree\n",
        ),
        (
            "switch 9 { case 1: break; default: print(\"fallback\"); case 2: print(\"fallthrough\"); }",
            "fallback\nfallthrough\n",
        ),
        (
            "match 9 { case 1: break; case _: print(\"fallback\"); case 2: print(\"fallthrough\"); }",
            "fallback\nfallthrough\n",
        ),
        (
            "for item in [1, 2, 3] { switch item { case 1: continue; case 2: break; default: print(\"three\"); } print(item); } else { print(\"done\"); }",
            "2\nthree\n3\ndone\n",
        ),
        ("function find() { for (;;) { switch 1 { case 1: return 7; } } else { return 0; } } print(find());", "7\n"),
        (
            "function grade(score, attendance, approved) { if (score >= 90 && attendance >= 90) { return \"A\"; } else if ((score >= 80 && attendance >= 80) || approved) { return \"pass\"; } else { return \"fail\"; } } print(grade(95, 92, false), grade(85, 81, false), grade(50, 50, true), grade(85, 70, false));",
            "A pass pass fail\n",
        ),
        ("if (false) { print(\"no\"); } print(\"after\"); if 1 {} else { print(\"no\"); }", "after\n"),
        (
            "var calls = 0; function hit() { calls++; return true; } if (false && hit()) {} if true || hit() {} print(calls);",
            "0\n",
        ),
        (
            "for (var i = 0; i < 10 && i * i < 20; i++) { print(i, end=\" \"); } for var j = 0; j > 5 || j < 2; j++ { print(j, end=\" \"); } print();",
            "0 1 2 3 4 0 1 \n",
        ),
        ("if (not (1 > 2) && \"x\" in \"xy\") { print(\"ok\"); }", "ok\n"),
        ("if (!!true && !false) { print(\"ok\"); } print(true && false, true || false, !true, not false);", "ok\nfalse true false true\n"),
    ]);
    for source in [
        "const count = 1; count++;",
        "var text = \"one\"; ++text;",
        "missing++;",
        "for (var index = 0; index < 1; ++index) {} print(index);",
        "if true {} celse {}",
        "celse {}",
        "cif (true) {} celse cif (false) {} celse {}",
        "cfor (;;) {}",
        "match 1 { default: break; case _: break; }",
        "for (int index = 0; index < 2; index++) {}",
        "for (index) in [1] {}",
        "switch 1 { case _: break; case _: break; }",
    ] {
        assert!(run(source).is_err(), "expected an error: {}", source);
    }
}

/// Every construct accepts a C-style and a Python-style spelling, and both
/// spellings are the same program.
#[test]
fn both_control_flow_spellings_agree() {
    let pairs = [
        (
            "const a = false; const b = true; if (a) { print(1); } else if (b) { print(2); } else { print(3); }",
            "const a = false; const b = true; if a { print(1); } elif b { print(2); } else { print(3); }",
        ),
        ("for (var index = 0; index < 3; index++) { print(index); }", "for var index = 0; index < 3; index++ { print(index); }"),
        (
            "switch 2 { case 1: print(\"one\"); default: print(\"other\"); }",
            "match 2 { case 1: print(\"one\"); case _: print(\"other\"); }",
        ),
        ("print(1 && 2 || 0, !0, !!1);", "print(1 and 2 or 0, not 0, not not 1);"),
    ];
    for (c_style, python) in pairs {
        let left = run(c_style).unwrap_or_else(|error| panic!("{}: {}", c_style, error));
        let right = run(python).unwrap_or_else(|error| panic!("{}: {}", python, error));
        assert_eq!(left, right, "{}\n{}", c_style, python);
    }
}

#[test]
fn range_and_exceptions() {
    expect_outputs(&[
        (
            "print(range(4), range(2, 5), range(5, -1, -2), range(0), range(5, 1), range(1, 5, -1));",
            "[0, 1, 2, 3] [2, 3, 4] [5, 3, 1] [] [] []\n",
        ),
        ("var total = 0; for value in range(5) { total = total + value; } else { print(total); }", "10\n"),
        (
            "function reject() { throw {\"code\": 7}; } try { reject(); } catch (error) { print(error[\"code\"]); } print(\"resumed\");",
            "7\nresumed\n",
        ),
        (
            "try { try { throw null; } catch (inner) { print(inner); throw \"again\"; } } catch (outer) { print(outer); }",
            "null\nagain\n",
        ),
        ("try { print(1 / 0); } catch (error) { print(\"division by zero\" in error); }", "true\n"),
        ("function result() { try { return 3; } catch (error) { return 0; } } print(result());", "3\n"),
        ("function result() { try { throw false; } catch (error) { return error; } } print(result());", "false\n"),
        (
            "for value in range(3) { try { if value == 1 { continue; } if value == 2 { break; } print(value); } catch (error) { print(\"wrong\"); } } else { print(\"wrong\"); }",
            "0\n",
        ),
        (
            "var count = 0; while count < 3 { try { throw count++; } catch (error) { if error == 1 { continue; } print(error); } } else { print(\"done\"); }",
            "0\n2\ndone\n",
        ),
        ("try { missing = 1; } catch (error) { print(\"const or var\" in error); }", "true\n"),
    ]);
    for source in [
        "range();",
        "range(1, 2, 3, 4);",
        "range(1.5);",
        "range(\"3\");",
        "range(0, 3, 0);",
        "range(1e20);",
        "range(1000001);",
        "throw \"unhandled\";",
        "try { throw 1; } catch (error) {} print(error);",
    ] {
        assert!(run(source).is_err(), "expected an error: {}", source);
    }
    // The step limit is not catchable.
    let error = run_with_steps("try { for (;;) {} } catch (error) { print(\"cannot catch limits\"); }", 20).unwrap_err();
    assert!(error.contains("step limit"), "{}", error);
}

#[test]
fn formatted_strings() {
    expect_outputs(&[
        ("const name = \"Tiger\"; print(f\"Hello, {name}! {2 + 3}\");", "Hello, Tiger! 5\n"),
        ("print(F'{{braces}} {true} {null} {range(3)}');", "{braces} true null [0, 1, 2]\n"),
        ("var count = 0; print(f\"{count++}:{++count}:{count}\");", "0:2:2\n"),
        ("const data = {\"value\": 7}; print(f\"{data[\"value\"]} {{ { {\"key\": 2}[\"key\"] } }}\");", "7 { 2 }\n"),
        ("print(f\"outer {f'inner {1 + 2}'}\", f\"\", f'line\\nnext');", "outer inner 3  line\nnext\n"),
        ("function message(value) { return f\"value={value}\"; } const result = message(9); print(result + \"!\");", "value=9!\n"),
        ("class Label { var name = \"item\"; function text() { return f\"{this.name}\"; } } print(Label().text());", "item\n"),
        ("try { print(f\"{missing}\"); } catch (error) { print(\"undefined name\" in error); }", "true\n"),
        ("print(f\"# // /* {1 /* } ignored */ + 2} */\");", "# // /* 3 */\n"),
    ]);
    for source in
        ["print(f\"{}\");", "print(f\"}\");", "print(f\"{1\");", "print(f\"unterminated);", "print(f\"{1; 2}\");", "print(f\"{1!r}\");"]
    {
        assert!(run(source).is_err(), "expected an error: {}", source);
    }
    let error = run("\nprint(f\"value {missing}\");").unwrap_err();
    assert!(error.contains("2:16:"), "{}", error);
}

#[test]
fn variadic_arguments() {
    expect_outputs(&[
        (
            "function collect(first, *rest, **options) { return str(first) + \" \" + str(rest) + \" \" + str(options); } print(collect(1, 2, 3, name=\"a\"));",
            "1 [2, 3] {\"name\": \"a\"}\n",
        ),
        ("function options(**named) { return len(named); } print(options(a=1, b=2), options());", "2 0\n"),
        ("function point(x, y) { return x * 10 + y; } print(point(y=2, x=1), point(3, 4));", "12 34\n"),
        ("function add3(a, b, c) { return a + b + c; } const values = [1, 2, 3]; print(add3(*values));", "6\n"),
        (
            "function label(text, end) { return text + end; } const config = {\"text\": \"a\", \"end\": \"b\"}; print(label(**config));",
            "ab\n",
        ),
        (
            "function inner(first, *rest, **opts) { return str(first) + str(rest) + str(opts); } function outer(*args, **kwargs) { return inner(0, *args, **kwargs); } print(outer(1, 2, flag=true));",
            "0[1, 2]{\"flag\": true}\n",
        ),
        (
            "class Counter { var value = 0; function add(*amounts) { for amount in amounts { this.value = this.value + amount; } return this.value; } } print(Counter().add(1, 2, 3));",
            "6\n",
        ),
        ("class Point { var coords; Point(*coords) { this.coords = coords; } } print(Point(1, 2, 3).coords);", "[1, 2, 3]\n"),
        (
            "function keys(**named) { var result = []; for key in named { result = result + [key]; } return result; } print(keys(one=1, two=2, three=3));",
            "[\"one\", \"two\", \"three\"]\n",
        ),
        ("function grow(*rest) { rest = rest + [9]; return rest; } print(grow(1), grow());", "[1, 9] [9]\n"),
        (
            "function mix(a, *rest, **named) { return [a, rest, named]; } print(mix(1, *[2, 3], **{\"k\": 4}));",
            "[1, [2, 3], {\"k\": 4}]\n",
        ),
        (
            "class Base { var values; Base(*values) { this.values = values; } } class Child extends Base { var first; Child(first, *rest) { super(*rest); this.first = first; } } const child = Child(1, 2, 3); print(child.first, child.values);",
            "1 [2, 3]\n",
        ),
    ]);
    for (source, message) in [
        ("function f(**options) {} f(1);", "expects 0 arguments"),
        ("function f(a) {} f();", "expects 1 arguments"),
        ("function f(a) {} f(1, 2);", "expects 1 arguments"),
        ("class Item {} Item(key=\"value\");", "expects 0 arguments"),
        ("function f(a, *rest, **options) {} f();", "missing required argument"),
        ("function f(a) {} f(bad=1);", "unexpected keyword argument"),
        ("function f(a) {} f(1, a=2);", "multiple values for argument"),
        ("function f(**options) {} f(a=1, **{\"a\": 2});", "duplicate keyword argument"),
        ("function f(*rest) {} f(*1);", "cannot unpack number"),
        ("function f(**options) {} f(**1);", "cannot unpack number"),
        ("function f(**options) {} f(**{1: 2});", "keyword argument names must be strings"),
    ] {
        let error = common::failure(source);
        assert!(error.contains(message), "{}: expected {:?}, got {}", source, message, error);
    }
}

#[test]
fn output_errors_are_reported() {
    let error = run_failing_output("print(\"hello\");").unwrap_err();
    assert!(error.contains("output closed"), "{}", error);
}

/// A parameter that nothing outside the frame can reach binds straight into the
/// frame's own locals, and one a nested function reads stays in the environment
/// where the closure will look for it.
#[test]
fn flat_parameters_and_frames() {
    expect_outputs(&[
        (
            "function add(a, b) { const total = a + b; return total; }\n\
             print(add(2, 3), add(-1, 1));",
            "5 0\n",
        ),
        (
            "function counter(start) { function next() { return start; } return next; }\n\
             print(counter(7)());",
            "7\n",
        ),
        (
            "function outer(seed) { function inner(step) { return seed + step; } return inner; }\n\
             print(outer(10)(5));",
            "15\n",
        ),
        (
            "function fib(n) { if n < 2 { return n; } return fib(n - 1) + fib(n - 2); }\n\
             print(fib(10));",
            "55\n",
        ),
    ]);
}
