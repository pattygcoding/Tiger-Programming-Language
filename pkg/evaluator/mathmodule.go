package evaluator

import (
	"fmt"
	"math"
	"tiger/pkg/object"
)

var unaryMath = map[string]func(float64) (float64, error){
	"ceil":  func(x float64) (float64, error) { return math.Ceil(x), nil },
	"floor": func(x float64) (float64, error) { return math.Floor(x), nil },
	// Python's round: halves go to the nearest even integer.
	"round": func(x float64) (float64, error) { return math.RoundToEven(x), nil },
	"abs":   func(x float64) (float64, error) { return math.Abs(x), nil },
	"sin":   func(x float64) (float64, error) { return math.Sin(x), nil },
	"cos":   func(x float64) (float64, error) { return math.Cos(x), nil },
	"tan":   func(x float64) (float64, error) { return math.Tan(x), nil },
	"sqrt": func(x float64) (float64, error) {
		if x < 0 {
			return 0, errMathDomain
		}
		return math.Sqrt(x), nil
	},
	"log": func(x float64) (float64, error) {
		if x <= 0 {
			return 0, errMathDomain
		}
		return math.Log(x), nil
	},
}

var binaryMath = map[string]func(float64, float64) (float64, error){
	"pow": func(x, y float64) (float64, error) {
		if x == 0 && y < 0 {
			return 0, errMathDomain
		}
		result := math.Pow(x, y)
		if math.IsNaN(result) {
			return 0, errMathDomain
		}
		if math.IsInf(result, 0) {
			return 0, fmt.Errorf("math range error")
		}
		return result, nil
	},
	"min": func(a, b float64) (float64, error) { return math.Min(a, b), nil },
	"max": func(a, b float64) (float64, error) { return math.Max(a, b), nil },
}

var errMathDomain = fmt.Errorf("math domain error")

func mathModule() *object.Module {
	env := object.NewEnvironment(nil)
	for name, function := range unaryMath {
		function := function
		_ = env.Define(name, mathBuiltin(name, 1, func(args []float64) (float64, error) { return function(args[0]) }), true)
	}
	for name, function := range binaryMath {
		function := function
		_ = env.Define(name, mathBuiltin(name, 2, func(args []float64) (float64, error) { return function(args[0], args[1]) }), true)
	}
	return &object.Module{Name: "math", Env: env}
}

func mathBuiltin(name string, arity int, function func([]float64) (float64, error)) *object.Builtin {
	qualified := "math." + name
	return &object.Builtin{Name: qualified, Call: func(args []object.Value, keywords map[string]object.Value) (object.Value, error) {
		if len(keywords) != 0 {
			return nil, fmt.Errorf("%s does not accept keyword arguments", qualified)
		}
		if len(args) != arity {
			return nil, fmt.Errorf("%s expects %d argument%s, got %d", qualified, arity, map[bool]string{true: "s"}[arity != 1], len(args))
		}
		numbers := make([]float64, arity)
		for index, arg := range args {
			number, ok := arg.(object.Number)
			if !ok {
				return nil, fmt.Errorf("%s expects a number, got %s", qualified, arg.Type())
			}
			numbers[index] = float64(number)
		}
		result, err := function(numbers)
		if err != nil {
			return nil, fmt.Errorf("%s: %w", qualified, err)
		}
		return object.Number(result + 0), nil
	}}
}
