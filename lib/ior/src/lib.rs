use serde::{Deserialize, Serialize};

use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum IOr<L, R> {
    Left(L),
    Right(R),
    Both(L, R),
}
impl<L, R> IOr<L, R> {
    pub fn from(left: Option<L>, right: Option<R>) -> Option<IOr<L, R>> {
        match (left, right) {
            (Some(l), None) => Some(IOr::Left(l)),
            (None, Some(r)) => Some(IOr::Right(r)),
            (Some(l), Some(r)) => Some(IOr::Both(l, r)),
            (None, None) => None,
        }
    }
    pub fn left(&self) -> Option<&L> {
        match self {
            IOr::Left(val) | IOr::Both(val, _) => Some(val),
            _ => None,
        }
    }

    pub fn right(&self) -> Option<&R> {
        match self {
            IOr::Right(val) | IOr::Both(_, val) => Some(val),
            _ => None,
        }
    }
}

pub fn union<'a, T: Clone + Eq + std::hash::Hash, L, R>(
    left: &'a HashMap<T, L>,
    right: &'a HashMap<T, R>,
) -> Vec<(T, IOr<&'a L, &'a R>)> {
    let mut keys_union: HashMap<T, ()> = HashMap::new();
    left.keys().for_each(|el| {
        keys_union.insert(el.clone(), ());
    });
    right.keys().for_each(|el| {
        keys_union.insert(el.clone(), ());
    });
    keys_union
        .keys()
        .into_iter()
        .map(|key| {
            let ior = IOr::from(left.get(key), right.get(key));
			let ior = ior.expect("Unexpected error upon set union: an element in the set union does not belong in either of the two original sets");
            (key.clone(), ior)
        })
        .collect()
}

pub fn intersect<'a, T: Clone + Eq + std::hash::Hash, S>(
    left: &'a HashMap<T, S>,
    right: &'a HashMap<T, S>,
) -> Vec<(T, (&'a S, &'a S))> {
    let mut intersection: Vec<(T, (&'a S, &'a S))> = Vec::new();

    for (name, left_child) in left {
        if let Some(right_child) = right.get(name) {
            intersection.push((name.clone(), (left_child, right_child)));
        }
    }

    intersection
}
