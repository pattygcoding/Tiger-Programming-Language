package evaluator

import (
	"fmt"
	"io"
	"math"
	"strings"
	"tiger/pkg/ast"
	"tiger/pkg/object"
	"tiger/pkg/parser"
)

type runtimeError struct{ err error }

type Evaluator struct {
	Output   io.Writer
	MaxSteps int
	steps    int
	depth    int
}

func New(output io.Writer) *Evaluator {
	if output == nil {
		output = io.Discard
	}
	return &Evaluator{Output: output, MaxSteps: 1_000_000}
}

func Run(source string, output io.Writer) error {
	program, err := parser.Parse(source)
	if err != nil {
		return err
	}
	return New(output).Execute(program)
}

func (eval *Evaluator) Execute(program *ast.Program) (err error) {
	defer func() {
		if failure := recover(); failure != nil {
			if runtime, ok := failure.(runtimeError); ok {
				err = runtime.err
			} else {
				panic(failure)
			}
		}
	}()
	eval.steps, eval.depth = 0, 0
	if eval.Output == nil {
		eval.Output = io.Discard
	}
	env := object.NewEnvironment(nil)
	eval.builtins(env)
	eval.block(program.Statements, env)
	return nil
}

func fail(node ast.Node, format string, args ...any) {
	panic(runtimeError{node.Position().Errorf(format, args...)})
}

func check(node ast.Node, err error) {
	if err != nil {
		fail(node, "%s", err)
	}
}

func (eval *Evaluator) tick(node ast.Node) {
	eval.steps++
	if eval.MaxSteps > 0 && eval.steps > eval.MaxSteps {
		fail(node, "execution step limit exceeded")
	}
}

func (eval *Evaluator) block(statements []ast.Stmt, env *object.Environment) *object.ReturnValue {
	for _, statement := range statements {
		if result := eval.statement(statement, env); result != nil {
			return result
		}
	}
	return nil
}

func (eval *Evaluator) statement(statement ast.Stmt, env *object.Environment) *object.ReturnValue {
	eval.tick(statement)
	switch node := statement.(type) {
	case *ast.ExpressionStmt:
		eval.expression(node.Value, env)
	case *ast.Assign:
		value := eval.expression(node.Value, env)
		switch target := node.Target.(type) {
		case *ast.Identifier:
			if node.Constant {
				check(node, env.Define(target.Name, value, true))
			} else {
				check(node, env.Assign(target.Name, value))
			}
		case *ast.Property:
			receiver := eval.expression(target.Receiver, env)
			instance, ok := receiver.(*object.Instance)
			if !ok {
				fail(target, "cannot assign a property of %s", receiver.Type())
			}
			instance.Fields[target.Name] = value
		case *ast.Index:
			collection := eval.expression(target.Collection, env)
			key := eval.expression(target.Key, env)
			switch typed := collection.(type) {
			case *object.List:
				typed.Elements[indexAt(target, key, len(typed.Elements))] = value
			case *object.Dict:
				check(target, typed.Set(key, value))
			default:
				fail(target, "cannot assign an index of %s", collection.Type())
			}
		}
	case *ast.Function:
		check(node, env.Define(node.Name, &object.Function{Declaration: node, Env: env}, false))
	case *ast.Class:
		class := &object.Class{Name: node.Name, Methods: map[string]*object.Method{}}
		if node.Parent != nil {
			parent := eval.expression(node.Parent, env)
			var ok bool
			class.Parent, ok = parent.(*object.Class)
			if !ok {
				fail(node.Parent, "parent must be a class, got %s", parent.Type())
			}
		}
		for _, method := range node.Methods {
			class.Methods[method.Name] = &object.Method{
				Function: &object.Function{Declaration: method, Env: env},
				Owner:    class,
			}
		}
		check(node, env.Define(node.Name, class, false))
	case *ast.Return:
		value := object.Value(object.Null{})
		if node.Value != nil {
			value = eval.expression(node.Value, env)
		}
		return &object.ReturnValue{Value: value}
	case *ast.If:
		for _, branch := range node.Branches {
			if object.Truthy(eval.expression(branch.Condition, env)) {
				return eval.block(branch.Body, object.NewEnvironment(env))
			}
		}
		return eval.block(node.Else, object.NewEnvironment(env))
	case *ast.While:
		for object.Truthy(eval.expression(node.Condition, env)) {
			if result := eval.block(node.Body, object.NewEnvironment(env)); result != nil {
				return result
			}
		}
	case *ast.For:
		iterable := eval.expression(node.Iterable, env)
		var values []object.Value
		switch typed := iterable.(type) {
		case *object.List:
			values = append(values, typed.Elements...)
		case object.String:
			for _, char := range string(typed) {
				values = append(values, object.String(string(char)))
			}
		case *object.Dict:
			for _, entry := range typed.Entries {
				values = append(values, entry.Key)
			}
		default:
			fail(node, "%s is not iterable", iterable.Type())
		}
		for _, value := range values {
			eval.tick(node)
			scope := object.NewEnvironment(env)
			check(node, scope.Define(node.Name, value, false))
			if result := eval.block(node.Body, scope); result != nil {
				return result
			}
		}
	}
	return nil
}

