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

type Import struct {
	Base
	Path string
	Name string
}

func (*Import) statement() {}

type Literal struct {
	Base
	Value any
}

type FormattedString struct {
	Base
	Parts []Expr
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

type Update struct {
	Base
	Target   Expr
	Operator string
	Prefix   bool
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
	Target      Expr
	Value       Expr
	Constant    bool
	Declaration bool
}

type Function struct {
	Base
	Access     string
	Name       string
	Parameters []string
	Body       []Stmt
}

type Class struct {
	Base
	Name    string
	Parent  *Identifier
	Methods []*Function
	Fields  []*Field
}

type Field struct {
	Base
	Name     string
	Access   string
	Constant bool
	Value    Expr
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
	Else      []Stmt
}

type For struct {
	Base
	Name     string
	Iterable Expr
	Body     []Stmt
	Else     []Stmt
}

type CFor struct {
	Base
	Initializer Stmt
	Condition   Expr
	Update      Stmt
	Body        []Stmt
	Else        []Stmt
}

type Control struct {
	Base
	Kind string
}

type Throw struct {
	Base
	Value Expr
}

type Try struct {
	Base
	Body  []Stmt
	Name  string
	Catch []Stmt
}

type Case struct {
	Value Expr
	Body  []Stmt
}

type Switch struct {
	Base
	Value Expr
	Cases []Case
}

func (*Literal) expression()         {}
func (*FormattedString) expression() {}
func (*Identifier) expression()      {}
func (*Unary) expression()           {}
func (*Update) expression()          {}
func (*Binary) expression()          {}
func (*Call) expression()            {}
func (*Index) expression()           {}
func (*Property) expression()        {}
func (*Super) expression()           {}
func (*List) expression()            {}
func (*Dict) expression()            {}
func (*ExpressionStmt) statement()   {}
func (*Assign) statement()           {}
func (*Function) statement()         {}
func (*Class) statement()            {}
func (*Return) statement()           {}
func (*If) statement()               {}
func (*While) statement()            {}
func (*For) statement()              {}
func (*CFor) statement()             {}
func (*Control) statement()          {}
func (*Switch) statement()           {}
func (*Throw) statement()            {}
func (*Try) statement()              {}
