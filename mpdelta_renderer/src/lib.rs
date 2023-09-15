use crate::cell_by_need::{DynFn, DynFuture, FunctionByNeed};
use crate::cloneable_iterator::{CloneableIterator, CloneableIteratorMarker};
use arc_swap::ArcSwapOption;
use arrayvec::ArrayVec;
use async_trait::async_trait;
use cgmath::{One, Quaternion};
use dashmap::DashMap;
use either::Either;
use futures::stream::{self, StreamExt};
use futures::{FutureExt, TryFutureExt, TryStreamExt};
use mpdelta_core::common::time_split_value::TimeSplitValue;
use mpdelta_core::component::instance::ComponentInstance;
use mpdelta_core::component::link::MarkerLink;
use mpdelta_core::component::marker_pin::{MarkerPin, MarkerTime};
use mpdelta_core::component::parameter::placeholder::{Placeholder, TagAudio, TagImage};
use mpdelta_core::component::parameter::value::{EasingValue, FrameVariableValue};
use mpdelta_core::component::parameter::{
    AbstractFile, AudioRequiredParams, AudioRequiredParamsFrameVariable, ComponentProcessorInputValue, ImageRequiredParams, ImageRequiredParamsFixed, ImageRequiredParamsFrameVariable, ImageRequiredParamsTransform, ImageRequiredParamsTransformFrameVariable, Never, Opacity, Parameter,
    ParameterAllValues, ParameterFrameVariableValue, ParameterNullableValue, ParameterSelect, ParameterValue, ParameterValueType, VariableParameterPriority, VariableParameterValue,
};
use mpdelta_core::component::processor::{ComponentProcessor, ComponentProcessorBody, NativeProcessorExecutable, NativeProcessorInput};
use mpdelta_core::core::{ComponentRendererBuilder, IdGenerator};
use mpdelta_core::native::processor::ParameterNativeProcessorInputFixed;
use mpdelta_core::ptr::{StaticPointer, StaticPointerCow};
use mpdelta_core::time::TimelineTime;
use mpdelta_core::usecase::RealtimeComponentRenderer;
use once_cell::sync::OnceCell;
use qcell::{TCell, TCellOwner};
use std::borrow::Borrow;
use std::cmp::Ordering;
use std::collections::hash_map::Entry;
use std::collections::{btree_map, BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::convert::Infallible;
use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::ops::{Deref, Range};

use std::sync::Arc;
use std::{array, future, iter};
use thiserror::Error;
use tokio::runtime::Handle;
use tokio::sync::{OwnedRwLockReadGuard, RwLock};
use tokio::task::JoinHandle;

mod cell_by_need;
mod cloneable_iterator;

pub struct MPDeltaRendererBuilder<K: 'static, Id, ImageCombinerBuilder, AudioCombinerBuilder> {
    id: Arc<Id>,
    image_combiner_builder: Arc<ImageCombinerBuilder>,
    audio_combiner_builder: Arc<AudioCombinerBuilder>,
    key: Arc<RwLock<TCellOwner<K>>>,
}

impl<K, Id, ImageCombinerBuilder, AudioCombinerBuilder> MPDeltaRendererBuilder<K, Id, ImageCombinerBuilder, AudioCombinerBuilder> {
    pub fn new(id: Arc<Id>, image_combiner_builder: Arc<ImageCombinerBuilder>, audio_combiner_builder: Arc<AudioCombinerBuilder>, key: Arc<RwLock<TCellOwner<K>>>) -> MPDeltaRendererBuilder<K, Id, ImageCombinerBuilder, AudioCombinerBuilder> {
        MPDeltaRendererBuilder {
            id,
            image_combiner_builder,
            audio_combiner_builder,
            key,
        }
    }
}

#[async_trait]
impl<
        K,
        T: ParameterValueType + 'static,
        ImageCombinerBuilder: CombinerBuilder<T::Image, Request = ImageSizeRequest, Param = ImageRequiredParamsFixed> + 'static,
        AudioCombinerBuilder: CombinerBuilder<T::Audio, Request = (), Param = AudioRequiredParamsFrameVariable> + 'static,
        Id: IdGenerator + 'static,
    > ComponentRendererBuilder<K, T> for MPDeltaRendererBuilder<K, Id, ImageCombinerBuilder, AudioCombinerBuilder>
{
    type Err = Infallible;
    type Renderer = MPDeltaRenderer<K, T, ImageCombinerBuilder, AudioCombinerBuilder, Id>;

    async fn create_renderer(&self, component: &StaticPointer<TCell<K, ComponentInstance<K, T>>>) -> Result<Self::Renderer, Self::Err> {
        let key = self.key.read().await;
        let component_ref = component.upgrade().unwrap();
        let component_ref = component_ref.ro(&key);
        let left = component_ref.marker_left().upgrade().unwrap();
        let right = component_ref.marker_right().upgrade().unwrap();
        let left = left.ro(&key).cached_timeline_time().value();
        let right = right.ro(&key).cached_timeline_time().value();
        let frames_count = ((right - left) * 60.) as usize;
        let frames = (0..frames_count).map(move |i| TimelineTime::new(left + i as f64 / 60.).unwrap());
        Ok(MPDeltaRenderer {
            runtime: Handle::current(),
            evaluate_component: Arc::new(EvaluateComponent::new(
                component.clone(),
                frames.clone_dyn(),
                ReferenceFunctions(Arc::new(DashMap::new())),
                Arc::new(HashMap::new()),
                Arc::clone(&self.id),
                Arc::clone(&self.image_combiner_builder),
                Arc::clone(&self.audio_combiner_builder),
            )),
            image_size_request: ImageSizeRequest { width: 1920., height: 1080. },
            frames: frames.collect(),
            key: Arc::clone(&self.key),
        })
    }
}

pub struct MPDeltaRenderer<K: 'static, T: ParameterValueType, ImageCombinerBuilder, AudioCombinerBuilder, Id> {
    runtime: Handle,
    evaluate_component: Arc<EvaluateComponent<K, T, ImageCombinerBuilder, AudioCombinerBuilder, Id>>,
    image_size_request: ImageSizeRequest,
    frames: Vec<TimelineTime>,
    key: Arc<RwLock<TCellOwner<K>>>,
}

#[derive(Error)]
pub enum RenderError<K: 'static, T> {
    #[error("{0}")]
    EvaluateError(#[from] EvaluateError<K, T>),
    #[error("a frame index is out of range: the length is {length} but the index is {index}")]
    FrameOutOfRange { length: usize, index: usize },
    #[error("required type value is not provided")]
    NotProvided,
}

impl<K, T> Debug for RenderError<K, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderError::EvaluateError(e) => f.debug_tuple("EvaluateError").field(e).finish(),
            RenderError::FrameOutOfRange { length, index } => f.debug_struct("FrameOutOfRange").field("length", length).field("index", index).finish(),
            RenderError::NotProvided => f.debug_struct("NotProvided").finish(),
        }
    }
}

async fn wait_for_drop_arc_inner<T>(arc: Arc<T>) {
    let mut arc = Some(arc);
    loop {
        let Err(a) = Arc::try_unwrap(arc.take().unwrap()) else {
            return;
        };
        arc = Some(a);
        tokio::task::yield_now().await;
    }
}

impl<
        K,
        T: ParameterValueType + 'static,
        ImageCombinerBuilder: CombinerBuilder<T::Image, Request = ImageSizeRequest, Param = ImageRequiredParamsFixed> + 'static,
        AudioCombinerBuilder: CombinerBuilder<T::Audio, Request = (), Param = AudioRequiredParamsFrameVariable> + 'static,
        Id: IdGenerator + 'static,
    > RealtimeComponentRenderer<T> for MPDeltaRenderer<K, T, ImageCombinerBuilder, AudioCombinerBuilder, Id>
{
    type Err = RenderError<K, T>;

    fn get_frame_count(&self) -> usize {
        self.frames.len()
    }

    fn render_frame(&self, frame: usize) -> Result<T::Image, Self::Err> {
        let Some(&at) = self.frames.get(frame) else {
            return Err(RenderError::FrameOutOfRange { length: self.frames.len(), index: frame });
        };
        let result = self.runtime.block_on(Arc::clone(&self.key).read_owned().map(Arc::new).then(|key| {
            Arc::clone(&self.evaluate_component)
                .evaluate(at, ParameterSelectValue(Parameter::Image(())), self.image_size_request, Arc::clone(&key))
                .then(|ret| wait_for_drop_arc_inner(key).map(|_| ret))
        }))?;
        let (image, _) = result.ok_or(RenderError::NotProvided)?.into_image().unwrap();
        Ok(image)
    }

    fn sampling_rate(&self) -> u32 {
        48_000
    }

    fn mix_audio(&self, _offset: usize, _length: usize) -> Result<T::Audio, Self::Err> {
        let result = self.runtime.block_on(Arc::clone(&self.key).read_owned().map(Arc::new).then(|key| {
            Arc::clone(&self.evaluate_component)
                .evaluate(self.frames[0], ParameterSelectValue(Parameter::Image(())), self.image_size_request, Arc::clone(&key))
                .then(|ret| wait_for_drop_arc_inner(key).map(|_| ret))
        }))?;
        let (audio, _) = result.ok_or(RenderError::NotProvided)?.into_audio().unwrap();
        Ok(audio)
    }

    fn render_param(&self, _param: Parameter<ParameterSelect>) -> Result<Parameter<T>, Self::Err> {
        todo!()
    }
}

struct ValueCacheType<Image, Audio>(PhantomData<(Image, Audio)>);

impl<Image: Send + Sync + 'static, Audio: Send + Sync + Clone + 'static> ParameterValueType for ValueCacheType<Image, Audio> {
    type Image = Arc<RwLock<BTreeMap<TimelineTime, Option<(Image, ImageRequiredParamsFixed)>>>>;
    type Audio = OnceCell<Option<(Audio, AudioRequiredParamsFrameVariable)>>;
    type Binary = OnceCell<Option<Arc<TimeSplitValue<TimelineTime, Option<Either<AbstractFile, FrameVariableValue<AbstractFile>>>>>>>;
    type String = OnceCell<Option<Arc<TimeSplitValue<TimelineTime, Option<Either<String, FrameVariableValue<String>>>>>>>;
    type Integer = OnceCell<Option<Arc<TimeSplitValue<TimelineTime, Option<Either<i64, FrameVariableValue<i64>>>>>>>;
    type RealNumber = OnceCell<Option<Arc<TimeSplitValue<TimelineTime, Option<Either<EasingValue<f64>, FrameVariableValue<f64>>>>>>>;
    type Boolean = OnceCell<Option<Arc<TimeSplitValue<TimelineTime, Option<Either<bool, FrameVariableValue<bool>>>>>>>;
    type Dictionary = ();
    type Array = ();
    type ComponentClass = ();
}

#[derive(Error)]
pub enum EvaluateError<K: 'static, T> {
    #[error("invalid component: {0:?}")]
    InvalidComponent(StaticPointer<TCell<K, ComponentInstance<K, T>>>),
    #[error("the output type by {component:?} is mismatch; expected: {expect:?}, but got {actual:?}")]
    OutputTypeMismatch {
        component: StaticPointer<TCell<K, ComponentInstance<K, T>>>,
        expect: Parameter<ParameterSelect>,
        actual: Parameter<ParameterSelect>,
    },
    #[error("a dependency cycle detected")]
    CycleDependency(Vec<StaticPointer<TCell<K, ComponentInstance<K, T>>>>),
    #[error("invalid link graph")]
    InvalidLinkGraph,
    #[error("invalid marker: {0:?}")]
    InvalidMarker(StaticPointer<TCell<K, MarkerPin>>),
    #[error("{index}-th variable parameter of {component:?} is invalid")]
    InvalidVariableParameter { component: StaticPointer<TCell<K, ComponentInstance<K, T>>>, index: usize },
}

impl<K, T> Debug for EvaluateError<K, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            EvaluateError::InvalidComponent(c) => f.debug_tuple("InvalidComponent").field(c).finish(),
            EvaluateError::OutputTypeMismatch { component, expect, actual } => f.debug_struct("OutputTypeMismatch").field("component", component).field("expect", expect).field("actual", actual).finish(),
            EvaluateError::CycleDependency(dependencies) => f.debug_tuple("CycleDependency").field(dependencies).finish(),
            EvaluateError::InvalidLinkGraph => f.debug_struct("InvalidLinkGraph").finish(),
            EvaluateError::InvalidMarker(m) => f.debug_tuple("InvalidMarker").field(m).finish(),
            EvaluateError::InvalidVariableParameter { component, index } => f.debug_struct("InvalidVariableParameter").field("component", component).field("index", index).finish(),
        }
    }
}

impl<K, T> Clone for EvaluateError<K, T> {
    fn clone(&self) -> Self {
        match self {
            EvaluateError::InvalidComponent(component) => EvaluateError::InvalidComponent(component.clone()),
            EvaluateError::OutputTypeMismatch { component, expect, actual } => EvaluateError::OutputTypeMismatch {
                component: component.clone(),
                expect: *expect,
                actual: *actual,
            },
            EvaluateError::CycleDependency(dependencies) => EvaluateError::CycleDependency(dependencies.clone()),
            EvaluateError::InvalidLinkGraph => EvaluateError::InvalidLinkGraph,
            EvaluateError::InvalidMarker(marker) => EvaluateError::InvalidMarker(marker.clone()),
            EvaluateError::InvalidVariableParameter { component, index } => EvaluateError::InvalidVariableParameter { component: component.clone(), index: *index },
        }
    }
}

fn value_into_processor_input_buffer<K: 'static>(param: ParameterValue<K>, key: &TCellOwner<K>) -> ComponentProcessorInputValueBuffer<(), ()> {
    fn convert<K, T: Clone + Send, U: Send>(value: TimeSplitValue<StaticPointer<TCell<K, MarkerPin>>, T>, key: &TCellOwner<K>) -> TimeSplitValue<TimelineTime, Option<Either<T, U>>> {
        value.map_time_value(
            |time| {
                let strong_ref = time.upgrade().unwrap();
                strong_ref.ro(key).cached_timeline_time()
            },
            |value| Some(Either::Left(value)),
        )
    }
    match param {
        ParameterValue::None => ComponentProcessorInputValueBuffer::None,
        ParameterValue::Image(_) => unreachable!(),
        ParameterValue::Audio(_) => unreachable!(),
        ParameterValue::Binary(value) => ComponentProcessorInputValueBuffer::Binary(convert(value, key)),
        ParameterValue::String(value) => ComponentProcessorInputValueBuffer::String(convert(value, key)),
        ParameterValue::Integer(value) => ComponentProcessorInputValueBuffer::Integer(convert(value, key)),
        ParameterValue::RealNumber(value) => ComponentProcessorInputValueBuffer::RealNumber(convert(value, key)),
        ParameterValue::Boolean(value) => ComponentProcessorInputValueBuffer::Boolean(convert(value, key)),
        ParameterValue::Dictionary(value) => {
            let _: Never = value;
            unreachable!()
        }
        ParameterValue::Array(value) => {
            let _: Never = value;
            unreachable!()
        }
        ParameterValue::ComponentClass(_) => ComponentProcessorInputValueBuffer::ComponentClass(()),
    }
}

pub struct ComponentProcessorInputBuffer<Image, Audio>(PhantomData<(Image, Audio)>);

type ComponentProcessorInputValueBuffer<Image, Audio> = Parameter<ComponentProcessorInputBuffer<Image, Audio>>;

impl<Image: Clone + Send + Sync + 'static, Audio: Clone + Send + Sync + 'static> ParameterValueType for ComponentProcessorInputBuffer<Image, Audio> {
    type Image = Image;
    type Audio = Audio;
    type Binary = TimeSplitValue<TimelineTime, Option<Either<AbstractFile, FrameVariableValue<AbstractFile>>>>;
    type String = TimeSplitValue<TimelineTime, Option<Either<String, FrameVariableValue<String>>>>;
    type Integer = TimeSplitValue<TimelineTime, Option<Either<i64, FrameVariableValue<i64>>>>;
    type RealNumber = TimeSplitValue<TimelineTime, Option<Either<EasingValue<f64>, FrameVariableValue<f64>>>>;
    type Boolean = TimeSplitValue<TimelineTime, Option<Either<bool, FrameVariableValue<bool>>>>;
    type Dictionary = Never;
    type Array = Never;
    type ComponentClass = ();
}

pub struct ComponentProcessorInputBufferRef<Image, Audio>(PhantomData<(Image, Audio)>);

type ComponentProcessorInputValueBufferRef<Image, Audio> = Parameter<ComponentProcessorInputBufferRef<Image, Audio>>;

