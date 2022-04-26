use super::{ResolverContext, ResolverTrait};
use crate::registry::utils::value_to_attribute;
use crate::registry::variables::VariableResolveDefinition;
use crate::{Context, Error, Value};
use chrono::Utc;
use dynamodb::{DynamoDBBatchersData, DynamoDBContext, TxItem};
use dynomite::dynamodb::{Delete, Put, TransactWriteItem};
use dynomite::AttributeValue;
use std::collections::HashMap;
use std::hash::Hash;

#[non_exhaustive]
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, Hash)]
pub enum DynamoMutationResolver {
    /// Create a new Node based on an input.
    CreateNode {
        input: VariableResolveDefinition,
        /// Type defined for Database side
        ty: String,
    },
    /// Delete a Node based on the inputed id.
    DeleteNode { id: VariableResolveDefinition },
}

#[async_trait::async_trait]
impl ResolverTrait for DynamoMutationResolver {
    async fn resolve(
        &self,
        ctx: &Context<'_>,
        resolver_ctx: &ResolverContext<'_>,
    ) -> Result<serde_json::Value, Error> {
        let batchers = &ctx.data::<DynamoDBBatchersData>()?.transaction;
        let dynamodb_ctx = ctx.data::<DynamoDBContext>()?;
        let a = ctx.resolver_node.unwrap().ty.unwrap();
        #[cfg(feature = "tracing_worker")]
        logworker::info!("dynamodb-resolver", "BL {:?}", &a);
        match self {
            DynamoMutationResolver::CreateNode { input, ty } => {
                let autogenerated_id = format!("{}#{}", ty, resolver_ctx.execution_id,);

                if let Some(resolver_id) = resolver_ctx.resolver_id {
                    ctx.resolvers_data
                        .write()
                        .map_err(|_| Error::new("Internal Server Error"))?
                        .insert(
                            format!("{}_id", resolver_id),
                            Value::String(autogenerated_id.clone()),
                        );
                }

                let input = match input.param(ctx).expect("can't fail") {
                    Value::Object(inner) => inner,
                    _ => {
                        return Err(Error::new("Internal Error: failed to infer key"));
                    }
                };

                let mut item = input.iter().fold(HashMap::new(), |mut acc, (key, val)| {
                    acc.insert(
                        key.to_string(),
                        value_to_attribute(val.clone().into_json().expect("can't fail")),
                    );
                    acc
                });

                item.insert(
                    "__pk".to_string(),
                    AttributeValue {
                        s: Some(autogenerated_id.clone()),
                        ..Default::default()
                    },
                );
                item.insert(
                    "__sk".to_string(),
                    AttributeValue {
                        s: Some(autogenerated_id.clone()),
                        ..Default::default()
                    },
                );
                item.insert(
                    "created_at".to_string(),
                    AttributeValue {
                        s: Some(Utc::now().to_string()),
                        ..Default::default()
                    },
                );
                item.insert(
                    "updated_at".to_string(),
                    AttributeValue {
                        s: Some(Utc::now().to_string()),
                        ..Default::default()
                    },
                );

                item.insert(
                    "__gsi1pk".to_string(),
                    AttributeValue {
                        s: Some(ty.clone()),
                        ..Default::default()
                    },
                );

                item.insert(
                    "__gsi1sk".to_string(),
                    AttributeValue {
                        s: Some(autogenerated_id.clone()),
                        ..Default::default()
                    },
                );

                item.insert(
                    "__gsi2pk".to_string(),
                    AttributeValue {
                        s: Some(autogenerated_id.clone()),
                        ..Default::default()
                    },
                );

                item.insert(
                    "__gsi2sk".to_string(),
                    AttributeValue {
                        s: Some(autogenerated_id.clone()),
                        ..Default::default()
                    },
                );

                let t = TxItem {
                    pk: autogenerated_id.clone(),
                    sk: autogenerated_id.clone(),
                    transaction: TransactWriteItem {
                        put: Some(Put {
                            table_name: dynamodb_ctx.dynamodb_table_name.clone(),
                            item,
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                };

                batchers.load_one(t).await?;
                Ok(serde_json::Value::Null)
            }
            DynamoMutationResolver::DeleteNode { id } => {
                let id_to_be_deleted = match id.param(ctx).expect("can't fail") {
                    Value::String(inner) => inner,
                    _ => {
                        return Err(Error::new("Internal Error: failed to infer key"));
                    }
                };

                let mut item = HashMap::new();
                item.insert(
                    "__pk".to_string(),
                    AttributeValue {
                        s: Some(id_to_be_deleted.clone()),
                        ..Default::default()
                    },
                );
                item.insert(
                    "__sk".to_string(),
                    AttributeValue {
                        s: Some(id_to_be_deleted.clone()),
                        ..Default::default()
                    },
                );

                let t = TxItem {
                    pk: id_to_be_deleted.clone(),
                    sk: id_to_be_deleted.clone(),
                    transaction: TransactWriteItem {
                        delete: Some(Delete {
                            expression_attribute_names: Some({
                                HashMap::from([("#pk".to_string(), "__pk".to_string())])
                            }),
                            condition_expression: Some("attribute_exists(#pk)".to_string()),
                            table_name: dynamodb_ctx.dynamodb_table_name.clone(),
                            key: item,
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                };

                batchers.load_one(t).await?;

                if let Some(resolver_id) = resolver_ctx.resolver_id {
                    ctx.resolvers_data
                        .write()
                        .map_err(|_| Error::new("Internal Server Error"))?
                        .insert(
                            format!("{}_deleted_id", resolver_id),
                            Value::String(id_to_be_deleted),
                        );
                }

                Ok(serde_json::Value::Null)
            }
        }
    }
}
