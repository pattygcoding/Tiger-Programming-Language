package object

import "fmt"

type binding struct {
	value    Value
	constant bool
}

type Environment struct {
	parent   *Environment
	bindings map[string]binding
}

func NewEnvironment(parent *Environment) *Environment {
	return &Environment{parent: parent, bindings: map[string]binding{}}
}

func (env *Environment) Define(name string, value Value, constant bool) error {
	if _, exists := env.bindings[name]; exists {
		return fmt.Errorf("%q is already declared in this scope", name)
	}
	env.bindings[name] = binding{value: value, constant: constant}
	return nil
}

func (env *Environment) resolve(name string) *Environment {
	for current := env; current != nil; current = current.parent {
		if _, exists := current.bindings[name]; exists {
			return current
		}
	}
	return nil
}

func (env *Environment) Get(name string) (Value, error) {
	if owner := env.resolve(name); owner != nil {
		return owner.bindings[name].value, nil
	}
	return nil, fmt.Errorf("undefined name %q", name)
}

func (env *Environment) Assign(name string, value Value) error {
	owner := env.resolve(name)
	if owner == nil {
		return env.Define(name, value, false)
	}
	if owner.bindings[name].constant {
		return fmt.Errorf("cannot reassign const %q", name)
	}
	owner.bindings[name] = binding{value: value}
	return nil
}
