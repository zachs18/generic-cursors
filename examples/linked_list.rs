use generic_cursors::simple::MutRefStack;

#[derive(Debug, Clone)]
/// A simple recursive data structure
pub struct SimpleLinkedList<T> {
    data: T,
    child: Option<Box<SimpleLinkedList<T>>>,
}

impl<T> SimpleLinkedList<T> {
    fn child_mut(&mut self) -> Option<&mut Self> {
        self.child.as_deref_mut()
    }
    fn insert_child(&mut self, new_child: Box<Self>) -> Option<Box<Self>> {
        std::mem::replace(&mut self.child, Some(new_child))
    }
}

fn main() {
    let mut the_t = SimpleLinkedList {
        data: 0_u32,
        child: None,
    };

    // Using a MutRefStack to descend the data structure.
    // This could be done with regular mutable references.
    let mut stack = MutRefStack::new(&mut the_t);
    for i in 1..10 {
        stack.top_mut().insert_child(Box::new(SimpleLinkedList {
            data: i,
            child: None,
        }));
        stack.descend_with(SimpleLinkedList::child_mut).unwrap();
    }
    println!("{:?}", the_t);

    // Using regular mutable references to descend the data structure.
    let mut top = &mut the_t;
    for i in 1..10 {
        top.insert_child(Box::new(SimpleLinkedList {
            data: i,
            child: None,
        }));
        top = top.child_mut().unwrap();
    }
    println!("{:?}", the_t);

    // Using a MutRefStack to descend *and then ascend* the data structure.
    // This cannot be done with regular mutable references.
    let mut stack = MutRefStack::new(&mut the_t);
    println!("Stack currently at item with value: {}", stack.top().data);
    loop {
        if let None = stack.descend_with(SimpleLinkedList::child_mut) {
            println!("Reached the end of the linked list!");
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
}
