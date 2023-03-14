use super::types::{Constraint, OperationKind, Record, Row, Sql};
use crate::constant::*;
use crate::graph_transaction::{
    DeleteAllRelationsInternalInput, DeleteMultipleRelationsInternalInput, DeleteNodeConstraintInternalInput,
    DeleteNodeInternalInput, DeleteRelationInternalInput, DeleteUnitNodeConstraintInput, ExecuteChangesOnDatabase,
    InsertNodeConstraintInternalInput, InsertNodeInternalInput, InsertRelationInternalInput, InsertUniqueConstraint,
    InternalChanges, InternalNodeChanges, InternalNodeConstraintChanges, InternalRelationChanges, ToTransactionError,
    ToTransactionFuture, UpdateNodeConstraintInternalInput, UpdateNodeInternalInput, UpdateRelation,
    UpdateRelationInternalInput, UpdateUniqueConstraint,
};
use crate::local::types::SqlValue;
use crate::{DynamoDBBatchersData, DynamoDBContext};
use chrono::{DateTime, Utc};
use dynomite::{Attribute, AttributeValue};
use graph_entities::{ConstraintID, NodeID};
use itertools::Itertools;
use maplit::hashmap;
use std::collections::{HashMap, VecDeque};

impl ExecuteChangesOnDatabase for InsertNodeInternalInput {
    fn to_transaction<'a>(
        self,
        _batchers: &'a DynamoDBBatchersData,
        ctx: &'a DynamoDBContext,
        pk: String,
        sk: String,
    ) -> ToTransactionFuture<'a> {
        Box::pin(async {
            let InsertNodeInternalInput {
                id,
                user_defined_item,
                ty,
                current_datetime,
            } = self;

            let id = NodeID::new_owned(ty, id);

            let now_attr = current_datetime.clone().into_attr();
            let ty_attr = id.ty().into_attr();
            let autogenerated_id_attr = id.clone().into_attr();

            let mut document = user_defined_item;

            document.insert(PK.to_string(), autogenerated_id_attr.clone());
            document.insert(SK.to_string(), autogenerated_id_attr.clone());
            document.insert(TYPE.to_string(), ty_attr.clone());
            document.insert(CREATED_AT.to_string(), now_attr.clone());
            document.insert(UPDATED_AT.to_string(), now_attr);
            document.insert(TYPE_INDEX_PK.to_string(), ty_attr);
            document.insert(TYPE_INDEX_SK.to_string(), autogenerated_id_attr.clone());
            document.insert(INVERTED_INDEX_PK.to_string(), autogenerated_id_attr.clone());
            document.insert(INVERTED_INDEX_SK.to_string(), autogenerated_id_attr);
            document.insert(OWNED_BY.to_string(), vec![ctx.user_id.clone()].into_attr());

            let record = Record {
                pk,
                sk,
                entity_type: Some(id.ty().to_string()),
                created_at: current_datetime.clone().into(),
                updated_at: current_datetime.into(),
                relation_names: Default::default(),
                gsi1pk: Some(id.ty().to_string()),
                gsi1sk: Some(id.to_string()),
                gsi2pk: Some(id.to_string()),
                gsi2sk: Some(id.to_string()),
                document,
            };

            let row = Row::from_record(record);

            let (query, values) = Sql::Insert(&row).compile(row.values.clone());

            Ok((query, values, None))
        })
    }
}

impl ExecuteChangesOnDatabase for UpdateNodeInternalInput {
    fn to_transaction<'a>(
        self,
        _batchers: &'a DynamoDBBatchersData,
        _ctx: &'a DynamoDBContext,
        pk: String,
        sk: String,
    ) -> ToTransactionFuture<'a> {
        Box::pin(async {
            let UpdateNodeInternalInput {
                id,
                user_defined_item,
                ty,
                increments,
                current_datetime,
            } = self;

            let id = NodeID::new_owned(ty, id);
            let ty_attr = id.ty().into_attr();
            let autogenerated_id_attr = id.into_attr();
            let now_attr = current_datetime.clone().into_attr();

            let mut document = user_defined_item;

            document.insert(PK.to_string(), pk.clone().into_attr());
            document.insert(SK.to_string(), sk.clone().into_attr());

            document.insert(TYPE.to_string(), ty_attr.clone());

            document.insert(CREATED_AT.to_string(), now_attr.clone());
            document.insert(UPDATED_AT.to_string(), now_attr);

            document.insert(TYPE_INDEX_PK.to_string(), ty_attr);
            document.insert(TYPE_INDEX_SK.to_string(), autogenerated_id_attr.clone());

            document.insert(INVERTED_INDEX_PK.to_string(), autogenerated_id_attr.clone());
            document.insert(INVERTED_INDEX_SK.to_string(), autogenerated_id_attr);

            let document = serde_json::to_string(&document).expect("must serialize");

            let (increment_fields, increment_values): (Vec<_>, Vec<_>) = increments.iter().unzip();

            let increment_values = increment_values
                .iter()
                .map(|increment_value| increment_value.n.clone().expect("must exist"))
                .collect::<VecDeque<_>>();

            let (query, values) = Sql::Update(increment_fields).compile(hashmap! {
                "pk" => SqlValue::String(pk),
                "sk" => SqlValue::String(sk),
                "document" => SqlValue::String(document),
                "updated_at" => SqlValue::String(current_datetime.to_string()),
                "increments" => SqlValue::VecDeque(increment_values)
            });

            Ok((query, values, None))
        })
    }
}

