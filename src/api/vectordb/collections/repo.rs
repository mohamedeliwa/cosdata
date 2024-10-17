use std::sync::Arc;

use crate::{
    api_service::{init_inverted_index, init_vector_store},
    app_context::AppContext,
    indexes::inverted_index::InvertedIndex,
    models::{collection::Collection, types::DenseIndex},
};

use super::{
    dtos::{CreateCollectionDto, FindCollectionDto, GetCollectionsDto},
    error::CollectionsError,
};

pub(crate) async fn create_collection(
    ctx: Arc<AppContext>,
    CreateCollectionDto {
        name,
        description,
        config,
        dense_vector,
        metadata_schema,
        sparse_vector,
    }: CreateCollectionDto,
) -> Result<Collection, CollectionsError> {
    let env = &ctx.ain_env.persist;
    let collections_db = &ctx.ain_env.collections_map.lmdb_collections_db;

    let collection = Collection::new(
        name,
        description,
        dense_vector,
        sparse_vector,
        metadata_schema,
        config,
    );
    // persisting collection after creation
    // note that CollectionsMap has similar functionality to
    // persist collections on the disk
    // TODO rework CollectionsMap
    let _ = collection
        .persist(env, collections_db.clone())
        .map_err(|e| CollectionsError::WaCustomError(e));
    Ok(collection)
}

/// creates a dense_index for a collection
pub(crate) async fn create_dense_index(
    ctx: Arc<AppContext>,
    name: &str,
    size: usize,
    lower_bound: Option<f32>,
    upper_bound: Option<f32>,
    max_cache_level: u8,
) -> Result<Arc<DenseIndex>, CollectionsError> {
    // Call init_vector_store using web::block
    let result = init_vector_store(
        ctx,
        name.into(),
        size,
        lower_bound,
        upper_bound,
        max_cache_level,
    )
    .await;
    result.map_err(|e| CollectionsError::FailedToCreateCollection(e.to_string()))
}

pub(crate) async fn create_inverted_index(
    ctx: Arc<AppContext>,
    name: &str,
    description: &Option<String>,
    auto_create_index: bool,
    metadata_schema: &Option<String>,
    max_vectors: Option<i32>,
    replication_factor: Option<i32>,
) -> Result<Arc<InvertedIndex>, CollectionsError> {
    let result = init_inverted_index(
        ctx,
        name.into(),
        description.clone(),
        auto_create_index,
        metadata_schema.clone(),
        max_vectors,
        replication_factor,
    )
    .await;
    result.map_err(|e| CollectionsError::FailedToCreateCollection(e.to_string()))
}

/// gets a dense_index for a collection
pub(crate) async fn get_dense_index(
    ctx: Arc<AppContext>,
    _get_collections_dto: GetCollectionsDto,
) -> Result<Vec<FindCollectionDto>, CollectionsError> {
    let dense_index = ctx
        .ain_env
        .collections_map
        .iter()
        .map(|v| FindCollectionDto {
            id: v.database_name.clone(),
            dimensions: v.quant_dim,
            vector_db_name: v.database_name.clone(),
        })
        .collect();
    Ok(dense_index)
}

/// gets a dense index for a collection by name
pub(crate) async fn get_dense_index_by_name(
    ctx: Arc<AppContext>,
    name: &str,
) -> Result<Arc<DenseIndex>, CollectionsError> {
    // Try to get the dense_index from the environment
    let dense_index = match ctx.ain_env.collections_map.get(name) {
        Some(index) => index.clone(),
        None => {
            // dense index not found, return an error response
            return Err(CollectionsError::NotFound);
        }
    };
    Ok(dense_index)
}

/// deletes a dense index of a collection by name
pub(crate) async fn delete_dense_index_by_name(
    ctx: Arc<AppContext>,
    name: &str,
) -> Result<Arc<DenseIndex>, CollectionsError> {
    // Try to get the dense index from the environment
    let result = ctx
        .ain_env
        .collections_map
        .remove(name)
        .map_err(CollectionsError::WaCustomError)?;
    match result {
        Some((_, index)) => Ok(index),
        None => {
            // dense index not found, return an error response
            return Err(CollectionsError::NotFound);
        }
    }
}
