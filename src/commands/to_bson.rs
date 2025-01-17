use crate::commands::WholeStreamCommand;
use crate::object::{Dictionary, Primitive, Value};
use crate::prelude::*;
use bson::{encode_document, oid::ObjectId, spec::BinarySubtype, Bson, Document};
use std::convert::TryInto;

pub struct ToBSON;

impl WholeStreamCommand for ToBSON {
    fn name(&self) -> &str {
        "to-bson"
    }

    fn signature(&self) -> Signature {
        Signature::build("to-bson")
    }

    fn usage(&self) -> &str {
        "Convert table into .bson text."
    }

    fn run(
        &self,
        args: CommandArgs,
        registry: &CommandRegistry,
    ) -> Result<OutputStream, ShellError> {
        to_bson(args, registry)
    }
}

pub fn value_to_bson_value(v: &Value) -> Bson {
    match v {
        Value::Primitive(Primitive::Boolean(b)) => Bson::Boolean(*b),
        // FIXME: What about really big decimals?
        Value::Primitive(Primitive::Bytes(decimal)) => Bson::FloatingPoint(
            (*decimal)
                .to_f64()
                .expect("Unimplemented BUG: What about big decimals?"),
        ),
        Value::Primitive(Primitive::Date(d)) => Bson::UtcDatetime(*d),
        Value::Primitive(Primitive::EndOfStream) => Bson::Null,
        Value::Primitive(Primitive::BeginningOfStream) => Bson::Null,
        Value::Primitive(Primitive::Decimal(d)) => Bson::FloatingPoint(d.to_f64().unwrap()),
        Value::Primitive(Primitive::Int(i)) => Bson::I64(*i),
        Value::Primitive(Primitive::Nothing) => Bson::Null,
        Value::Primitive(Primitive::String(s)) => Bson::String(s.clone()),
        Value::Primitive(Primitive::Path(s)) => Bson::String(s.display().to_string()),
        Value::List(l) => Bson::Array(l.iter().map(|x| value_to_bson_value(x)).collect()),
        Value::Block(_) => Bson::Null,
        Value::Binary(b) => Bson::Binary(BinarySubtype::Generic, b.clone()),
        Value::Object(o) => object_value_to_bson(o),
    }
}

// object_value_to_bson handles all Objects, even those that correspond to special
// types (things like regex or javascript code).
fn object_value_to_bson(o: &Dictionary) -> Bson {
    let mut it = o.entries.iter();
    if it.len() > 2 {
        return generic_object_value_to_bson(o);
    }
    match it.next() {
        Some((regex, tagged_regex_value)) if regex == "$regex" => match it.next() {
            Some((options, tagged_opts_value)) if options == "$options" => {
                let r: Result<String, _> = tagged_regex_value.try_into();
                let opts: Result<String, _> = tagged_opts_value.try_into();
                if r.is_err() || opts.is_err() {
                    generic_object_value_to_bson(o)
                } else {
                    Bson::RegExp(r.unwrap(), opts.unwrap())
                }
            }
            _ => generic_object_value_to_bson(o),
        },
        Some((javascript, tagged_javascript_value)) if javascript == "$javascript" => {
            match it.next() {
                Some((scope, tagged_scope_value)) if scope == "$scope" => {
                    let js: Result<String, _> = tagged_javascript_value.try_into();
                    let s: Result<&Dictionary, _> = tagged_scope_value.try_into();
                    if js.is_err() || s.is_err() {
                        generic_object_value_to_bson(o)
                    } else {
                        if let Bson::Document(doc) = object_value_to_bson(s.unwrap()) {
                            Bson::JavaScriptCodeWithScope(js.unwrap(), doc)
                        } else {
                            generic_object_value_to_bson(o)
                        }
                    }
                }
                None => {
                    let js: Result<String, _> = tagged_javascript_value.try_into();
                    if js.is_err() {
                        generic_object_value_to_bson(o)
                    } else {
                        Bson::JavaScriptCode(js.unwrap())
                    }
                }
                _ => generic_object_value_to_bson(o),
            }
        }
        Some((timestamp, tagged_timestamp_value)) if timestamp == "$timestamp" => {
            let ts: Result<i64, _> = tagged_timestamp_value.try_into();
            if ts.is_err() {
                generic_object_value_to_bson(o)
            } else {
                Bson::TimeStamp(ts.unwrap())
            }
        }
        Some((binary_subtype, tagged_binary_subtype_value))
            if binary_subtype == "$binary_subtype" =>
        {
            match it.next() {
                Some((binary, tagged_bin_value)) if binary == "$binary" => {
                    let bst = get_binary_subtype(tagged_binary_subtype_value);
                    let bin: Result<Vec<u8>, _> = tagged_bin_value.try_into();
                    if bst.is_none() || bin.is_err() {
                        generic_object_value_to_bson(o)
                    } else {
                        Bson::Binary(bst.unwrap(), bin.unwrap())
                    }
                }
                _ => generic_object_value_to_bson(o),
            }
        }
        Some((object_id, tagged_object_id_value)) if object_id == "$object_id" => {
            let obj_id: Result<String, _> = tagged_object_id_value.try_into();
            if obj_id.is_err() {
                generic_object_value_to_bson(o)
            } else {
                let obj_id = ObjectId::with_string(&obj_id.unwrap());
                if obj_id.is_err() {
                    generic_object_value_to_bson(o)
                } else {
                    Bson::ObjectId(obj_id.unwrap())
                }
            }
        }
        Some((symbol, tagged_symbol_value)) if symbol == "$symbol" => {
            let sym: Result<String, _> = tagged_symbol_value.try_into();
            if sym.is_err() {
                generic_object_value_to_bson(o)
            } else {
                Bson::Symbol(sym.unwrap())
            }
        }
        _ => generic_object_value_to_bson(o),
    }
}

