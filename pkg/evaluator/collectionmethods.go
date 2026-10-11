package evaluator

import (
	"math"
	"strings"

	"tiger/pkg/format"
	"tiger/pkg/object"
)

// listMethod builds the list member method named name, or nil when name is not
// a list method. sort is handled in class.go because it calls a user key.
func listMethod(name string, receiver *object.List) object.Value {
	switch name {
	case "join":
		return joinMethod(receiver)
	case "pop":
		return popMethod(receiver)
	case "remove":
		return removeMethod(receiver)
	case "insert":
		return insertMethod(receiver)
	case "index":
		return listIndexMethod(receiver)
	case "extend":
		return extendMethod(receiver)
	case "reverse":
		return listReverseMethod(receiver)
	case "clear":
		return listClearMethod(receiver)
	}
	return nil
}

// popMethod removes and returns the last element, or the element at an index.
func popMethod(list *object.List) object.Value {
	return &object.Builtin{Name: "pop", Call: func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		if len(keywords) != 0 {
			return nil, format.Errorf("pop does not accept keyword arguments")
		}
		if len(arguments) > 1 {
			return nil, format.Errorf("pop expects at most 1 argument, got %d", len(arguments))
		}
		index := len(list.Elements) - 1
		if len(arguments) == 1 {
			resolved, err := elementIndex("pop", arguments[0], len(list.Elements))
			if err != nil {
				return nil, err
			}
			index = resolved
		}
		if index < 0 {
			return nil, format.Errorf("pop from empty list")
		}
		value := list.Elements[index]
		list.Elements = append(list.Elements[:index], list.Elements[index+1:]...)
		return value, nil
	}}
}

// removeMethod removes the first element equal to the argument.
func removeMethod(list *object.List) object.Value {
	return &object.Builtin{Name: "remove", Call: func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		if len(keywords) != 0 {
			return nil, format.Errorf("remove does not accept keyword arguments")
		}
		if len(arguments) != 1 {
			return nil, format.Errorf("remove expects 1 argument, got %d", len(arguments))
		}
		for index, element := range list.Elements {
			if object.Equal(element, arguments[0]) {
				list.Elements = append(list.Elements[:index], list.Elements[index+1:]...)
				return object.Null{}, nil
			}
		}
		return nil, format.Errorf("remove: value not in list")
	}}
}

// insertMethod inserts value before index, clamping out-of-range indexes like Python.
func insertMethod(list *object.List) object.Value {
	return &object.Builtin{Name: "insert", Call: func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		if len(keywords) != 0 {
			return nil, format.Errorf("insert does not accept keyword arguments")
		}
		if len(arguments) != 2 {
			return nil, format.Errorf("insert expects 2 arguments, got %d", len(arguments))
		}
		number, err := integerValue("insert", "index", arguments[0])
		if err != nil {
			return nil, err
		}
		index := int(number)
		if index < 0 {
			index += len(list.Elements)
			if index < 0 {
				index = 0
			}
		}
		if index > len(list.Elements) {
			index = len(list.Elements)
		}
		list.Elements = append(list.Elements, object.Null{})
		copy(list.Elements[index+1:], list.Elements[index:])
		list.Elements[index] = arguments[1]
		return object.Null{}, nil
	}}
}

// listIndexMethod returns the first index of the value, or -1, like string find.
func listIndexMethod(list *object.List) object.Value {
	return &object.Builtin{Name: "index", Call: func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		if len(keywords) != 0 {
			return nil, format.Errorf("index does not accept keyword arguments")
		}
		if len(arguments) != 1 {
			return nil, format.Errorf("index expects 1 argument, got %d", len(arguments))
		}
		for index, element := range list.Elements {
			if object.Equal(element, arguments[0]) {
				return object.Number(index), nil
			}
		}
		return object.Number(-1), nil
	}}
}

func extendMethod(list *object.List) object.Value {
	return &object.Builtin{Name: "extend", Call: func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		if len(keywords) != 0 {
			return nil, format.Errorf("extend does not accept keyword arguments")
		}
		if len(arguments) != 1 {
			return nil, format.Errorf("extend expects 1 argument, got %d", len(arguments))
		}
		other, ok := arguments[0].(*object.List)
		if !ok {
			return nil, format.Errorf("extend expects a list, got %s", arguments[0].Type())
		}
		list.Elements = append(list.Elements, other.Elements...)
		return object.Null{}, nil
	}}
}

func listReverseMethod(list *object.List) object.Value {
	return &object.Builtin{Name: "reverse", Call: func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		if err := noArguments("reverse", arguments, keywords); err != nil {
			return nil, err
		}
		for left, right := 0, len(list.Elements)-1; left < right; left, right = left+1, right-1 {
			list.Elements[left], list.Elements[right] = list.Elements[right], list.Elements[left]
		}
		return object.Null{}, nil
	}}
}

func listClearMethod(list *object.List) object.Value {
	return &object.Builtin{Name: "clear", Call: func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		if err := noArguments("clear", arguments, keywords); err != nil {
			return nil, err
		}
		list.Elements = nil
		return object.Null{}, nil
	}}
}

