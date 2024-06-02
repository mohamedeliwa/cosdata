use dashmap::DashMap;
use futures::future::{join_all, BoxFuture, FutureExt};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::task;
// type NumericValue = Vec<Vec<i32>>; // Two-dimensional vector

// type VectorHash = String; // Assuming VectorHash is a String, replace with appropriate type if different

// #[derive(Debug, Clone, PartialEq)]
// pub struct VectorTreeNode {
//     vector_list: NumericValue,         // Two-dimensional vector
//     neighbors: Vec<(VectorHash, f64)>, // neighbor, cosine distance
// }

// #[derive(Debug, Clone)]
// pub struct VectorStore {
//     database_name: String,
//     root_vec: (VectorHash, NumericValue), // Two-dimensional vector
//     cache: HashMap<(i8, VectorHash), (Option<VectorTreeNode>, Arc<Mutex<()>>)>, // (level, vector), map prefixnorm hash
//     max_cache_level: i8,
// }

// #[derive(Debug, Clone)]
// pub struct VectorEmbedding {
//     raw_vec: NumericValue, // Two-dimensional vector
//     hash_vec: VectorHash,
// }

pub struct CosResult {
    pub dotprod: i32,
    pub premag_a: i32,
    pub premag_b: i32,
}

// Function to convert a sequence of bits to an integer value
fn bits_to_integer(bits: &[i32], size: usize) -> u32 {
    let mut result: u32 = 0;
    for i in 0..size {
        result = (result << 1) | (bits[i] as u32);
    }
    result
}

fn x_function(value: u32) -> i32 {
    match value {
        0 => 0,
        1 => 1,
        2 => 1,
        3 => 2,
        4 => 1,
        5 => 2,
        6 => 2,
        7 => 3,
        8 => 1,
        9 => 2,
        10 => 2,
        11 => 3,
        12 => 2,
        13 => 3,
        14 => 3,
        15 => 4,
        _ => -1, // Invalid input
    }
}

fn shift_and_accumulate(value: u32) -> i32 {
    let mut result: i32 = 0;
    result += x_function(15 & (value >> 0));
    result += x_function(15 & (value >> 4));
    result += x_function(15 & (value >> 8));
    result += x_function(15 & (value >> 12));
    result
}

fn dot_product(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(&x, &y)| x * y).sum()
}

fn magnitude(vec: &[f32]) -> f32 {
    vec.iter().map(|&x| x * x).sum::<f32>().sqrt()
}

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    dot_product(a, b) / (magnitude(a) * magnitude(b))
}

pub fn compute_cosine_similarity(a: &[i32], b: &[i32], size: usize) -> CosResult {
    let value1 = bits_to_integer(a, size);
    let value2 = bits_to_integer(b, size);

    let dotprod = shift_and_accumulate(value1 & value2);
    let premag_a = shift_and_accumulate(value1);
    let premag_b = shift_and_accumulate(value2);

    CosResult {
        dotprod,
        premag_a,
        premag_b,
    }
}

pub fn cosine_coalesce(x: &[Vec<i32>], y: &[Vec<i32>]) -> f64 {
    if x.len() != y.len() {
        panic!("Input vectors must have the same length");
    }

    let mut results = Vec::new();

    for (sub_x, sub_y) in x.iter().zip(y.iter()) {
        let cs = compute_cosine_similarity(sub_x, sub_y, 16);
        results.push(cs);
    }

    let summed = sum_components(&results);

    f64::from(summed.dotprod)
        / (f64::sqrt(f64::from(summed.premag_a)) * f64::sqrt(f64::from(summed.premag_b)))
}

pub fn cosine_sim_unsigned(x: &Vec<u32>, y: &Vec<u32>) -> f64 {
    let mut acc = CosResult {
        dotprod: 0,
        premag_a: 0,
        premag_b: 0,
    };
    for (value1, value2) in x.iter().zip(y.iter()) {
        let dotprod = shift_and_accumulate(value1 & value2);
        let premag_a = shift_and_accumulate(*value1);
        let premag_b = shift_and_accumulate(*value2);

        acc.dotprod += dotprod;
        acc.premag_a += premag_a;
        acc.premag_b += premag_b;
    }

    f64::from(acc.dotprod)
        / (f64::sqrt(f64::from(acc.premag_a)) * f64::sqrt(f64::from(acc.premag_b)))
}

fn sum_components(results: &[CosResult]) -> CosResult {
    let mut acc = CosResult {
        dotprod: 0,
        premag_a: 0,
        premag_b: 0,
    };

    for res in results {
        acc.dotprod += res.dotprod;
        acc.premag_a += res.premag_a;
        acc.premag_b += res.premag_b;
    }

    acc
}

