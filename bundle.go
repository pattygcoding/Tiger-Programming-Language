package tiger

import "embed"

//go:embed go.mod pkg/ast/*.go pkg/lexer/*.go pkg/parser/*.go pkg/object/*.go pkg/evaluator/*.go
var RuntimeSources embed.FS
