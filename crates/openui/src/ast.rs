//! The syntax tree of a statement's expression, as the parser reads it.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Or,
    And,
    Eq,
    NotEq,
    Greater,
    Less,
    GreaterEq,
    LessEq,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

impl BinaryOp {
    /// Higher binds tighter (language.md §1.5).
    pub fn precedence(self) -> u8 {
        match self {
            BinaryOp::Or => 2,
            BinaryOp::And => 3,
            BinaryOp::Eq | BinaryOp::NotEq => 4,
            BinaryOp::Greater | BinaryOp::Less | BinaryOp::GreaterEq | BinaryOp::LessEq => 5,
            BinaryOp::Add | BinaryOp::Sub => 6,
            BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => 7,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            BinaryOp::Or => "||",
            BinaryOp::And => "&&",
            BinaryOp::Eq => "==",
            BinaryOp::NotEq => "!=",
            BinaryOp::Greater => ">",
            BinaryOp::Less => "<",
            BinaryOp::GreaterEq => ">=",
            BinaryOp::LessEq => "<=",
            BinaryOp::Add => "+",
            BinaryOp::Sub => "-",
            BinaryOp::Mul => "*",
            BinaryOp::Div => "/",
            BinaryOp::Rem => "%",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
    Neg,
}

impl UnaryOp {
    pub fn as_str(self) -> &'static str {
        match self {
            UnaryOp::Not => "!",
            UnaryOp::Neg => "-",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Str(String),
    Num(f64),
    Bool(bool),
    Null,
    Array(Vec<Expr>),
    /// Keys in the order written.
    Object(Vec<(String, Expr)>),
    /// A bare name: another statement, or `@Each`'s loop variable.
    Ref(String),
    /// `$name`, with the `$`.
    State(String),
    /// `Name(...)`, or `@name(...)` when `at`.
    Call {
        name: String,
        args: Vec<Expr>,
        at: bool,
    },
    Unary(UnaryOp, Box<Expr>),
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
    Member(Box<Expr>, String),
    Index(Box<Expr>, Box<Expr>),
    /// `$name = expression` inside an expression: a binding's write.
    Assign(String, Box<Expr>),
}

impl Expr {
    /// Every node, parents before children.
    pub fn walk(&self, visit: &mut impl FnMut(&Expr)) {
        visit(self);
        match self {
            Expr::Array(items) => items.iter().for_each(|item| item.walk(visit)),
            Expr::Object(entries) => entries.iter().for_each(|(_, value)| value.walk(visit)),
            Expr::Call { args, .. } => args.iter().for_each(|arg| arg.walk(visit)),
            Expr::Unary(_, operand) => operand.walk(visit),
            Expr::Binary(_, left, right) => {
                left.walk(visit);
                right.walk(visit);
            }
            Expr::Ternary(cond, then, otherwise) => {
                cond.walk(visit);
                then.walk(visit);
                otherwise.walk(visit);
            }
            Expr::Member(object, _) => object.walk(visit),
            Expr::Index(object, index) => {
                object.walk(visit);
                index.walk(visit);
            }
            Expr::Assign(_, value) => value.walk(visit),
            Expr::Str(_) | Expr::Num(_) | Expr::Bool(_) | Expr::Null | Expr::Ref(_) | Expr::State(_) => {}
        }
    }

    /// The `$names` it reads, in the order met, once each.
    pub fn states(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        self.walk(&mut |expr| {
            if let Expr::State(name) = expr
                && !names.contains(name)
            {
                names.push(name.clone());
            }
        });
        names
    }

    /// The statement names it refers to, in the order met, once each.
    pub fn refs(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        self.walk(&mut |expr| {
            if let Expr::Ref(name) = expr
                && !names.contains(name)
            {
                names.push(name.clone());
            }
        });
        names
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Value,
    /// `$name = default`.
    State,
    /// `name = Query(tool, args, defaults, refresh?)`.
    Query,
    /// `name = Mutation(tool, args)`.
    Mutation,
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Kind::Value => "value",
            Kind::State => "state",
            Kind::Query => "query",
            Kind::Mutation => "mutation",
        })
    }
}

/// `name = expression`, classified as language.md §2.1 says.
#[derive(Debug, Clone, PartialEq)]
pub struct Statement {
    pub name: String,
    pub kind: Kind,
    pub expr: Expr,
    /// The `$names` the statement reads.
    pub states: Vec<String>,
}

impl Statement {
    pub fn new(name: String, state: bool, expr: Expr) -> Statement {
        let kind = match &expr {
            Expr::Call { name, .. } if name == "Query" => Kind::Query,
            Expr::Call { name, .. } if name == "Mutation" => Kind::Mutation,
            _ if state => Kind::State,
            _ => Kind::Value,
        };
        let states = expr.states();
        Statement { name, kind, expr, states }
    }

    /// The arguments of a `Query(...)` or `Mutation(...)` statement.
    pub fn call_args(&self) -> &[Expr] {
        match (&self.kind, &self.expr) {
            (Kind::Query | Kind::Mutation, Expr::Call { args, .. }) => args,
            _ => &[],
        }
    }
}
