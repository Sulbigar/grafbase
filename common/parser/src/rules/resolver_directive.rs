use super::{
    directive::Directive,
    visitor::{Visitor, VisitorContext},
};
use dynaql::Positioned;
use dynaql_parser::types::{FieldDefinition, TypeDefinition};
use dynaql_value::ConstValue;

pub const RESOLVER_DIRECTIVE: &str = "resolver";
pub const NAME_ARGUMENT: &str = "name";

pub struct ResolverDirective;

impl ResolverDirective {
    #[allow(dead_code)]
    pub fn resolver_name(field: &FieldDefinition) -> Option<&str> {
        field
            .directives
            .iter()
            .find(|directive| directive.node.name.node == RESOLVER_DIRECTIVE)
            .and_then(|directive| directive.node.get_argument(NAME_ARGUMENT))
            .and_then(|value| match &value.node {
                ConstValue::String(resolver_name) => Some(resolver_name.as_str()),
                _ => None,
            })
    }
}

impl<'a> Visitor<'a> for ResolverDirective {
    fn enter_field(
        &mut self,
        ctx: &mut VisitorContext<'a>,
        field: &'a Positioned<FieldDefinition>,
        _parent_type: &'a Positioned<TypeDefinition>,
    ) {
        if let Some(directive) = field
            .node
            .directives
            .iter()
            .find(|d| d.node.name.node == RESOLVER_DIRECTIVE)
        {
            if let Ok(mut arguments) = super::directive::extract_arguments(ctx, directive, &[&[NAME_ARGUMENT]], None) {
                if let ConstValue::String(_resolver_name) = arguments.remove(NAME_ARGUMENT).unwrap() {
                    // OK.
                } else {
                    ctx.report_error(
                        vec![directive.pos],
                        "The @{RESOLVER_DIRECTIVE} directive expects the `{name}` argument to be a string".to_string(),
                    );
                }
            }
        }
    }
}

impl Directive for ResolverDirective {
    fn definition() -> String {
        r#"
        directive @resolver(name: String) on FIELD_DEFINITION
        "#
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::visitor::visit;
    use dynaql_parser::parse_schema;
    use pretty_assertions::assert_eq;

    #[rstest::rstest]
    #[case(r#"
        type Customer @model {
            id: ID!
            balance: Int! @resolver
        }
    "#, &[
        "The @resolver directive takes a single `name` argument"
    ])]
    #[case(r#"
        type Customer @model {
            id: ID!
            balance: Int! @resolver(path: "resolvers/balance")
        }
    "#, &[
        "The @resolver directive takes a single `name` argument"
    ])]
    #[case(r#"
        type Customer @model {
            id: ID!
            balance: Int! @resolver(name: "resolvers/balance")
        }
    "#, &[])]
    fn test_parse_result(#[case] schema: &str, #[case] expected_messages: &[&str]) {
        let schema = parse_schema(schema).unwrap();
        let mut ctx = VisitorContext::new(&schema);
        visit(&mut ResolverDirective, &mut ctx, &schema);

        let actual_messages: Vec<_> = ctx.errors.iter().map(|error| error.message.as_str()).collect();
        assert_eq!(actual_messages.as_slice(), expected_messages);
    }
}