impl ExecuteChangesOnDatabase for UpdateUniqueConstraint {
    fn to_transaction<'a>(
        self,
        _batchers: &'a DynamoDBBatchersData,
        _ctx: &'a DynamoDBContext,
        pk: String,
        sk: String,
    ) -> ToTransactionFuture<'a> {
        Box::pin(async {
            let UpdateUniqueConstraint {
                target,
                user_defined_item,
                increments,
                current_datetime,
                ..
            } = self;

            let id = ConstraintID::try_from(pk.clone()).expect("Wrong Constraint ID");
            let now_attr = current_datetime.clone().into_attr();
            let id_attr = id.to_string().into_attr();

            let mut document: HashMap<String, AttributeValue> = user_defined_item;

            document.insert(PK.to_string(), id_attr.clone());
            document.insert(SK.to_string(), id_attr.clone());
            document.insert(CREATED_AT.to_string(), now_attr.clone());
            document.insert(UPDATED_AT.to_string(), now_attr);
            document.insert(INVERTED_INDEX_PK.to_string(), target.into_attr());
            document.insert(INVERTED_INDEX_SK.to_string(), id_attr);

            document.remove(&TYPE_INDEX_PK.to_string());
            document.remove(&TYPE_INDEX_SK.to_string());

            let document = serde_json::to_string(&document).expect("must serialize");

            let (increment_fields, increment_values): (Vec<_>, Vec<_>) = increments.iter().unzip();

            let increment_values = increment_values
                .iter()
                .map(|increment_value| increment_value.n.clone().expect("must exist"))
                .collect::<VecDeque<_>>();

            let (query, values) = Sql::Update(increment_fields).compile(hashmap! {
                "pk" => SqlValue::String(pk),
                "sk" => SqlValue::String(sk),
                "document" => SqlValue::String(document),
                "updated_at" => SqlValue::String(current_datetime.to_string()),
                "increments" => SqlValue::VecDeque(increment_values)
            });

            Ok((query, values, None))
        })
    }
}

impl ExecuteChangesOnDatabase for DeleteNodeInternalInput {
    fn to_transaction<'a>(
        self,
        _batchers: &'a DynamoDBBatchersData,
        ctx: &'a DynamoDBContext,
        pk: String,
        sk: String,
    ) -> ToTransactionFuture<'a> {
        Box::pin(async {
            let mut map = hashmap! {
                "pk" => SqlValue::String(pk),
                "sk" => SqlValue::String(sk),
            };
            if let Some(user_id) = &ctx.user_id {
                map.insert(crate::local::types::OWNED_BY_KEY, SqlValue::String(user_id.to_string()));
            }
            let (query, values) = Sql::DeleteByIds {
                filter_by_owner: ctx.user_id.is_some(),
            }
            .compile(map);
            Ok((query, values, None))
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
            Self::Insert(input) => input.to_transaction(batchers, ctx, pk, sk),
            Self::Delete(input) => input.to_transaction(batchers, ctx, pk, sk),
            Self::Update(input) => input.to_transaction(batchers, ctx, pk, sk),
        }
    }
}

