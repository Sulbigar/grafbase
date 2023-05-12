use super::{
    DeleteAllRelationsInternalInput, DeleteMultipleRelationsInternalInput, DeleteNodeConstraintInternalInput,
    DeleteNodeInternalInput, DeleteRelationInternalInput, DeleteUnitNodeConstraintInput, ExecuteChangesOnDatabase,
    InsertNodeConstraintInternalInput, InsertNodeInternalInput, InsertRelationInternalInput, InsertUniqueConstraint,
    InternalChanges, InternalNodeChanges, InternalNodeConstraintChanges, InternalRelationChanges, ToTransactionError,
    ToTransactionFuture, UpdateNodeConstraintInternalInput, UpdateNodeInternalInput, UpdateRelation,
    UpdateRelationInternalInput, UpdateUniqueConstraint,
};
use crate::constant::{self, PK, SK};
use crate::transaction::TxItemMetadata;
use crate::{DynamoDBBatchersData, DynamoDBContext, OperationAuthorization};
use crate::{RequestedOperation, TxItem};

use dynomite::Attribute;
use graph_entities::{ConstraintID, NodeID};
use rusoto_dynamodb::{Delete, Put, TransactWriteItem, Update};
use std::collections::{HashMap, HashSet};

impl ExecuteChangesOnDatabase for InsertNodeInternalInput {
    fn to_transaction<'a>(
        self,
        batchers: &'a DynamoDBBatchersData,
        ctx: &'a DynamoDBContext,
        pk: String,
        sk: String,
    ) -> ToTransactionFuture<'a> {
        Box::pin(async {
            let InsertNodeInternalInput {
                id,
                mut user_defined_item,
                ty,
                current_datetime,
            } = self;

            let id = NodeID::new_owned(ty, id);
            let now_attr = current_datetime.into_attr();
            let ty_attr = id.ty().into_attr();
            let autogenerated_id_attr = id.into_attr();

            user_defined_item.insert(constant::PK.to_string(), autogenerated_id_attr.clone());
            user_defined_item.insert(constant::SK.to_string(), autogenerated_id_attr.clone());

            user_defined_item.insert(constant::TYPE.to_string(), ty_attr.clone());

            user_defined_item.insert(constant::CREATED_AT.to_string(), now_attr.clone());
            user_defined_item.insert(constant::UPDATED_AT.to_string(), now_attr);

            if let OperationAuthorization::OwnerBased(user_id) = ctx.authorize_operation(RequestedOperation::Create)? {
                user_defined_item.insert(
                    constant::OWNED_BY.to_string(),
                    HashSet::from([user_id.to_string()]).into_attr(),
                );
            }

            user_defined_item.insert(constant::TYPE_INDEX_PK.to_string(), ty_attr);
            user_defined_item.insert(constant::TYPE_INDEX_SK.to_string(), autogenerated_id_attr.clone());

            user_defined_item.insert(constant::INVERTED_INDEX_PK.to_string(), autogenerated_id_attr.clone());
            user_defined_item.insert(constant::INVERTED_INDEX_SK.to_string(), autogenerated_id_attr);

            let mut node_transaction = vec![];

            node_transaction.push(TxItem {
                pk,
                sk,
                relation_name: None,
                metadata: TxItemMetadata::None,
                transaction: TransactWriteItem {
                    put: Some(Put {
                        table_name: ctx.dynamodb_table_name.clone(),
                        item: user_defined_item,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            });

            batchers
                .transaction
                .load_many(node_transaction)
                .await
                .map_err(ToTransactionError::TransactionError)
        })
    }
}

impl ExecuteChangesOnDatabase for UpdateNodeInternalInput {
    fn to_transaction<'a>(
        self,
        batchers: &'a DynamoDBBatchersData,
        ctx: &'a DynamoDBContext,
        pk: String,
        sk: String,
    ) -> ToTransactionFuture<'a> {
        Box::pin(async {
            let UpdateNodeInternalInput {
                id,
                mut user_defined_item,
                ty,
                increments,
                current_datetime,
            } = self;

            let id = NodeID::new_owned(ty, id);
            let now_attr = current_datetime.clone().into_attr();
            let ty_attr = id.ty().into_attr();
            let autogenerated_id_attr = id.into_attr();
            let len = user_defined_item.len();

            user_defined_item.insert(constant::PK.to_string(), autogenerated_id_attr.clone());
            user_defined_item.insert(constant::SK.to_string(), autogenerated_id_attr.clone());

            user_defined_item.insert(constant::TYPE.to_string(), ty_attr.clone());

            user_defined_item.insert(constant::CREATED_AT.to_string(), now_attr.clone());
            user_defined_item.insert(constant::UPDATED_AT.to_string(), now_attr);

            user_defined_item.insert(constant::TYPE_INDEX_PK.to_string(), ty_attr);
            user_defined_item.insert(constant::TYPE_INDEX_SK.to_string(), autogenerated_id_attr.clone());

            user_defined_item.insert(constant::INVERTED_INDEX_PK.to_string(), autogenerated_id_attr.clone());
            user_defined_item.insert(constant::INVERTED_INDEX_SK.to_string(), autogenerated_id_attr);

            let mut node_transaction = vec![];

            let mut exp_att_values = HashMap::with_capacity(len);
            let mut exp_att_names =
                HashMap::from([("#pk".to_string(), PK.to_string()), ("#sk".to_string(), SK.to_string())]);
            let update_expression = Self::to_update_expression(
                current_datetime,
                user_defined_item,
                increments,
                &mut exp_att_values,
                &mut exp_att_names,
            );
            let key = dynomite::attr_map! {
                constant::PK => pk.clone(),
                constant::SK => sk.clone(),
            };
            let mut cond_expr = "attribute_exists(#pk) AND attribute_exists(#sk)".to_string();

            if let OperationAuthorization::OwnerBased(user_id) = ctx.authorize_operation(RequestedOperation::Update)? {
                cond_expr.push_str(" AND contains(#owner_attr_name, :owner_val_name)");
                exp_att_names.insert("#owner_attr_name".to_string(), constant::OWNED_BY.to_string());
                exp_att_values.insert(":owner_val_name".to_string(), user_id.to_string().into_attr());
            }

            let update_transaction: TransactWriteItem = TransactWriteItem {
                update: Some(Update {
                    table_name: ctx.dynamodb_table_name.clone(),
                    key,
                    condition_expression: Some(cond_expr),
                    update_expression,
                    expression_attribute_values: Some(exp_att_values),
                    expression_attribute_names: Some(exp_att_names),
                    ..Default::default()
                }),
                ..Default::default()
            };

            node_transaction.push(TxItem {
                pk,
                sk,
                relation_name: None,
                metadata: TxItemMetadata::None,
                transaction: update_transaction,
            });

            batchers
                .transaction
                .load_many(node_transaction)
                .await
                .map_err(ToTransactionError::TransactionError)
        })
    }
}

