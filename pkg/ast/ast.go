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

// Interpolation is one "{expression[:spec]}" field of a formatted string.
type Interpolation struct {
	Base
	Value Expr
	Spec  string
}

// Conditional is the "value if condition else fallback" expression. Only the
// selected branch is evaluated.
type Conditional struct {
	Base
	Condition Expr
	Then      Expr
	Else      Expr
}

// Slice is "collection[start:stop:step]" on a list or a string. A nil bound
// means "the default for this step".
type Slice struct {
	Base
	Collection Expr
	Start      Expr
	Stop       Expr
	Step       Expr
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
	Arguments []Argument
	Keywords  []KeywordArgument
}

// Argument is one positional slot. Spread marks a star argument, "*value".
type Argument struct {
	Value  Expr
	Spread bool
}

// KeywordArgument is one keyword slot. A non-spread entry is "name=value";
// a spread entry is "**value".
type KeywordArgument struct {
	Name   string
	Value  Expr
	Spread bool
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
	Operator    string
	Constant    bool
	Declaration bool
}

type Function struct {
	Base
	Access     string
	Name       string
	Parameters []string
	// Defaults runs parallel to Parameters; a nil entry means the parameter is
	// required, and a non-nil entry is the default value expression.
	Defaults []Expr
	Variadic string
	Keyword  string
	Body     []Stmt
}

// Destructure binds several names from a list or dictionary value, as in
// "const [a, b] = pair();" or "var {name, age} = record;". Lookup selects
// dictionary keys instead of list positions.
type Destructure struct {
	Base
	Names    []string
	Constant bool
	Value    Expr
	Lookup   bool
}

func (*Destructure) statement() {}

type Class struct {
	Base
	Name        string
	Parent      *Identifier
	Constructor *Function
	Methods     []*Function
	Fields      []*Field
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
	Label     string
	Condition Expr
	Body      []Stmt
	Else      []Stmt
}

type For struct {
	Base
	Label    string
	Name     string
	Iterable Expr
	Body     []Stmt
	Else     []Stmt
}

type CFor struct {
	Base
	Label       string
	Initializer Stmt
	Condition   Expr
	Update      Stmt
	Body        []Stmt
	Else        []Stmt
}

// DoWhile runs its body once and then repeats for as long as its condition
// holds.
type DoWhile struct {
	Base
	Label     string
	Condition Expr
	Body      []Stmt
}

type Control struct {
	Base
	Kind  string
	Label string
}

type Throw struct {
	Base
	Value Expr
}

type Try struct {
	Base
	Body    []Stmt
	Catches []Catch
	Finally []Stmt
}

// Catch is one catch clause. A nil Condition matches every error; otherwise the
// clause matches only when the condition is truthy after binding Name.
type Catch struct {
	Name      string
	Condition Expr
	Body      []Stmt
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
func (*Interpolation) expression()   {}
func (*Conditional) expression()     {}
func (*Slice) expression()           {}
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
func (*Function) expression()        {}
func (*Class) statement()            {}
func (*Return) statement()           {}
func (*If) statement()               {}
func (*While) statement()            {}
func (*For) statement()              {}
func (*CFor) statement()             {}
func (*DoWhile) statement()          {}
func (*Control) statement()          {}
func (*Switch) statement()           {}
func (*Throw) statement()            {}
func (*Try) statement()              {}
