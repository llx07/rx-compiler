use std::collections::{HashMap, VecDeque};

use crate::diagnostic::{Diagnostic, Span};
use crate::frontend::ast::{Crate, ItemKind, TraitKind};
use crate::semantic::ctx::{DefId, SemanticCtx, Ty, TypeId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct TraitCheckNode {
    def_id: DefId,
    tr: TraitKind,
}

enum Requirement {
    Satisfied,
    Dependency(TypeId),
    Unsatisfied,
}

struct TraitChecker<'a> {
    ctx: &'a mut SemanticCtx,
    diagnostics: &'a mut Vec<Diagnostic>,
    // A depends on B: store A in B's predecessor list, together with the reason
    // to report if B becomes invalid. Only initially set traits enter the graph.
    predecessor: HashMap<TraitCheckNode, Vec<(TraitCheckNode, String)>>,
    derive_spans: HashMap<TraitCheckNode, Span>,
    invalid: Vec<(TraitCheckNode, String)>,
    queue_unset: VecDeque<TraitCheckNode>,
}

impl<'a> TraitChecker<'a> {
    fn new(ctx: &'a mut SemanticCtx, diagnostics: &'a mut Vec<Diagnostic>) -> Self {
        Self {
            ctx,
            diagnostics,
            predecessor: HashMap::new(),
            derive_spans: HashMap::new(),
            invalid: Vec::new(),
            queue_unset: VecDeque::new(),
        }
    }

    fn check(mut self, ast: &Crate) {
        self.build_graph(ast);
        self.propagate();
    }

    // Follow type constructors, stopping at a struct instead of expanding its fields.
    // Struct recursion is handled by the dependency graph.
    fn field_requirement(mut ty: &Ty, tr: TraitKind) -> Requirement {
        loop {
            match ty {
                Ty::I32 | Ty::U32 | Ty::ISize | Ty::USize | Ty::Bool | Ty::Unit | Ty::Never => {
                    return Requirement::Satisfied;
                }
                Ty::Struct(id) => return Requirement::Dependency(*id),
                Ty::Ref { is_mut, inner } => match tr {
                    TraitKind::Copy | TraitKind::Clone => {
                        return if *is_mut {
                            Requirement::Unsatisfied
                        } else {
                            Requirement::Satisfied
                        };
                    }
                    TraitKind::PartialEq | TraitKind::Eq => ty = inner,
                },
                Ty::Array { elem, .. } => ty = elem,
                Ty::Box(inner) | Ty::Vec(inner) => match tr {
                    TraitKind::Copy => return Requirement::Unsatisfied,
                    TraitKind::Clone | TraitKind::PartialEq | TraitKind::Eq => ty = inner,
                },
            }
        }
    }

    fn unset_and_enqueue(&mut self, node: TraitCheckNode, reason: &str) {
        let def = node
            .def_id
            .as_struct_def_mut(self.ctx)
            .expect("trait graph nodes must be structs");
        if !def.traits.contains(node.tr) {
            return;
        }

        // Unset before enqueueing so the trait bit also prevents duplicate work.
        def.traits.unset(node.tr);
        self.diagnostics.push(Diagnostic::new(
            format!(
                "Cannot derive {:?} for struct '{}': {reason}",
                node.tr, def.name
            ),
            self.derive_spans[&node].clone(),
        ));
        self.queue_unset.push_back(node);
    }

    fn build_graph(&mut self, ast: &Crate) {
        for item in &ast.items {
            let ItemKind::Struct(structure) = &item.kind else {
                continue;
            };
            let def_id = self
                .ctx
                .node_def(structure.id)
                .expect("struct must be bound before trait checking");
            let def = def_id
                .as_struct_def(self.ctx)
                .expect("struct must have a struct definition");

            for tr in [
                TraitKind::Clone,
                TraitKind::Copy,
                TraitKind::PartialEq,
                TraitKind::Eq,
            ] {
                if !def.traits.contains(tr) {
                    continue;
                }
                let node = TraitCheckNode { def_id, tr };
                let derive = structure
                    .derives
                    .iter()
                    .find(|derive| derive.kind == tr)
                    .expect("set struct traits must have an explicit derive");
                self.derive_spans.insert(node, derive.span.clone());
                self.predecessor.entry(node).or_default();

                let prerequisite = match tr {
                    TraitKind::Copy => Some(TraitKind::Clone),
                    TraitKind::Eq => Some(TraitKind::PartialEq),
                    TraitKind::Clone | TraitKind::PartialEq => None,
                };
                if let Some(required) = prerequisite {
                    let reason = format!("requires {required:?} on the same struct");
                    if def.traits.contains(required) {
                        self.predecessor
                            .entry(TraitCheckNode {
                                def_id,
                                tr: required,
                            })
                            .or_default()
                            .push((node, reason));
                    } else {
                        self.invalid.push((node, reason));
                    }
                }

                for (_, field) in def.fields.iter() {
                    let ty = field
                        .ty
                        .as_ref()
                        .expect("field types must be resolved before trait checking");
                    let reason = format!("field '{}' does not implement {tr:?}", field.name);
                    match Self::field_requirement(ty, tr) {
                        Requirement::Satisfied => {}
                        Requirement::Unsatisfied => self.invalid.push((node, reason)),
                        Requirement::Dependency(dependency) => {
                            if self.ctx.has_derive(dependency, tr) == Some(true) {
                                self.predecessor
                                    .entry(TraitCheckNode {
                                        def_id: DefId::Type(dependency),
                                        tr,
                                    })
                                    .or_default()
                                    .push((node, reason));
                            } else {
                                self.invalid.push((node, reason));
                            }
                        }
                    }
                }
            }
        }
    }

    fn propagate(&mut self) {
        // Finish the graph before changing any bits, including for forward references.
        for (node, reason) in std::mem::take(&mut self.invalid) {
            self.unset_and_enqueue(node, &reason);
        }
        while let Some(node) = self.queue_unset.pop_front() {
            let predecessors = self
                .predecessor
                .remove(&node)
                .expect("queued traits must be graph nodes");
            for (predecessor, reason) in predecessors {
                self.unset_and_enqueue(predecessor, &reason);
            }
        }
    }
}

pub fn trait_check(ast: &Crate, ctx: &mut SemanticCtx, diagnostics: &mut Vec<Diagnostic>) {
    TraitChecker::new(ctx, diagnostics).check(ast);
}
