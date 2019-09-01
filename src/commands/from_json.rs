use crate::commands::WholeStreamCommand;
use crate::object::{Primitive, TaggedDictBuilder, Value};
use crate::prelude::*;

pub struct FromJSON;

#[derive(Deserialize)]
pub struct FromJSONArgs {
    objects: bool,
}

impl WholeStreamCommand for FromJSON {
    fn name(&self) -> &str {
        "from-json"
    }

    fn signature(&self) -> Signature {
        Signature::build("from-json").switch("objects")
    }

    fn run(
        &self,
        args: CommandArgs,
        registry: &CommandRegistry,
    ) -> Result<OutputStream, ShellError> {
        args.process(registry, from_json)?.run()
    }
}

fn convert_json_value_to_nu_value(v: &serde_hjson::Value, tag: impl Into<Tag>) -> Tagged<Value> {
    let tag = tag.into();

    match v {
        serde_hjson::Value::Null => Value::Primitive(Primitive::Nothing).tagged(tag),
        serde_hjson::Value::Bool(b) => Value::boolean(*b).tagged(tag),
        serde_hjson::Value::F64(n) => Value::number(n).tagged(tag),
        serde_hjson::Value::U64(n) => Value::number(n).tagged(tag),
        serde_hjson::Value::I64(n) => Value::number(n).tagged(tag),
        serde_hjson::Value::String(s) => {
            Value::Primitive(Primitive::String(String::from(s))).tagged(tag)
        }
        serde_hjson::Value::Array(a) => Value::List(
            a.iter()
                .map(|x| convert_json_value_to_nu_value(x, tag))
                .collect(),
        )
        .tagged(tag),
        serde_hjson::Value::Object(o) => {
            let mut collected = TaggedDictBuilder::new(tag);
            for (k, v) in o.iter() {
                collected.insert_tagged(k.clone(), convert_json_value_to_nu_value(v, tag));
            }

            collected.into_tagged_value()
        }
    }
}

pub fn from_json_string_to_value(
    s: String,
    tag: impl Into<Tag>,
) -> serde_hjson::Result<Tagged<Value>> {
    let v: serde_hjson::Value = serde_hjson::from_str(&s)?;
    Ok(convert_json_value_to_nu_value(&v, tag))
}

fn from_json(
    FromJSONArgs { objects }: FromJSONArgs,
    RunnableContext { input, name, .. }: RunnableContext,
) -> Result<OutputStream, ShellError> {
    let name_span = name;

    let stream = async_stream_block! {
        let values: Vec<Tagged<Value>> = input.values.collect().await;

        let mut concat_string = String::new();
        let mut latest_tag: Option<Tag> = None;

        for value in values {
            let value_tag = value.tag();
            latest_tag = Some(value_tag);
            match value.item {
                Value::Primitive(Primitive::String(s)) => {
                    concat_string.push_str(&s);
                    concat_string.push_str("\n");
                }
                _ => yield Err(ShellError::labeled_error_with_secondary(
                    "Expected a string from pipeline",
                    "requires string input",
                    name_span,
                    "value originates from here",
                    value_tag.span,
                )),

            }
        }


        if objects {
            for json_str in concat_string.lines() {
                if json_str.is_empty() {
                    continue;
                }

                match from_json_string_to_value(json_str.to_string(), name_span) {
                    Ok(x) =>
                        yield ReturnSuccess::value(x),
                    Err(_) => {
                        if let Some(last_tag) = latest_tag {
                            yield Err(ShellError::labeled_error_with_secondary(
                                "Could nnot parse as JSON",
                                "input cannot be parsed as JSON",
                                name_span,
                                "value originates from here",
                                last_tag.span))
                        }
                    }
                }
            }
        } else {
            match from_json_string_to_value(concat_string, name_span) {
                Ok(x) =>
                    match x {
                        Tagged { item: Value::List(list), .. } => {
                            for l in list {
                                yield ReturnSuccess::value(l);
                            }
                        }
                        x => yield ReturnSuccess::value(x),
                    }
                Err(_) => {
                    if let Some(last_tag) = latest_tag {
                        yield Err(ShellError::labeled_error_with_secondary(
                            "Could not parse as JSON",
                            "input cannot be parsed as JSON",
                            name_span,
                            "value originates from here",
                            last_tag.span))
                    }
                }
            }
        }
    };

    Ok(stream.to_output_stream())
}
