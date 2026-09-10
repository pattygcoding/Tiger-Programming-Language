package ast

import "tiger/pkg/lexer"

type Node interface {
	Position() lexer.Token
}

type Base struct{ Token lexer.Token }

func (base Base) Position() lexer.Token { return base.Token }

type Expr interface {
	Node
	expression()
}

type Stmt interface {
	Node
	statement()
}

type Program struct{ Statements []Stmt }

type Literal struct {
	Base
	Value any
}

type Identifier struct {
	Base
	Name string
}

type Unary struct {
	Base
	Operator string
	Right    Expr
}

type Binary struct {
	Base
	Left     Expr
	Operator string
	Right    Expr
}

type Call struct {
	Base
	Function  Expr
	Arguments []Expr
}

type Index struct {
	Base
	Collection Expr
	Key        Expr
}

type Property struct {
	Base
	Receiver Expr
	Name     string
}

type Super struct{ Base }

type List struct {
	Base
	Elements []Expr
}

type Pair struct{ Key, Value Expr }

type Dict struct {
	Base
	Pairs []Pair
}

type ExpressionStmt struct {
	Base
	Value Expr
}

type Assign struct {
	Base
	Target   Expr
	Value    Expr
	Constant bool
}

type Function struct {
	Base
	Name       string
	Parameters []string
	Body       []Stmt
}

type Class struct {
	Base
	Name    string
	Parent  *Identifier
	Methods []*Function
}

type Return struct {
	Base
	Value Expr
}

type Branch struct {
	Condition Expr
	Body      []Stmt
}

type If struct {
	Base
	Branches []Branch
	Else     []Stmt
}

type While struct {
	Base
	Condition Expr
	Body      []Stmt
}

type For struct {
	Base
	Name     string
	Iterable Expr
	Body     []Stmt
}

func (*Literal) expression()       {}
func (*Identifier) expression()    {}
func (*Unary) expression()         {}
func (*Binary) expression()        {}
func (*Call) expression()          {}
func (*Index) expression()         {}
func (*Property) expression()      {}
func (*Super) expression()         {}
func (*List) expression()          {}
func (*Dict) expression()          {}
func (*ExpressionStmt) statement() {}
func (*Assign) statement()         {}
func (*Function) statement()       {}
func (*Class) statement()          {}
func (*Return) statement()         {}
func (*If) statement()             {}
func (*While) statement()          {}
func (*For) statement()            {}
