//! Call frames: the shapes a call has to leave the engine's state in.
//!
//! A plain function call does not re-enter the instruction loop. It parks the running
//! frame inside the loop, loads the callee's state into the loop's own variables, and
//! carries on, and `return` puts back what the call parked. A frame's own slots are the
//! bottom of its region on the one shared value stack, with its operands above them —
//! so the arguments a call already pushed are the slots its parameters own, and a
//! return drops back to the slot below the frame rather than handing a buffer back.
//! What is pinned here is everything that has to stay true across that exchange: the
//! depth a recursive call still counts toward, the operand stack the caller's pending
//! values are resting on, the caller's slots surviving every call it makes, the scopes
//! and iterators a frame unwinds and no more, the `Pop` a discarded call leaves behind
//! it, and the frame a `return` inside a `try` leaves.

mod common;

use common::{failure, output};

#[test]
fn a_call_runs_in_a_frame_of_its_own() {
    // A recursive call is 400 frames deep and returns, which is what a parked frame
    // has to survive: the value has to come back through every one of them.
    assert_eq!(
        output("function down(n) {\n    if n == 0 {\n        return 0;\n    }\n    return down(n - 1);\n}\nprint(down(400));"),
        "0\n"
    );
    // And the depth is still counted, so the documented limit is reached where it
    // always was rather than by overflowing the host's own stack.
    let error = failure("function down(n) {\n    if n == 0 {\n        return 0;\n    }\n    return down(n - 1);\n}\nprint(down(600));");
    assert!(error.contains("maximum evaluation depth exceeded"), "{error}");
}

#[test]
fn a_discarded_call_leaves_the_stack_as_the_next_instruction_expects() {
    // A call written as a statement has its `Pop` fused into the call, and the
    // instruction it replaced is still in the code for the loop to step over. A frame
    // that is parked instead of entered has to step over it too, or the next
    // instruction reads a value the discarded call should have dropped.
    assert_eq!(
        output(
            "function mark(value) {\n    return value;\n}\nvar total = 0;\nfor (var i = 0; i < 3; ++i) {\n    mark(i);\n    total += mark(i) + 1;\n}\nprint(total);"
        ),
        "6\n"
    );
}

#[test]
fn a_result_lands_where_the_call_left_room_for_it() {
    // The caller's own operands stay on the stack below the call, so a result has to
    // take exactly the slot the callee occupied — no more, no less. A call in the
    // middle of an expression is where that shows.
    assert_eq!(
        output(
            "function double(value) {\n    return value * 2;\n}\nvar total = 0;\nfor (var i = 0; i < 4; ++i) {\n    total = total + double(i) + double(i + 1);\n}\nprint(total);"
        ),
        "32\n"
    );
    // Nesting puts a whole chain of results through the same slot.
    assert_eq!(output("function double(value) {\n    return value * 2;\n}\nprint(double(double(double(1))));"), "8\n");
}

#[test]
fn every_argument_lands_in_its_own_parameter() {
    // A frame's first slots are its parameters, and the arguments a call pushed are
    // already in them — the callee's region starts where those arguments were, so no
    // slice operation and no copy is involved. What that rests on is the slots being
    // `0..n`, so this pins the shapes around it: no parameters, one, several, and
    // arguments that are themselves values the caller is still holding on the operand
    // stack.
    assert_eq!(
        output(
            "function mix(a, b) {\n    return str(a) + \"/\" + str(b);\n}\nfunction none() {\n    return 7;\n}\nvar held = mix(1, 2);\nprint(mix(1, 2));\nprint(mix(\"x\", 3.5));\nprint(mix(none(), mix(3, 4)));\nprint(mix(mix(1, 2), none()));\nprint(none(), none(), held);"
        ),
        "1/2\nx/3.5\n7/3/4\n1/2/7\n7 7 1/2\n"
    );
}