impl<Image: Clone + Send + Sync + 'static, Audio: Clone + Send + Sync + 'static> ParameterValueType for ComponentProcessorInputBufferRef<Image, Audio> {
    type Image = Image;
    type Audio = Audio;
    type Binary = Arc<TimeSplitValue<TimelineTime, Option<Either<AbstractFile, FrameVariableValue<AbstractFile>>>>>;
    type String = Arc<TimeSplitValue<TimelineTime, Option<Either<String, FrameVariableValue<String>>>>>;
    type Integer = Arc<TimeSplitValue<TimelineTime, Option<Either<i64, FrameVariableValue<i64>>>>>;
    type RealNumber = Arc<TimeSplitValue<TimelineTime, Option<Either<EasingValue<f64>, FrameVariableValue<f64>>>>>;
    type Boolean = Arc<TimeSplitValue<TimelineTime, Option<Either<bool, FrameVariableValue<bool>>>>>;
    type Dictionary = Never;
    type Array = Never;
    type ComponentClass = ();
}

fn nullable_into_processor_input_buffer_ref<K: 'static>(param: ParameterNullableValue<K>, key: &TCellOwner<K>) -> ComponentProcessorInputValueBufferRef<(), ()> {
    fn convert<K, T>(value: TimeSplitValue<StaticPointer<TCell<K, MarkerPin>>, Option<T>>, key: &TCellOwner<K>) -> TimeSplitValue<TimelineTime, Option<Either<T, FrameVariableValue<T>>>> {
        value.map_time_value(|t| t.upgrade().unwrap().ro(key).cached_timeline_time(), |v| v.map(Either::Left))
    }
    fn convert_easing<K, T>(value: TimeSplitValue<StaticPointer<TCell<K, MarkerPin>>, Option<EasingValue<T>>>, key: &TCellOwner<K>) -> TimeSplitValue<TimelineTime, Option<Either<EasingValue<T>, FrameVariableValue<T>>>> {
        value.map_time_value(|t| t.upgrade().unwrap().ro(key).cached_timeline_time(), |v| v.map(Either::Left))
    }
    match param {
        ParameterNullableValue::None => Parameter::None,
        ParameterNullableValue::Image(_) => unreachable!(),
        ParameterNullableValue::Audio(_) => unreachable!(),
        ParameterNullableValue::Binary(value) => Parameter::Binary(Arc::new(convert(value, key))),
        ParameterNullableValue::String(value) => Parameter::String(Arc::new(convert(value, key))),
        ParameterNullableValue::Integer(value) => Parameter::Integer(Arc::new(convert(value, key))),
        ParameterNullableValue::RealNumber(value) => Parameter::RealNumber(Arc::new(convert_easing(value, key))),
        ParameterNullableValue::Boolean(value) => Parameter::Boolean(Arc::new(convert(value, key))),
        ParameterNullableValue::Dictionary(value) => {
            let _: Never = value;
            unreachable!()
        }
        ParameterNullableValue::Array(value) => {
            let _: Never = value;
            unreachable!()
        }
        ParameterNullableValue::ComponentClass(_) => Parameter::ComponentClass(()),
    }
}

fn nullable_into_processor_input_buffer<K: 'static, Image: Clone + Send + Sync + 'static, Audio: Clone + Send + Sync + 'static>(param: ParameterNullableValue<K>, key: &TCellOwner<K>) -> ComponentProcessorInputValueBuffer<Image, Audio> {
    fn convert<K, T>(value: TimeSplitValue<StaticPointer<TCell<K, MarkerPin>>, Option<T>>, key: &TCellOwner<K>) -> TimeSplitValue<TimelineTime, Option<Either<T, FrameVariableValue<T>>>> {
        value.map_time_value(|t| t.upgrade().unwrap().ro(key).cached_timeline_time(), |v| v.map(Either::Left))
    }
    fn convert_easing<K, T>(value: TimeSplitValue<StaticPointer<TCell<K, MarkerPin>>, Option<EasingValue<T>>>, key: &TCellOwner<K>) -> TimeSplitValue<TimelineTime, Option<Either<EasingValue<T>, FrameVariableValue<T>>>> {
        value.map_time_value(|t| t.upgrade().unwrap().ro(key).cached_timeline_time(), |v| v.map(Either::Left))
    }
    match param {
        ParameterNullableValue::None => Parameter::None,
        ParameterNullableValue::Image(_) => unreachable!(),
        ParameterNullableValue::Audio(_) => unreachable!(),
        ParameterNullableValue::Binary(value) => Parameter::Binary(convert(value, key)),
        ParameterNullableValue::String(value) => Parameter::String(convert(value, key)),
        ParameterNullableValue::Integer(value) => Parameter::Integer(convert(value, key)),
        ParameterNullableValue::RealNumber(value) => Parameter::RealNumber(convert_easing(value, key)),
        ParameterNullableValue::Boolean(value) => Parameter::Boolean(convert(value, key)),
        ParameterNullableValue::Dictionary(value) => {
            let _: Never = value;
            unreachable!()
        }
        ParameterNullableValue::Array(value) => {
            let _: Never = value;
            unreachable!()
        }
        ParameterNullableValue::ComponentClass(_) => Parameter::ComponentClass(()),
    }
}

fn empty_input_buffer<T: ParameterValueType>(ty: &Parameter<T>, left: TimelineTime, right: TimelineTime) -> ComponentProcessorInputValueBuffer<(), ()> {
    match ty {
        Parameter::None => Parameter::None,
        Parameter::Image(_) => unreachable!(),
        Parameter::Audio(_) => unreachable!(),
        Parameter::Binary(_) => Parameter::Binary(TimeSplitValue::new(left, None, right)),
        Parameter::String(_) => Parameter::String(TimeSplitValue::new(left, None, right)),
        Parameter::Integer(_) => Parameter::Integer(TimeSplitValue::new(left, None, right)),
        Parameter::RealNumber(_) => Parameter::RealNumber(TimeSplitValue::new(left, None, right)),
        Parameter::Boolean(_) => Parameter::Boolean(TimeSplitValue::new(left, None, right)),
        Parameter::Dictionary(_) => unreachable!(),
        Parameter::Array(_) => unreachable!(),
        Parameter::ComponentClass(_) => Parameter::ComponentClass(()),
    }
}

fn upper_bound(range: Range<usize>, mut term: impl FnMut(usize) -> bool) -> usize {
    let Range { mut start, mut end } = range;
    assert!(start <= end);
    while end - start > 1 {
        let mid = (start + end) / 2;
        *if term(mid) { &mut start } else { &mut end } = mid;
    }
    start
}

fn override_time_split_value<T: Clone>(mut value1: TimeSplitValue<TimelineTime, Option<T>>, value2: &TimeSplitValue<TimelineTime, Option<T>>, TimelineRangeSet(ranges): &TimelineRangeSet) -> TimeSplitValue<TimelineTime, Option<T>> {
    let mut global_start1 = 0;
    let mut global_start2 = 0;
    for &TimelineRange([start, end]) in ranges {
        if *value1.get_time(0).unwrap().1 > start {
            value1.push_first(start, None);
        }
        if *value1.get_time(value1.len_time() - 1).unwrap().1 < end {
            value1.push_last(None, end);
        }
        let value1_start = upper_bound(global_start1..value1.len_time(), |i| *value1.get_time(i).unwrap().1 <= start);
        let value2_start = upper_bound(global_start2..value2.len_time(), |i| *value2.get_time(i).unwrap().1 <= start);
        let mut iter1 = (value1_start..).peekable();
        for value2_index in value2_start.. {
            match value2.get_value(value2_index).filter(|&(&left, ..)| left < end) {
                Some((&value2_left, Some(value2_value), &value2_right)) => {
                    let value2_left = value2_left.max(start);
                    let value2_right = value2_right.min(end);
                    let value1_index = {
                        let value1_index = iter1.next().unwrap();
                        let (value1_left, _, _) = value1.get_value(value1_index).unwrap();
                        match (value1_left).cmp(&value2_left) {
                            Ordering::Less => {
                                value1.split_value_by_clone(value1_index, value2_left);
                                value1_index + 1
                            }
                            Ordering::Greater => unreachable!(),
                            Ordering::Equal => value1_index,
                        }
                    };
                    let value1_right_index = upper_bound(value1_index..value1.len_time(), |i| *value1.get_time(i).unwrap().1 <= value2_right);
                    debug_assert!(*value1.get_time(value1_right_index).unwrap().1 <= value2_right);
                    let value1_right_index = if (*value1.get_time(value1_right_index).unwrap().1) < value2_right {
                        value1.split_value_by_clone(value1_right_index, value2_right);
                        value1_right_index + 1
                    } else {
                        value1_right_index
                    };
                    value1.merge_multiple_values(value1_index + 1..value1_right_index, Some(value2_value.clone()));
                }
                Some((_, None, _)) => {}
                None => {
                    global_start1 = *iter1.peek().unwrap();
                    global_start2 = global_start2.max(value2_index.saturating_sub(1));
                    break;
                }
            }
        }
    }
    value1
}

fn combine_params<Image: Clone + Send + Sync + 'static, Audio: Clone + Send + Sync + 'static>(value1: Parameter<ComponentProcessorInputBuffer<(), ()>>, value2: &Parameter<ComponentProcessorInputBufferRef<Image, Audio>>, ranges: &TimelineRangeSet) -> Parameter<ComponentProcessorInputBuffer<(), ()>> {
    match value1 {
        Parameter::None => {
            value2.as_none().unwrap();
            Parameter::None
        }
        Parameter::Image(_) => unreachable!(),
        Parameter::Audio(_) => unreachable!(),
        Parameter::Binary(value1) => Parameter::Binary(override_time_split_value(value1, value2.as_binary().unwrap(), ranges)),
        Parameter::String(value1) => Parameter::String(override_time_split_value(value1, value2.as_string().unwrap(), ranges)),
        Parameter::Integer(value1) => Parameter::Integer(override_time_split_value(value1, value2.as_integer().unwrap(), ranges)),
        Parameter::RealNumber(value1) => Parameter::RealNumber(override_time_split_value(value1, value2.as_real_number().unwrap(), ranges)),
        Parameter::Boolean(value1) => Parameter::Boolean(override_time_split_value(value1, value2.as_boolean().unwrap(), ranges)),
        Parameter::Dictionary(_) => unreachable!(),
        Parameter::Array(_) => unreachable!(),
        Parameter::ComponentClass(_) => Parameter::ComponentClass(()),
    }
}

pub trait CombinerBuilder<Data>: Send + Sync {
    type Request;
    type Param;
    type Combiner: Combiner<Data, Param = Self::Param>;
    fn new_combiner(&self, request: Self::Request) -> Self::Combiner;
}

pub trait Combiner<Data>: Send + Sync {
    type Param;
    fn add(&mut self, data: Data, param: Self::Param);
    fn collect(self) -> Data;
}

fn collect_dependencies<K, T: ParameterValueType>(component: &ComponentInstance<K, T>, required_type: &mut HashMap<StaticPointer<TCell<K, ComponentInstance<K, T>>>, HashSet<Parameter<ParameterSelect>>>) -> HashSet<StaticPointer<TCell<K, ComponentInstance<K, T>>>> {
    let dependencies = component
        .variable_parameters()
        .iter()
        .zip(component.variable_parameters_type())
        .flat_map(|(param, (_, ty))| {
            let slice = match param {
                VariableParameterValue::Manually(_) => &[],
                VariableParameterValue::MayComponent { components, .. } => components.as_slice(),
            };
            slice.iter().map(|component| (ty.select(), component))
        })
        .chain(component.image_required_params().into_iter().flat_map(|image_required_params| {
            let array = match &image_required_params.transform {
                ImageRequiredParamsTransform::Params {
                    scale,
                    translate,
                    rotate: _,
                    scale_center,
                    rotate_center,
                } => [scale, translate, scale_center, rotate_center],
                ImageRequiredParamsTransform::Free { left_top, right_top, left_bottom, right_bottom } => [left_top, right_top, left_bottom, right_bottom],
            };
            array.into_iter().flat_map(AsRef::<[_; 3]>::as_ref).flat_map(|param| {
                let slice = match param {
                    VariableParameterValue::Manually(_) => &[],
                    VariableParameterValue::MayComponent { components, .. } => components.as_slice(),
                };
                slice.iter().map(|component| (Parameter::RealNumber(()), component))
            })
        }))
        .chain(component.audio_required_params().into_iter().flat_map(|audio_required_params| {
            audio_required_params.volume.iter().flat_map(|param| {
                let slice = match param {
                    VariableParameterValue::Manually(_) => &[],
                    VariableParameterValue::MayComponent { params: _, components, priority: _ } => components.as_slice(),
                };
                slice.iter().map(|component| (Parameter::RealNumber(()), component))
            })
        }))
        .inspect(|&(ty, component)| {
            required_type.entry(component.clone()).or_default().insert(ty);
        })
        .map(|(_, component)| component.clone())
        .collect();
    dependencies
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct TimelineRange([TimelineTime; 2]);

impl Borrow<[TimelineTime; 2]> for TimelineRange {
    fn borrow(&self) -> &[TimelineTime; 2] {
        &self.0
    }
}

impl From<[TimelineTime; 2]> for TimelineRange {
    fn from(value: [TimelineTime; 2]) -> Self {
        TimelineRange(value)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct TimelineRangeSet(BTreeSet<TimelineRange>);

impl From<BTreeSet<TimelineRange>> for TimelineRangeSet {
    fn from(value: BTreeSet<TimelineRange>) -> Self {
        TimelineRangeSet(value)
    }
}

impl<const N: usize> From<[TimelineRange; N]> for TimelineRangeSet {
    fn from(value: [TimelineRange; N]) -> Self {
        TimelineRangeSet(value.into())
    }
}

impl TimelineRangeSet {
    fn contains(&self, at: TimelineTime) -> bool {
        self.0.range(..[at, TimelineTime::MAX]).next_back().map_or(false, |&TimelineRange([_, end])| at < end)
    }
}

fn get_time_range<K, T: ParameterValueType>(component: &ComponentInstance<K, T>, key: &TCellOwner<K>) -> TimelineRange {
    let left = component.marker_left().upgrade().unwrap().ro(key).cached_timeline_time();
    let right = component.marker_right().upgrade().unwrap().ro(key).cached_timeline_time();
    TimelineRange([left, right])
}

fn range_intersection(TimelineRange([start1, end1]): TimelineRange, TimelineRange([start2, end2]): TimelineRange) -> Option<TimelineRange> {
    let result = [start1.max(start2), end1.min(end2)];
    if result[0] < result[1] {
        Some(TimelineRange(result))
    } else {
        None
    }
}

fn range_subtract(TimelineRange(mut range): TimelineRange, TimelineRangeSet(already_used): &TimelineRangeSet) -> TimelineRangeSet {
    debug_assert!(range[0] < range[1]);
    if let Some(&TimelineRange([_, left])) = already_used.range(..range).next_back() {
        if range[1] <= left {
            return TimelineRangeSet(BTreeSet::new());
        } else if range[0] < left {
            range[0] = left;
        }
    }
    let mut ret = BTreeSet::new();
    for &TimelineRange([left, right]) in already_used.range([range[0]; 2]..[range[1]; 2]) {
        debug_assert!(range[0] <= left);
        if range[0] < left {
            ret.insert([range[0], left].into());
        }
        range[0] = right;
    }
    if range[0] < range[1] {
        ret.insert(range.into());
    }
    TimelineRangeSet(ret)
}

fn range_set_union(TimelineRangeSet(already_used): &mut TimelineRangeSet, TimelineRange(mut range): TimelineRange) {
    if let Some(&left_range @ TimelineRange([left, right])) = already_used.range(..range).next_back() {
        if range[0] <= right {
            let result = already_used.remove(&left_range);
            debug_assert!(result);
            range[0] = left;
            range[1] = range[1].max(right);
        }
    }
    let vec = already_used.range([range[0]; 2]..=[range[1], TimelineTime::MAX]).copied().collect::<Vec<_>>();
    if let Some(&TimelineRange([_, right])) = vec.last() {
        range[1] = range[1].max(right);
    }
    for range in vec {
        let result = already_used.remove(&range);
        debug_assert!(result);
    }
    already_used.insert(range.into());
}

struct FunctionArg<K: 'static>(TimelineTime, ParameterSelectValue, ImageSizeRequest, Arc<OwnedRwLockReadGuard<TCellOwner<K>>>);

impl<K> Clone for FunctionArg<K> {
    fn clone(&self) -> Self {
        let FunctionArg(at, ty, size, ref key) = *self;
        FunctionArg(at, ty, size, Arc::clone(key))
    }
}

impl<K> PartialEq for FunctionArg<K> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0 && self.1 == other.1 && self.2 == other.2
    }
}

impl<K> Eq for FunctionArg<K> {}

impl<K> Hash for FunctionArg<K> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
        self.1.hash(state);
        self.2.hash(state);
    }
}

struct Function<K: 'static, T: ParameterValueType> {
    function: FunctionByNeed<FunctionArg<K>, EvaluateComponentResult<K, T>>,
    ranges: TimelineRangeSet,
}

struct EvaluateAllComponent<K: 'static, T: ParameterValueType, Id> {
    functions: Vec<Function<K, T>>,
    phantom: PhantomData<Id>,
}

