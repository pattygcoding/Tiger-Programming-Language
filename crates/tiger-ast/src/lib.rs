//! Abstract syntax tree for the Tiger language.
//!
//! Function and class declarations are shared through `Rc` so a runtime value can
//! keep its declaration alive without copying the tree. Expressions are owned
//! (`Box`/`Vec`) because they are only ever borrowed while evaluating.

use std::rc::Rc;

use tiger_lexer::Token;

/// A block of statements.
pub type Block = Vec<Stmt>;

/// Source position, carrying the file name so diagnostics read
/// `source:line:column` (or `line:column` when there is no source name).
#[derive(Clone, Debug)]
pub struct Base {
    pub line: u32,
    pub column: u32,
    pub source: Rc<str>,
}

impl Base {
    pub fn from_token(token: &Token) -> Base {
        Base { line: token.line, column: token.column, source: token.source.clone() }
    }

    pub fn location(&self) -> String {
        if self.source.is_empty() {
            format!("{}:{}", self.line, self.column)
        } else {
            format!("{}:{}:{}", self.source, self.line, self.column)
        }
    }

    pub fn errorf(&self, message: &str) -> String {
        format!("{}: {}", self.location(), message)
    }
}

/// A literal expression payload.
#[derive(Clone, Debug)]
pub enum LiteralValue {
    Number(f64),
    String(String),
    Bool(bool),
    Null,
}

/// A statement node.
#[derive(Clone, Debug)]
pub enum Stmt {
    Import {
        base: Base,
        path: String,
        /// The alias a whole-module import binds, empty for a named import.
        name: String,
        /// The members a named import binds, empty for a whole-module import.
        names: Vec<ImportName>,
    },
    Throw {
        base: Base,
        value: Expr,
    },
    Try {
        base: Base,
        body: Block,
        catches: Vec<Catch>,
        finally: Block,
    },
    Destructure {
        base: Base,
        names: Vec<String>,
        constant: bool,
        value: Expr,
        lookup: bool,
    },
    Expression {
        base: Base,
        value: Expr,
    },
    Assign {
        base: Base,
        target: Expr,
        value: Expr,
        operator: String,
        constant: bool,
        declaration: bool,
    },
    Function(Rc<FunctionDecl>),
    Class(Rc<ClassDecl>),
    Return {
        base: Base,
        value: Option<Expr>,
    },
    If {
        base: Base,
        branches: Vec<Branch>,
        else_block: Block,
    },
    While {
        base: Base,
        label: String,
        condition: Expr,
        body: Block,
        else_block: Block,
    },
    /// `for name in iterable { ... }`, which iterates a list, string, or dictionary.
    For {
        base: Base,
        label: String,
        name: String,
        iterable: Expr,
        body: Block,
        else_block: Block,
    },
    /// The three-clause header `for (initializer; condition; update) { ... }`,
    /// whose parentheses are optional. The `C` name is historical: the header
    /// shape is C-style, but the keyword is plain `for`.
    CFor {
        base: Base,
        label: String,
        initializer: Option<Box<Stmt>>,
        condition: Option<Expr>,
        update: Option<Box<Stmt>>,
        body: Block,
        else_block: Block,
    },
    DoWhile {
        base: Base,
        label: String,
        condition: Expr,
        body: Block,
    },
    Control {
        base: Base,
        kind: String,
        label: String,
    },
    Switch {
        base: Base,
        value: Expr,
        cases: Vec<Case>,
    },
}

impl Stmt {
    pub fn base(&self) -> &Base {
        match self {
            Stmt::Import { base, .. }
            | Stmt::Throw { base, .. }
            | Stmt::Try { base, .. }
            | Stmt::Destructure { base, .. }
            | Stmt::Expression { base, .. }
            | Stmt::Assign { base, .. }
            | Stmt::Return { base, .. }
            | Stmt::If { base, .. }
            | Stmt::While { base, .. }
            | Stmt::For { base, .. }
            | Stmt::CFor { base, .. }
            | Stmt::DoWhile { base, .. }
            | Stmt::Control { base, .. }
            | Stmt::Switch { base, .. } => base,
            Stmt::Function(declaration) => &declaration.base,
            Stmt::Class(declaration) => &declaration.base,
        }
    }
}

/// One `if`/`elif` branch.
#[derive(Clone, Debug)]
pub struct Branch {
    pub condition: Expr,
    pub body: Block,
}

