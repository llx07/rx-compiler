use std::collections::{HashMap, hash_map::Entry};

use crate::diagnostic::Span;
use crate::frontend::ast::{AstNodeId, TraitKind};

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

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (I, &mut D)> {
        self.definitions
            .iter_mut()
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
#[derive(Debug, Clone, Copy, Default)]
pub struct TraitSet(u8);

impl TraitSet {
    pub fn new() -> Self {
        Self(0)
    }

    pub fn contains(&self, tr: TraitKind) -> bool {
        let bit = 1u8 << (tr as u8);
        self.0 & bit != 0
    }

    pub fn set(&mut self, tr: TraitKind) {
        self.0 |= 1u8 << (tr as u8);
    }

    pub fn unset(&mut self, tr: TraitKind) {
        self.0 &= !(1u8 << (tr as u8));
    }
}

#[derive(Debug)]
pub enum TypeDef {
    Builtin(BuiltinDef),
    Struct(StructDef),
}

#[derive(Debug)]
pub struct BuiltinDef {
    pub kind: BuiltinType,
    pub associated_values: Namespace<ValueId, ValueDef>,
}

#[derive(Debug)]
pub struct StructDef {
    pub name: String,
    pub ast_node_id: AstNodeId,
    pub span: Span,
    pub fields: Namespace<FieldId, FieldDef>,
    pub associated_values: Namespace<ValueId, ValueDef>,
    pub traits: TraitSet,
}

impl TypeDef {
    pub fn associated_values(&self) -> &Namespace<ValueId, ValueDef> {
        match self {
            Self::Builtin(def) => &def.associated_values,
            Self::Struct(def) => &def.associated_values,
        }
    }

    pub fn associated_values_mut(&mut self) -> &mut Namespace<ValueId, ValueDef> {
        match self {
            Self::Builtin(def) => &mut def.associated_values,
            Self::Struct(def) => &mut def.associated_values,
        }
    }
}

#[derive(Debug)]
pub struct FieldDef {
    pub name: String,
    pub ast_node_id: AstNodeId,
    pub span: Span,
    pub ty: Option<Ty>,
}

#[derive(Debug)]
pub struct ValueDef {
    pub name: String,
    /// The function or constant node ID; `None` for builtins.
    pub ast_node_id: Option<AstNodeId>,
    /// The function or constant node span; `None` for builtins.
    pub span: Option<Span>,
    pub kind: ValueDefKind,
}

impl ValueDef {
    pub fn is_const(&self) -> bool {
        self.kind == ValueDefKind::Const
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueDefKind {
    BuiltinFunction,
    Function(FunctionDef),
    Const,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionDef {
    pub sig: Option<FnSig>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FnSig {
    pub receiver: Option<Ty>,
    pub params: Vec<Ty>, // not including receiver
    pub result: Ty,
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
    pub span: Span,
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

#[derive(Debug, Default)]
pub struct NodeInfo {
    pub def: Option<DefId>,
    pub ty: Option<Ty>,
    pub constant: Option<TypedConst>,
    pub expr: Option<ExprSemantics>,
}

#[derive(Debug, Default)]
pub struct ExprSemantics {
    pub place: Option<PlaceInfo>,
    pub access: Option<ResolvedAccess>,
    pub adjustment: Option<UseAdjustment>,
    pub call: Option<ResolvedCall>,
    pub jump: Option<JumpTarget>,
}

#[derive(Debug, Clone, Copy)]
pub struct PlaceInfo {
    pub writable: bool,
    pub mutable_access_blocked: bool,
    pub origin: PlaceOrigin,
}

#[derive(Debug, Clone, Copy)]
pub enum PlaceOrigin {
    Existing,
    Temporary,
}

#[derive(Debug, Clone, Copy)]
pub enum ResolvedAccess {
    Deref,

    Field { owner: TypeId, field: FieldId },

    Index(IndexKind),
}

#[derive(Debug, Clone, Copy)]
pub enum IndexKind {
    Array,
    Vec,
}

#[derive(Debug, Clone)]
pub struct UseAdjustment {
    pub target: Ty,
    pub steps: Vec<Adjustment>,
}

#[derive(Debug, Clone)]
pub enum Adjustment {
    Deref,
    Borrow { is_mut: bool },
    MaterializeTemporary,
    NeverTo,
}

#[derive(Debug, Clone)]
pub struct ResolvedCall {
    pub target: CallTarget,
    pub sig: FnSig,
}

#[derive(Debug, Clone)]
pub enum CallTarget {
    User(DefId),
    Builtin { op: BuiltinOp, type_args: Vec<Ty> },
}

#[derive(Debug, Clone, Copy)]
pub enum BuiltinOp {
    GetI32,
    PrintI32,
    PrintlnI32,

    BoxNew,
    VecNew,
    VecLen,
    VecIsEmpty,
    VecPush,
    VecRemove,

    ArrayLen,
    Clone,
}

#[derive(Debug, Clone, Copy)]
pub enum JumpTarget {
    Return(AstNodeId),
    Break(AstNodeId),
    Continue(AstNodeId),
}

pub struct SemanticCtx {
    defs: DefTable,
    locals: LocalTable,
    node_info: HashMap<AstNodeId, NodeInfo>,
}

impl SemanticCtx {
    pub fn new() -> Self {
        Self {
            defs: DefTable::new(),
            locals: LocalTable::new(),
            node_info: HashMap::new(),
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

    pub fn value_def(&self, id: DefId) -> Option<&ValueDef> {
        match id {
            DefId::GlobalValue(value) => Some(self.defs.values.get(value)),
            DefId::AssociatedValue { owner, value } => {
                Some(self.defs.types.get(owner).associated_values().get(value))
            }
            _ => None,
        }
    }

    pub fn value_def_mut(&mut self, id: DefId) -> Option<&mut ValueDef> {
        match id {
            DefId::GlobalValue(value) => Some(self.defs.values.get_mut(value)),
            DefId::AssociatedValue { owner, value } => Some(
                self.defs
                    .types
                    .get_mut(owner)
                    .associated_values_mut()
                    .get_mut(value),
            ),
            _ => None,
        }
    }

    pub fn get_type_def(&self, id: TypeId) -> &TypeDef {
        self.defs.types.get(id)
    }

    pub fn get_type_def_mut(&mut self, id: TypeId) -> &mut TypeDef {
        self.defs.types.get_mut(id)
    }

    pub fn iter_types(&self) -> impl Iterator<Item = (TypeId, &TypeDef)> {
        self.defs.types.iter()
    }

    pub fn iter_types_mut(&mut self) -> impl Iterator<Item = (TypeId, &mut TypeDef)> {
        self.defs.types.iter_mut()
    }

    /// Records the definition ID associated with an AST node.
    pub fn record_node_def(&mut self, node: AstNodeId, def: DefId) {
        let old = self.node_info.entry(node).or_default().def.replace(def);
        debug_assert!(old.is_none());
    }

    pub fn node_def(&self, node: AstNodeId) -> Option<DefId> {
        self.node_info.get(&node).and_then(|info| info.def)
    }

    pub fn alloc_local(&mut self, def: LocalDef) -> LocalId {
        self.locals.alloc(def)
    }

    pub fn get_local(&self, id: LocalId) -> &LocalDef {
        self.locals.get(id)
    }

    pub fn node_const(&self, node: AstNodeId) -> Option<TypedConst> {
        self.node_info.get(&node).and_then(|info| info.constant)
    }

    pub fn record_node_const(&mut self, node: AstNodeId, value: TypedConst) {
        self.node_info.entry(node).or_default().constant = Some(value);
    }

    pub fn node_ty(&self, node: AstNodeId) -> Option<&Ty> {
        self.node_info.get(&node).and_then(|info| info.ty.as_ref())
    }

    /// Returns the adjusted value type, falling back to the node's original type.
    pub fn node_value_ty(&self, node: AstNodeId) -> Option<&Ty> {
        let info = self.node_info.get(&node)?;
        info.expr
            .as_ref()
            .and_then(|expr| expr.adjustment.as_ref())
            .map(|adjustment| &adjustment.target)
            .or(info.ty.as_ref())
    }

    pub fn record_node_ty(&mut self, node: AstNodeId, value: Ty) {
        self.node_info.entry(node).or_default().ty = Some(value);
    }

    pub fn node_place(&self, node: AstNodeId) -> Option<PlaceInfo> {
        self.node_info
            .get(&node)
            .and_then(|info| info.expr.as_ref())
            .and_then(|expr| expr.place)
    }

    pub fn record_node_place(&mut self, node: AstNodeId, place: PlaceInfo) {
        self.node_info
            .entry(node)
            .or_default()
            .expr
            .get_or_insert_with(ExprSemantics::default)
            .place = Some(place);
    }

    pub fn record_node_adjustment(&mut self, node: AstNodeId, adjustment: UseAdjustment) {
        self.node_info
            .entry(node)
            .or_default()
            .expr
            .get_or_insert_with(ExprSemantics::default)
            .adjustment = Some(adjustment);
    }

    pub fn implements_trait(&self, ty: &Ty, tr: TraitKind) -> bool {
        match ty {
            Ty::I32 | Ty::U32 | Ty::ISize | Ty::USize | Ty::Bool | Ty::Unit | Ty::Never => true,
            // Trust declared derives; struct field validation is a separate step.
            Ty::Struct(id) => self.has_derive(*id, tr).unwrap_or(false),
            Ty::Ref { is_mut, inner } => match tr {
                TraitKind::Copy | TraitKind::Clone => !is_mut,
                TraitKind::PartialEq | TraitKind::Eq => self.implements_trait(inner, tr),
            },
            Ty::Array { elem, .. } => self.implements_trait(elem, tr),
            Ty::Box(inner) | Ty::Vec(inner) => match tr {
                TraitKind::Copy => false,
                TraitKind::Clone | TraitKind::PartialEq | TraitKind::Eq => {
                    self.implements_trait(inner, tr)
                }
            },
        }
    }

    /// Returns the declared derive capability of a user struct, or `None` for a builtin type.
    pub fn has_derive(&self, id: TypeId, tr: TraitKind) -> Option<bool> {
        match self.get_type_def(id) {
            TypeDef::Struct(def) => Some(def.traits.contains(tr)),
            TypeDef::Builtin(_) => None,
        }
    }
}
