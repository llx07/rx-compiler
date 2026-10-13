- 签名收集
```rust
#[derive(Debug, Clone)]
pub struct FnSig {
    pub receiver: Option<ReceiverSig>,
    pub params: Vec<Ty>, // 不包含 receiver
    pub result: Ty,
}

#[derive(Debug, Clone)]
pub struct ReceiverSig {
    pub kind: ReceiverKind,
    pub ty: Ty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiverKind {
    Value,
    Shared,
    Mutable,
}
```
- derive check
