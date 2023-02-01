//! Resolvers dynamic strategy explained here.
//!
//! A Resolver is a part of the way we resolve a field. It's an asynchronous
//! operation which is cached based on his id and on his execution_id.
//!
//! When you `resolve` a Resolver, you have access to the `ResolverContext`
//! which will grant you access to the current `Transformer` that must be
//! applied to this resolve, after getting the data by the resolvers.
//!
//! A Resolver always know how to apply the associated transformers.

use self::debug::DebugResolver;
use crate::{Context, Error};
use context_data::ContextDataResolver;
use derivative::Derivative;
use dynamo_mutation::DynamoMutationResolver;
use dynamo_querying::DynamoResolver;
use dynamodb::PaginatedCursor;
use dynaql_parser::types::SelectionSet;
use dynaql_value::{ConstValue, Name};
use graph_entities::{cursor::PaginationCursor, ConstraintID};

use std::sync::Arc;
use ulid::Ulid;

use super::{Constraint, MetaField, MetaType};

pub mod context_data;
pub mod debug;
pub mod dynamo_mutation;
pub mod dynamo_querying;

/// Resolver declarative struct to assign a Resolver for a Field.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, Hash, PartialEq, Eq)]
pub struct Resolver {
    /// Unique id to identify Resolver.
    pub id: Option<String>,
    pub r#type: ResolverType,
}

/// Resolver Context
///
/// Each time a Resolver is accessed to be resolved, a context for the resolving
/// strategy is created.
///
/// This context contain safe access data to be used inside `ResolverTrait`.
/// This give you access to the `resolver_id` which define the resolver, the
/// `execution_id` which is linked to the actual execution, a unique ID is
/// created each time the resolver is called.
pub struct ResolverContext<'a> {
    /// Every declared resolver can have an ID, these ID can be used for
    /// memoization.
    pub resolver_id: Option<&'a str>,
    /// When a resolver is executed, it gains a Resolver unique ID for his
    /// execution, this ID is used for internal cache strategy
    pub execution_id: &'a Ulid,
    /// The current Type being resolved if we know it. It's the type linked to the resolver.
    pub ty: Option<&'a MetaType>,
    /// The current SelectionSet.
    pub selections: Option<&'a SelectionSet>,
    /// The current field being resolved if we know it.
    pub field: Option<&'a MetaField>,
}

impl<'a> ResolverContext<'a> {
    pub fn new(id: &'a Ulid) -> Self {
        Self {
            resolver_id: None,
            execution_id: id,
            ty: None,
            selections: None,
            field: None,
        }
    }

    pub fn with_resolver_id(mut self, id: Option<&'a str>) -> Self {
        self.resolver_id = id;
        self
    }

    pub fn with_ty(mut self, ty: Option<&'a MetaType>) -> Self {
        self.ty = ty;
        self
    }

    pub fn with_field(mut self, field: Option<&'a MetaField>) -> Self {
        self.field = field;
        self
    }

    pub fn with_selection_set(mut self, selections: Option<&'a SelectionSet>) -> Self {
        self.selections = selections;
        self
    }
}

#[derive(Debug, Hash, Clone)]
pub enum ResolvedPaginationDirection {
    Forward,
    Backward,
}

impl ResolvedPaginationDirection {
    pub fn from_paginated_cursor(cursor: &PaginatedCursor) -> Self {
        match cursor {
            PaginatedCursor::Forward { .. } => Self::Forward,
            PaginatedCursor::Backward { .. } => Self::Backward,
        }
    }
}

#[derive(Debug, Hash, Clone)]
pub struct ResolvedPaginationInfo {
    pub direction: ResolvedPaginationDirection,
    pub end_cursor: Option<PaginationCursor>,
    pub start_cursor: Option<PaginationCursor>,
    pub more_data: bool,
}

impl ResolvedPaginationInfo {
    pub fn new(direction: ResolvedPaginationDirection) -> Self {
        Self {
            direction,
            end_cursor: None,
            start_cursor: None,
            more_data: false,
        }
    }

    pub fn with_start(mut self, start_cursor: Option<PaginationCursor>) -> Self {
        self.start_cursor = start_cursor;
        self
    }

    pub fn with_end(mut self, end_cursor: Option<PaginationCursor>) -> Self {
        self.end_cursor = end_cursor;
        self
    }

    pub fn with_more_data(mut self, data: bool) -> Self {
        self.more_data = data;
        self
    }

    pub fn output(&self) -> serde_json::Value {
        let has_next_page = matches!(
            (&self.direction, self.more_data),
            (&ResolvedPaginationDirection::Forward, true)
        );

        let has_previous_page = matches!(
            (&self.direction, self.more_data),
            (&ResolvedPaginationDirection::Backward, true)
        );

        serde_json::json!({
            "has_next_page": has_next_page,
            "has_previous_page": has_previous_page,
            "start_cursor": self.start_cursor,
            "end_cursor": self.end_cursor,
        })
    }
}

