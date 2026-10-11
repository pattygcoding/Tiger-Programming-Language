package evaluator

import (
	"math"

	"tiger/pkg/format"
	"tiger/pkg/object"
)

// The time module is shared by every target; the platform files supply the
// clock. Native and WASI builds use the host clock, while the browser build
// returns a clear error because it has no clock in this design.
func timeModule() *object.Module {
	env := object.NewEnvironment(nil)
	define := func(name string, call func([]object.Value, map[string]object.Value) (object.Value, error)) {
		_ = env.Define(name, &object.Builtin{Name: "time." + name, Call: call}, true)
	}

	define("now", func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		if err := noArguments("time.now", arguments, keywords); err != nil {
			return nil, err
		}
		seconds, err := timeNow()
		if err != nil {
			return nil, err
		}
		return object.Number(seconds), nil
	})

	define("now_ms", func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		if err := noArguments("time.now_ms", arguments, keywords); err != nil {
			return nil, err
		}
		millis, err := timeNowMillis()
		if err != nil {
			return nil, err
		}
		return object.Number(millis), nil
	})

	define("sleep", func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		seconds, err := timestampArgument("time.sleep", arguments, keywords)
		if err != nil {
			return nil, err
		}
		if seconds < 0 {
			return nil, format.Errorf("time.sleep expects a non-negative number of seconds")
		}
		if err := timeSleep(seconds); err != nil {
			return nil, err
		}
		return object.Null{}, nil
	})

	define("iso", func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		seconds, err := timestampArgument("time.iso", arguments, keywords)
		if err != nil {
			return nil, err
		}
		text, err := timeISO(seconds)
		if err != nil {
			return nil, err
		}
		return object.String(text), nil
	})

	define("parts", func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		seconds, err := timestampArgument("time.parts", arguments, keywords)
		if err != nil {
			return nil, err
		}
		return timeParts(seconds)
	})

	return &object.Module{Name: "time", Env: env}
}

func timestampArgument(name string, arguments []object.Value, keywords map[string]object.Value) (float64, error) {
	if len(keywords) != 0 {
		return 0, format.Errorf("%s does not accept keyword arguments", name)
	}
	if len(arguments) != 1 {
		return 0, format.Errorf("%s expects 1 argument, got %d", name, len(arguments))
	}
	number, ok := arguments[0].(object.Number)
	if !ok || math.IsNaN(float64(number)) || math.IsInf(float64(number), 0) {
		return 0, format.Errorf("%s expects a number of seconds, got %s", name, arguments[0].Type())
	}
	return float64(number), nil
}