#[test]
fn a_return_out_of_a_body_that_opened_scopes_leaves_them_behind() {
    // A body with a `for` in it pushes scopes and an iterator on the frame's own stacks,
    // and a `return` from inside one leaves them standing. The frame that returns unwinds
    // exactly what it pushed: the caller is holding a `for` of its own across the call,
    // and it has to still be reading the right element afterwards.
    assert_eq!(
        output(
            "function pick(value) {\n    for i in [1, 2, 3] {\n        var scaled = i * 10;\n        if i == value {\n            return scaled;\n        }\n    }\n    return -1;\n}\nvar seen = \"\";\nfor outer in [1, 2, 4] {\n    seen = seen + str(pick(outer)) + \",\";\n}\nprint(seen);"
        ),
        "10,20,-1,\n"
    );
}

#[test]
fn a_return_inside_a_try_leaves_the_frame_a_call_entered() {
    // A `return` in a try body is compiled into a separate chunk and reaches the
    // enclosing function as a signal, so it arrives at the call's frame rather than at
    // the instruction that made the call. It has to leave through that frame all the
    // same, with the `finally` run and the value intact.
    assert_eq!(
        output(
            "function pick(flag) {\n    var log = \"\";\n    try {\n        if flag {\n            return \"yes\";\n        }\n        log = \"no\";\n    } finally {\n        log = log + \"!\";\n    }\n    return log;\n}\nprint(pick(true), pick(false));"
        ),
        "yes no!\n"
    );
}

#[test]
fn an_error_that_unwinds_calls_leaves_no_frame_behind() {
    // The frames below the error are abandoned rather than returned from, so the depth
    // they counted has to come back down with them. Otherwise one caught error would
    // raise the recursion limit for the rest of the run — and the second call here,
    // which is as deep as the first, would hit it.
    assert_eq!(
        output(
            "function down(n, explode) {\n    if n == 0 {\n        if explode {\n            throw \"boom\";\n        }\n        return 0;\n    }\n    return down(n - 1, explode);\n}\ntry {\n    down(300, true);\n} catch (error) {\n    print(\"caught\", error);\n}\nprint(down(300, false));"
        ),
        "caught boom\n0\n"
    );
}

#[test]
fn a_throw_through_calls_reaches_the_handler() {
    // The same unwinding, with the error caught one function out. Every frame it
    // crosses is a parked one, and the handler still runs in the frame that owns it.
    assert_eq!(
        output(
            "function inner(n) {\n    if n == 0 {\n        throw \"deep\";\n    }\n    return inner(n - 1);\n}\nfunction outer() {\n    try {\n        inner(50);\n    } catch (error) {\n        return \"caught \" + error;\n    }\n    return \"no\";\n}\nprint(outer());"
        ),
        "caught deep\n"
    );
}

#[test]
fn a_call_inside_a_loop_is_between_iterators() {
    // A `for` loop's iterator belongs to the frame that opened it. A call made from
    // inside the loop runs above it and leaves it where it was, and a callee that opens
    // its own loop takes the inner iterator away with it when it returns.
    assert_eq!(
        output(
            "function visit(depth) {\n    if depth == 0 {\n        return 0;\n    }\n    var total = 0;\n    for step in [1, 2] {\n        total += visit(depth - 1) + 1;\n    }\n    return total;\n}\nprint(visit(3));"
        ),
        "14\n"
    );
    // The iterator the caller is holding still reads the right element afterwards.
    assert_eq!(
        output(
            "function twice(value) {\n    return value * 2;\n}\nvar seen = [];\nfor item in [1, 2, 3] {\n    seen = seen + [twice(item)];\n    seen = seen + [item];\n}\nprint(seen);"
        ),
        "[2, 1, 4, 2, 6, 3]\n"
    );
}

#[test]
fn a_callers_slots_survive_every_call_it_makes() {
    // The caller's slots stay on the stack below the callee's region, so a call made in
    // the middle of an expression — with the caller still holding several locals — must
    // not disturb them, however many frames open above it. The result has to land in
    // the slot the callee held, which is one below the callee's own slots.
    assert_eq!(
        output(
            "function inner(a) {\n    var local = a * 2;\n    return local + a;\n}\nfunction outer(x, y) {\n    var first = x + 1;\n    var second = y + 2;\n    var total = first + inner(second) + second + first;\n    return str(total) + \"/\" + str(first) + \"/\" + str(second) + \"/\" + str(inner(inner(x)));\n}\nprint(outer(1, 2));\nprint(outer(inner(2), 3));"
        ),
        "20/2/4/9\n34/7/5/54\n"
    );
}

