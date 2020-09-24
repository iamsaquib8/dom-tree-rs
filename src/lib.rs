pub mod dom;
pub mod html;

#[cfg(test)]
mod tests {
    use std::fs;

    use super::html;
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
        // println!("{:#?}", dom::output(res.clone()));
        assert_eq!(bar.unwrap(), res.clone().to_string());
    }

    #[test]
    fn to_tree3() {
        let foo = fs::read_to_string("examples/3.html");
        let bar = fs::read_to_string("examples/2a.txt");

        let res = html::parse(foo.unwrap()).to_string();
        assert_eq!(bar.unwrap(), res);
    }
}