fn sanitize_expression_attribute_values(
    values: HashMap<String, dynomite::AttributeValue>,
) -> Option<HashMap<String, dynomite::AttributeValue>> {
    if values.is_empty() {
        None
    } else {
        Some(values)
    }
}

impl ExecuteChangesOnDatabase for DeleteNodeInternalInput {
    fn to_transaction<'a>(
        self,
        batchers: &'a DynamoDBBatchersData,
        ctx: &'a DynamoDBContext,
        pk: String,
        sk: String,
    ) -> ToTransactionFuture<'a> {
        Box::pin(async {
            let key = dynomite::attr_map! {
                    constant::PK => pk.clone(),
                    constant::SK => sk.clone(),
            };

            let mut exp_att_names = HashMap::from([
                ("#pk".to_string(), constant::PK.to_string()),
                ("#sk".to_string(), constant::SK.to_string()),
            ]);
            let mut exp_att_values = HashMap::new();
            let mut cond_expr = "attribute_exists(#pk) AND attribute_exists(#sk)".to_string();
            if let OperationAuthorization::OwnerBased(user_id) = ctx.authorize_operation(RequestedOperation::Delete)? {
                cond_expr.push_str(" AND contains(#owner_attr_name, :owner_val_name)");
                exp_att_names.insert("#owner_attr_name".to_string(), constant::OWNED_BY.to_string());
                exp_att_values.insert(":owner_val_name".to_string(), user_id.to_string().into_attr());
            }
            let delete_transaction = Delete {
                table_name: ctx.dynamodb_table_name.clone(),
                condition_expression: Some(cond_expr),
                key,
                expression_attribute_names: Some(exp_att_names),
                expression_attribute_values: sanitize_expression_attribute_values(exp_att_values),
                ..Default::default()
            };

            let node_transaction = TxItem {
                pk,
                sk,
                relation_name: None,
                metadata: TxItemMetadata::None,
                transaction: TransactWriteItem {
                    delete: Some(delete_transaction),
                    ..Default::default()
                },
            };

            batchers
                .transaction
                .load_many(vec![node_transaction])
                .await
                .map_err(ToTransactionError::TransactionError)
        })
    }
}

