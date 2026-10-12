//! The `math`, `random`, `json`, and `time` modules.
//!
//! A port of `pkg/evaluator/{math,random,json,time}module_test.go`.

mod common;

use common::{expect_errors, expect_outputs, output};

#[test]
fn math_module() {
    expect_outputs(&[
        ("print(math.ceil(2.1), math.ceil(-2.9), math.ceil(-0.5), math.ceil(4));", "3 -2 0 4\n"),
        ("print(math.floor(2.9), math.floor(-2.1), math.floor(7));", "2 -3 7\n"),
        (
            "print(math.round(2.5), math.round(3.5), math.round(-2.5), math.round(-0.4), math.round(2.6), math.round(1.49));",
            "2 4 -2 0 3 1\n",
        ),
        ("print(math.sqrt(16), math.sqrt(2), math.sqrt(0));", "4 1.4142135623730951 0\n"),
        ("print(math.abs(-3.5), math.abs(4), math.abs(-0));", "3.5 4 0\n"),
        ("print(math.pow(2, 10), math.pow(9, 0.5), math.pow(2, -1), math.pow(0, 0), math.pow(-8, 3));", "1024 3 0.5 1 -512\n"),
        ("print(math.log(1), math.log(2), math.round(math.log(1000) / math.log(10)));", "0 0.6931471805599453 3\n"),
        ("print(math.sin(0), math.cos(0), math.tan(0), math.sin(1));", "0 1 0 0.8414709848078965\n"),
        ("print(math.min(3, -2), math.max(3, -2), math.min(1.5, 1.5), math.max(-1, -0.5));", "-2 3 1.5 -0.5\n"),
        ("print(math);", "<module math>\n"),
        ("const f = math.sqrt; print([1, 4, 9].length(), f(81));", "3 9\n"),
        ("function hyp(a, b) { return math.sqrt(a * a + b * b); } print(hyp(3, 4));", "5\n"),
        ("const math = {\"pi\": 3}; print(math[\"pi\"]);", "3\n"),
    ]);
}

#[test]
fn math_module_errors() {
    expect_errors(&[
        ("math.sqrt(-1);", "math.sqrt: math domain error"),
        ("math.log(0);", "math.log: math domain error"),
        ("math.log(-2);", "math.log: math domain error"),
        ("math.pow(0, -1);", "math.pow: math domain error"),
        ("math.pow(-8, 0.5);", "math.pow: math domain error"),
        ("math.pow(10, 400);", "math.pow: math range error"),
        ("math.sqrt(\"4\");", "math.sqrt expects a number, got string"),
        ("math.max(1, true);", "math.max expects a number, got bool"),
        ("math.abs();", "math.abs expects 1 argument, got 0"),
        ("math.min(1);", "math.min expects 2 arguments, got 1"),
        ("math.floor(1.5, 2);", "math.floor expects 1 argument, got 2"),
        ("math.round(x=1.5);", "math.round does not accept keyword arguments"),
        ("math.pi;", "module \"math\" has no export \"pi\""),
        ("math.sqrt = 1;", "cannot assign a property of module"),
        ("math = 1;", "math"),
    ]);
}

#[test]
fn random_module() {
    expect_outputs(&[
        ("print(random.int(4, 4), random.int(-1, -1));", "4 -1\n"),
        ("var value = random.random(); print(value >= 0 and value < 1);", "true\n"),
        ("var value = random.uniform(2, 3); print(value >= 2 and value < 3);", "true\n"),
        ("const items = [\"a\", \"b\", \"c\"]; print(random.choice(items) in items);", "true\n"),
        (
            "random.seed(7); var first = [random.int(0, 1000), random.int(0, 1000)]; random.seed(7); var second = [random.int(0, 1000), random.int(0, 1000)]; print(first == second);",
            "true\n",
        ),
        (
            "random.seed(1); var items = [1, 2, 3, 4, 5]; random.shuffle(items); items.sort(); print(items);",
            "[1, 2, 3, 4, 5]\n",
        ),
    ]);
    // The default seed is fixed, so an unseeded program repeats itself.
    let once = output("print(random.int(0, 1000000), random.random());");
    let twice = output("print(random.int(0, 1000000), random.random());");
    assert_eq!(once, twice, "the default seed must be reproducible");
}

