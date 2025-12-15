use std::sync::Arc;

use crate::{new_values::expr::ValueExpr, values::ValueRef, workflow::PathPart};
use serde::Deserialize as _;
use serde_json::Value;
use trie_rs::{
    inc_search::{Answer, IncSearch},
    map::{Trie, TrieBuilder},
    try_collect::TryFromIterator,
};

#[derive(Debug)]
struct ValueTemplateRepr {
    // DO NOT SUBMIT: Should this be a value ref so the trivial case can just clone it?
    template: Value,
    exprs: Vec<ValueExpr>,
    // Technically, the PathPart includes strings that are duplicates of what is in template.
    // We could optimize this by using a `&` and some unsafe tricks, but for now we're not
    // worried about the overhead of this datatype.
    expr_paths: Trie<PathPart, usize>,
}

#[derive(Clone, Debug, PartialOrd, Ord, PartialEq, Eq, Hash)]
pub enum PathPartRef<'a> {
    Field(&'a str),
    Index(usize),
}

impl PathPartRef<'_> {
    pub fn as_owned(&self) -> PathPart {
        match self {
            PathPartRef::Field(s) => PathPart::IndexStr(s.to_string()),
            PathPartRef::Index(i) => PathPart::Index(*i),
        }
    }
}

struct PathCollect;
impl<'a> TryFromIterator<PathPartRef<'a>, PathCollect> for String {
    type Error = ();

    fn try_from_iter<I: IntoIterator<Item = PathPartRef<'a>>>(
        iter: I,
    ) -> Result<Self, Self::Error> {
        let mut path = "$".to_string();
        for part in iter {
            let part_str = match part {
                PathPartRef::Field(s) => format!("[{s:?}]"),
                PathPartRef::Index(i) => format!("[{i}]"),
            };
            path.push_str(&part_str);
        }
        Ok(path)
    }
}
impl TryFromIterator<PathPart, PathCollect> for String {
    type Error = ();

    fn try_from_iter<I: IntoIterator<Item = PathPart>>(iter: I) -> Result<Self, Self::Error> {
        let mut path = "$".to_string();
        for part in iter {
            let part_str = match part {
                PathPart::Field(s) => format!(".{s}"),
                PathPart::Index(i) => format!("[{i}]"),
                PathPart::IndexStr(s) => format!("[{s:?}]"),
            };
            path.push_str(&part_str);
        }
        Ok(path)
    }
}
impl TryFromIterator<PathPart, PathCollect> for () {
    type Error = ();

    fn try_from_iter<T>(_iter: T) -> Result<Self, Self::Error>
    where
        Self: Sized,
        T: IntoIterator<Item = PathPart>,
    {
        Ok(())
    }
}

#[derive(Debug)]
#[repr(transparent)]
pub struct ValueTemplate(Arc<ValueTemplateRepr>);

impl ValueTemplate {
    pub fn template(&self) -> &Value {
        &self.0.template
    }

    pub fn exprs(&self) -> &[ValueExpr] {
        &self.0.exprs
    }

    /// Apply substitutions to the template to produce a concrete JSON value.
    // DO NOT SUBMIT: Return ValueRef?
    pub fn substitute(&self, values: Vec<ValueRef>) -> serde_json::Value {
        // DO NOT SUBMIT: Treat this as an error?
        assert_eq!(values.len(), self.0.exprs.len());

        if self.0.exprs.is_empty() {
            return self.0.template.clone();
        }

        fn recurse<'a>(
            tpl: &serde_json::Value,
            inc: IncSearch<'a, PathPart, usize>,
            values: &[ValueRef],
        ) -> serde_json::Value {
            match tpl {
                Value::Array(tpl_values) => {
                    let mut new_items = Vec::with_capacity(tpl_values.len());
                    for (index, tpl_value) in tpl_values.iter().enumerate() {
                        let mut next_inc = inc.clone();
                        let new_item = match next_inc.query(&PathPart::Index(index)) {
                            None => {
                                // No match in the trie. We can just use the whole value.
                                tpl_value.clone()
                            }
                            Some(Answer::Match) => {
                                // This is a match, which means we have an index in the trie.
                                values[*next_inc.value().expect("at a match")].clone_value()
                            }
                            Some(Answer::Prefix) => {
                                // There is a prefix *below* this field, so we need to recurse into it.
                                recurse(tpl_value, next_inc, values)
                            }
                            Some(Answer::PrefixAndMatch) => {
                                panic!(
                                    "Should not be possible to have both a prefix and a match at the same time"
                                )
                            }
                        };
                        new_items.push(new_item)
                    }
                    Value::Array(new_items)
                }
                Value::Object(tpl_fields) => {
                    let mut new_fields = serde_json::Map::with_capacity(tpl_fields.len());
                    for (field, tpl_value) in tpl_fields.iter() {
                        let mut next_inc = inc.clone();
                        let new_value = match next_inc.query(&PathPart::IndexStr(field.clone())) {
                            None => {
                                // No match in the trie. We can just use the whole value.
                                tpl_value.clone()
                            }
                            Some(Answer::Match) => {
                                // This is a match, which means we have an index in the trie.
                                values[*next_inc.value().expect("at a match")].clone_value()
                            }
                            Some(Answer::Prefix) => {
                                // There is a prefix *below* this field, so we need to recurse into it.
                                recurse(tpl_value, next_inc, values)
                            }
                            Some(Answer::PrefixAndMatch) => {
                                panic!(
                                    "Should not be possible to have both a prefix and a match at the same time"
                                )
                            }
                        };
                        new_fields.insert(field.clone(), new_value);
                    }
                    Value::Object(new_fields)
                }
                v => v.clone(),
            }
        }

