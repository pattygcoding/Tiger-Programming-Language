package evaluator

import (
	"fmt"
	"math"
	"strconv"
	"strings"
	"tiger/pkg/object"
)

// isIntegerLiteral matches an optional sign followed by decimal digits.
func isIntegerLiteral(text string) bool {
	text = trimSign(text)
	return text != "" && allDigits(text)
}

// isFloatLiteral matches [+-](digits[.digits] | .digits)[(e|E)[+-]digits].
func isFloatLiteral(text string) bool {
	text = trimSign(text)
	if index := strings.IndexAny(text, "eE"); index >= 0 {
		exponent := trimSign(text[index+1:])
		if exponent == "" || !allDigits(exponent) {
			return false
		}
		text = text[:index]
	}
	whole, fraction, _ := strings.Cut(text, ".")
	return whole+fraction != "" && allDigits(whole) && allDigits(fraction)
}

func trimSign(text string) string {
	if text != "" && (text[0] == '+' || text[0] == '-') {
		return text[1:]
	}
	return text
}

func allDigits(text string) bool {
	for index := 0; index < len(text); index++ {
		if text[index] < '0' || text[index] > '9' {
			return false
		}
	}
	return true
}

func toInt(name string, value object.Value) (object.Value, error) {
	switch typed := value.(type) {
	case object.Number:
		return object.Number(math.Trunc(float64(typed)) + 0), nil
	case object.Bool:
		return boolNumber(typed), nil
	case object.String:
		text := strings.TrimSpace(string(typed))
		if !isIntegerLiteral(text) {
			return nil, fmt.Errorf("%s: invalid integer literal %s", name, strconv.Quote(string(typed)))
		}
		number, err := parseFinite(name, text)
		if err != nil {
			return nil, err
		}
		return number.(object.Number) + 0, nil
	}
	return nil, fmt.Errorf("%s cannot convert %s", name, value.Type())
}

func toFloat(name string, value object.Value) (object.Value, error) {
	switch typed := value.(type) {
	case object.Number:
		return typed, nil
	case object.Bool:
		return boolNumber(typed), nil
	case object.String:
		text := strings.TrimSpace(string(typed))
		if !isFloatLiteral(text) {
			return nil, fmt.Errorf("%s: invalid number literal %s", name, strconv.Quote(string(typed)))
		}
		return parseFinite(name, text)
	}
	return nil, fmt.Errorf("%s cannot convert %s", name, value.Type())
}

func parseFinite(name, text string) (object.Value, error) {
	number, err := strconv.ParseFloat(text, 64)
	if err != nil || math.IsInf(number, 0) {
		return nil, fmt.Errorf("%s: %s is out of range", name, text)
	}
	return object.Number(number), nil
}

func boolNumber(value object.Bool) object.Number {
	if value {
		return 1
	}
	return 0
}

// conversions backs both the int/float/str/bool built-ins and the To* methods.
var conversions = map[string]struct {
	builtin string
	convert func(name string, value object.Value) (object.Value, error)
	empty   object.Value
}{
	"ToInt":   {"int", toInt, object.Number(0)},
	"ToFloat": {"float", toFloat, object.Number(0)},
	"ToString": {"str", func(_ string, value object.Value) (object.Value, error) {
		return object.String(object.Format(value)), nil
	}, object.String("")},
	"ToBool": {"bool", func(_ string, value object.Value) (object.Value, error) {
		return object.Bool(object.Truthy(value)), nil
	}, object.Bool(false)},
}

func conversionBuiltins() map[string]func([]object.Value, map[string]object.Value) (object.Value, error) {
	functions := map[string]func([]object.Value, map[string]object.Value) (object.Value, error){}
	for _, conversion := range conversions {
		conversion := conversion
		functions[conversion.builtin] = func(args []object.Value, keywords map[string]object.Value) (object.Value, error) {
			if len(keywords) != 0 {
				return nil, fmt.Errorf("%s does not accept keyword arguments", conversion.builtin)
			}
			switch len(args) {
			case 0:
				return conversion.empty, nil
			case 1:
				return conversion.convert(conversion.builtin, args[0])
			}
			return nil, fmt.Errorf("%s expects at most 1 argument, got %d", conversion.builtin, len(args))
		}
	}
	return functions
}

// conversionMethod returns a bound To* method for any receiver, or nil for other names.
func conversionMethod(name string, receiver object.Value) object.Value {
	conversion, exists := conversions[name]
	if !exists {
		return nil
	}
	return &object.Builtin{Name: name, Call: func(args []object.Value, keywords map[string]object.Value) (object.Value, error) {
		if len(args) != 0 || len(keywords) != 0 {
			return nil, fmt.Errorf("%s expects 0 arguments", name)
		}
		return conversion.convert(name, receiver)
	}}
}