impl ExecuteChangesOnDatabase for InternalNodeChanges {
    fn to_transaction<'a>(
        self,
        batchers: &'a DynamoDBBatchersData,
        ctx: &'a DynamoDBContext,
        pk: String,
        sk: String,
    ) -> ToTransactionFuture<'a> {
        match self {
            Self::Insert(a) => a.to_transaction(batchers, ctx, pk, sk),
            Self::Delete(a) => a.to_transaction(batchers, ctx, pk, sk),
            Self::Update(a) => a.to_transaction(batchers, ctx, pk, sk),
        }
    }
}

impl ExecuteChangesOnDatabase for InsertRelationInternalInput {
    fn to_transaction<'a>(
        self,
        batchers: &'a DynamoDBBatchersData,
        ctx: &'a DynamoDBContext,
        pk: String,
        sk: String,
    ) -> ToTransactionFuture<'a> {
        Box::pin(async {
            let InsertRelationInternalInput {
                mut fields,
                relation_names,
                from_ty,
                to_ty,
                current_datetime,
                ..
            } = self;

            let now_attr = current_datetime.clone().into_attr();
            let gsi1pk_attr = from_ty.into_attr();
            let ty_attr = to_ty.into_attr();
            let partition_key_attr = pk.clone().into_attr();
            let sorting_key_attr = sk.clone().into_attr();

            // We're adding a relation row with its own PK & SK
            fields.remove(constant::PK);
            fields.remove(constant::SK);

            // The relation holds a copy of the the actual node. If node didn't exist before
            // we won't have any of the reserved fields initialized so we need to set the
            // createdAt/updatedAt to the same date as the function creating the node outside.
            // Relying on CurrentDateTime ensures we'll be consistent.
            // If the node exist already, we keep the existing values. This is NOT the creation
            // date of the relation it's a COPY of the node data.
            fields
                .entry(constant::CREATED_AT.to_string())
                .or_insert_with(|| now_attr.clone());
            fields.entry(constant::UPDATED_AT.to_string()).or_insert(now_attr);

            // Present or not, those fields would be same. So we're just overwriting them without
            // further logic.
            fields.insert(constant::TYPE.to_string(), ty_attr.clone());
            fields.insert(constant::TYPE_INDEX_PK.to_string(), gsi1pk_attr);
            fields.insert(constant::TYPE_INDEX_SK.to_string(), partition_key_attr.clone());
            fields.insert(constant::INVERTED_INDEX_PK.to_string(), sorting_key_attr);
            fields.insert(constant::INVERTED_INDEX_SK.to_string(), partition_key_attr);
            if let OperationAuthorization::OwnerBased(user_id) = ctx.authorize_operation(RequestedOperation::Create)? {
                fields.insert(
                    constant::OWNED_BY.to_string(),
                    HashSet::from([user_id.to_string()]).into_attr(),
                );
            }

            let mut exp_values = HashMap::with_capacity(fields.len() + 1);
            let mut exp_att_names = HashMap::with_capacity(fields.len() + 1);
            let update_expression = UpdateRelationInternalInput::to_update_expression(
                current_datetime,
                fields,
                &mut exp_values,
                &mut exp_att_names,
                relation_names.into_iter().map(UpdateRelation::Add).collect(),
                true,
            );

            let key = dynomite::attr_map! {
                    constant::PK => pk.clone(),
                    constant::SK => sk.clone(),
            };

            let update_transaction: TransactWriteItem = TransactWriteItem {
                update: Some(Update {
                    table_name: ctx.dynamodb_table_name.clone(),
                    key,
                    update_expression,
                    expression_attribute_values: Some(exp_values),
                    expression_attribute_names: Some(exp_att_names),
                    ..Default::default()
                }),
                ..Default::default()
            };

            let node_transaction = TxItem {
                pk,
                sk,
                relation_name: None,
                metadata: TxItemMetadata::None,
                transaction: update_transaction,
            };

            batchers
                .transaction
                .load_many(vec![node_transaction])
                .await
                .map_err(ToTransactionError::TransactionError)
        })
    }
}