func (eval *Evaluator) expression(expression ast.Expr, env *object.Environment) object.Value {
	eval.tick(expression)
	eval.depth++
	defer func() { eval.depth-- }()
	if eval.depth > 512 {
		fail(expression, "maximum evaluation depth exceeded")
	}
	switch node := expression.(type) {
	case *ast.Literal:
		switch value := node.Value.(type) {
		case float64:
			return object.Number(value)
		case string:
			return object.String(value)
		case bool:
			return object.Bool(value)
		default:
			return object.Null{}
		}
	case *ast.Identifier:
		value, err := env.Get(node.Name)
		check(node, err)
		return value
	case *ast.Super:
		value, err := env.Get("super")
		if err != nil {
			fail(node, "super is only available inside methods")
		}
		return value
	case *ast.Property:
		return property(node, eval.expression(node.Receiver, env))
	case *ast.List:
		list := &object.List{}
		for _, element := range node.Elements {
			list.Elements = append(list.Elements, eval.expression(element, env))
		}
		return list
	case *ast.Dict:
		dict := &object.Dict{}
		for _, pair := range node.Pairs {
			key := eval.expression(pair.Key, env)
			value := eval.expression(pair.Value, env)
			check(node, dict.Set(key, value))
		}
		return dict
	case *ast.Unary:
		right := eval.expression(node.Right, env)
		if node.Operator == "not" {
			return object.Bool(!object.Truthy(right))
		}
		value, ok := right.(object.Number)
		if !ok {
			fail(node, "operator %s requires a number", node.Operator)
		}
		if node.Operator == "-" {
			return -value
		}
		return value
	case *ast.Binary:
		left := eval.expression(node.Left, env)
		if node.Operator == "and" {
			if !object.Truthy(left) {
				return left
			}
			return eval.expression(node.Right, env)
		}
		if node.Operator == "or" {
			if object.Truthy(left) {
				return left
			}
			return eval.expression(node.Right, env)
		}
		return binary(node, left, eval.expression(node.Right, env))
	case *ast.Index:
		collection := eval.expression(node.Collection, env)
		key := eval.expression(node.Key, env)
		switch typed := collection.(type) {
		case *object.List:
			return typed.Elements[indexAt(node, key, len(typed.Elements))]
		case object.String:
			chars := []rune(string(typed))
			return object.String(string(chars[indexAt(node, key, len(chars))]))
		case *object.Dict:
			value, exists, err := typed.Get(key)
			check(node, err)
			if !exists {
				fail(node, "dictionary key not found: %s", object.Format(key))
			}
			return value
		default:
			fail(node, "%s cannot be indexed", collection.Type())
		}
	case *ast.Call:
		function := eval.expression(node.Function, env)
		arguments := make([]object.Value, len(node.Arguments))
		for index, argument := range node.Arguments {
			arguments[index] = eval.expression(argument, env)
		}
		return eval.call(node, function, arguments)
	}
	fail(expression, "unsupported expression")
	return object.Null{}
}

