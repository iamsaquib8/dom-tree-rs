use std::fmt;
use std::collections::{HashMap, HashSet};
use serde::{Serialize};
use serde_json;

pub type Attrs = HashMap<String, String>;

#[derive(Debug, PartialEq, Clone, Serialize)]
pub struct Node {
  pub tag_name: String,
  pub children: Vec<Node>,
  pub props: Props,
}

#[derive(Debug, PartialEq, Clone, Serialize)]
pub enum Props {
  Data(NodeData),
  Text(String),
}

#[derive(Debug, PartialEq, Clone, Serialize)]
pub struct NodeData {
  pub attrs: Attrs,
}

pub fn new_text(data: String) -> Node {
  Node {
    tag_name: String::from(""),
    children: vec![],
    props: Props::Text(data),
  }
}

pub fn new_data(tag_name: String, attrs: Attrs, children: Vec<Node>) -> Node {
  Node {
    tag_name,
    children,
    props: Props::Data(NodeData { attrs }),
  }
}

impl NodeData {
  pub fn id(&self) -> Option<&String> {
    self.attrs.get("id")
  }

  pub fn classes(&self) -> HashSet<&str> {
    match self.attrs.get("class") {
      Some(class_list) => class_list.split(' ').collect(),
      None => HashSet::new(),
    }
  }
}


impl fmt::Display for Node {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
      write!(
          f,
          r#"{:#?}"#,
          self
      )
  }
}

pub fn output(node: Node) -> String {
  serde_json::to_string(&node).unwrap()
}