impl ExecuteChangesOnDatabase for DeleteAllRelationsInternalInput {
    fn to_transaction<'a>(
        self,
        batchers: &'a DynamoDBBatchersData,
        ctx: &'a DynamoDBContext,
        pk: String,
        sk: String,
    ) -> ToTransactionFuture<'a> {
        Box::pin(async {
            let key = dynomite::attr_map! {
                    constant::PK => pk.clone(),
                    constant::SK => sk.clone(),
            };

            let exp_att_names = HashMap::from([
                ("#pk".to_string(), "__pk".to_string()),
                ("#sk".to_string(), "__sk".to_string()),
            ]);

            let delete_transaction = Delete {
                table_name: ctx.dynamodb_table_name.clone(),
                condition_expression: Some("attribute_exists(#pk) AND attribute_exists(#sk)".to_string()),
                expression_attribute_names: Some(exp_att_names),
                key,
                ..Default::default()
            };

            let node_transaction = TxItem {
                pk,
                sk,
                relation_name: None,
                metadata: TxItemMetadata::None,
                transaction: TransactWriteItem {
                    delete: Some(delete_transaction),
                    ..Default::default()
                },
            };

            batchers
                .transaction
                .load_many(vec![node_transaction])
                .await
                .map_err(ToTransactionError::TransactionError)
        })
    }
}

impl ExecuteChangesOnDatabase for DeleteMultipleRelationsInternalInput {
    fn to_transaction<'a>(
        self,
        batchers: &'a DynamoDBBatchersData,
        ctx: &'a DynamoDBContext,
        pk: String,
        sk: String,
    ) -> ToTransactionFuture<'a> {
        Box::pin(async {
            let DeleteMultipleRelationsInternalInput {
                relation_names,
                current_datetime,
                ..
            } = self;

            let now_attr = current_datetime.clone().into_attr();

            let mut user_defined_item = HashMap::with_capacity(1);
            user_defined_item.insert(constant::UPDATED_AT.to_string(), now_attr);

            let mut exp_values = HashMap::with_capacity(16);
            let mut exp_att_names = HashMap::with_capacity(user_defined_item.len() + 1);

            let update_expression = UpdateRelationInternalInput::to_update_expression(
                current_datetime,
                user_defined_item,
                &mut exp_values,
                &mut exp_att_names,
                relation_names.into_iter().map(UpdateRelation::Remove).collect(),
                false,
            );
            let key = dynomite::attr_map! {
                    constant::PK => pk.clone(),
                    constant::SK => sk.clone(),
            };

            let update_transaction: TransactWriteItem = TransactWriteItem {
                update: Some(Update {
                    table_name: ctx.dynamodb_table_name.clone(),
                    key,
                    update_expression,
                    expression_attribute_values: Some(exp_values),
                    expression_attribute_names: Some(exp_att_names),
                    ..Default::default()
                }),
                ..Default::default()
            };

            let node_transaction = TxItem {
                pk,
                sk,
                relation_name: None,
                metadata: TxItemMetadata::None,
                transaction: update_transaction,
            };

            batchers
                .transaction
                .load_many(vec![node_transaction])
                .await
                .map_err(ToTransactionError::TransactionError)
        })
    }
}

impl ExecuteChangesOnDatabase for DeleteRelationInternalInput {
    fn to_transaction<'a>(
        self,
        batchers: &'a DynamoDBBatchersData,
        ctx: &'a DynamoDBContext,
        pk: String,
        sk: String,
    ) -> ToTransactionFuture<'a> {
        match self {
            Self::All(a) => a.to_transaction(batchers, ctx, pk, sk),
            Self::Multiple(a) => a.to_transaction(batchers, ctx, pk, sk),
        }
    }
}