impl ExecuteChangesOnDatabase for InsertRelationInternalInput {
    fn to_transaction<'a>(
        self,
        _batchers: &'a DynamoDBBatchersData,
        ctx: &'a DynamoDBContext,
        pk: String,
        sk: String,
    ) -> ToTransactionFuture<'a> {
        Box::pin(async move {
            let InsertRelationInternalInput {
                fields,
                relation_names,
                from_ty,
                to_ty,
                current_datetime,
                ..
            } = self;

            let mut document = fields;

            let now_attr = current_datetime.into_attr();
            let gsi1pk_attr = from_ty.clone().into_attr();
            let ty_attr = to_ty.clone().into_attr();

            document.insert(PK.to_string(), pk.clone().into_attr());
            document.insert(SK.to_string(), sk.clone().into_attr());
            document.insert(TYPE.to_string(), ty_attr);
            // The relation stores a COPY of the node, so it's the createdAt of the
            // node not of the relation here. As the document may be a new node
            // we need to add all reserved fields. But if it's an existing one we have
            // to keep the original values of createdAt & updatedAt.
            document
                .entry(CREATED_AT.to_string())
                .or_insert_with(|| now_attr.clone());
            document.entry(UPDATED_AT.to_string()).or_insert(now_attr);
            document.insert(TYPE_INDEX_PK.to_string(), gsi1pk_attr);
            document.insert(TYPE_INDEX_SK.to_string(), pk.clone().into_attr());
            document.insert(INVERTED_INDEX_PK.to_string(), sk.clone().into_attr());
            document.insert(INVERTED_INDEX_SK.to_string(), pk.clone().into_attr());
            document.insert(
                RELATION_NAMES.to_string(),
                AttributeValue {
                    ss: Some(relation_names.clone()),
                    ..Default::default()
                },
            );
            document
                .entry(OWNED_BY.to_string())
                .or_insert_with(|| ctx.user_id.clone().into_attr());

            let record = Record {
                pk: pk.clone(),
                sk: sk.clone(),
                entity_type: Some(to_ty),
                created_at: DateTime::<Utc>::from_attr(document.get(CREATED_AT).expect("Was added before.").clone())
                    .expect("Has to be valid"),
                updated_at: DateTime::<Utc>::from_attr(document.get(UPDATED_AT).expect("Was added before.").clone())
                    .expect("Has to be valid"),
                gsi1pk: Some(from_ty),
                gsi1sk: Some(pk.clone()),
                gsi2pk: Some(sk.clone()),
                gsi2sk: Some(pk.clone()),
                relation_names: relation_names.clone(),
                document,
            };

            let row = Row::from_record(record);

            let mut value_map = row.values.clone();

            value_map.insert("to_add", SqlValue::VecDeque(relation_names.clone().into()));

            let (query, values) = Sql::InsertRelation(&row, relation_names.len()).compile(value_map);

            Ok((query, values, None))
        })
    }
}

impl ExecuteChangesOnDatabase for DeleteAllRelationsInternalInput {
    fn to_transaction<'a>(
        self,
        _batchers: &'a DynamoDBBatchersData,
        ctx: &'a DynamoDBContext,
        pk: String,
        sk: String,
    ) -> ToTransactionFuture<'a> {
        Box::pin(async {
            let mut map = hashmap! {
                "pk"=> SqlValue::String(pk),
                "sk" => SqlValue::String(sk),
            };
            if let Some(user_id) = &ctx.user_id {
                map.insert(crate::local::types::OWNED_BY_KEY, SqlValue::String(user_id.to_string()));
            }
            let (query, values) = Sql::DeleteByIds {
                filter_by_owner: ctx.user_id.is_some(),
            }
            .compile(map);
            Ok((query, values, None))
        })
    }
}

impl ExecuteChangesOnDatabase for DeleteMultipleRelationsInternalInput {
    fn to_transaction<'a>(
        self,
        _batchers: &'a DynamoDBBatchersData,
        _ctx: &'a DynamoDBContext,
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

            let mut document = HashMap::<String, AttributeValue>::new();

            document.insert(UPDATED_AT.to_string(), now_attr);

            let document = serde_json::to_string(&document).expect("must serialize");

            let value_map = hashmap! {
                "pk" => SqlValue::String(pk),
                "sk" => SqlValue::String(sk),
                "to_remove" => SqlValue::VecDeque(relation_names.clone().into()),
                "document" => SqlValue::String(document),
                "updated_at" => SqlValue::String(current_datetime.to_string()),
            };

            let (query, values) = Sql::DeleteRelations(relation_names.len()).compile(value_map);

