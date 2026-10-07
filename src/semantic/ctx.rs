use std::collections::{HashMap, hash_map::Entry};

use crate::frontend::ast::AstNodeId;

/// IDs are local to this namespace and must be queried through the same instance.
#[derive(Debug)]
pub struct Namespace<I, D> {
    names: HashMap<String, I>,
    definitions: Vec<D>,
}

impl<I, D> Namespace<I, D>
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

    pub fn get_mut(&mut self, id: I) -> &mut D {
        &mut self.definitions[id.into() as usize]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FieldId(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LocalId(u32);

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

impl From<u32> for FieldId {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl From<FieldId> for u32 {
    fn from(id: FieldId) -> Self {
        id.0
    }
}

impl From<u32> for LocalId {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl From<LocalId> for u32 {
    fn from(id: LocalId) -> Self {
        id.0
    }
}

pub struct DefTable {
    pub types: Namespace<TypeId, TypeDef>,
    pub values: Namespace<ValueId, ValueDef>,
}

impl DefTable {
    pub fn new() -> Self {
        Self {
            types: Namespace::new(),
            values: Namespace::new(),
        }
    }
}

#[derive(Debug)]
pub struct TypeDef {
    pub name: String,
    /// The enclosing item ID for source declarations; `None` for builtins.
    pub ast_node_id: Option<AstNodeId>,
    pub fields: Namespace<FieldId, FieldDef>,
    pub associated_values: Namespace<ValueId, ValueDef>,
}

impl TypeDef {
    pub fn is_builtin(&self) -> bool {
        matches!(
            self.name.as_str(),
            "i32" | "u32" | "isize" | "usize" | "bool" | "Box" | "Vec"
        )
    }
}

#[derive(Debug)]
pub struct FieldDef {
    pub name: String,
    pub ast_node_id: AstNodeId,
}

#[derive(Debug)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefId {
    Type(TypeId),
    GlobalValue(ValueId),
    AssociatedValue { owner: TypeId, value: ValueId },
    Local(LocalId),
    Field { owner: TypeId, field: FieldId },
}

pub struct LocalTable {
    table: Vec<LocalDef>,
}

impl LocalTable {
    pub fn new() -> Self {
        Self { table: Vec::new() }
    }

    pub fn alloc(&mut self, def: LocalDef) -> LocalId {
        let id = LocalId(u32::try_from(self.table.len()).expect("local ID space exhausted"));
        self.table.push(def);
        id
    }

    pub fn get(&self, id: LocalId) -> &LocalDef {
        &self.table[id.0 as usize]
    }
}

#[derive(Debug)]
pub struct LocalDef {
    pub name: String,
    pub ast_node_id: AstNodeId,
    pub is_mut: bool,
    pub kind: LocalDefKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalDefKind {
    Param,
    Let,
    Receiver { ty: TypeId },
}

pub struct SemanticCtx {
    defs: DefTable,
    locals: LocalTable,
    bindings: HashMap<AstNodeId, DefId>,
}

impl SemanticCtx {
    pub fn new() -> Self {
        Self {
            defs: DefTable::new(),
            locals: LocalTable::new(),
            bindings: HashMap::new(),
        }
    }

    pub fn define_type(&mut self, name: String, def: TypeDef) -> Result<TypeId, TypeId> {
        self.defs.types.define(name, def)
    }

    pub fn define_value(&mut self, name: String, def: ValueDef) -> Result<ValueId, ValueId> {
        self.defs.values.define(name, def)
    }

    pub fn lookup_type(&self, name: &str) -> Option<TypeId> {
        self.defs.types.lookup(name)
    }

    pub fn lookup_value(&self, name: &str) -> Option<ValueId> {
        self.defs.values.lookup(name)
    }

    pub fn get_type(&self, id: TypeId) -> &TypeDef {
        self.defs.types.get(id)
    }

    pub fn get_type_mut(&mut self, id: TypeId) -> &mut TypeDef {
        self.defs.types.get_mut(id)
    }

    pub fn bind(&mut self, node: AstNodeId, def: DefId) {
        let old = self.bindings.insert(node, def);
        debug_assert!(old.is_none());
    }

    pub fn binding(&self, node: AstNodeId) -> Option<DefId> {
        self.bindings.get(&node).copied()
    }

    pub fn alloc_local(&mut self, def: LocalDef) -> LocalId {
        self.locals.alloc(def)
    }

    pub fn get_local(&self, id: LocalId) -> &LocalDef {
        self.locals.get(id)
    }
}