#[test]
fn random_module_errors() {
    expect_errors(&[
        ("random.int(5, 1);", "expects low <= high"),
        ("random.int(0.5, 1);", "must be a safe integer"),
        ("random.int(\"a\", 1);", "must be a number"),
        ("random.seed();", "expects 1 argument"),
        ("random.choice([]);", "empty list"),
        ("random.choice(\"x\");", "expects a list"),
        ("random.shuffle(1);", "expects a list"),
        ("random.random(1);", "expects 0 arguments"),
    ]);
}

#[test]
fn json_module() {
    expect_outputs(&[
        (
            "const value = json.parse(\"{\\\"a\\\": 1, \\\"b\\\": [true, null, \\\"x\\\"]}\"); print(value[\"a\"], value[\"b\"]);",
            "1 [true, null, \"x\"]\n",
        ),
        ("print(json.stringify(json.parse(\"{\\\"n\\\": 1.5, \\\"ok\\\": true}\")));", "{\"n\":1.5,\"ok\":true}\n"),
        ("print(json.stringify(json.parse(\"[1, 2.5, -3, 1e2]\")));", "[1,2.5,-3,100]\n"),
        ("print(json.stringify({\"k\": [1, {\"n\": null}]}));", "{\"k\":[1,{\"n\":null}]}\n"),
        ("print(json.stringify(json.parse(\"{}\")), json.stringify(json.parse(\"[]\")));", "{} []\n"),
        ("print(json.stringify(\"line\\nbreak\"));", "\"line\\nbreak\"\n"),
        ("print(json.parse(\"\\\"\\\\u0041\\\\u00e9\\\"\"));", "A\u{e9}\n"),
        ("print(json.stringify(\"😀\"));", "\"😀\"\n"),
        ("print(json.parse(\"true\"), json.parse(\"false\"), json.parse(\"null\"), json.parse(\"42\"));", "true false null 42\n"),
    ]);
}

#[test]
fn json_module_errors() {
    expect_errors(&[
        ("json.parse(\"{bad}\");", "json.parse"),
        ("json.parse(\"[1,]\");", "json.parse"),
        ("json.parse(\"1 2\");", "trailing content"),
        ("json.parse(\"\");", "unexpected end of input"),
        ("json.parse(1);", "expects a string"),
        ("json.parse();", "expects 1 argument"),
        ("json.stringify(print);", "cannot serialize"),
        ("json.stringify({1: \"x\"});", "requires string keys"),
        ("const a = [null]; a[0] = a; json.stringify(a);", "cyclic structure"),
    ]);
}

#[test]
fn time_module() {
    expect_outputs(&[
        ("print(time.iso(0));", "1970-01-01T00:00:00Z\n"),
        ("print(time.iso(1600000000));", "2020-09-13T12:26:40Z\n"),
        (
            "const p = time.parts(1600000000); print(p[\"year\"], p[\"month\"], p[\"day\"], p[\"hour\"], p[\"minute\"], p[\"second\"], p[\"weekday\"], p[\"yearday\"]);",
            "2020 9 13 12 26 40 0 257\n",
        ),
        ("print(time.now() > 1600000000, time.now_ms() > 1600000000000);", "true true\n"),
        ("time.sleep(0); print(\"slept\");", "slept\n"),
    ]);
}

#[test]
fn time_module_errors() {
    expect_errors(&[
        ("time.iso(\"x\");", "expects a number"),
        ("time.sleep(-1);", "non-negative"),
        ("time.parts();", "expects 1 argument"),
        ("time.now(1);", "expects 0 arguments"),
    ]);
}
