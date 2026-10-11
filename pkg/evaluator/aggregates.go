package evaluator

import (
	"math"

	"tiger/pkg/ast"
	"tiger/pkg/format"
	"tiger/pkg/object"
)

// aggregateBuiltins implements the reduction and reshaping built-ins that do
// not call back into user code. map, filter, and sorted need a call site and are
// dispatched by the evaluator instead.
func aggregateBuiltins() map[string]func([]object.Value, map[string]object.Value) (object.Value, error) {
	return map[string]func([]object.Value, map[string]object.Value) (object.Value, error){
		"sum":       sumBuiltin,
		"min":       minBuiltin,
		"max":       maxBuiltin,
		"any":       anyBuiltin,
		"all":       allBuiltin,
		"enumerate": enumerateBuiltin,
		"zip":       zipBuiltin,
	}
}

// orderValues returns -1, 0, or 1 for two numbers or two strings, mirroring the
// comparison rules used by sort.
func orderValues(left, right object.Value) (int, error) {
	switch first := left.(type) {
	case object.Number:
		if second, ok := right.(object.Number); ok {
			switch {
			case first < second:
				return -1, nil
			case first > second:
				return 1, nil
			}
			return 0, nil
		}
	case object.String:
		if second, ok := right.(object.String); ok {
			switch {
			case first < second:
				return -1, nil
			case first > second:
				return 1, nil
			}
			return 0, nil
		}
	}
	return 0, format.Errorf("cannot compare %s and %s", left.Type(), right.Type())
}

func sumBuiltin(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
	list, err := listArgument("sum", arguments, keywords)
	if err != nil {
		return nil, err
	}
	total := object.Number(0)
	for index, element := range list.Elements {
		number, ok := element.(object.Number)
		if !ok {
			return nil, format.Errorf("sum expects numbers, got %s at index %d", element.Type(), index)
		}
		total += number
	}
	if math.IsInf(float64(total), 0) || math.IsNaN(float64(total)) {
		return nil, format.Errorf("non-finite numeric result")
	}
	return total, nil
}

func minBuiltin(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
	return extremeValue("min", arguments, keywords, -1)
}

func maxBuiltin(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
	return extremeValue("max", arguments, keywords, 1)
}

// extremeValue picks the smallest (direction -1) or largest (direction 1) value
// from a single list or from several positional arguments.
func extremeValue(name string, arguments []object.Value, keywords map[string]object.Value, direction int) (object.Value, error) {
	values, err := sequenceArguments(name, arguments, keywords)
	if err != nil {
		return nil, err
	}
	if len(values) == 0 {
		return nil, format.Errorf("%s of an empty sequence", name)
	}
	best := values[0]
	for _, value := range values[1:] {
		order, err := orderValues(value, best)
		if err != nil {
			return nil, format.Errorf("%s: %s", name, err)
		}
		if order*direction > 0 {
			best = value
		}
	}
	return best, nil
}

func anyBuiltin(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
	list, err := listArgument("any", arguments, keywords)
	if err != nil {
		return nil, err
	}
	for _, element := range list.Elements {
		if object.Truthy(element) {
			return object.Bool(true), nil
		}
	}
	return object.Bool(false), nil
}

func allBuiltin(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
	list, err := listArgument("all", arguments, keywords)
	if err != nil {
		return nil, err
	}
	for _, element := range list.Elements {
		if !object.Truthy(element) {
			return object.Bool(false), nil
		}
	}
	return object.Bool(true), nil
}

func enumerateBuiltin(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
	list, err := listArgument("enumerate", arguments, keywords)
	if err != nil {
		return nil, err
	}
	elements := make([]object.Value, len(list.Elements))
	for index, element := range list.Elements {
		elements[index] = &object.List{Elements: []object.Value{object.Number(index), element}}
	}
	return &object.List{Elements: elements}, nil
}

// zipBuiltin pairs elements by position; the result is as short as the shortest
// argument, and each pair is a two-element list because there is no tuple type.
func zipBuiltin(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
	if len(keywords) != 0 {
		return nil, format.Errorf("zip does not accept keyword arguments")
	}
	if len(arguments) == 0 {
		return nil, format.Errorf("zip expects at least 1 argument")
	}
	lists := make([]*object.List, len(arguments))
	shortest := -1
	for index, argument := range arguments {
		list, ok := argument.(*object.List)
		if !ok {
			return nil, format.Errorf("zip expects lists, got %s at position %d", argument.Type(), index+1)
		}
		lists[index] = list
		if shortest < 0 || len(list.Elements) < shortest {
			shortest = len(list.Elements)
		}
	}
	rows := make([]object.Value, shortest)
	for position := 0; position < shortest; position++ {
		row := make([]object.Value, len(lists))
		for index, list := range lists {
			row[index] = list.Elements[position]
		}
		rows[position] = &object.List{Elements: row}
	}
	return &object.List{Elements: rows}, nil
}

