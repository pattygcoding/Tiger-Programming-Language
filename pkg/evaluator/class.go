package evaluator

import (
	"fmt"
	"math"
	"sort"
	"tiger/pkg/ast"
	"tiger/pkg/object"
)

func accessible(node ast.Node, access string, owner *object.Class, env *object.Environment) {
	if access == "" || access == "public" {
		return
	}
	context := env.ClassContext()
	if context == owner {
		return
	}
	if access == "protected" {
		for current := context; current != nil; current = current.Parent {
			if current == owner {
				return
			}
		}
	}
	fail(node, "cannot access %s member of %s", access, owner.Name)
}

func memberAccess(node ast.Node, class *object.Class, name string, env *object.Environment, writing bool) {
	if field := class.FindField(name); field != nil {
		accessible(node, field.Declaration.Access, field.Owner, env)
		if writing && field.Declaration.Constant {
			fail(node, "cannot reassign const field %q", name)
		}
	} else if method := class.FindMethod(name); method != nil {
		accessible(node, method.Function.Declaration.Access, method.Owner, env)
	}
}

func (eval *Evaluator) property(node *ast.Property, receiver object.Value, env *object.Environment) object.Value {
	switch typed := receiver.(type) {
	case object.String:
		if node.Name == "size" || node.Name == "length" {
			return lengthMethod(node.Name, object.Number(len([]rune(string(typed)))))
		}
		if method := conversionMethod(node.Name, typed); method != nil {
			return method
		}
		fail(node, "string has no property %q", node.Name)
	case *object.List:
		if node.Name == "size" || node.Name == "length" {
			return lengthMethod(node.Name, object.Number(len(typed.Elements)))
		}
		if node.Name == "sort" {
			return eval.sortMethod(node, typed, env)
		}
		if method := conversionMethod(node.Name, typed); method != nil {
			return method
		}
		fail(node, "list has no property %q", node.Name)
	case *object.Module:
		if value, exists := typed.Env.GetOwn(node.Name); exists {
			return value
		}
		fail(node, "module %q has no export %q", typed.Name, node.Name)
	case *object.File:
		return fileProperty(node, typed)
	case *object.Instance:
		memberAccess(node, typed.Class, node.Name, env, false)
		if field, exists := typed.Fields[node.Name]; exists {
			return field
		}
		if method := typed.Class.FindMethod(node.Name); method != nil {
			return &object.BoundMethod{Method: method, Receiver: typed}
		}
		if method := conversionMethod(node.Name, typed); method != nil {
			return method
		}
		fail(node, "%s has no property %q", typed.Class.Name, node.Name)
	case *object.Super:
		if typed.Parent == nil {
			fail(node, "super requires a parent class")
		}
		if method := typed.Parent.FindMethod(node.Name); method != nil {
			accessible(node, method.Function.Declaration.Access, method.Owner, env)
			return &object.BoundMethod{Method: method, Receiver: typed.Receiver}
		}
		fail(node, "parent class %s has no method %q", typed.Parent.Name, node.Name)
	default:
		if method := conversionMethod(node.Name, receiver); method != nil {
			return method
		}
		fail(node, "%s has no properties", receiver.Type())
	}
	return object.Null{}
}

func lengthMethod(name string, length object.Number) object.Value {
	return &object.Builtin{Name: name, Call: func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		if len(keywords) != 0 {
			return nil, fmt.Errorf("%s does not accept keyword arguments", name)
		}
		if len(arguments) != 0 {
			return nil, fmt.Errorf("%s expects 0 arguments, got %d", name, len(arguments))
		}
		return length, nil
	}}
}

// sortMethod mirrors Python's list.sort(*, key=None, reverse=False): in place, stable, returns null.
func (eval *Evaluator) sortMethod(node *ast.Property, list *object.List, env *object.Environment) object.Value {
	return &object.Builtin{Name: "sort", Call: func(arguments []object.Value, keywords map[string]object.Value) (object.Value, error) {
		if len(arguments) != 0 {
			return nil, fmt.Errorf("sort takes no positional arguments")
		}
		var key object.Value = object.Null{}
		reverse := false
		for name, value := range keywords {
			switch name {
			case "key":
				key = value
			case "reverse":
				switch flag := value.(type) {
				case object.Bool:
					reverse = bool(flag)
				case object.Number:
					if math.Trunc(float64(flag)) != float64(flag) {
						return nil, fmt.Errorf("sort reverse must be a boolean or integer")
					}
					reverse = flag != 0
				default:
					return nil, fmt.Errorf("sort reverse must be a boolean or integer, got %s", value.Type())
				}
			default:
				return nil, fmt.Errorf("sort does not accept keyword argument %q", name)
			}
		}
		items := append([]object.Value{}, list.Elements...)
		keys := items
		if _, none := key.(object.Null); !none {
			keys = make([]object.Value, len(items))
			for i, item := range items {
				keys[i] = eval.call(node, key, []object.Value{item}, nil, env)
			}
		}
		order := make([]int, len(items))
		for i := range order {
			order[i] = i
		}
		sort.SliceStable(order, func(i, j int) bool {
			left, right := keys[order[i]], keys[order[j]]
			if reverse {
				left, right = right, left
			}
			return lessThan(node, left, right)
		})
		sorted := make([]object.Value, len(items))
		for i, index := range order {
			sorted[i] = items[index]
		}
		list.Elements = sorted
		return object.Null{}, nil
	}}
}

func lessThan(node ast.Node, left, right object.Value) bool {
	switch first := left.(type) {
	case object.Number:
		if second, ok := right.(object.Number); ok {
			return first < second
		}
	case object.String:
		if second, ok := right.(object.String); ok {
			return first < second
		}
	}
	fail(node, "cannot compare %s and %s while sorting", left.Type(), right.Type())
	return false
}