        recurse(&self.0.template, self.0.expr_paths.inc_search(), &values)
    }
}

fn parse_value_template(template: Value) -> Result<ValueTemplateRepr, ParseError> {
    fn recurse<'a>(
        v: &'a Value,
        path: &mut Vec<PathPartRef<'a>>,
        exprs: &mut Vec<ValueExpr>,
        expr_paths: &mut TrieBuilder<PathPart, usize>,
    ) -> Result<(), ParseError> {
        match v {
            Value::Object(o) if o.contains_key("$literal") => {
                // 1. There should be no *other* keys.
                todo!("Handle literals")
            }
            Value::Object(o) if o.contains_key("$from") => {
                match ValueExpr::deserialize(v.clone()) {
                    Ok(expr) => {
                        let expr_index = exprs.len();
                        exprs.push(expr);
                        expr_paths.insert(path.iter().map(PathPartRef::as_owned), expr_index);
                    }
                    Err(error) => {
                        return Err(ParseError {
                            path: path.iter().map(PathPartRef::as_owned).collect(),
                            error,
                        });
                    }
                }
            }
            Value::Object(map) => {
                for (key, value) in map.iter() {
                    path.push(PathPartRef::Field(key));
                    recurse(value, path, exprs, expr_paths)?;
                    path.pop();
                }
            }
            Value::Array(arr) => {
                for (index, item) in arr.iter().enumerate() {
                    path.push(PathPartRef::Index(index));
                    recurse(item, path, exprs, expr_paths)?;
                    path.pop();
                }
            }
            _ => {}
        };

        Ok(())
    }

    let mut path = Vec::new();
    let mut exprs = Vec::new();
    let mut expr_paths = TrieBuilder::new();
    recurse(&template, &mut path, &mut exprs, &mut expr_paths)?;

    let expr_paths = expr_paths.build();
    Ok(ValueTemplateRepr {
        template,
        exprs,
        expr_paths,
    })
}

#[derive(Debug)]
pub struct ParseError {
    path: Vec<PathPart>,
    error: serde_json::Error,
}

impl TryFrom<Value> for ValueTemplateRepr {
    type Error = ParseError;
    fn try_from(value: Value) -> Result<Self, Self::Error> {
        parse_value_template(value)
    }
}

impl TryFrom<Value> for ValueTemplate {
    type Error = ParseError;
    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let value_template = ValueTemplateRepr::try_from(value)?;
        Ok(ValueTemplate(Arc::new(value_template)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_literal_string() {
        let template = ValueTemplate::try_from(json!("hello")).unwrap();
        assert_eq!(template.exprs(), &[]);
        assert_eq!(template.substitute(vec![]), json!("hello"));
    }

    #[test]
    fn test_literal_number() {
        let template = ValueTemplate::try_from(json!(5)).unwrap();
        assert_eq!(template.exprs(), &[]);
        assert_eq!(template.substitute(vec![]), json!(5));
    }

    #[test]
    fn test_literal_object() {
        let template = ValueTemplate::try_from(json!({ "a": 5, "b": "hi" })).unwrap();
        assert_eq!(template.exprs(), &[]);
        assert_eq!(template.substitute(vec![]), json!({ "a": 5, "b": "hi" }));
    }

    // Ignored: This test is for the old trie-based implementation that will be removed in Phase 8
    #[test]
    #[ignore]
    fn test_object_with_step_references() {
        let template = ValueTemplate::try_from(json!({
            "a": { "$from": { "step": "foo" }},
            "b": { "$from": { "step": "bar" }},
        }))
        .unwrap();

        assert_eq!(
            template.exprs(),
            &[ValueExpr::step_output("foo"), ValueExpr::step_output("bar")]
        );
        assert_eq!(
            template.substitute(vec![json!("A").into(), json!("B").into()]),
            json!({
                "a": "A",
                "b": "B",
            })
        );
    }

    // Ignored: This test is for the old trie-based implementation that will be removed in Phase 8
    #[test]
    #[ignore]
    fn test_array_with_step_references() {
        let template = ValueTemplate::try_from(json!([
            { "$from": { "step": "foo" }},
            { "$from": { "step": "bar" }},
        ]))
        .unwrap();

        assert_eq!(
            template.exprs(),
            &[ValueExpr::step_output("foo"), ValueExpr::step_output("bar")]
        );
        assert_eq!(
            template.substitute(vec![json!("A").into(), json!("B").into()]),
            json!(["A", "B"])
        );
    }

    // Ignored: This test is for the old trie-based implementation that will be removed in Phase 8
    #[test]
    #[ignore]
    fn test_parse_errors() {
        let error = ValueTemplate::try_from(json!([
            { "$from": { "stp": "foo" }},
            { "$from": { "tep": "bar" }},
        ]))
        .unwrap_err();

        // TODO: We should probably improve the parser errors when it doesn't match
        // the schema, but this demonstrates that we report *all* malformed expressions.
        assert_eq!(error.path, vec![PathPart::Index(0)]);
        assert_eq!(
            error.error.to_string(),
            "data did not match any variant of untagged enum BaseRef"
        );
    }
}
