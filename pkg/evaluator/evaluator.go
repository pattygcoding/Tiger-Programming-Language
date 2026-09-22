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

type runtimeError struct {
	err   error
	fatal bool
}

type thrown struct {
	node  ast.Node
	value object.Value
}

type flow struct {
	kind  string
	value object.Value
}

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
			switch failure := failure.(type) {
			case runtimeError:
				err = failure.err
			case thrown:
				err = failure.node.Position().Errorf("uncaught throw: %s", object.Format(failure.value))
			default:
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
	panic(runtimeError{err: node.Position().Errorf(format, args...)})
}

func check(node ast.Node, err error) {
	if err != nil {
		fail(node, "%s", err)
	}
}

func (eval *Evaluator) tick(node ast.Node) {
	eval.steps++
	if eval.MaxSteps > 0 && eval.steps > eval.MaxSteps {
		panic(runtimeError{err: node.Position().Errorf("execution step limit exceeded"), fatal: true})
	}
}

func (eval *Evaluator) block(statements []ast.Stmt, env *object.Environment) *flow {
	for _, statement := range statements {
		if result := eval.statement(statement, env); result != nil {
			return result
		}
	}
	return nil
}

func (eval *Evaluator) statement(statement ast.Stmt, env *object.Environment) *flow {
	eval.tick(statement)
	switch node := statement.(type) {
	case *ast.Throw:
		panic(thrown{node: node, value: eval.expression(node.Value, env)})
	case *ast.Try:
		return eval.tryStatement(node, env)
	case *ast.ExpressionStmt:
		eval.expression(node.Value, env)
	case *ast.Assign:
		value := eval.expression(node.Value, env)
		if node.Declaration || node.Constant {
			check(node, env.Define(node.Target.(*ast.Identifier).Name, value, node.Constant))
		} else {
			_, write := eval.reference(node.Target, env)
			write(value)
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
			if class.Parent != nil && class.Parent.FindField(method.Name) != nil {
				fail(method, "cannot override inherited field %q", method.Name)
			}
			class.Methods[method.Name] = &object.Method{
				Function: &object.Function{Declaration: method, Env: env},
				Owner:    class,
			}
		}
		for _, field := range node.Fields {
			if class.Parent != nil && (class.Parent.FindField(field.Name) != nil || class.Parent.FindMethod(field.Name) != nil) {
				fail(field, "cannot redeclare inherited member %q", field.Name)
			}
			class.Fields = append(class.Fields, &object.Field{Declaration: field, Owner: class, Env: env})
		}
		check(node, env.Define(node.Name, class, false))
	case *ast.Return:
		value := object.Value(object.Null{})
		if node.Value != nil {
			value = eval.expression(node.Value, env)
		}
		return &flow{kind: "return", value: value}
	case *ast.Control:
		return &flow{kind: node.Kind}
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
				if result.kind == "break" {
					return nil
				}
				if result.kind == "return" {
					return result
				}
			}
		}
		return eval.block(node.Else, object.NewEnvironment(env))
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
				if result.kind == "break" {
					return nil
				}
				if result.kind == "return" {
					return result
				}
			}
		}
		return eval.block(node.Else, object.NewEnvironment(env))
	case *ast.CFor:
		scope := object.NewEnvironment(env)
		if node.Initializer != nil {
			eval.statement(node.Initializer, scope)
		}
		for node.Condition == nil || object.Truthy(eval.expression(node.Condition, scope)) {
			eval.tick(node)
			if result := eval.block(node.Body, object.NewEnvironment(scope)); result != nil {
				if result.kind == "break" {
					return nil
				}
				if result.kind == "return" {
					return result
				}
			}
			if node.Update != nil {
				eval.statement(node.Update, scope)
			}
		}
		return eval.block(node.Else, object.NewEnvironment(scope))
	case *ast.Switch:
		value := eval.expression(node.Value, env)
		start, fallback := -1, -1
		for index, branch := range node.Cases {
			if branch.Value == nil {
				fallback = index
			} else if object.Equal(value, eval.expression(branch.Value, env)) {
				start = index
				break
			}
		}
		if start < 0 {
			start = fallback
		}
		if start >= 0 {
			scope := object.NewEnvironment(env)
			for _, branch := range node.Cases[start:] {
				if result := eval.block(branch.Body, scope); result != nil {
					if result.kind == "break" {
						return nil
					}
					return result
				}
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
		panic(runtimeError{err: expression.Position().Errorf("maximum evaluation depth exceeded"), fatal: true})
	}
	switch node := expression.(type) {
	case *ast.FormattedString:
		var text strings.Builder
		for _, part := range node.Parts {
			text.WriteString(object.Format(eval.expression(part, env)))
		}
		return object.String(text.String())
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
		return property(node, eval.expression(node.Receiver, env), env)
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
	case *ast.Update:
		read, write := eval.reference(node.Target, env)
		previous, ok := read().(object.Number)
		if !ok {
			fail(node, "%s requires a number", node.Operator)
		}
		updated := previous + 1
		if node.Operator == "--" {
			updated = previous - 1
		}
		if math.IsInf(float64(updated), 0) || math.IsNaN(float64(updated)) {
			fail(node, "non-finite numeric result")
		}
		write(updated)
		if node.Prefix {
			return updated
		}
		return previous
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
		return eval.call(node, function, arguments, env)
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
		"range": rangeValues,
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
