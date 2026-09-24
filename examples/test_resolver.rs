use ggg::config::{Dependency, Source};
use ggg::dependency::resolver;

fn main() {
    let url = "https://github.com/bitwes/Gut.git";

    for rev in ["v9.3.0", "main", &"a".repeat(40)] {
        let dep = Dependency::new(
            "gut",
            Source::Git {
                git: url.to_owned(),
                rev: rev.to_owned(),
            },
            None,
            None,
        );
        match resolver::resolve(&dep) {
            Ok(r) => println!("{rev:20} -> {}", r.sha()),
            Err(e) => println!("{rev:20} -> ERROR: {e:#}"),
        }
    }
}