impl ExecuteChangesOnDatabase for UpdateRelationInternalInput {
    fn to_transaction<'a>(
        self,
        batchers: &'a DynamoDBBatchersData,
        ctx: &'a DynamoDBContext,
        pk: String,
        sk: String,
    ) -> ToTransactionFuture<'a> {
        Box::pin(async {
            let UpdateRelationInternalInput {
                mut user_defined_item,
                relation_names,
                current_datetime,
                ..
            } = self;

            let now_attr = current_datetime.clone().into_attr();
            user_defined_item.insert(constant::UPDATED_AT.to_string(), now_attr);

            let mut exp_values = HashMap::with_capacity(user_defined_item.len() + 1);
            let mut exp_att_names = HashMap::with_capacity(user_defined_item.len() + 1);
            let update_expression = Self::to_update_expression(
                current_datetime,
                user_defined_item,
                &mut exp_values,
                &mut exp_att_names,
                relation_names,
                false,
            );

            let key = dynomite::attr_map! {
                    constant::PK => pk.clone(),
                    constant::SK => sk.clone(),
            };

            let update_transaction: TransactWriteItem = TransactWriteItem {
                update: Some(Update {
                    table_name: ctx.dynamodb_table_name.clone(),
                    key,
                    update_expression,
                    expression_attribute_values: Some(exp_values),
                    expression_attribute_names: Some(exp_att_names),
                    ..Default::default()
                }),
                ..Default::default()
            };

            let node_transaction = TxItem {
                pk,
                sk,
                relation_name: None,
                metadata: TxItemMetadata::None,
                transaction: update_transaction,
            };

            batchers
                .transaction
                .load_many(vec![node_transaction])
                .await
                .map_err(ToTransactionError::TransactionError)
        })
    }
}

impl ExecuteChangesOnDatabase for DeleteUnitNodeConstraintInput {
    fn to_transaction<'a>(
        self,
        batchers: &'a DynamoDBBatchersData,
        ctx: &'a DynamoDBContext,
        pk: String,
        sk: String,
    ) -> ToTransactionFuture<'a> {
        Box::pin(async {
            let key = dynomite::attr_map! {
                    constant::PK => pk.clone(),
                    constant::SK => sk.clone(),
            };

            let exp_att_names = HashMap::from([
                ("#pk".to_string(), constant::PK.to_string()),
                ("#sk".to_string(), constant::SK.to_string()),
            ]);
            let delete_transaction = Delete {
                table_name: ctx.dynamodb_table_name.clone(),
                condition_expression: Some("attribute_exists(#pk) AND attribute_exists(#sk)".to_string()),
                key,
                expression_attribute_names: Some(exp_att_names),
                ..Default::default()
            };

            let node_transaction = TxItem {
                pk,
                sk,
                relation_name: None,
                metadata: TxItemMetadata::None,
                transaction: TransactWriteItem {
                    delete: Some(delete_transaction),
                    ..Default::default()
                },
            };

            batchers
                .transaction
                .load_many(vec![node_transaction])
                .await
                .map_err(ToTransactionError::TransactionError)
        })
    }
}

