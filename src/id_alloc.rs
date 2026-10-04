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