impl<K, T: ParameterValueType, Id: IdGenerator + 'static> EvaluateAllComponent<K, T, Id> {
    fn new<ImageCombinerBuilder: CombinerBuilder<T::Image, Request = ImageSizeRequest, Param = ImageRequiredParamsFixed> + 'static, AudioCombinerBuilder: CombinerBuilder<T::Audio, Request = (), Param = AudioRequiredParamsFrameVariable> + 'static>(
        components: Vec<StaticPointer<TCell<K, ComponentInstance<K, T>>>>,
        links: Vec<StaticPointer<TCell<K, MarkerLink<K>>>>,
        begin: StaticPointer<TCell<K, MarkerPin>>,
        end: StaticPointer<TCell<K, MarkerPin>>,
        frames: CloneableIterator<TimelineTime>,
        id_generator: Arc<Id>,
        image_combiner_builder: Arc<ImageCombinerBuilder>,
        audio_combiner_builder: Arc<AudioCombinerBuilder>,
        key: &TCellOwner<K>,
    ) -> Result<EvaluateAllComponent<K, T, Id>, EvaluateError<K, T>> {
        collect_cached_time(&components, &links, begin, end, key)?;
        let mut required_type_map = HashMap::new();
        let mut dependencies_map = HashMap::with_capacity(components.len());
        let mut time_range_map = HashMap::with_capacity(components.len());
        let components_strong_ref = components.iter().map(|component| component.upgrade().ok_or_else(|| EvaluateError::InvalidComponent(component.clone()))).collect::<Result<Vec<_>, _>>()?;
        let component_instances = components_strong_ref.iter().map(|component| component.ro(key)).collect::<Vec<_>>();
        let component_instance_map = components.iter().zip(component_instances.iter().map(Deref::deref)).collect::<HashMap<_, _>>();
        for component in &*components {
            let component_ref = component_instance_map.get(&component).unwrap();
            let dependencies = collect_dependencies(component_ref, &mut required_type_map);
            let time_range = get_time_range(component_ref, key);
            dependencies_map.insert(component, dependencies);
            time_range_map.insert(component.clone(), time_range);
        }
        let mut dependents_count = components.iter().map(|component| (component, 0usize)).collect::<HashMap<_, _>>();
        dependencies_map.values().flatten().for_each(|component| *dependents_count.get_mut(component).unwrap() += 1);
        let no_dependent_components = dependents_count.iter().filter_map(|(&component, &count)| (count == 0).then_some(component)).collect::<Vec<_>>();
        let mut q = VecDeque::from(no_dependent_components.clone());
        dependents_count.retain(|_, &mut count| count > 0);
        let mut sorted = Vec::with_capacity(components.len());
        while let Some(no_dependent_component) = q.pop_front() {
            sorted.push(no_dependent_component);
            for dependency in dependencies_map.get(no_dependent_component).unwrap() {
                let Entry::Occupied(mut entry) = dependents_count.entry(dependency) else { unreachable!() };
                let count = entry.get_mut();
                *count -= 1;
                if *count == 0 {
                    let (new_no_dependent, _) = entry.remove_entry();
                    q.push_back(new_no_dependent);
                }
            }
        }
        assert_eq!(q.len(), 0);
        if !dependents_count.is_empty() {
            return Err(EvaluateError::CycleDependency(dependents_count.into_keys().cloned().collect()));
        }
        let mut already_used_ranges = HashMap::with_capacity(components.len());
        let mut component_reference_ranges = HashMap::with_capacity(components.len());
        let mut temporary_set = HashSet::with_capacity(components.len());
        for component in &*components {
            let component_ref = component_instance_map.get(&component).unwrap();
            let dependencies = component_ref.variable_parameters().iter().zip(component_ref.variable_parameters_type()).flat_map(|(param, (_, _ty))| match param {
                VariableParameterValue::Manually(_) => &[][..],
                VariableParameterValue::MayComponent { components, .. } => components.as_slice(),
            });
            temporary_set.extend(dependencies);
            let mut reference_range = HashMap::with_capacity(temporary_set.len());
            for dependency in temporary_set.drain() {
                let dependency_range = range_intersection(*time_range_map.get(component).unwrap(), *time_range_map.get(dependency).unwrap());
                let dependency_range = if let Some(range) = dependency_range {
                    range
                } else {
                    let result = reference_range.insert(dependency.clone(), TimelineRangeSet::default());
                    debug_assert!(result.is_none());
                    continue;
                };
                let entry = already_used_ranges.entry(dependency);
                let dependency_range = match entry {
                    Entry::Occupied(entry) => {
                        let entry = entry.into_mut();
                        range_set_union(entry, dependency_range);
                        range_subtract(dependency_range, entry)
                    }
                    Entry::Vacant(entry) => {
                        let ranges = TimelineRangeSet::from(BTreeSet::from([dependency_range]));
                        entry.insert(ranges.clone());
                        ranges
                    }
                };
                let result = reference_range.insert(dependency.clone(), dependency_range);
                debug_assert!(result.is_none());
            }
            let result = component_reference_ranges.insert(component, reference_range);
            debug_assert!(result.is_none());
        }
        let map: Arc<DashMap<StaticPointer<TCell<K, ComponentInstance<K, T>>>, FunctionByNeed<FunctionArg<K>, EvaluateComponentResult<K, T>>>> = Arc::new(DashMap::new());
        for component in sorted {
            let component = component.clone();
            let map = Arc::clone(&map);
            let reference_range = Arc::new(component_reference_ranges.remove(&component).unwrap());
            map.insert(
                component.clone(),
                FunctionByNeed::new({
                    let evaluate_component = Arc::new(EvaluateComponent::new(
                        component.clone(),
                        frames.clone(),
                        ReferenceFunctions(Arc::clone(&map)),
                        Arc::clone(&reference_range),
                        Arc::clone(&id_generator),
                        Arc::clone(&image_combiner_builder),
                        Arc::clone(&audio_combiner_builder),
                    ));
                    // DynFnとDynFutureはhigher-ranked lifetime error(原因不明)回避のため
                    DynFn(Box::new(move |FunctionArg(time, ty, request, key)| Arc::clone(&evaluate_component).evaluate_boxed(time, ty, request, key)))
                }),
            );
        }
        let default_range = TimelineRangeSet(BTreeSet::new());
        let functions = components
            .iter()
            .map(|component| Function {
                function: map.get(component).unwrap().clone(),
                ranges: range_subtract(*time_range_map.get(component).unwrap(), already_used_ranges.get(component).unwrap_or(&default_range)),
            })
            .collect::<Vec<_>>();
        Ok(EvaluateAllComponent { functions, phantom: Default::default() })
    }

    async fn evaluate<ImageCombiner: Combiner<T::Image, Param = ImageRequiredParamsFixed> + 'static, AudioCombiner: Combiner<T::Audio, Param = AudioRequiredParamsFrameVariable> + 'static>(
        &self,
        at: TimelineTime,
        ty: ParameterSelectValue,
        image_size_request: ImageSizeRequest,
        image_combiner: Option<&mut ImageCombiner>,
        audio_combiner: Option<&mut AudioCombiner>,
        left: TimelineTime,
        right: TimelineTime,
        key: Arc<OwnedRwLockReadGuard<TCellOwner<K>>>,
    ) -> EvaluateAllComponentResult<K, T> {
        match ty.0 {
            Parameter::None => Ok(Parameter::None),
            Parameter::Image(_) => {
                let tasks = self
                    .functions
                    .iter()
                    .zip(iter::repeat(key))
                    .filter_map(|(Function { function, ranges }, key)| ranges.contains(at).then(|| tokio::spawn(function.call(FunctionArg(at, ty, image_size_request, key))).map(|result| result.unwrap())))
                    .collect::<Vec<_>>();
                let combiner = image_combiner.unwrap();
                for task in tasks {
                    let Some(result) = task.await? else {
                        continue;
                    };
                    let Parameter::Image((image, param)) = result else { unreachable!("ここの保証はevaluate_component側の責務") };
                    combiner.add(image, param);
                }
                Ok(Parameter::Image(()))
            }
            Parameter::Audio(_) => {
                let tasks = self
                    .functions
                    .iter()
                    .zip(iter::repeat(key))
                    .filter_map(|(Function { function, ranges }, key)| ranges.contains(at).then(|| tokio::spawn(function.call(FunctionArg(at, ty, image_size_request, key))).map(|result| result.unwrap())))
                    .collect::<Vec<_>>();
                let combiner = audio_combiner.unwrap();
                for task in tasks {
                    let Some(result) = task.await? else {
                        continue;
                    };
                    let Parameter::Audio((image, param)) = result else { unreachable!("ここの保証はevaluate_component側の責務") };
                    combiner.add(image, param);
                }
                Ok(Parameter::Audio(()))
            }
            Parameter::Binary(_) | Parameter::String(_) | Parameter::Integer(_) | Parameter::RealNumber(_) | Parameter::Boolean(_) | Parameter::Dictionary(_) | Parameter::Array(_) | Parameter::ComponentClass(_) => {
                let tasks = self
                    .functions
                    .iter()
                    .zip(iter::repeat(key))
                    .map(|(Function { function, ranges }, key)| tokio::spawn(function.call(FunctionArg(at, ty, image_size_request, key))).map(move |result| (result.unwrap(), ranges)))
                    .collect::<Vec<_>>();
                let mut buffer = empty_input_buffer(&ty.0, left, right);
                for task in tasks {
                    let (param, range) = task.await;
                    let Some(param) = param? else {
                        continue;
                    };
                    buffer = combine_params(buffer, &param, range);
                }
                let ret = change_type_parameter(buffer);
                Ok(ret)
            }
        }
    }
}

fn collect_cached_time<K, T>(_components: &[StaticPointer<TCell<K, ComponentInstance<K, T>>>], links: &[StaticPointer<TCell<K, MarkerLink<K>>>], begin: StaticPointer<TCell<K, MarkerPin>>, end: StaticPointer<TCell<K, MarkerPin>>, key: &TCellOwner<K>) -> Result<(), EvaluateError<K, T>> {
    let links = links.iter().filter_map(StaticPointer::upgrade).collect::<Vec<_>>();
    let mut links = links.iter().map(|link| link.ro(key)).collect::<HashSet<&MarkerLink<K>>>();
    let mut locked = HashSet::from([&begin, &end]);

    loop {
        let process = 'block: {
            for &link in &links {
                match (locked.contains(&link.from), locked.contains(&link.to)) {
                    (false, false) => {}
                    (true, false) => break 'block Some((link, &link.from, &link.to, link.len)),
                    (false, true) => break 'block Some((link, &link.to, &link.from, -link.len)),
                    (true, true) => return Err(EvaluateError::InvalidLinkGraph),
                }
            }
            None
        };
        let Some((link, from, to, len)) = process else {
            break;
        };
        links.remove(&link);
        locked.insert(to);
        let from = from.upgrade().ok_or_else(|| EvaluateError::InvalidMarker(from.clone()))?;
        let to = to.upgrade().ok_or_else(|| EvaluateError::InvalidMarker(to.clone()))?;
        let from = from.ro(key);
        let to = to.ro(key);
        to.cache_timeline_time(TimelineTime::new(from.cached_timeline_time().value() + len.value()).unwrap());
    }
    if links.is_empty() {
        Ok(())
    } else {
        Err(EvaluateError::InvalidLinkGraph)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ImageSizeRequest {
    pub width: f32,
    pub height: f32,
}

impl PartialEq for ImageSizeRequest {
    fn eq(&self, other: &Self) -> bool {
        self.width == other.width && self.height == other.height
    }
}

impl Eq for ImageSizeRequest {}

impl Hash for ImageSizeRequest {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.width.to_ne_bytes().hash(state);
        self.height.to_ne_bytes().hash(state);
    }
}

type EvaluateAllComponentResult<K, T> = Result<ComponentProcessorInputValueBuffer<(), ()>, EvaluateError<K, T>>;

type EvaluateComponentResult<K, T> = Result<Option<ComponentProcessorInputValueBufferRef<(<T as ParameterValueType>::Image, ImageRequiredParamsFixed), (<T as ParameterValueType>::Audio, AudioRequiredParamsFrameVariable)>>, EvaluateError<K, T>>;

#[derive(PartialEq, Eq, Clone, Copy)]
struct PlaceholderListItem {
    image: Option<Placeholder<TagImage>>,
    audio: Option<Placeholder<TagAudio>>,
}

struct ResultCache<Image: Send + Sync + Clone + 'static, Audio: Send + Sync + Clone + 'static>(ParameterAllValues<ValueCacheType<Image, Audio>>);

impl<Image: Send + Sync + Clone + 'static, Audio: Send + Sync + Clone + 'static> Default for ResultCache<Image, Audio> {
    fn default() -> Self {
        ResultCache(ParameterAllValues::default())
    }
}

struct EvaluateComponentCache<K: 'static, T: ParameterValueType, Id> {
    result_cache: ResultCache<T::Image, T::Audio>,
    map_time: OnceCell<Arc<MapTime>>,
    image_required_params: tokio::sync::OnceCell<Arc<Option<ImageRequiredParamsFrameVariable>>>,
    audio_required_params: tokio::sync::OnceCell<Arc<Option<AudioRequiredParamsFrameVariable>>>,
    result_components_renderer: OnceCell<(EvaluateAllComponent<K, T, Id>, Vec<StaticPointerCow<TCell<K, ComponentInstance<K, T>>>>, Vec<StaticPointerCow<TCell<K, MarkerLink<K>>>>)>,
    image_executable: OnceCell<NativeProcessorExecutable<T>>,
    placeholder_list: OnceCell<Vec<ArcSwapOption<PlaceholderListItem>>>,
}

impl<K, T: ParameterValueType, Id> Default for EvaluateComponentCache<K, T, Id> {
    fn default() -> Self {
        EvaluateComponentCache {
            result_cache: Default::default(),
            map_time: OnceCell::new(),
            image_required_params: tokio::sync::OnceCell::new(),
            audio_required_params: tokio::sync::OnceCell::new(),
            result_components_renderer: OnceCell::new(),
            image_executable: OnceCell::new(),
            placeholder_list: OnceCell::new(),
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Hash)]
struct ParameterSelectValue(Parameter<ParameterSelect>);

struct ReferenceFunctions<K: 'static, T: ParameterValueType>(Arc<DashMap<StaticPointer<TCell<K, ComponentInstance<K, T>>>, FunctionByNeed<FunctionArg<K>, EvaluateComponentResult<K, T>>>>);

impl<K, T: ParameterValueType> Clone for ReferenceFunctions<K, T> {
    fn clone(&self) -> Self {
        ReferenceFunctions(Arc::clone(&self.0))
    }
}

struct ImageGenerator<K: 'static, T: ParameterValueType, ImageCombinerBuilder> {
    reference_functions: ReferenceFunctions<K, T>,
    argument_reference_range: Arc<HashMap<StaticPointer<TCell<K, ComponentInstance<K, T>>>, TimelineRangeSet>>,
    image_combiner_builder: Arc<ImageCombinerBuilder>,
    components: Vec<StaticPointer<TCell<K, ComponentInstance<K, T>>>>,
    image_size_request: ImageSizeRequest,
    default_range: Arc<TimelineRangeSet>,
}

impl<K, T: ParameterValueType, ImageCombinerBuilder> Clone for ImageGenerator<K, T, ImageCombinerBuilder> {
    fn clone(&self) -> Self {
        let ImageGenerator {
            ref reference_functions,
            ref argument_reference_range,
            ref image_combiner_builder,
            ref components,
            image_size_request,
            ref default_range,
        } = *self;
        ImageGenerator {
            reference_functions: reference_functions.clone(),
            argument_reference_range: Arc::clone(argument_reference_range),
            image_combiner_builder: Arc::clone(image_combiner_builder),
            components: components.clone(),
            image_size_request,
            default_range: Arc::clone(default_range),
        }
    }
}

impl<K, T: ParameterValueType, ImageCombinerBuilder: CombinerBuilder<T::Image, Param = ImageRequiredParamsFixed, Request = ImageSizeRequest>> ImageGenerator<K, T, ImageCombinerBuilder> {
    fn new(
        reference_functions: ReferenceFunctions<K, T>,
        argument_reference_range: Arc<HashMap<StaticPointer<TCell<K, ComponentInstance<K, T>>>, TimelineRangeSet>>,
        image_combiner_builder: Arc<ImageCombinerBuilder>,
        components: Vec<StaticPointer<TCell<K, ComponentInstance<K, T>>>>,
        image_size_request: ImageSizeRequest,
        default_range: Arc<TimelineRangeSet>,
    ) -> Self {
        ImageGenerator {
            reference_functions,
            argument_reference_range,
            image_combiner_builder,
            components,
            image_size_request,
            default_range,
        }
    }
    async fn get(&self, at: TimelineTime, key: &Arc<OwnedRwLockReadGuard<TCellOwner<K>>>) -> Result<T::Image, EvaluateError<K, T>> {
        let tasks = self
            .components
            .iter()
            .filter_map(|component| {
                let range = self.argument_reference_range.get(component).unwrap_or(&self.default_range);
                range
                    .contains(at)
                    .then(|| tokio::spawn(self.reference_functions.0.get(component).unwrap().call(FunctionArg(at, ParameterSelectValue(Parameter::Image(())), self.image_size_request, Arc::clone(key)))).map(|result| result.unwrap()))
            })
            .collect::<Vec<_>>();
        let mut combiner = self.image_combiner_builder.new_combiner(self.image_size_request);
        for task in tasks {
            let Some(param) = task.await? else {
                continue;
            };
            if let Parameter::Image((image, param)) = param {
                combiner.add(image, param);
            } else {
                unreachable!()
            }
        }
        Ok::<_, EvaluateError<K, T>>(combiner.collect())
    }
}

