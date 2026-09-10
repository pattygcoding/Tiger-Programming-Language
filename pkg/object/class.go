package object

type Class struct {
	Name    string
	Parent  *Class
	Methods map[string]*Method
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