// joinMethod joins a list of strings with a separator: items.join(separator).
func joinMethod(receiver *object.List) object.Value {
	return &object.Builtin{Name: "join", Call: func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		if len(keywords) != 0 {
			return nil, format.Errorf("join does not accept keyword arguments")
		}
		if len(arguments) != 1 {
			return nil, format.Errorf("join expects 1 argument, got %d", len(arguments))
		}
		separator, err := stringArgument("join", "separator", arguments[0])
		if err != nil {
			return nil, err
		}
		parts := make([]string, len(receiver.Elements))
		for index, element := range receiver.Elements {
			text, ok := element.(object.String)
			if !ok {
				return nil, format.Errorf("join expects a list of strings, got %s at index %d", element.Type(), index)
			}
			parts[index] = string(text)
		}
		return object.String(strings.Join(parts, separator)), nil
	}}
}

func elementIndex(name string, value object.Value, length int) (int, error) {
	number, err := integerValue(name, "index", value)
	if err != nil {
		return 0, err
	}
	index := int(number)
	if index < 0 {
		index += length
	}
	if index < 0 || index >= length {
		return 0, format.Errorf("%s index out of range", name)
	}
	return index, nil
}

func integerValue(name, role string, value object.Value) (object.Number, error) {
	number, ok := value.(object.Number)
	if !ok || math.Trunc(float64(number)) != float64(number) || math.IsNaN(float64(number)) || math.IsInf(float64(number), 0) {
		return 0, format.Errorf("%s %s must be an integer", name, role)
	}
	return number, nil
}

// dictMethod builds the dictionary member method named name, or nil.
func dictMethod(name string, receiver *object.Dict) object.Value {
	switch name {
	case "keys":
		return dictKeysMethod(receiver)
	case "values":
		return dictValuesMethod(receiver)
	case "items":
		return dictItemsMethod(receiver)
	case "get":
		return dictGetMethod(receiver)
	case "pop":
		return dictPopMethod(receiver)
	case "delete":
		return dictDeleteMethod(receiver)
	case "clear":
		return dictClearMethod(receiver)
	}
	return nil
}

func dictKeysMethod(dict *object.Dict) object.Value {
	return &object.Builtin{Name: "keys", Call: func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		if err := noArguments("keys", arguments, keywords); err != nil {
			return nil, err
		}
		keys := make([]object.Value, len(dict.Entries))
		for index, entry := range dict.Entries {
			keys[index] = entry.Key
		}
		return &object.List{Elements: keys}, nil
	}}
}

func dictValuesMethod(dict *object.Dict) object.Value {
	return &object.Builtin{Name: "values", Call: func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		if err := noArguments("values", arguments, keywords); err != nil {
			return nil, err
		}
		values := make([]object.Value, len(dict.Entries))
		for index, entry := range dict.Entries {
			values[index] = entry.Value
		}
		return &object.List{Elements: values}, nil
	}}
}

// dictItemsMethod returns [key, value] lists because there is no tuple type.
func dictItemsMethod(dict *object.Dict) object.Value {
	return &object.Builtin{Name: "items", Call: func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		if err := noArguments("items", arguments, keywords); err != nil {
			return nil, err
		}
		items := make([]object.Value, len(dict.Entries))
		for index, entry := range dict.Entries {
			items[index] = &object.List{Elements: []object.Value{entry.Key, entry.Value}}
		}
		return &object.List{Elements: items}, nil
	}}
}

func dictGetMethod(dict *object.Dict) object.Value {
	return &object.Builtin{Name: "get", Call: func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		if len(keywords) != 0 {
			return nil, format.Errorf("get does not accept keyword arguments")
		}
		if len(arguments) < 1 || len(arguments) > 2 {
			return nil, format.Errorf("get expects 1 or 2 arguments, got %d", len(arguments))
		}
		value, exists, err := dict.Get(arguments[0])
		if err != nil {
			return nil, err
		}
		if exists {
			return value, nil
		}
		if len(arguments) == 2 {
			return arguments[1], nil
		}
		return object.Null{}, nil
	}}
}

func dictPopMethod(dict *object.Dict) object.Value {
	return &object.Builtin{Name: "pop", Call: func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		if len(keywords) != 0 {
			return nil, format.Errorf("pop does not accept keyword arguments")
		}
		if len(arguments) < 1 || len(arguments) > 2 {
			return nil, format.Errorf("pop expects 1 or 2 arguments, got %d", len(arguments))
		}
		value, exists, err := dict.Get(arguments[0])
		if err != nil {
			return nil, err
		}
		if !exists {
			if len(arguments) == 2 {
				return arguments[1], nil
			}
			return nil, format.Errorf("pop: key not found: %s", object.Format(arguments[0]))
		}
		if _, err := dict.Delete(arguments[0]); err != nil {
			return nil, err
		}
		return value, nil
	}}
}

// dictDeleteMethod removes a key and reports whether it was present.
func dictDeleteMethod(dict *object.Dict) object.Value {
	return &object.Builtin{Name: "delete", Call: func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		if len(keywords) != 0 {
			return nil, format.Errorf("delete does not accept keyword arguments")
		}
		if len(arguments) != 1 {
			return nil, format.Errorf("delete expects 1 argument, got %d", len(arguments))
		}
		removed, err := dict.Delete(arguments[0])
		if err != nil {
			return nil, err
		}
		return object.Bool(removed), nil
	}}
}

func dictClearMethod(dict *object.Dict) object.Value {
	return &object.Builtin{Name: "clear", Call: func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		if err := noArguments("clear", arguments, keywords); err != nil {
			return nil, err
		}
		dict.Clear()
		return object.Null{}, nil
	}}
}
