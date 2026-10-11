package evaluator

import (
	"math"

	"tiger/pkg/format"
	"tiger/pkg/object"
)

// The random module is a xorshift64* generator. It needs no clock and only
// integer math, so it stays inside the browser Wasm build. Seeding is explicit
// and the default seed is fixed, so an unseeded program is reproducible.

const defaultRandomSeed uint64 = 0x2545F4914F6CDD1D

type randomSource struct{ state uint64 }

func (source *randomSource) next() uint64 {
	state := source.state
	state ^= state >> 12
	state ^= state << 25
	state ^= state >> 27
	source.state = state
	return state * 2685821657736338717
}

// fraction returns a value in [0, 1) using the top 53 bits, which float64 holds
// exactly.
func (source *randomSource) fraction() float64 {
	return float64(source.next()>>11) / float64(uint64(1)<<53)
}

func randomModule() *object.Module {
	source := &randomSource{state: defaultRandomSeed}
	env := object.NewEnvironment(nil)
	define := func(name string, call func([]object.Value, map[string]object.Value) (object.Value, error)) {
		_ = env.Define(name, &object.Builtin{Name: "random." + name, Call: call}, true)
	}

	define("seed", func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		if len(keywords) != 0 {
			return nil, format.Errorf("random.seed does not accept keyword arguments")
		}
		if len(arguments) != 1 {
			return nil, format.Errorf("random.seed expects 1 argument, got %d", len(arguments))
		}
		number, ok := arguments[0].(object.Number)
		if !ok || math.IsNaN(float64(number)) || math.IsInf(float64(number), 0) {
			return nil, format.Errorf("random.seed expects a number, got %s", arguments[0].Type())
		}
		seed := uint64(int64(number))
		if seed == 0 {
			seed = defaultRandomSeed
		}
		source.state = seed
		return object.Null{}, nil
	})

	define("random", func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		if err := noArguments("random.random", arguments, keywords); err != nil {
			return nil, err
		}
		return object.Number(source.fraction()), nil
	})

	define("int", func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		low, high, err := randomRange("random.int", arguments, keywords)
		if err != nil {
			return nil, err
		}
		width := uint64(high-low) + 1
		return object.Number(float64(low + int64(source.next()%width))), nil
	})

	define("uniform", func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		low, high, err := randomFloatRange("random.uniform", arguments, keywords)
		if err != nil {
			return nil, err
		}
		return object.Number(low + source.fraction()*(high-low)), nil
	})

	define("choice", func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		if len(keywords) != 0 {
			return nil, format.Errorf("random.choice does not accept keyword arguments")
		}
		if len(arguments) != 1 {
			return nil, format.Errorf("random.choice expects 1 argument, got %d", len(arguments))
		}
		list, ok := arguments[0].(*object.List)
		if !ok {
			return nil, format.Errorf("random.choice expects a list, got %s", arguments[0].Type())
		}
		if len(list.Elements) == 0 {
			return nil, format.Errorf("random.choice cannot choose from an empty list")
		}
		return list.Elements[source.next()%uint64(len(list.Elements))], nil
	})

	define("shuffle", func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		if len(keywords) != 0 {
			return nil, format.Errorf("random.shuffle does not accept keyword arguments")
		}
		if len(arguments) != 1 {
			return nil, format.Errorf("random.shuffle expects 1 argument, got %d", len(arguments))
		}
		list, ok := arguments[0].(*object.List)
		if !ok {
			return nil, format.Errorf("random.shuffle expects a list, got %s", arguments[0].Type())
		}
		for index := len(list.Elements) - 1; index > 0; index-- {
			swap := int(source.next() % uint64(index+1))
			list.Elements[index], list.Elements[swap] = list.Elements[swap], list.Elements[index]
		}
		return object.Null{}, nil
	})

	return &object.Module{Name: "random", Env: env}
}

func randomRange(name string, arguments []object.Value, keywords map[string]object.Value) (int64, int64, error) {
	if len(keywords) != 0 {
		return 0, 0, format.Errorf("%s does not accept keyword arguments", name)
	}
	if len(arguments) != 2 {
		return 0, 0, format.Errorf("%s expects 2 arguments, got %d", name, len(arguments))
	}
	low, err := randomInteger(name, "low", arguments[0])
	if err != nil {
		return 0, 0, err
	}
	high, err := randomInteger(name, "high", arguments[1])
	if err != nil {
		return 0, 0, err
	}
	if high < low {
		return 0, 0, format.Errorf("%s expects low <= high, got %d and %d", name, low, high)
	}
	return low, high, nil
}

func randomFloatRange(name string, arguments []object.Value, keywords map[string]object.Value) (float64, float64, error) {
	if len(keywords) != 0 {
		return 0, 0, format.Errorf("%s does not accept keyword arguments", name)
	}
	if len(arguments) != 2 {
		return 0, 0, format.Errorf("%s expects 2 arguments, got %d", name, len(arguments))
	}
	low, err := randomNumber(name, "low", arguments[0])
	if err != nil {
		return 0, 0, err
	}
	high, err := randomNumber(name, "high", arguments[1])
	if err != nil {
		return 0, 0, err
	}
	if high < low {
		return 0, 0, format.Errorf("%s expects low <= high, got %v and %v", name, low, high)
	}
	return low, high, nil
}

func randomInteger(name, role string, value object.Value) (int64, error) {
	number, err := randomNumber(name, role, value)
	if err != nil {
		return 0, err
	}
	if math.Trunc(number) != number || math.Abs(number) > 9007199254740991 {
		return 0, format.Errorf("%s %s must be a safe integer", name, role)
	}
	return int64(number), nil
}

func randomNumber(name, role string, value object.Value) (float64, error) {
	number, ok := value.(object.Number)
	if !ok || math.IsNaN(float64(number)) || math.IsInf(float64(number), 0) {
		return 0, format.Errorf("%s %s must be a number, got %s", name, role, value.Type())
	}
	return float64(number), nil
}