struct GetParam<'a, K: 'static, T: ParameterValueType, ImageCombinerBuilder> {
    params: Vec<ParameterNativeProcessorInputFixed<T::Image, T::Audio>>,
    executable: &'a NativeProcessorExecutable<T>,
    image_map: &'a HashMap<Placeholder<TagImage>, ImageGenerator<K, T, ImageCombinerBuilder>>,
    audio_map: &'a HashMap<Placeholder<TagAudio>, T::Audio>,
}

impl<'a, K, T: ParameterValueType, ImageCombinerBuilder: CombinerBuilder<T::Image, Param = ImageRequiredParamsFixed, Request = ImageSizeRequest>> GetParam<'a, K, T, ImageCombinerBuilder> {
    fn new(executable: &'a NativeProcessorExecutable<T>, image_map: &'a HashMap<Placeholder<TagImage>, ImageGenerator<K, T, ImageCombinerBuilder>>, audio_map: &'a HashMap<Placeholder<TagAudio>, T::Audio>) -> Self {
        GetParam {
            params: vec![Parameter::None; executable.parameter.len()],
            executable,
            image_map,
            audio_map,
        }
    }
    async fn get(&mut self, at: TimelineTime, key: &Arc<OwnedRwLockReadGuard<TCellOwner<K>>>) -> Result<&[ParameterNativeProcessorInputFixed<T::Image, T::Audio>], EvaluateError<K, T>> {
        for (param_out, param_raw) in self.params.iter_mut().zip(self.executable.parameter.iter()) {
            *param_out = get_param_at(param_raw, self.image_map, self.audio_map, at, key).await?;
        }
        Ok::<_, EvaluateError<K, T>>(self.params.as_slice())
    }

    async fn process_all<U, Ret: From<BTreeMap<TimelineTime, U>>>(
        &mut self,
        frames: impl Iterator<Item = TimelineTime>,
        map: impl Fn(ParameterNativeProcessorInputFixed<T::Image, T::Audio>) -> Result<U, EvaluateError<K, T>>,
        key: &Arc<OwnedRwLockReadGuard<TCellOwner<K>>>,
    ) -> Result<Ret, EvaluateError<K, T>> {
        let mut buffer = Vec::with_capacity(frames.size_hint().0);
        for at in frames {
            buffer.push((at, map(self.executable.processor.process(at, self.get(at, key).await?))?));
        }
        Ok(BTreeMap::from_iter(buffer).into())
    }

    async fn get_time_split_value<U, Any>(
        mut self,
        frames: impl Iterator<Item = TimelineTime>,
        map: impl Fn(ParameterNativeProcessorInputFixed<T::Image, T::Audio>) -> Result<U, EvaluateError<K, T>>,
        left: TimelineTime,
        right: TimelineTime,
        key: &Arc<OwnedRwLockReadGuard<TCellOwner<K>>>,
    ) -> Result<TimeSplitValue<TimelineTime, Option<Either<Any, FrameVariableValue<U>>>>, EvaluateError<K, T>> {
        Ok(into_time_split_value(self.process_all(frames, map, key).await?, left, right))
    }

    async fn get_time_split_value_array<U: Copy, Any, V: AsRef<[U; N]>, Ret: From<[TimeSplitValue<TimelineTime, Option<Either<Any, FrameVariableValue<U>>>>; N]>, const N: usize>(
        mut self,
        frames: impl Iterator<Item = TimelineTime>,
        map: impl Fn(ParameterNativeProcessorInputFixed<T::Image, T::Audio>) -> Result<V, EvaluateError<K, T>>,
        left: TimelineTime,
        right: TimelineTime,
        key: &Arc<OwnedRwLockReadGuard<TCellOwner<K>>>,
    ) -> Result<Ret, EvaluateError<K, T>> {
        Ok(into_time_split_value_array(self.process_all(frames, map, key).await?, left, right))
    }
}

struct EvaluateComponent<K: 'static, T: ParameterValueType, ImageCombinerBuilder, AudioCombinerBuilder, Id> {
    cache_context: Arc<EvaluateComponentCache<K, T, Id>>,
    component: StaticPointer<TCell<K, ComponentInstance<K, T>>>,
    frames: CloneableIterator<TimelineTime>,
    reference_functions: ReferenceFunctions<K, T>,
    argument_reference_range: Arc<HashMap<StaticPointer<TCell<K, ComponentInstance<K, T>>>, TimelineRangeSet>>,
    id_generator: Arc<Id>,
    image_combiner_builder: Arc<ImageCombinerBuilder>,
    audio_combiner_builder: Arc<AudioCombinerBuilder>,
    default_range: OnceCell<Arc<TimelineRangeSet>>,
}

impl<
        K: 'static,
        T: ParameterValueType + 'static,
        ImageCombinerBuilder: CombinerBuilder<T::Image, Request = ImageSizeRequest, Param = ImageRequiredParamsFixed> + 'static,
        AudioCombinerBuilder: CombinerBuilder<T::Audio, Request = (), Param = AudioRequiredParamsFrameVariable> + 'static,
        Id: IdGenerator + 'static,
    > EvaluateComponent<K, T, ImageCombinerBuilder, AudioCombinerBuilder, Id>
{
    fn new(
        component: StaticPointer<TCell<K, ComponentInstance<K, T>>>,
        frames: CloneableIterator<TimelineTime>,
        reference_functions: ReferenceFunctions<K, T>,
        argument_reference_range: Arc<HashMap<StaticPointer<TCell<K, ComponentInstance<K, T>>>, TimelineRangeSet>>,
        id_generator: Arc<Id>,
        image_combiner_builder: Arc<ImageCombinerBuilder>,
        audio_combiner_builder: Arc<AudioCombinerBuilder>,
    ) -> EvaluateComponent<K, T, ImageCombinerBuilder, AudioCombinerBuilder, Id> {
        EvaluateComponent {
            cache_context: Arc::new(EvaluateComponentCache::default()),
            component,
            frames,
            reference_functions,
            argument_reference_range,
            id_generator,
            image_combiner_builder,
            audio_combiner_builder,
            default_range: OnceCell::new(),
        }
    }

    fn evaluate_boxed(self: Arc<Self>, at: TimelineTime, ty: ParameterSelectValue, image_size_request: ImageSizeRequest, key: Arc<OwnedRwLockReadGuard<TCellOwner<K>>>) -> DynFuture<'static, EvaluateComponentResult<K, T>> {
        DynFuture((self.evaluate(at, ty, image_size_request, key)).boxed())
    }

    fn evaluate(self: Arc<Self>, at: TimelineTime, ty: ParameterSelectValue, image_size_request: ImageSizeRequest, key_arc: Arc<OwnedRwLockReadGuard<TCellOwner<K>>>) -> impl Future<Output = EvaluateComponentResult<K, T>> + Send + 'static {
        check_in_cache(Arc::clone(&self.cache_context), at, ty).then(move |result| async move {
            let EvaluateComponent {
                cache_context,
                component,
                frames,
                reference_functions,
                argument_reference_range,
                id_generator,
                image_combiner_builder,
                audio_combiner_builder,
                default_range,
            } = &*self;
            if let Some(cached_result) = result {
                return cached_result;
            }
            let key: &TCellOwner<K> = &key_arc;
            let Some(component_ref) = component.upgrade() else {
                return Err(EvaluateError::InvalidComponent(component.clone()));
            };
            let component_ref = component_ref.ro(key);
            let marker_left = component_ref.marker_left().upgrade().unwrap();
            let marker_right = component_ref.marker_right().upgrade().unwrap();
            let marker_left = marker_left.ro(key);
            let marker_right = marker_right.ro(key);
            let left = marker_left.cached_timeline_time();
            let left_marker_time = marker_left.locked_component_time();
            let right = marker_right.cached_timeline_time();
            let right_marker_time = marker_right.locked_component_time();
            let default_range = default_range.get_or_init(|| Arc::new(TimelineRangeSet(BTreeSet::from([TimelineRange([left, right])]))));
            let parameters = evaluate_parameters(
                at,
                image_size_request,
                reference_functions,
                argument_reference_range,
                image_combiner_builder,
                audio_combiner_builder,
                component,
                component_ref,
                left,
                right,
                default_range,
                &key_arc,
            )
            .await;
            let map_time = Arc::clone(cache_context.map_time.get_or_init(|| {
                let times_iter = iter::once((left, left_marker_time.map(TimelineTime::from)))
                    .chain(component_ref.markers().iter().map(|marker| {
                        let marker = marker.ro(key);
                        (marker.cached_timeline_time(), marker.locked_component_time().map(TimelineTime::from))
                    }))
                    .chain(iter::once((right, right_marker_time.map(TimelineTime::from))));
                Arc::new(MapTime::new(remove_option_from_times(times_iter)))
            }));
            let frames = frames.clone();
            let processor = component_ref.processor();
            let processed = process(
                cache_context,
                frames.clone(),
                at,
                ty,
                image_size_request,
                id_generator,
                image_combiner_builder,
                audio_combiner_builder,
                component,
                component_ref,
                processor,
                left,
                right,
                parameters?,
                &map_time,
                &key_arc,
            );
            let image_required_params = acquire_image_required_param(cache_context, &frames, image_size_request, reference_functions, argument_reference_range, component_ref, left, right, default_range, &key_arc);
            let audio_required_params = acquire_audio_required_param(cache_context, &frames, image_size_request, reference_functions, argument_reference_range, component_ref, left, right, default_range, &key_arc);
            let (result, image_required_params, audio_required_params) = futures::try_join!(processed, image_required_params, audio_required_params)?;
            let create_error = |actual: Parameter<_>| EvaluateError::OutputTypeMismatch {
                component: component.clone(),
                expect: ty.0,
                actual: actual.select(),
            };
            match ty.0 {
                Parameter::None => Ok(Some(Parameter::None)),
                Parameter::Image(_) => {
                    {
                        cache_context
                            .result_cache
                            .0
                            .image
                            .write()
                            .map(|mut lock| {
                                // TODO: キャッシュ管理をもうちょっと賢くする
                                lock.retain(|&time, _| time == at);
                                let result = match lock.entry(at) {
                                    btree_map::Entry::Vacant(entry) => entry.insert(result.map(|result| Ok((result.into_image().map_err(create_error)?, (*image_required_params).as_ref().unwrap().get(at)))).transpose()?).clone(),
                                    btree_map::Entry::Occupied(entry) => entry.into_mut().clone(),
                                };
                                Ok(result.map(Parameter::Image))
                            })
                            .await
                    }
                }
                Parameter::Audio(_) => Ok(cache_context
                    .result_cache
                    .0
                    .audio
                    .get_or_try_init(|| result.map(|result| Ok((result.into_audio().map_err(create_error)?, (*audio_required_params).as_ref().cloned().unwrap()))).transpose())
                    .map(Clone::clone)?
                    .map(Parameter::Audio)),
                Parameter::Binary(_) => Ok(cache_context
                    .result_cache
                    .0
                    .binary
                    .get_or_try_init(|| result.map(|result| result.into_binary().map(Arc::new).map_err(create_error)).transpose())
                    .map(Clone::clone)?
                    .map(Parameter::Binary)),
                Parameter::String(_) => Ok(cache_context
                    .result_cache
                    .0
                    .string
                    .get_or_try_init(|| result.map(|result| result.into_string().map(Arc::new).map_err(create_error)).transpose())
                    .map(Clone::clone)?
                    .map(Parameter::String)),
                Parameter::Integer(_) => Ok(cache_context
                    .result_cache
                    .0
                    .integer
                    .get_or_try_init(|| result.map(|result| result.into_integer().map(Arc::new).map_err(create_error)).transpose())
                    .map(Clone::clone)?
                    .map(Parameter::Integer)),
                Parameter::RealNumber(_) => Ok(cache_context
                    .result_cache
                    .0
                    .real_number
                    .get_or_try_init(|| result.map(|result| result.into_real_number().map(Arc::new).map_err(create_error)).transpose())
                    .map(Clone::clone)?
                    .map(Parameter::RealNumber)),
                Parameter::Boolean(_) => Ok(cache_context
                    .result_cache
                    .0
                    .boolean
                    .get_or_try_init(|| result.map(|result| result.into_boolean().map(Arc::new).map_err(create_error)).transpose())
                    .map(Clone::clone)?
                    .map(Parameter::Boolean)),
                Parameter::Dictionary(_) => todo!(),
                Parameter::Array(_) => todo!(),
                Parameter::ComponentClass(_) => unreachable!(),
            }
        })
    }
}

fn process<
    'a,
    K,
    T: ParameterValueType + 'static,
    ImageCombinerBuilder: CombinerBuilder<T::Image, Request = ImageSizeRequest, Param = ImageRequiredParamsFixed> + 'static,
    AudioCombinerBuilder: CombinerBuilder<T::Audio, Request = (), Param = AudioRequiredParamsFrameVariable> + 'static,
    Id: IdGenerator + 'static,
