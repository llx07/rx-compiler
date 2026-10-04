use std::marker::PhantomData;

pub struct IdAllocator<I> {
    next: u32,
    _marker: PhantomData<I>,
}

impl<I> IdAllocator<I>
where
    I: From<u32>,
{
    pub fn new() -> Self {
        Self {
            next: 0,
            _marker: PhantomData,
        }
    }

    pub fn alloc(&mut self) -> I {
        let id = self.next;
        self.next = self.next.checked_add(1).expect("ID allocator exhausted");
        id.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "ID allocator exhausted")]
    fn exhaustion_panics_instead_of_reusing_ids() {
        let mut ids = IdAllocator::<u32> {
            next: u32::MAX,
            _marker: PhantomData,
        };
        ids.alloc();
    }
}
