use ggg::dependency::resolver;

fn main() {
    let url = "https://github.com/bitwes/Gut.git";

    for rev in ["v9.3.0", "main", &"a".repeat(40)] {
        match resolver::resolve(url, rev) {
            Ok(sha) => println!("{rev:20} -> {sha}"),
            Err(e)  => println!("{rev:20} -> ERROR: {e:#}"),
        }
    }
}
