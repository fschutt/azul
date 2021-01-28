use azul::str::String as AzString;

fn main() {
    {
        let s = AzString::from_string(String::from("hello"));
        println!("s: {:?}", s);
    }
    println!("string dropped!");
}