use mpdelta_core::common::mixed_fraction::MixedFraction;
use mpdelta_core::component::instance::ComponentInstanceHandle;
use mpdelta_core::component::link::MarkerLinkHandle;
use mpdelta_core::component::marker_pin::MarkerPinHandle;
use mpdelta_core::component::parameter::ParameterValueType;
use mpdelta_core::ptr::StaticPointerOwned;
use mpdelta_core::time::TimelineTime;
use nalgebra::{Const, CsMatrix, DVector, Dyn, OMatrix, VecStorage, QR};
use qcell::TCellOwner;
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::iter;
use thiserror::Error;

#[derive(Clone, Error)]
pub enum CollectCachedTimeError<K, T>
where
    K: 'static,
    T: ParameterValueType,
{
    #[error("invalid component instance {0:?}")]
    InvalidComponentInstance(ComponentInstanceHandle<K, T>),
    #[error("invalid marker link {0:?}")]
    InvalidMarkerLink(MarkerLinkHandle<K>),
    #[error("invalid marker {0:?}")]
    InvalidMarker(MarkerPinHandle<K>),
    #[error("invalid link graph")]
    InvalidLinkGraph,
}

impl<K, T> Debug for CollectCachedTimeError<K, T>
where
    K: 'static,
    T: ParameterValueType,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            CollectCachedTimeError::InvalidComponentInstance(value) => f.debug_tuple("InvalidComponentInstance").field(value).finish(),
            CollectCachedTimeError::InvalidMarkerLink(value) => f.debug_tuple("InvalidMarkerLink").field(value).finish(),
            CollectCachedTimeError::InvalidMarker(value) => f.debug_tuple("InvalidMarker").field(value).finish(),
            CollectCachedTimeError::InvalidLinkGraph => f.write_str("InvalidLinkGraph"),
        }
    }
}