            Ok((query, values, None))
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
        _batchers: &'a DynamoDBBatchersData,
        _ctx: &'a DynamoDBContext,
        pk: String,
        sk: String,
    ) -> ToTransactionFuture<'a> {
        Box::pin(async {
            let UpdateRelationInternalInput {
                user_defined_item,
                relation_names,
                current_datetime,
                ..
            } = self;

            let (removed, added): (Vec<String>, Vec<String>) =
                relation_names.into_iter().partition_map(|relation| match relation {
                    UpdateRelation::Add(a) => itertools::Either::Right(a),
                    UpdateRelation::Remove(a) => itertools::Either::Left(a),
                });

            let now_attr = current_datetime.clone().into_attr();

            let mut document = user_defined_item;
            document.insert(UPDATED_AT.to_string(), now_attr);
            let document = serde_json::to_string(&document).expect("must serialize");

            let value_map = hashmap! {
                "pk" => SqlValue::String(pk),
                "sk" => SqlValue::String(sk),
                "to_remove" => SqlValue::VecDeque(removed.clone().into()),
                "to_add" => SqlValue::VecDeque(added.clone().into()),
                "document" => SqlValue::String(document),
                "updated_at" => SqlValue::String(current_datetime.to_string())
            };

            let (query, values) = Sql::UpdateWithRelations(removed.len(), added.len()).compile(value_map);

            Ok((query, values, None))
        })
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
            Self::Insert(input) => input.to_transaction(batchers, ctx, pk, sk),
            Self::Delete(input) => input.to_transaction(batchers, ctx, pk, sk),
            Self::Update(input) => input.to_transaction(batchers, ctx, pk, sk),
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
            Self::Node(input) => input.to_transaction(batchers, ctx, pk, sk),
            Self::Relation(input) => input.to_transaction(batchers, ctx, pk, sk),
            Self::NodeConstraints(input) => input.to_transaction(batchers, ctx, pk, sk),
        }
    }
}

impl ExecuteChangesOnDatabase for DeleteUnitNodeConstraintInput {
    fn to_transaction<'a>(
        self,
        _batchers: &'a DynamoDBBatchersData,
        ctx: &'a DynamoDBContext,
        pk: String,
        sk: String,
    ) -> ToTransactionFuture<'a> {
        Box::pin(async {
            let mut map = hashmap! {
                "pk" => SqlValue::String(pk),
                "sk" => SqlValue::String(sk)
            };
            if let Some(user_id) = &ctx.user_id {
                map.insert(crate::local::types::OWNED_BY_KEY, SqlValue::String(user_id.to_string()));
            }
            let (query, values) = Sql::DeleteByIds {
                filter_by_owner: ctx.user_id.is_some(),
            }
            .compile(map);

            Ok((query, values, None))
        })
    }
}

impl ExecuteChangesOnDatabase for InsertUniqueConstraint {
    fn to_transaction<'a>(
        self,
        _batchers: &'a DynamoDBBatchersData,
        ctx: &'a DynamoDBContext,
        pk: String,
        sk: String,
    ) -> ToTransactionFuture<'a> {
        Box::pin(async {
            let InsertUniqueConstraint {
                target,
                user_defined_item,
                current_datetime,
                constraint_fields,
                constraint_values,
            } = self;

            let id = ConstraintID::try_from(pk.clone()).expect("Wrong Constraint ID");
            let now_attr = current_datetime.clone().into_attr();
            let id_attr = id.to_string().into_attr();

            let mut document: HashMap<String, AttributeValue> = user_defined_item;

            document.insert(PK.to_string(), id_attr.clone());
            document.insert(SK.to_string(), id_attr.clone());
            document.insert(CREATED_AT.to_string(), now_attr.clone());
            document.insert(UPDATED_AT.to_string(), now_attr);
            document.insert(INVERTED_INDEX_PK.to_string(), target.clone().into_attr());
            document.insert(INVERTED_INDEX_SK.to_string(), id_attr);
            document.insert(OWNED_BY.to_string(), vec![ctx.user_id.clone()].into_attr());

            let record = Record {
                pk,
                sk,
                entity_type: None,
                created_at: current_datetime.clone().into(),
                updated_at: current_datetime.into(),
                relation_names: Default::default(),
                gsi1pk: None,
                gsi1sk: None,
                gsi2pk: Some(target),
                gsi2sk: Some(id.to_string()),
                document,
            };

            let row = Row::from_record(record);

            let (query, values) = Sql::Insert(&row).compile(row.values.clone());

            Ok((
                query,
                values,
                Some(OperationKind::Constraint(Constraint::Unique {
                    values: constraint_values,
                    fields: constraint_fields,
                })),
            ))
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
            Self::Insert(a) => a.to_transaction(batchers, ctx, pk, sk),
            Self::Update(a) => a.to_transaction(batchers, ctx, pk, sk),
        }
    }
}
