package object

import "tiger/pkg/ast"

type Class struct {
	Name    string
	Parent  *Class
	Methods map[string]*Method
	Fields  []*Field
}

type Field struct {
	Declaration *ast.Field
	Owner       *Class
	Env         *Environment
}

type Method struct {
	Function *Function
	Owner    *Class
}

type Instance struct {
	Class  *Class
	Fields map[string]Value
}

type BoundMethod struct {
	Method   *Method
	Receiver *Instance
}

type Super struct {
	Parent   *Class
	Receiver *Instance
}

func (*Class) Type() string       { return "class" }
func (*Instance) Type() string    { return "instance" }
func (*BoundMethod) Type() string { return "bound method" }
func (*Super) Type() string       { return "super" }

func (class *Class) FindMethod(name string) *Method {
	for current := class; current != nil; current = current.Parent {
		if method, exists := current.Methods[name]; exists {
			return method
		}
	}
	return nil
}

func (class *Class) FindField(name string) *Field {
	for current := class; current != nil; current = current.Parent {
		for _, field := range current.Fields {
			if field.Declaration.Name == name {
				return field
			}
		}
	}
	return nil
}
