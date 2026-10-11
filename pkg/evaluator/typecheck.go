package evaluator

import (
	"tiger/pkg/format"
	"tiger/pkg/object"
)

// typeNames lists every name type(value) can report for a value a Tiger program
// can hold. It excludes the evaluator-internal "return" type and matches the
// strings returned by object.Value.Type().
var typeNames = map[string]bool{
	"number": true, "string": true, "bool": true, "null": true,
	"list": true, "dict": true, "function": true, "builtin": true,
	"class": true, "instance": true, "bound method": true, "super": true,
	"module": true, "file": true, "error": true,
}

// typeBuiltin implements type(value): it returns the same type name that
// value.Type() reports internally.
func typeBuiltin(args []object.Value, keywords map[string]object.Value) (object.Value, error) {
	if len(keywords) != 0 {
		return nil, format.Errorf("type does not accept keyword arguments")
	}
	if len(args) != 1 {
		return nil, format.Errorf("type expects 1 argument, got %d", len(args))
	}
	return object.String(args[0].Type()), nil
}

// isinstanceBuiltin implements isinstance(value, name). The name is a type-name
// string or a list of type-name strings, using the names that type(value)
// reports.
func isinstanceBuiltin(args []object.Value, keywords map[string]object.Value) (object.Value, error) {
	if len(keywords) != 0 {
		return nil, format.Errorf("isinstance does not accept keyword arguments")
	}
	if len(args) != 2 {
		return nil, format.Errorf("isinstance expects 2 arguments, got %d", len(args))
	}
	names, err := typeNameList(args[1])
	if err != nil {
		return nil, err
	}
	for _, name := range names {
		if args[0].Type() == name {
			return object.Bool(true), nil
		}
	}
	return object.Bool(false), nil
}

// typeNameList validates the second isinstance argument and returns its names.
func typeNameList(value object.Value) ([]string, error) {
	switch typed := value.(type) {
	case object.String:
		return checkedTypeName(string(typed))
	case *object.List:
		names := make([]string, 0, len(typed.Elements))
		for _, element := range typed.Elements {
			name, ok := element.(object.String)
			if !ok {
				return nil, format.Errorf("isinstance: type name must be a string, got %s", element.Type())
			}
			single, err := checkedTypeName(string(name))
			if err != nil {
				return nil, err
			}
			names = append(names, single...)
		}
		return names, nil
	default:
		return nil, format.Errorf("isinstance: expected a string or list of strings, got %s", value.Type())
	}
}

func checkedTypeName(name string) ([]string, error) {
	if !typeNames[name] {
		return nil, format.Errorf("isinstance: unknown type name %q", name)
	}
	return []string{name}, nil
}
