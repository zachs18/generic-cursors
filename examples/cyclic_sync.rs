use std::sync::{Arc, Mutex};

use generic_cursors::mutex::MutexGuardStack;

#[derive(Debug, Clone)]
pub struct CyclicDataStructure<T> {
    data: T,
    next: Option<Arc<Mutex<Self>>>,
}

impl<T> CyclicDataStructure<T> {
    fn next(&mut self) -> Option<&Mutex<Self>> {
        self.next.as_deref()
    }
    fn insert_next(&mut self, new_next: Arc<Mutex<Self>>) -> Option<Arc<Mutex<Self>>> {
        self.next.replace(new_next)
    }
    fn take_next(&mut self) -> Option<Arc<Mutex<Self>>> {
        self.next.take()
    }
}

fn main() {
    let cycle_root = Arc::new(Mutex::new(CyclicDataStructure {
        data: 0_u32,
        next: None,
    }));
    let mut current = cycle_root.clone();
    for i in (1..128).rev() {
        current = Arc::new(Mutex::new(CyclicDataStructure {
            data: i,
            next: Some(current),
        }))
    }
    cycle_root.lock().unwrap().next = Some(current);

    // Using a MutRefStack to descend *and then ascend* the data structure.
    // This cannot be done with regular mutable references.
    let mut stack = MutexGuardStack::new(&cycle_root).expect("not mutable borrowed yet");
    println!("Stack currently at item with value: {}", stack.top().data);
    loop {
        if let Err(_borrow_error) = stack
            .descend_with(CyclicDataStructure::next, false)
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