fn get_binary_subtype<'a>(tagged_value: &'a Tagged<Value>) -> Option<BinarySubtype> {
    match tagged_value.item() {
        Value::Primitive(Primitive::String(s)) => Some(match s.as_ref() {
            "generic" => BinarySubtype::Generic,
            "function" => BinarySubtype::Function,
            "binary_old" => BinarySubtype::BinaryOld,
            "uuid_old" => BinarySubtype::UuidOld,
            "uuid" => BinarySubtype::Uuid,
            "md5" => BinarySubtype::Md5,
            _ => unreachable!(),
        }),
        Value::Primitive(Primitive::Int(i)) => Some(BinarySubtype::UserDefined(*i as u8)),
        _ => None,
    }
}

// generic_object_value_bson handles any Object that does not
// correspond to a special bson type (things like regex or javascript code).
fn generic_object_value_to_bson(o: &Dictionary) -> Bson {
    let mut doc = Document::new();
    for (k, v) in o.entries.iter() {
        doc.insert(k.clone(), value_to_bson_value(v));
    }
    Bson::Document(doc)
}

fn shell_encode_document(
    writer: &mut Vec<u8>,
    doc: Document,
    span: Span,
) -> Result<(), ShellError> {
    match encode_document(writer, &doc) {
        Err(e) => Err(ShellError::labeled_error(
            format!("Failed to encode document due to: {:?}", e),
            "requires BSON-compatible document",
            span,
        )),
        _ => Ok(()),
    }
}

fn bson_value_to_bytes(bson: Bson, span: Span) -> Result<Vec<u8>, ShellError> {
    let mut out = Vec::new();
    match bson {
        Bson::Array(a) => {
            for v in a.into_iter() {
                match v {
                    Bson::Document(d) => shell_encode_document(&mut out, d, span)?,
                    _ => {
                        return Err(ShellError::labeled_error(
                            format!("All top level values must be Documents, got {:?}", v),
                            "requires BSON-compatible document",
                            span,
                        ))
                    }
                }
            }
        }
        Bson::Document(d) => shell_encode_document(&mut out, d, span)?,
        _ => {
            return Err(ShellError::labeled_error(
                format!("All top level values must be Documents, got {:?}", bson),
                "requires BSON-compatible document",
                span,
            ))
        }
    }
    Ok(out)
}

fn to_bson(args: CommandArgs, registry: &CommandRegistry) -> Result<OutputStream, ShellError> {
    let args = args.evaluate_once(registry)?;
    let name_span = args.name_span();
    let out = args.input;

    Ok(out
        .values
        .map(
            move |a| match bson_value_to_bytes(value_to_bson_value(&a), name_span) {
                Ok(x) => ReturnSuccess::value(Value::Binary(x).simple_spanned(name_span)),
                _ => Err(ShellError::labeled_error_with_secondary(
                    "Expected an object with BSON-compatible structure from pipeline",
                    "requires BSON-compatible input: Must be Array or Object",
                    name_span,
                    format!("{} originates from here", a.item.type_name()),
                    a.span(),
                )),
            },
        )
        .to_output_stream())
}
