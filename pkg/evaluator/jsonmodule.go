package evaluator

import (
	"math"
	"strconv"
	"strings"

	"tiger/pkg/format"
	"tiger/pkg/object"
)

// The json module parses and serializes JSON without encoding/json, which would
// pull reflect and sort into web/tiger.wasm. Only strconv, strings, and math
// are used, all already linked.
func jsonModule() *object.Module {
	env := object.NewEnvironment(nil)
	_ = env.Define("parse", &object.Builtin{Name: "json.parse", Call: func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		if len(keywords) != 0 {
			return nil, format.Errorf("json.parse does not accept keyword arguments")
		}
		if len(arguments) != 1 {
			return nil, format.Errorf("json.parse expects 1 argument, got %d", len(arguments))
		}
		text, ok := arguments[0].(object.String)
		if !ok {
			return nil, format.Errorf("json.parse expects a string, got %s", arguments[0].Type())
		}
		return parseJSON(string(text))
	}}, true)
	_ = env.Define("stringify", &object.Builtin{Name: "json.stringify", Call: func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		if len(keywords) != 0 {
			return nil, format.Errorf("json.stringify does not accept keyword arguments")
		}
		if len(arguments) != 1 {
			return nil, format.Errorf("json.stringify expects 1 argument, got %d", len(arguments))
		}
		var builder strings.Builder
		if err := writeJSON(&builder, arguments[0], map[object.Value]bool{}); err != nil {
			return nil, err
		}
		return object.String(builder.String()), nil
	}}, true)
	return &object.Module{Name: "json", Env: env}
}

type jsonParser struct {
	text []rune
	pos  int
}

func parseJSON(text string) (object.Value, error) {
	parser := &jsonParser{text: []rune(text)}
	value, err := parser.value()
	if err != nil {
		return nil, err
	}
	parser.skipSpace()
	if parser.pos != len(parser.text) {
		return nil, parser.errorf("unexpected trailing content")
	}
	return value, nil
}

func (parser *jsonParser) errorf(template string, args ...any) error {
	return format.Errorf("json.parse at position %d: "+template, append([]any{parser.pos}, args...)...)
}

func (parser *jsonParser) skipSpace() {
	for parser.pos < len(parser.text) {
		switch parser.text[parser.pos] {
		case ' ', '\t', '\n', '\r':
			parser.pos++
		default:
			return
		}
	}
}

func (parser *jsonParser) value() (object.Value, error) {
	parser.skipSpace()
	if parser.pos >= len(parser.text) {
		return nil, parser.errorf("unexpected end of input")
	}
	switch char := parser.text[parser.pos]; {
	case char == '{':
		return parser.object()
	case char == '[':
		return parser.array()
	case char == '"':
		text, err := parser.string()
		if err != nil {
			return nil, err
		}
		return object.String(text), nil
	case char == 't':
		return parser.literal("true", object.Bool(true))
	case char == 'f':
		return parser.literal("false", object.Bool(false))
	case char == 'n':
		return parser.literal("null", object.Null{})
	case char == '-' || (char >= '0' && char <= '9'):
		return parser.number()
	}
	return nil, parser.errorf("unexpected character %q", string(parser.text[parser.pos]))
}

func (parser *jsonParser) literal(word string, value object.Value) (object.Value, error) {
	for _, char := range word {
		if parser.pos >= len(parser.text) || parser.text[parser.pos] != char {
			return nil, parser.errorf("invalid literal")
		}
		parser.pos++
	}
	return value, nil
}

func (parser *jsonParser) object() (object.Value, error) {
	parser.pos++ // '{'
	dict := &object.Dict{}
	parser.skipSpace()
	if parser.pos < len(parser.text) && parser.text[parser.pos] == '}' {
		parser.pos++
		return dict, nil
	}
	for {
		parser.skipSpace()
		if parser.pos >= len(parser.text) || parser.text[parser.pos] != '"' {
			return nil, parser.errorf("expected an object key")
		}
		key, err := parser.string()
		if err != nil {
			return nil, err
		}
		parser.skipSpace()
		if parser.pos >= len(parser.text) || parser.text[parser.pos] != ':' {
			return nil, parser.errorf("expected ':' after an object key")
		}
		parser.pos++
		value, err := parser.value()
		if err != nil {
			return nil, err
		}
		if err := dict.Set(object.String(key), value); err != nil {
			return nil, err
		}
		parser.skipSpace()
		if parser.pos >= len(parser.text) {
			return nil, parser.errorf("unterminated object")
		}
		switch parser.text[parser.pos] {
		case ',':
			parser.pos++
		case '}':
			parser.pos++
			return dict, nil
		default:
			return nil, parser.errorf("expected ',' or '}' in an object")
		}
	}
}

