use std::collections::{HashMap, hash_map::Entry};

use crate::frontend::ast::AstNodeId;

pub struct GlobalNamespace<I, D> {
    names: HashMap<String, I>,
    definitions: Vec<D>,
}

impl<I, D> GlobalNamespace<I, D>
where
    I: Copy + From<u32> + Into<u32>,
{
    pub fn new() -> Self {
        Self {
            names: HashMap::new(),
            definitions: Vec::new(),
        }
    }

    pub fn lookup(&self, name: &str) -> Option<I> {
        self.names.get(name).copied()
    }

    pub fn get(&self, id: I) -> &D {
        &self.definitions[id.into() as usize]
    }

    /// Defines a name, returning its new ID or the existing ID on a duplicate.
    pub fn define(&mut self, name: String, def: D) -> Result<I, I> {
        match self.names.entry(name) {
            Entry::Occupied(entry) => Err(*entry.get()),
            Entry::Vacant(entry) => {
                let id = I::from(
                    u32::try_from(self.definitions.len()).expect("namespace ID space exhausted"),
                );
                self.definitions.push(def);
                entry.insert(id);
                Ok(id)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeId(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ValueId(u32);

impl From<u32> for TypeId {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl From<TypeId> for u32 {
    fn from(id: TypeId) -> Self {
        id.0
    }
}

impl From<u32> for ValueId {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl From<ValueId> for u32 {
    fn from(id: ValueId) -> Self {
        id.0
    }
}

pub struct DefTable {
    pub types: GlobalNamespace<TypeId, TypeDef>,
    pub values: GlobalNamespace<ValueId, ValueDef>,
}

impl DefTable {
    pub fn new() -> Self {
        Self {
            types: GlobalNamespace::new(),
            values: GlobalNamespace::new(),
        }
    }
}

pub struct TypeDef {
    pub name: String,
    /// The enclosing item ID for source declarations; `None` for builtins.
    pub ast_node_id: Option<AstNodeId>,
    pub kind: TypeDefKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeDefKind {
    Builtin,
    Struct,
}

pub struct ValueDef {
    pub name: String,
    /// The enclosing item or associated item ID; `None` for builtins.
    pub ast_node_id: Option<AstNodeId>,
    pub kind: ValueDefKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueDefKind {
    BuiltinFunction,
    Function,
    Const,
}

pub struct SemanticCtx {
    pub defs: DefTable,
}

impl SemanticCtx {
    pub fn new() -> Self {
        Self {
            defs: DefTable::new(),
        }
    }
}
