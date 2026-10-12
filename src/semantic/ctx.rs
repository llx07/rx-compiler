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

    pub fn iter(&self) -> impl Iterator<Item = (I, &D)> {
        self.definitions
            .iter()
            .enumerate()
            .map(|(index, def)| (I::from(index as u32), def))
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
pub enum BuiltinType {
    I32,
    U32,
    ISize,
    USize,
    Bool,
    Box,
    Vec,
}

impl BuiltinType {
    pub const ALL: [Self; 7] = [
        Self::I32,
        Self::U32,
        Self::ISize,
        Self::USize,
        Self::Bool,
        Self::Box,
        Self::Vec,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::I32 => "i32",
            Self::U32 => "u32",
            Self::ISize => "isize",
            Self::USize => "usize",
            Self::Bool => "bool",
            Self::Box => "Box",
            Self::Vec => "Vec",
        }
    }

    pub fn type_arg_count(self) -> usize {
        match self {
            Self::I32 | Self::U32 | Self::ISize | Self::USize | Self::Bool => 0,
            Self::Box | Self::Vec => 1,
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
pub enum TypeDef {
    Builtin {
        kind: BuiltinType,
        associated_values: Namespace<ValueId, ValueDef>,
    },
    Struct {
        name: String,
        ast_node_id: AstNodeId,
        fields: Namespace<FieldId, FieldDef>,
        associated_values: Namespace<ValueId, ValueDef>,
    },
}

impl TypeDef {
    pub fn associated_values(&self) -> &Namespace<ValueId, ValueDef> {
        match self {
            Self::Builtin {
                associated_values, ..
            }
            | Self::Struct {
                associated_values, ..
            } => associated_values,
        }
    }

    pub fn associated_values_mut(&mut self) -> &mut Namespace<ValueId, ValueDef> {
        match self {
            Self::Builtin {
                associated_values, ..
            }
            | Self::Struct {
                associated_values, ..
            } => associated_values,
        }
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
    /// The function or constant node ID; `None` for builtins.
    pub ast_node_id: Option<AstNodeId>,
    pub kind: ValueDefKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueDefKind {
    BuiltinFunction,
    Function,
    Const,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ty {
    I32,
    U32,
    ISize,
    USize,
    Bool,
    Unit,
    Never,
    Struct(TypeId),
    Ref { is_mut: bool, inner: Box<Ty> },
    Array { elem: Box<Ty>, len: usize },
    Box(Box<Ty>),
    Vec(Box<Ty>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypedConst {
    pub kind: TypedConstKind,
    pub value: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypedConstKind {
    U32,
    I32,
    Usize,
    Isize,
    Bool,
}

pub struct SemanticCtx {
    defs: DefTable,
    locals: LocalTable,
    node_defs: HashMap<AstNodeId, DefId>,

    ast_types: HashMap<AstNodeId, Ty>,
    const_values: HashMap<AstNodeId, TypedConst>,
}

impl SemanticCtx {
    pub fn new() -> Self {
        Self {
            defs: DefTable::new(),
            locals: LocalTable::new(),
            node_defs: HashMap::new(),
            ast_types: HashMap::new(),
            const_values: HashMap::new(),
        }
    }

    pub fn define_type(&mut self, name: String, def: TypeDef) -> Result<TypeId, TypeId> {
        self.defs.types.define(name, def)
    }

    pub fn define_value(&mut self, name: String, def: ValueDef) -> Result<ValueId, ValueId> {
        self.defs.values.define(name, def)
    }

    pub fn lookup_type_def(&self, name: &str) -> Option<TypeId> {
        self.defs.types.lookup(name)
    }

    pub fn lookup_value_def(&self, name: &str) -> Option<ValueId> {
        self.defs.values.lookup(name)
    }

    pub fn get_value_def(&self, id: ValueId) -> &ValueDef {
        self.defs.values.get(id)
    }

    pub fn get_type_def(&self, id: TypeId) -> &TypeDef {
        self.defs.types.get(id)
    }

    pub fn get_type_def_mut(&mut self, id: TypeId) -> &mut TypeDef {
        self.defs.types.get_mut(id)
    }

    /// Records the definition ID associated with an AST node.
    pub fn record_node_def(&mut self, node: AstNodeId, def: DefId) {
        let old = self.node_defs.insert(node, def);
        debug_assert!(old.is_none());
    }

    pub fn node_def(&self, node: AstNodeId) -> Option<DefId> {
        self.node_defs.get(&node).copied()
    }

    pub fn alloc_local(&mut self, def: LocalDef) -> LocalId {
        self.locals.alloc(def)
    }

    pub fn get_local(&self, id: LocalId) -> &LocalDef {
        self.locals.get(id)
    }

    pub fn const_value(&self, node: AstNodeId) -> Option<TypedConst> {
        self.const_values.get(&node).copied()
    }

    pub fn record_const_value(&mut self, node: AstNodeId, value: TypedConst) {
        self.const_values.insert(node, value);
    }

    pub fn get_ast_type(&self, node: AstNodeId) -> Option<&Ty> {
        self.ast_types.get(&node)
    }

    pub fn record_type(&mut self, node: AstNodeId, value: Ty) {
        self.ast_types.insert(node, value);
    }
}
