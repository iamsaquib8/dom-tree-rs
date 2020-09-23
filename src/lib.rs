pub mod dom;
pub mod html;

#[cfg(test)]
mod tests {
    use std::{fmt, fs};

    use super::{dom, html};

    impl fmt::Display for dom::Node {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(
                f,
                r#"{:#?}"#,
                self
            )
        }
    }
    #[test]
    fn to_tree2() {
        let foo = fs::read_to_string("examples/2.html");
        let bar = fs::read_to_string("examples/2a.txt");
        let res = html::parse(foo.unwrap());
        assert_eq!(bar.unwrap(), res.to_string());
    }
    #[test]
    fn to_tree1() {
        let foo = fs::read_to_string("examples/1.html");
        let bar = fs::read_to_string("examples/1a.txt");
        let res = html::parse(foo.unwrap());
        assert_eq!(bar.unwrap(), res.to_string());
    }
}
