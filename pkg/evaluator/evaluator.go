package evaluator

import (
	"io"
	"math"
	"strings"

	"tiger/pkg/ast"
	"tiger/pkg/format"
	"tiger/pkg/object"
	"tiger/pkg/parser"
)

type runtimeError struct {
	value *object.Error
	fatal bool
}

type thrown struct {
	node  ast.Node
	value object.Value
}

type flow struct {
	kind  string
	label string
	value object.Value
}

// loopControl interprets a flow value that reached a loop body. It reports
// whether the loop must stop and, when the flow targets an outer loop, the flow
// to keep propagating.
func loopControl(label string, result *flow) (stop bool, propagate *flow) {
	switch result.kind {
	case "break":
		if result.label == "" || result.label == label {
			return true, nil
		}
	case "continue":
		if result.label == "" || result.label == label {
			return false, nil
		}
	}
	return false, result
}

type Evaluator struct {
	Output      io.Writer
	Input       io.Reader
	MaxSteps    int
	Loader      ModuleLoader
	FileSystem  FileSystem
	SourcePath  string
	reader      *bufferedReader
	readerFrom  io.Reader
	modules     map[string]*object.Module
	loading     map[string]bool
	importDepth int
	steps       int
	depth       int
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
				err = format.Errorf("%s", failure.value.Text())
			case thrown:
				err = failure.node.Position().Errorf("uncaught throw: %s", object.Format(failure.value))
			default:
				panic(failure)
			}
		}
	}()
	eval.steps, eval.depth = 0, 0
	eval.modules = map[string]*object.Module{}
	eval.loading = map[string]bool{}
	eval.importDepth = 0
	if eval.SourcePath != "" {
		eval.loading[eval.SourcePath] = true
	}
	if eval.Output == nil {
		eval.Output = io.Discard
	}
	builtins := object.NewEnvironment(nil)
	eval.builtins(builtins)
	env := object.NewEnvironment(builtins)
	env.SourcePath = eval.SourcePath
	eval.block(program.Statements, env)
	return nil
}

func fail(node ast.Node, template string, args ...any) {
	panic(runtimeError{value: newRuntimeError(node, format.Sprintf(template, args...))})
}

// newRuntimeError builds a structured failure with a classified kind.
func newRuntimeError(node ast.Node, message string) *object.Error {
	return &object.Error{Message: message, Kind: errorKind(message), Origin: node.Position().Location()}
}

// errorKind classifies a diagnostic once, where it is raised, so a handler can
// dispatch on a stable kind instead of matching message text. The first marker
// that appears wins; unmatched failures are "runtime error".
func errorKind(message string) string {
	for _, entry := range errorKinds {
		if strings.Contains(message, entry.marker) {
			return entry.kind
		}
	}
	return "runtime error"
}

var errorKinds = []struct{ marker, kind string }{
	{"division by zero", "zero division"},
	{"domain error", "math domain"},
	{"range error", "math range"},
	{"non-finite numeric result", "numeric range"},
	{"cannot reassign const", "constant reassignment"},
	{"already declared", "duplicate declaration"},
	{"undefined name", "undefined name"},
	{"index out of range", "index range"},
	{"index must be an integer", "index type"},
	{"dictionary key not found", "missing key"},
	{"pop: key not found", "missing key"},
	{"cannot be indexed", "not indexable"},
	{"is not supported for", "unsupported operands"},
	{"no such file", "missing file"},
	{"file does not exist", "missing file"},
}

func check(node ast.Node, err error) {
	if err != nil {
		fail(node, "%s", err)
	}
}