// keyword is one evaluated keyword argument in call order. An ordered slice is
// used instead of a map so that **kwargs preserves the caller's order and
// duplicates can be detected.
type keyword struct {
	name  string
	value object.Value
}

func (eval *Evaluator) call(node ast.Node, function object.Value, arguments []object.Value, keywords []keyword, env *object.Environment) object.Value {
	switch callable := function.(type) {
	case *object.Builtin:
		table := make(map[string]object.Value, len(keywords))
		for _, entry := range keywords {
			table[entry.name] = entry.value
		}
		value, err := callable.Call(arguments, table)
		check(node, err)
		return value
	case *object.Function:
		return eval.invoke(node, callable, arguments, keywords, nil)
	case *object.BoundMethod:
		return eval.invoke(node, callable.Method.Function, arguments, keywords, callable)
	case *object.Super:
		if !callable.Constructor {
			fail(node, "super(...) can only be called inside a constructor")
		}
		if callable.Parent == nil {
			fail(node, "super requires a parent class")
		}
		if constructor := callable.Parent.FindConstructor(); constructor != nil {
			accessible(node, constructor.Function.Declaration.Access, constructor.Owner, env)
		}
		eval.construct(node, callable.Parent, callable.Receiver, arguments, keywords)
		return object.Null{}
	case *object.Class:
		instance := &object.Instance{Class: callable, Fields: map[string]object.Value{}}
		if constructor := callable.FindConstructor(); constructor != nil {
			accessible(node, constructor.Function.Declaration.Access, constructor.Owner, env)
		}
		eval.initializeFields(instance, callable)
		eval.construct(node, callable, instance, arguments, keywords)
		return instance
	default:
		fail(node, "%s is not callable", function.Type())
	}
	return object.Null{}
}

func (eval *Evaluator) construct(node ast.Node, class *object.Class, instance *object.Instance, arguments []object.Value, keywords []keyword) {
	constructor := class.FindConstructor()
	if constructor == nil {
		if len(arguments) != 0 || len(keywords) != 0 {
			fail(node, "%s expects 0 arguments; it declares no constructor", class.Name)
		}
		return
	}
	eval.invoke(node, constructor.Function, arguments, keywords, &object.BoundMethod{Method: constructor, Receiver: instance})
}

func (eval *Evaluator) invoke(node ast.Node, function *object.Function, arguments []object.Value, keywords []keyword, bound *object.BoundMethod) object.Value {
	declaration := function.Declaration
	parameters := declaration.Parameters
	fixed := len(parameters)
	if len(arguments) > fixed && declaration.Variadic == "" {
		fail(node, "%s expects %d arguments, got %d", declaration.Name, fixed, len(arguments))
	}
	if len(arguments) < fixed && declaration.Variadic == "" && len(keywords) == 0 {
		fail(node, "%s expects %d arguments, got %d", declaration.Name, fixed, len(arguments))
	}
	scope := object.NewEnvironment(function.Env)
	if bound != nil {
		scope.AccessClass = bound.Method.Owner
		check(node, scope.Define("this", bound.Receiver, true))
		check(node, scope.Define("super", &object.Super{Parent: bound.Method.Owner.Parent, Receiver: bound.Receiver, Constructor: bound.Method.Constructor}, true))
	}
	assigned := make(map[string]bool, fixed)
	for index, parameter := range parameters {
		if index >= len(arguments) {
			break
		}
		check(node, scope.Define(parameter, arguments[index], false))
		assigned[parameter] = true
	}
	if declaration.Variadic != "" {
		extra := []object.Value{}
		if len(arguments) > fixed {
			extra = append(extra, arguments[fixed:]...)
		}
		check(node, scope.Define(declaration.Variadic, &object.List{Elements: extra}, false))
	}
	collected := &object.Dict{}
	for _, entry := range keywords {
		if contains(parameters, entry.name) {
			if assigned[entry.name] {
				fail(node, "%s got multiple values for argument %q", declaration.Name, entry.name)
			}
			check(node, scope.Define(entry.name, entry.value, false))
			assigned[entry.name] = true
			continue
		}
		if declaration.Keyword == "" {
			fail(node, "%s got an unexpected keyword argument %q", declaration.Name, entry.name)
		}
		if _, exists, _ := collected.Get(object.String(entry.name)); exists {
			fail(node, "%s got multiple values for keyword argument %q", declaration.Name, entry.name)
		}
		check(node, collected.Set(object.String(entry.name), entry.value))
	}
	for _, parameter := range parameters {
		if !assigned[parameter] {
			fail(node, "%s missing required argument %q", declaration.Name, parameter)
		}
	}
	if declaration.Keyword != "" {
		check(node, scope.Define(declaration.Keyword, collected, false))
	}
	if result := eval.block(declaration.Body, scope); result != nil {
		return result.value
	}
	return object.Null{}
}

func contains(names []string, name string) bool {
	for _, candidate := range names {
		if candidate == name {
			return true
		}
	}
	return false
}

func (eval *Evaluator) initializeFields(instance *object.Instance, class *object.Class) {
	if class.Parent != nil {
		eval.initializeFields(instance, class.Parent)
	}
	for _, field := range class.Fields {
		scope := object.NewEnvironment(field.Env)
		scope.AccessClass = class
		check(field.Declaration, scope.Define("this", instance, true))
		check(field.Declaration, scope.Define("super", &object.Super{Parent: class.Parent, Receiver: instance}, true))
		var value object.Value = object.Null{}
		if field.Declaration.Value != nil {
			value = eval.expression(field.Declaration.Value, scope)
		}
		instance.Fields[field.Declaration.Name] = value
	}
}