pub fn collect_cached_time<K, T>(components: &[impl AsRef<ComponentInstanceHandle<K, T>>], links: &[impl AsRef<MarkerLinkHandle<K>>], left: &MarkerPinHandle<K>, right: &MarkerPinHandle<K>, key: &TCellOwner<K>) -> Result<(), CollectCachedTimeError<K, T>>
where
    T: ParameterValueType,
{
    let components = components
        .iter()
        .map(|component| component.as_ref().upgrade().ok_or_else(|| CollectCachedTimeError::InvalidComponentInstance(component.as_ref().clone())))
        .collect::<Result<Vec<_>, _>>()?;
    let links = links.iter().map(|link| link.as_ref().upgrade().ok_or_else(|| CollectCachedTimeError::InvalidMarkerLink(link.as_ref().clone()))).collect::<Result<Vec<_>, _>>()?;

    let Some(left_ref) = left.upgrade() else {
        return Err(CollectCachedTimeError::InvalidMarker(left.clone()));
    };
    let Some(right_ref) = right.upgrade() else {
        return Err(CollectCachedTimeError::InvalidMarker(right.clone()));
    };

    let pin_map = [left, right]
        .into_iter()
        .chain(
            components
                .iter()
                .flat_map(|component| iter::once(component.ro(key).marker_left()).chain(component.ro(key).markers()).chain(iter::once(component.ro(key).marker_right())))
                .map(StaticPointerOwned::reference),
        )
        .enumerate()
        .map(|(i, p)| (p, i))
        .collect::<HashMap<_, _>>();
    let mut union_find = UnionFind::new(pin_map.len());
    links.iter().try_for_each(|link| {
        let link = link.ro(key);
        let from = pin_map[link.from()];
        let to = pin_map[link.to()];
        if union_find.get_root(from) == union_find.get_root(to) {
            return Err(CollectCachedTimeError::InvalidLinkGraph);
        }
        union_find.union(from, to);
        Ok(())
    })?;

    let mut connected_subgraph = Vec::new();
    let mut pin_data = Vec::new();
    let mut is_locked = vec![false; union_find.len()];
    for (i, is_locked) in is_locked.iter_mut().enumerate() {
        let root = union_find.get_root(i);
        if root == i {
            pin_data.push(PinData::new(connected_subgraph.len()));
            connected_subgraph.push(SubGraphData::new());
            *is_locked = true;
        } else {
            let subtree_index = pin_data[root].subtree_index;
            connected_subgraph[subtree_index].connected_node_count += 1;
            pin_data.push(PinData::new(subtree_index));
        }
    }
    let mut unproceed_links = links.iter().map(|link| link.ro(key)).collect::<Vec<_>>();
    while !unproceed_links.is_empty() {
        let prev_len = unproceed_links.len();
        unproceed_links.retain(|link| {
            let from_pin_id = pin_map[link.from()];
            let to_pin_id = pin_map[link.to()];
            match (is_locked[from_pin_id], is_locked[to_pin_id]) {
                (false, false) => true,
                (true, false) => {
                    pin_data[to_pin_id].time_from_base = pin_data[from_pin_id].time_from_base + link.len();
                    is_locked[to_pin_id] = true;
                    false
                }
                (false, true) => {
                    pin_data[from_pin_id].time_from_base = pin_data[to_pin_id].time_from_base - link.len();
                    is_locked[from_pin_id] = true;
                    false
                }
                (true, true) => unreachable!(),
            }
        });
        assert!(unproceed_links.len() < prev_len);
    }

    if connected_subgraph.len() > 1 {
        let mut pin_data_slice = &pin_data[2..];
        let mut pin_handle = Vec::new();
        let mut used = vec![false; connected_subgraph.len()];
        let mut row_indices = vec![0];
        let mut column_indices = vec![0];
        let mut values = vec![1.];
        let mut time_diff = vec![0.];
        let mut assume_pin_difference = |pin1: &PinData, pin2: &PinData, diff: MixedFraction| {
            let diff = (diff + pin1.time_from_base.value() - pin2.time_from_base.value()).into_f64();
            column_indices.extend([pin2.subtree_index, pin1.subtree_index]);
            row_indices.extend([time_diff.len(); 2]);
            values.extend([1., -1.]);
            time_diff.push(diff);
        };
        for component in components.iter() {
            let component = component.ro(key);
            pin_handle.clear();
            pin_handle.extend(iter::once(component.marker_left()).chain(component.markers()).chain(iter::once(component.marker_right())));
            let (pins, tail) = pin_data_slice.split_at(component.markers().len() + 2);
            pin_data_slice = tail;
            used.fill(false);
            for (i, pin) in pins.iter().enumerate() {
                let current_subtree_index = pin.subtree_index;
                if used[current_subtree_index] || pin_handle[i].ro(key).locked_component_time().is_none() {
                    continue;
                }
                used[current_subtree_index] = true;
                let base_time = pin_handle[i].ro(key).locked_component_time().unwrap();
                if let Some((right_index, right_pin)) = pins[i..].iter().enumerate().skip(1).find_map(|(j, p)| (p.subtree_index == current_subtree_index && pin_handle[j].ro(key).locked_component_time().is_some()).then_some((j + i, p))) {
                    let mut right = pin_handle[right_index].ro(key).locked_component_time().unwrap();
                    let time_ratio = (right_pin.time_from_base - pin.time_from_base).value() / (right.value() - base_time.value());
                    let time_ratio = if time_ratio.signum() < 0 { -time_ratio } else { time_ratio };
                    for (j, p) in pins[..i].iter().enumerate() {
                        if current_subtree_index == p.subtree_index {
                            continue;
                        }
                        let Some(t) = pin_handle[j].ro(key).locked_component_time() else {
                            continue;
                        };
                        let diff = base_time.value() - t.value();
                        let diff = if diff.signum() < 0 { -diff } else { diff };
                        assume_pin_difference(pin, p, diff * time_ratio);
                    }
                    let mut time_ratio = time_ratio;
                    let mut right_index = right_index;
                    let mut right_pin = right_pin;
                    let mut pin = pin;
                    let mut i = i;
                    let mut base_time = base_time;
                    while i < pins.len() {
                        for (j, p) in pins[i..right_index].iter().enumerate().skip(1).map(|(j, p)| (j + i, p)) {
                            if pin.subtree_index == p.subtree_index {
                                continue;
                            }
                            let Some(t) = pin_handle[j].ro(key).locked_component_time() else {
                                continue;
                            };
                            let diff = base_time.value() - t.value();
                            let diff = if diff.signum() < 0 { -diff } else { diff };
                            assume_pin_difference(pin, p, diff * time_ratio);
                        }
                        i = right_index;
                        pin = right_pin;
                        if i == pins.len() {
                            break;
                        }
                        base_time = pin_handle[i].ro(key).locked_component_time().unwrap();
                        if let Some((ri, rp)) = pins[i..].iter().enumerate().skip(1).find_map(|(j, p)| (p.subtree_index == current_subtree_index && pin_handle[j].ro(key).locked_component_time().is_some()).then_some((j + i, p))) {
                            right_index = ri;
                            right_pin = rp;
                            right = pin_handle[right_index].ro(key).locked_component_time().unwrap();
                            time_ratio = {
                                let time_ratio = (right_pin.time_from_base - pin.time_from_base).value() / (right.value() - base_time.value());
                                if time_ratio.signum() < 0 {
                                    -time_ratio
                                } else {
                                    time_ratio
                                }
                            };
                        } else {
                            right_index = pins.len();
                        };
                    }
                } else {
                    let mut used = vec![false; connected_subgraph.len()];
                    for (j, p) in pins[..i].iter().enumerate() {
                        if current_subtree_index == p.subtree_index || used[p.subtree_index] {
                            continue;
                        }
                        let Some(t) = pin_handle[j].ro(key).locked_component_time() else {
                            continue;
                        };
                        used[p.subtree_index] = true;
                        let diff = base_time.value() - t.value();
                        let diff = if (j < i) == (diff.signum() < 0) { diff } else { -diff };
                        assume_pin_difference(pin, p, diff);
                    }
                    used.fill(false);
                    for (j, p) in pins[i..].iter().enumerate().skip(1).map(|(j, p)| (j + i, p)) {
                        if current_subtree_index == p.subtree_index || used[p.subtree_index] {
                            continue;
                        }
                        let Some(t) = pin_handle[j].ro(key).locked_component_time() else {
                            continue;
                        };
                        used[p.subtree_index] = true;
                        let diff = base_time.value() - t.value();
                        let diff = if (j < i) == (diff.signum() < 0) { diff } else { -diff };
                        assume_pin_difference(pin, p, diff);
                    }
                }
            }
        }
        if connected_subgraph[pin_data[1].subtree_index].connected_node_count == 1 {
            column_indices.push(pin_data[1].subtree_index);
            row_indices.push(time_diff.len());
            values.push(1.);
            time_diff.push(right_ref.ro(key).locked_component_time().map_or(10., |t| t.value().into_f64()));
        }
        let a = CsMatrix::from_triplet(time_diff.len(), connected_subgraph.len(), &row_indices, &column_indices, &values);
        let b = DVector::from_data(VecStorage::new(Dyn(time_diff.len()), Const::<1>, time_diff));

        let a_transpose = a.transpose();
        let mut right = OMatrix::from(&a_transpose * &CsMatrix::from(b));
        let solve_succeed = QR::new(OMatrix::from(&a_transpose * &a)).solve_mut(&mut right);
        if !solve_succeed {
            return Err(CollectCachedTimeError::InvalidLinkGraph);
        }
        connected_subgraph.iter_mut().zip(right.iter().copied()).for_each(|(subgraph, time)| {
            subgraph.base_time = TimelineTime::new(MixedFraction::from_f64(time));
        });
    }

    [left_ref, right_ref].into_iter().zip(&pin_data[..2]).for_each(|(pin_handle, pin)| {
        pin_handle.ro(key).cache_timeline_time(connected_subgraph[pin.subtree_index].base_time + pin.time_from_base);
    });
    components
        .iter()
        .flat_map(|component| iter::once(component.ro(key).marker_left()).chain(component.ro(key).markers()).chain(iter::once(component.ro(key).marker_right())))
        .zip(&pin_data[2..])
        .for_each(|(pin_handle, pin)| pin_handle.ro(key).cache_timeline_time(connected_subgraph[pin.subtree_index].base_time + pin.time_from_base));
    Ok(())
}

