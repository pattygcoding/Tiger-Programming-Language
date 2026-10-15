//! Pratt parser for the Tiger language.
//!
//! A port of `pkg/parser`'s precedence table and diagnostics, with `if`, `else`,
//! and `for` accepting both the Python-style and the C-style header, `&&`, `||`,
//! and `!` accepted as spellings of `and`, `or`, and `not`, and `match` and
//! `case _:` accepted as spellings of `switch` and `default:`. Parser errors are
//! returned as `Err(String)` (already positioned) instead of Go's panic/recover.

use std::rc::Rc;

use tiger_ast::*;
use tiger_lexer::{self as lexer, Token};

/// Parses `source` without a source name.
pub fn parse(source: &str) -> Result<Program, String> {
    parse_source(source, "")
}

/// Parses `source`, tagging diagnostics with `filename`.
pub fn parse_source(source: &str, filename: &str) -> Result<Program, String> {
    let tokens = lexer::scan_named(source, filename)?;
    let mut parser =
        Parser { tokens, pos: 0, function_depth: 0, loop_depth: 0, switch_depth: 0, labels: Vec::new(), depth: 0, clause_header: false };
    let mut program = Program::default();
    while !parser.at(lexer::EOF) {
        program.statements.push(parser.statement()?);
    }
    Ok(program)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    function_depth: usize,
    loop_depth: usize,
    switch_depth: usize,
    labels: Vec<String>,
    depth: usize,
    /// Whether the expression being parsed is the iterable or a filter of a
    /// comprehension clause, where `if` starts the next filter instead of a
    /// conditional expression. Brackets turn it off again.
    clause_header: bool,
}

/// The three clauses of a `for` header: initializer, condition, and update.
type ForClauses = (Option<Box<Stmt>>, Option<Expr>, Option<Box<Stmt>>);

