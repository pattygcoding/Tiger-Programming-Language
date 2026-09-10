package evaluator

import (
	"tiger/pkg/ast"
	"tiger/pkg/object"
)

func property(node *ast.Property, receiver object.Value) object.Value {
	switch typed := receiver.(type) {
	case *object.Instance:
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
			return &object.BoundMethod{Method: method, Receiver: typed.Receiver}
		}
		fail(node, "parent class %s has no method %q", typed.Parent.Name, node.Name)
	default:
		fail(node, "%s has no properties", receiver.Type())
	}
	return object.Null{}
}

func (eval *Evaluator) call(node ast.Node, function object.Value, arguments []object.Value) object.Value {
	switch callable := function.(type) {
	case *object.Builtin:
		value, err := callable.Call(arguments)
		check(node, err)
		return value
	case *object.Function:
		return eval.invoke(node, callable, arguments, nil)
	case *object.BoundMethod:
		return eval.invoke(node, callable.Method.Function, arguments, callable)
	case *object.Class:
		instance := &object.Instance{Class: callable, Fields: map[string]object.Value{}}
		if initializer := callable.FindMethod("init"); initializer != nil {
			eval.call(node, &object.BoundMethod{Method: initializer, Receiver: instance}, arguments)
		} else if len(arguments) != 0 {
			fail(node, "%s expects 0 arguments, got %d", callable.Name, len(arguments))
		}
		return instance
	default:
		fail(node, "%s is not callable", function.Type())
	}
	return object.Null{}
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
		check(node, scope.Define("self", bound.Receiver, false))
		check(node, scope.Define("super", &object.Super{Parent: bound.Method.Owner.Parent, Receiver: bound.Receiver}, true))
	}
	for index, parameter := range parameters {
		check(node, scope.Define(parameter, arguments[index], false))
	}
	if result := eval.block(declaration.Body, scope); result != nil {
		return result.Value
	}
	return object.Null{}
}