fn to_float_flag(x: f32) -> i32 {
    if x >= 0.0 {
        1
    } else {
        0
    }
}

pub fn floats_to_bits(floats: &[f32]) -> Vec<u32> {
    let mut result = vec![0; (floats.len() + 31) / 32];

    for (i, &f) in floats.iter().enumerate() {
        if f >= 0.0 {
            result[i / 32] |= 1 << (i % 32);
        }
    }

    result
}

pub fn quantize(fins: &[f32]) -> Vec<Vec<i32>> {
    let mut quantized = Vec::with_capacity((fins.len() + 15) / 16);
    let mut chunk = Vec::with_capacity(16);

    for &f in fins {
        chunk.push(to_float_flag(f));
        if chunk.len() == 16 {
            quantized.push(chunk.clone());
            chunk.clear();
        }
    }

    if !chunk.is_empty() {
        quantized.push(chunk);
    }

    quantized
}

// #[derive(Debug, Clone)]
// enum NumericValue {
//     U32(Vec<u32>),
//     F32(Vec<f32>),
// }
type NumericValue = Vec<f32>;
type VectorHash = Vec<u8>;

type CacheType = DashMap<(i8, VectorHash), Option<(VectorTreeNode, Arc<()>)>>;

pub struct VectorStore {
    cache: Arc<CacheType>,
    max_cache_level: i8,
    database_name: String,
    root_vec: (VectorHash, NumericValue),
}

#[derive(Debug, Clone)]
pub struct VectorEmbedding {
    raw_vec: NumericValue,
    hash_vec: VectorHash,
}

#[derive(Debug, Clone)]
pub struct VectorTreeNode {
    vector_list: NumericValue,
    neighbors: Vec<(VectorHash, f32)>,
}

async fn insert_embedding(
    vec_store: Arc<VectorStore>,
    vector_emb: VectorEmbedding,
    cur_entry: VectorHash,
    cur_level: i8,
    max_insert_level: i8,
) {
    if cur_level == -1 {
        return;
    }

    if let Some(res) = vec_store.cache.clone().get(&(cur_level, cur_entry.clone())) {
        let fvec = vector_emb.raw_vec.clone();

        if let Some((vthm, mv)) = res.value() {
            //let Some((vthm,  mv))= x.value();
            let vtm = vthm.clone();
            let skipm = Arc::new(DashMap::new());
            let z = traverse_find_nearest(
                vec_store.clone(),
                mv.clone(),
                vtm.clone(),
                fvec.clone(),
                vector_emb.hash_vec.clone(),
                0,
                skipm.clone(),
                cur_level,
            )
            .await;

            let y = cosine_similarity(&fvec, &vtm.vector_list);
            let z = if z.is_empty() {
                vec![(cur_entry.clone(), y)]
            } else {
                z
            };
            let vec_store_clone = vec_store.clone();
            let vector_emb_clone = vector_emb.clone();
            let z_clone = z.clone();

            if cur_level <= max_insert_level {
                let recursive_call = Box::pin(async move {
                    insert_embedding(
                        vec_store.clone(),
                        vector_emb.clone(),
                        z[0].0.clone(),
                        cur_level - 1,
                        max_insert_level,
                    )
                    .await;
                });
                recursive_call.await;
                insert_node_create_edges(
                    vec_store_clone.clone(),
                    fvec,
                    vector_emb_clone.hash_vec.clone(),
                    z_clone,
                    cur_level,
                )
                .await;
            } else {
                let recursive_call = Box::pin(async move {
                    insert_embedding(
                        vec_store.clone(),
                        vector_emb.clone(),
                        z[0].0.clone(),
                        cur_level - 1,
                        max_insert_level,
                    )
                    .await;
                });
                recursive_call.await;
            }
        } else {
            if cur_level > vec_store.max_cache_level {
                let xvtm = get_vector_from_db(&vec_store.database_name, cur_entry.clone()).await;
                if let Some(vtm) = xvtm {
                    let skipm = Arc::new(DashMap::new());
                    let z = traverse_find_nearest(
                        vec_store.clone(),
                        Arc::new(()),
                        vtm.clone(),
                        fvec.clone(),
                        vector_emb.hash_vec.clone(),
                        0,
                        skipm.clone(),
                        cur_level,
                    )
                    .await;
                    insert_node_create_edges(
                        vec_store.clone(),
                        fvec,
                        vector_emb.hash_vec.clone(),
                        z,
                        cur_level,
                    )
                    .await;
                } else {
                    eprintln!(
                        "Error case, should have been found: {} {:?}",
                        cur_level, xvtm
                    );
                }
            } else {
                eprintln!("Error case, should not happen: {} ", cur_level);
            }
        }
    } else {
        eprintln!("Error case, should not happen: {}", cur_level);
    }
}