>(
    cache_context: &'a Arc<EvaluateComponentCache<K, T, Id>>,
    frames: CloneableIterator<TimelineTime>,
    at: TimelineTime,
    ty: ParameterSelectValue,
    image_size_request: ImageSizeRequest,
    id_generator: &'a Arc<Id>,
    image_combiner_builder: &'a Arc<ImageCombinerBuilder>,
    audio_combiner_builder: &'a Arc<AudioCombinerBuilder>,
    component: &'a StaticPointer<TCell<K, ComponentInstance<K, T>>>,
    component_ref: &'a ComponentInstance<K, T>,
    processor: &'a Arc<dyn ComponentProcessor<K, T>>,
    left: TimelineTime,
    right: TimelineTime,
    parameters: Vec<ComponentProcessorInputValueBuffer<ImageGenerator<K, T, ImageCombinerBuilder>, T::Audio>>,
    map_time: &'a MapTime,
    key_arc: &'a Arc<OwnedRwLockReadGuard<TCellOwner<K>>>,
) -> impl Future<Output = Result<Option<ComponentProcessorInputValueBuffer<T::Image, T::Audio>>, EvaluateError<K, T>>> + Send + 'a
where
    T::Image: 'static,
    T::Audio: 'static,
{
    processor
        .natural_length(component_ref.fixed_parameters())
        .map(|natural_length| natural_length.as_secs_f64())
        .then(|natural_length| processor.get_processor().map(move |processor| (natural_length, processor)))
        .then(move |(natural_length, processor)| async move {
            let map_time_fn = move |t| MarkerTime::new(map_time.map_time(t).value().clamp(0., natural_length)).unwrap();
            match processor {
                ComponentProcessorBody::Component(processor) => {
                    let (all_component, _, _) = cache_context.result_components_renderer.get_or_try_init(move || {
                        let (components, links) = processor.build(
                            component_ref.fixed_parameters(),
                            &parameters.into_iter().map(into_component_processor_input_value).collect::<Vec<_>>(),
                            component_ref.variable_parameters_type(),
                            frames.clone().as_dyn_iterator(),
                            &map_time_fn,
                        );
                        let components_weak = components.iter().map(AsRef::as_ref).cloned().collect::<Vec<_>>();
                        let links_weak = links.iter().map(AsRef::as_ref).cloned().collect::<Vec<_>>();
                        let ret = EvaluateAllComponent::new(
                            components_weak,
                            links_weak,
                            component_ref.marker_left().reference(),
                            component_ref.marker_right().reference(),
                            frames,
                            Arc::clone(id_generator),
                            Arc::clone(image_combiner_builder),
                            Arc::clone(audio_combiner_builder),
                            key_arc,
                        )?;
                        Ok((ret, components, links))
                    })?;
                    let mut image_combiner = (ty == ParameterSelectValue(Parameter::Image(()))).then(|| image_combiner_builder.new_combiner(image_size_request));
                    let mut audio_combiner = (ty == ParameterSelectValue(Parameter::Audio(()))).then(|| audio_combiner_builder.new_combiner(()));
                    let result = all_component.evaluate(at, ty, image_size_request, image_combiner.as_mut(), audio_combiner.as_mut(), left, right, Arc::clone(key_arc)).await?;
                    Ok(Some(match result {
                        Parameter::None => ComponentProcessorInputValueBuffer::<T::Image, T::Audio>::None,
                        Parameter::Image(()) => ComponentProcessorInputValueBuffer::<T::Image, T::Audio>::Image(image_combiner.unwrap().collect()),
                        Parameter::Audio(()) => ComponentProcessorInputValueBuffer::<T::Image, T::Audio>::Audio(audio_combiner.unwrap().collect()),
                        Parameter::Binary(value) => ComponentProcessorInputValueBuffer::<T::Image, T::Audio>::Binary(value),
                        Parameter::String(value) => ComponentProcessorInputValueBuffer::<T::Image, T::Audio>::String(value),
                        Parameter::Integer(value) => ComponentProcessorInputValueBuffer::<T::Image, T::Audio>::Integer(value),
                        Parameter::RealNumber(value) => ComponentProcessorInputValueBuffer::<T::Image, T::Audio>::RealNumber(value),
                        Parameter::Boolean(value) => ComponentProcessorInputValueBuffer::<T::Image, T::Audio>::Boolean(value),
                        Parameter::Dictionary(value) => ComponentProcessorInputValueBuffer::<T::Image, T::Audio>::Dictionary(value),
                        Parameter::Array(value) => ComponentProcessorInputValueBuffer::<T::Image, T::Audio>::Array(value),
                        Parameter::ComponentClass(value) => ComponentProcessorInputValueBuffer::<T::Image, T::Audio>::ComponentClass(value),
                    }))
                }
                ComponentProcessorBody::Native(processors) => {
                    let Some(processor) = processors.iter().find(|processor| ParameterSelectValue(processor.output_type()) == ty) else {
                        return Ok(None);
                    };
                    let processor = Arc::clone(processor);
                    async move {
                        let placeholder_list = cache_context.placeholder_list.get_or_init(|| iter::from_fn(|| Some(ArcSwapOption::empty())).take(parameters.len()).collect());
                        let mut image_map = HashMap::new();
                        let mut audio_map = HashMap::new();
                        let generate_variable_parameters = || {
                            parameters
                                .into_iter()
                                .zip(placeholder_list)
                                .map(|(param, placeholder)| into_frame_variable_value(param, frames.clone(), placeholder, &mut image_map, &mut audio_map, &**id_generator))
                                .collect::<Vec<_>>()
                        };
                        let executable = if let ParameterSelectValue(Parameter::Image(())) = ty {
                            cache_context
                                .image_executable
                                .get_or_init(|| processor.build(component_ref.fixed_parameters(), &generate_variable_parameters(), component_ref.variable_parameters_type(), frames.clone().as_dyn_iterator(), &map_time_fn))
                                .clone()
                        } else {
                            processor.build(component_ref.fixed_parameters(), &generate_variable_parameters(), component_ref.variable_parameters_type(), frames.clone().as_dyn_iterator(), &map_time_fn)
                        };
                        let mut get_param = GetParam::new(&executable, &image_map, &audio_map);
                        match ty.0 {
                            Parameter::None => Ok(Some(Parameter::None)),
                            Parameter::Image(_) => Ok(Some(Parameter::Image(executable.processor.process(at, (get_param.get(at, key_arc)).await?).into_image().map_err(|actual| EvaluateError::OutputTypeMismatch {
                                component: component.clone(),
                                expect: Parameter::Image(()),
                                actual: actual.select(),
                            })?))),
                            Parameter::Audio(_) => Ok(Some(Parameter::Audio(executable.processor.process(at, (get_param.get(at, key_arc)).await?).into_audio().map_err(|actual| EvaluateError::OutputTypeMismatch {
                                component: component.clone(),
                                expect: Parameter::Image(()),
                                actual: actual.select(),
                            })?))),
                            Parameter::Binary(_) => Ok(Some(Parameter::Binary(
                                get_param
                                    .get_time_split_value(
                                        frames,
                                        |value| {
                                            Parameter::into_binary(value).map_err(|actual| EvaluateError::OutputTypeMismatch {
                                                component: component.clone(),
                                                expect: Parameter::Binary(()),
                                                actual: actual.select(),
                                            })
                                        },
                                        left,
                                        right,
                                        key_arc,
                                    )
                                    .await?,
                            ))),
                            Parameter::String(_) => Ok(Some(Parameter::String(
                                get_param
                                    .get_time_split_value(
                                        frames,
                                        |value| {
                                            Parameter::into_string(value).map_err(|actual| EvaluateError::OutputTypeMismatch {
                                                component: component.clone(),
                                                expect: Parameter::String(()),
                                                actual: actual.select(),
                                            })
                                        },
                                        left,
                                        right,
                                        key_arc,
                                    )
                                    .await?,
                            ))),
                            Parameter::Integer(_) => Ok(Some(Parameter::Integer(
                                get_param
                                    .get_time_split_value(
                                        frames,
                                        |value| {
                                            Parameter::into_integer(value).map_err(|actual| EvaluateError::OutputTypeMismatch {
                                                component: component.clone(),
                                                expect: Parameter::Integer(()),
                                                actual: actual.select(),
                                            })
                                        },
                                        left,
                                        right,
                                        key_arc,
                                    )
                                    .await?,
                            ))),
                            Parameter::RealNumber(_) => Ok(Some(Parameter::RealNumber(
                                get_param
                                    .get_time_split_value(
                                        frames,
                                        |value| {
                                            Parameter::into_real_number(value).map_err(|actual| EvaluateError::OutputTypeMismatch {
                                                component: component.clone(),
                                                expect: Parameter::RealNumber(()),
                                                actual: actual.select(),
                                            })
                                        },
                                        left,
                                        right,
                                        key_arc,
                                    )
                                    .await?,
                            ))),
                            Parameter::Boolean(_) => Ok(Some(Parameter::Boolean(
                                get_param
                                    .get_time_split_value(
                                        frames,
                                        |value| {
                                            Parameter::into_boolean(value).map_err(|actual| EvaluateError::OutputTypeMismatch {
                                                component: component.clone(),
                                                expect: Parameter::Boolean(()),
                                                actual: actual.select(),
                                            })
                                        },
                                        left,
                                        right,
                                        key_arc,
                                    )
                                    .await?,
                            ))),
                            Parameter::Dictionary(_) => todo!(),
                            Parameter::Array(_) => todo!(),
                            Parameter::ComponentClass(_) => Ok(Some(Parameter::ComponentClass(()))),
                        }
                    }
                    .await
                }
            }
        })
}

fn acquire_image_required_param<'a, K, T: ParameterValueType + 'static, Id: IdGenerator + 'static>(
    cache_context: &'a Arc<EvaluateComponentCache<K, T, Id>>,
    frames: &'a CloneableIterator<TimelineTime>,
    image_size_request: ImageSizeRequest,
    reference_functions: &'a ReferenceFunctions<K, T>,
    argument_reference_range: &'a Arc<HashMap<StaticPointer<TCell<K, ComponentInstance<K, T>>>, TimelineRangeSet>>,
    component_ref: &'a ComponentInstance<K, T>,
    left: TimelineTime,
    right: TimelineTime,
    default_range: &'a Arc<TimelineRangeSet>,
    key: &'a Arc<OwnedRwLockReadGuard<TCellOwner<K>>>,
) -> impl Future<Output = Result<Arc<Option<ImageRequiredParamsFrameVariable>>, EvaluateError<K, T>>> + Send + 'a {
    cache_context
        .image_required_params
        .get_or_try_init::<EvaluateError<K, T>, _, _>(move || async move {
            if let Some(&ImageRequiredParams {
                aspect_ratio,
                ref transform,
                background_color,
                ref opacity,
                ref blend_mode,
                ref composite_operation,
            }) = component_ref.image_required_params()
            {
                let transform = match transform {
                    ImageRequiredParamsTransform::Params { scale, translate, rotate, scale_center, rotate_center } => {
                        let rotate = as_frame_variable_value_easing(rotate, frames.clone(), |v| v, |_| Quaternion::one(), key);
                        let (scale, translate, scale_center, rotate_center) = futures::try_join!(
                            param_as_frame_variable_value_easing(scale.as_ref(), reference_functions, argument_reference_range, image_size_request, left, right, frames.clone(), From::from, |_| 1., default_range, key),
                            param_as_frame_variable_value_easing(translate.as_ref(), reference_functions, argument_reference_range, image_size_request, left, right, frames.clone(), From::from, |_| 0., default_range, key),
                            param_as_frame_variable_value_easing(scale_center.as_ref(), reference_functions, argument_reference_range, image_size_request, left, right, frames.clone(), From::from, |_| 0., default_range, key),
                            param_as_frame_variable_value_easing(rotate_center.as_ref(), reference_functions, argument_reference_range, image_size_request, left, right, frames.clone(), From::from, |_| 0., default_range, key),
                        )?;
                        ImageRequiredParamsTransformFrameVariable::Params { scale, translate, rotate, scale_center, rotate_center }
                    }
                    ImageRequiredParamsTransform::Free { left_top, right_top, left_bottom, right_bottom } => {
                        let (left_top, right_top, left_bottom, right_bottom) = futures::try_join!(
                            param_as_frame_variable_value_easing(left_top.as_ref(), reference_functions, argument_reference_range, image_size_request, left, right, frames.clone(), From::from, |_| 0., default_range, key),
                            param_as_frame_variable_value_easing(
                                right_top.as_ref(),
                                reference_functions,
                                argument_reference_range,
                                image_size_request,
                                left,
                                right,
                                frames.clone(),
                                From::from,
                                |i| [image_size_request.width as f64, 0., 0.][i],
                                default_range,
                                key
                            ),
                            param_as_frame_variable_value_easing(
                                left_bottom.as_ref(),
                                reference_functions,
                                argument_reference_range,
                                image_size_request,
                                left,
                                right,
                                frames.clone(),
                                From::from,
                                |i| [0., image_size_request.height as f64, 0.][i],
                                default_range,
                                key
                            ),
                            param_as_frame_variable_value_easing(
                                right_bottom.as_ref(),
                                reference_functions,
                                argument_reference_range,
                                image_size_request,
                                left,
                                right,
                                frames.clone(),
                                From::from,
                                |i| [image_size_request.width as f64, image_size_request.height as f64, 0.][i],
                                default_range,
                                key
                            ),
                        )?;
                        ImageRequiredParamsTransformFrameVariable::Free { left_top, right_top, left_bottom, right_bottom }
                    }
                };
                let opacity = as_frame_variable_value_easing(opacity, frames.clone(), |opacity| Opacity::new(opacity).unwrap(), |_| 1., key);
                let blend_mode = as_frame_variable_value(blend_mode, frames.clone(), key);
                let composite_operation = as_frame_variable_value(composite_operation, frames.clone(), key);
                Ok(Arc::new(Some(ImageRequiredParamsFrameVariable {
                    aspect_ratio,
                    transform,
                    background_color,
                    opacity,
                    blend_mode,
                    composite_operation,
                })))
            } else {
                Ok(Arc::new(None))
            }
        })
        .map_ok(Arc::clone)
}

fn acquire_audio_required_param<'a, K, T: ParameterValueType + 'static, Id: IdGenerator + 'static>(
    cache_context: &'a EvaluateComponentCache<K, T, Id>,
    frames: &'a CloneableIterator<TimelineTime>,
    image_size_request: ImageSizeRequest,
    reference_functions: &'a ReferenceFunctions<K, T>,
    argument_reference_range: &'a Arc<HashMap<StaticPointer<TCell<K, ComponentInstance<K, T>>>, TimelineRangeSet>>,
    component_ref: &'a ComponentInstance<K, T>,
    left: TimelineTime,
    right: TimelineTime,
    default_range: &'a Arc<TimelineRangeSet>,
    key: &'a Arc<OwnedRwLockReadGuard<TCellOwner<K>>>,
) -> impl Future<Output = Result<Arc<Option<AudioRequiredParamsFrameVariable>>, EvaluateError<K, T>>> + Send + 'a {
    cache_context
        .audio_required_params
        .get_or_try_init::<EvaluateError<K, T>, _, _>(move || {
            future::ready(component_ref.audio_required_params().ok_or(()))
                .and_then(move |AudioRequiredParams { volume }| {
                    stream::iter(volume.iter())
                        .then(move |param| param_as_frame_variable_value_easing(array::from_ref(param), reference_functions, argument_reference_range, image_size_request, left, right, frames.clone(), |[v]| v, |_| 1., default_range, key))
                        .try_collect()
                        .map(Ok)
                })
                .map_ok_or_else(|()| Ok(Arc::new(None)), |volume| Ok(Arc::new(Some(AudioRequiredParamsFrameVariable { volume: volume? }))))
        })
        .map_ok(Arc::clone)
}

fn evaluate_parameters<
    'a,
    K: 'static,
    T: ParameterValueType + 'static,
    ImageCombinerBuilder: CombinerBuilder<T::Image, Request = ImageSizeRequest, Param = ImageRequiredParamsFixed> + 'static,
    AudioCombinerBuilder: CombinerBuilder<T::Audio, Request = (), Param = AudioRequiredParamsFrameVariable> + 'static,
>(
    at: TimelineTime,
    image_size_request: ImageSizeRequest,
    reference_functions: &'a ReferenceFunctions<K, T>,
    argument_reference_range: &'a Arc<HashMap<StaticPointer<TCell<K, ComponentInstance<K, T>>>, TimelineRangeSet>>,
    image_combiner_builder: &'a Arc<ImageCombinerBuilder>,
    audio_combiner_builder: &'a Arc<AudioCombinerBuilder>,
    component: &'a StaticPointer<TCell<K, ComponentInstance<K, T>>>,
    component_ref: &'a ComponentInstance<K, T>,
    left: TimelineTime,
    right: TimelineTime,
    default_range: &'a Arc<TimelineRangeSet>,
    key: &'a Arc<OwnedRwLockReadGuard<TCellOwner<K>>>,
) -> impl Future<Output = Result<Vec<Parameter<ComponentProcessorInputBuffer<ImageGenerator<K, T, ImageCombinerBuilder>, <T as ParameterValueType>::Audio>>>, EvaluateError<K, T>>> + Send + 'a {
    stream::iter(component_ref.variable_parameters().iter().zip(component_ref.variable_parameters_type()).enumerate())
        .map(
            move |(index, (param, (_, ty)))| -> Result<JoinHandle<Result<ComponentProcessorInputValueBuffer<ImageGenerator<K, T, ImageCombinerBuilder>, T::Audio>, EvaluateError<K, T>>>, EvaluateError<K, T>> {
                match ty {
                    Parameter::Image(_) => {
                        if let VariableParameterValue::MayComponent {
                            params: Parameter::Image(Option::None),
                            components,
                            priority: _,
                        } = param
                        {
                            let reference_functions = reference_functions.clone();
                            let argument_reference_range = Arc::clone(argument_reference_range);
                            let image_combiner_builder = Arc::clone(image_combiner_builder);
                            let components = components.clone();
                            Ok(tokio::spawn(future::ready(Ok(ComponentProcessorInputValueBuffer::Image(ImageGenerator::new(
                                reference_functions,
                                argument_reference_range,
                                image_combiner_builder,
                                components,
                                image_size_request,
                                Arc::clone(default_range),
                            ))))))
                        } else {
                            Err(EvaluateError::InvalidVariableParameter { component: component.clone(), index })
                        }
                    }
                    Parameter::Audio(_) => {
                        if let VariableParameterValue::MayComponent {
                            params: Parameter::Audio(Option::None),
                            components,
                            priority: _,
                        } = param
                        {
                            let reference_functions = reference_functions.clone();
                            let components = components.clone();
                            let combiner = audio_combiner_builder.new_combiner(());
                            let key = Arc::clone(key);
                            Ok(tokio::spawn(
                                stream::iter(components)
                                    .map(move |component| {
                                        // let range = argument_reference_range.get(&component).unwrap();
                                        tokio::spawn(reference_functions.0.get(&component).unwrap().call(FunctionArg(at, ParameterSelectValue(Parameter::Audio(())), image_size_request, Arc::clone(&key)))).map(|result| result.unwrap())
                                    })
                                    .buffered(16)
                                    .try_fold(combiner, |mut combiner, param| {
                                        if let Some(param) = param {
                                            if let Parameter::Audio((image, param)) = param {
                                                combiner.add(image, param);
                                            } else {
                                                unreachable!()
                                            }
                                        }
                                        future::ready(Ok(combiner))
                                    })
                                    .map_ok(|combiner| ComponentProcessorInputValueBuffer::Audio(combiner.collect())),
                            ))
                        } else {
                            Err(EvaluateError::InvalidVariableParameter { component: component.clone(), index })
                        }
                    }
                    ty @ (Parameter::None | Parameter::Binary(_) | Parameter::String(_) | Parameter::Integer(_) | Parameter::RealNumber(_) | Parameter::Boolean(_) | Parameter::Dictionary(_) | Parameter::Array(_) | Parameter::ComponentClass(_)) => match param {
                        VariableParameterValue::Manually(param) => Ok(tokio::spawn(future::ready(Ok::<_, EvaluateError<K, T>>(change_type_parameter(value_into_processor_input_buffer(param.clone(), key)))))),
                        VariableParameterValue::MayComponent { params, components, priority } => {
                            let params = params.clone();
                            let reference_functions = reference_functions.clone();
                            let argument_reference_range = Arc::clone(argument_reference_range);
                            let ty = ty.select();
                            let tasks = stream::iter(components.clone())
                                .map({
                                    let key = Arc::clone(key);
                                    move |component| tokio::spawn(reference_functions.0.get(&component).unwrap().call(FunctionArg(at, ParameterSelectValue(ty), image_size_request, Arc::clone(&key)))).map(|result| result.unwrap().map(|result| (result, component)))
                                })
                                .buffered(16);
                            let default_range = Arc::clone(default_range);
                            match priority {
                                VariableParameterPriority::PrioritizeManually => Ok(tokio::spawn(
                                    tasks
                                        .try_fold(empty_input_buffer(&ty, left, right), move |buffer, (param, component)| {
                                            if let Some(param) = param {
                                                let range = argument_reference_range.get(&component).unwrap_or(&default_range);
                                                future::ready(Ok(combine_params(buffer, &param, range)))
                                            } else {
                                                future::ready(Ok(buffer))
                                            }
                                        })
                                        .map_ok({
                                            let key = Arc::clone(key);
                                            move |buffer| (buffer, nullable_into_processor_input_buffer_ref(params, &key))
                                        })
                                        .map_ok(move |(buffer, param)| combine_params(buffer, &param, &BTreeSet::from([TimelineRange::from([left, right])]).into()))
                                        .map_ok(change_type_parameter),
                                )),
                                VariableParameterPriority::PrioritizeComponent => Ok(tokio::spawn(
                                    tasks
                                        .try_fold(nullable_into_processor_input_buffer(params, key), move |buffer, (param, component)| {
                                            if let Some(param) = param {
                                                let range = argument_reference_range.get(&component).unwrap_or(&default_range);
                                                future::ready(Ok(combine_params(buffer, &param, range)))
                                            } else {
                                                future::ready(Ok(buffer))
                                            }
                                        })
                                        .map_ok(change_type_parameter),
                                )),
                            }
                        }
                    },
                }
            },
        )
        .map_ok(|task| task.map(Result::unwrap))
        .try_buffered(16)
        .try_collect::<Vec<_>>()
}

