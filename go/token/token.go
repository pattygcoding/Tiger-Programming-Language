package token

type TokenType string

type Token struct {
	Type    TokenType
	Literal string
}

const (
	ILLEGAL = "ILLEGAL"
	EOF     = "EOF"

	// Identifiers + literals
	IDENT  = "IDENT"
	INT    = "INT"
	FLOAT  = "FLOAT"
	STRING = "STRING"

	// Operators
	ASSIGN   = "="
	PLUS     = "+"
	MINUS    = "-"
	ASTERISK = "*"
	SLASH    = "/"

	EQ     = "=="
	NOT_EQ = "!="
	LT     = "<"
	LTE    = "<="
	GT     = ">"
	GTE    = ">="

	// Delimiters
	COMMA     = ","
	SEMICOLON = ";"
	LPAREN    = "("
	RPAREN    = ")"
	LBRACE    = "{"
	RBRACE    = "}"

	// Keywords
	LET   = "LET"
	CONST = "CONST"
	PRINT = "PRINT"
	IF    = "IF"
	ELSE  = "ELSE"
	WHILE = "WHILE"
	FOR   = "FOR"
	TRUE  = "TRUE"
	FALSE = "FALSE"

	FUNC   = "FUNC"
	CLASS  = "CLASS"
	RETURN = "RETURN"
)

var keywords = map[string]TokenType{
	"let":    LET,
	"const":  CONST,
	"print":  PRINT,
	"if":     IF,
	"else":   ELSE,
	"while":  WHILE,
	"for":    FOR,
	"true":   TRUE,
	"false":  FALSE,
	"func":   FUNC,
	"class":  CLASS,
	"return": RETURN,
}

func LookupIdent(ident string) TokenType {
	if tok, ok := keywords[ident]; ok {
		return tok
	}
	return IDENT
}
