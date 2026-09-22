package evaluator

import (
	"tiger/pkg/ast"
	"tiger/pkg/object"
)

func (eval *Evaluator) reference(target ast.Expr, env *object.Environment) (func() object.Value, func(object.Value)) {
	switch node := target.(type) {
	case *ast.Identifier:
		return func() object.Value {
			value, err := env.Get(node.Name)
			check(node, err)
			return value
		}, func(value object.Value) { check(node, env.Assign(node.Name, value)) }
	case *ast.Property:
		receiver := eval.expression(node.Receiver, env)
		instance, ok := receiver.(*object.Instance)
		if !ok {
			fail(node, "cannot assign a property of %s", receiver.Type())
		}
		return func() object.Value { return property(node, instance, env) }, func(value object.Value) {
			memberAccess(node, instance.Class, node.Name, env, true)
			instance.Fields[node.Name] = value
		}
	case *ast.Index:
		collection := eval.expression(node.Collection, env)
		key := eval.expression(node.Key, env)
		switch typed := collection.(type) {
		case *object.List:
			index := indexAt(node, key, len(typed.Elements))
			return func() object.Value { return typed.Elements[index] }, func(value object.Value) { typed.Elements[index] = value }
		case *object.Dict:
			return func() object.Value {
				value, exists, err := typed.Get(key)
				check(node, err)
				if !exists {
					fail(node, "dictionary key not found: %s", object.Format(key))
				}
				return value
			}, func(value object.Value) { check(node, typed.Set(key, value)) }
		default:
			fail(node, "cannot assign an index of %s", collection.Type())
		}
	}
	fail(target, "invalid assignment target")
	return nil, nil
}