func (eval *Evaluator) tick(node ast.Node) {
	eval.steps++
	if eval.MaxSteps > 0 && eval.steps > eval.MaxSteps {
		panic(runtimeError{value: newRuntimeError(node, "execution step limit exceeded"), fatal: true})
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
	case *ast.Import:
		module := eval.importModule(node, env)
		check(node, env.Define(node.Name, module, true))
	case *ast.Throw:
		panic(thrown{node: node, value: eval.expression(node.Value, env)})
	case *ast.Try:
		return eval.tryStatement(node, env)
	case *ast.Destructure:
		return eval.destructure(node, env)
	case *ast.ExpressionStmt:
		eval.expression(node.Value, env)
	case *ast.Assign:
		if node.Declaration || node.Constant {
			value := eval.expression(node.Value, env)
			check(node, env.Define(node.Target.(*ast.Identifier).Name, value, node.Constant))
		} else if node.Operator != "=" {
			read, write := eval.reference(node.Target, env)
			left := read()
			right := eval.expression(node.Value, env)
			write(binary(&ast.Binary{Base: node.Base, Operator: strings.TrimSuffix(node.Operator, "=")}, left, right))
		} else {
			value := eval.expression(node.Value, env)
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
		if node.Constructor != nil {
			class.Constructor = &object.Method{
				Function:    &object.Function{Declaration: node.Constructor, Env: env},
				Owner:       class,
				Constructor: true,
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
		return &flow{kind: node.Kind, label: node.Label}
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
				stop, propagate := loopControl(node.Label, result)
				if propagate != nil {
					return propagate
				}
				if stop {
					return nil
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
				stop, propagate := loopControl(node.Label, result)
				if propagate != nil {
					return propagate
				}
				if stop {
					return nil
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
				stop, propagate := loopControl(node.Label, result)
				if propagate != nil {
					return propagate
				}
				if stop {
					return nil
				}
			}
			if node.Update != nil {
				eval.statement(node.Update, scope)
			}
		}
		return eval.block(node.Else, object.NewEnvironment(scope))
	case *ast.DoWhile:
		for {
			eval.tick(node)
			if result := eval.block(node.Body, object.NewEnvironment(env)); result != nil {
				stop, propagate := loopControl(node.Label, result)
				if propagate != nil {
					return propagate
				}
				if stop {
					return nil
				}
			}
			if !object.Truthy(eval.expression(node.Condition, env)) {
				return nil
			}
		}
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
					if result.kind == "break" && result.label == "" {
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
		panic(runtimeError{value: newRuntimeError(expression, "maximum evaluation depth exceeded"), fatal: true})
	}
	switch node := expression.(type) {
	case *ast.FormattedString:
		var text strings.Builder
		for _, part := range node.Parts {
			if field, ok := part.(*ast.Interpolation); ok {
				text.WriteString(applyFormat(node, field.Spec, eval.expression(field.Value, env)))
				continue
			}
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
		return eval.property(node, eval.expression(node.Receiver, env), env)
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
	case *ast.Function:
		// An anonymous function expression captures its defining environment.
		return &object.Function{Declaration: node, Env: env}
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
		if node.Operator == "~" {
			return object.Number(float64(^bitwiseInteger(node, value)))
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
	case *ast.Conditional:
		if object.Truthy(eval.expression(node.Condition, env)) {
			return eval.expression(node.Then, env)
		}
		return eval.expression(node.Else, env)
	case *ast.Slice:
		collection := eval.expression(node.Collection, env)
		switch typed := collection.(type) {
		case *object.List:
			indices := sliceIndices(eval.sliceBounds(node, len(typed.Elements), env))
			elements := make([]object.Value, len(indices))
			for position, index := range indices {
				elements[position] = typed.Elements[index]
			}
			return &object.List{Elements: elements}
		case object.String:
			runes := []rune(string(typed))
			indices := sliceIndices(eval.sliceBounds(node, len(runes), env))
			var text strings.Builder
			for _, index := range indices {
				text.WriteRune(runes[index])
			}
			return object.String(text.String())
		default:
			fail(node, "%s cannot be sliced", collection.Type())
		}
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
		case *object.Error:
			return errorField(node, typed, key)
		default:
			fail(node, "%s cannot be indexed", collection.Type())
		}
	case *ast.Call:
		function := eval.expression(node.Function, env)
		arguments := make([]object.Value, 0, len(node.Arguments))
		for _, argument := range node.Arguments {
			value := eval.expression(argument.Value, env)
			if !argument.Spread {
				arguments = append(arguments, value)
				continue
			}
			list, ok := value.(*object.List)
			if !ok {
				fail(node, "cannot unpack %s in a call; expected a list", value.Type())
			}
			arguments = append(arguments, list.Elements...)
		}
		keywords := make([]keyword, 0, len(node.Keywords))
		seen := map[string]bool{}
		for _, entry := range node.Keywords {
			value := eval.expression(entry.Value, env)
			if !entry.Spread {
				if seen[entry.Name] {
					fail(node, "duplicate keyword argument %q", entry.Name)
				}
				seen[entry.Name] = true
				keywords = append(keywords, keyword{name: entry.Name, value: value})
				continue
			}
			dict, ok := value.(*object.Dict)
			if !ok {
				fail(node, "cannot unpack %s in a call; expected a dictionary", value.Type())
			}
			for _, pair := range dict.Entries {
				name, ok := pair.Key.(object.String)
				if !ok {
					fail(node, "keyword argument names must be strings, got %s", pair.Key.Type())
				}
				if seen[string(name)] {
					fail(node, "duplicate keyword argument %q", string(name))
				}
				seen[string(name)] = true
				keywords = append(keywords, keyword{name: string(name), value: pair.Value})
			}
		}
		return eval.call(node, function, arguments, keywords, env)
	}
	fail(expression, "unsupported expression")
	return object.Null{}
}

// errorField reads one structured field from an Error value.
func errorField(node ast.Node, failure *object.Error, key object.Value) object.Value {
	name, ok := key.(object.String)
	if !ok {
		fail(node, "error fields must be read with a string key, got %s", key.Type())
	}
	switch string(name) {
	case "message":
		return object.String(failure.Message)
	case "kind":
		return object.String(failure.Kind)
	case "origin":
		return object.String(failure.Origin)
	}
	fail(node, "error has no field %q", string(name))
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

// sliceBounds resolves the start, stop, and step of a slice against a length,
// following Python: omitted bounds take the default for the step, negatives
// count from the end, and out-of-range bounds clamp instead of failing.
func (eval *Evaluator) sliceBounds(node *ast.Slice, length int, env *object.Environment) (int, int, int) {
	step := 1
	if node.Step != nil {
		step = sliceInteger(node, eval.expression(node.Step, env))
		if step == 0 {
			fail(node, "slice step cannot be zero")
		}
	}
	low, high := 0, length
	start, stop := 0, length
	if step < 0 {
		low, high = -1, length-1
		start, stop = high, low
	}
	if node.Start != nil {
		start = sliceInteger(node, eval.expression(node.Start, env))
		if start < 0 {
			start += length
		}
		if start < low {
			start = low
		} else if start > high {
			start = high
		}
	}
	if node.Stop != nil {
		stop = sliceInteger(node, eval.expression(node.Stop, env))
		if stop < 0 {
			stop += length
		}
		if stop < low {
			stop = low
		} else if stop > high {
			stop = high
		}
	}
	return start, stop, step
}

// sliceInteger reads one slice bound, clamping values too large to convert.
func sliceInteger(node ast.Node, value object.Value) int {
	number, ok := value.(object.Number)
	if !ok || math.Trunc(float64(number)) != float64(number) {
		fail(node, "slice bounds must be integers")
	}
	switch {
	case number <= -9.3e18:
		return math.MinInt64
	case number >= 9.3e18:
		return math.MaxInt64
	}
	return int(number)
}

// sliceIndices lists the positions a slice selects, in order.
func sliceIndices(start, stop, step int) []int {
	indices := []int{}
	if step > 0 {
		for index := start; index < stop; index += step {
			indices = append(indices, index)
		}
		return indices
	}
	for index := start; index > stop; index += step {
		indices = append(indices, index)
	}
	return indices
}

// bitwiseInteger truncates a number toward zero for the bitwise operators.
func bitwiseInteger(node ast.Node, value object.Number) int64 {
	const limit = 9223372036854775808.0 // 2^63
	number := float64(value)
	if math.IsNaN(number) || math.IsInf(number, 0) || number < -limit || number >= limit {
		fail(node, "bitwise operands must be within the 64-bit integer range")
	}
	return int64(number)
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
		case *object.Error:
			if text, ok := left.(object.String); ok {
				return object.Bool(strings.Contains(collection.Text(), string(text)))
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
			case "/", "%", "div":
				if second == 0 {
					fail(node, "division by zero")
				}
				switch node.Operator {
				case "/":
					result = first / second
				case "div":
					result = object.Number(math.Floor(float64(first) / float64(second)))
				default:
					result = object.Number(math.Mod(float64(first), float64(second)))
					if result != 0 && (result < 0) != (second < 0) {
						result += second
					}
				}
			case "**":
				result = object.Number(math.Pow(float64(first), float64(second)))
			case "&", "|", "^", "<<", ">>":
				switch node.Operator {
				case "&":
					result = object.Number(float64(bitwiseInteger(node, first) & bitwiseInteger(node, second)))
				case "|":
					result = object.Number(float64(bitwiseInteger(node, first) | bitwiseInteger(node, second)))
				case "^":
					result = object.Number(float64(bitwiseInteger(node, first) ^ bitwiseInteger(node, second)))
				default:
					count := bitwiseInteger(node, second)
					if count < 0 {
						fail(node, "shift count must not be negative")
					}
					value := bitwiseInteger(node, first)
					if node.Operator == "<<" {
						result = object.Number(float64(value << uint(count)))
					} else {
						result = object.Number(float64(value >> uint(count)))
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
	functions := map[string]func([]object.Value, map[string]object.Value) (object.Value, error){
		"range": func(args []object.Value, keywords map[string]object.Value) (object.Value, error) {
			if len(keywords) != 0 {
				return nil, format.Errorf("range does not accept keyword arguments")
			}
			return rangeValues(args)
		},
		"print": func(args []object.Value, keywords map[string]object.Value) (object.Value, error) {
			ending := "\n"
			for name, value := range keywords {
				if name != "end" {
					return nil, format.Errorf("print does not accept keyword argument %q", name)
				}
				text, ok := value.(object.String)
				if !ok {
					return nil, format.Errorf("print end must be a string, got %s", value.Type())
				}
				ending = string(text)
			}
			parts := make([]string, len(args))
			for index, value := range args {
				parts[index] = object.Format(value)
			}
			_, err := io.WriteString(eval.Output, strings.Join(parts, " ")+ending)
			return object.Null{}, err
		},
		"len": func(args []object.Value, keywords map[string]object.Value) (object.Value, error) {
			if len(keywords) != 0 {
				return nil, format.Errorf("len does not accept keyword arguments")
			}
			if len(args) != 1 {
				return nil, format.Errorf("len expects 1 argument, got %d", len(args))
			}
			switch value := args[0].(type) {
			case object.String:
				return object.Number(len([]rune(string(value)))), nil
			case *object.List:
				return object.Number(len(value.Elements)), nil
			case *object.Dict:
				return object.Number(len(value.Entries)), nil
			default:
				return nil, format.Errorf("len does not accept %s", value.Type())
			}
		},
		"type":       typeBuiltin,
		"isinstance": isinstanceBuiltin,
		"graphemes":  graphemesBuiltin,
	}
	for name, function := range eval.fileBuiltins() {
		functions[name] = function
	}
	for name, function := range aggregateBuiltins() {
		functions[name] = function
	}
	functions["input"] = eval.input
	for name, function := range conversionBuiltins() {
		functions[name] = function
	}
	for name, function := range functions {
		_ = env.Define(name, &object.Builtin{Name: name, Call: function}, true)
	}
	_ = env.Define("math", mathModule(), true)
	_ = env.Define("algo", algoModule(), true)
	for _, name := range []string{"sorted", "map", "filter"} {
		name := name
		_ = env.Define(name, &object.Builtin{Name: name, HigherOrder: true, Call: func([]object.Value, map[string]object.Value) (object.Value, error) {
			return nil, format.Errorf("%s is dispatched by the evaluator", name)
		}}, true)
	}
	_ = env.Define("random", randomModule(), true)
	_ = env.Define("json", jsonModule(), true)
	_ = env.Define("time", timeModule(), true)
	_ = env.Define("regex", regexModule(), true)
}
