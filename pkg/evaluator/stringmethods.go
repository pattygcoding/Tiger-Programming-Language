package evaluator

import (
	"math"
	"strings"
	"unicode/utf8"

	"tiger/pkg/format"
	"tiger/pkg/object"
)

// maxPadWidth caps string padding so a bad width cannot exhaust memory.
const maxPadWidth = 1000000

// stringMethod builds the string member method named name, or nil when name is
// not a string method. The methods use only the strings and unicode/utf8
// packages, which are already linked in every target, so they stay Wasm-safe.
func stringMethod(name string, receiver object.String) object.Value {
	switch name {
	case "upper":
		return stringTransform(name, receiver, strings.ToUpper)
	case "lower":
		return stringTransform(name, receiver, strings.ToLower)
	case "trim":
		return stringTransform(name, receiver, strings.TrimSpace)
	case "split":
		return splitMethod(receiver)
	case "replace":
		return replaceMethod(receiver)
	case "startswith":
		return affixMethod(name, "prefix", receiver, strings.HasPrefix)
	case "endswith":
		return affixMethod(name, "suffix", receiver, strings.HasSuffix)
	case "find":
		return findMethod(receiver)
	case "padStart":
		return padMethod(name, receiver, true)
	case "padEnd":
		return padMethod(name, receiver, false)
	}
	return nil
}

func stringTransform(name string, receiver object.String, transform func(string) string) object.Value {
	return &object.Builtin{Name: name, Call: func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		if err := noArguments(name, arguments, keywords); err != nil {
			return nil, err
		}
		return object.String(transform(string(receiver))), nil
	}}
}

func noArguments(name string, arguments []object.Value, keywords map[string]object.Value) error {
	if len(keywords) != 0 {
		return format.Errorf("%s does not accept keyword arguments", name)
	}
	if len(arguments) != 0 {
		return format.Errorf("%s expects 0 arguments, got %d", name, len(arguments))
	}
	return nil
}

func stringArgument(name, role string, value object.Value) (string, error) {
	text, ok := value.(object.String)
	if !ok {
		return "", format.Errorf("%s %s must be a string, got %s", name, role, value.Type())
	}
	return string(text), nil
}

// splitMethod mirrors Python's str.split: split() folds runs of whitespace and
// drops empty fields; split(separator) splits on the exact separator and keeps
// empty fields. Returns a list of strings.
func splitMethod(receiver object.String) object.Value {
	return &object.Builtin{Name: "split", Call: func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		if len(keywords) != 0 {
			return nil, format.Errorf("split does not accept keyword arguments")
		}
		var parts []string
		switch len(arguments) {
		case 0:
			parts = strings.Fields(string(receiver))
		case 1:
			separator, ok := arguments[0].(object.String)
			if !ok {
				return nil, format.Errorf("split separator must be a string, got %s", arguments[0].Type())
			}
			if separator == "" {
				return nil, format.Errorf("split: empty separator")
			}
			parts = strings.Split(string(receiver), string(separator))
		default:
			return nil, format.Errorf("split expects at most 1 argument, got %d", len(arguments))
		}
		elements := make([]object.Value, len(parts))
		for index, part := range parts {
			elements[index] = object.String(part)
		}
		return &object.List{Elements: elements}, nil
	}}
}

func replaceMethod(receiver object.String) object.Value {
	return &object.Builtin{Name: "replace", Call: func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		if len(keywords) != 0 {
			return nil, format.Errorf("replace does not accept keyword arguments")
		}
		if len(arguments) != 2 {
			return nil, format.Errorf("replace expects 2 arguments, got %d", len(arguments))
		}
		old, err := stringArgument("replace", "old", arguments[0])
		if err != nil {
			return nil, err
		}
		replacement, err := stringArgument("replace", "new", arguments[1])
		if err != nil {
			return nil, err
		}
		return object.String(strings.ReplaceAll(string(receiver), old, replacement)), nil
	}}
}

func affixMethod(name, role string, receiver object.String, predicate func(string, string) bool) object.Value {
	return &object.Builtin{Name: name, Call: func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		if len(keywords) != 0 {
			return nil, format.Errorf("%s does not accept keyword arguments", name)
		}
		if len(arguments) != 1 {
			return nil, format.Errorf("%s expects 1 argument, got %d", name, len(arguments))
		}
		affix, err := stringArgument(name, role, arguments[0])
		if err != nil {
			return nil, err
		}
		return object.Bool(predicate(string(receiver), affix)), nil
	}}
}

// findMethod returns the code-point index of the first occurrence, or -1.
func findMethod(receiver object.String) object.Value {
	return &object.Builtin{Name: "find", Call: func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		if len(keywords) != 0 {
			return nil, format.Errorf("find does not accept keyword arguments")
		}
		if len(arguments) != 1 {
			return nil, format.Errorf("find expects 1 argument, got %d", len(arguments))
		}
		substring, err := stringArgument("find", "argument", arguments[0])
		if err != nil {
			return nil, err
		}
		text := string(receiver)
		byteIndex := strings.Index(text, substring)
		if byteIndex < 0 {
			return object.Number(-1), nil
		}
		return object.Number(utf8.RuneCountInString(text[:byteIndex])), nil
	}}
}

// padMethod implements padStart/padEnd(width, fill=" "). fill must be one code point.
func padMethod(name string, receiver object.String, start bool) object.Value {
	return &object.Builtin{Name: name, Call: func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		if len(keywords) != 0 {
			return nil, format.Errorf("%s does not accept keyword arguments", name)
		}
		if len(arguments) < 1 || len(arguments) > 2 {
			return nil, format.Errorf("%s expects 1 or 2 arguments, got %d", name, len(arguments))
		}
		width, err := padWidth(name, arguments[0])
		if err != nil {
			return nil, err
		}
		fill := " "
		if len(arguments) == 2 {
			fill, err = stringArgument(name, "fill", arguments[1])
			if err != nil {
				return nil, err
			}
			if utf8.RuneCountInString(fill) != 1 {
				return nil, format.Errorf("%s fill must be a single character", name)
			}
		}
		text := string(receiver)
		needed := width - utf8.RuneCountInString(text)
		if needed <= 0 {
			return object.String(text), nil
		}
		padding := strings.Repeat(fill, needed)
		if start {
			return object.String(padding + text), nil
		}
		return object.String(text + padding), nil
	}}
}

func padWidth(name string, value object.Value) (int, error) {
	number, ok := value.(object.Number)
	if !ok || math.Trunc(float64(number)) != float64(number) || math.IsNaN(float64(number)) || math.IsInf(float64(number), 0) {
		return 0, format.Errorf("%s width must be an integer", name)
	}
	if number < 0 {
		return 0, format.Errorf("%s width must not be negative", name)
	}
	if number > object.Number(maxPadWidth) {
		return 0, format.Errorf("%s width must be at most %d", name, maxPadWidth)
	}
	return int(number), nil
}
