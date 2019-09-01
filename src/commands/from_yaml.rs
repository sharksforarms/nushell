use crate::commands::WholeStreamCommand;
use crate::object::{Primitive, TaggedDictBuilder, Value};
use crate::prelude::*;

pub struct FromYAML;

impl WholeStreamCommand for FromYAML {
    fn run(
        &self,
        args: CommandArgs,
        registry: &CommandRegistry,
    ) -> Result<OutputStream, ShellError> {
        from_yaml(args, registry)
    }

    fn name(&self) -> &str {
        "from-yaml"
    }

    fn signature(&self) -> Signature {
        Signature::build("from-yaml")
    }
}

pub struct FromYML;

impl WholeStreamCommand for FromYML {
    fn run(
        &self,
        args: CommandArgs,
        registry: &CommandRegistry,
    ) -> Result<OutputStream, ShellError> {
        from_yaml(args, registry)
    }

    fn name(&self) -> &str {
        "from-yml"
    }

    fn signature(&self) -> Signature {
        Signature::build("from-yml")
    }
}

fn convert_yaml_value_to_nu_value(v: &serde_yaml::Value, tag: impl Into<Tag>) -> Tagged<Value> {
    let tag = tag.into();

    match v {
        serde_yaml::Value::Bool(b) => Value::boolean(*b).tagged(tag),
        serde_yaml::Value::Number(n) if n.is_i64() => {
            Value::number(n.as_i64().unwrap()).tagged(tag)
        }
        serde_yaml::Value::Number(n) if n.is_f64() => {
            Value::Primitive(Primitive::from(n.as_f64().unwrap())).tagged(tag)
        }
        serde_yaml::Value::String(s) => Value::string(s).tagged(tag),
        serde_yaml::Value::Sequence(a) => Value::List(
            a.iter()
                .map(|x| convert_yaml_value_to_nu_value(x, tag))
                .collect(),
        )
        .tagged(tag),
        serde_yaml::Value::Mapping(t) => {
            let mut collected = TaggedDictBuilder::new(tag);

            for (k, v) in t.iter() {
                match k {
                    serde_yaml::Value::String(k) => {
                        collected.insert_tagged(k.clone(), convert_yaml_value_to_nu_value(v, tag));
                    }
                    _ => unimplemented!("Unknown key type"),
                }
            }

            collected.into_tagged_value()
        }
        serde_yaml::Value::Null => Value::Primitive(Primitive::Nothing).tagged(tag),
        x => unimplemented!("Unsupported yaml case: {:?}", x),
    }
}

pub fn from_yaml_string_to_value(
    s: String,
    tag: impl Into<Tag>,
) -> serde_yaml::Result<Tagged<Value>> {
    let v: serde_yaml::Value = serde_yaml::from_str(&s)?;
    Ok(convert_yaml_value_to_nu_value(&v, tag))
}

fn from_yaml(args: CommandArgs, registry: &CommandRegistry) -> Result<OutputStream, ShellError> {
    let args = args.evaluate_once(registry)?;
    let span = args.name_span();
    let input = args.input;

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
                    span,
                    "value originates from here",
                    value_tag.span,
                )),

            }
        }

        match from_yaml_string_to_value(concat_string, span) {
            Ok(x) => match x {
                Tagged { item: Value::List(list), .. } => {
                    for l in list {
                        yield ReturnSuccess::value(l);
                    }
                }
                x => yield ReturnSuccess::value(x),
            },
            Err(_) => if let Some(last_tag) = latest_tag {
                yield Err(ShellError::labeled_error_with_secondary(
                    "Could not parse as YAML",
                    "input cannot be parsed as YAML",
                    span,
                    "value originates from here",
                    last_tag.span,
                ))
            } ,
        }
    };

    Ok(stream.to_output_stream())
}
