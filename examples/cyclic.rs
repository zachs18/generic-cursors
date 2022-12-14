use std::{cell::RefCell, rc::Rc};

use generic_cursors::refcell::RefCellRefMutStack;

#[derive(Debug, Clone)]
pub struct CyclicDataStructure<T> {
    data: T,
    next: Option<Rc<RefCell<Self>>>,
}

impl<T> CyclicDataStructure<T> {
    fn next(&mut self) -> Option<&RefCell<Self>> {
        self.next.as_deref()
    }
    fn insert_next(&mut self, new_next: Rc<RefCell<Self>>) -> Option<Rc<RefCell<Self>>> {
        self.next.replace(new_next)
    }
    fn take_next(&mut self) -> Option<Rc<RefCell<Self>>> {
        self.next.take()
    }
}

fn main() {
    let cycle_a = Rc::new(RefCell::new(CyclicDataStructure {
        data: 0_u32,
        next: None,
    }));
    let cycle_b = Rc::new(RefCell::new(CyclicDataStructure {
        data: 1_u32,
        next: None,
    }));
    let cycle_c = Rc::new(RefCell::new(CyclicDataStructure {
        data: 2_u32,
        next: None,
    }));

    cycle_a.borrow_mut().insert_next(cycle_b.clone());
    cycle_b.borrow_mut().insert_next(cycle_c.clone());
    cycle_c.borrow_mut().insert_next(cycle_a.clone());

    // Using a MutRefStack to descend *and then ascend* the data structure.
    // This cannot be done with regular mutable references.
    let mut stack = RefCellRefMutStack::new(&cycle_a).expect("not mutable borrowed yet");
    println!("Stack currently at item with value: {}", stack.top().data);
    loop {
        if let Err(_borrow_error) = stack
            .descend_with(CyclicDataStructure::next)
            .expect("no node has no next")
        {
            println!("Found a cycle!");
            break;
        }
        println!("Descended successfully!");
        println!("Stack currently at item with value: {}", stack.top().data);
    }
    println!("Stack currently at item with value: {}", stack.top().data);
    loop {
        if let None = stack.ascend() {
            println!("Reached the head of the linked list!");
            break;
        }
        println!("Ascended successfully!");
        println!("Stack currently at item with value: {}", stack.top().data);
    }

    println!("(Breaking the cycle to prevent miri from complaining about memory leaks)");
    stack.top_mut().take_next();
}
