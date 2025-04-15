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
	PRINT = "PRINT"
	IF    = "IF"
	ELSE  = "ELSE"
	WHILE = "WHILE"
	TRUE  = "TRUE"
	FALSE = "FALSE"

	FUNC = "FUNC"
)

var keywords = map[string]TokenType{
	"let":   LET,
	"print": PRINT,
	"if":    IF,
	"else":  ELSE,
	"while": WHILE,
	"true":  TRUE,
	"false": FALSE,
	"func":  FUNC,
}

func LookupIdent(ident string) TokenType {
	if tok, ok := keywords[ident]; ok {
		return tok
	}
	return IDENT
}
