//! Conversions: `int`, `float`, `str`, `bool`, the `To*` methods, and truthiness.
//!
//! A port of `pkg/evaluator/convert_test.go`.

mod common;

use common::{expect_errors, expect_outputs, output};

#[test]
fn conversions() {
    expect_outputs(&[
        (
            "print(int(3.9), int(-3.9), int(-0.5), int(\"42\"), int(\"  -7 \"), int(\"+8\"), int(true), int(false), int());",
            "3 -3 0 42 -7 8 1 0 0\n",
        ),
        (
            "print(float(3), float(\"2.5\"), float(\" -.5 \"), float(\"1e3\"), float(\"7\"), float(\"4.\"), float(true), float());",
            "3 2.5 -0.5 1000 7 4 1 0\n",
        ),
        (
            "class Box {} print(str(1.5) + str(true) + str(null), str([1, \"a\"]), str({\"k\": 2}), str(Box()), str() == \"\");",
            "1.5truenull [1, \"a\"] {\"k\": 2} <Box instance> true\n",
        ),
        (
            "print(bool(0), bool(0.1), bool(\"\"), bool(\" \"), bool(null), bool([]), bool([0]), bool({}), bool({\"a\": 1}), bool(false), bool());",
            "false true false true false false true false true false false\n",
        ),
        (
            "class Box {} function f() {} print(bool(Box()), bool(f), bool(print), bool(Box));",
            "true true true true\n",
        ),
        ("print(int(\"2\") + 0.5, float(\"1.5\") * int(4.9), 7 / int(\"2\"));", "2.5 6 3.5\n"),
        (
            "for value in [0, 1, \"\", \"x\", null, [], [0], {}] { if value { print(\"T\", end=\"\"); } else { print(\"F\", end=\"\"); } } print();",
            "FTFTFFTF\n",
        ),
        (
            "var items = [1, 2, 3]; var count = 0; while items { items = []; count++; } print(count);",
            "1\n",
        ),
        (
            "print(42.ToString() + \"!\", \"12\".ToInt() + 1, \"2.5\".ToFloat() * 2, 0.ToBool(), \"\".ToBool(), \"no\".ToBool());",
            "42! 13 5 false false true\n",
        ),
        (
            "const n = 9.75; const flag = true; print(n.ToInt(), flag.ToInt(), flag.ToString(), null.ToString(), null.ToBool(), [1].ToString(), {\"a\": 1}.ToBool(), [].ToBool());",
            "9 1 true null false [1] true false\n",
        ),
        (
            "class Point { var x = 1; } const p = Point(); print(p.ToString(), p.ToBool());",
            "<Point instance> true\n",
        ),
        (
            "class Money { var cents = 250; function ToString() { return \"$\" + str(this.cents / 100); } } print(Money().ToString(), str(Money()));",
            "$2.5 <Money instance>\n",
        ),
        ("const convert = \"7\".ToInt; print(convert() * 2);", "14\n"),
        ("print(int(str(3) + str(4)) + 1);", "35\n"),
    ]);
}

#[test]
fn conversion_errors() {
    expect_errors(&[
        ("int(\"3.5\");", "int: invalid integer literal \"3.5\""),
        ("int(\"abc\");", "int: invalid integer literal \"abc\""),
        ("int(\"\");", "int: invalid integer literal \"\""),
        ("int(\"1e3\");", "int: invalid integer literal \"1e3\""),
        ("int([1]);", "int cannot convert list"),
        ("int(null);", "int cannot convert null"),
        ("float(\"abc\");", "float: invalid number literal \"abc\""),
        ("float(\"inf\");", "float: invalid number literal \"inf\""),
        ("float(\"nan\");", "float: invalid number literal \"nan\""),
        ("float(\"0x10\");", "float: invalid number literal \"0x10\""),
        ("float(\"1e400\");", "float: 1e400 is out of range"),
        ("float({});", "float cannot convert dict"),
        ("int(1, 2);", "int expects at most 1 argument, got 2"),
        ("bool(value=1);", "bool does not accept keyword arguments"),
        ("\"x\".ToInt();", "ToInt: invalid integer literal \"x\""),
        ("[1].ToFloat();", "ToFloat cannot convert list"),
        ("1.ToString(2);", "ToString expects 0 arguments"),
        ("1.ToNumber();", "number has no properties"),
        ("class Box {} Box().ToList();", "Box has no property \"ToList\""),
    ]);
    assert_eq!(output("try { int(\"x\"); } catch (error) { print(\"caught\", \"invalid integer\" in error); }"), "caught true\n");
}