impl ExecuteChangesOnDatabase for InsertUniqueConstraint {
    fn to_transaction<'a>(
        self,
        batchers: &'a DynamoDBBatchersData,
        ctx: &'a DynamoDBContext,
        pk: String,
        _sk: String,
    ) -> ToTransactionFuture<'a> {
        Box::pin(async {
            // A Unique directive is a Constraint on a specific field
            let InsertUniqueConstraint {
                ty,
                target,
                mut user_defined_item,
                current_datetime,
                constraint_values,
                constraint_fields,
            } = self;

            let id = ConstraintID::try_from(pk).expect("Wrong Constraint ID");
            let ty_attr = ty.into_attr();

            let exp_att_names = HashMap::from([("#pk".to_string(), constant::PK.to_string())]);

            let now_attr = current_datetime.into_attr();

            user_defined_item.insert(constant::PK.to_string(), id.to_string().into_attr());
            user_defined_item.insert(constant::SK.to_string(), id.to_string().into_attr());
            user_defined_item.insert(constant::TYPE.to_string(), ty_attr);
            user_defined_item.insert(constant::INVERTED_INDEX_PK.to_string(), target.into_attr());
            user_defined_item.insert(constant::INVERTED_INDEX_SK.to_string(), id.to_string().into_attr());
            user_defined_item.insert(constant::CREATED_AT.to_string(), now_attr.clone());
            user_defined_item.insert(constant::UPDATED_AT.to_string(), now_attr);

            if let OperationAuthorization::OwnerBased(user_id) = ctx.authorize_operation(RequestedOperation::Create)? {
                user_defined_item.insert(
                    constant::OWNED_BY.to_string(),
                    HashSet::from([user_id.to_string()]).into_attr(),
                );
            }

            // If user_defined_item is passed in as part of an update it'll have these
            // keys in it and we do not want them on a unique constraint.
            // Seems like it would be easier to not have these in user_defined_item
            // in the first place but here we are.
            user_defined_item.remove(&constant::TYPE_INDEX_PK.to_string());
            user_defined_item.remove(&constant::TYPE_INDEX_SK.to_string());

            let node_transaction = TxItem {
                pk: id.to_string(),
                sk: id.to_string(),
                relation_name: None,
                metadata: TxItemMetadata::Unique {
                    values: constraint_values.clone(),
                    fields: constraint_fields.clone(),
                },
                transaction: TransactWriteItem {
                    // We can do a Put here because we only have the Unique constraint, as soon
                    // as we have other cnstraints sharing the same row in db, we'll need to
                    // move to an update.
                    put: Some(Put {
                        table_name: ctx.dynamodb_table_name.clone(),
                        item: user_defined_item,
                        condition_expression: Some("attribute_not_exists(#pk)".to_string()),
                        expression_attribute_names: Some(exp_att_names),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            };

            batchers
                .transaction
                .load_many(vec![node_transaction])
                .await
                .map_err(|err| ToTransactionError::UniqueCondition {
                    source: err,
                    values: constraint_values,
                    fields: constraint_fields,
                })
        })
    }
}

impl ExecuteChangesOnDatabase for UpdateUniqueConstraint {
    fn to_transaction<'a>(
        self,
        batchers: &'a DynamoDBBatchersData,
        ctx: &'a DynamoDBContext,
        pk: String,
        sk: String,
    ) -> ToTransactionFuture<'a> {
        Box::pin(async {
            let UpdateUniqueConstraint {
                target,
                mut user_defined_item,
                increments,
                current_datetime,
            } = self;

            let id = ConstraintID::try_from(pk.clone()).expect("Wrong Constraint ID");
            let now_attr = current_datetime.clone().into_attr();

            user_defined_item.insert(constant::PK.to_string(), id.to_string().into_attr());
            user_defined_item.insert(constant::SK.to_string(), id.to_string().into_attr());
            user_defined_item.insert(constant::INVERTED_INDEX_PK.to_string(), target.into_attr());
            user_defined_item.insert(constant::INVERTED_INDEX_SK.to_string(), id.to_string().into_attr());
            user_defined_item.insert(constant::CREATED_AT.to_string(), now_attr.clone());
            user_defined_item.insert(constant::UPDATED_AT.to_string(), now_attr);

            user_defined_item.remove(&constant::TYPE_INDEX_PK.to_string());
            user_defined_item.remove(&constant::TYPE_INDEX_SK.to_string());

            let key = dynomite::attr_map! {
                constant::PK => pk.clone(),
                constant::SK => sk.clone(),
            };

            let mut node_transaction = vec![];

            let len = user_defined_item.len();

            let mut exp_att_values = HashMap::with_capacity(len);
            let mut exp_att_names =
                HashMap::from([("#pk".to_string(), PK.to_string()), ("#sk".to_string(), SK.to_string())]);

            let update_expression = Self::to_update_expression(
                current_datetime,
                user_defined_item,
                increments,
                &mut exp_att_values,
                &mut exp_att_names,
            );
            let mut cond_expr = "attribute_exists(#pk) AND attribute_exists(#sk)".to_string();
            if let OperationAuthorization::OwnerBased(user_id) = ctx.authorize_operation(RequestedOperation::Update)? {
                cond_expr.push_str(" AND contains(#owner_attr_name, :owner_val_name)");
                exp_att_names.insert("#owner_attr_name".to_string(), constant::OWNED_BY.to_string());
                exp_att_values.insert(":owner_val_name".to_string(), user_id.to_string().into_attr());
            }

            let update_transaction: TransactWriteItem = TransactWriteItem {
                update: Some(Update {
                    table_name: ctx.dynamodb_table_name.clone(),
                    key,
                    condition_expression: Some(cond_expr),
                    update_expression,
                    expression_attribute_values: Some(exp_att_values),
                    expression_attribute_names: Some(exp_att_names),
                    ..Default::default()
                }),
                ..Default::default()
            };

            node_transaction.push(TxItem {
                pk,
                sk,
                relation_name: None,
                metadata: TxItemMetadata::None,
                transaction: update_transaction,
            });

            batchers
                .transaction
                .load_many(node_transaction)
                .await
                .map_err(ToTransactionError::TransactionError)
        })
    }
}

