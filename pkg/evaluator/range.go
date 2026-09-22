package evaluator

import (
	"fmt"
	"math"
	"tiger/pkg/object"
)

func rangeValues(arguments []object.Value) (object.Value, error) {
	if len(arguments) < 1 || len(arguments) > 3 {
		return nil, fmt.Errorf("range expects 1 to 3 arguments, got %d", len(arguments))
	}
	values := make([]int64, len(arguments))
	for index, argument := range arguments {
		number, ok := argument.(object.Number)
		if !ok || math.IsNaN(float64(number)) || math.IsInf(float64(number), 0) || math.Trunc(float64(number)) != float64(number) || math.Abs(float64(number)) > 9007199254740991 {
			return nil, fmt.Errorf("range arguments must be safe integers (absolute value <= 9007199254740991)")
		}
		values[index] = int64(number)
	}
	start, stop, step := int64(0), values[0], int64(1)
	if len(values) >= 2 {
		start, stop = values[0], values[1]
	}
	if len(values) == 3 {
		step = values[2]
	}
	if step == 0 {
		return nil, fmt.Errorf("range step cannot be zero")
	}
	count := int64(0)
	if step > 0 && start < stop {
		count = (stop-start-1)/step + 1
	}
	if step < 0 && start > stop {
		count = (start-stop-1)/(-step) + 1
	}
	if count > 1_000_000 {
		return nil, fmt.Errorf("range exceeds 1000000 elements")
	}
	list := &object.List{Elements: make([]object.Value, int(count))}
	for index := range list.Elements {
		list.Elements[index] = object.Number(start + int64(index)*step)
	}
	return list, nil
}