#[test]
fn functions_that_call_each_other_keep_their_own_frames() {
    assert_eq!(
        output(
            "function is_even(n) {\n    if n == 0 {\n        return true;\n    }\n    return is_odd(n - 1);\n}\nfunction is_odd(n) {\n    if n == 0 {\n        return false;\n    }\n    return is_even(n - 1);\n}\nprint(is_even(100), is_odd(100), is_even(101));"
        ),
        "true false false\n"
    );
}

#[test]
fn a_frame_that_needs_a_scope_of_its_own_is_entered_the_same_way() {
    // A function that captured a name cannot run in the environment it was defined
    // in — the binding has to be reachable from the frame — so its call takes a scope
    // and its frame is the one the loop parks its own above.
    assert_eq!(
        output(
            "function adder(step) {\n    function add(value) {\n        return value + step;\n    }\n    return add;\n}\nconst add2 = adder(2);\nprint(add2(40));"
        ),
        "42\n"
    );
    // A call that passes keywords binds them in the same frame, which is the other
    // reason a frame needs a scope rather than flat slots.
    assert_eq!(
        output(
            "function greet(name, greeting = \"hi\") {\n    return greeting + \" \" + name;\n}\nprint(greet(\"ann\"), greet(\"bob\", greeting = \"yo\"));"
        ),
        "hi ann yo bob\n"
    );
}

#[test]
fn a_method_call_gets_a_frame_with_its_receiver_bound() {
    assert_eq!(
        output(
            "class Box {\n    var v = 1;\n    Box() {}\n    function get() {\n        return this.v;\n    }\n}\nconst box = Box();\nprint(box.get(), box.get() + 1);"
        ),
        "1 2\n"
    );
    // A method taken off its receiver is the same frame with `this` already in hand.
    assert_eq!(
        output(
            "class Box {\n    var v = 7;\n    Box() {}\n    function get() {\n        return this.v;\n    }\n}\nconst box = Box();\nconst read = box.get;\nprint(read(), box.get());"
        ),
        "7 7\n"
    );
}

#[test]
fn a_return_out_of_a_try_leaves_a_method_frame_whole() {
    // The method's frame owns a scope for its `this`, so a `return` that arrives from
    // the try's own chunk has to leave through the frame with that scope released and
    // the `finally` run — twice, with the receiver's state carried across both calls.
    assert_eq!(
        output(
            "class Box {\n    var v = 1;\n    Box() {}\n    function get(flag) {\n        try {\n            if flag {\n                return this.v;\n            }\n        } finally {\n            this.v = this.v + 10;\n        }\n        return this.v;\n    }\n}\nconst box = Box();\nprint(box.get(true), box.get(false));"
        ),
        "1 21\n"
    );
}

#[test]
fn a_call_the_frame_cannot_take_reports_as_it_always_did() {
    // The shape a parked frame is entered for is proven by the compiler, so a call of
    // the wrong arity reaches the general path instead — and reports from there, at the
    // position of the call.
    let error = failure("function f(a) {\n    return a;\n}\nprint(f(1, 2));");
    assert!(error.contains("f expects 1 arguments, got 2"), "{error}");
    assert!(error.starts_with("4:8:"), "{error}");
}