/// One `catch` clause; a `None` condition matches every error.
#[derive(Clone, Debug)]
pub struct Catch {
    pub name: String,
    pub condition: Option<Expr>,
    pub body: Block,
}

/// One `switch` (or `match`) case; a `None` value is the `default` case, which
/// `case _:` also spells.
#[derive(Clone, Debug)]
pub struct Case {
    pub value: Option<Expr>,
    pub body: Block,
}

/// A named or anonymous function declaration.
#[derive(Clone, Debug)]
pub struct FunctionDecl {
    pub base: Base,
    pub access: String,
    pub name: String,
    pub parameters: Vec<Rc<str>>,
    /// Runs parallel to `parameters`; a `None` entry means "required".
    pub defaults: Vec<Option<Expr>>,
    pub variadic: String,
    pub keyword: String,
    pub body: Block,
}

/// A class field declaration.
#[derive(Clone, Debug)]
pub struct FieldDecl {
    pub base: Base,
    pub name: String,
    pub access: String,
    pub constant: bool,
    pub value: Option<Expr>,
}

/// A class declaration.
#[derive(Clone, Debug)]
pub struct ClassDecl {
    pub base: Base,
    pub name: String,
    pub parent: Option<Base>,
    pub parent_name: Option<String>,
    pub constructor: Option<Rc<FunctionDecl>>,
    pub methods: Vec<Rc<FunctionDecl>>,
    pub fields: Vec<FieldDecl>,
}

/// One positional call argument; `spread` marks a star argument.
#[derive(Clone, Debug)]
pub struct Argument {
    pub value: Expr,
    pub spread: bool,
}

/// One keyword call argument; a `spread` entry is `**value`.
#[derive(Clone, Debug)]
pub struct KeywordArgument {
    pub name: String,
    pub value: Expr,
    pub spread: bool,
}

/// One dictionary entry.
#[derive(Clone, Debug)]
pub struct Pair {
    pub key: Expr,
    pub value: Expr,
}

/// One `for name in iterable if condition ...` clause of a comprehension.
///
/// The filters belong to the clause they follow, so
/// `[v for a in xs for b in ys if p if q]` checks `p` and `q` for every `b`.
#[derive(Clone, Debug)]
pub struct ComprehensionClause {
    pub name: String,
    pub iterable: Expr,
    pub filters: Vec<Expr>,
}

/// What a comprehension builds: one value per iteration for a list, or a key and
/// a value for a dictionary.
#[derive(Clone, Debug)]
pub enum ComprehensionBody {
    List(Box<Expr>),
    Dict(Box<Pair>),
}

/// An expression node.
#[derive(Clone, Debug)]
pub enum Expr {
    Literal {
        base: Base,
        value: LiteralValue,
    },
    FormattedString {
        base: Base,
        parts: Vec<Expr>,
    },
    Interpolation {
        base: Base,
        value: Box<Expr>,
        spec: String,
    },
    Conditional {
        base: Base,
        condition: Box<Expr>,
        then: Box<Expr>,
        otherwise: Box<Expr>,
    },
    Slice {
        base: Base,
        collection: Box<Expr>,
        start: Option<Box<Expr>>,
        stop: Option<Box<Expr>>,
        step: Option<Box<Expr>>,
    },
    Identifier {
        base: Base,
        name: String,
    },
    Unary {
        base: Base,
        operator: String,
        right: Box<Expr>,
    },
    Update {
        base: Base,
        target: Box<Expr>,
        operator: String,
        prefix: bool,
    },
    Binary {
        base: Base,
        left: Box<Expr>,
        operator: String,
        right: Box<Expr>,
    },
    Call {
        base: Base,
        function: Box<Expr>,
        arguments: Vec<Argument>,
        keywords: Vec<KeywordArgument>,
    },
    Index {
        base: Base,
        collection: Box<Expr>,
        key: Box<Expr>,
    },
    Property {
        base: Base,
        receiver: Box<Expr>,
        name: String,
    },
    Super {
        base: Base,
    },
    List {
        base: Base,
        elements: Vec<Expr>,
    },
    /// A set literal: `{1, 2}`.
    Set {
        base: Base,
        elements: Vec<Expr>,
    },
    /// A tuple literal: `(1, 2)`, `(1,)`, or `()`.
    Tuple {
        base: Base,
        elements: Vec<Expr>,
    },
    /// A comprehension: `[x * 2 for x in items if x > 0]`.
    Comprehension {
        base: Base,
        body: ComprehensionBody,
        clauses: Vec<ComprehensionClause>,
    },
    Dict {
        base: Base,
        pairs: Vec<Pair>,
    },
    Function(Rc<FunctionDecl>),
}