#[derive(Debug)]
struct SubGraphData {
    base_time: TimelineTime,
    connected_node_count: usize,
}

impl SubGraphData {
    fn new() -> SubGraphData {
        SubGraphData { base_time: TimelineTime::ZERO, connected_node_count: 1 }
    }
}

#[derive(Debug)]
struct PinData {
    subtree_index: usize,
    time_from_base: TimelineTime,
}

impl PinData {
    fn new(subgraph: usize) -> PinData {
        PinData {
            subtree_index: subgraph,
            time_from_base: TimelineTime::ZERO,
        }
    }
}

struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> UnionFind {
        UnionFind { parent: (0..n).collect() }
    }

    fn len(&self) -> usize {
        self.parent.len()
    }

    fn get_root(&mut self, x: usize) -> usize {
        let p = self.parent[x];
        if p == x {
            x
        } else {
            let r = self.get_root(p);
            self.parent[x] = r;
            r
        }
    }

    fn union(&mut self, x: usize, y: usize) {
        let x = self.get_root(x);
        let y = self.get_root(y);
        let (x, y) = (x.max(y), x.min(y));
        self.parent[x] = y;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mpdelta_core_test_util::{assert_eq_root_component_class, marker, root_component_class};
    use std::sync::Arc;
    use tokio::sync::RwLock;

    struct T;

    impl ParameterValueType for T {
        type Image = ();
        type Audio = ();
        type Binary = ();
        type String = ();
        type Integer = ();
        type RealNumber = ();
        type Boolean = ();
        type Dictionary = ();
        type Array = ();
        type ComponentClass = ();
    }

    #[tokio::test]
    async fn test_collect_cached_time() {
        struct K;
        let key = Arc::new(RwLock::new(TCellOwner::<K>::new()));
        macro_rules! key {
            () => {
                *key.read().await
            };
        }
        root_component_class! {
            custom_differential = collect_cached_time;
            differential_calcurated = <K, T> key!();
            left: left,
            right: right,
            components: [
                { markers: [marker!(locked: 0) => l1, marker!() => r1] },
            ],
            links: [
                left = 1 => l1; link1,
                l1 = 1 => r1,
            ],
        }
        root_component_class! {
            no_differential;
            expect = <K, T> key!();
            left: left,
            right: right,
            components: [
                { markers: [marker!(cached: 1, locked: 0) => l1, marker!(cached: 2) => r1] },
            ],
            links: [
                left = 1 => l1; link1,
                l1 = 1 => r1,
            ],
        }
        assert_eq_root_component_class(&differential_calcurated, &expect, &key!()).await;

        root_component_class! {
            custom_differential = collect_cached_time;
            differential_calcurated = <K, T> key!();
            left: left,
            right: right,
            components: [
                { markers: [marker!(locked: 0) => l1, marker!(locked: 0.5) => m1, marker!(locked: 1) => r1] },
                { markers: [marker!(locked: 0) => l2, marker!(locked: 2) => r2] },
            ],
            links: [
                left = 1 => l1; link1,
                l1 = 1 => r1,
                m1 = 0 => l2,
            ],
        }
        root_component_class! {
            no_differential;
            expect = <K, T> key!();
            left: left,
            right: right,
            components: [
                { markers: [marker!(cached: 1, locked: 0) => l1, marker!(cached: 1.5, locked: 0.5) => m1, marker!(cached: 2, locked: 1) => r1] },
                { markers: [marker!(cached: 1.5, locked: 0) => l2, marker!(cached: 3.5, locked: 2) => r2] },
            ],
            links: [
                left = 1 => l1; link1,
                l1 = 1 => r1,
                m1 = 0 => l2,
            ],
        }
        assert_eq_root_component_class(&differential_calcurated, &expect, &key!()).await;

        root_component_class! {
            custom_differential = collect_cached_time;
            differential_calcurated = <K, T> key!();
            left: left,
            right: right,
            components: [
                { markers: [marker!(locked: 0) => l1, marker!(locked: 1) => m1, marker!(locked: 2) => r1] },
                { markers: [marker!(locked: 0) => l2, marker!(locked: 2) => r2] },
            ],
            links: [
                left = 1 => l1; link1,
                l1 = 1 => r1,
                m1 = 0 => l2,
            ],
        }
        root_component_class! {
            no_differential;
            expect = <K, T> key!();
            left: left,
            right: right,
            components: [
                { markers: [marker!(cached: 1, locked: 0) => l1, marker!(cached: 1.5, locked: 1) => m1, marker!(cached: 2, locked: 2) => r1] },
                { markers: [marker!(cached: 1.5, locked: 0) => l2, marker!(cached: 3.5, locked: 2) => r2] },
            ],
            links: [
                left = 1 => l1; link1,
                l1 = 1 => r1,
                m1 = 0 => l2,
            ],
        }
        assert_eq_root_component_class(&differential_calcurated, &expect, &key!()).await;

        root_component_class! {
            custom_differential = collect_cached_time;
            differential_calcurated = <K, T> key!();
            left: left,
            right: right,
            components: [
                { markers: [marker!(locked: 0) => l1, marker!(locked: 1) => m1, marker!(locked: 2) => m2, marker!(locked: 4) => r1] },
                { markers: [marker!(locked: 0) => l2, marker!(locked: 2) => r2] },
            ],
            links: [
                left = 1 => l1,
                l1 = 1 => m2,
                m1 = 0 => l2,
                m2 = 1 => r1,
            ],
        }
        root_component_class! {
            no_differential;
            expect = <K, T> key!();
            left: left,
            right: right,
            components: [
                { markers: [marker!(cached: 1, locked: 0) => l1, marker!(cached: 1.5, locked: 1) => m1, marker!(cached: 2, locked: 2) => m2, marker!(cached: 3, locked: 4) => r1] },
                { markers: [marker!(cached: 1.5, locked: 0) => l2, marker!(cached: 3.5, locked: 2) => r2] },
            ],
            links: [
                left = 1 => l1,
                l1 = 1 => m2,
                m1 = 0 => l2,
                m2 = 1 => r1,
            ],
        }
        assert_eq_root_component_class(&differential_calcurated, &expect, &key!()).await;
    }
}
