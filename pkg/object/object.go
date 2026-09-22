package object

import (
	"fmt"
	"math"
	"strconv"
	"strings"
	"tiger/pkg/ast"
)

type Value interface{ Type() string }

type Number float64
type String string
type Bool bool
type Null struct{}
type List struct{ Elements []Value }
type Entry struct{ Key, Value Value }
type Dict struct {
	Entries []Entry
	indices map[Key]int
}
type Function struct {
	Declaration *ast.Function
	Env         *Environment
}
type Builtin struct {
	Name string
	Call func([]Value, map[string]Value) (Value, error)
}
type ReturnValue struct{ Value Value }

type Module struct {
	Name string
	Env  *Environment
}

func (*Module) Type() string { return "module" }

func (Number) Type() string       { return "number" }
func (String) Type() string       { return "string" }
func (Bool) Type() string         { return "bool" }
func (Null) Type() string         { return "null" }
func (*List) Type() string        { return "list" }
func (*Dict) Type() string        { return "dict" }
func (*Function) Type() string    { return "function" }
func (*Builtin) Type() string     { return "builtin" }
func (*ReturnValue) Type() string { return "return" }

type Key struct{ Kind, Text string }

func Hash(value Value) (Key, error) {
	switch typed := value.(type) {
	case Number:
		if math.IsNaN(float64(typed)) || math.IsInf(float64(typed), 0) {
			return Key{}, fmt.Errorf("dictionary keys must be finite")
		}
		if typed == 0 {
			typed = 0
		}
		return Key{Kind: value.Type(), Text: Format(typed)}, nil
	case String, Bool:
		return Key{Kind: value.Type(), Text: Format(value)}, nil
	default:
		return Key{}, fmt.Errorf("%s cannot be a dictionary key", value.Type())
	}
}

func (dict *Dict) Set(key, value Value) error {
	hash, err := Hash(key)
	if err != nil {
		return err
	}
	if dict.indices == nil {
		dict.indices = map[Key]int{}
	}
	if index, exists := dict.indices[hash]; exists {
		dict.Entries[index].Value = value
	} else {
		dict.indices[hash] = len(dict.Entries)
		dict.Entries = append(dict.Entries, Entry{Key: key, Value: value})
	}
	return nil
}

func (dict *Dict) Get(key Value) (Value, bool, error) {
	hash, err := Hash(key)
	if err != nil {
		return nil, false, err
	}
	index, exists := dict.indices[hash]
	if !exists {
		return Null{}, false, nil
	}
	return dict.Entries[index].Value, true, nil
}

func Truthy(value Value) bool {
	switch typed := value.(type) {
	case Null:
		return false
	case Bool:
		return bool(typed)
	case Number:
		return typed != 0
	case String:
		return len(typed) != 0
	case *List:
		return len(typed.Elements) != 0
	case *Dict:
		return len(typed.Entries) != 0
	default:
		return true
	}
}

func Format(value Value) string { return format(value, map[Value]bool{}, false) }

func format(value Value, seen map[Value]bool, nested bool) string {
	switch typed := value.(type) {
	case Number:
		return strconv.FormatFloat(float64(typed), 'f', -1, 64)
	case String:
		if nested {
			return strconv.Quote(string(typed))
		}
		return string(typed)
	case Bool:
		return strconv.FormatBool(bool(typed))
	case Null:
		return "null"
	case *List:
		if seen[value] {
			return "[...]"
		}
		seen[value] = true
		defer delete(seen, value)
		parts := make([]string, len(typed.Elements))
		for index, element := range typed.Elements {
			parts[index] = format(element, seen, true)
		}
		return "[" + strings.Join(parts, ", ") + "]"
	case *Dict:
		if seen[value] {
			return "{...}"
		}
		seen[value] = true
		defer delete(seen, value)
		parts := make([]string, len(typed.Entries))
		for index, entry := range typed.Entries {
			parts[index] = format(entry.Key, seen, true) + ": " + format(entry.Value, seen, true)
		}
		return "{" + strings.Join(parts, ", ") + "}"
	case *Function:
		return "<function " + typed.Declaration.Name + ">"
	case *Module:
		return "<module " + typed.Name + ">"
	case *Class:
		return "<class " + typed.Name + ">"
	case *Instance:
		return "<" + typed.Class.Name + " instance>"
	case *BoundMethod:
		return "<bound method " + typed.Method.Owner.Name + "." + typed.Method.Function.Declaration.Name + ">"
	case *Super:
		return "<super>"
	case *Builtin:
		return "<builtin " + typed.Name + ">"
	case *ReturnValue:
		return format(typed.Value, seen, nested)
	default:
		return "null"
	}
}

func Equal(left, right Value) bool {
	return equal(left, right, map[[2]Value]bool{})
}

func equal(left, right Value, seen map[[2]Value]bool) bool {
	if left.Type() != right.Type() {
		return false
	}
	pair := [2]Value{left, right}
	if seen[pair] {
		return true
	}
	seen[pair] = true
	switch typed := left.(type) {
	case *BoundMethod:
		other := right.(*BoundMethod)
		return typed.Method == other.Method && typed.Receiver == other.Receiver
	case *List:
		other := right.(*List)
		if len(typed.Elements) != len(other.Elements) {
			return false
		}
		for index, element := range typed.Elements {
			if !equal(element, other.Elements[index], seen) {
				return false
			}
		}
		return true
	case *Dict:
		other := right.(*Dict)
		if len(typed.Entries) != len(other.Entries) {
			return false
		}
		for _, entry := range typed.Entries {
			value, exists, err := other.Get(entry.Key)
			if err != nil || !exists || !equal(entry.Value, value, seen) {
				return false
			}
		}
		return true
	default:
		return left == right
	}
}