impl Parser {
    fn current(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn at(&self, kind: lexer::Kind) -> bool {
        self.current().kind == kind
    }

    fn take(&mut self) -> Token {
        let token = self.tokens[self.pos].clone();
        if token.kind != lexer::EOF {
            self.pos += 1;
        }
        token
    }

    fn take_if(&mut self, kind: lexer::Kind) -> bool {
        if !self.at(kind) {
            return false;
        }
        self.take();
        true
    }

    fn expect(&mut self, kind: lexer::Kind) -> Result<Token, String> {
        if !self.at(kind) {
            return Err(self.current().errorf(&format!("expected {:?}, found {:?}", kind, self.current().kind)));
        }
        Ok(self.take())
    }

    /// Consumes a member name after `.`. A keyword is a name there too, so
    /// `this.value` and the built-in `regex.match` keep working even though
    /// `this` and `match` are reserved words.
    fn member_name(&mut self) -> Result<Token, String> {
        let token = self.current().clone();
        if self.at(lexer::IDENT) || lexer::KEYWORDS.contains(&token.kind) {
            self.take();
            return Ok(token);
        }
        Err(token.errorf(&format!("expected a member name after '.', found {:?}", token.kind)))
    }

    fn enter(&mut self) -> Result<(), String> {
        self.depth += 1;
        if self.depth > 512 {
            return Err(self.current().errorf("maximum syntax nesting exceeded"));
        }
        Ok(())
    }

    /// Reports whether the parser is at `name:` followed by a loop keyword.
    fn labeled_loop(&self) -> bool {
        if self.pos + 2 >= self.tokens.len() {
            return false;
        }
        if self.tokens[self.pos].kind != lexer::IDENT || self.tokens[self.pos + 1].kind != ":" {
            return false;
        }
        matches!(self.tokens[self.pos + 2].kind, "for" | "while" | "do")
    }

    fn has_label(&self, name: &str) -> bool {
        self.labels.iter().any(|label| label == name)
    }

    /// A hint for the removed `cif`, `celse`, and `cfor` keywords. They are
    /// ordinary identifiers now that `if`, `else`, and `for` accept both header
    /// styles, so a program that still uses them would otherwise report only a
    /// generic parse error.
    fn removed_keyword(&self) -> Option<String> {
        if !self.at(lexer::IDENT) {
            return None;
        }
        let hint = match self.current().text.as_str() {
            "cif" => "write if (condition) { ... } or if condition { ... }",
            "celse" => "write else or else if",
            "cfor" => "write for (initializer; condition; update) { ... }",
            _ => return None,
        };
        Some(self.current().errorf(&format!("{} is no longer a keyword; {}", self.current().text, hint)))
    }

    fn statement(&mut self) -> Result<Stmt, String> {
        if let Some(error) = self.removed_keyword() {
            return Err(error);
        }
        let base = Base::from_token(self.current());
        if self.take_if("import") {
            let path = self.expect(lexer::STRING)?;
            self.expect("as")?;
            let name = self.expect(lexer::IDENT)?;
            self.expect(";")?;
            return Ok(Stmt::Import { base, path: path.text, name: name.text });
        }
        if self.take_if("throw") {
            let value = self.expression(0)?;
            self.expect(";")?;
            return Ok(Stmt::Throw { base, value });
        }
        if self.take_if("try") {
            return self.try_statement(base);
        }
        if self.labeled_loop() {
            let label = self.take();
            self.expect(":")?;
            self.labels.push(label.text.clone());
            let mut statement = self.statement_tail(Base::from_token(self.current()))?;
            self.labels.pop();
            match &mut statement {
                Stmt::While { label: name, .. }
                | Stmt::For { label: name, .. }
                | Stmt::CFor { label: name, .. }
                | Stmt::DoWhile { label: name, .. } => *name = label.text.clone(),
                _ => return Err(label.errorf("a label must name a loop")),
            }
            return Ok(statement);
        }
        if self.take_if("class") {
            return self.class(base);
        }
        if self.take_if("function") {
            if self.at(lexer::IDENT) {
                let name = self.expect(lexer::IDENT)?.text;
                return self.function(base, name);
            }
            let declaration = self.signature(base.clone(), false)?;
            self.expect(";")?;
            return Ok(Stmt::Expression { base, value: Expr::Function(Rc::new(declaration)) });
        }
        self.statement_tail(base)
    }

    fn try_statement(&mut self, base: Base) -> Result<Stmt, String> {
        let body = self.block()?;
        let mut catches = Vec::new();
        while self.take_if("catch") {
            self.expect("(")?;
            let name = self.expect(lexer::IDENT)?.text;
            self.expect(")")?;
            let mut condition = None;
            if self.take_if("if") {
                condition = Some(self.expression(0)?);
            }
            let clause_body = self.block()?;
            catches.push(Catch { name, condition, body: clause_body });
        }
        let mut finally = Vec::new();
        let mut has_finally = false;
        if self.at(lexer::IDENT) && self.current().text == "finally" {
            self.take();
            finally = self.block()?;
            has_finally = true;
        }
        if catches.is_empty() && !has_finally {
            return Err(self.current().errorf("a try statement needs a catch or finally block"));
        }
        Ok(Stmt::Try { base, body, catches, finally })
    }

    fn function(&mut self, base: Base, name: String) -> Result<Stmt, String> {
        let mut declaration = self.signature(base, true)?;
        declaration.name = name;
        Ok(Stmt::Function(Rc::new(declaration)))
    }

    fn signature(&mut self, base: Base, terminator: bool) -> Result<FunctionDecl, String> {
        // A function body is its own context: the clause-header rule of an
        // enclosing comprehension does not reach inside it. A parse error there
        // ends the whole parse, so the flag needs no restoring on that path.
        let clause_header = std::mem::replace(&mut self.clause_header, false);
        let declaration = self.signature_body(base, terminator);
        self.clause_header = clause_header;
        declaration
    }

    fn signature_body(&mut self, base: Base, terminator: bool) -> Result<FunctionDecl, String> {
        self.expect("(")?;
        let mut parameters = Vec::new();
        let mut defaults: Vec<Option<Expr>> = Vec::new();
        let mut variadic = String::new();
        let mut keyword = String::new();
        let mut defaulted = false;
        let mut seen: Vec<String> = Vec::new();
        if !self.at(")") {
            loop {
                let parameter = self.current().clone();
                let star = self.at("*");
                let double_star = self.at("**");
                if star || double_star {
                    let marker = self.take();
                    if double_star {
                        if !keyword.is_empty() {
                            return Err(marker.errorf("function may declare at most one **kwargs parameter"));
                        }
                    } else {
                        if !keyword.is_empty() {
                            return Err(marker.errorf("*args cannot follow **kwargs"));
                        }
                        if !variadic.is_empty() {
                            return Err(marker.errorf("function may declare at most one *args parameter"));
                        }
                    }
                    let declaration = self.current().clone();
                    if declaration.kind != lexer::IDENT {
                        return Err(declaration.errorf(&format!("expected a parameter name after {:?}", marker.text)));
                    }
                    self.take();
                    if seen.contains(&declaration.text) {
                        return Err(declaration.errorf(&format!("duplicate parameter {:?}", declaration.text)));
                    }
                    seen.push(declaration.text.clone());
                    if double_star {
                        keyword = declaration.text;
                    } else {
                        variadic = declaration.text;
                    }
                } else {
                    if !variadic.is_empty() {
                        return Err(
                            parameter.errorf(&format!("parameter {:?} cannot follow *args; only **kwargs may follow", parameter.text))
                        );
                    }
                    if self.at("this") {
                        return Err(parameter.errorf("this is implicit in methods and constructors; remove it from the parameter list"));
                    }
                    self.expect(lexer::IDENT)?;
                    if seen.contains(&parameter.text) {
                        return Err(parameter.errorf(&format!("duplicate parameter {:?}", parameter.text)));
                    }
                    seen.push(parameter.text.clone());
                    let mut fallback = None;
                    if self.take_if("=") {
                        fallback = Some(self.expression(0)?);
                        defaulted = true;
                    } else if defaulted {
                        return Err(parameter.errorf("a required parameter cannot follow a defaulted parameter"));
                    }
                    parameters.push(Rc::from(parameter.text.as_str()));
                    defaults.push(fallback);
                }
                if !self.take_if(",") || self.at(")") {
                    break;
                }
            }
        }
        self.expect(")")?;
        self.function_depth += 1;
        let (loop_depth, switch_depth) = (self.loop_depth, self.switch_depth);
        let labels = std::mem::take(&mut self.labels);
        self.loop_depth = 0;
        self.switch_depth = 0;
        let body = self.block_body(terminator)?;
        self.loop_depth = loop_depth;
        self.switch_depth = switch_depth;
        self.labels = labels;
        self.function_depth -= 1;
        Ok(FunctionDecl { base, access: String::new(), name: String::new(), parameters, defaults, variadic, keyword, body })
    }

    fn statement_tail(&mut self, base: Base) -> Result<Stmt, String> {
        if self.take_if("return") {
            if self.function_depth == 0 {
                return Err(base.errorf("return outside a function"));
            }
            let mut value = None;
            if !self.at(";") {
                let first = self.expression(0)?;
                if self.at(",") {
                    let mut elements = vec![first];
                    while self.take_if(",") {
                        elements.push(self.expression(0)?);
                    }
                    value = Some(Expr::List { base: base.clone(), elements });
                } else {
                    value = Some(first);
                }
            }
            self.expect(";")?;
            return Ok(Stmt::Return { base, value });
        }
        if self.take_if("if") {
            let mut branches = Vec::new();
            let mut else_block = Vec::new();
            loop {
                let condition = self.expression(0)?;
                let body = self.block()?;
                branches.push(Branch { condition, body });
                if self.take_if("elif") {
                    continue;
                }
                if self.take_if("else") {
                    // C-style `else if` continues the chain; a bare `else` takes a block.
                    if self.take_if("if") {
                        continue;
                    }
                    else_block = self.block()?;
                }
                break;
            }
            return Ok(Stmt::If { base, branches, else_block });
        }
        if self.take_if("while") {
            let condition = self.expression(0)?;
            let (body, else_block) = self.loop_body()?;
            return Ok(Stmt::While { base, label: String::new(), condition, body, else_block });
        }
        if self.take_if("do") {
            self.loop_depth += 1;
            let body = self.block()?;
            self.loop_depth -= 1;
            self.expect("while")?;
            let condition = self.expression(0)?;
            self.expect(";")?;
            return Ok(Stmt::DoWhile { base, label: String::new(), condition, body });
        }
        if self.take_if("for") {
            if let Some((name, iterable)) = self.for_iterable()? {
                let (body, else_block) = self.loop_body()?;
                return Ok(Stmt::For { base, label: String::new(), name, iterable, body, else_block });
            }
            let (initializer, condition, update) = self.for_clauses()?;
            let (body, else_block) = self.loop_body()?;
            return Ok(Stmt::CFor { base, label: String::new(), initializer, condition, update, body, else_block });
        }
        if self.at("break") || self.at("continue") {
            let kind = self.take().text;
            let mut label = String::new();
            if self.at(lexer::IDENT) {
                let name = self.take();
                if !self.has_label(&name.text) {
                    return Err(name.errorf(&format!("unknown label {:?}", name.text)));
                }
                label = name.text;
            } else if self.loop_depth == 0 && (kind == "continue" || self.switch_depth == 0) {
                let suffix = if kind == "break" { " or switch" } else { "" };
                return Err(base.errorf(&format!("{} outside a loop{}", kind, suffix)));
            }
            self.expect(";")?;
            return Ok(Stmt::Control { base, kind, label });
        }
        if self.take_if("switch") || self.take_if("match") {
            self.enter()?;
            let value = self.expression(0)?;
            self.expect("{")?;
            self.switch_depth += 1;
            let mut cases = Vec::new();
            let mut seen_default = false;
            while !self.at("}") && !self.at(lexer::EOF) {
                let mut case = Case { value: None, body: Vec::new() };
                if self.take_if("case") {
                    // `case _:` is the wildcard spelling of `default:`.
                    if self.at(lexer::IDENT) && self.current().text == "_" {
                        let marker = self.take();
                        if seen_default {
                            return Err(marker.errorf("duplicate default case"));
                        }
                        seen_default = true;
                    } else {
                        case.value = Some(self.expression(0)?);
                    }
                } else {
                    let marker = self.expect("default")?;
                    if seen_default {
                        return Err(marker.errorf("duplicate default case"));
                    }
                    seen_default = true;
                }
                self.expect(":")?;
                while !self.at("case") && !self.at("default") && !self.at("}") && !self.at(lexer::EOF) {
                    case.body.push(self.statement()?);
                }
                cases.push(case);
            }
            self.switch_depth -= 1;
            self.expect("}")?;
            self.take_if(";");
            self.depth -= 1;
            return Ok(Stmt::Switch { base, value, cases });
        }
        let statement = self.simple_statement()?;
        self.expect(";")?;
        Ok(statement)
    }

    fn simple_statement(&mut self) -> Result<Stmt, String> {
        let base = Base::from_token(self.current());
        if self.at("const") || self.at("var") {
            let constant = self.take().kind == "const";
            if self.at("[") || self.at("{") {
                return self.destructure(base, constant);
            }
            let name = self.expect(lexer::IDENT)?;
            self.expect("=")?;
            let value = self.expression(0)?;
            return Ok(Stmt::Assign {
                base,
                target: Expr::Identifier { base: Base::from_token(&name), name: name.text },
                value,
                operator: String::new(),
                constant,
                declaration: true,
            });
        }
        let value = self.expression(0)?;
        if self.at("=") || self.at("+=") || self.at("-=") || self.at("*=") || self.at("%=") {
            let operator = self.take();
            self.assignment_target(&value)?;
            let right = self.expression(0)?;
            return Ok(Stmt::Assign { base, target: value, value: right, operator: operator.text, constant: false, declaration: false });
        }
        Ok(Stmt::Expression { base, value })
    }

    fn destructure(&mut self, base: Base, constant: bool) -> Result<Stmt, String> {
        let lookup = self.at("{");
        let closer = if lookup { "}" } else { "]" };
        self.take();
        let mut names = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        if !self.at(closer) {
            loop {
                let name = self.expect(lexer::IDENT)?;
                if seen.contains(&name.text) {
                    return Err(name.errorf(&format!("duplicate destructuring target {:?}", name.text)));
                }
                seen.push(name.text.clone());
                names.push(name.text);
                if !self.take_if(",") || self.at(closer) {
                    break;
                }
            }
        }
        self.expect(closer)?;
        self.expect("=")?;
        let value = self.expression(0)?;
        Ok(Stmt::Destructure { base, names, constant, value, lookup })
    }

    fn assignment_target(&self, value: &Expr) -> Result<(), String> {
        match value {
            Expr::Identifier { .. } | Expr::Index { .. } | Expr::Property { .. } => Ok(()),
            _ => Err(value.base().errorf("invalid assignment target")),
        }
    }

    fn loop_body(&mut self) -> Result<(Block, Block), String> {
        self.loop_depth += 1;
        let body = self.block()?;
        self.loop_depth -= 1;
        let mut otherwise = Vec::new();
        if self.take_if("else") {
            otherwise = self.block()?;
        }
        Ok((body, otherwise))
    }

    /// The `for name in iterable` header, or `None` when the loop uses the
    /// three-clause form instead.
    fn for_iterable(&mut self) -> Result<Option<(String, Expr)>, String> {
        let iterates = self.at(lexer::IDENT) && self.tokens.get(self.pos + 1).map(|token| token.kind) == Some("in");
        if !iterates {
            return Ok(None);
        }
        let name = self.expect(lexer::IDENT)?.text;
        self.expect("in")?;
        Ok(Some((name, self.expression(0)?)))
    }

    /// The three-clause header, with the parentheses optional:
    /// `for (initializer; condition; update)` and `for initializer; condition; update`.
    /// Each clause may be empty; the update may assign but not declare.
    fn for_clauses(&mut self) -> Result<ForClauses, String> {
        let parenthesized = self.take_if("(");
        let closer = if parenthesized { ")" } else { "{" };
        // C-style headers often name a type; Tiger has no type annotations.
        if self.at(lexer::IDENT)
            && self.tokens.get(self.pos + 1).map(|token| token.kind) == Some(lexer::IDENT)
            && self.tokens.get(self.pos + 2).map(|token| token.kind) == Some("=")
        {
            return Err(self.current().errorf(&format!(
                "{} looks like a type name; Tiger has no type annotations, so declare the loop variable with var",
                self.current().text
            )));
        }
        let mut initializer = None;
        if !self.at(";") {
            initializer = Some(Box::new(self.simple_statement()?));
        }
        self.expect(";")?;
        let mut condition = None;
        if !self.at(";") {
            condition = Some(self.expression(0)?);
        }
        self.expect(";")?;
        let mut update = None;
        if !self.at(closer) {
            let statement = self.simple_statement()?;
            if let Stmt::Assign { declaration: true, base, .. } = &statement {
                return Err(base.errorf("the update clause of a for loop cannot declare a variable"));
            }
            update = Some(Box::new(statement));
        }
        if parenthesized {
            self.expect(")")?;
        }
        Ok((initializer, condition, update))
    }

    fn class(&mut self, base: Base) -> Result<Stmt, String> {
        self.enter()?;
        let name = self.expect(lexer::IDENT)?;
        let class_name = name.text.clone();
        if self.at("(") {
            return Err(self.current().errorf(&format!("use 'class {} extends Parent' to declare inheritance", class_name)));
        }
        let mut parent = None;
        let mut parent_name = None;
        if self.take_if("extends") {
            let parent_token = self.expect(lexer::IDENT)?;
            if parent_token.text == class_name {
                return Err(parent_token.errorf("a class cannot inherit from itself"));
            }
            parent = Some(Base::from_token(&parent_token));
            parent_name = Some(parent_token.text);
        }
        self.expect("{")?;
        let mut seen: Vec<String> = Vec::new();
        let mut constructor: Option<Rc<FunctionDecl>> = None;
        let mut methods = Vec::new();
        let mut fields = Vec::new();
        while !self.at("}") && !self.at(lexer::EOF) {
            let mut access = "public".to_string();
            if self.at("public") || self.at("private") || self.at("protected") {
                access = self.take().text;
            }
            if self.at("var") || self.at("const") {
                let start = self.take();
                let field_name = self.expect(lexer::IDENT)?;
                let mut value = None;
                if self.take_if("=") {
                    value = Some(self.expression(0)?);
                } else if start.kind == "const" {
                    return Err(field_name.errorf(&format!("const field {:?} requires an initializer", field_name.text)));
                }
                self.expect(";")?;
                if seen.contains(&field_name.text) {
                    return Err(field_name.errorf(&format!("duplicate member {:?}", field_name.text)));
                }
                seen.push(field_name.text.clone());
                fields.push(FieldDecl {
                    base: Base::from_token(&start),
                    name: field_name.text,
                    access,
                    constant: start.kind == "const",
                    value,
                });
                continue;
            }
            if self.at(lexer::IDENT) && self.current().text == class_name {
                let start = self.take();
                let mut declaration = self.signature(Base::from_token(&start), true)?;
                declaration.name = class_name.clone();
                declaration.access = access;
                if constructor.is_some() {
                    return Err(start.errorf(&format!("duplicate constructor for {}", class_name)));
                }
                constructor = Some(Rc::new(declaration));
                continue;
            }
            if !self.at("function") {
                return Err(self.current().errorf("class bodies may only contain a constructor, methods, and field declarations"));
            }
            let statement = self.statement()?;
            let mut declaration = match statement {
                Stmt::Function(shared) => (*shared).clone(),
                _ => unreachable!("class body function"),
            };
            declaration.access = access;
            if declaration.name == class_name {
                return Err(declaration.base.errorf(&format!("declare the constructor as {}(...) without 'function'", class_name)));
            }
            if seen.contains(&declaration.name) {
                return Err(declaration.base.errorf(&format!("duplicate method {:?}", declaration.name)));
            }
            seen.push(declaration.name.clone());
            methods.push(Rc::new(declaration));
        }
        self.expect("}")?;
        self.take_if(";");
        self.depth -= 1;
        Ok(Stmt::Class(Rc::new(ClassDecl { base, name: class_name, parent, parent_name, constructor, methods, fields })))
    }

    fn block(&mut self) -> Result<Block, String> {
        self.block_body(true)
    }

    /// Parses `{ statements }`. A statement-level block swallows one optional
    /// trailing `;`, but an anonymous function body must leave it for the
    /// enclosing statement.
    fn block_body(&mut self, terminator: bool) -> Result<Block, String> {
        self.enter()?;
        self.expect("{")?;
        let mut statements = Vec::new();
        while !self.at("}") && !self.at(lexer::EOF) {
            statements.push(self.statement()?);
        }
        self.expect("}")?;
        if terminator {
            self.take_if(";");
        }
        self.depth -= 1;
        Ok(statements)
    }

    fn expression(&mut self, minimum: i32) -> Result<Expr, String> {
        self.enter()?;
        let result = self.expression_body(minimum);
        self.depth -= 1;
        result
    }

    /// Parses an expression inside brackets, a call's arguments, or a function
    /// body, where the clause-header rule does not apply: `(a if b else c)` is a
    /// conditional expression even inside a comprehension clause.
    fn nested<T>(&mut self, body: impl FnOnce(&mut Self) -> Result<T, String>) -> Result<T, String> {
        let previous = std::mem::replace(&mut self.clause_header, false);
        let result = body(self);
        self.clause_header = previous;
        result
    }

    /// Parses the iterable or a filter of a comprehension clause.
    fn clause_expression(&mut self) -> Result<Expr, String> {
        let previous = std::mem::replace(&mut self.clause_header, true);
        let result = self.expression(0);
        self.clause_header = previous;
        result
    }

    /// Parses the `for name in iterable [if condition]*` clauses of a
    /// comprehension, which is what follows its element.
    fn comprehension_clauses(&mut self) -> Result<Vec<ComprehensionClause>, String> {
        let mut clauses = Vec::new();
        loop {
            self.expect("for")?;
            let name = self.expect(lexer::IDENT)?;
            self.expect("in")?;
            let iterable = self.clause_expression()?;
            let mut filters = Vec::new();
            while self.take_if("if") {
                filters.push(self.clause_expression()?);
            }
            clauses.push(ComprehensionClause { name: name.text, iterable, filters });
            if !self.at("for") {
                return Ok(clauses);
            }
        }
    }

    fn expression_body(&mut self, minimum: i32) -> Result<Expr, String> {
        let token = self.take();
        let mut base = Base::from_token(&token);
        let mut left: Expr;
        match token.kind {
            lexer::NUMBER => {
                let value = token.text.parse::<f64>().unwrap_or(f64::NAN);
                left = Expr::Literal { base: base.clone(), value: LiteralValue::Number(value) };
            }
            lexer::STRING => {
                left = Expr::Literal { base: base.clone(), value: LiteralValue::String(token.text.clone()) };
            }
            lexer::FSTRING => {
                let mut parts = Vec::new();
                for part in &token.parts {
                    if part.kind == lexer::STRING {
                        parts.push(Expr::Literal { base: Base::from_token(part), value: LiteralValue::String(part.text.clone()) });
                        continue;
                    }
                    let (expression_tokens, spec) = split_format_spec(&part.parts);
                    let mut nested = Parser {
                        tokens: expression_tokens,
                        pos: 0,
                        function_depth: 0,
                        loop_depth: 0,
                        switch_depth: 0,
                        labels: Vec::new(),
                        depth: self.depth,
                        // An f-string field is bracketed text of its own, so the
                        // clause-header rule does not reach into it.
                        clause_header: false,
                    };
                    let value = nested.expression(0)?;
                    nested.expect(lexer::EOF)?;
                    parts.push(Expr::Interpolation { base: Base::from_token(part), value: Box::new(value), spec });
                }
                left = Expr::FormattedString { base: base.clone(), parts };
            }
            "true" | "false" => {
                left = Expr::Literal { base: base.clone(), value: LiteralValue::Bool(token.kind == "true") };
            }
            "null" => {
                left = Expr::Literal { base: base.clone(), value: LiteralValue::Null };
            }
            "function" => {
                let declaration = self.signature(base.clone(), false)?;
                left = Expr::Function(Rc::new(declaration));
            }
            lexer::IDENT | "this" => {
                left = Expr::Identifier { base: base.clone(), name: token.text.clone() };
            }
            "super" => {
                if self.at("(") {
                    left = Expr::Super { base: base.clone() };
                } else {
                    self.expect(".")?;
                    let member = self.member_name()?;
                    left = Expr::Property {
                        base: Base::from_token(&member),
                        receiver: Box::new(Expr::Super { base: base.clone() }),
                        name: member.text,
                    };
                }
            }
            "-" | "+" | "~" | "not" | "!" => {
                // `!` is `not` under another spelling, including its precedence.
                let binding = if token.kind == "not" || token.kind == "!" { 2 } else { 10 };
                let operator = if token.kind == "!" { "not".to_string() } else { token.text.clone() };
                let right = self.expression(binding)?;
                left = Expr::Unary { base: base.clone(), operator, right: Box::new(right) };
            }
            "++" | "--" => {
                let target = self.expression(11)?;
                self.assignment_target(&target)?;
                left = Expr::Update { base: base.clone(), target: Box::new(target), operator: token.text.clone(), prefix: true };
            }
            "(" => {
                // `()` is the empty tuple, `(value)` is a parenthesized
                // expression, and a comma makes a tuple: `(1,)` is the one-element
                // form, and a trailing comma is allowed after every element.
                if self.at(")") {
                    self.expect(")")?;
                    left = Expr::Tuple { base: base.clone(), elements: Vec::new() };
                } else {
                    let first = self.nested(|parser| parser.expression(0))?;
                    if self.at("for") {
                        // `(x for x in items)` would build values one at a time; the
                        // runtime is eager, so this form stays unimplemented.
                        return Err(self
                            .current()
                            .errorf("generator expressions are not supported; use a list comprehension like [x for x in items]"));
                    }
                    if self.at(",") {
                        let mut elements = vec![first];
                        while self.take_if(",") {
                            if self.at(")") {
                                break;
                            }
                            elements.push(self.nested(|parser| parser.expression(0))?);
                        }
                        self.expect(")")?;
                        left = Expr::Tuple { base: base.clone(), elements };
                    } else {
                        self.expect(")")?;
                        left = first;
                    }
                }
            }
            "[" => {
                // A `for` after the first value makes this a list comprehension:
                // `[x * 2 for x in items if x > 0]`.
                if self.at("]") {
                    self.expect("]")?;
                    left = Expr::List { base: base.clone(), elements: Vec::new() };
                } else {
                    let first = self.nested(|parser| parser.expression(0))?;
                    if self.at("for") {
                        let clauses = self.comprehension_clauses()?;
                        self.expect("]")?;
                        left = Expr::Comprehension { base: base.clone(), body: ComprehensionBody::List(Box::new(first)), clauses };
                    } else {
                        let mut elements = vec![first];
                        while self.take_if(",") {
                            if self.at("]") {
                                break;
                            }
                            elements.push(self.nested(|parser| parser.expression(0))?);
                        }
                        self.expect("]")?;
                        left = Expr::List { base: base.clone(), elements };
                    }
                }
            }
            "{" => {
                // A brace literal is a dictionary when its first entry is a
                // `key: value` pair and a set otherwise, so `{}` stays an empty
                // dictionary and a set is written as a list of values. A `for`
                // after the first entry makes it a comprehension, which needs a
                // key and a value.
                if self.at("}") {
                    self.expect("}")?;
                    left = Expr::Dict { base: base.clone(), pairs: Vec::new() };
                } else {
                    let first = self.nested(|parser| parser.expression(0))?;
                    if self.take_if(":") {
                        let first_value = self.nested(|parser| parser.expression(0))?;
                        if self.at("for") {
                            let clauses = self.comprehension_clauses()?;
                            self.expect("}")?;
                            left = Expr::Comprehension {
                                base: base.clone(),
                                body: ComprehensionBody::Dict(Box::new(Pair { key: first, value: first_value })),
                                clauses,
                            };
                        } else {
                            let mut pairs = vec![Pair { key: first, value: first_value }];
                            while self.take_if(",") {
                                if self.at("}") {
                                    break;
                                }
                                let key = self.nested(|parser| parser.expression(0))?;
                                if !self.at(":") {
                                    return Err(key.base().errorf("dictionary entries are key: value pairs; a set lists only values"));
                                }
                                self.take_if(":");
                                let value = self.nested(|parser| parser.expression(0))?;
                                pairs.push(Pair { key, value });
                            }
                            self.expect("}")?;
                            left = Expr::Dict { base: base.clone(), pairs };
                        }
                    } else if self.at("for") {
                        // `{x for x in items}` would build a set from an
                        // expression; write `set([x for x in items])` instead.
                        return Err(self.current().errorf("set comprehensions are not supported; use set([x for x in items])"));
                    } else {
                        let mut elements = vec![first];
                        loop {
                            if self.at(":") {
                                return Err(base.errorf("a set lists values separated by commas; a dictionary entry is key: value"));
                            }
                            if !self.take_if(",") || self.at("}") {
                                break;
                            }
                            elements.push(self.nested(|parser| parser.expression(0))?);
                        }
                        self.expect("}")?;
                        left = Expr::Set { base: base.clone(), elements };
                    }
                }
            }
            _ => {
                return Err(token.errorf(&format!("expected expression, found {:?}", token.kind)));
            }
        }
        // In a comprehension clause `if` opens a filter rather than a conditional
        // expression, so the loop stops there and the clause parser takes it.
        while precedence(self.current().kind) > minimum && !(self.clause_header && self.at("if")) {
            let operator = self.take();
            base = Base::from_token(&operator);
            match operator.kind {
                "++" | "--" => {
                    self.assignment_target(&left)?;
                    left = Expr::Update { base: base.clone(), target: Box::new(left), operator: operator.text.clone(), prefix: false };
                }
                "." => {
                    let member = self.member_name()?;
                    left = Expr::Property { base: Base::from_token(&member), receiver: Box::new(left), name: member.text };
                }
                "(" => {
                    left = self.call(base.clone(), left)?;
                }
                "[" => {
                    left = self.index_or_slice(base.clone(), left)?;
                }
                "if" => {
                    let condition = self.expression(0)?;
                    self.expect("else")?;
                    let otherwise = self.expression(0)?;
                    left = Expr::Conditional {
                        base: base.clone(),
                        condition: Box::new(condition),
                        then: Box::new(left),
                        otherwise: Box::new(otherwise),
                    };
                }
                _ => {
                    let mut text = operator.text.clone();
                    if text == "&&" || text == "||" {
                        text = if text == "&&" { "and".to_string() } else { "or".to_string() };
                    }
                    let mut binding = precedence(operator.kind);
                    if text == "**" {
                        binding -= 1;
                    }
                    let right = self.expression(binding)?;
                    left = Expr::Binary { base: base.clone(), left: Box::new(left), operator: text, right: Box::new(right) };
                }
            }
        }
        Ok(left)
    }

    fn call(&mut self, base: Base, function: Expr) -> Result<Expr, String> {
        // The arguments sit inside parentheses, where a conditional expression is
        // allowed even in a comprehension clause.
        let clause_header = std::mem::replace(&mut self.clause_header, false);
        let call = self.call_body(base, function);
        self.clause_header = clause_header;
        call
    }

    fn call_body(&mut self, base: Base, function: Expr) -> Result<Expr, String> {
        let mut arguments = Vec::new();
        let mut keywords = Vec::new();
        let mut seen_keyword = false;
        let mut seen_names: Vec<String> = Vec::new();
        if !self.at(")") {
            loop {
                if self.at("**") {
                    self.take();
                    seen_keyword = true;
                    let value = self.expression(0)?;
                    keywords.push(KeywordArgument { name: String::new(), value, spread: true });
                } else if self.at("*") {
                    let marker = self.take();
                    if seen_keyword {
                        return Err(marker.errorf("positional argument cannot follow a keyword argument"));
                    }
                    let value = self.expression(0)?;
                    arguments.push(Argument { value, spread: true });
                } else if self.at(lexer::IDENT) && self.pos + 1 < self.tokens.len() && self.tokens[self.pos + 1].kind == "=" {
                    seen_keyword = true;
                    let name = self.take();
                    self.take();
                    if seen_names.contains(&name.text) {
                        return Err(name.errorf(&format!("duplicate keyword argument {:?}", name.text)));
                    }
                    seen_names.push(name.text.clone());
                    let value = self.expression(0)?;
                    keywords.push(KeywordArgument { name: name.text, value, spread: false });
                } else {
                    if seen_keyword {
                        return Err(self.current().errorf("positional argument cannot follow a keyword argument"));
                    }
                    let value = self.expression(0)?;
                    arguments.push(Argument { value, spread: false });
                }
                if !self.take_if(",") || self.at(")") {
                    break;
                }
            }
        }
        self.expect(")")?;
        Ok(Expr::Call { base, function: Box::new(function), arguments, keywords })
    }

    fn index_or_slice(&mut self, base: Base, collection: Expr) -> Result<Expr, String> {
        // The bounds sit inside brackets, where a conditional expression is allowed
        // even in a comprehension clause.
        let clause_header = std::mem::replace(&mut self.clause_header, false);
        let index = self.index_or_slice_body(base, collection);
        self.clause_header = clause_header;
        index
    }

    fn index_or_slice_body(&mut self, base: Base, collection: Expr) -> Result<Expr, String> {
        let mut start = None;
        if !self.at(":") && !self.at("]") {
            start = Some(self.expression(0)?);
        }
        if !self.at(":") {
            self.expect("]")?;
            let key = start.unwrap_or(Expr::Literal { base: base.clone(), value: LiteralValue::Null });
            return Ok(Expr::Index { base, collection: Box::new(collection), key: Box::new(key) });
        }
        self.expect(":")?;
        let mut stop = None;
        if !self.at(":") && !self.at("]") {
            stop = Some(self.expression(0)?);
        }
        let mut step = None;
        if self.take_if(":") && !self.at("]") {
            step = Some(self.expression(0)?);
        }
        self.expect("]")?;
        Ok(Expr::Slice {
            base,
            collection: Box::new(collection),
            start: start.map(Box::new),
            stop: stop.map(Box::new),
            step: step.map(Box::new),
        })
    }
}

/// Splits the expression tokens of an f-string field from a trailing `:spec`
/// that sits at bracket depth zero.
fn split_format_spec(tokens: &[Token]) -> (Vec<Token>, String) {
    let mut depth = 0i32;
    for (index, token) in tokens.iter().enumerate() {
        match token.kind {
            "(" | "[" | "{" => depth += 1,
            ")" | "]" | "}" => depth -= 1,
            ":" => {
                if depth != 0 {
                    continue;
                }
                let spec: String = tokens[index + 1..].iter().map(|piece| piece.text.as_str()).collect();
                let mut expression: Vec<Token> = tokens[..index].to_vec();
                let mut eof = Token::new(lexer::EOF, token.line, token.column);
                eof.source = token.source.clone();
                expression.push(eof);
                return (expression, spec);
            }
            _ => {}
        }
    }
    (tokens.to_vec(), String::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(source: &str) -> Program {
        parse(source).unwrap_or_else(|error| panic!("parse failed: {error}"))
    }

    fn parse_err(source: &str) -> String {
        parse(source).expect_err("expected a syntax error")
    }

    #[test]
    fn precedence() {
        let program = parse_ok("const answer = 1 + 2 * 3;");
        let Stmt::Assign { value, .. } = &program.statements[0] else { panic!("assign") };
        let Expr::Binary { operator, right, .. } = value else { panic!("binary") };
        assert_eq!(operator, "+");
        assert!(matches!(**right, Expr::Binary { ref operator, .. } if operator == "*"));
    }

    #[test]
    fn parses_valid_syntax() {
        let source = "const PI = 3.14;\n\
            function add(a, b) { return a + b; }\n\
            if PI > 3 { print(add(1, 2)); } elif PI == 3 { print(\"three\"); } else { print(false); }\n\
            while false { const value = 1; }\n\
            for item in [1, 2, \"apple\", true] { print(item); }\n\
            const config = {\"port\": 8080, \"host\": \"localhost\"};\n\
            config[\"port\"] = 80;\n\
            function empty() { return; };\n";
        parse_ok(source);
    }

    #[test]
    fn rejects_invalid_syntax() {
        for source in [
            "value = 1",
            "print(1)",
            "function bad() { return 1 }",
            "if true: { print(1); }",
            "while true print(1);",
            "for item in [1] print(item);",
            "const value;",
            "function bad(a, a) {}",
            "return 1;",
            "1 = 2;",
            "if true {",
            "[1, 2;",
            "{1 2};",
            "var value;",
            "var value = ;",
            "const value = ;",
            "var items[0] = 1;",
            "const this.value = 1;",
        ] {
            parse_err(source);
        }
    }

    #[test]
    fn parses_syntax_gaps() {
        let program = parse_ok(
            "const pick = \"a\" if flag else \"b\";\n\
            outer: for item in [1, 2] {\n\
                if item == 1 { continue outer; }\n\
                break outer;\n\
            }\n\
            do { print(1); } while (false);\n\
            const window = [1, 2, 3][0:2];",
        );
        let Stmt::Assign { value, .. } = &program.statements[0] else { panic!("assign") };
        assert!(matches!(value, Expr::Conditional { .. }), "conditional did not parse");
        let Stmt::For { label, body, .. } = &program.statements[1] else { panic!("for") };
        assert_eq!(label, "outer");
        let Stmt::If { branches, .. } = &body[0] else { panic!("if") };
        let Stmt::Control { kind, label, .. } = &branches[0].body[0] else { panic!("control") };
        assert_eq!(kind, "continue");
        assert_eq!(label, "outer");
        let Stmt::Control { kind, label, .. } = &body[1] else { panic!("control") };
        assert_eq!(kind, "break");
        assert_eq!(label, "outer");
        assert!(matches!(program.statements[2], Stmt::DoWhile { .. }), "do...while did not parse");
        let Stmt::Assign { value, .. } = &program.statements[3] else { panic!("assign") };
        let Expr::Slice { start, stop, step, .. } = value else { panic!("slice") };
        assert!(start.is_some() && stop.is_some() && step.is_none());
    }

    #[test]
    fn rejects_syntax_gap_errors() {
        for source in [
            "print(1 if true);",
            "print(1 if true else);",
            "break missing;",
            "outer: for item in [1] { continue missing; }",
            "outer: while true { function inner() { break outer; } }",
            "do { print(1); } while;",
            "do { print(1); } while true",
            "for item in [1] { continue 5; }",
        ] {
            parse_err(source);
        }
    }

    #[test]
    fn enforces_nesting_limit() {
        // Deep recursion needs a larger stack than the default test thread; the
        // CLI runs the parser on a dedicated big-stack thread for the same reason.
        let handle = std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(|| parse_err(&format!("{}1{}", "(".repeat(600), ")".repeat(600))))
            .expect("spawn");
        handle.join().expect("join");
    }

    #[test]
    fn operator_precedence() {
        let program = parse_ok("const value = -2 ** 2;");
        let Stmt::Assign { value, .. } = &program.statements[0] else { panic!("assign") };
        let Expr::Unary { operator, right, .. } = value else { panic!("unary") };
        assert_eq!(operator, "-");
        assert!(matches!(**right, Expr::Binary { ref operator, .. } if operator == "**"));

        let program = parse_ok("const bits = 1 | 2 & 3;");
        let Stmt::Assign { value, .. } = &program.statements[0] else { panic!("assign") };
        let Expr::Binary { operator, right, .. } = value else { panic!("binary") };
        assert_eq!(operator, "|");
        assert!(matches!(**right, Expr::Binary { ref operator, .. } if operator == "&"));

        let program = parse_ok("const shift = 1 + 2 << 3;");
        let Stmt::Assign { value, .. } = &program.statements[0] else { panic!("assign") };
        let Expr::Binary { operator, left, .. } = value else { panic!("binary") };
        assert_eq!(operator, "<<");
        assert!(matches!(**left, Expr::Binary { ref operator, .. } if operator == "+"));

        let program = parse_ok("const compare = 1 | 2 == 3;");
        let Stmt::Assign { value, .. } = &program.statements[0] else { panic!("assign") };
        let Expr::Binary { operator, left, .. } = value else { panic!("binary") };
        assert_eq!(operator, "==");
        assert!(matches!(**left, Expr::Binary { ref operator, .. } if operator == "|"));

        let program = parse_ok("const floor = 6 div 3 * 2;");
        let Stmt::Assign { value, .. } = &program.statements[0] else { panic!("assign") };
        let Expr::Binary { operator, left, .. } = value else { panic!("binary") };
        assert_eq!(operator, "*");
        assert!(matches!(**left, Expr::Binary { ref operator, .. } if operator == "div"));
    }

    #[test]
    fn parses_function_ergonomics() {
        let named = parse_ok("function f(a, b = 2) { return a; }");
        let Stmt::Function(declaration) = &named.statements[0] else { panic!("function") };
        assert_eq!(declaration.parameters.len(), 2);
        assert!(declaration.defaults[0].is_none());
        assert!(declaration.defaults[1].is_some());

        let anonymous = parse_ok("const f = function(x) { return x; };");
        let Stmt::Assign { value, .. } = &anonymous.statements[0] else { panic!("assign") };
        let Expr::Function(declaration) = value else { panic!("function") };
        assert_eq!(declaration.name, "");

        let multiple = parse_ok("function f() { return 1, 2; }");
        let Stmt::Function(declaration) = &multiple.statements[0] else { panic!("function") };
        let Stmt::Return { value: Some(Expr::List { elements, .. }), .. } = &declaration.body[0] else { panic!("return list") };
        assert_eq!(elements.len(), 2);

        let destructured = parse_ok("const [a, b] = pair();");
        let Stmt::Destructure { lookup, names, constant, .. } = &destructured.statements[0] else { panic!("destructure") };
        assert!(!lookup && names.len() == 2 && *constant);

        let record = parse_ok("var {x, y} = record;");
        let Stmt::Destructure { lookup, constant, .. } = &record.statements[0] else { panic!("destructure") };
        assert!(*lookup && !*constant);

        parse_err("function bad(a = 1, b) { return a; }");
    }

    #[test]
    fn parses_try_catches_and_finally() {
        let program = parse_ok("try { print(1); } catch (error) if isinstance(error, \"error\") { print(2); } finally { print(3); }");
        let Stmt::Try { catches, finally, .. } = &program.statements[0] else { panic!("try") };
        assert_eq!(catches.len(), 1);
        assert!(catches[0].condition.is_some());
        assert!(!finally.is_empty());
        parse_ok("try { } finally { }");
        parse_err("try { }");
        let chained = parse_ok("try { } catch (error) if error[\"kind\"] == \"x\" { } catch (other) { }");
        let Stmt::Try { catches, .. } = &chained.statements[0] else { panic!("try") };
        assert_eq!(catches.len(), 2);
    }

    #[test]
    fn parses_class_syntax() {
        let program = parse_ok(
            "class Animal {\n\
            Animal(name) { this.name = name; }\n\
            function speak() { return this.name; }\n\
        }\n\
        class Dog extends Animal {\n\
            Dog(name) { super(name); }\n\
            function speak() { return super.speak() + \"!\"; }\n\
        }\n\
        const pet = Dog(\"Rex\");\n\
        pet.friend = Animal(\"Cat\");\n\
        print(pet.friend.speak(), [pet][0].name);",
        );
        let Stmt::Class(derived) = &program.statements[1] else { panic!("class") };
        assert_eq!(derived.parent_name.as_deref(), Some("Animal"));
        assert_eq!(derived.methods.len(), 1);
        let constructor = derived.constructor.as_ref().expect("constructor");
        assert_eq!(constructor.name, "Dog");
        let Stmt::Assign { target, .. } = &program.statements[3] else { panic!("assign") };
        assert!(matches!(target, Expr::Property { ref name, .. } if name == "friend"));
    }

    #[test]
    fn rejects_invalid_class_syntax() {
        for source in [
            "class Animal: {}",
            "class Animal extends Parent, Other {}",
            "class Animal extends Animal {}",
            "class Animal(Parent) {}",
            "class Animal extends {}",
            "class Animal extends 1 {}",
            "class Animal { function speak(this) {} }",
            "class Animal { function speak(name, this) {} }",
            "class Animal { function speak() {} function speak() {} }",
            "class Animal { value = 1; }",
            "class Animal { Animal() { this.name = \"Rex\" } }",
            "class Animal { Animal(this) {} }",
            "class Animal { Animal(name, this) {} }",
            "class Animal { Animal() {} Animal(name) {} }",
            "class Animal { function Animal() {} }",
            "class Animal { Other() {} }",
            "super.;",
            "class Animal { const legs; }",
            "class Animal { var legs }",
            "class Animal { var legs = ; }",
            "class Animal { function speak() { return 1 } }",
            "class Animal {",
            "pet.name = 1",
            "pet.speak()",
            "super;",
            "pet.;",
        ] {
            parse_err(source);
        }
    }

    #[test]
    fn hints_at_parenthesized_inheritance() {
        let error = parse_err("class Dog(Animal) {}");
        assert!(error.contains("use 'class Dog extends Parent'"), "{error}");
    }

    #[test]
    fn parses_both_control_flow_styles() {
        let c_style = parse_ok(
            "if (a && !b) { print(1); } else if (c) {} else {}\n\
             for (var index = 0; index < 3; index++) { print(index); }\n\
             match 1 { case 1: break; case _: break; }",
        );
        let python = parse_ok(
            "if a and not b { print(1); } elif c {} else {}\n\
             for index in range(3) { print(index); }\n\
             switch 1 { case 1: break; default: break; }",
        );
        assert_eq!(c_style.statements.len(), python.statements.len());
        assert!(matches!(c_style.statements[0], Stmt::If { .. }), "both header styles build one chain");
        assert!(matches!(c_style.statements[1], Stmt::CFor { .. }), "parenthesized clauses");
        assert!(matches!(python.statements[1], Stmt::For { .. }), "for ... in iterates");
        assert!(matches!(c_style.statements[2], Stmt::Switch { .. }), "match builds a switch");
        assert!(matches!(python.statements[2], Stmt::Switch { .. }), "switch builds a switch");
    }

    #[test]
    fn rejects_the_removed_keywords() {
        for (source, message) in [
            ("cif (a) { print(1); }", "cif is no longer a keyword"),
            ("if a {} celse {}", "celse is no longer a keyword"),
            ("cfor (;;) {}", "cfor is no longer a keyword"),
            ("for (int index = 0; index < 3; index++) {}", "looks like a type name"),
        ] {
            let error = parse_err(source);
            assert!(error.contains(message), "{}: expected {:?}, got {}", source, message, error);
        }
    }

    #[test]
    fn collects_imports() {
        let program = parse_ok("outer: do { import \"a.tg\" as a; if true { import \"b.tg\" as b; } } while (false);");
        let found = tiger_ast::imports(&program);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].0, "a.tg");
        assert_eq!(found[1].0, "b.tg");

        let program = parse_ok("class Box { Box() { import \"a.tg\" as a; } function run() { import \"b.tg\" as b; } }");
        let found = tiger_ast::imports(&program);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].0, "a.tg");
        assert_eq!(found[1].0, "b.tg");
    }
}

fn precedence(kind: lexer::Kind) -> i32 {
    match kind {
        "or" | "||" => 1,
        "if" => 1,
        "and" | "&&" => 2,
        "==" | "!=" | "<" | "<=" | ">" | ">=" | "in" => 3,
        "|" => 4,
        "^" => 5,
        "&" => 6,
        "<<" | ">>" => 7,
        "+" | "-" => 8,
        "*" | "/" | "%" | "div" => 9,
        "**" => 11,
        "(" | "[" | "." => 12,
        "++" | "--" => 13,
        _ => 0,
    }
}