// callAggregate runs the higher-order built-ins, which call back into user code
// and so need the call site (node) and environment.
func (eval *Evaluator) callAggregate(node ast.Node, name string, arguments []object.Value, keywords []keyword, env *object.Environment) (object.Value, error) {
	table := make(map[string]object.Value, len(keywords))
	for _, entry := range keywords {
		table[entry.name] = entry.value
	}
	switch name {
	case "map":
		return eval.mapBuiltin(node, arguments, table, env)
	case "filter":
		return eval.filterBuiltin(node, arguments, table, env)
	case "sorted":
		return eval.sortedBuiltin(node, arguments, table, env)
	}
	return nil, format.Errorf("unknown aggregate built-in %q", name)
}

func (eval *Evaluator) mapBuiltin(node ast.Node, arguments []object.Value, keywords map[string]object.Value, env *object.Environment) (object.Value, error) {
	list, function, err := callableListArguments("map", arguments, keywords)
	if err != nil {
		return nil, err
	}
	elements := make([]object.Value, len(list.Elements))
	for index, element := range list.Elements {
		elements[index] = eval.call(node, function, []object.Value{element}, nil, env)
	}
	return &object.List{Elements: elements}, nil
}

func (eval *Evaluator) filterBuiltin(node ast.Node, arguments []object.Value, keywords map[string]object.Value, env *object.Environment) (object.Value, error) {
	list, function, err := callableListArguments("filter", arguments, keywords)
	if err != nil {
		return nil, err
	}
	elements := []object.Value{}
	for _, element := range list.Elements {
		if object.Truthy(eval.call(node, function, []object.Value{element}, nil, env)) {
			elements = append(elements, element)
		}
	}
	return &object.List{Elements: elements}, nil
}

// sortedBuiltin mirrors list.sort's keywords but returns a new list and leaves
// the original untouched.
func (eval *Evaluator) sortedBuiltin(node ast.Node, arguments []object.Value, keywords map[string]object.Value, env *object.Environment) (object.Value, error) {
	if len(arguments) != 1 {
		return nil, format.Errorf("sorted expects 1 argument, got %d", len(arguments))
	}
	list, ok := arguments[0].(*object.List)
	if !ok {
		return nil, format.Errorf("sorted expects a list, got %s", arguments[0].Type())
	}
	var key object.Value = object.Null{}
	reverse := false
	for name, value := range keywords {
		switch name {
		case "key":
			key = value
		case "reverse":
			switch flag := value.(type) {
			case object.Bool:
				reverse = bool(flag)
			case object.Number:
				if math.Trunc(float64(flag)) != float64(flag) {
					return nil, format.Errorf("sorted reverse must be a boolean or integer")
				}
				reverse = flag != 0
			default:
				return nil, format.Errorf("sorted reverse must be a boolean or integer, got %s", value.Type())
			}
		default:
			return nil, format.Errorf("sorted does not accept keyword argument %q", name)
		}
	}
	items := append([]object.Value{}, list.Elements...)
	keys := items
	if _, none := key.(object.Null); !none {
		keys = make([]object.Value, len(items))
		for index, item := range items {
			keys[index] = eval.call(node, key, []object.Value{item}, nil, env)
		}
	}
	order := make([]int, len(items))
	for index := range order {
		order[index] = index
	}
	stableSort(order, func(left, right int) bool {
		first, second := keys[left], keys[right]
		if reverse {
			first, second = second, first
		}
		comparison, err := orderValues(first, second)
		if err != nil {
			fail(node, "%s", err)
		}
		return comparison < 0
	})
	sorted := make([]object.Value, len(items))
	for index, position := range order {
		sorted[index] = items[position]
	}
	return &object.List{Elements: sorted}, nil
}

func listArgument(name string, arguments []object.Value, keywords map[string]object.Value) (*object.List, error) {
	if len(keywords) != 0 {
		return nil, format.Errorf("%s does not accept keyword arguments", name)
	}
	if len(arguments) != 1 {
		return nil, format.Errorf("%s expects 1 argument, got %d", name, len(arguments))
	}
	list, ok := arguments[0].(*object.List)
	if !ok {
		return nil, format.Errorf("%s expects a list, got %s", name, arguments[0].Type())
	}
	return list, nil
}

func sequenceArguments(name string, arguments []object.Value, keywords map[string]object.Value) ([]object.Value, error) {
	if len(keywords) != 0 {
		return nil, format.Errorf("%s does not accept keyword arguments", name)
	}
	if len(arguments) == 0 {
		return nil, format.Errorf("%s expects at least 1 argument", name)
	}
	if len(arguments) == 1 {
		if list, ok := arguments[0].(*object.List); ok {
			return list.Elements, nil
		}
	}
	return arguments, nil
}

func callableListArguments(name string, arguments []object.Value, keywords map[string]object.Value) (*object.List, object.Value, error) {
	if len(keywords) != 0 {
		return nil, nil, format.Errorf("%s does not accept keyword arguments", name)
	}
	if len(arguments) != 2 {
		return nil, nil, format.Errorf("%s expects 2 arguments, got %d", name, len(arguments))
	}
	list, ok := arguments[0].(*object.List)
	if !ok {
		return nil, nil, format.Errorf("%s expects a list, got %s", name, arguments[0].Type())
	}
	return list, arguments[1], nil
}