async fn insert_node_create_edges(
    vec_store: Arc<VectorStore>,
    fvec: NumericValue,
    hs: VectorHash,
    nbs: Vec<(VectorHash, f32)>,
    cur_level: i8,
) {
    let em = Arc::new(());
    let nv = VectorTreeNode {
        vector_list: fvec.clone(),
        neighbors: nbs.clone(),
    };

    vec_store
        .cache
        .insert((cur_level, hs.clone()), Some((nv, em.clone())));

    let tasks: Vec<_> =
        nbs.into_iter()
            .map(|(nb, cs)| {
                let vec_store = vec_store.clone();
                let hs = hs.clone();
                let cur_level = cur_level;
                task::spawn(async move {
                    vec_store
                        .cache
                        .alter(&(cur_level, nb.clone()), |_, ref existing_value| {
                            match existing_value {
                                Some(res) => {
                                    let ((vthm, mv)) = res;
                                    let mut ng = vthm.neighbors.clone();
                                    ng.push((hs.clone(), cs));
                                    ng.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
                                    ng.dedup_by(|a, b| a.0 == b.0);
                                    let ng = ng.into_iter().take(2).collect::<Vec<_>>();

                                    let nv = VectorTreeNode {
                                        vector_list: vthm.vector_list.clone(),
                                        neighbors: ng,
                                    };
                                    return Some((nv, mv.clone()));
                                }
                                None => {
                                    return existing_value.clone();
                                }
                            }
                        });
                })
            })
            .collect();

    join_all(tasks).await;
}

async fn traverse_find_nearest(
    vec_store: Arc<VectorStore>,
    mv: Arc<()>,
    vtm: VectorTreeNode,
    fvec: NumericValue,
    hs: VectorHash,
    hops: i8,
    skipm: Arc<DashMap<VectorHash, ()>>,
    cur_level: i8,
) -> Vec<(VectorHash, f32)> {
    traverse_find_nearest_inner(vec_store, mv, vtm, fvec, hs, hops, skipm, cur_level).await
}

fn traverse_find_nearest_inner(
    vec_store: Arc<VectorStore>,
    mv: Arc<()>,
    vtm: VectorTreeNode,
    fvec: NumericValue,
    hs: VectorHash,
    hops: i8,
    skipm: Arc<DashMap<VectorHash, ()>>,
    cur_level: i8,
) -> BoxFuture<'static, Vec<(VectorHash, f32)>> {
    async move {
        let tasks: Vec<_> = vtm
            .neighbors
            .clone()
            .into_iter()
            .filter(|(nb, _)| *nb != hs)
            .map(|(nb, _)| {
                let skipm = skipm.clone();
                let vec_store = vec_store.clone();
                let fvec = fvec.clone();
                let hs = hs.clone();
                task::spawn(async move {
                    if skipm.contains_key(&nb) {
                        vec![]
                    } else {
                        skipm.insert(nb.clone(), ());

                        if let Some(res) = vec_store.cache.get(&(cur_level, nb.clone())) {
                            if let Some((vthm, mv)) = res.value() {
                                let cs = cosine_similarity(&fvec, &vthm.vector_list);
                                if hops < 4 {
                                    let mut z = traverse_find_nearest_inner(
                                        vec_store.clone(),
                                        mv.clone(),
                                        vthm.clone(),
                                        fvec.clone(),
                                        hs.clone(),
                                        hops + 1,
                                        skipm.clone(),
                                        cur_level,
                                    )
                                    .await;
                                    z.push((nb.clone(), cs));
                                    z
                                } else {
                                    vec![(nb.clone(), cs)]
                                }
                            } else {
                                eprintln!(
                                    "Error case, should not happen: {} key {:?}",
                                    cur_level,
                                    (cur_level, nb)
                                );
                                vec![]
                            }
                        } else {
                            eprintln!(
                                "Error case, should not happen: {} key {:?}",
                                cur_level,
                                (cur_level, nb)
                            );
                            vec![]
                        }
                    }
                })
            })
            .collect();

        let results: Vec<Result<Vec<(VectorHash, f32)>, task::JoinError>> = join_all(tasks).await;
        let mut nn: Vec<_> = results
            .into_iter()
            .filter_map(Result::ok) // Filter out the errors
            .flat_map(|inner_vec| inner_vec) // Flatten the inner vectors
            .collect();
        nn.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        let mut seen = HashSet::new();
        nn.retain(|(vec_u8, _)| seen.insert(vec_u8.clone()));
        nn.into_iter().take(2).collect()
    }
    .boxed()
}

async fn get_vector_from_db(db_name: &str, entry: VectorHash) -> Option<VectorTreeNode> {
    // Your implementation to get vector from the database
    unimplemented!()
}