func (parser *jsonParser) array() (object.Value, error) {
	parser.pos++ // '['
	list := &object.List{Elements: []object.Value{}}
	parser.skipSpace()
	if parser.pos < len(parser.text) && parser.text[parser.pos] == ']' {
		parser.pos++
		return list, nil
	}
	for {
		value, err := parser.value()
		if err != nil {
			return nil, err
		}
		list.Elements = append(list.Elements, value)
		parser.skipSpace()
		if parser.pos >= len(parser.text) {
			return nil, parser.errorf("unterminated array")
		}
		switch parser.text[parser.pos] {
		case ',':
			parser.pos++
		case ']':
			parser.pos++
			return list, nil
		default:
			return nil, parser.errorf("expected ',' or ']' in an array")
		}
	}
}

func (parser *jsonParser) string() (string, error) {
	parser.pos++ // '"'
	var builder strings.Builder
	for parser.pos < len(parser.text) {
		char := parser.text[parser.pos]
		parser.pos++
		switch {
		case char == '"':
			return builder.String(), nil
		case char == '\\':
			escaped, err := parser.escape()
			if err != nil {
				return "", err
			}
			builder.WriteRune(escaped)
		case char < 0x20:
			return "", parser.errorf("unescaped control character in a string")
		default:
			builder.WriteRune(char)
		}
	}
	return "", parser.errorf("unterminated string")
}

func (parser *jsonParser) escape() (rune, error) {
	if parser.pos >= len(parser.text) {
		return 0, parser.errorf("unterminated escape")
	}
	char := parser.text[parser.pos]
	parser.pos++
	switch char {
	case '"', '\\', '/':
		return char, nil
	case 'b':
		return '\b', nil
	case 'f':
		return '\f', nil
	case 'n':
		return '\n', nil
	case 'r':
		return '\r', nil
	case 't':
		return '\t', nil
	case 'u':
		return parser.unicode()
	}
	return 0, parser.errorf("invalid escape \\%c", char)
}

func (parser *jsonParser) unicode() (rune, error) {
	first, err := parser.hexadecimal()
	if err != nil {
		return 0, err
	}
	if first < 0xD800 || first > 0xDBFF {
		return first, nil
	}
	if parser.pos+1 >= len(parser.text) || parser.text[parser.pos] != '\\' || parser.text[parser.pos+1] != 'u' {
		return 0, parser.errorf("unpaired surrogate")
	}
	parser.pos += 2
	second, err := parser.hexadecimal()
	if err != nil {
		return 0, err
	}
	if second < 0xDC00 || second > 0xDFFF {
		return 0, parser.errorf("unpaired surrogate")
	}
	return 0x10000 + (first-0xD800)<<10 + (second - 0xDC00), nil
}

func (parser *jsonParser) hexadecimal() (rune, error) {
	if parser.pos+4 > len(parser.text) {
		return 0, parser.errorf("invalid \\u escape")
	}
	value := 0
	for offset := 0; offset < 4; offset++ {
		digit := parser.text[parser.pos+offset]
		switch {
		case digit >= '0' && digit <= '9':
			value = value*16 + int(digit-'0')
		case digit >= 'a' && digit <= 'f':
			value = value*16 + int(digit-'a'+10)
		case digit >= 'A' && digit <= 'F':
			value = value*16 + int(digit-'A'+10)
		default:
			return 0, parser.errorf("invalid \\u escape")
		}
	}
	parser.pos += 4
	return rune(value), nil
}

