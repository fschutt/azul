use std::env;

fn main() {
    // MAKE SURE CLANG IS INSTALLED!
    env::set_var("CC", "clang-cl");
    env::set_var("CXX", "clang-cl");
}