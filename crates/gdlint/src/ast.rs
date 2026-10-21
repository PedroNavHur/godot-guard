//! `gdtoolkit/common/ast.py`: classes, their statements and functions, read from the parse tree.
//!
//! Python builds a `Statement` for every class-level statement, sub-statements included; only a
//! `Function`'s sub-statements are ever read (by max-returns), so this computes them for functions alone.
//! The trees whose shape would crash the Python (a token where it reads `.data`) panic here too.

use gdscript::{Node, Tree};

use crate::utils::{
    child_token, child_tree, find_name_token_among_children, find_tree_among_children,
};

/// `gdtoolkit/formatter/annotation.py`'s `_STANDALONE_ANNOTATIONS`.
const STANDALONE_ANNOTATIONS: [&str; 8] = [
    "abstract",
    "export_category",
    "export_group",
    "export_subgroup",
    "icon",
    "tool",
    "warning_ignore_start",
    "warning_ignore_restore",
];

/// `_NON_STANDALONE_WARNING_IGNORES`: warnings a `@warning_ignore` attaches to the next statement for.
const NON_STANDALONE_WARNING_IGNORES: [&str; 35] = [
    "unassigned_variable",
    "unassigned_variable_op_assign",
    "unused_parameter",
    "shadowed_global_identifier",
    "shadowed_variable",
    "shadowed_variable_base_class",
    "unreachable_code",
    "unreachable_pattern",
    "standalone_expression",
    "standalone_ternary",
    "incompatible_ternary",
    "untyped_declaration",
    "inferred_declaration",
    "unsafe_property_access",
    "unsafe_method_access",
    "unsafe_cast",
    "unsafe_call_argument",
    "unsafe_void_return",
    "return_value_discarded",
    "static_called_on_instance",
    "redundant_await",
    "assert_always_true",
    "assert_always_false",
    "integer_division",
    "narrowing_conversion",
    "int_as_enum_without_cast",
    "int_as_enum_without_match",
    "enum_variable_without_default",
    "deprecated_keyword",
    "confusable_identifier",
    "confusable_local_declaration",
    "confusable_local_usage",
    "confusable_capture_reassignment",
    "inference_on_variant",
    "native_method_override",
];

/// `is_non_standalone_annotation` from `gdtoolkit/formatter/annotation.py`.
pub fn is_non_standalone_annotation(statement: &Tree) -> bool {
    if statement.data != "annotation" {
        return false;
    }
    let name = annotation_name(statement);
    if STANDALONE_ANNOTATIONS.contains(&name) {
        return false;
    }
    if name != "warning_ignore" {
        return true;
    }
    let first_argument = child_tree(child_tree(statement, 1), 0);
    let ignoree = child_token(first_argument, 0).value.trim_matches('"');
    NON_STANDALONE_WARNING_IGNORES.contains(&ignoree)
}

/// `Annotation(node).name`.
pub fn annotation_name(annotation: &Tree) -> &str {
    &child_token(annotation, 0).value
}

pub struct Statement<'a> {
    pub lark_node: &'a Tree,
    pub kind: &'static str,
    /// Names of the non-standalone annotations written before it.
    pub annotations: Vec<&'a str>,
}

pub struct Function<'a> {
    pub lark_node: &'a Tree,
    pub name: &'a str,
    pub parameters: usize,
    /// The function's own node, then every statement in it, depth first, as `Function.all_statements`.
    pub all_statements: Vec<&'a Tree>,
}