/// ResolvedValue are values passed arround between resolvers, it contains the actual Resolver data
/// but will also contain other informations wich may be use later by custom resolvers, like for
/// example Pagination Details.
///
/// Cheap to Clone
#[derive(Debug, Derivative, Clone)]
#[derivative(Hash)]
pub struct ResolvedValue {
    /// Data Resolved by the current Resolver
    #[derivative(Hash = "ignore")]
    pub data_resolved: Arc<serde_json::Value>,
    /// Optional pagination data for Paginated Resolvers
    pub pagination: Option<ResolvedPaginationInfo>,
    /// Resolvers can set this value when resolving so the engine will know it's
    /// not usefull to continue iterating over the ResolverChain.
    pub early_return_null: bool,
}

impl ResolvedValue {
    pub fn new(value: Arc<serde_json::Value>) -> Self {
        Self {
            data_resolved: value,
            pagination: None,
            early_return_null: false,
        }
    }

    pub fn with_pagination(mut self, pagination: ResolvedPaginationInfo) -> Self {
        self.pagination = Some(pagination);
        self
    }

    pub fn with_early_return(mut self) -> Self {
        self.early_return_null = true;
        self
    }

    /// We can check from the schema definition if it's a node, if it is, we need to
    /// have a way to get it
    /// temp: Little hack here, we know that `ResolvedValue` are bound to have a format
    /// of:
    /// ```ignore
    /// {
    ///   "Node": {
    ///     "__sk": {
    ///       "S": "node_id"
    ///     }
    ///   }
    /// }
    /// ```
    /// We use that fact without checking it here.
    ///
    /// This have to be removed when we rework registry & dynaql to have a proper query
    /// planning.
    pub fn node_id<S: AsRef<str>>(&self, entity: S) -> Option<String> {
        self.data_resolved.get(entity.as_ref()).and_then(|x| {
            x.get("__sk")
                .and_then(|x| {
                    if let serde_json::Value::Object(value) = x {
                        Some(value)
                    } else {
                        None
                    }
                })
                .and_then(|x| x.get("S"))
                .and_then(|value| {
                    if let serde_json::Value::String(value) = value {
                        Some(value.clone())
                    } else {
                        None
                    }
                })
        })
    }

    pub fn is_early_returned(&self) -> bool {
        self.early_return_null
    }
}

#[async_trait::async_trait]
pub trait ResolverTrait: Sync {
    async fn resolve(
        &self,
        ctx: &Context<'_>,
        resolver_ctx: &ResolverContext<'_>,
        last_resolver_value: Option<&ResolvedValue>,
    ) -> Result<ResolvedValue, Error>;
}

#[async_trait::async_trait]
impl ResolverTrait for Resolver {
    /// The `[ResolverTrait]` should be a core element of the resolver chain.
    /// When you cross the ResolverChain, every Resolver Result is passed on the Children
    /// By Reference.
    ///
    /// WE MUST ENSURE EVERY VALUES ACCEDED BY THE RESOLVER COULD BE GETTED.
    /// Why? To ensure security.
    ///
    /// We resolver can only access the TRANSFORMED result from his resolver ancestor.
    async fn resolve(
        &self,
        ctx: &Context<'_>,
        resolver_ctx: &ResolverContext<'_>,
        last_resolver_value: Option<&ResolvedValue>,
    ) -> Result<ResolvedValue, Error> {
        match &self.r#type {
            ResolverType::DebugResolver(debug) => {
                debug.resolve(ctx, resolver_ctx, last_resolver_value).await
            }
            ResolverType::DynamoResolver(dynamodb) => {
                dynamodb
                    .resolve(ctx, resolver_ctx, last_resolver_value)
                    .await
            }
            ResolverType::DynamoMutationResolver(dynamodb) => {
                dynamodb
                    .resolve(ctx, resolver_ctx, last_resolver_value)
                    .await
            }
            ResolverType::ContextDataResolver(ctx_data) => {
                ctx_data
                    .resolve(ctx, resolver_ctx, last_resolver_value)
                    .await
            }
        }
    }
}

#[non_exhaustive]
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, Hash, PartialEq, Eq)]
pub enum ResolverType {
    DynamoResolver(DynamoResolver),
    DynamoMutationResolver(DynamoMutationResolver),
    ContextDataResolver(ContextDataResolver),
    DebugResolver(DebugResolver),
}

impl Constraint {
    /// Extracts a ConstraintID for this constraint from the corresponding field
    /// of a `*ByInput` type.
    ///
    /// If the constraint has one field we expect the value to just be a string.
    /// If the constraint has multiple it should be an Object of fieldName: value
    fn extract_id_from_by_input_field(
        &self,
        ty: &str,
        value: &ConstValue,
    ) -> Option<ConstraintID<'static>> {
        let fields = self.fields();
        if fields.len() == 1 {
            return Some(ConstraintID::new(
                ty.to_string(),
                vec![(fields[0].clone(), value.clone().into_json().ok()?)],
            ));
        }

        let ConstValue::Object(by_fields) = value else {
            return None;
        };

        let constraint_fields = fields
            .into_iter()
            .map(|field| {
                Some((
                    field.clone(),
                    by_fields.get(&Name::new(field))?.clone().into_json().ok()?,
                ))
            })
            .collect::<Option<Vec<_>>>()?;

        Some(ConstraintID::new(ty.to_string(), constraint_fields))
    }
}
