use goofy_ahh_alloc::FileAllocator;

#[global_allocator]
static GLOBAL: FileAllocator = FileAllocator;

#[test]
fn main() {
    let mut name = String::from("bruh");
    let boxed = Box::new(String::from("ohayo"));

    println!("hello {name}");

    name = String::from("123e");

    println!("hello {name}");
    println!("hello {boxed}");
}
