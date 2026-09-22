package ast

func Imports(program *Program) []*Import {
	var imports []*Import
	var visit func([]Stmt)
	visit = func(statements []Stmt) {
		for _, statement := range statements {
			switch node := statement.(type) {
			case *Import:
				imports = append(imports, node)
			case *Function:
				visit(node.Body)
			case *Class:
				for _, method := range node.Methods {
					visit(method.Body)
				}
			case *If:
				for _, branch := range node.Branches {
					visit(branch.Body)
				}
				visit(node.Else)
			case *While:
				visit(node.Body)
				visit(node.Else)
			case *For:
				visit(node.Body)
				visit(node.Else)
			case *CFor:
				visit(node.Body)
				visit(node.Else)
			case *Switch:
				for _, branch := range node.Cases {
					visit(branch.Body)
				}
			case *Try:
				visit(node.Body)
				visit(node.Catch)
			}
		}
	}
	visit(program.Statements)
	return imports
}
