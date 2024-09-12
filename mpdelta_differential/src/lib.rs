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
        let mut group_count = vec![0; connected_subgraph.len()];
        let mut solver = LSMSolver::new(connected_subgraph.len());
        solver.assume_pin_at(&pin_data[0], MixedFraction::ZERO);
        if connected_subgraph[pin_data[1].subtree_index].connected_node_count == 1 {
            solver.assume_pin_at(&pin_data[1], components_and_links.right().locked_component_time().map_or(MixedFraction::from_integer(10), |t| t.value()));
        }
        for component in components_and_links.components() {
            let (pins, tail) = pin_data_slice.split_at(component.markers().len() + 2);
            pin_data_slice = tail;
            pin_handle.clear();
            pin_handle.extend(iter::once(component.marker_left()).chain(component.markers()).chain(iter::once(component.marker_right())).zip(pins).filter(|(p, _)| p.locked_component_time().is_some()));
            group_count.fill(0);
            pin_handle.iter().for_each(|(_, p)| group_count[p.subtree_index] += 1);
            for w in pin_handle.windows(2) {
                let (left_pin, left_data) = w[0];
                let (right_pin, right_data) = w[1];
                if group_count[left_data.subtree_index] > 1 || group_count[right_data.subtree_index] > 1 {
                    continue;
                }
                let diff = right_pin.locked_component_time().unwrap().value() - left_pin.locked_component_time().unwrap().value();
                solver.assume_pin_difference(left_data, right_data, diff);
            }
            for w in pin_handle.windows(3) {
                let (left_pin, left_data) = w[0];
                let (middle_pin, middle_data) = w[1];
                let (right_pin, right_data) = w[2];
                if left_data.subtree_index == middle_data.subtree_index && middle_data.subtree_index == right_data.subtree_index {
                    continue;
                }
                let diff1 = middle_pin.locked_component_time().unwrap().value() - left_pin.locked_component_time().unwrap().value();
                let diff2 = right_pin.locked_component_time().unwrap().value() - middle_pin.locked_component_time().unwrap().value();
                solver.assume_pin_difference3(left_data, middle_data, right_data, diff1, diff2);
            }
        }
        let times = solver.solve().ok_or(CollectCachedTimeError::FailedToSolve)?;

        connected_subgraph.iter_mut().zip(times).for_each(|(subgraph, time)| {
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

struct LSMSolver {
    cols: usize,
    a_row_indices: Vec<usize>,
    a_column_indices: Vec<usize>,
    a_values: Vec<f64>,
    b_values: Vec<f64>,
}

impl LSMSolver {
    fn new(subgraph_count: usize) -> LSMSolver {
        LSMSolver {
            cols: subgraph_count,
            a_row_indices: Vec::new(),
            a_column_indices: Vec::new(),
            a_values: Vec::new(),
            b_values: Vec::new(),
        }
    }

    fn assume_pin_at(&mut self, pin: &PinData, time: MixedFraction) {
        self.a_column_indices.push(pin.subtree_index);
        self.a_row_indices.push(self.b_values.len());
        self.a_values.push(1.);
        self.b_values.push((time - pin.time_from_base.value()).into_f64());
    }

    fn assume_pin_difference(&mut self, pin1: &PinData, pin2: &PinData, diff: MixedFraction) {
        let diff = (diff.abs() + pin1.time_from_base.value() - pin2.time_from_base.value()).into_f64();
        self.a_column_indices.extend([pin2.subtree_index, pin1.subtree_index]);
        self.a_row_indices.extend([self.b_values.len(); 2]);
        self.a_values.extend([1., -1.]);
        self.b_values.push(diff);
    }

    fn assume_pin_difference3(&mut self, pin1: &PinData, pin2: &PinData, pin3: &PinData, diff1: MixedFraction, diff2: MixedFraction) {
        if pin1.subtree_index == pin2.subtree_index && pin2.subtree_index == pin3.subtree_index {
            return;
        }
        let diff1 = diff1.abs();
        let diff2 = diff2.abs();
        let diff_sum = diff1 + diff2;
        if diff_sum == MixedFraction::ZERO {
            self.assume_pin_difference(pin1, pin3, MixedFraction::ZERO);
            self.assume_pin_difference(pin2, pin3, MixedFraction::ZERO);
            return;
        }
        // pin2 - pin1 ~ diff1 / (diff1 + diff2) * (pin3 - pin1)
        // pin2 - pin1 ~ diff1 / (diff1 + diff2) * pin3 - diff1 / (diff1 + diff2) * pin1
        // -diff2 / (diff1 + diff2) * pin1 + pin2 - diff1 / (diff1 + diff2) * pin3 ~ 0
        // -diff2 / (diff1 + diff2) * pin1.base_time + pin2.base_time - diff1 / (diff1 + diff2) * pin3.base_time ~ diff2 / (diff1 + diff2) * pin1.time_from_base - pin2.time_from_base + diff1 / (diff1 + diff2) * pin3.time_from_base
        let mut pins = [(pin1.subtree_index, -diff2 / (diff1 + diff2)), (pin2.subtree_index, MixedFraction::ONE), (pin3.subtree_index, -diff1 / (diff1 + diff2))];
        pins.sort_by_key(|&(i, _)| i);
        match pins {
            [(p1, d1), (p2, d2), (p3, d3)] if p1 == p2 => {
                self.a_column_indices.extend([p2, p3]);
                self.a_row_indices.extend([self.b_values.len(); 2]);
                self.a_values.extend([(d1 + d2).into_f64(), d3.into_f64()]);
            }
            [(p1, d1), (p2, d2), (p3, d3)] if p2 == p3 => {
                self.a_column_indices.extend([p1, p2]);
                self.a_row_indices.extend([self.b_values.len(); 2]);
                self.a_values.extend([d1.into_f64(), (d2 + d3).into_f64()]);
            }
            [(p1, d1), (p2, d2), (p3, d3)] => {
                self.a_column_indices.extend([p1, p2, p3]);
                self.a_row_indices.extend([self.b_values.len(); 3]);
                self.a_values.extend([d1.into_f64(), d2.into_f64(), d3.into_f64()]);
            }
        }
        self.b_values.push((diff2 / (diff1 + diff2) * pin1.time_from_base.value() - pin2.time_from_base.value() + diff1 / (diff1 + diff2) * pin3.time_from_base.value()).into_f64());
    }

    fn solve(self) -> Option<Vec<f64>> {
        let a = CsMatrix::from_triplet(self.b_values.len(), self.cols, &self.a_row_indices, &self.a_column_indices, &self.a_values);
        let b = DVector::from_data(VecStorage::new(Dyn(self.b_values.len()), Const::<1>, self.b_values));
        let a_transpose = a.transpose();
        let mut right = OMatrix::from(&a_transpose * &CsMatrix::from(b));
        let solve_succeed = QR::new(OMatrix::from(&a_transpose * &a)).solve_mut(&mut right);
        if !solve_succeed {
            return None;
        }
        Some(right.data.into())
    }
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
    use mpdelta_core::component::link::MarkerLink;
    use mpdelta_core::mfrac;
    use mpdelta_core::project::RootComponentClassItemWrite;
    use mpdelta_core_test_util::{assert_eq_root_component_class, root_component_class, TestIdGenerator};
    use std::array;
    use std::collections::BTreeMap;
    use std::fmt::Formatter;

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
    async fn test_collect_cached_time_all() {
        let id = TestIdGenerator::new();
        root_component_class! {
            custom_differential: |_| Ok::<_, ()>(HashMap::new());
            target; <T>; id;
            left: left,
            right: right,
            components: [
                {
                    markers: [
                        marker!(locked: 0) => p0,
                        marker!(locked: 1) => p1,
                        marker!(locked: 2) => p2,
                        marker!(locked: 3) => p3,
                        marker!(locked: 4) => p4,
                    ]
                },
            ],
            links: [],
        }
        let answer = BTreeMap::from([
            (left, TimelineTime::new(mfrac!(0))),
            (p0, TimelineTime::new(mfrac!(0))),
            (p1, TimelineTime::new(mfrac!(1))),
            (p2, TimelineTime::new(mfrac!(2))),
            (p3, TimelineTime::new(mfrac!(3))),
            (p4, TimelineTime::new(mfrac!(4))),
            (right, TimelineTime::new(mfrac!(10))),
        ]);
        struct CompactDebug<'a, K, V>(&'a BTreeMap<K, V>);
        impl<'a, K, V> Debug for CompactDebug<'a, K, V>
        where
            K: Debug,
            V: Debug,
        {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                writeln!(f, "{{")?;
                for (k, v) in self.0 {
                    writeln!(f, "    {k:?}: {v:?}")?;
                }
                write!(f, "}}")
            }
        }
        const LEN: usize = 5;
        let markers: [_; LEN] = [p0, p1, p2, p3, p4];
        let root_component_class = target.read().await;
        let mut err = 0;
        for state in 0..LEN.pow(LEN as u32 + 1) {
            let [combination @ .., base_pin] = array::from_fn::<_, { LEN + 1 }, _>(|i| (state / LEN.pow(i as u32)) % LEN);
            let mut root_component_class = root_component_class.get_mut().await;
            for i in 0..LEN {
                let mut iter = combination.iter().enumerate().filter_map(|(j, &group_id)| (group_id == i).then_some(j));
                let Some(base) = iter.next() else {
                    continue;
                };
                for p in iter {
                    root_component_class.add_link(MarkerLink::new(markers[base], markers[p], TimelineTime::new(MixedFraction::from_integer((p - base) as i32))));
                }
            }
            root_component_class.add_link(MarkerLink::new(left, markers[base_pin], TimelineTime::new(MixedFraction::from_integer(base_pin as i32))));
            let time_map = collect_cached_time(&*root_component_class).unwrap();
            let time_map = BTreeMap::from_iter(time_map);
            if time_map != answer {
                err += 1;
                eprintln!("{combination:?} {base_pin}");
                eprintln!("{:?}", CompactDebug(&time_map));
            }
        }
        assert_eq!(err, 0, "{err} / {} error", LEN.pow(LEN as u32 + 1));
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
        let expect_timeline_time = HashMap::from([
            (left, TimelineTime::new(mfrac!(0))),
            (l1, TimelineTime::new(mfrac!(1))),
            (m, TimelineTime::new(mfrac!(3))),
            (r1, TimelineTime::new(mfrac!(4))),
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

        root_component_class! {
            custom_differential: collect_cached_time;
            target; <T>; id;
            left: left,
            right: right,
            components: [
                { markers: [marker!(locked: 0) => l1, marker!() => m, marker!(locked: 2) => r1] },
            ],
            links: [
                left = 1 => l1,
                l1 = 1 => m,
            ],
        }
        root_component_class! {
            custom_differential: |_| Ok::<_, ()>(HashMap::new());
            expect; <T>; id;
            left: left,
            right: right,
            components: [
                { markers: [marker!(locked: 0) => l1, marker!() => m, marker!(locked: 2) => r1] },
            ],
            links: [
                left = 1 => l1,
                l1 = 1 => m,
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
    }
}