async fn check_in_cache<K, T: ParameterValueType + 'static, Id: IdGenerator + 'static>(cache_context: Arc<EvaluateComponentCache<K, T, Id>>, at: TimelineTime, ty: ParameterSelectValue) -> Option<EvaluateComponentResult<K, T>> {
    match ty.0 {
        Parameter::None => unreachable!(),
        Parameter::Image(_) => {
            let image = &cache_context.result_cache.0.image;
            let lock = (image.read()).await;
            if let Some(cache) = lock.get(&at) {
                return Some(Ok(cache.clone().map(Parameter::Image)));
            }
        }
        Parameter::Audio(_) => {
            if let Some(cache) = cache_context.result_cache.0.audio.get() {
                return Some(Ok(cache.clone().map(Parameter::Audio)));
            }
        }
        Parameter::Binary(_) => {
            if let Some(cache) = cache_context.result_cache.0.binary.get() {
                return Some(Ok(cache.clone().map(Parameter::Binary)));
            }
        }
        Parameter::String(_) => {
            if let Some(cache) = cache_context.result_cache.0.string.get() {
                return Some(Ok(cache.clone().map(Parameter::String)));
            }
        }
        Parameter::Integer(_) => {
            if let Some(cache) = cache_context.result_cache.0.integer.get() {
                return Some(Ok(cache.clone().map(Parameter::Integer)));
            }
        }
        Parameter::RealNumber(_) => {
            if let Some(cache) = cache_context.result_cache.0.real_number.get() {
                return Some(Ok(cache.clone().map(Parameter::RealNumber)));
            }
        }
        Parameter::Boolean(_) => {
            if let Some(cache) = cache_context.result_cache.0.boolean.get() {
                return Some(Ok(cache.clone().map(Parameter::Boolean)));
            }
        }
        Parameter::Dictionary(_) => todo!(),
        Parameter::Array(_) => todo!(),
        Parameter::ComponentClass(_) => unreachable!(),
    }
    None
}

async fn param_as_frame_variable_value_easing<'a, K, T: ParameterValueType, V, const N: usize>(
    value: &'a [VariableParameterValue<K, T, TimeSplitValue<StaticPointer<TCell<K, MarkerPin>>, EasingValue<f64>>, TimeSplitValue<StaticPointer<TCell<K, MarkerPin>>, Option<EasingValue<f64>>>>; N],
    reference_functions: &'a ReferenceFunctions<K, T>,
    argument_reference_range: &'a HashMap<StaticPointer<TCell<K, ComponentInstance<K, T>>>, TimelineRangeSet>,
    image_size_request: ImageSizeRequest,
    left: TimelineTime,
    right: TimelineTime,
    frames: impl Iterator<Item = TimelineTime> + Send + 'a,
    map: impl Fn([f64; N]) -> V + Send + 'a,
    default: impl Fn(usize) -> f64 + Send + 'a,
    default_range: &'a Arc<TimelineRangeSet>,
    key: &'a Arc<OwnedRwLockReadGuard<TCellOwner<K>>>,
) -> Result<FrameVariableValue<V>, EvaluateError<K, T>> {
    let value: [_; N] = (stream::iter(value.iter())
        .then(move |value| async move {
            match value {
                VariableParameterValue::Manually(value) => Ok(value.map_time_value_ref(
                    |t| {
                        let strong_ref = t.upgrade().unwrap();
                        strong_ref.ro(key).cached_timeline_time()
                    },
                    |v| (Some(Either::Left(v.clone()))),
                )),
                VariableParameterValue::MayComponent { params, components, priority } => {
                    let tasks = stream::iter(components.iter())
                        .map(move |component| {
                            tokio::spawn(reference_functions.0.get(component).unwrap().call(FunctionArg(TimelineTime::ZERO, ParameterSelectValue(Parameter::RealNumber(())), image_size_request, Arc::clone(key))))
                                .map(Result::unwrap)
                                .map_ok(move |result| (component, result))
                        })
                        .buffered(16)
                        .boxed();
                    match *priority {
                        VariableParameterPriority::PrioritizeManually => {
                            tasks
                                .try_fold(TimeSplitValue::new(left, None, right), |buffer, (component, param)| match param.map(Parameter::into_real_number) {
                                    None => future::ready(Ok(buffer)),
                                    Some(Ok(param)) => future::ready(Ok(override_time_split_value(buffer, &param, argument_reference_range.get(component).unwrap_or(default_range)))),
                                    Some(Err(param)) => future::ready(Err(EvaluateError::OutputTypeMismatch {
                                        component: component.clone(),
                                        expect: Parameter::RealNumber(()),
                                        actual: param.select(),
                                    })),
                                })
                                .map_ok(move |buffer| {
                                    let params = params.map_time_value_ref(
                                        |t| {
                                            let strong_ref = t.upgrade().unwrap();
                                            strong_ref.ro(key).cached_timeline_time()
                                        },
                                        |v| v.clone().map(Either::Left),
                                    );
                                    override_time_split_value(buffer, &params, &BTreeSet::from([[left, right].into()]).into())
                                })
                                .await
                        }
                        VariableParameterPriority::PrioritizeComponent => {
                            let buffer = params.map_time_value_ref(
                                |t| {
                                    let strong_ref = t.upgrade().unwrap();
                                    strong_ref.ro(key).cached_timeline_time()
                                },
                                |v| v.clone().map(Either::Left),
                            );
                            tasks
                                .try_fold(buffer, |buffer, (component, param)| match param.map(Parameter::into_real_number) {
                                    None => future::ready(Ok(buffer)),
                                    Some(Ok(param)) => future::ready(Ok(override_time_split_value(buffer, &param, argument_reference_range.get(component).unwrap_or(default_range)))),
                                    Some(Err(param)) => future::ready(Err(EvaluateError::OutputTypeMismatch {
                                        component: component.clone(),
                                        expect: Parameter::RealNumber(()),
                                        actual: param.select(),
                                    })),
                                })
                                .await
                        }
                    }
                }
            }
        })
        .try_collect::<Vec<_>>())
    .await?
    .try_into()
    .unwrap();
    Ok(FrameValuesEasing::new(value).collect(frames, map, default))
}

fn as_frame_variable_value_easing<K, U: Copy, V>(value: &TimeSplitValue<StaticPointer<TCell<K, MarkerPin>>, EasingValue<U>>, frames: impl Iterator<Item = TimelineTime>, map: impl Fn(U) -> V, default: impl Fn(usize) -> U, key: &TCellOwner<K>) -> FrameVariableValue<V> {
    let value = value.map_time_value_ref(
        |t| {
            let strong_ref = t.upgrade().unwrap();
            strong_ref.ro(key).cached_timeline_time()
        },
        |v| Some(Either::Left(v.clone())),
    );
    FrameValuesEasing::new([value]).collect(frames, |[v]| map(v), default)
}

fn as_frame_variable_value<K, U: Copy + Default>(value: &TimeSplitValue<StaticPointer<TCell<K, MarkerPin>>, U>, frames: impl Iterator<Item = TimelineTime>, key: &TCellOwner<K>) -> FrameVariableValue<U> {
    let value = value.map_time_value_ref(
        |t| {
            let strong_ref = t.upgrade().unwrap();
            strong_ref.ro(key).cached_timeline_time()
        },
        |v| Some(Either::Left(*v)),
    );
    FrameValues::new([value]).collect(frames, |[v]| v)
}

fn into_time_split_value<T, Any>(value: FrameVariableValue<T>, left: TimelineTime, right: TimelineTime) -> TimeSplitValue<TimelineTime, Option<Either<Any, FrameVariableValue<T>>>> {
    TimeSplitValue::new(left, Some(Either::Right(value)), right)
}

fn into_time_split_value_array<T: Copy, Any, Ret: From<[TimeSplitValue<TimelineTime, Option<Either<Any, FrameVariableValue<T>>>>; N]>, const N: usize>(value: FrameVariableValue<impl AsRef<[T; N]>>, left: TimelineTime, right: TimelineTime) -> Ret {
    array::from_fn(|i| TimeSplitValue::new(left, Some(Either::Right(value.map_ref(|array| array.as_ref()[i]))), right)).into()
}

async fn get_param_at<K, T: ParameterValueType>(
    param: &Parameter<NativeProcessorInput>,
    image_map: &HashMap<Placeholder<TagImage>, ImageGenerator<K, T, impl CombinerBuilder<T::Image, Param = ImageRequiredParamsFixed, Request = ImageSizeRequest>>>,
    audio_map: &HashMap<Placeholder<TagAudio>, T::Audio>,
    at: TimelineTime,
    key: &Arc<OwnedRwLockReadGuard<TCellOwner<K>>>,
) -> Result<ParameterNativeProcessorInputFixed<T::Image, T::Audio>, EvaluateError<K, T>> {
    let result = match param {
        Parameter::None => Parameter::None,
        Parameter::Image(image_placeholder) => Parameter::Image(image_map.get(image_placeholder).unwrap().get(at, key).await?),
        Parameter::Audio(audio_placeholder) => Parameter::Audio(audio_map.get(audio_placeholder).unwrap().clone()),
        Parameter::Binary(value) => Parameter::Binary(value.get(at).unwrap().clone()),
        Parameter::String(value) => Parameter::String(value.get(at).unwrap().clone()),
        Parameter::Integer(value) => Parameter::Integer(*value.get(at).unwrap()),
        Parameter::RealNumber(value) => Parameter::RealNumber(*value.get(at).unwrap()),
        Parameter::Boolean(value) => Parameter::Boolean(*value.get(at).unwrap()),
        Parameter::Dictionary(value) => {
            let _: &Never = value;
            unreachable!()
        }
        Parameter::Array(value) => {
            let _: &Never = value;
            unreachable!()
        }
        Parameter::ComponentClass(value) => {
            let _: &Never = value;
            unreachable!()
        }
    };
    Ok(result)
}

struct FrameValues<T, const N: usize> {
    value: [TimeSplitValue<TimelineTime, Option<Either<T, FrameVariableValue<T>>>>; N],
    index_buffer: [Option<usize>; N],
}

impl<T: Default + Clone, const N: usize> FrameValues<T, N> {
    fn new(value: [TimeSplitValue<TimelineTime, Option<Either<T, FrameVariableValue<T>>>>; N]) -> FrameValues<T, N> {
        FrameValues { value, index_buffer: [None; N] }
    }

    fn next(&mut self, time: TimelineTime) -> Option<[T; N]> {
        self.value
            .iter()
            .zip(self.index_buffer.iter_mut())
            .filter_map(|(value, index)| {
                let value = if let Some(index) = index.as_mut() {
                    loop {
                        let (&left, value, &right) = value.get_value(*index)?;
                        if left <= time && time < right {
                            break value;
                        }
                        *index += 1;
                    }
                } else {
                    let i = upper_bound(0..value.len_time(), |i| *value.get_time(i).unwrap().1 <= time);
                    *index = Some(i);
                    value.get_value(i)?.1
                };
                match value {
                    None => Some(T::default()),
                    Some(Either::Left(value)) => Some(value.clone()),
                    Some(Either::Right(value)) => Some(value.get(time).unwrap().clone()),
                }
            })
            .collect::<ArrayVec<_, N>>()
            .into_inner()
            .ok()
    }

    fn collect<U>(&mut self, frames: impl Iterator<Item = TimelineTime>, map: impl Fn([T; N]) -> U) -> FrameVariableValue<U> {
        frames.filter_map(|time| self.next(time).map(|value| (time, map(value)))).collect::<BTreeMap<_, _>>().into()
    }
}

struct FrameValuesEasing<T, const N: usize> {
    value: [TimeSplitValue<TimelineTime, Option<Either<EasingValue<T>, FrameVariableValue<T>>>>; N],
    index_buffer: [Option<usize>; N],
}

impl<T: Clone, const N: usize> FrameValuesEasing<T, N> {
    fn new(value: [TimeSplitValue<TimelineTime, Option<Either<EasingValue<T>, FrameVariableValue<T>>>>; N]) -> FrameValuesEasing<T, N> {
        FrameValuesEasing { value, index_buffer: [None; N] }
    }

    fn next(&mut self, time: TimelineTime, default: impl Fn(usize) -> T) -> Option<[T; N]> {
        self.value
            .iter()
            .zip(self.index_buffer.iter_mut())
            .enumerate()
            .filter_map(|(j, (value, index))| {
                let (left, value, right) = if let Some(index) = index.as_mut() {
                    loop {
                        let value @ (&left, _, &right) = value.get_value(*index)?;
                        if left <= time && time < right {
                            break value;
                        }
                        *index += 1;
                    }
                } else {
                    let i = upper_bound(0..value.len_time(), |i| *value.get_time(i).unwrap().1 <= time);
                    *index = Some(i);
                    value.get_value(i)?
                };
                match value {
                    None => Some(default(j)),
                    Some(Either::Left(value)) => {
                        #[allow(clippy::float_equality_without_abs)] // left < rightなので
                        let p = if right.value() - left.value() < f64::EPSILON { 0. } else { (time.value() - left.value()) / (right.value() - left.value()) };
                        Some(value.easing.easing(&value.from, &value.to, p))
                    }
                    Some(Either::Right(value)) => Some(value.get(time).unwrap().clone()),
                }
            })
            .collect::<ArrayVec<_, N>>()
            .into_inner()
            .ok()
    }

    fn collect<U>(&mut self, frames: impl Iterator<Item = TimelineTime>, map: impl Fn([T; N]) -> U, default: impl Fn(usize) -> T) -> FrameVariableValue<U> {
        frames.filter_map(|time| self.next(time, &default).map(|value| (time, map(value)))).collect::<BTreeMap<_, _>>().into()
    }
}

// image_map,audio_mapに登録したPlaceholderのIdと、それがどのパラメータから参照されているかという情報の保存をやらないといけない
fn into_frame_variable_value<Image: Clone + Send + Sync + 'static, Audio: Clone + Send + Sync + 'static>(
    param: ComponentProcessorInputValueBuffer<Image, Audio>,
    frames: impl Iterator<Item = TimelineTime>,
    id: &ArcSwapOption<PlaceholderListItem>,
    image_map: &mut HashMap<Placeholder<TagImage>, Image>,
    audio_map: &mut HashMap<Placeholder<TagAudio>, Audio>,
    id_generator: &impl IdGenerator,
) -> ParameterFrameVariableValue {
    //ここのframesは単調増加性を仮定したい
    match param {
        Parameter::None => Parameter::None,
        Parameter::Image(image) => {
            let id = id.load().as_deref().copied().unwrap_or_else(|| {
                let new_id = PlaceholderListItem {
                    image: Some(Placeholder::new(id_generator)),
                    audio: None,
                };
                id.compare_and_swap(&None::<Arc<_>>, Some(Arc::new(new_id))).as_deref().copied().unwrap_or(new_id)
            });
            let id = id.image.unwrap();
            image_map.insert(id, image);
            Parameter::Image(id)
        }
        Parameter::Audio(audio) => {
            let id = id.load().as_deref().copied().unwrap_or_else(|| {
                let new_id = PlaceholderListItem {
                    image: None,
                    audio: Some(Placeholder::new(id_generator)),
                };
                id.compare_and_swap(&None::<Arc<_>>, Some(Arc::new(new_id))).as_deref().copied().unwrap_or(new_id)
            });
            let id = id.audio.unwrap();
            audio_map.insert(id, audio);
            Parameter::Audio(id)
        }
        Parameter::Binary(value) => Parameter::Binary(FrameValues::new([value]).collect(frames, |[v]| v)),
        Parameter::String(value) => Parameter::String(FrameValues::new([value]).collect(frames, |[v]| v)),
        Parameter::Integer(value) => Parameter::Integer(FrameValues::new([value]).collect(frames, |[v]| v)),
        Parameter::RealNumber(value) => Parameter::RealNumber(FrameValuesEasing::new([value]).collect(frames, |[v]| v, |_| 0.)),
        Parameter::Boolean(value) => Parameter::Boolean(FrameValues::new([value]).collect(frames, |[v]| v)),
        Parameter::Dictionary(_value) => todo!(),
        Parameter::Array(_value) => todo!(),
        Parameter::ComponentClass(()) => todo!(),
    }
}

