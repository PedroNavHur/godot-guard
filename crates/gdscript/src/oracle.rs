//! Read `tools/oracle/dump_tree.py` output, so ports and parity tests can run on gdtoolkit's own trees.

use serde_json::Value;

use crate::tree::{Node, Span, Token, Tree, intern};

/// One line of a dump: the file's trees, or the error gdtoolkit raised.
#[derive(Debug)]
pub enum Dump {
    Parsed {
        path: String,
        tree: Tree,
        comments: Tree,
    },
    Rejected {
        path: String,
        error: Value,
    },
}

pub fn read_line(line: &str) -> Result<Dump, String> {
    // Expression trees nest deeper than serde_json's default limit of 128.
    let mut reader = serde_json::Deserializer::from_str(line);
    reader.disable_recursion_limit();
    let value: Value = serde::Deserialize::deserialize(&mut reader).map_err(|e| e.to_string())?;
    let path = value["path"]
        .as_str()
        .ok_or("dump line has no path")?
        .to_owned();
    if let Some(error) = value.get("error") {
        return Ok(Dump::Rejected {
            path,
            error: error.clone(),
        });
    }
    let tree = tree_from(&value["tree"])?;
    let comments = tree_from(&value["comments"])?;
    Ok(Dump::Parsed {
        path,
        tree,
        comments,
    })
}

fn tree_from(value: &Value) -> Result<Tree, String> {
    match node(value)? {
        Node::Tree(tree) => Ok(tree),
        Node::Token(_) => Err("expected a tree, found a token".into()),
    }
}

pub fn node(value: &Value) -> Result<Node, String> {
    if let Some(kind) = value.get("k") {
        let kind = intern(kind.as_str().ok_or("token kind is not a string")?);
        let text = value["v"]
            .as_str()
            .ok_or("token value is not a string")?
            .to_owned();
        let span = span(&value["p"])?.ok_or("token has no position")?;
        return Ok(Node::Token(Token {
            kind,
            value: text,
            span,
        }));
    }
    let data = intern(value["t"].as_str().ok_or("tree has no rule name")?);
    let children = value["c"].as_array().ok_or("tree has no children")?;
    let children = children.iter().map(node).collect::<Result<Vec<_>, _>>()?;
    Ok(Node::Tree(Tree {
        data,
        children,
        meta: span(&value["m"])?,
    }))
}

fn span(value: &Value) -> Result<Option<Span>, String> {
    if value.is_null() {
        return Ok(None);
    }
    let field = |index: usize| {
        value[index]
            .as_u64()
            .map(|n| n as usize)
            .ok_or_else(|| format!("span field {index} is not a number"))
    };
    let optional = |index: usize| match value[index].is_null() {
        true => Ok(None),
        false => field(index).map(Some),
    };
    Ok(Some(Span {
        line: field(0)?,
        column: field(1)?,
        end_line: field(2)?,
        end_column: optional(3)?,
        start_pos: field(4)?,
        end_pos: optional(5)?,
    }))
}