#[test]
fn a_recursive_call_enters_the_chunk_and_environment_it_is_already_in() {
    // A function calling itself runs the same chunk in the same environment, so the frame
    // it parks records neither and the return puts nothing back. What has to stay true is
    // everything that is *not* shared: the slots, the operands, and the result.
    assert_eq!(
        output("function fib(n) {\n    if n < 2 {\n        return n;\n    }\n    return fib(n - 1) + fib(n - 2);\n}\nprint(fib(15), fib(1), fib(0));"),
        "610 1 0\n"
    );
    // Two functions calling each other swap chunks on every call but share one environment.
    assert_eq!(
        output("function a(n) {\n    if n == 0 {\n        return 0;\n    }\n    return 1 + b(n - 1);\n}\nfunction b(n) {\n    if n == 0 {\n        return 0;\n    }\n    return 1 + a(n - 1);\n}\nprint(a(9), b(9));"),
        "9 9\n"
    );
}

#[test]
fn a_callee_from_another_environment_restores_the_callers_on_return() {
    // `make` returns closures that each capture their own environment, so calling one
    // from the top level swaps the environment in and a return has to swap it back. A
    // call that skipped the swap when it should not have would read `step` from the
    // wrong scope on the second call.
    assert_eq!(
        output(
            "function make(step) {\n    function add(value) {\n        return value + step;\n    }\n    return add;\n}\nconst one = make(1);\nconst ten = make(10);\nvar total = 0;\nfor (var i = 0; i < 3; ++i) {\n    total = total + one(i) + ten(i);\n}\nprint(total, one(0), ten(0), one(1));"
        ),
        "39 1 10 2\n"
    );
}

#[test]
fn a_rebound_callee_is_called_afresh_every_time() {
    // The callee is read by name and the read is remembered, so rebinding the name has to
    // reach the next call. Both bodies are plain functions, which is the shape a call
    // enters without taking the callee out of its slot.
    assert_eq!(
        output(
            "function one(x) {\n    return x + 1;\n}\nfunction two(x) {\n    return x + 2;\n}\nvar f = one;\nvar total = 0;\nfor (var i = 0; i < 4; ++i) {\n    total += f(10);\n    if i == 1 {\n        f = two;\n    }\n}\nprint(total);"
        ),
        "46\n"
    );
}

#[test]
fn the_depth_limit_is_reached_at_the_same_call_by_every_path() {
    // The flat entry tests the limit before it counts the frame and the older path counted
    // first and tested after. They have to agree on which call is the one that fails, so a
    // chain that is exactly one short of the limit runs and one more does not.
    let program = |depth: u32| {
        format!("function down(n) {{\n    if n == 0 {{\n        return 0;\n    }}\n    return down(n - 1);\n}}\nprint(down({depth}));")
    };
    assert_eq!(output(&program(500)), "0\n");
    let error = failure(&program(520));
    assert!(error.contains("maximum evaluation depth exceeded"), "{error}");
    // The failure is positioned at the call that crossed the limit: the recursive call on
    // line 5, not the one in `print` and not the function's own declaration.
    assert!(error.starts_with("5:"), "{error}");
}

#[test]
fn a_call_leaves_a_callee_that_is_not_a_function_to_the_general_path() {
    // A flat function is entered with the callee left in its slot; everything else has to
    // be taken out of it first. Calling a value that is not callable must still report at
    // the call, and must not have been mistaken for one of the shapes the entry accepts.
    let error = failure("const f = 3;\nprint(f(1));");
    assert!(error.contains("number is not callable"), "{error}");
    assert!(error.starts_with("2:"), "{error}");
    // And a callable with the wrong shape of arguments still reaches its own diagnostic.
    let error = failure("function f(a, b) {\n    return a;\n}\nprint(f(1));");
    assert!(error.contains("f expects 2 arguments, got 1"), "{error}");
    assert!(error.starts_with("4:"), "{error}");
}

#[test]
fn a_discarded_recursive_call_steps_over_the_pop_it_absorbed() {
    // The fused call that drops its value parks a resume point one past the `Pop`, and a
    // recursive call parks no chunk at all, so the resume point is all that is left to
    // get right.
    assert_eq!(
        output(
            "var calls = 0;\nfunction walk(n) {\n    calls += 1;\n    if n > 0 {\n        walk(n - 1);\n        walk(n - 1);\n    }\n    return n;\n}\nwalk(4);\nprint(calls);"
        ),
        "31\n"
    );
}