fn into_component_processor_input_value<Image: Clone + Send + Sync + 'static, Audio: Clone + Send + Sync + 'static>(param: ComponentProcessorInputValueBuffer<Image, Audio>) -> ComponentProcessorInputValue {
    match param {
        Parameter::None => Parameter::None,
        Parameter::Image(_) => Parameter::Image(todo!()),
        Parameter::Audio(_) => Parameter::Audio(todo!()),
        Parameter::Binary(value) => Parameter::Binary(value),
        Parameter::String(value) => Parameter::String(value),
        Parameter::Integer(value) => Parameter::Integer(value),
        Parameter::RealNumber(value) => Parameter::RealNumber(value),
        Parameter::Boolean(value) => Parameter::Boolean(value),
        Parameter::Dictionary(value) => Parameter::Dictionary(value),
        Parameter::Array(value) => Parameter::Array(value),
        Parameter::ComponentClass(value) => Parameter::ComponentClass(value),
    }
}

fn remove_option_from_times(data: impl IntoIterator<Item = (TimelineTime, Option<TimelineTime>)>) -> Vec<(TimelineTime, TimelineTime)> {
    let data = data.into_iter();
    let mut buffer = Vec::with_capacity(data.size_hint().0);
    let mut data = data.peekable();
    while let Some((time, locked)) = data.next() {
        if buffer.is_empty() || locked.is_some() || data.peek().is_none() {
            buffer.push((time, locked));
        }
    }
    let locked_count = buffer.iter().filter(|(_, option)| option.is_some()).count();
    match locked_count {
        0 => {
            if let [(left_cached, ref mut left_locked), (right_cached, ref mut right_locked)] = *buffer.as_mut_slice() {
                *left_locked = Some(TimelineTime::ZERO);
                *right_locked = Some(TimelineTime::new(right_cached.value() - left_cached.value()).unwrap());
            } else {
                unreachable!()
            }
        }
        1 => {
            if let [(left_cached, ref mut left_locked @ None), (right_cached, Some(right_locked)), ..] = *buffer.as_mut_slice() {
                *left_locked = Some(TimelineTime::new(right_locked.value() - (right_cached.value() - left_cached.value())).unwrap());
                if buffer.len() > 2 {
                    buffer.remove(1);
                }
            }
            if let [ref head @ .., (ref mut left_cached, Some(ref mut left_locked)), (right_cached, ref mut right_locked @ None)] = *buffer.as_mut_slice() {
                if !head.is_empty() {
                    *left_locked = TimelineTime::new(left_locked.value() + (right_cached.value() - left_cached.value())).unwrap();
                    *left_cached = right_cached;
                    buffer.pop().unwrap();
                } else {
                    *right_locked = Some(TimelineTime::new(left_locked.value() + (right_cached.value() - left_cached.value())).unwrap());
                }
            }
        }
        _ => {
            if let [(left_cached, ref mut left_locked @ None), (center_cached, Some(center_locked)), (right_cached, Some(right_locked)), ..] = *buffer.as_mut_slice() {
                let marker_time_per_timeline_time = (right_locked.value() - center_locked.value()) / (right_cached.value() - center_cached.value());
                *left_locked = Some(TimelineTime::new(center_locked.value() - (center_cached.value() - left_cached.value()) * marker_time_per_timeline_time).unwrap());
                buffer.remove(1);
            }
            if let [.., (left_cached, Some(left_locked)), (ref mut center_cached, Some(ref mut center_locked)), (right_cached, Option::None)] = *buffer.as_mut_slice() {
                let marker_time_per_timeline_time = (center_locked.value() - left_locked.value()) / (center_cached.value() - left_cached.value());
                *center_locked = TimelineTime::new(center_locked.value() + (right_cached.value() - center_cached.value()) * marker_time_per_timeline_time).unwrap();
                *center_cached = right_cached;
                buffer.pop().unwrap();
            }
        }
    }
    buffer.into_iter().map(|(a, b)| (a, b.unwrap())).collect()
}

struct MapTime {
    data: Vec<(TimelineTime, TimelineTime)>,
}

impl MapTime {
    fn new(data: impl Into<Vec<(TimelineTime, TimelineTime)>>) -> MapTime {
        let data = data.into();
        assert!(data.len() >= 2);
        MapTime { data }
    }

    fn map_time(&self, at: TimelineTime) -> TimelineTime {
        let index = self.data.partition_point(|&(time, _)| time <= at);
        let index = index.saturating_sub(1).min(self.data.len() - 2);
        let [(left_timeline, left_marker), (right_timeline, right_marker)]: [_; 2] = self.data[index..][..2].try_into().unwrap();
        let p = (at.value() - left_timeline.value()) / (right_timeline.value() - left_timeline.value());
        TimelineTime::new(left_marker.value() + (right_marker.value() - left_marker.value()) * p).unwrap()
    }
}

fn change_type_parameter<Image1: Clone + Send + Sync + 'static, Image2: Clone + Send + Sync + 'static, Audio1: Clone + Send + Sync + 'static, Audio2: Clone + Send + Sync + 'static>(parameter: ComponentProcessorInputValueBuffer<Image1, Audio1>) -> ComponentProcessorInputValueBuffer<Image2, Audio2> {
    match parameter {
        ComponentProcessorInputValueBuffer::None => Parameter::None,
        ComponentProcessorInputValueBuffer::Image(_) => unreachable!(),
        ComponentProcessorInputValueBuffer::Audio(_) => unreachable!(),
        ComponentProcessorInputValueBuffer::Binary(value) => Parameter::Binary(value),
        ComponentProcessorInputValueBuffer::String(value) => Parameter::String(value),
        ComponentProcessorInputValueBuffer::Integer(value) => Parameter::Integer(value),
        ComponentProcessorInputValueBuffer::RealNumber(value) => Parameter::RealNumber(value),
        ComponentProcessorInputValueBuffer::Boolean(value) => Parameter::Boolean(value),
        ComponentProcessorInputValueBuffer::Dictionary(value) => Parameter::Dictionary(value),
        ComponentProcessorInputValueBuffer::Array(value) => Parameter::Array(value),
        ComponentProcessorInputValueBuffer::ComponentClass(value) => Parameter::ComponentClass(value),
    }
}

#[cfg(test)]
mod tests {
    use mpdelta_core::component::parameter::value::LinearEasing;
    use mpdelta_core::time_split_value;

    use super::*;

    macro_rules! time {
        ($value:expr) => {
            TimelineTime::new($value as f64).unwrap()
        };
        ($left:expr, $right:expr) => {
            TimelineRange([time!($left), time!($right)])
        };
    }

    #[test]
    fn test_range_intersection() {
        assert_eq!(range_intersection(time![-2, -1], time![0, 1]), None);
        assert_eq!(range_intersection(time![-1, 0], time![0, 1]), None);
        assert_eq!(range_intersection(time![-1, 0.5], time![0, 1]), Some(time![0, 0.5]));
        assert_eq!(range_intersection(time![0, 1], time![0, 1]), Some(time![0, 1]));
        assert_eq!(range_intersection(time![0.5, 2], time![0, 1]), Some(time![0.5, 1]));
        assert_eq!(range_intersection(time![1, 2], time![0, 1]), None);
        assert_eq!(range_intersection(time![2, 3], time![0, 1]), None);

        assert_eq!(range_intersection(time![0, 1], time![-2, -1]), None);
        assert_eq!(range_intersection(time![0, 1], time![-1, 0]), None);
        assert_eq!(range_intersection(time![0, 1], time![-1, 0.5]), Some(time![0, 0.5]));
        assert_eq!(range_intersection(time![0, 1], time![0, 1]), Some(time![0, 1]));
        assert_eq!(range_intersection(time![0, 1], time![0.5, 2]), Some(time![0.5, 1]));
        assert_eq!(range_intersection(time![0, 1], time![1, 2]), None);
        assert_eq!(range_intersection(time![0, 1], time![2, 3]), None);
    }

    #[test]
    fn test_range_subtract() {
        assert_eq!(range_subtract(time![0, 10], &[].into()), [time![0, 10]].into());

        assert_eq!(range_subtract(time![0, 10], &[time![-2, -1]].into()), [time![0, 10]].into());
        assert_eq!(range_subtract(time![0, 10], &[time![-1, 0]].into()), [time![0, 10]].into());
        assert_eq!(range_subtract(time![0, 10], &[time![-1, 1]].into()), [time![1, 10]].into());
        assert_eq!(range_subtract(time![0, 10], &[time![0, 1]].into()), [time![1, 10]].into());
        assert_eq!(range_subtract(time![0, 10], &[time![1, 2]].into()), [time![0, 1], time![2, 10]].into());
        assert_eq!(range_subtract(time![0, 10], &[time![9, 10]].into()), [time![0, 9]].into());
        assert_eq!(range_subtract(time![0, 10], &[time![10, 11]].into()), [time![0, 10]].into());
        assert_eq!(range_subtract(time![0, 10], &[time![0, 10]].into()), [].into());
        assert_eq!(range_subtract(time![0, 10], &[time![-1, 10]].into()), [].into());
        assert_eq!(range_subtract(time![0, 10], &[time![0, 11]].into()), [].into());
        assert_eq!(range_subtract(time![0, 10], &[time![-1, 11]].into()), [].into());

        assert_eq!(range_subtract(time![0, 10], &[time![-3, -2], time![-1.5, -0.5]].into()), [time![0, 10]].into());
        assert_eq!(range_subtract(time![0, 10], &[time![-3, -2], time![-1, 0]].into()), [time![0, 10]].into());
        assert_eq!(range_subtract(time![0, 10], &[time![-3, -2], time![-1, 1]].into()), [time![1, 10]].into());
        assert_eq!(range_subtract(time![0, 10], &[time![-3, -2], time![0, 1]].into()), [time![1, 10]].into());
        assert_eq!(range_subtract(time![0, 10], &[time![-3, -2], time![1, 2]].into()), [time![0, 1], time![2, 10]].into());
        assert_eq!(range_subtract(time![0, 10], &[time![-3, -2], time![9, 10]].into()), [time![0, 9]].into());
        assert_eq!(range_subtract(time![0, 10], &[time![-3, -2], time![9, 11]].into()), [time![0, 9]].into());
        assert_eq!(range_subtract(time![0, 10], &[time![-3, -2], time![10, 11]].into()), [time![0, 10]].into());
        assert_eq!(range_subtract(time![0, 10], &[time![-3, -2], time![11, 12]].into()), [time![0, 10]].into());

        assert_eq!(range_subtract(time![0, 10], &[time![-1, 0], time![1, 2]].into()), [time![0, 1], time![2, 10]].into());
        assert_eq!(range_subtract(time![0, 10], &[time![-1, 0], time![9, 10]].into()), [time![0, 9]].into());
        assert_eq!(range_subtract(time![0, 10], &[time![-1, 0], time![9, 11]].into()), [time![0, 9]].into());
        assert_eq!(range_subtract(time![0, 10], &[time![-1, 0], time![10, 11]].into()), [time![0, 10]].into());
        assert_eq!(range_subtract(time![0, 10], &[time![-1, 0], time![11, 12]].into()), [time![0, 10]].into());

        assert_eq!(range_subtract(time![0, 10], &[time![-1, 1], time![2, 3]].into()), [time![1, 2], time![3, 10]].into());
        assert_eq!(range_subtract(time![0, 10], &[time![-1, 1], time![9, 10]].into()), [time![1, 9]].into());
        assert_eq!(range_subtract(time![0, 10], &[time![-1, 1], time![9, 11]].into()), [time![1, 9]].into());
        assert_eq!(range_subtract(time![0, 10], &[time![-1, 1], time![10, 11]].into()), [time![1, 10]].into());
        assert_eq!(range_subtract(time![0, 10], &[time![-1, 1], time![11, 12]].into()), [time![1, 10]].into());

        assert_eq!(range_subtract(time![0, 10], &[time![0, 1], time![2, 3]].into()), [time![1, 2], time![3, 10]].into());
        assert_eq!(range_subtract(time![0, 10], &[time![0, 1], time![9, 10]].into()), [time![1, 9]].into());
        assert_eq!(range_subtract(time![0, 10], &[time![0, 1], time![9, 11]].into()), [time![1, 9]].into());
        assert_eq!(range_subtract(time![0, 10], &[time![0, 1], time![10, 11]].into()), [time![1, 10]].into());
        assert_eq!(range_subtract(time![0, 10], &[time![0, 1], time![11, 12]].into()), [time![1, 10]].into());

        assert_eq!(range_subtract(time![0, 10], &[time![1, 2], time![3, 4]].into()), [time![0, 1], time![2, 3], time![4, 10]].into());
        assert_eq!(range_subtract(time![0, 10], &[time![1, 2], time![9, 10]].into()), [time![0, 1], time![2, 9]].into());
        assert_eq!(range_subtract(time![0, 10], &[time![1, 2], time![9, 11]].into()), [time![0, 1], time![2, 9]].into());
        assert_eq!(range_subtract(time![0, 10], &[time![1, 2], time![10, 11]].into()), [time![0, 1], time![2, 10]].into());
        assert_eq!(range_subtract(time![0, 10], &[time![1, 2], time![11, 12]].into()), [time![0, 1], time![2, 10]].into());

        assert_eq!(range_subtract(time![0, 10], &[time![9, 10], time![11, 12]].into()), [time![0, 9]].into());

        assert_eq!(range_subtract(time![0, 10], &[time![9, 11], time![12, 13]].into()), [time![0, 9]].into());

        assert_eq!(range_subtract(time![0, 10], &[time![10, 11], time![12, 13]].into()), [time![0, 10]].into());

        assert_eq!(range_subtract(time![0, 10], &[time![10.5, 11.5], time![12, 13]].into()), [time![0, 10]].into());
    }

