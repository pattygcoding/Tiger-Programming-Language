package evaluator

import (
	"fmt"
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

func property(node *ast.Property, receiver object.Value, env *object.Environment) object.Value {
	switch typed := receiver.(type) {
	case object.String:
		if node.Name == "size" || node.Name == "length" {
			return lengthMethod(node.Name, object.Number(len([]rune(string(typed)))))
		}
		fail(node, "string has no property %q", node.Name)
	case *object.List:
		if node.Name == "size" || node.Name == "length" {
			return lengthMethod(node.Name, object.Number(len(typed.Elements)))
		}
		fail(node, "list has no property %q", node.Name)
	case *object.Module:
		if value, exists := typed.Env.GetOwn(node.Name); exists {
			return value
		}
		fail(node, "module %q has no export %q", typed.Name, node.Name)
	case *object.Instance:
		memberAccess(node, typed.Class, node.Name, env, false)
		if field, exists := typed.Fields[node.Name]; exists {
			return field
		}
		if method := typed.Class.FindMethod(node.Name); method != nil {
			return &object.BoundMethod{Method: method, Receiver: typed}
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

func (eval *Evaluator) call(node ast.Node, function object.Value, arguments []object.Value, keywords map[string]object.Value, env *object.Environment) object.Value {
	switch callable := function.(type) {
	case *object.Builtin:
		value, err := callable.Call(arguments, keywords)
		check(node, err)
		return value
	case *object.Function:
		rejectKeywords(node, keywords)
		return eval.invoke(node, callable, arguments, nil)
	case *object.BoundMethod:
		rejectKeywords(node, keywords)
		return eval.invoke(node, callable.Method.Function, arguments, callable)
	case *object.Class:
		rejectKeywords(node, keywords)
		instance := &object.Instance{Class: callable, Fields: map[string]object.Value{}}
		if initializer := callable.FindMethod("init"); initializer != nil {
			accessible(node, initializer.Function.Declaration.Access, initializer.Owner, env)
		}
		eval.initializeFields(instance, callable)
		if initializer := callable.FindMethod("init"); initializer != nil {
			eval.call(node, &object.BoundMethod{Method: initializer, Receiver: instance}, arguments, nil, env)
		} else if len(arguments) != 0 {
			fail(node, "%s expects 0 arguments, got %d", callable.Name, len(arguments))
		}
		return instance
	default:
		fail(node, "%s is not callable", function.Type())
	}
	return object.Null{}
}

func rejectKeywords(node ast.Node, keywords map[string]object.Value) {
	if len(keywords) != 0 {
		fail(node, "keyword arguments are only supported by built-in functions")
	}
}

func (eval *Evaluator) invoke(node ast.Node, function *object.Function, arguments []object.Value, bound *object.BoundMethod) object.Value {
	declaration := function.Declaration
	parameters := declaration.Parameters
	if bound != nil {
		parameters = parameters[1:]
	}
	if len(arguments) != len(parameters) {
		fail(node, "%s expects %d arguments, got %d", declaration.Name, len(parameters), len(arguments))
	}
	scope := object.NewEnvironment(function.Env)
	if bound != nil {
		scope.AccessClass = bound.Method.Owner
		check(node, scope.Define("this", bound.Receiver, true))
		check(node, scope.Define("super", &object.Super{Parent: bound.Method.Owner.Parent, Receiver: bound.Receiver}, true))
	}
	for index, parameter := range parameters {
		check(node, scope.Define(parameter, arguments[index], false))
	}
	if result := eval.block(declaration.Body, scope); result != nil {
		return result.value
	}
	return object.Null{}
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
		instance.Fields[field.Declaration.Name] = eval.expression(field.Declaration.Value, scope)
	}
}