func indexAt(node ast.Node, key object.Value, length int) int {
	number, ok := key.(object.Number)
	if !ok || math.Trunc(float64(number)) != float64(number) {
		fail(node, "index must be an integer")
	}
	if number < 0 {
		number += object.Number(length)
	}
	if number < 0 || number >= object.Number(length) {
		fail(node, "index out of range")
	}
	return int(number)
}

func binary(node *ast.Binary, left, right object.Value) object.Value {
	switch node.Operator {
	case "==":
		return object.Bool(object.Equal(left, right))
	case "!=":
		return object.Bool(!object.Equal(left, right))
	case "in":
		switch collection := right.(type) {
		case *object.List:
			for _, element := range collection.Elements {
				if object.Equal(left, element) {
					return object.Bool(true)
				}
			}
			return object.Bool(false)
		case *object.Dict:
			_, exists, err := collection.Get(left)
			check(node, err)
			return object.Bool(exists)
		case object.String:
			if text, ok := left.(object.String); ok {
				return object.Bool(strings.Contains(string(collection), string(text)))
			}
		}
		fail(node, "invalid operands for 'in': %s and %s", left.Type(), right.Type())
	}
	if first, ok := left.(object.Number); ok {
		if second, ok := right.(object.Number); ok {
			var result object.Number
			switch node.Operator {
			case "+":
				result = first + second
			case "-":
				result = first - second
			case "*":
				result = first * second
			case "/", "%":
				if second == 0 {
					fail(node, "division by zero")
				}
				if node.Operator == "/" {
					result = first / second
				} else {
					result = object.Number(math.Mod(float64(first), float64(second)))
					if result != 0 && (result < 0) != (second < 0) {
						result += second
					}
				}
			case "<":
				return object.Bool(first < second)
			case "<=":
				return object.Bool(first <= second)
			case ">":
				return object.Bool(first > second)
			case ">=":
				return object.Bool(first >= second)
			default:
				fail(node, "unknown operator %s", node.Operator)
			}
			if math.IsInf(float64(result), 0) || math.IsNaN(float64(result)) {
				fail(node, "non-finite numeric result")
			}
			return result
		}
	}
	if first, ok := left.(object.String); ok {
		if second, ok := right.(object.String); ok {
			switch node.Operator {
			case "+":
				return first + second
			case "<":
				return object.Bool(first < second)
			case "<=":
				return object.Bool(first <= second)
			case ">":
				return object.Bool(first > second)
			case ">=":
				return object.Bool(first >= second)
			}
		}
	}
	if first, ok := left.(*object.List); ok && node.Operator == "+" {
		if second, ok := right.(*object.List); ok {
			items := append([]object.Value{}, first.Elements...)
			return &object.List{Elements: append(items, second.Elements...)}
		}
	}
	fail(node, "operator %s is not supported for %s and %s", node.Operator, left.Type(), right.Type())
	return object.Null{}
}

func (eval *Evaluator) builtins(env *object.Environment) {
	functions := map[string]func([]object.Value) (object.Value, error){
		"print": func(args []object.Value) (object.Value, error) {
			parts := make([]string, len(args))
			for index, value := range args {
				parts[index] = object.Format(value)
			}
			_, err := fmt.Fprintln(eval.Output, strings.Join(parts, " "))
			return object.Null{}, err
		},
		"str": func(args []object.Value) (object.Value, error) {
			if len(args) != 1 {
				return nil, fmt.Errorf("str expects 1 argument, got %d", len(args))
			}
			return object.String(object.Format(args[0])), nil
		},
		"len": func(args []object.Value) (object.Value, error) {
			if len(args) != 1 {
				return nil, fmt.Errorf("len expects 1 argument, got %d", len(args))
			}
			switch value := args[0].(type) {
			case object.String:
				return object.Number(len([]rune(string(value)))), nil
			case *object.List:
				return object.Number(len(value.Elements)), nil
			case *object.Dict:
				return object.Number(len(value.Entries)), nil
			default:
				return nil, fmt.Errorf("len does not accept %s", value.Type())
			}
		},
	}
	for name, function := range functions {
		_ = env.Define(name, &object.Builtin{Name: name, Call: function}, true)
	}
}
