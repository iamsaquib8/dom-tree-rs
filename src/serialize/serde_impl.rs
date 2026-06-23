//! Optional `serde::Serialize` impls.
//!
//! These produce byte-for-byte the same JSON shape as the zero-dependency
//! [`Dom::to_json`](crate::Dom::to_json), so the two paths agree (pinned by a
//! roundtrip test). Only `Serialize` is provided — the tree is a parse *output*,
//! not something you deserialize into.

use serde::ser::{Serialize, SerializeMap, SerializeSeq, SerializeStruct, Serializer};

use crate::node::{Element, NodeKind};
use crate::tree::{Dom, NodeRef};

impl Serialize for Dom {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut st = s.serialize_struct("document", 3)?;
        st.serialize_field("type", "document")?;
        st.serialize_field("errors", &self.errors().len())?;
        st.serialize_field("children", &Forest(self))?;
        st.end()
    }
}

struct Forest<'a>(&'a Dom);

impl Serialize for Forest<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut seq = s.serialize_seq(None)?;
        for r in self.0.roots() {
            seq.serialize_element(&r)?;
        }
        seq.end()
    }
}

impl<'a> Serialize for NodeRef<'a> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self.kind() {
            NodeKind::Element(e) => {
                let mut st = s.serialize_struct("element", 4)?;
                st.serialize_field("type", "element")?;
                st.serialize_field("tag", e.name())?;
                st.serialize_field("attrs", &AttrMap(e))?;
                st.serialize_field("children", &Children(*self))?;
                st.end()
            }
            NodeKind::Text(t) => two_field(s, "text", t),
            NodeKind::Comment(c) => two_field(s, "comment", c),
            NodeKind::Doctype(d) => two_field(s, "doctype", d),
        }
    }
}

fn two_field<S: Serializer>(s: S, ty: &'static str, value: &str) -> Result<S::Ok, S::Error> {
    let mut st = s.serialize_struct(ty, 2)?;
    st.serialize_field("type", ty)?;
    st.serialize_field("value", value)?;
    st.end()
}

struct AttrMap<'a>(&'a Element);

impl Serialize for AttrMap<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut m = s.serialize_map(None)?;
        for (k, v) in self.0.attributes() {
            m.serialize_entry(k, v)?;
        }
        m.end()
    }
}

struct Children<'a>(NodeRef<'a>);

impl Serialize for Children<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut seq = s.serialize_seq(None)?;
        for c in self.0.children() {
            seq.serialize_element(&c)?;
        }
        seq.end()
    }
}