impl Expr {
    pub fn base(&self) -> &Base {
        match self {
            Expr::Literal { base, .. }
            | Expr::FormattedString { base, .. }
            | Expr::Interpolation { base, .. }
            | Expr::Conditional { base, .. }
            | Expr::Slice { base, .. }
            | Expr::Identifier { base, .. }
            | Expr::Unary { base, .. }
            | Expr::Update { base, .. }
            | Expr::Binary { base, .. }
            | Expr::Call { base, .. }
            | Expr::Index { base, .. }
            | Expr::Property { base, .. }
            | Expr::Super { base }
            | Expr::List { base, .. }
            | Expr::Set { base, .. }
            | Expr::Tuple { base, .. }
            | Expr::Comprehension { base, .. }
            | Expr::Dict { base, .. } => base,
            Expr::Function(declaration) => &declaration.base,
        }
    }
}

/// A parsed program: its top-level statements and the names it shares.
#[derive(Clone, Debug, Default)]
pub struct Program {
    pub statements: Block,
    /// The names `export` marks, in source order. A file that marks nothing shares
    /// nothing with the files that import it.
    pub exports: Vec<ExportName>,
}

/// One member a named import binds: the name the module must export and the name
/// this file binds it to.
#[derive(Clone, Debug)]
pub struct ImportName {
    /// The name the imported module must export.
    pub name: String,
    /// The name bound in this file, which is the same unless `as` renamed it.
    pub alias: String,
    pub base: Base,
}

/// One name an `export` marker shares with the files that import this one.
#[derive(Clone, Debug)]
pub struct ExportName {
    pub name: String,
    pub base: Base,
}

/// An `import` statement: its path, the alias a whole-module import binds, and the
/// members a named import selects.
#[derive(Clone, Debug)]
pub struct ImportDecl {
    pub path: String,
    /// The alias for the whole module, empty for a named import.
    pub name: String,
    /// The members a named import binds, empty for a whole-module import.
    pub names: Vec<ImportName>,
    pub base: Base,
}

/// Collects every literal `import` statement, including those nested inside
/// functions, classes, conditionals, loops, switch cases, and try blocks.
///
/// A named import has no alias of its own, so its name is empty here; the path is
/// what a bundler needs.
pub fn imports(program: &Program) -> Vec<(String, String)> {
    import_decls(program).into_iter().map(|declaration| (declaration.path, declaration.name)).collect()
}

/// Collects every literal `import` statement together with its position, which
/// the standalone builder needs for its diagnostics.
pub fn import_decls(program: &Program) -> Vec<ImportDecl> {
    let mut found = Vec::new();
    visit_block(&program.statements, &mut found);
    found
}

fn visit_block(statements: &[Stmt], found: &mut Vec<ImportDecl>) {
    for statement in statements {
        match statement {
            Stmt::Import { base, path, name, names } => {
                found.push(ImportDecl { path: path.clone(), name: name.clone(), names: names.clone(), base: base.clone() })
            }
            Stmt::Function(declaration) => visit_block(&declaration.body, found),
            Stmt::Class(declaration) => {
                if let Some(constructor) = &declaration.constructor {
                    visit_block(&constructor.body, found);
                }
                for method in &declaration.methods {
                    visit_block(&method.body, found);
                }
            }
            Stmt::If { branches, else_block, .. } => {
                for branch in branches {
                    visit_block(&branch.body, found);
                }
                visit_block(else_block, found);
            }
            Stmt::While { body, else_block, .. } | Stmt::For { body, else_block, .. } => {
                visit_block(body, found);
                visit_block(else_block, found);
            }
            Stmt::CFor { body, else_block, .. } => {
                visit_block(body, found);
                visit_block(else_block, found);
            }
            Stmt::DoWhile { body, .. } => visit_block(body, found),
            Stmt::Switch { cases, .. } => {
                for case in cases {
                    visit_block(&case.body, found);
                }
            }
            Stmt::Try { body, catches, finally, .. } => {
                visit_block(body, found);
                for clause in catches {
                    visit_block(&clause.body, found);
                }
                visit_block(finally, found);
            }
            _ => {}
        }
    }
}
