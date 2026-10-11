// Package format provides the small subset of Go's fmt package that the Tiger
// runtime needs. It deliberately avoids importing fmt (and therefore reflect),
// which keeps the browser Wasm binary small. Only the verbs used by the runtime
// are supported: %s, %d, %q, %c, %v, %t, %w, and %%.
package format

import (
	"errors"
	"strconv"
	"strings"
)

// stringer mirrors fmt.Stringer without importing fmt.
type stringer interface{ String() string }

// Sprintf formats its arguments according to the format string.
func Sprintf(format string, args ...any) string {
	var builder strings.Builder
	index := 0
	write(&builder, format, args, &index, nil)
	return builder.String()
}

// Errorf returns an error whose message is formatted like Sprintf. When the
// format contains %w, the matching argument is wrapped so errors.Is and
// errors.As keep working.
func Errorf(format string, args ...any) error {
	var wrapped []error
	var builder strings.Builder
	index := 0
	write(&builder, format, args, &index, &wrapped)
	if len(wrapped) == 1 {
		return &wrappedError{message: builder.String(), err: wrapped[0]}
	}
	if len(wrapped) > 1 {
		return &joinedError{message: builder.String(), errs: wrapped}
	}
	return errors.New(builder.String())
}

func write(builder *strings.Builder, format string, args []any, index *int, wrapped *[]error) {
	for position := 0; position < len(format); position++ {
		if format[position] != '%' {
			builder.WriteByte(format[position])
			continue
		}
		if position+1 >= len(format) {
			builder.WriteString("%!(NOVERB)")
			return
		}
		position++
		verb := format[position]
		if verb == '%' {
			builder.WriteByte('%')
			continue
		}
		if *index >= len(args) {
			builder.WriteString("%!")
			builder.WriteByte(verb)
			builder.WriteString("(MISSING)")
			continue
		}
		argument := args[*index]
		*index = *index + 1
		if verb == 'w' {
			failure, ok := argument.(error)
			if !ok {
				builder.WriteString("%!w(")
				builder.WriteString(defaultText(argument))
				builder.WriteByte(')')
				continue
			}
			if wrapped != nil {
				*wrapped = append(*wrapped, failure)
			}
			builder.WriteString(failure.Error())
			continue
		}
		builder.WriteString(verbText(verb, argument))
	}
}

func verbText(verb byte, argument any) string {
	switch verb {
	case 's', 'v':
		return text(argument)
	case 'q':
		return quoted(argument)
	case 'd':
		return decimal(argument)
	case 'c':
		return character(argument)
	case 't':
		return boolean(argument)
	default:
		return "%!" + string(verb) + "(" + defaultText(argument) + ")"
	}
}

func text(argument any) string {
	switch typed := argument.(type) {
	case nil:
		return "<nil>"
	case string:
		return typed
	case error:
		return typed.Error()
	case stringer:
		return typed.String()
	default:
		return defaultText(argument)
	}
}

func quoted(argument any) string {
	switch typed := argument.(type) {
	case nil:
		return "<nil>"
	case string:
		return strconv.Quote(typed)
	case []byte:
		return strconv.Quote(string(typed))
	case error:
		return strconv.Quote(typed.Error())
	case stringer:
		return strconv.Quote(typed.String())
	}
	if value, ok := asRune(argument); ok {
		return strconv.QuoteRune(value)
	}
	return strconv.Quote(defaultText(argument))
}

func decimal(argument any) string {
	switch typed := argument.(type) {
	case int:
		return strconv.FormatInt(int64(typed), 10)
	case int8:
		return strconv.FormatInt(int64(typed), 10)
	case int16:
		return strconv.FormatInt(int64(typed), 10)
	case int32:
		return strconv.FormatInt(int64(typed), 10)
	case int64:
		return strconv.FormatInt(typed, 10)
	case uint:
		return strconv.FormatUint(uint64(typed), 10)
	case uint8:
		return strconv.FormatUint(uint64(typed), 10)
	case uint16:
		return strconv.FormatUint(uint64(typed), 10)
	case uint32:
		return strconv.FormatUint(uint64(typed), 10)
	case uint64:
		return strconv.FormatUint(typed, 10)
	case bool:
		return strconv.FormatBool(typed)
	}
	return defaultText(argument)
}

func character(argument any) string {
	if value, ok := asRune(argument); ok {
		return string(value)
	}
	return "%!c(" + defaultText(argument) + ")"
}

func boolean(argument any) string {
	if value, ok := argument.(bool); ok {
		return strconv.FormatBool(value)
	}
	return "%!t(" + defaultText(argument) + ")"
}

// asRune reports whether the argument is an integer, returning it as a rune so
// that %q and %c match Go's rune handling.
func asRune(argument any) (rune, bool) {
	switch typed := argument.(type) {
	case rune:
		return typed, true
	case int:
		return rune(typed), true
	case int8:
		return rune(typed), true
	case int16:
		return rune(typed), true
	case int64:
		return rune(typed), true
	case uint:
		return rune(typed), true
	case uint8:
		return rune(typed), true
	case uint16:
		return rune(typed), true
	case uint32:
		return rune(typed), true
	case uint64:
		return rune(typed), true
	}
	return 0, false
}

func defaultText(argument any) string {
	switch typed := argument.(type) {
	case nil:
		return "<nil>"
	case string:
		return typed
	case bool:
		return strconv.FormatBool(typed)
	case error:
		return typed.Error()
	case stringer:
		return typed.String()
	case int:
		return strconv.FormatInt(int64(typed), 10)
	case int8:
		return strconv.FormatInt(int64(typed), 10)
	case int16:
		return strconv.FormatInt(int64(typed), 10)
	case int32:
		return strconv.FormatInt(int64(typed), 10)
	case int64:
		return strconv.FormatInt(typed, 10)
	case uint:
		return strconv.FormatUint(uint64(typed), 10)
	case uint8:
		return strconv.FormatUint(uint64(typed), 10)
	case uint16:
		return strconv.FormatUint(uint64(typed), 10)
	case uint32:
		return strconv.FormatUint(uint64(typed), 10)
	case uint64:
		return strconv.FormatUint(typed, 10)
	case float32:
		return strconv.FormatFloat(float64(typed), 'g', -1, 32)
	case float64:
		return strconv.FormatFloat(typed, 'g', -1, 64)
	default:
		return "?"
	}
}

type wrappedError struct {
	message string
	err     error
}

func (failure *wrappedError) Error() string { return failure.message }

func (failure *wrappedError) Unwrap() error { return failure.err }

type joinedError struct {
	message string
	errs    []error
}

func (failure *joinedError) Error() string { return failure.message }

func (failure *joinedError) Unwrap() []error { return failure.errs }
