use ggg::config::Dependency;
use ggg::dependency::resolver;

fn main() {
    let url = "https://github.com/bitwes/Gut.git";

    for rev in ["v9.3.0", "main", &"a".repeat(40)] {
        let dep = Dependency::new_git("gut", url, rev);
        match resolver::resolve(&dep) {
            Ok(r)  => println!("{rev:20} -> {}", r.sha),
            Err(e) => println!("{rev:20} -> ERROR: {e:#}"),
        }
    }
}
