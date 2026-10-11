package evaluator

import (
	"tiger/pkg/format"
	"tiger/pkg/object"
)

// The regex module is shared by every target; the platform files implement it
// with the standard regexp engine on native/WASI and return a clear error in
// the browser, where regexp (and its sort and bytes dependencies) must stay out
// of web/tiger.wasm.
func regexModule() *object.Module {
	env := object.NewEnvironment(nil)
	define := func(name string, call func([]object.Value, map[string]object.Value) (object.Value, error)) {
		_ = env.Define(name, &object.Builtin{Name: "regex." + name, Call: call}, true)
	}

	define("match", func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		values, err := regexArguments("regex.match", arguments, keywords, 2)
		if err != nil {
			return nil, err
		}
		matched, err := regexMatchString(values[0], values[1])
		if err != nil {
			return nil, err
		}
		return object.Bool(matched), nil
	})

	define("find", func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		values, err := regexArguments("regex.find", arguments, keywords, 2)
		if err != nil {
			return nil, err
		}
		match, found, err := regexFindString(values[0], values[1])
		if err != nil {
			return nil, err
		}
		if !found {
			return object.Null{}, nil
		}
		return object.String(match), nil
	})

	define("findAll", func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		values, err := regexArguments("regex.findAll", arguments, keywords, 2)
		if err != nil {
			return nil, err
		}
		matches, err := regexFindAllStrings(values[0], values[1])
		if err != nil {
			return nil, err
		}
		return stringListValue(matches), nil
	})

	define("groups", func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		values, err := regexArguments("regex.groups", arguments, keywords, 2)
		if err != nil {
			return nil, err
		}
		groups, found, err := regexGroupsString(values[0], values[1])
		if err != nil {
			return nil, err
		}
		if !found {
			return object.Null{}, nil
		}
		return stringListValue(groups), nil
	})

	define("replace", func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		values, err := regexArguments("regex.replace", arguments, keywords, 3)
		if err != nil {
			return nil, err
		}
		replaced, err := regexReplaceString(values[0], values[1], values[2])
		if err != nil {
			return nil, err
		}
		return object.String(replaced), nil
	})

	define("split", func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		values, err := regexArguments("regex.split", arguments, keywords, 2)
		if err != nil {
			return nil, err
		}
		parts, err := regexSplitString(values[0], values[1])
		if err != nil {
			return nil, err
		}
		return stringListValue(parts), nil
	})

	return &object.Module{Name: "regex", Env: env}
}

func regexArguments(name string, arguments []object.Value, keywords map[string]object.Value, count int) ([]string, error) {
	if len(keywords) != 0 {
		return nil, format.Errorf("%s does not accept keyword arguments", name)
	}
	if len(arguments) != count {
		return nil, format.Errorf("%s expects %d arguments, got %d", name, count, len(arguments))
	}
	values := make([]string, count)
	for index, argument := range arguments {
		text, ok := argument.(object.String)
		if !ok {
			return nil, format.Errorf("%s expects string arguments, got %s at position %d", name, argument.Type(), index+1)
		}
		values[index] = string(text)
	}
	return values, nil
}

func stringListValue(values []string) *object.List {
	elements := make([]object.Value, len(values))
	for index, value := range values {
		elements[index] = object.String(value)
	}
	return &object.List{Elements: elements}
}