    #[test]
    fn test_range_set_union() {
        fn range_set_union_for_test(range: TimelineRange, mut already_used: TimelineRangeSet) -> TimelineRangeSet {
            range_set_union(&mut already_used, range);
            already_used
        }
        assert_eq!(range_set_union_for_test(time![0, 10], [].into()), [time![0, 10]].into());

        assert_eq!(range_set_union_for_test(time![0, 10], [time![-2, -1]].into()), [time![-2, -1], time![0, 10]].into());
        assert_eq!(range_set_union_for_test(time![0, 10], [time![-1, 0]].into()), [time![-1, 10]].into());
        assert_eq!(range_set_union_for_test(time![0, 10], [time![-1, 1]].into()), [time![-1, 10]].into());
        assert_eq!(range_set_union_for_test(time![0, 10], [time![0, 1]].into()), [time![0, 10]].into());
        assert_eq!(range_set_union_for_test(time![0, 10], [time![1, 2]].into()), [time![0, 10]].into());
        assert_eq!(range_set_union_for_test(time![0, 10], [time![9, 10]].into()), [time![0, 10]].into());
        assert_eq!(range_set_union_for_test(time![0, 10], [time![10, 11]].into()), [time![0, 11]].into());
        assert_eq!(range_set_union_for_test(time![0, 10], [time![0, 10]].into()), [time![0, 10]].into());
        assert_eq!(range_set_union_for_test(time![0, 10], [time![-1, 10]].into()), [time![-1, 10]].into());
        assert_eq!(range_set_union_for_test(time![0, 10], [time![0, 11]].into()), [time![0, 11]].into());
        assert_eq!(range_set_union_for_test(time![0, 10], [time![-1, 11]].into()), [time![-1, 11]].into());

        assert_eq!(range_set_union_for_test(time![0, 10], [time![-3, -2], time![-1.5, -0.5]].into()), [time![-3, -2], time![-1.5, -0.5], time![0, 10]].into());
        assert_eq!(range_set_union_for_test(time![0, 10], [time![-3, -2], time![-1, 0]].into()), [time![-3, -2], time![-1, 10]].into());
        assert_eq!(range_set_union_for_test(time![0, 10], [time![-3, -2], time![-1, 1]].into()), [time![-3, -2], time![-1, 10]].into());
        assert_eq!(range_set_union_for_test(time![0, 10], [time![-3, -2], time![0, 1]].into()), [time![-3, -2], time![0, 10]].into());
        assert_eq!(range_set_union_for_test(time![0, 10], [time![-3, -2], time![1, 2]].into()), [time![-3, -2], time![0, 10]].into());
        assert_eq!(range_set_union_for_test(time![0, 10], [time![-3, -2], time![9, 10]].into()), [time![-3, -2], time![0, 10]].into());
        assert_eq!(range_set_union_for_test(time![0, 10], [time![-3, -2], time![9, 11]].into()), [time![-3, -2], time![0, 11]].into());
        assert_eq!(range_set_union_for_test(time![0, 10], [time![-3, -2], time![10, 11]].into()), [time![-3, -2], time![0, 11]].into());
        assert_eq!(range_set_union_for_test(time![0, 10], [time![-3, -2], time![11, 12]].into()), [time![-3, -2], time![0, 10], time![11, 12]].into());

        assert_eq!(range_set_union_for_test(time![0, 10], [time![-1, 0], time![1, 2]].into()), [time![-1, 10]].into());
        assert_eq!(range_set_union_for_test(time![0, 10], [time![-1, 0], time![9, 10]].into()), [time![-1, 10]].into());
        assert_eq!(range_set_union_for_test(time![0, 10], [time![-1, 0], time![9, 11]].into()), [time![-1, 11]].into());
        assert_eq!(range_set_union_for_test(time![0, 10], [time![-1, 0], time![10, 11]].into()), [time![-1, 11]].into());
        assert_eq!(range_set_union_for_test(time![0, 10], [time![-1, 0], time![11, 12]].into()), [time![-1, 10], time![11, 12]].into());

        assert_eq!(range_set_union_for_test(time![0, 10], [time![-1, 1], time![2, 3]].into()), [time![-1, 10]].into());
        assert_eq!(range_set_union_for_test(time![0, 10], [time![-1, 1], time![9, 10]].into()), [time![-1, 10]].into());
        assert_eq!(range_set_union_for_test(time![0, 10], [time![-1, 1], time![9, 11]].into()), [time![-1, 11]].into());
        assert_eq!(range_set_union_for_test(time![0, 10], [time![-1, 1], time![10, 11]].into()), [time![-1, 11]].into());
        assert_eq!(range_set_union_for_test(time![0, 10], [time![-1, 1], time![11, 12]].into()), [time![-1, 10], time![11, 12]].into());

        assert_eq!(range_set_union_for_test(time![0, 10], [time![0, 1], time![2, 3]].into()), [time![0, 10]].into());
        assert_eq!(range_set_union_for_test(time![0, 10], [time![0, 1], time![9, 10]].into()), [time![0, 10]].into());
        assert_eq!(range_set_union_for_test(time![0, 10], [time![0, 1], time![9, 11]].into()), [time![0, 11]].into());
        assert_eq!(range_set_union_for_test(time![0, 10], [time![0, 1], time![10, 11]].into()), [time![0, 11]].into());
        assert_eq!(range_set_union_for_test(time![0, 10], [time![0, 1], time![11, 12]].into()), [time![0, 10], time![11, 12]].into());

        assert_eq!(range_set_union_for_test(time![0, 10], [time![1, 2], time![3, 4]].into()), [time![0, 10]].into());
        assert_eq!(range_set_union_for_test(time![0, 10], [time![1, 2], time![9, 10]].into()), [time![0, 10]].into());
        assert_eq!(range_set_union_for_test(time![0, 10], [time![1, 2], time![9, 11]].into()), [time![0, 11]].into());
        assert_eq!(range_set_union_for_test(time![0, 10], [time![1, 2], time![10, 11]].into()), [time![0, 11]].into());
        assert_eq!(range_set_union_for_test(time![0, 10], [time![1, 2], time![11, 12]].into()), [time![0, 10], time![11, 12]].into());

        assert_eq!(range_set_union_for_test(time![0, 10], [time![9, 10], time![11, 12]].into()), [time![0, 10], time![11, 12]].into());

        assert_eq!(range_set_union_for_test(time![0, 10], [time![9, 11], time![12, 13]].into()), [time![0, 11], time![12, 13]].into());

        assert_eq!(range_set_union_for_test(time![0, 10], [time![10, 11], time![12, 13]].into()), [time![0, 11], time![12, 13]].into());

        assert_eq!(range_set_union_for_test(time![0, 10], [time![10.5, 11.5], time![12, 13]].into()), [time![0, 10], time![10.5, 11.5], time![12, 13]].into());
    }

    #[test]
    fn test_upper_bound() {
        assert_eq!(upper_bound(0..10, |i| [0, 1, 2, 3, 4, 5, 6, 7, 8, 9][i] < 5), 4);
        assert_eq!(upper_bound(0..10, |i| [0, 0, 1, 1, 2, 2, 3, 3, 4, 4][i] < 2), 3);
        assert_eq!(upper_bound(0..10, |i| [0, 0, 1, 1, 2, 2, 3, 3, 4, 4][i] < 10), 9);
        assert_eq!(upper_bound(0..0, |_| true), 0);
    }

    #[test]
    fn test_override_time_split_value() {
        assert_eq!(
            override_time_split_value(time_split_value![time!(0), Some('a'), time!(1), Some('b'), time!(2)], &time_split_value![time!(0), Some('a'), time!(2)], &[].into()),
            time_split_value![time!(0), Some('a'), time!(1), Some('b'), time!(2)]
        );
        assert_eq!(
            override_time_split_value(time_split_value![time!(0), Some('a'), time!(1), Some('b'), time!(2)], &time_split_value![time!(0), Some('c'), time!(2)], &[time![0.5, 1.5]].into()),
            time_split_value![time!(0), Some('a'), time!(0.5), Some('c'), time!(1.5), Some('b'), time!(2)]
        );
        assert_eq!(
            override_time_split_value(time_split_value![time!(0), Some('a'), time!(1), Some('b'), time!(2)], &time_split_value![time!(0), Some('c'), time!(2)], &[time![0.5, 1]].into()),
            time_split_value![time!(0), Some('a'), time!(0.5), Some('c'), time!(1), Some('b'), time!(2)]
        );
        assert_eq!(
            override_time_split_value(time_split_value![time!(0), Some('a'), time!(1), Some('b'), time!(2)], &time_split_value![time!(0), Some('c'), time!(2)], &[time![1, 1.5]].into()),
            time_split_value![time!(0), Some('a'), time!(1), Some('c'), time!(1.5), Some('b'), time!(2)]
        );
        assert_eq!(
            override_time_split_value(time_split_value![time!(0), Some('a'), time!(1), Some('b'), time!(2)], &time_split_value![time!(0), None, time!(0.3), Some('c'), time!(0.7), None, time!(2)], &[time![0, 2]].into()),
            time_split_value![time!(0), Some('a'), time!(0.3), Some('c'), time!(0.7), Some('a'), time!(1), Some('b'), time!(2)]
        );
        assert_eq!(
            override_time_split_value(
                time_split_value![time!(0), Some('a'), time!(1), Some('b'), time!(2)],
                &time_split_value![time!(0), None, time!(0.3), Some('c'), time!(0.7), None, time!(2)],
                &[time![0, 1], time![1.5, 2]].into()
            ),
            time_split_value![time!(0), Some('a'), time!(0.3), Some('c'), time!(0.7), Some('a'), time!(1), Some('b'), time!(2)]
        );
        assert_eq!(
            override_time_split_value(
                time_split_value![time!(0), Some('a'), time!(1), Some('b'), time!(2)],
                &time_split_value![time!(0), None, time!(0.3), Some('c'), time!(0.7), None, time!(1.3), Some('d'), time!(1.7), None, time!(2)],
                &[time![0, 1], time![1.5, 2]].into(),
            ),
            time_split_value![time!(0), Some('a'), time!(0.3), Some('c'), time!(0.7), Some('a'), time!(1), Some('b'), time!(1.5), Some('d'), time!(1.7), Some('b'), time!(2)]
        );
        assert_eq!(
            override_time_split_value(time_split_value![time!(0), Some('a'), time!(2)], &time_split_value![time!(0), Some('b'), time!(2)], &[time![0, 0.5], time![1.5, 2]].into()),
            time_split_value![time!(0), Some('b'), time!(0.5), Some('a'), time!(1.5), Some('b'), time!(2)]
        );
        assert_eq!(
            override_time_split_value(time_split_value![time!(0), Some('a'), time!(2)], &time_split_value![time!(0), None, time!(0.5), Some('b'), time!(1.5), None, time!(2)], &[time![0, 2]].into()),
            time_split_value![time!(0), Some('a'), time!(0.5), Some('b'), time!(1.5), Some('a'), time!(2)]
        );
        assert_eq!(
            override_time_split_value(time_split_value![time!(0), Some('a'), time!(1)], &time_split_value![time!(0), None, time!(0.5), Some('b'), time!(1.5), None, time!(2)], &[time![0, 2]].into()),
            time_split_value![time!(0), Some('a'), time!(0.5), Some('b'), time!(1.5), None, time!(2)]
        );
        assert_eq!(
            override_time_split_value(time_split_value![time!(1), Some('a'), time!(2)], &time_split_value![time!(0), None, time!(0.5), Some('b'), time!(1.5), None, time!(2)], &[time![0, 2]].into()),
            time_split_value![time!(0), None, time!(0.5), Some('b'), time!(1.5), Some('a'), time!(2)]
        );
        assert_eq!(
            override_time_split_value(time_split_value![time!(0), Some('a'), time!(2)], &time_split_value![time!(0), None, time!(0.5), Some('b'), time!(1)], &[time![0, 2]].into()),
            time_split_value![time!(0), Some('a'), time!(0.5), Some('b'), time!(1), Some('a'), time!(2)]
        );
        assert_eq!(
            override_time_split_value(time_split_value![time!(0), Some('a'), time!(2)], &time_split_value![time!(1), Some('b'), time!(1.5), None, time!(2)], &[time![0, 2]].into()),
            time_split_value![time!(0), Some('a'), time!(1), Some('b'), time!(1.5), Some('a'), time!(2)]
        );
    }

    #[test]
    fn test_map_time() {
        let map_time = MapTime::new(remove_option_from_times([(time!(10), None), (time!(20), None)]));
        assert_eq!(map_time.map_time(time!(10)), time!(0));
        assert_eq!(map_time.map_time(time!(11)), time!(1));
        assert_eq!(map_time.map_time(time!(20)), time!(10));

        let map_time = MapTime::new(remove_option_from_times([(time!(10), None), (time!(12), Some(time!(5))), (time!(20), None)]));
        assert_eq!(map_time.map_time(time!(10)), time!(3));
        assert_eq!(map_time.map_time(time!(11)), time!(4));
        assert_eq!(map_time.map_time(time!(20)), time!(13));

        let map_time = MapTime::new(remove_option_from_times([(time!(12), Some(time!(5))), (time!(20), None)]));
        assert_eq!(map_time.map_time(time!(10)), time!(3));
        assert_eq!(map_time.map_time(time!(11)), time!(4));
        assert_eq!(map_time.map_time(time!(20)), time!(13));

        let map_time = MapTime::new(remove_option_from_times([(time!(10), None), (time!(12), Some(time!(5)))]));
        assert_eq!(map_time.map_time(time!(10)), time!(3));
        assert_eq!(map_time.map_time(time!(11)), time!(4));
        assert_eq!(map_time.map_time(time!(20)), time!(13));

        let map_time = MapTime::new(remove_option_from_times([(time!(10), None), (time!(12), Some(time!(5))), (time!(18), Some(time!(8))), (time!(20), None)]));
        assert_eq!(map_time.map_time(time!(10)), time!(4));
        assert_eq!(map_time.map_time(time!(11)), time!(4.5));
        assert_eq!(map_time.map_time(time!(20)), time!(9));

        let map_time = MapTime::new(remove_option_from_times([(time!(10), None), (time!(12), Some(time!(5))), (time!(15), Some(time!(6.5))), (time!(18), Some(time!(9.5))), (time!(20), None)]));
        assert_eq!(map_time.map_time(time!(10)), time!(4));
        assert_eq!(map_time.map_time(time!(11)), time!(4.5));
        assert_eq!(map_time.map_time(time!(15)), time!(6.5));
        assert_eq!(map_time.map_time(time!(19)), time!(10.5));
        assert_eq!(map_time.map_time(time!(20)), time!(11.5));
    }

    #[test]
    fn test_frame_values() {
        assert_eq!(
            FrameValues::new([time_split_value![time!(0), Some(Either::Left(0)), time!(1), Some(Either::Left(1)), time!(2), Some(Either::Left(2)), time!(3)]]).collect([].into_iter(), |[v]| v),
            FrameVariableValue::from(BTreeMap::from([]))
        );
        assert_eq!(
            FrameValues::new([time_split_value![time!(0), Some(Either::Left(0)), time!(1), Some(Either::Left(1)), time!(2), Some(Either::Left(2)), time!(3)]]).collect([time!(0.5), time!(1.5), time!(2.5)].into_iter(), |[v]| v),
            FrameVariableValue::from(BTreeMap::from([(time!(0.5), 0), (time!(1.5), 1), (time!(2.5), 2)]))
        );
        assert_eq!(
            FrameValues::new([time_split_value![time!(0), Some(Either::Left(0)), time!(1), Some(Either::Left(1)), time!(2), Some(Either::Left(2)), time!(3)]]).collect([time!(0.2), time!(0.7), time!(1.2), time!(1.7), time!(2.2), time!(2.7)].into_iter(), |[v]| v),
            FrameVariableValue::from(BTreeMap::from([(time!(0.2), 0), (time!(0.7), 0), (time!(1.2), 1), (time!(1.7), 1), (time!(2.2), 2), (time!(2.7), 2)]))
        );
        assert_eq!(
            FrameValues::new([time_split_value![time!(0), Some(Either::Left(0)), time!(1), Some(Either::Left(1)), time!(2), Some(Either::Left(2)), time!(3)]]).collect([time!(0.5), time!(2.5)].into_iter(), |[v]| v),
            FrameVariableValue::from(BTreeMap::from([(time!(0.5), 0), (time!(2.5), 2)]))
        );

        assert_eq!(
            FrameValuesEasing::new([time_split_value![
                time!(0),
                Some(Either::Left(EasingValue { from: 0., to: 1., easing: Arc::new(LinearEasing) })),
                time!(1),
                Some(Either::Left(EasingValue { from: 1., to: 2., easing: Arc::new(LinearEasing) })),
                time!(2),
                Some(Either::Left(EasingValue { from: 2., to: 3., easing: Arc::new(LinearEasing) })),
                time!(3)
            ]])
            .collect([].into_iter(), |[v]| v, |_| 100.),
            FrameVariableValue::from(BTreeMap::from([]))
        );
        assert_eq!(
            FrameValuesEasing::new([time_split_value![
                time!(0),
                Some(Either::Left(EasingValue { from: 0., to: 1., easing: Arc::new(LinearEasing) })),
                time!(1),
                Some(Either::Left(EasingValue { from: 1., to: 2., easing: Arc::new(LinearEasing) })),
                time!(2),
                Some(Either::Left(EasingValue { from: 2., to: 3., easing: Arc::new(LinearEasing) })),
                time!(3)
            ]])
            .collect([time!(0.5), time!(1.5), time!(2.5)].into_iter(), |[v]| v, |_| 100.),
            FrameVariableValue::from(BTreeMap::from([(time!(0.5), 0.5), (time!(1.5), 1.5), (time!(2.5), 2.5)]))
        );
        assert_eq!(
            FrameValuesEasing::new([time_split_value![
                time!(0),
                Some(Either::Left(EasingValue { from: 0., to: 1., easing: Arc::new(LinearEasing) })),
                time!(1),
                Some(Either::Left(EasingValue { from: 1., to: 2., easing: Arc::new(LinearEasing) })),
                time!(2),
                Some(Either::Left(EasingValue { from: 2., to: 3., easing: Arc::new(LinearEasing) })),
                time!(3)
            ]])
            .collect([time!(0.2), time!(0.7), time!(1.2), time!(1.7), time!(2.2), time!(2.7)].into_iter(), |[v]| v, |_| 100.),
            FrameVariableValue::from(BTreeMap::from([(time!(0.2), 0.2), (time!(0.7), 0.7), (time!(1.2), 1.2), (time!(1.7), 1.7), (time!(2.2), 2.2), (time!(2.7), 2.7)]))
        );
        assert_eq!(
            FrameValuesEasing::new([time_split_value![
                time!(0),
                Some(Either::Left(EasingValue { from: 0., to: 1., easing: Arc::new(LinearEasing) })),
                time!(1),
                Some(Either::Left(EasingValue { from: 1., to: 2., easing: Arc::new(LinearEasing) })),
                time!(2),
                Some(Either::Left(EasingValue { from: 2., to: 3., easing: Arc::new(LinearEasing) })),
                time!(3)
            ]])
            .collect([time!(0.5), time!(2.5)].into_iter(), |[v]| v, |_| 100.),
            FrameVariableValue::from(BTreeMap::from([(time!(0.5), 0.5), (time!(2.5), 2.5)]))
        );
    }
}