impl<'a> Function<'a> {
    fn new(func_def: &'a Tree) -> Function<'a> {
        let func_header = child_tree(func_def, 0);
        let name = find_name_token_among_children(func_header)
            .unwrap_or_else(|| panic!("{} has no name", func_header.data));
        let func_args = find_tree_among_children("func_args", func_header)
            .unwrap_or_else(|| panic!("{} has no func_args", func_header.data));
        let parameters = func_args
            .children
            .iter()
            .filter(|child| as_statement(child).data != "trailing_comma")
            .count();
        let mut all_statements = vec![func_def];
        load_all_sub_statements(func_def, &mut all_statements);
        Function {
            lark_node: func_def,
            name: &name.value,
            parameters,
            all_statements,
        }
    }
}

/// A child the Python wraps in a `Statement` or reads `.data` from.
fn as_statement(node: &Node) -> &Tree {
    match node {
        Node::Tree(tree) => tree,
        Node::Token(token) => panic!("token {token:?} where a statement was expected"),
    }
}

/// `Statement._load_sub_statements`, flattened as `all_sub_statements`.
fn load_all_sub_statements<'a>(statement: &'a Tree, out: &mut Vec<&'a Tree>) {
    for sub_statement in sub_statements(statement) {
        out.push(sub_statement);
        load_all_sub_statements(sub_statement, out);
    }
}

fn sub_statements(statement: &Tree) -> Vec<&Tree> {
    let children = &statement.children;
    let from = |start: usize| {
        children
            .iter()
            .skip(start)
            .map(as_statement)
            .collect::<Vec<_>>()
    };
    match statement.data {
        "property_body_def" => panic!("NotImplementedError: sub-statements of property_body_def"),
        "func_def" | "static_func_def" | "while_stmt" => from(1),
        "for_stmt" => from(2),
        "for_stmt_typed" => from(3),
        "if_stmt" => children
            .iter()
            .map(as_statement)
            .flat_map(|branch| {
                let skip = usize::from(matches!(branch.data, "if_branch" | "elif_branch"));
                branch.children.iter().skip(skip).map(as_statement)
            })
            .collect(),
        "match_stmt" => children
            .iter()
            .map(as_statement)
            .flat_map(|branch| branch.children.iter().skip(1).map(as_statement))
            .collect(),
        _ => Vec::new(),
    }
}

pub struct Class<'a> {
    pub lark_node: &'a Tree,
    pub name: &'a str,
    /// Names of the functions defined directly in this class.
    pub function_names: Vec<&'a str>,
    pub statements: Vec<Statement<'a>>,
}

/// `AbstractSyntaxTree`: every class, the global scope first and the rest depth first, and every function
/// in them, in the order `Class.all_functions` lists them.
pub struct AbstractSyntaxTree<'a> {
    pub all_classes: Vec<Class<'a>>,
    pub all_functions: Vec<Function<'a>>,
}

impl<'a> AbstractSyntaxTree<'a> {
    pub fn new(parse_tree: &'a Tree) -> AbstractSyntaxTree<'a> {
        assert_eq!(parse_tree.data, "start", "Cannot load class from that node");
        let mut ast = AbstractSyntaxTree {
            all_classes: Vec::new(),
            all_functions: Vec::new(),
        };
        ast.load_class(parse_tree, "global scope");
        ast
    }

    fn load_class(&mut self, node: &'a Tree, name: &'a str) {
        let index = self.all_classes.len();
        self.all_classes.push(Class {
            lark_node: node,
            name,
            function_names: Vec::new(),
            statements: Vec::new(),
        });
        let offset = usize::from(node.data == "class_def");
        let mut annotations = Vec::new();
        for stmt in node.children.iter().skip(offset).map(as_statement) {
            if stmt.data == "annotation" && is_non_standalone_annotation(stmt) {
                annotations.push(annotation_name(stmt));
                continue;
            }
            if stmt.data == "property_body_def" {
                continue;
            }
            let statement = Statement {
                lark_node: stmt,
                kind: stmt.data,
                annotations: std::mem::take(&mut annotations),
            };
            self.all_classes[index].statements.push(statement);
            match stmt.data {
                "class_def" => {
                    let class_name =
                        find_name_token_among_children(stmt).expect("class_def has a name");
                    self.load_class(stmt, &class_name.value);
                }
                "func_def" | "abstract_func_def" => {
                    let function = Function::new(stmt);
                    self.all_classes[index].function_names.push(function.name);
                    self.all_functions.push(function);
                }
                _ => {}
            }
        }
    }
}
