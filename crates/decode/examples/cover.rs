fn main() {
    for path in std::env::args().skip(1) {
        match backspn_decode::cover_art(&path) {
            Some(bytes) => println!("{} bytes  {path}", bytes.len()),
            None => println!("no cover  {path}"),
        }
    }
}
