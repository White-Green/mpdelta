use mpdelta_core::common::mixed_fraction::MixedFraction;
use mpdelta_core::component::marker_pin::{MarkerPin, MarkerPinId};
use mpdelta_core::component::parameter::ParameterValueType;
use mpdelta_core::component::processor::ComponentsLinksPair;
use mpdelta_core::time::TimelineTime;
use nalgebra::{Const, CsMatrix, DVector, Dyn, OMatrix, VecStorage, QR};
use std::collections::HashMap;
use std::fmt::Debug;
use std::iter;
use thiserror::Error;

#[derive(Clone, Debug, Error)]
pub enum CollectCachedTimeError {
    #[error("strong connected loop found")]
    StrongConnectedLoopFound,
    #[error("unknown pin id")]
    UnknownPinId(MarkerPinId),
    #[error("failed to solve")]
    FailedToSolve,
}

pub fn collect_cached_time<T, C>(components_and_links: C) -> Result<HashMap<MarkerPinId, TimelineTime>, CollectCachedTimeError>
where
    T: ParameterValueType,
    C: ComponentsLinksPair<T>,
{
    let pin_map = [components_and_links.left(), components_and_links.right()]
        .into_iter()
        .chain(components_and_links.components().flat_map(|component| iter::once(component.marker_left()).chain(component.markers()).chain(iter::once(component.marker_right()))))
        .map(MarkerPin::id)
        .copied()
        .enumerate()
        .map(|(i, p)| (p, i))
        .collect::<HashMap<_, _>>();
    let mut union_find = UnionFind::new(pin_map.len());
    components_and_links.links().try_for_each(|link| {
        let from = *pin_map.get(link.from()).ok_or(CollectCachedTimeError::UnknownPinId(*link.from()))?;
        let to = *pin_map.get(link.to()).ok_or(CollectCachedTimeError::UnknownPinId(*link.to()))?;
        if union_find.get_root(from) == union_find.get_root(to) {
            return Err(CollectCachedTimeError::StrongConnectedLoopFound);
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
    let mut unproceed_links = components_and_links.links().collect::<Vec<_>>();
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
        for component in components_and_links.components() {
            pin_handle.clear();
            pin_handle.extend(iter::once(component.marker_left()).chain(component.markers()).chain(iter::once(component.marker_right())));
            let (pins, tail) = pin_data_slice.split_at(component.markers().len() + 2);
            pin_data_slice = tail;
            used.fill(false);
            for (i, pin) in pins.iter().enumerate() {
                let current_subtree_index = pin.subtree_index;
                if used[current_subtree_index] || pin_handle[i].locked_component_time().is_none() {
                    continue;
                }
                used[current_subtree_index] = true;
                let base_time = pin_handle[i].locked_component_time().unwrap();
                if let Some((right_index, right_pin)) = pins[i..].iter().enumerate().skip(1).find_map(|(j, p)| (p.subtree_index == current_subtree_index && pin_handle[j].locked_component_time().is_some()).then_some((j + i, p))) {
                    let mut right = pin_handle[right_index].locked_component_time().unwrap();
                    let time_ratio = (right_pin.time_from_base - pin.time_from_base).value() / (right.value() - base_time.value());
                    let time_ratio = if time_ratio.signum() < 0 { -time_ratio } else { time_ratio };
                    for (j, p) in pins[..i].iter().enumerate() {
                        if current_subtree_index == p.subtree_index {
                            continue;
                        }
                        let Some(t) = pin_handle[j].locked_component_time() else {
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
                            let Some(t) = pin_handle[j].locked_component_time() else {
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
                        base_time = pin_handle[i].locked_component_time().unwrap();
                        if let Some((ri, rp)) = pins[i..].iter().enumerate().skip(1).find_map(|(j, p)| (p.subtree_index == current_subtree_index && pin_handle[j].locked_component_time().is_some()).then_some((j + i, p))) {
                            right_index = ri;
                            right_pin = rp;
                            right = pin_handle[right_index].locked_component_time().unwrap();
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
                        let Some(t) = pin_handle[j].locked_component_time() else {
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
                        let Some(t) = pin_handle[j].locked_component_time() else {
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
            time_diff.push(components_and_links.right().locked_component_time().map_or(10., |t| t.value().into_f64()));
        }
        let a = CsMatrix::from_triplet(time_diff.len(), connected_subgraph.len(), &row_indices, &column_indices, &values);
        let b = DVector::from_data(VecStorage::new(Dyn(time_diff.len()), Const::<1>, time_diff));

        let a_transpose = a.transpose();
        let mut right = OMatrix::from(&a_transpose * &CsMatrix::from(b));
        let solve_succeed = QR::new(OMatrix::from(&a_transpose * &a)).solve_mut(&mut right);
        if !solve_succeed {
            return Err(CollectCachedTimeError::FailedToSolve);
        }
        connected_subgraph.iter_mut().zip(right.iter().copied()).for_each(|(subgraph, time)| {
            subgraph.base_time = TimelineTime::new(MixedFraction::from_f64(time));
        });
    }

    let timeline_time = [components_and_links.left(), components_and_links.right()]
        .into_iter()
        .chain(components_and_links.components().flat_map(|component| iter::once(component.marker_left()).chain(component.markers()).chain(iter::once(component.marker_right()))))
        .zip(&pin_data)
        .map(|(pin_handle, pin)| (*pin_handle.id(), connected_subgraph[pin.subtree_index].base_time + pin.time_from_base))
        .collect();
    Ok(timeline_time)
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
    use mpdelta_core::mfrac;
    use mpdelta_core::project::RootComponentClassItemWrite;
    use mpdelta_core_test_util::{assert_eq_root_component_class, root_component_class, TestIdGenerator};

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
        let id = TestIdGenerator::new();
        root_component_class! {
            custom_differential: collect_cached_time;
            target; <T>; id;
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
            custom_differential: |_| Ok::<_, ()>(HashMap::new());
            expect; <T>; id;
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
        let expect_timeline_time = HashMap::from([(left, TimelineTime::new(mfrac!(0))), (l1, TimelineTime::new(mfrac!(1))), (r1, TimelineTime::new(mfrac!(2))), (right, TimelineTime::new(mfrac!(10)))]);
        let root = expect.read().await;
        RootComponentClassItemWrite::commit_changes(root.get_mut().await, expect_timeline_time);
        assert_eq_root_component_class(&target, &expect).await;

        root_component_class! {
            custom_differential: collect_cached_time;
            target; <T>; id;
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
            custom_differential: |_| Ok::<_, ()>(HashMap::new());
            expect; <T>; id;
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
        let expect_timeline_time = HashMap::from([
            (left, TimelineTime::new(mfrac!(0))),
            (l1, TimelineTime::new(mfrac!(1))),
            (m1, TimelineTime::new(mfrac!(3, 2))),
            (r1, TimelineTime::new(mfrac!(2))),
            (l2, TimelineTime::new(mfrac!(3, 2))),
            (r2, TimelineTime::new(mfrac!(7, 2))),
            (right, TimelineTime::new(mfrac!(10))),
        ]);
        let root = expect.read().await;
        RootComponentClassItemWrite::commit_changes(root.get_mut().await, expect_timeline_time);
        assert_eq_root_component_class(&target, &expect).await;

        root_component_class! {
            custom_differential: collect_cached_time;
            target; <T>; id;
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
            custom_differential: |_| Ok::<_, ()>(HashMap::new());
            expect; <T>; id;
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
        let expect_timeline_time = HashMap::from([
            (left, TimelineTime::new(mfrac!(0))),
            (l1, TimelineTime::new(mfrac!(1))),
            (m1, TimelineTime::new(mfrac!(3, 2))),
            (r1, TimelineTime::new(mfrac!(2))),
            (l2, TimelineTime::new(mfrac!(3, 2))),
            (r2, TimelineTime::new(mfrac!(7, 2))),
            (right, TimelineTime::new(mfrac!(10))),
        ]);
        let root = expect.read().await;
        RootComponentClassItemWrite::commit_changes(root.get_mut().await, expect_timeline_time);
        assert_eq_root_component_class(&target, &expect).await;

        root_component_class! {
            custom_differential: collect_cached_time;
            target; <T>; id;
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
            custom_differential: |_| Ok::<_, ()>(HashMap::new());
            expect; <T>; id;
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
        let expect_timeline_time = HashMap::from([
            (left, TimelineTime::new(mfrac!(0))),
            (l1, TimelineTime::new(mfrac!(1))),
            (m1, TimelineTime::new(mfrac!(3, 2))),
            (m2, TimelineTime::new(mfrac!(2))),
            (r1, TimelineTime::new(mfrac!(3))),
            (l2, TimelineTime::new(mfrac!(3, 2))),
            (r2, TimelineTime::new(mfrac!(7, 2))),
            (right, TimelineTime::new(mfrac!(10))),
        ]);
        let root = expect.read().await;
        RootComponentClassItemWrite::commit_changes(root.get_mut().await, expect_timeline_time);
        assert_eq_root_component_class(&target, &expect).await;

        root_component_class! {
            custom_differential: collect_cached_time;
            target; <T>; id;
            left: left,
            right: right,
            components: [
                { markers: [marker!(locked: 0) => l1, marker!(locked: 1) => m, marker!(locked: 2) => r1] },
            ],
            links: [
                left = 2 => m,
            ],
        }
        root_component_class! {
            custom_differential: |_| Ok::<_, ()>(HashMap::new());
            expect; <T>; id;
            left: left,
            right: right,
            components: [
                { markers: [marker!(locked: 0) => l1, marker!(locked: 1) => m, marker!(locked: 2) => r1] },
            ],
            links: [
                left = 2 => m,
            ],
        }
        let expect_timeline_time = HashMap::from([
            (left, TimelineTime::new(mfrac!(0))),
            (l1, TimelineTime::new(mfrac!(1))),
            (m, TimelineTime::new(mfrac!(2))),
            (r1, TimelineTime::new(mfrac!(3))),
            (right, TimelineTime::new(mfrac!(10))),
        ]);
        let root = expect.read().await;
        RootComponentClassItemWrite::commit_changes(root.get_mut().await, expect_timeline_time);
        assert_eq_root_component_class(&target, &expect).await;

        root_component_class! {
            custom_differential: collect_cached_time;
            target; <T>; id;
            left: left,
            right: right,
            components: [
                { markers: [marker!(locked: 0) => l1, marker!(locked: 1) => m, marker!(locked: 3) => r1] },
            ],
            links: [
                left = 2 => m,
                m = 1 => r1,
            ],
        }
        root_component_class! {
            custom_differential: |_| Ok::<_, ()>(HashMap::new());
            expect; <T>; id;
            left: left,
            right: right,
            components: [
                { markers: [marker!(locked: 0) => l1, marker!(locked: 1) => m, marker!(locked: 3) => r1] },
            ],
            links: [
                left = 2 => m,
                m = 1 => r1,
            ],
        }
        let expect_timeline_time = HashMap::from([
            (left, TimelineTime::new(mfrac!(0))),
            (l1, TimelineTime::new(mfrac!(3, 2))),
            (m, TimelineTime::new(mfrac!(2))),
            (r1, TimelineTime::new(mfrac!(3))),
            (right, TimelineTime::new(mfrac!(10))),
        ]);
        let root = expect.read().await;
        RootComponentClassItemWrite::commit_changes(root.get_mut().await, expect_timeline_time);
        assert_eq_root_component_class(&target, &expect).await;

        root_component_class! {
            custom_differential: collect_cached_time;
            target; <T>; id;
            left: left,
            right: right,
            components: [
                { markers: [marker!(locked: 0) => l1, marker!(locked: 1) => m, marker!() => r1] },
            ],
            links: [
                left = 1 => l1,
                l1 = 2 => m,
                l1 = 3 => r1,
            ],
        }
        root_component_class! {
            custom_differential: |_| Ok::<_, ()>(HashMap::new());
            expect; <T>; id;
            left: left,
            right: right,
            components: [
                { markers: [marker!(locked: 0) => l1, marker!(locked: 1) => m, marker!() => r1] },
            ],
            links: [
                left = 1 => l1,
                l1 = 2 => m,
                l1 = 3 => r1,
            ],
        }
        let expect_timeline_time = HashMap::from([]);
        let root = expect.read().await;
        RootComponentClassItemWrite::commit_changes(root.get_mut().await, expect_timeline_time);
        assert_eq_root_component_class(&target, &expect).await;

        root_component_class! {
            custom_differential: collect_cached_time;
            target; <T>; id;
            left: left,
            right: right,
            components: [
                { markers: [marker!(locked: 0) => l1, marker!(locked: 1) => m1, marker!(locked: 2) => m2, marker!(locked: 0) => r1] },
            ],
            links: [
                left = 2 => m2,
                m1 = 3 => r1,
            ],
        }
        root_component_class! {
            custom_differential: |_| Ok::<_, ()>(HashMap::new());
            expect; <T>; id;
            left: left,
            right: right,
            components: [
                { markers: [marker!(locked: 0) => l1, marker!(locked: 1) => m1, marker!(locked: 2) => m2, marker!(locked: 0) => r1] },
            ],
            links: [
                left = 2 => m2,
                m1 = 3 => r1,
            ],
        }
        let expect_timeline_time = HashMap::from([
            (left, TimelineTime::new(mfrac!(0))),
            (l1, TimelineTime::new(mfrac!(0))),
            (m1, TimelineTime::new(mfrac!(1))),
            (m2, TimelineTime::new(mfrac!(2))),
            (r1, TimelineTime::new(mfrac!(4))),
            (right, TimelineTime::new(mfrac!(10))),
        ]);
        let root = expect.read().await;
        RootComponentClassItemWrite::commit_changes(root.get_mut().await, expect_timeline_time);
        assert_eq_root_component_class(&target, &expect).await;
    }
}
