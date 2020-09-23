use crate::dom;
use std::collections::HashMap;

pub fn parse(html: String) -> dom::Node {
  let mut nodes = Parser { index: 0, html }.parse_nodes();

  if nodes.len() == 1 {
    nodes.swap_remove(0)
  } else {
    dom::new_data("html".to_string(), HashMap::new(), nodes)
  }
}

struct Parser {
  index: usize,
  html: String,
}

impl Parser {
  fn parse_nodes(&mut self) -> Vec<dom::Node> {
    let mut nodes = vec!();
    loop {
      self.iterate_whitespace();
      if self.eof() || self.starts_with("</") {
        break;
      }
      nodes.push(self.parse_node());
    }
    nodes
  }

  fn parse_node(&mut self) -> dom::Node {
    match self.next() {
      '<' => self.parse_element(),
      _ => self.parse_text(),
    }
  }

  fn parse_element(&mut self) -> dom::Node {

    assert_eq!(self.iterate_char(), '<');
    let tag_name = self.parse_tag();

    let attrs = self.parse_attributes();
    assert_eq!(self.iterate_char(), '>');
    
    let children = self.parse_nodes();
    
    assert_eq!(self.iterate_char(), '<');
    assert_eq!(self.iterate_char(), '/');
    assert_eq!(self.parse_tag(), tag_name);
    assert_eq!(self.iterate_char(), '>');

    dom::new_data(tag_name, attrs, children)
  }

  fn parse_tag(&mut self) -> String {
    self.iterate_while(|c| match c {
      'a'..='z' | 'A'..='Z' | '0'..='9' => true,
      _ => false,
    })
  }

  fn parse_attributes(&mut self) -> dom::Attrs {
    let mut attributes = HashMap::new();
    loop {
      self.iterate_whitespace();
      if self.next() == '>' {
        break;
      }
      let (name, value) = self.parse_attr();
      attributes.insert(name, value);
    }
    attributes
  }

  fn parse_attr(&mut self) -> (String, String) {
    let name = self.parse_tag();
    assert_eq!(self.iterate_char(), '=');
    let value = self.parse_attr_value();
    (name, value)
  }

  fn parse_attr_value(&mut self) -> String {
    let open_quote = self.iterate_char();
    assert!(open_quote == '"' || open_quote == '\'');
    let value = self.iterate_while(|c| c != open_quote);
    assert_eq!(self.iterate_char(), open_quote);
    value
  }
  
  fn parse_text(&mut self) -> dom::Node {
    dom::new_text(self.iterate_while(|c| c != '<'))
  }
  
  fn iterate_whitespace(&mut self) {
    self.iterate_while(char::is_whitespace);
  }
  
  fn iterate_while<F>(&mut self, check: F) -> String
  where
    F: Fn(char) -> bool,
  {
    let mut res = String::new();
    while !self.eof() && check(self.next()) {
      res.push(self.iterate_char());
    }
    res
  }

  fn iterate_char(&mut self) -> char {
    let mut iter = self.html[self.index..].char_indices();
    let (_, current) = iter.next().unwrap();
    let (next, _) = iter.next().unwrap_or((1, ' '));
  
    self.index += next;
    current
  }
  
  fn next(&self) -> char {
    self.html[self.index..].chars().next().unwrap()
  }

  fn starts_with(&self, s: &str) -> bool {
    self.html[self.index..].starts_with(s)
  }
  
  // If eof of html is reached
  fn eof(&self) -> bool {
    self.index >= self.html.len()
  }
}