impl ExecuteChangesOnDatabase for DeleteNodeConstraintInternalInput {
    fn to_transaction<'a>(
        self,
        batchers: &'a DynamoDBBatchersData,
        ctx: &'a DynamoDBContext,
        pk: String,
        sk: String,
    ) -> ToTransactionFuture<'a> {
        match self {
            Self::Unit(a) => a.to_transaction(batchers, ctx, pk, sk),
        }
    }
}

impl ExecuteChangesOnDatabase for InsertNodeConstraintInternalInput {
    fn to_transaction<'a>(
        self,
        batchers: &'a DynamoDBBatchersData,
        ctx: &'a DynamoDBContext,
        pk: String,
        sk: String,
    ) -> ToTransactionFuture<'a> {
        match self {
            Self::Unique(a) => a.to_transaction(batchers, ctx, pk, sk),
        }
    }
}

impl ExecuteChangesOnDatabase for InternalNodeConstraintChanges {
    fn to_transaction<'a>(
        self,
        batchers: &'a DynamoDBBatchersData,
        ctx: &'a DynamoDBContext,
        pk: String,
        sk: String,
    ) -> ToTransactionFuture<'a> {
        match self {
            Self::Delete(a) => a.to_transaction(batchers, ctx, pk, sk),
            Self::Update(a) => a.to_transaction(batchers, ctx, pk, sk),
            Self::Insert(a) => a.to_transaction(batchers, ctx, pk, sk),
        }
    }
}

impl ExecuteChangesOnDatabase for InternalRelationChanges {
    fn to_transaction<'a>(
        self,
        batchers: &'a DynamoDBBatchersData,
        ctx: &'a DynamoDBContext,
        pk: String,
        sk: String,
    ) -> ToTransactionFuture<'a> {
        match self {
            Self::Insert(a) => a.to_transaction(batchers, ctx, pk, sk),
            Self::Delete(a) => a.to_transaction(batchers, ctx, pk, sk),
            Self::Update(a) => a.to_transaction(batchers, ctx, pk, sk),
        }
    }
}

impl ExecuteChangesOnDatabase for Vec<InternalChanges> {
    fn to_transaction<'a>(
        self,
        batchers: &'a DynamoDBBatchersData,
        ctx: &'a DynamoDBContext,
        pk: String,
        sk: String,
    ) -> ToTransactionFuture<'a> {
        let mut list = self.into_iter();
        let first = list.next().map(|first| list.try_fold(first, |acc, cur| acc.with(cur)));

        let Some(Ok(first)) = first else {
            return Box::pin(async { Err(ToTransactionError::Unknown) });
        };

        first.to_transaction(batchers, ctx, pk, sk)
    }
}

impl ExecuteChangesOnDatabase for InternalChanges {
    fn to_transaction<'a>(
        self,
        batchers: &'a DynamoDBBatchersData,
        ctx: &'a DynamoDBContext,
        pk: String,
        sk: String,
    ) -> ToTransactionFuture<'a> {
        match self {
            Self::Node(a) => a.to_transaction(batchers, ctx, pk, sk),
            Self::Relation(a) => a.to_transaction(batchers, ctx, pk, sk),
            Self::NodeConstraints(a) => a.to_transaction(batchers, ctx, pk, sk),
        }
    }
}

impl ExecuteChangesOnDatabase for UpdateNodeConstraintInternalInput {
    fn to_transaction<'a>(
        self,
        batchers: &'a DynamoDBBatchersData,
        ctx: &'a DynamoDBContext,
        pk: String,
        sk: String,
    ) -> ToTransactionFuture<'a> {
        match self {
            Self::Unique(a) => a.to_transaction(batchers, ctx, pk, sk),
        }
    }
}
