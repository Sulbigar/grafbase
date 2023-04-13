use dynaql_value::{ConstValue, Name};
use indexmap::IndexMap;

use crate::registry::scalars::{DynamicScalar, PossibleScalar};
use crate::registry::{MetaEnumValue, MetaInputValue, MetaType, MetaTypeName, Registry};

use crate::{Context, Error, ServerResult};

pub fn resolve_input(
    ctx_field: &Context<'_>,
    meta_input_value: &MetaInputValue,
    value: ConstValue,
) -> ServerResult<ConstValue> {
    // We do keep serde_json::Value::Null here contrary to resolver_input_inner
    // as it allows casting to either T or Option<T> later.
    resolve_input_inner(
        &ctx_field.schema_env.registry,
        &mut Vec::new(),
        &meta_input_value.into(),
        value,
    )
    .map_err(|err| err.into_server_error(ctx_field.item.pos))
}

struct InputContext<'a> {
    /// Expected GraphQL type
    ty: &'a str,
    /// Whether we allow list coercion at this point:
    /// https://spec.graphql.org/October2021/#sec-List.Input-Coercion
    /// Most of time this will be true expect for:
    /// ty: [[Int]]  value: [1, 2, 3] => Error: Incorrect item value
    allow_list_coercion: bool,
    default_value: Option<&'a ConstValue>,
}

impl<'a> From<&'a MetaInputValue> for InputContext<'a> {
    fn from(input: &'a MetaInputValue) -> Self {
        InputContext {
            ty: &input.ty,
            allow_list_coercion: true,
            default_value: input.default_value.as_ref(),
        }
    }
}

fn resolve_input_inner(
    registry: &Registry,
    path: &mut Vec<String>,
    ctx: &InputContext<'_>,
    mut value: ConstValue,
) -> Result<ConstValue, Error> {
    if value == ConstValue::Null {
        value = match ctx.default_value {
            Some(v) => v.clone(),
            None => match MetaTypeName::create(&ctx.ty) {
                MetaTypeName::NonNull(_) => return Err(input_error("Unexpected null value", path)),
                _ => return Ok(ConstValue::Null),
            },
        }
    }

    match MetaTypeName::create(&ctx.ty) {
        MetaTypeName::List(type_name) => {
            if let ConstValue::List(list) = value {
                let input_context = InputContext {
                    ty: &type_name,
                    allow_list_coercion: list.len() <= 1,
                    default_value: None,
                };
                let mut arr = Vec::new();
                for (idx, element) in list.into_iter().enumerate() {
                    path.push(idx.to_string());
                    arr.push(resolve_input_inner(
                        registry,
                        path,
                        &input_context,
                        element,
                    )?);
                    path.pop();
                }
                Ok(ConstValue::List(arr))
            } else if ctx.allow_list_coercion {
                Ok(ConstValue::List(vec![resolve_input_inner(
                    registry,
                    path,
                    &InputContext {
                        ty: &type_name,
                        allow_list_coercion: true,
                        default_value: None,
                    },
                    value,
                )?]))
            } else {
                Err(input_error("Expected a List", path))
            }
        }
        MetaTypeName::NonNull(type_name) => resolve_input_inner(
            registry,
            path,
            &InputContext {
                ty: type_name,
                allow_list_coercion: true,
                default_value: None,
            },
            value,
        ),
        MetaTypeName::Named(type_name) => {
            match registry
                .types
                .get(type_name)
                .expect("Registry has already been validated")
            {
                MetaType::InputObject {
                    input_fields,
                    oneof,
                    ..
                } => {
                    if let ConstValue::Object(mut fields) = value {
                        let mut map = IndexMap::with_capacity(input_fields.len());
                        for (name, meta_input_value) in input_fields {
                            path.push(name.clone());
                            let field_value = resolve_input_inner(
                                registry,
                                path,
                                &meta_input_value.into(),
                                fields.remove(&Name::new(name)).unwrap_or(ConstValue::Null),
                            )?;
                            path.pop();
                            // Not adding NULLs for now makes it easier to work with later.
                            // TODO: Keep NULL, they might be relevant in the future. Currently
                            // it's just not ideal with how we manipulate @oneof inputs
                            if field_value != ConstValue::Null {
                                map.insert(
                                    Name::new(meta_input_value.rename.as_ref().unwrap_or(name)),
                                    field_value,
                                );
                            }
                        }
                        if *oneof && map.len() != 1 {
                            Err(input_error(
                                &format!("Expected exactly one fields (@oneof), got {}", map.len()),
                                path,
                            ))
                        } else {
                            Ok(ConstValue::Object(map))
                        }
                    } else {
                        Err(input_error("Expected an Object", path))
                    }
                }
                MetaType::Enum { enum_values, .. } => resolve_input_enum(value, enum_values, path),
                // TODO: this conversion ConstValue -> serde_json -> ConstValue is sad...
                // we need an intermediate representation between the database & dynaql
                MetaType::Scalar { .. } => Ok(ConstValue::from_json(
                    PossibleScalar::parse(type_name, value)
                        .map_err(|err| Error::new(err.message()))?,
                )?),
                _ => Err(input_error(
                    &format!("Internal Error: Unsupported input type {type_name}"),
                    path,
                )),
            }
        }
    }
}

fn resolve_input_enum(
    value: ConstValue,
    values: &IndexMap<String, MetaEnumValue>,
    path: &[String],
) -> Result<ConstValue, Error> {
    let str_value = match &value {
        ConstValue::Enum(name) => name.as_str(),
        ConstValue::String(string) => string.as_str(),
        _ => {
            return Err(input_error(
                &format!("Expected an enum, not a {}", value.kind_str()),
                path,
            ))
        }
    };
    let meta_value = values
        .get(str_value)
        .ok_or_else(|| input_error("Unknown enum value: {name}", path))?;

    if let Some(value) = &meta_value.value {
        return Ok(ConstValue::String(value.clone()));
    }

    Ok(value)
}

fn input_error(expected: &str, path: &[String]) -> Error {
    Error::new(format!("{expected} for {}", path.join(".")))
}
