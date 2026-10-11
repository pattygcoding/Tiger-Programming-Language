package evaluator

import (
	"strconv"
	"strings"
	"unicode"

	"tiger/pkg/ast"
	"tiger/pkg/format"
	"tiger/pkg/object"
)

// applyFormat renders one f-string field with a small format spec:
// [<|>|^][0][width][.Nf]. An empty spec falls back to object.Format.
func applyFormat(node ast.Node, spec string, value object.Value) string {
	text := object.Format(value)
	if spec == "" {
		return text
	}
	align := byte(0)
	pad := rune(' ')
	index := 0
	if index < len(spec) && (spec[index] == '<' || spec[index] == '>' || spec[index] == '^') {
		align = spec[index]
		index++
	}
	if index < len(spec) && spec[index] == '0' {
		pad = '0'
		index++
	}
	width := 0
	for index < len(spec) && spec[index] >= '0' && spec[index] <= '9' {
		width = width*10 + int(spec[index]-'0')
		index++
	}
	if index < len(spec) && spec[index] == '.' {
		index++
		if index >= len(spec) || spec[index] < '0' || spec[index] > '9' {
			fail(node, "invalid format spec %q: expected digits after '.'", spec)
		}
		precision := 0
		for index < len(spec) && spec[index] >= '0' && spec[index] <= '9' {
			precision = precision*10 + int(spec[index]-'0')
			index++
		}
		if index >= len(spec) || spec[index] != 'f' {
			fail(node, "invalid format spec %q: only '.Nf' precision is supported", spec)
		}
		index++
		if precision > 17 {
			fail(node, "invalid format spec %q: precision must be at most 17", spec)
		}
		number, ok := value.(object.Number)
		if !ok {
			fail(node, "format spec %q requires a number, got %s", spec, value.Type())
		}
		text = strconv.FormatFloat(float64(number), 'f', precision, 64)
	}
	if index != len(spec) {
		fail(node, "invalid format spec %q", spec)
	}
	if align == 0 {
		if _, isText := value.(object.String); isText {
			align = '<'
		} else {
			align = '>'
		}
	}
	return padText(text, align, pad, width)
}

// padText pads text to at least width code points using the given alignment.
func padText(text string, align byte, pad rune, width int) string {
	length := len([]rune(text))
	if width <= length {
		return text
	}
	fill := strings.Repeat(string(pad), width-length)
	switch align {
	case '>':
		if pad == '0' && len(text) > 0 && (text[0] == '-' || text[0] == '+') {
			return text[:1] + fill + text[1:]
		}
		return fill + text
	case '^':
		left := (width - length) / 2
		return strings.Repeat(string(pad), left) + text + strings.Repeat(string(pad), width-length-left)
	default:
		return text + fill
	}
}

// graphemesBuiltin splits a string into approximate grapheme clusters.
func graphemesBuiltin(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
	if len(keywords) != 0 {
		return nil, format.Errorf("graphemes does not accept keyword arguments")
	}
	if len(arguments) != 1 {
		return nil, format.Errorf("graphemes expects 1 argument, got %d", len(arguments))
	}
	text, ok := arguments[0].(object.String)
	if !ok {
		return nil, format.Errorf("graphemes expects a string, got %s", arguments[0].Type())
	}
	clusters := splitGraphemes(string(text))
	elements := make([]object.Value, len(clusters))
	for index, cluster := range clusters {
		elements[index] = object.String(cluster)
	}
	return &object.List{Elements: elements}, nil
}

const zeroWidthJoiner = '\u200D'

// splitGraphemes groups a base character with its combining marks, variation
// selectors, emoji modifiers, and zero-width-joiner continuations, and pairs
// regional indicators into flags. It approximates UAX #29 for the common cases
// without carrying the full Unicode tables.
func splitGraphemes(text string) []string {
	clusters := []string{}
	current := []rune{}
	join := false
	for _, char := range text {
		switch {
		case len(current) == 0:
		case join:
			join = false
		case extendsGrapheme(char):
		case isRegionalIndicator(char) && len(current) == 1 && isRegionalIndicator(current[0]):
		case char == '\n' && current[len(current)-1] == '\r':
		default:
			clusters = append(clusters, string(current))
			current = current[:0]
		}
		current = append(current, char)
		if char == zeroWidthJoiner {
			join = true
		}
	}
	if len(current) > 0 {
		clusters = append(clusters, string(current))
	}
	return clusters
}

// extendsGrapheme reports whether char continues the preceding cluster without
// starting a new one.
func extendsGrapheme(char rune) bool {
	if char == zeroWidthJoiner {
		return true
	}
	if unicode.Is(unicode.Mn, char) || unicode.Is(unicode.Mc, char) || unicode.Is(unicode.Me, char) {
		return true
	}
	if (char >= 0xFE00 && char <= 0xFE0F) || (char >= 0xE0100 && char <= 0xE01EF) {
		return true // variation selectors
	}
	return char >= 0x1F3FB && char <= 0x1F3FF // emoji skin-tone modifiers
}

func isRegionalIndicator(char rune) bool { return char >= 0x1F1E6 && char <= 0x1F1FF }