func (parser *jsonParser) number() (object.Value, error) {
	start := parser.pos
	if parser.text[parser.pos] == '-' {
		parser.pos++
	}
	for parser.pos < len(parser.text) && parser.text[parser.pos] >= '0' && parser.text[parser.pos] <= '9' {
		parser.pos++
	}
	if parser.pos < len(parser.text) && parser.text[parser.pos] == '.' {
		parser.pos++
		if parser.pos >= len(parser.text) || parser.text[parser.pos] < '0' || parser.text[parser.pos] > '9' {
			return nil, parser.errorf("invalid number")
		}
		for parser.pos < len(parser.text) && parser.text[parser.pos] >= '0' && parser.text[parser.pos] <= '9' {
			parser.pos++
		}
	}
	if parser.pos < len(parser.text) && (parser.text[parser.pos] == 'e' || parser.text[parser.pos] == 'E') {
		parser.pos++
		if parser.pos < len(parser.text) && (parser.text[parser.pos] == '+' || parser.text[parser.pos] == '-') {
			parser.pos++
		}
		if parser.pos >= len(parser.text) || parser.text[parser.pos] < '0' || parser.text[parser.pos] > '9' {
			return nil, parser.errorf("invalid number")
		}
		for parser.pos < len(parser.text) && parser.text[parser.pos] >= '0' && parser.text[parser.pos] <= '9' {
			parser.pos++
		}
	}
	text := string(parser.text[start:parser.pos])
	value, err := strconv.ParseFloat(text, 64)
	if err != nil || math.IsInf(value, 0) || math.IsNaN(value) {
		return nil, parser.errorf("invalid or out-of-range number %q", text)
	}
	return object.Number(value), nil
}

func writeJSON(builder *strings.Builder, value object.Value, seen map[object.Value]bool) error {
	switch typed := value.(type) {
	case object.Null:
		builder.WriteString("null")
	case object.Bool:
		if bool(typed) {
			builder.WriteString("true")
		} else {
			builder.WriteString("false")
		}
	case object.Number:
		number := float64(typed)
		if math.IsNaN(number) || math.IsInf(number, 0) {
			return format.Errorf("json.stringify cannot serialize a non-finite number")
		}
		builder.WriteString(object.Format(typed))
	case object.String:
		writeJSONString(builder, string(typed))
	case *object.List:
		if seen[typed] {
			return format.Errorf("json.stringify cannot serialize a cyclic structure")
		}
		seen[typed] = true
		builder.WriteByte('[')
		for index, element := range typed.Elements {
			if index > 0 {
				builder.WriteByte(',')
			}
			if err := writeJSON(builder, element, seen); err != nil {
				return err
			}
		}
		builder.WriteByte(']')
		delete(seen, typed)
	case *object.Dict:
		if seen[typed] {
			return format.Errorf("json.stringify cannot serialize a cyclic structure")
		}
		seen[typed] = true
		builder.WriteByte('{')
		for index, entry := range typed.Entries {
			key, ok := entry.Key.(object.String)
			if !ok {
				return format.Errorf("json.stringify requires string keys, got %s", entry.Key.Type())
			}
			if index > 0 {
				builder.WriteByte(',')
			}
			writeJSONString(builder, string(key))
			builder.WriteByte(':')
			if err := writeJSON(builder, entry.Value, seen); err != nil {
				return err
			}
		}
		builder.WriteByte('}')
		delete(seen, typed)
	default:
		return format.Errorf("json.stringify cannot serialize %s", value.Type())
	}
	return nil
}

func writeJSONString(builder *strings.Builder, text string) {
	const hex = "0123456789abcdef"
	builder.WriteByte('"')
	for _, char := range text {
		switch char {
		case '"':
			builder.WriteString("\\\"")
		case '\\':
			builder.WriteString("\\\\")
		case '\b':
			builder.WriteString("\\b")
		case '\f':
			builder.WriteString("\\f")
		case '\n':
			builder.WriteString("\\n")
		case '\r':
			builder.WriteString("\\r")
		case '\t':
			builder.WriteString("\\t")
		default:
			if char < 0x20 {
				builder.WriteString("\\u00")
				builder.WriteByte(hex[(char>>4)&0xF])
				builder.WriteByte(hex[char&0xF])
			} else {
				builder.WriteRune(char)
			}
		}
	}
	builder.WriteByte('"')
}
