use crate::{stream, StreamExt};
use async_recursion::async_recursion;
use cgmath::{One, Quaternion, Vector2, Vector3};
use dashmap::DashMap;
use either::Either;
use mpdelta_core::common::time_split_value::TimeSplitValue;
use mpdelta_core::component::instance::ComponentInstance;
use mpdelta_core::component::link::MarkerLink;
use mpdelta_core::component::marker_pin::{MarkerPin, MarkerTime};
use mpdelta_core::component::parameter::placeholder::{Placeholder, TagAudio, TagImage};
use mpdelta_core::component::parameter::value::{DefaultEasing, EasingValue, FrameVariableValue};
use mpdelta_core::component::parameter::{
    AudioRequiredParams, ComponentProcessorInputValue, ImageRequiredParams, ImageRequiredParamsFrameVariable, ImageRequiredParamsTransform, ImageRequiredParamsTransformFrameVariable, Opacity, Parameter, ParameterFrameVariableValue, ParameterNullableValue, ParameterType,
    ParameterTypeExceptComponentClass, ParameterValue, ParameterValueType, VariableParameterPriority, VariableParameterValue,
};
use mpdelta_core::component::processor::{ComponentProcessorBody, NativeProcessorExecutable};
use mpdelta_core::core::IdGenerator;
use mpdelta_core::native::processor::ParameterNativeProcessorInputFixed;
use mpdelta_core::ptr::{StaticPointer, StaticPointerOwned};
use mpdelta_core::time::TimelineTime;
use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::iter::{Peekable, SkipWhile, TakeWhile};
use std::ops::{Deref, Range};
use std::path::PathBuf;
use std::sync::Arc;
use std::{array, iter, mem};
use tokio::sync::RwLock;

pub struct ComponentProcessorInputBuffer;

impl<'a> ParameterValueType<'a> for ComponentProcessorInputBuffer {
    type Image = Vec<Placeholder<TagImage>>;
    type Audio = Vec<Placeholder<TagAudio>>;
    type Video = (Vec<Placeholder<TagImage>>, Vec<Placeholder<TagAudio>>);
    type File = TimeSplitValue<TimelineTime, Option<Either<PathBuf, FrameVariableValue<PathBuf>>>>;
    type String = TimeSplitValue<TimelineTime, Option<Either<String, FrameVariableValue<String>>>>;
    type Select = TimeSplitValue<TimelineTime, Option<Either<usize, FrameVariableValue<usize>>>>;
    type Boolean = TimeSplitValue<TimelineTime, Option<Either<bool, FrameVariableValue<bool>>>>;
    type Radio = TimeSplitValue<TimelineTime, Option<Either<bool, FrameVariableValue<bool>>>>;
    type Integer = TimeSplitValue<TimelineTime, Option<Either<i64, FrameVariableValue<i64>>>>;
    type RealNumber = TimeSplitValue<TimelineTime, Option<Either<EasingValue<f64>, FrameVariableValue<f64>>>>;
    type Vec2 = Vector2<TimeSplitValue<TimelineTime, Option<Either<EasingValue<f64>, FrameVariableValue<f64>>>>>;
    type Vec3 = Vector3<TimeSplitValue<TimelineTime, Option<Either<EasingValue<f64>, FrameVariableValue<f64>>>>>;
    type Dictionary = TimeSplitValue<TimelineTime, Option<Either<HashMap<String, Parameter<'static, ComponentProcessorInputBuffer>>, FrameVariableValue<HashMap<String, Parameter<'static, ComponentProcessorInputBuffer>>>>>>;
    type ComponentClass = ();
}

// TODO: 二分探索して左端の位置を探索、ループを通じて保持することでたぶん効率化できる
async fn override_time_split_value<T: Clone>(mut value1: TimeSplitValue<TimelineTime, Option<T>>, mut value2: TimeSplitValue<TimelineTime, Option<T>>) -> TimeSplitValue<TimelineTime, Option<T>> {
    for i in 0..value2.len_value() {
        let (&left, value, &right) = value2.get_value_mut(i).unwrap();
        if value.is_none() {
            continue;
        }
        let mut j = 0;
        let start = loop {
            if let Some((_, &time, _)) = value1.get_time(j) {
                if time <= left {
                    break Some(j);
                }
            } else {
                break None;
            }
            j += 1;
        };
        let start = if let Some(start) = start {
            start
        } else {
            value1.push(None, left);
            value1.push(value.take(), right);
            continue;
        };
        loop {
            if let Some((_, &time, _)) = value1.get_time(start + 1) {
                if right <= time {
                    drop(time);
                    let (_, current_value, _) = value1.get_value_mut(start).unwrap();
                    let current_value = current_value.take();
                    let current_value_cloned = current_value.as_ref().cloned();
                    value1.split_value(start, right, None, current_value_cloned);
                    value1.split_value(start, left, current_value, value.take());
                    break;
                } else {
                    drop(time);
                    if let Some((_, &time, _)) = value1.get_time(start + 2) {
                        if time <= right {
                            drop(time);
                            let (_, current_value, _) = value1.get_value_mut(start).unwrap();
                            let current_value = current_value.take();
                            value1.split_value(start, left, current_value, value.take());
                            let (_, time, _) = value1.get_time_mut(start + 1).unwrap();
                            *time = right;
                            break;
                        } else {
                            drop(time);
                            let (_, current_value, _) = value1.get_value_mut(start).unwrap();
                            let current_value = current_value.take();
                            value1.merge_two_values(start + 1, current_value);
                        }
                    } else {
                        let (_, current_value, _) = value1.get_value_mut(start).unwrap();
                        let current_value = current_value.take();
                        value1.split_value(start, left, current_value, value.take());
                        let (_, time, _) = value1.get_time_mut(start + 1).unwrap();
                        *time = right;
                        break;
                    }
                }
            } else {
                value1.push(None, left);
                value1.push(value.take(), right);
                break;
            }
        }
    }
    value1
}

async fn combine_params(value1: Parameter<'static, ComponentProcessorInputBuffer>, value2: Parameter<'static, ComponentProcessorInputBuffer>) -> Parameter<'static, ComponentProcessorInputBuffer> {
    match value1 {
        Parameter::<ComponentProcessorInputBuffer>::None => {
            value2.into_none();
            Parameter::<ComponentProcessorInputBuffer>::None
        }
        Parameter::<ComponentProcessorInputBuffer>::Image(mut value1) => {
            let value2 = match value2 {
                Parameter::Image(image) => image,
                Parameter::Video((image, _)) => image,
                _ => panic!(),
            };
            value1.extend(value2);
            Parameter::Image(value1)
        }
        Parameter::<ComponentProcessorInputBuffer>::Audio(mut value1) => {
            let value2 = match value2 {
                Parameter::Audio(audio) => audio,
                Parameter::Video((_, audio)) => audio,
                _ => panic!(),
            };
            value1.extend(value2);
            Parameter::Audio(value1)
        }
        Parameter::<ComponentProcessorInputBuffer>::Video(mut value1) => {
            let (image, audio) = match value2 {
                Parameter::Image(image) => (image, vec![]),
                Parameter::Audio(audio) => (vec![], audio),
                Parameter::Video((image, audio)) => (image, audio),
                _ => panic!(),
            };
            value1.0.extend(image);
            value1.1.extend(audio);
            Parameter::Video(value1)
        }
        Parameter::<ComponentProcessorInputBuffer>::File(value1) => Parameter::File(override_time_split_value(value1, value2.into_file().unwrap()).await),
        Parameter::<ComponentProcessorInputBuffer>::String(value1) => Parameter::String(override_time_split_value(value1, value2.into_string().unwrap()).await),
        Parameter::<ComponentProcessorInputBuffer>::Select(value1) => Parameter::Select(override_time_split_value(value1, value2.into_select().unwrap()).await),
        Parameter::<ComponentProcessorInputBuffer>::Boolean(value1) => Parameter::Boolean(override_time_split_value(value1, value2.into_boolean().unwrap()).await),
        Parameter::<ComponentProcessorInputBuffer>::Radio(value1) => Parameter::Radio(override_time_split_value(value1, value2.into_radio().unwrap()).await),
        Parameter::<ComponentProcessorInputBuffer>::Integer(value1) => Parameter::Integer(override_time_split_value(value1, value2.into_integer().unwrap()).await),
        Parameter::<ComponentProcessorInputBuffer>::RealNumber(value1) => Parameter::RealNumber(override_time_split_value(value1, value2.into_real_number().unwrap()).await),
        Parameter::<ComponentProcessorInputBuffer>::Vec2(Vector2 { x: value1x, y: value1y }) => {
            let Vector2 { x: value2x, y: value2y } = value2.into_vec2().unwrap();
            Parameter::Vec2(Vector2 {
                x: override_time_split_value(value1x, value2x).await,
                y: override_time_split_value(value1y, value2y).await,
            })
        }
        Parameter::<ComponentProcessorInputBuffer>::Vec3(Vector3 { x: value1x, y: value1y, z: value1z }) => {
            let Vector3 { x: value2x, y: value2y, z: value2z } = value2.into_vec3().unwrap();
            Parameter::Vec3(Vector3 {
                x: override_time_split_value(value1x, value2x).await,
                y: override_time_split_value(value1y, value2y).await,
                z: override_time_split_value(value1z, value2z).await,
            })
        }
        Parameter::<ComponentProcessorInputBuffer>::Dictionary(value1) => Parameter::Dictionary(override_time_split_value(value1, value2.into_dictionary().unwrap()).await),
        Parameter::<ComponentProcessorInputBuffer>::ComponentClass(_) => Parameter::ComponentClass(()),
    }
}

#[async_recursion]
async fn nullable_value_as_input_buffer(params: &ParameterNullableValue) -> Parameter<'static, ComponentProcessorInputBuffer> {
    match params {
        ParameterNullableValue::None => Parameter::None,
        ParameterNullableValue::Image(value) => Parameter::Image(value.clone().into_iter().collect()),
        ParameterNullableValue::Audio(value) => Parameter::Audio(value.clone().into_iter().collect()),
        ParameterNullableValue::Video(value) => Parameter::Video(value.clone().into_iter().unzip()),
        ParameterNullableValue::File(value) => Parameter::File(value.clone().map_time_value_async(|t| async move { t.upgrade().unwrap().read().await.cached_timeline_time() }, |v| async move { v.map(Either::Left) }).await),
        ParameterNullableValue::String(value) => Parameter::String(value.clone().map_time_value_async(|t| async move { t.upgrade().unwrap().read().await.cached_timeline_time() }, |v| async move { v.map(Either::Left) }).await),
        ParameterNullableValue::Select(value) => Parameter::Select(value.clone().map_time_value_async(|t| async move { t.upgrade().unwrap().read().await.cached_timeline_time() }, |v| async move { v.map(Either::Left) }).await),
        ParameterNullableValue::Boolean(value) => Parameter::Boolean(value.clone().map_time_value_async(|t| async move { t.upgrade().unwrap().read().await.cached_timeline_time() }, |v| async move { v.map(Either::Left) }).await),
        ParameterNullableValue::Radio(value) => Parameter::Radio(value.clone().map_time_value_async(|t| async move { t.upgrade().unwrap().read().await.cached_timeline_time() }, |v| async move { v.map(Either::Left) }).await),
        ParameterNullableValue::Integer(value) => Parameter::Integer(value.clone().map_time_value_async(|t| async move { t.upgrade().unwrap().read().await.cached_timeline_time() }, |v| async move { v.map(Either::Left) }).await),
        ParameterNullableValue::RealNumber(value) => Parameter::RealNumber(value.clone().map_time_value_async(|t| async move { t.upgrade().unwrap().read().await.cached_timeline_time() }, |v| async move { v.map(Either::Left) }).await),
        ParameterNullableValue::Vec2(value) => {
            let Vector2 { x, y } = value.clone();
            Parameter::Vec2(Vector2 {
                x: x.map_time_value_async(|t| async move { t.upgrade().unwrap().read().await.cached_timeline_time() }, |v| async move { v.map(Either::Left) }).await,
                y: y.map_time_value_async(|t| async move { t.upgrade().unwrap().read().await.cached_timeline_time() }, |v| async move { v.map(Either::Left) }).await,
            })
        }
        ParameterNullableValue::Vec3(value) => {
            let Vector3 { x, y, z } = value.clone();
            Parameter::Vec3(Vector3 {
                x: x.map_time_value_async(|t| async move { t.upgrade().unwrap().read().await.cached_timeline_time() }, |v| async move { v.map(Either::Left) }).await,
                y: y.map_time_value_async(|t| async move { t.upgrade().unwrap().read().await.cached_timeline_time() }, |v| async move { v.map(Either::Left) }).await,
                z: z.map_time_value_async(|t| async move { t.upgrade().unwrap().read().await.cached_timeline_time() }, |v| async move { v.map(Either::Left) }).await,
            })
        }
        ParameterNullableValue::Dictionary(value) => Parameter::Dictionary(
            value
                .clone()
                .map_time_value_async(
                    |t| async move { t.upgrade().unwrap().read().await.cached_timeline_time() },
                    |v| async move {
                        if let Some(values) = v {
                            Some(Either::Left(stream::iter(values).then(|(k, v)| async move { (k, nullable_value_as_input_buffer(&v).await) }).collect().await))
                        } else {
                            None
                        }
                    },
                )
                .await,
        ),
        ParameterNullableValue::ComponentClass(_) => Parameter::ComponentClass(()),
    }
}

#[async_recursion]
async fn value_as_input_buffer(params: &ParameterValue) -> Parameter<'static, ComponentProcessorInputBuffer> {
    match params {
        ParameterValue::None => Parameter::None,
        ParameterValue::Image(value) => Parameter::Image(vec![value.clone()]),
        ParameterValue::Audio(value) => Parameter::Audio(vec![value.clone()]),
        ParameterValue::Video((image, audio)) => Parameter::Video((vec![image.clone()], vec![audio.clone()])),
        ParameterValue::File(value) => Parameter::File(value.clone().map_time_value_async(|t| async move { t.upgrade().unwrap().read().await.cached_timeline_time() }, |v| async move { Some(Either::Left(v)) }).await),
        ParameterValue::String(value) => Parameter::String(value.clone().map_time_value_async(|t| async move { t.upgrade().unwrap().read().await.cached_timeline_time() }, |v| async move { Some(Either::Left(v)) }).await),
        ParameterValue::Select(value) => Parameter::Select(value.clone().map_time_value_async(|t| async move { t.upgrade().unwrap().read().await.cached_timeline_time() }, |v| async move { Some(Either::Left(v)) }).await),
        ParameterValue::Boolean(value) => Parameter::Boolean(value.clone().map_time_value_async(|t| async move { t.upgrade().unwrap().read().await.cached_timeline_time() }, |v| async move { Some(Either::Left(v)) }).await),
        ParameterValue::Radio(value) => Parameter::Radio(value.clone().map_time_value_async(|t| async move { t.upgrade().unwrap().read().await.cached_timeline_time() }, |v| async move { Some(Either::Left(v)) }).await),
        ParameterValue::Integer(value) => Parameter::Integer(value.clone().map_time_value_async(|t| async move { t.upgrade().unwrap().read().await.cached_timeline_time() }, |v| async move { Some(Either::Left(v)) }).await),
        ParameterValue::RealNumber(value) => Parameter::RealNumber(value.clone().map_time_value_async(|t| async move { t.upgrade().unwrap().read().await.cached_timeline_time() }, |v| async move { Some(Either::Left(v)) }).await),
        ParameterValue::Vec2(value) => {
            let Vector2 { x, y } = value.clone();
            Parameter::Vec2(Vector2 {
                x: x.map_time_value_async(|t| async move { t.upgrade().unwrap().read().await.cached_timeline_time() }, |v| async move { Some(Either::Left(v)) }).await,
                y: y.map_time_value_async(|t| async move { t.upgrade().unwrap().read().await.cached_timeline_time() }, |v| async move { Some(Either::Left(v)) }).await,
            })
        }
        ParameterValue::Vec3(value) => {
            let Vector3 { x, y, z } = value.clone();
            Parameter::Vec3(Vector3 {
                x: x.map_time_value_async(|t| async move { t.upgrade().unwrap().read().await.cached_timeline_time() }, |v| async move { Some(Either::Left(v)) }).await,
                y: y.map_time_value_async(|t| async move { t.upgrade().unwrap().read().await.cached_timeline_time() }, |v| async move { Some(Either::Left(v)) }).await,
                z: z.map_time_value_async(|t| async move { t.upgrade().unwrap().read().await.cached_timeline_time() }, |v| async move { Some(Either::Left(v)) }).await,
            })
        }
        ParameterValue::Dictionary(value) => Parameter::Dictionary(
            value
                .clone()
                .map_time_value_async(
                    |t| async move { t.upgrade().unwrap().read().await.cached_timeline_time() },
                    |values| async move { Some(Either::Left(stream::iter(values).then(|(k, v)| async move { (k, value_as_input_buffer(&v).await) }).collect().await)) },
                )
                .await,
        ),
        ParameterValue::ComponentClass(_) => Parameter::ComponentClass(()),
    }
}

fn frame_variable_value_into_input_buffer(params: ParameterFrameVariableValue, left: TimelineTime, right: TimelineTime) -> Parameter<'static, ComponentProcessorInputBuffer> {
    match params {
        ParameterFrameVariableValue::None => Parameter::None,
        ParameterFrameVariableValue::Image(value) => Parameter::Image(vec![value]),
        ParameterFrameVariableValue::Audio(value) => Parameter::Audio(vec![value]),
        ParameterFrameVariableValue::Video((image, audio)) => Parameter::Video((vec![image], vec![audio])),
        ParameterFrameVariableValue::File(value) => Parameter::File(TimeSplitValue::new(left, Some(Either::Right(value)), right)),
        ParameterFrameVariableValue::String(value) => Parameter::String(TimeSplitValue::new(left, Some(Either::Right(value)), right)),
        ParameterFrameVariableValue::Select(value) => Parameter::Select(TimeSplitValue::new(left, Some(Either::Right(value)), right)),
        ParameterFrameVariableValue::Boolean(value) => Parameter::Boolean(TimeSplitValue::new(left, Some(Either::Right(value)), right)),
        ParameterFrameVariableValue::Radio(value) => Parameter::Radio(TimeSplitValue::new(left, Some(Either::Right(value)), right)),
        ParameterFrameVariableValue::Integer(value) => Parameter::Integer(TimeSplitValue::new(left, Some(Either::Right(value)), right)),
        ParameterFrameVariableValue::RealNumber(value) => Parameter::RealNumber(TimeSplitValue::new(left, Some(Either::Right(value)), right)),
        ParameterFrameVariableValue::Vec2(value) => Parameter::Vec2(Vector2 {
            x: TimeSplitValue::new(left, Some(Either::Right(value.map_ref(|vec| vec.x))), right),
            y: TimeSplitValue::new(left, Some(Either::Right(value.map_ref(|vec| vec.y))), right),
        }),
        ParameterFrameVariableValue::Vec3(value) => Parameter::Vec3(Vector3 {
            x: TimeSplitValue::new(left, Some(Either::Right(value.map_ref(|vec| vec.x))), right),
            y: TimeSplitValue::new(left, Some(Either::Right(value.map_ref(|vec| vec.y))), right),
            z: TimeSplitValue::new(left, Some(Either::Right(value.map_ref(|vec| vec.z))), right),
        }),
        ParameterFrameVariableValue::Dictionary(value) => {
            let parameter = value.map(|v| v.into_iter().map(|(k, v)| (k, frame_variable_value_into_input_buffer(v, left, right))).collect());
            Parameter::Dictionary(TimeSplitValue::new(left, Some(Either::Right(parameter)), right))
        }
        ParameterFrameVariableValue::ComponentClass(value) => Parameter::ComponentClass(()),
    }
}

fn empty_input_buffer<'a, T: ParameterValueType<'a>>(ty: &Parameter<'a, T>, left: TimelineTime, right: TimelineTime) -> Parameter<'static, ComponentProcessorInputBuffer> {
    match ty {
        Parameter::None => Parameter::None,
        Parameter::Image(_) => Parameter::Image(Vec::new()),
        Parameter::Audio(_) => Parameter::Audio(Vec::new()),
        Parameter::Video(_) => Parameter::Video((Vec::new(), Vec::new())),
        Parameter::File(_) => Parameter::File(TimeSplitValue::new(left, None, right)),
        Parameter::String(_) => Parameter::String(TimeSplitValue::new(left, None, right)),
        Parameter::Select(_) => Parameter::Select(TimeSplitValue::new(left, None, right)),
        Parameter::Boolean(_) => Parameter::Boolean(TimeSplitValue::new(left, None, right)),
        Parameter::Radio(_) => Parameter::Boolean(TimeSplitValue::new(left, None, right)),
        Parameter::Integer(_) => Parameter::Integer(TimeSplitValue::new(left, None, right)),
        Parameter::RealNumber(_) => Parameter::RealNumber(TimeSplitValue::new(left, None, right)),
        Parameter::Vec2(_) => Parameter::Vec2(Vector2 {
            x: TimeSplitValue::new(left, None, right),
            y: TimeSplitValue::new(left, None, right),
        }),
        Parameter::Vec3(_) => Parameter::Vec3(Vector3 {
            x: TimeSplitValue::new(left, None, right),
            y: TimeSplitValue::new(left, None, right),
            z: TimeSplitValue::new(left, None, right),
        }),
        Parameter::Dictionary(_) => Parameter::Dictionary(TimeSplitValue::new(left, None, right)),
        Parameter::ComponentClass(_) => Parameter::ComponentClass(()),
    }
}

#[derive(Debug)]
pub(crate) struct SourceTree<T, U, ID> {
    id: Arc<ID>,
    tree: DashMap<Placeholder<T>, Either<(Vec<Placeholder<T>>, Option<Arc<[(Range<TimelineTime>, Range<MarkerTime>)]>>), U>>,
}

impl<T, U, ID: IdGenerator> SourceTree<T, U, ID> {
    pub(crate) fn new(id: Arc<ID>) -> SourceTree<T, U, ID> {
        SourceTree { id, tree: DashMap::new() }
    }

    fn new_id(&self, tree: impl Into<Vec<Placeholder<T>>>, time_map: Arc<[(Range<TimelineTime>, Range<MarkerTime>)]>) -> Placeholder<T> {
        let id = Placeholder::<T>::new(&*self.id);
        self.tree.insert(id.clone(), Either::Left((tree.into(), Some(time_map))));
        id
    }

    fn new_id_without_map(&self, tree: impl Into<Vec<Placeholder<T>>>) -> Placeholder<T> {
        let id = Placeholder::<T>::new(&*self.id);
        self.tree.insert(id.clone(), Either::Left((tree.into(), None)));
        id
    }

    fn new_id_native(&self, node: U) -> Placeholder<T> {
        let id = Placeholder::<T>::new(&*self.id);
        self.tree.insert(id.clone(), Either::Right(node));
        id
    }

    pub(crate) fn into_readonly(self) -> ReadonlySourceTree<T, U> {
        ReadonlySourceTree { tree: self.tree }
    }
}

#[derive(Debug)]
pub struct ReadonlySourceTree<T, U> {
    tree: DashMap<Placeholder<T>, Either<(Vec<Placeholder<T>>, Option<Arc<[(Range<TimelineTime>, Range<MarkerTime>)]>>), U>>,
}

impl<T, U> ReadonlySourceTree<T, U> {
    pub fn get(&self, key: Placeholder<T>) -> Option<impl Deref<Target = Either<(Vec<Placeholder<T>>, Option<Arc<[(Range<TimelineTime>, Range<MarkerTime>)]>>), U>> + '_> {
        self.tree.get(&key)
    }
}

pub type ImageNativeTreeNode<T> = (ImageRequiredParamsFrameVariable, NativeProcessorExecutable<T>);
pub type AudioNativeTreeNode<T> = (AudioRequiredParams<T>, NativeProcessorExecutable<T>);

fn unwrap_or_default<ID: IdGenerator, T: ParameterValueType<'static>>(
    params: Parameter<'static, ComponentProcessorInputBuffer>,
    time_map: Arc<[(Range<TimelineTime>, Range<MarkerTime>)]>,
    image_source_tree: &SourceTree<TagImage, ImageNativeTreeNode<T>, ID>,
    audio_source_tree: &SourceTree<TagAudio, AudioNativeTreeNode<T>, ID>,
) -> ComponentProcessorInputValue {
    let mut map_time = get_map_time(Arc::clone(&time_map));
    match params {
        Parameter::None => ComponentProcessorInputValue::None,
        Parameter::Image(image) => ComponentProcessorInputValue::Image(image_source_tree.new_id(image, time_map)),
        Parameter::Audio(audio) => ComponentProcessorInputValue::Audio(audio_source_tree.new_id(audio, time_map)),
        Parameter::Video((image, audio)) => ComponentProcessorInputValue::Video((image_source_tree.new_id(image, Arc::clone(&time_map)), audio_source_tree.new_id(audio, time_map))),
        Parameter::File(value) => Parameter::File(value.map_time(|time| map_time.map_time(time))),
        Parameter::String(value) => Parameter::String(value.map_time(|time| map_time.map_time(time))),
        Parameter::Select(value) => Parameter::Select(value.map_time(|time| map_time.map_time(time))),
        Parameter::Boolean(value) => Parameter::Boolean(value.map_time(|time| map_time.map_time(time))),
        Parameter::Radio(value) => Parameter::Radio(value.map_time(|time| map_time.map_time(time))),
        Parameter::Integer(value) => Parameter::Integer(value.map_time(|time| map_time.map_time(time))),
        Parameter::RealNumber(value) => Parameter::RealNumber(value.map_time(|time| map_time.map_time(time))),
        Parameter::Vec2(value) => Parameter::Vec2(value.map(|value| value.map_time(|time| map_time.map_time(time)))),
        Parameter::Vec3(value) => Parameter::Vec3(value.map(|value| value.map_time(|time| map_time.map_time(time)))),
        Parameter::Dictionary(value) => ComponentProcessorInputValue::Dictionary(value.map_value(|value| match value {
            None => None,
            Some(Either::Left(value)) => Some(Either::Left(value.into_iter().map(|(k, v)| (k, unwrap_or_default(v, Arc::clone(&time_map), image_source_tree, audio_source_tree))).collect())),
            Some(Either::Right(value)) => Some(Either::Right(value.map(|values| values.into_iter().map(|(k, v)| (k, unwrap_or_default(v, Arc::clone(&time_map), image_source_tree, audio_source_tree))).collect()))),
        })),
        Parameter::ComponentClass(_) => ComponentProcessorInputValue::ComponentClass(()),
    }
}

trait MapTime: Clone {
    fn map_time(&mut self, time: TimelineTime) -> TimelineTime;
    fn map_time_reverse(&mut self, time: TimelineTime) -> TimelineTime;
}

impl<F1: FnMut(TimelineTime) -> TimelineTime + Clone, F2: FnMut(TimelineTime) -> TimelineTime + Clone> MapTime for (F1, F2) {
    fn map_time(&mut self, time: TimelineTime) -> TimelineTime {
        self.0(time)
    }
    fn map_time_reverse(&mut self, time: TimelineTime) -> TimelineTime {
        self.1(time)
    }
}

fn get_map_time(time: Arc<[(Range<TimelineTime>, Range<MarkerTime>)]>) -> impl MapTime {
    // TODO: 計算量をもうちょっとなんとかしてくれ
    let time1 = Arc::clone(&time);
    (
        move |t: TimelineTime| -> TimelineTime {
            if let Some((timeline_time_range, marker_time_range)) = time1.iter().find(|(range, _)| range.contains(&t)) {
                if (timeline_time_range.end.value() - timeline_time_range.start.value()).abs() < f64::EPSILON {
                    marker_time_range.start.into()
                } else {
                    let p = (t.value() - timeline_time_range.start.value()) / (timeline_time_range.end.value() - timeline_time_range.start.value());
                    MarkerTime::new(marker_time_range.start.value() * (1. - p) + marker_time_range.end.value() * p).unwrap().into()
                }
            } else if t < time1.first().unwrap().0.start {
                time1.first().unwrap().1.start.into()
            } else {
                time1.last().unwrap().1.end.into()
            }
        },
        move |t: TimelineTime| -> TimelineTime {
            if let Some((timeline_time_range, marker_time_range)) = time.iter().find(|&&(_, Range { start, end })| (start.into()..end.into()).contains(&t)) {
                if (marker_time_range.end.value() - marker_time_range.start.value()).abs() < f64::EPSILON {
                    timeline_time_range.start.into()
                } else {
                    let p = (t.value() - marker_time_range.start.value()) / (marker_time_range.end.value() - marker_time_range.start.value());
                    MarkerTime::new(timeline_time_range.start.value() * (1. - p) + timeline_time_range.end.value() * p).unwrap().into()
                }
            } else if t < time.first().unwrap().0.start {
                time.first().unwrap().1.start.into()
            } else {
                time.last().unwrap().1.end.into()
            }
        },
    )
}

fn frame_values<T: Default + Clone>(value: TimeSplitValue<TimelineTime, Option<Either<T, FrameVariableValue<T>>>>, frames: impl Iterator<Item = TimelineTime>) -> FrameVariableValue<T> {
    // TODO: 計算量が論外
    frames
        .filter_map(|time| {
            let value = (0..value.len_value()).find_map(|i| {
                let (&left, value, &right) = value.get_value(i).unwrap();
                if left <= time && time < right {
                    match value {
                        None => Some(T::default()),
                        Some(Either::Left(value)) => Some(value.clone()),
                        Some(Either::Right(value)) => Some(value.get(time).unwrap().clone()),
                    }
                } else {
                    None
                }
            })?;
            Some((time, value))
        })
        .collect::<BTreeMap<_, _>>()
        .into()
}

fn frame_values_easing<T: Clone, U, F: Fn([T; N]) -> U, const N: usize>(value: [TimeSplitValue<TimelineTime, Option<Either<EasingValue<T>, FrameVariableValue<T>>>>; N], f: F, frames: impl Iterator<Item = TimelineTime>, default: impl Fn() -> T) -> FrameVariableValue<U> {
    // TODO: 計算量が論外
    frames
        .map(|time| {
            let value = f(array::from_fn(|j| {
                (0..value[j].len_value())
                    .find_map(|i| {
                        let (&left, value, &right) = value[j].get_value(i).unwrap();
                        if left <= time && time <= right {
                            match value {
                                None => Some(default()),
                                Some(Either::Left(value)) => {
                                    let p = if right.value() - left.value() < f64::EPSILON { 0. } else { (time.value() - left.value()) / (right.value() - left.value()) };
                                    Some(value.easing.easing(&value.from, &value.to, p))
                                }
                                Some(Either::Right(value)) => Some(value.get(time).unwrap().clone()),
                            }
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| panic!("{time:?}"))
            }));
            (time, value)
        })
        .collect::<BTreeMap<_, _>>()
        .into()
}

fn into_frame_variable_value<T, ID: IdGenerator>(
    param: Parameter<'static, ComponentProcessorInputBuffer>,
    frames: impl Iterator<Item = TimelineTime>,
    image_source_tree: &SourceTree<TagImage, ImageNativeTreeNode<T>, ID>,
    audio_source_tree: &SourceTree<TagAudio, AudioNativeTreeNode<T>, ID>,
) -> ParameterFrameVariableValue {
    match param {
        Parameter::None => Parameter::None,
        Parameter::Image(value) => ParameterFrameVariableValue::Image(image_source_tree.new_id_without_map(value)),
        Parameter::Audio(value) => ParameterFrameVariableValue::Audio(audio_source_tree.new_id_without_map(value)),
        Parameter::Video((image, audio)) => ParameterFrameVariableValue::Video((image_source_tree.new_id_without_map(image), audio_source_tree.new_id_without_map(audio))),
        Parameter::File(value) => ParameterFrameVariableValue::File(frame_values(value, frames)),
        Parameter::String(value) => ParameterFrameVariableValue::String(frame_values(value, frames)),
        Parameter::Select(value) => ParameterFrameVariableValue::Select(frame_values(value, frames)),
        Parameter::Boolean(value) => ParameterFrameVariableValue::Boolean(frame_values(value, frames)),
        Parameter::Radio(value) => ParameterFrameVariableValue::Radio(frame_values(value, frames)),
        Parameter::Integer(value) => ParameterFrameVariableValue::Integer(frame_values(value, frames)),
        Parameter::RealNumber(value) => ParameterFrameVariableValue::RealNumber(frame_values_easing([value], |[v]| v, frames, Default::default)),
        Parameter::Vec2(value) => ParameterFrameVariableValue::Vec2(frame_values_easing(value.into(), From::from, frames, Default::default)),
        Parameter::Vec3(value) => ParameterFrameVariableValue::Vec3(frame_values_easing(value.into(), From::from, frames, Default::default)),
        Parameter::Dictionary(value) => todo!(),
        Parameter::ComponentClass(()) => todo!(),
    }
}

async fn shift_time<T, ID: IdGenerator>(
    param: Parameter<'static, ComponentProcessorInputBuffer>,
    time: Arc<[(Range<TimelineTime>, Range<MarkerTime>)]>,
    frames: impl Iterator<Item = TimelineTime>,
    image_source_tree: &SourceTree<TagImage, ImageNativeTreeNode<T>, ID>,
    audio_source_tree: &SourceTree<TagAudio, AudioNativeTreeNode<T>, ID>,
) -> ParameterFrameVariableValue {
    let mut map_time = get_map_time(Arc::clone(&time));
    match param {
        Parameter::None => Parameter::None,
        Parameter::Image(value) => Parameter::Image(image_source_tree.new_id(value, Arc::clone(&time))),
        Parameter::Audio(value) => Parameter::Audio(audio_source_tree.new_id(value, Arc::clone(&time))),
        Parameter::Video((image, audio)) => Parameter::Video((image_source_tree.new_id(image, Arc::clone(&time)), audio_source_tree.new_id(audio, Arc::clone(&time)))),
        Parameter::File(value) => Parameter::File(frame_values(value.map_time(|time| map_time.map_time(time)), frames)),
        Parameter::String(value) => Parameter::String(frame_values(value.map_time(|time| map_time.map_time(time)), frames)),
        Parameter::Select(value) => Parameter::Select(frame_values(value.map_time(|time| map_time.map_time(time)), frames)),
        Parameter::Boolean(value) => Parameter::Boolean(frame_values(value.map_time(|time| map_time.map_time(time)), frames)),
        Parameter::Radio(value) => Parameter::Radio(frame_values(value.map_time(|time| map_time.map_time(time)), frames)),
        Parameter::Integer(value) => Parameter::Integer(frame_values(value.map_time(|time| map_time.map_time(time)), frames)),
        Parameter::RealNumber(value) => Parameter::RealNumber(frame_values_easing([value.map_time(|time| map_time.map_time(time))], |[v]| v, frames, Default::default)),
        Parameter::Vec2(value) => Parameter::Vec2(frame_values_easing(value.map(|value| value.map_time(|time| map_time.map_time(time))).into(), From::from, frames, Default::default)),
        Parameter::Vec3(value) => Parameter::Vec3(frame_values_easing(value.map(|value| value.map_time(|time| map_time.map_time(time))).into(), From::from, frames, Default::default)),
        Parameter::Dictionary(value) => todo!(),
        Parameter::ComponentClass(()) => todo!(),
    }
}

fn map_time_reverse(param: ParameterFrameVariableValue, mut map_time: impl MapTime) -> ParameterFrameVariableValue {
    match param {
        ParameterFrameVariableValue::None => ParameterFrameVariableValue::None,
        ParameterFrameVariableValue::Image(value) => ParameterFrameVariableValue::Image(value),
        ParameterFrameVariableValue::Audio(value) => ParameterFrameVariableValue::Audio(value),
        ParameterFrameVariableValue::Video(value) => ParameterFrameVariableValue::Video(value),
        ParameterFrameVariableValue::File(value) => ParameterFrameVariableValue::File(value.map_time(|t| map_time.map_time_reverse(t))),
        ParameterFrameVariableValue::String(value) => ParameterFrameVariableValue::String(value.map_time(|t| map_time.map_time_reverse(t))),
        ParameterFrameVariableValue::Select(value) => ParameterFrameVariableValue::Select(value.map_time(|t| map_time.map_time_reverse(t))),
        ParameterFrameVariableValue::Boolean(value) => ParameterFrameVariableValue::Boolean(value.map_time(|t| map_time.map_time_reverse(t))),
        ParameterFrameVariableValue::Radio(value) => ParameterFrameVariableValue::Radio(value.map_time(|t| map_time.map_time_reverse(t))),
        ParameterFrameVariableValue::Integer(value) => ParameterFrameVariableValue::Integer(value.map_time(|t| map_time.map_time_reverse(t))),
        ParameterFrameVariableValue::RealNumber(value) => ParameterFrameVariableValue::RealNumber(value.map_time(|t| map_time.map_time_reverse(t))),
        ParameterFrameVariableValue::Vec2(value) => ParameterFrameVariableValue::Vec2(value.map_time(|t| map_time.map_time_reverse(t))),
        ParameterFrameVariableValue::Vec3(value) => ParameterFrameVariableValue::Vec3(value.map_time(|t| map_time.map_time_reverse(t))),
        ParameterFrameVariableValue::Dictionary(value) => ParameterFrameVariableValue::Dictionary(value),
        ParameterFrameVariableValue::ComponentClass(value) => ParameterFrameVariableValue::ComponentClass(value),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RendererError {
    #[error("required params is None")]
    RequiredParamsIsNone,
    #[error("NativeProcessor output type mismatch")]
    NativeProcessorOutputTypeMismatch,
    #[error("NativeProcessor not found")]
    NativeProcessorNotFound,
    #[error("invalid component")]
    InvalidComponent,
}

fn evaluate_processor_executable<T: ParameterValueType<'static>, ID: IdGenerator>(
    processor_executable: NativeProcessorExecutable<T>,
    frames: impl Iterator<Item = TimelineTime>,
    image_required_params: Option<&ImageRequiredParamsFrameVariable>,
    audio_required_params: Option<&AudioRequiredParams<T>>,
    image_source_tree: &SourceTree<TagImage, ImageNativeTreeNode<T>, ID>,
    audio_source_tree: &SourceTree<TagAudio, AudioNativeTreeNode<T>, ID>,
) -> Result<ParameterFrameVariableValue, RendererError> {
    let mut parameters = frames.map(|time| {
        let value = processor_executable
            .parameter
            .iter()
            .map(|param| match param {
                Parameter::None => ParameterNativeProcessorInputFixed::<T::Image, T::Audio>::None,
                Parameter::Image(value) => unimplemented!(),
                Parameter::Audio(value) => unimplemented!(),
                Parameter::Video(value) => unimplemented!(),
                Parameter::File(value) => ParameterNativeProcessorInputFixed::<T::Image, T::Audio>::File(value.get(time).unwrap().clone()),
                Parameter::String(value) => ParameterNativeProcessorInputFixed::<T::Image, T::Audio>::String(value.get(time).unwrap().clone()),
                Parameter::Select(value) => ParameterNativeProcessorInputFixed::<T::Image, T::Audio>::Select(*value.get(time).unwrap()),
                Parameter::Boolean(value) => ParameterNativeProcessorInputFixed::<T::Image, T::Audio>::Boolean(*value.get(time).unwrap()),
                Parameter::Radio(value) => ParameterNativeProcessorInputFixed::<T::Image, T::Audio>::Radio(*value.get(time).unwrap()),
                Parameter::Integer(value) => ParameterNativeProcessorInputFixed::<T::Image, T::Audio>::Integer(*value.get(time).unwrap()),
                Parameter::RealNumber(value) => ParameterNativeProcessorInputFixed::<T::Image, T::Audio>::RealNumber(*value.get(time).unwrap()),
                Parameter::Vec2(value) => ParameterNativeProcessorInputFixed::<T::Image, T::Audio>::Vec2(*value.get(time).unwrap()),
                Parameter::Vec3(value) => ParameterNativeProcessorInputFixed::<T::Image, T::Audio>::Vec3(*value.get(time).unwrap()),
                Parameter::Dictionary(value) => todo!(),
                Parameter::ComponentClass(_) => unreachable!(),
            })
            .collect::<Vec<_>>();
        (time, processor_executable.processor.process(&value))
    });
    let value = match processor_executable.processor.return_type() {
        ParameterTypeExceptComponentClass::None => ParameterFrameVariableValue::None,
        ParameterTypeExceptComponentClass::Image(_) => ParameterFrameVariableValue::Image(image_source_tree.new_id_native((image_required_params.cloned().ok_or(RendererError::RequiredParamsIsNone)?, processor_executable))),
        ParameterTypeExceptComponentClass::Audio(_) => ParameterFrameVariableValue::Audio(audio_source_tree.new_id_native((audio_required_params.cloned().ok_or(RendererError::RequiredParamsIsNone)?, processor_executable))),
        ParameterTypeExceptComponentClass::Video(_) => ParameterFrameVariableValue::Video((
            image_source_tree.new_id_native((image_required_params.cloned().ok_or(RendererError::RequiredParamsIsNone)?, processor_executable.clone())),
            audio_source_tree.new_id_native((audio_required_params.cloned().ok_or(RendererError::RequiredParamsIsNone)?, processor_executable)),
        )),
        ParameterTypeExceptComponentClass::File(_) => parameters
            .try_fold(Vec::new(), |mut acc, (time, param)| {
                acc.push((time, param.into_file().ok()?));
                Some(acc)
            })
            .map(|values| ParameterFrameVariableValue::File(BTreeMap::from_iter(values).into()))
            .ok_or(RendererError::NativeProcessorOutputTypeMismatch)?,
        ParameterTypeExceptComponentClass::String(_) => parameters
            .try_fold(Vec::new(), |mut acc, (time, param)| {
                acc.push((time, param.into_string().ok()?));
                Some(acc)
            })
            .map(|values| ParameterFrameVariableValue::String(BTreeMap::from_iter(values).into()))
            .ok_or(RendererError::NativeProcessorOutputTypeMismatch)?,
        ParameterTypeExceptComponentClass::Select(_) => parameters
            .try_fold(Vec::new(), |mut acc, (time, param)| {
                acc.push((time, param.into_select().ok()?));
                Some(acc)
            })
            .map(|values| ParameterFrameVariableValue::Select(BTreeMap::from_iter(values).into()))
            .ok_or(RendererError::NativeProcessorOutputTypeMismatch)?,
        ParameterTypeExceptComponentClass::Boolean(_) => parameters
            .try_fold(Vec::new(), |mut acc, (time, param)| {
                acc.push((time, param.into_boolean().ok()?));
                Some(acc)
            })
            .map(|values| ParameterFrameVariableValue::Boolean(BTreeMap::from_iter(values).into()))
            .ok_or(RendererError::NativeProcessorOutputTypeMismatch)?,
        ParameterTypeExceptComponentClass::Radio(_) => parameters
            .try_fold(Vec::new(), |mut acc, (time, param)| {
                acc.push((time, param.into_radio().ok()?));
                Some(acc)
            })
            .map(|values| ParameterFrameVariableValue::Radio(BTreeMap::from_iter(values).into()))
            .ok_or(RendererError::NativeProcessorOutputTypeMismatch)?,
        ParameterTypeExceptComponentClass::Integer(_) => parameters
            .try_fold(Vec::new(), |mut acc, (time, param)| {
                acc.push((time, param.into_integer().ok()?));
                Some(acc)
            })
            .map(|values| ParameterFrameVariableValue::Integer(BTreeMap::from_iter(values).into()))
            .ok_or(RendererError::NativeProcessorOutputTypeMismatch)?,
        ParameterTypeExceptComponentClass::RealNumber(_) => parameters
            .try_fold(Vec::new(), |mut acc, (time, param)| {
                acc.push((time, param.into_real_number().ok()?));
                Some(acc)
            })
            .map(|values| ParameterFrameVariableValue::RealNumber(BTreeMap::from_iter(values).into()))
            .ok_or(RendererError::NativeProcessorOutputTypeMismatch)?,
        ParameterTypeExceptComponentClass::Vec2(_) => parameters
            .try_fold(Vec::new(), |mut acc, (time, param)| {
                acc.push((time, param.into_vec2().ok()?));
                Some(acc)
            })
            .map(|values| ParameterFrameVariableValue::Vec2(BTreeMap::from_iter(values).into()))
            .ok_or(RendererError::NativeProcessorOutputTypeMismatch)?,
        ParameterTypeExceptComponentClass::Vec3(_) => parameters
            .try_fold(Vec::new(), |mut acc, (time, param)| {
                acc.push((time, param.into_vec3().ok()?));
                Some(acc)
            })
            .map(|values| ParameterFrameVariableValue::Vec3(BTreeMap::from_iter(values).into()))
            .ok_or(RendererError::NativeProcessorOutputTypeMismatch)?,
        ParameterTypeExceptComponentClass::Dictionary(_) => todo!(),
        ParameterTypeExceptComponentClass::ComponentClass(_) => todo!(),
    };
    Ok(value)
}

pub(crate) trait CloneableIterator: Iterator {
    fn clone_dyn(&self) -> Box<dyn CloneableIterator<Item = Self::Item> + Send + Sync + 'static>;
}

impl<T: Iterator + Clone + Send + Sync + 'static> CloneableIterator for T {
    #[inline(never)]
    fn clone_dyn(&self) -> Box<dyn CloneableIterator<Item = Self::Item> + Send + Sync + 'static> {
        Box::new(self.clone())
    }
}

impl<T: 'static> Clone for Box<dyn CloneableIterator<Item = T> + Send + Sync + 'static> {
    #[inline(never)]
    fn clone(&self) -> Self {
        <dyn CloneableIterator<Item = T> + Send + Sync + 'static as CloneableIterator>::clone_dyn(self.deref())
    }
}

#[async_recursion]
pub(crate) async fn evaluate_component<T: ParameterValueType<'static> + 'static, ID: IdGenerator + 'static>(
    component: StaticPointer<RwLock<ComponentInstance<T>>>,
    value_type: ParameterType,
    image_source_tree: Arc<SourceTree<TagImage, ImageNativeTreeNode<T>, ID>>,
    audio_source_tree: Arc<SourceTree<TagAudio, AudioNativeTreeNode<T>, ID>>,
    frames: Box<dyn CloneableIterator<Item = TimelineTime> + Send + Sync + 'static>,
) -> Result<(StaticPointer<RwLock<MarkerPin>>, ParameterFrameVariableValue, StaticPointer<RwLock<MarkerPin>>), RendererError> {
    let component = component.upgrade().ok_or(RendererError::InvalidComponent)?;
    let component = component.read().await;
    let left = component.marker_left().ptr().clone();
    let right = component.marker_right().ptr().clone();
    let left_time = left.upgrade().unwrap().read().await.cached_timeline_time();
    let right_time = right.upgrade().unwrap().read().await.cached_timeline_time();
    let mut frames = frames.skip_while(move |&time| time < left_time).take_while(move |&time| time < right_time).peekable();
    let _ = frames.peek();
    let image_required_params = component.image_required_params();
    let audio_required_params = component.audio_required_params();
    let fixed_parameters = component.fixed_parameters();
    let variable_parameters = component.variable_parameters();
    let variable_parameters_type = component.variable_parameters_type();
    let variable_parameters = stream::iter(variable_parameters.iter().zip(variable_parameters_type))
        .fold(
            Ok(Vec::with_capacity(variable_parameters.len())),
            |acc, (param, (_, param_type)): (&VariableParameterValue<T, ParameterValue, ParameterNullableValue>, &(String, ParameterType))| {
                let frames = frames.clone();
                let image_source_tree = Arc::clone(&image_source_tree);
                let audio_source_tree = Arc::clone(&audio_source_tree);
                async move {
                    let mut acc = acc?;
                    acc.push(evaluate_parameter(left_time, right_time, param, param_type, frames, &image_source_tree, &audio_source_tree).await?);
                    Ok(acc)
                }
            },
        )
        .await?;
    // TODO: 同一時間のフレームを複数回見る場合は別系統にする必要がある
    let processor = component.processor();
    let natural_length = processor.natural_length(fixed_parameters).await;
    let markers = stream::once(async {
        let left = left.upgrade().unwrap();
        let marker = left.read().await;
        (marker.cached_timeline_time(), marker.locked_component_time())
    })
    .chain(stream::iter(component.markers()).then(|marker| async move {
        let marker = marker.read().await;
        (marker.cached_timeline_time(), marker.locked_component_time())
    }))
    .chain(stream::once(async {
        let right = right.upgrade().unwrap();
        let marker = right.read().await;
        (marker.cached_timeline_time(), marker.locked_component_time())
    }))
    .filter_map(|(cached_timeline_time, locked_component_time)| async move { Some((cached_timeline_time, locked_component_time?)) })
    .collect::<Vec<(TimelineTime, MarkerTime)>>()
    .await;
    let marker_ranges = match *markers.as_slice() {
        [] => vec![(left_time..right_time, MarkerTime::ZERO..MarkerTime::new(natural_length.as_secs_f64()).unwrap())],
        [(timeline_time, marker_time)] => vec![
            (left_time..timeline_time, MarkerTime::new((timeline_time.value() - left_time.value()).max(0.0)).unwrap()..marker_time),
            (timeline_time..right_time, marker_time..MarkerTime::new((right_time.value() - timeline_time.value()).min(natural_length.as_secs_f64())).unwrap()),
        ],
        ref markers => {
            // n点固定、左右端はその隣の速度から
            let marker_ranges = markers.windows(2).map(|window| <&[_; 2]>::try_from(window).unwrap()).map(|[left, right]| (left.0..right.0, left.1..right.1)).collect::<Vec<_>>();
            let (timeline_time_range, marker_time_range) = marker_ranges.first().unwrap();
            let dm_dt = match timeline_time_range {
                timeline_time_range if (timeline_time_range.end.value() - timeline_time_range.start.value()).abs() < f64::EPSILON => (marker_time_range.end.value() - marker_time_range.start.value()) / (timeline_time_range.end.value() - timeline_time_range.start.value()),
                _ => 1.,
            };
            let left_marker_time = MarkerTime::new((marker_time_range.start.value() - (timeline_time_range.start.value() - left_time.value()) * dm_dt).max(0.)).unwrap();
            let left_ranges = (left_time..timeline_time_range.start, left_marker_time..marker_time_range.start);
            let (timeline_time_range, marker_time_range) = marker_ranges.last().unwrap();
            let dm_dt = match timeline_time_range {
                timeline_time_range if (timeline_time_range.end.value() - timeline_time_range.start.value()).abs() < f64::EPSILON => (marker_time_range.end.value() - marker_time_range.start.value()) / (timeline_time_range.end.value() - timeline_time_range.start.value()),
                _ => 1.,
            };
            let right_marker_time = MarkerTime::new((marker_time_range.end.value() + (right_time.value() - timeline_time_range.end.value()) * dm_dt).min(natural_length.as_secs_f64())).unwrap();
            let right_ranges = (timeline_time_range.end..right_time, marker_time_range.end..right_marker_time);
            iter::once(left_ranges).chain(marker_ranges).chain(iter::once(right_ranges)).collect()
        }
    };
    // 時間シフトを考えるのはここだけ
    // 時間シフト
    let marker_ranges = marker_ranges.into();
    let map_time = get_map_time(Arc::clone(&marker_ranges));
    let image_required_params = if let Some(image_required_params) = image_required_params {
        let transform = {
            match &image_required_params.transform {
                ImageRequiredParamsTransform::Params { scale, translate, rotate, scale_center, rotate_center } => {
                    let (scale, translate, rotate, scale_center, rotate_center) = (
                        evaluate_variable(left_time, right_time, scale.clone(), &image_source_tree, &audio_source_tree, frames.clone()).await,
                        evaluate_variable(left_time, right_time, translate.clone(), &image_source_tree, &audio_source_tree, frames.clone()).await,
                        evaluate_variable_easing(rotate.clone(), frames.clone(), Quaternion::one).await,
                        evaluate_variable(left_time, right_time, scale_center.clone(), &image_source_tree, &audio_source_tree, frames.clone()).await,
                        evaluate_variable(left_time, right_time, rotate_center.clone(), &image_source_tree, &audio_source_tree, frames.clone()).await,
                    );
                    Ok(ImageRequiredParamsTransformFrameVariable::Params {
                        scale: scale?,
                        translate: translate?,
                        rotate,
                        scale_center: scale_center?,
                        rotate_center: rotate_center?,
                    })
                }
                ImageRequiredParamsTransform::Free { left_top, right_top, left_bottom, right_bottom } => {
                    let (left_top, right_top, left_bottom, right_bottom) = tokio::join!(
                        evaluate_variable(left_time, right_time, left_top.clone(), &image_source_tree, &audio_source_tree, frames.clone()),
                        evaluate_variable(left_time, right_time, right_top.clone(), &image_source_tree, &audio_source_tree, frames.clone()),
                        evaluate_variable(left_time, right_time, left_bottom.clone(), &image_source_tree, &audio_source_tree, frames.clone()),
                        evaluate_variable(left_time, right_time, right_bottom.clone(), &image_source_tree, &audio_source_tree, frames.clone()),
                    );
                    Ok(ImageRequiredParamsTransformFrameVariable::Free {
                        left_top: left_top?,
                        right_top: right_top?,
                        left_bottom: left_bottom?,
                        right_bottom: right_bottom?,
                    })
                }
            }
        };
        let opacity = evaluate_variable_easing(image_required_params.opacity.clone(), frames.clone(), || Opacity::OPAQUE).await;
        let blend_mode = evaluate_variable_all(image_required_params.blend_mode.clone(), frames.clone()).await;
        let composite_operation = evaluate_variable_all(image_required_params.composite_operation.clone(), frames.clone()).await;
        // let (transform, opacity, blend_mode, composite_operation) = tokio::join!(transform, opacity, blend_mode, composite_operation);
        Some(ImageRequiredParamsFrameVariable {
            aspect_ratio: image_required_params.aspect_ratio,
            transform: transform?,
            background_color: image_required_params.background_color,
            opacity,
            blend_mode,
            composite_operation,
        })
    } else {
        None
    };
    let value = match processor.get_processor().await {
        ComponentProcessorBody::Native(native_processor) => {
            let variable_parameter_tasks = variable_parameters
                .into_iter()
                .map(|param| {
                    let marker_ranges = Arc::clone(&marker_ranges);
                    let frames = frames.clone();
                    let image_source_tree = Arc::clone(&image_source_tree);
                    let audio_source_tree = Arc::clone(&audio_source_tree);
                    tokio::spawn(async move { shift_time(param, marker_ranges, frames, &image_source_tree, &audio_source_tree).await })
                })
                .collect::<Vec<_>>();
            let variable_parameters = stream::iter(variable_parameter_tasks).then(|task| async move { task.await.unwrap() }).collect::<Vec<_>>().await;
            native_processor
                .iter()
                .find_map(|processor| {
                    let processor_executable = processor(fixed_parameters, &variable_parameters);
                    // 画像から文字列をとるみたいな場合に評価を遅延しなければならない　とりあえず初版ではエラーにするか？
                    if !processor_executable.processor.return_type().equals_type(&value_type) {
                        return None;
                    }
                    Some(evaluate_processor_executable(processor_executable, frames.clone(), image_required_params.as_ref(), audio_required_params, &image_source_tree, &audio_source_tree))
                })
                .ok_or(RendererError::NativeProcessorNotFound)??
        }
        ComponentProcessorBody::Component(component_processor) => {
            let variable_parameter_tasks = variable_parameters
                .into_iter()
                .map(|param| {
                    let marker_ranges = Arc::clone(&marker_ranges);
                    let image_source_tree = Arc::clone(&image_source_tree);
                    let audio_source_tree = Arc::clone(&audio_source_tree);
                    tokio::task::spawn_blocking(move || unwrap_or_default(param, marker_ranges, &image_source_tree, &audio_source_tree))
                })
                .collect::<Vec<_>>();
            let variable_parameters = stream::iter(variable_parameter_tasks).then(|task| async move { task.await.unwrap() }).collect::<Vec<_>>().await;
            let (components, links) = component_processor(fixed_parameters, &variable_parameters);
            collect_cached_time(&components, &links).await;
            let mut map_time = map_time.clone();
            let frames = frames.map(move |time| map_time.map_time(time));
            let component_evaluate_tasks = components
                .iter()
                .map(AsRef::as_ref)
                .cloned()
                .zip(iter::repeat(frames.clone()))
                .map(|(component, frames)| {
                    let value_type = value_type.clone();
                    let image_source_tree = Arc::clone(&image_source_tree);
                    let audio_source_tree = Arc::clone(&audio_source_tree);
                    tokio::spawn(async move { evaluate_component(component, value_type, image_source_tree, audio_source_tree, Box::new(frames)).await })
                })
                .collect::<Vec<_>>();
            let result = stream::iter(component_evaluate_tasks)
                .fold(Ok(empty_input_buffer(&value_type, TimelineTime::new(0.).unwrap(), TimelineTime::new(natural_length.as_secs_f64()).unwrap())), |acc, component| async move {
                    let acc = acc?;
                    match component.await.unwrap() {
                        Ok((left, value, right)) => {
                            let left = left.upgrade().unwrap().read().await.cached_timeline_time();
                            let right = right.upgrade().unwrap().read().await.cached_timeline_time();
                            Ok(combine_params(acc, frame_variable_value_into_input_buffer(value, left, right)).await)
                        }
                        Err(RendererError::NativeProcessorNotFound) => Ok(acc),
                        Err(e) => Err(e),
                    }
                })
                .await?;
            drop(components);
            drop(links);
            into_frame_variable_value(result, frames, &image_source_tree, &audio_source_tree)
        }
    };
    // 時間シフトを戻す
    let value = map_time_reverse(value, map_time);
    Ok((left.clone(), value, right.clone()))
}

async fn evaluate_variable<T: ParameterValueType<'static>, ID: IdGenerator + 'static>(
    left_time: TimelineTime,
    right_time: TimelineTime,
    param: Vector3<VariableParameterValue<T, TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, EasingValue<f64>>, TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, Option<EasingValue<f64>>>>>,
    image_source_tree: &Arc<SourceTree<TagImage, ImageNativeTreeNode<T>, ID>>,
    audio_source_tree: &Arc<SourceTree<TagAudio, AudioNativeTreeNode<T>, ID>>,
    frames: impl Iterator<Item = TimelineTime> + Send + Sync + Clone + 'static,
) -> Result<FrameVariableValue<Vector3<f64>>, RendererError> {
    let value = {
        let frames_ref = &frames;
        let param = param.map(|param| async move {
            match param {
                VariableParameterValue::Manually(param) => Ok(param.map_time_value_async(|t| async move { t.upgrade().unwrap().read().await.cached_timeline_time() }, |v| async move { Some(Either::Left(v)) }).await),
                VariableParameterValue::MayComponent { params, components, priority } => {
                    let component_values = components
                        .iter()
                        .cloned()
                        .map(|comp| {
                            let frames = frames_ref.clone();
                            let image_source_tree = Arc::clone(image_source_tree);
                            let audio_source_tree = Arc::clone(audio_source_tree);
                            tokio::spawn(async move { evaluate_component(comp, ParameterType::RealNumber(None), image_source_tree, audio_source_tree, Box::new(frames)).await })
                        })
                        .collect::<Vec<_>>();
                    let component_values = stream::iter(component_values).filter_map(|task| async move {
                        let (left, value, right) = match task.await.unwrap() {
                            Ok(value) => value,
                            Err(RendererError::NativeProcessorNotFound) => return None,
                            Err(e) => return Some(Err(e)),
                        };
                        let value = if let Ok(value) = value.into_real_number() {
                            value
                        } else {
                            return Some(Err(RendererError::InvalidComponent));
                        };
                        let left = left.upgrade().unwrap().read().await.cached_timeline_time();
                        let right = right.upgrade().unwrap().read().await.cached_timeline_time();
                        Some(Ok(TimeSplitValue::new(left, Some(Either::Right(value)), right)))
                    });
                    Ok(match priority {
                        VariableParameterPriority::PrioritizeManually => {
                            override_time_split_value(
                                component_values.fold(Ok(TimeSplitValue::new(left_time, None, right_time)), |acc, value| async move { Ok(override_time_split_value(acc?, value?).await) }).await?,
                                params.map_time_value_async(|t| async move { t.upgrade().unwrap().read().await.cached_timeline_time() }, |v| async move { v.map(Either::Left) }).await,
                            )
                            .await
                        }
                        VariableParameterPriority::PrioritizeComponent => {
                            component_values
                                .fold(
                                    Ok(params.map_time_value_async(|t| async move { t.upgrade().unwrap().read().await.cached_timeline_time() }, |v| async move { v.map(Either::Left) }).await),
                                    |acc, value| async move { Ok(override_time_split_value(acc?, value?).await) },
                                )
                                .await?
                        }
                    })
                }
            }
        });
        Vector3 {
            x: param.x.await?,
            y: param.y.await?,
            z: param.z.await?,
        }
    };
    Ok(frame_values_easing(value.into(), Vector3::from, frames, Default::default))
}

async fn evaluate_variable_easing<T: Clone>(param: TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, EasingValue<T>>, frames: impl Iterator<Item = TimelineTime> + Send + Sync + Clone + 'static, default: impl Fn() -> T) -> FrameVariableValue<T> {
    let value = param.map_time_value_async(|t| async move { t.upgrade().unwrap().read().await.cached_timeline_time() }, |v| async move { Some(Either::Left(v)) }).await;
    frame_values_easing([value], |[v]| v, frames, default)
}

async fn evaluate_variable_all<Value: Default + Clone>(param: TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, Value>, frames: impl Iterator<Item = TimelineTime> + Send + Sync + Clone + 'static) -> FrameVariableValue<Value> {
    let value = param.map_time_value_async(|t| async move { t.upgrade().unwrap().read().await.cached_timeline_time() }, |v| async move { Some(Either::Left(v)) }).await;
    frame_values(value, frames)
}

async fn evaluate_parameter<ID: IdGenerator + 'static, T: ParameterValueType<'static>>(
    left_time: TimelineTime,
    right_time: TimelineTime,
    param: &VariableParameterValue<T, ParameterValue, ParameterNullableValue>,
    param_type: &ParameterType,
    frames: impl Iterator<Item = TimelineTime> + Send + Sync + Clone + 'static,
    image_source_tree: &Arc<SourceTree<TagImage, ImageNativeTreeNode<T>, ID>>,
    audio_source_tree: &Arc<SourceTree<TagAudio, AudioNativeTreeNode<T>, ID>>,
) -> Result<Parameter<'static, ComponentProcessorInputBuffer>, RendererError> {
    let param = match param {
        VariableParameterValue::Manually(param) => value_as_input_buffer(param).await,
        VariableParameterValue::MayComponent { params, components, priority } => {
            let component_values = components
                .iter()
                .cloned()
                .map(|comp| {
                    let param_type = param_type.clone();
                    let frames = frames.clone();
                    let image_source_tree = Arc::clone(&image_source_tree);
                    let audio_source_tree = Arc::clone(&audio_source_tree);
                    tokio::spawn(async move { evaluate_component(comp, param_type, image_source_tree, audio_source_tree, Box::new(frames)).await })
                })
                .collect::<Vec<_>>();
            let component_values = stream::iter(component_values).filter_map(|task| async move {
                let (left, value, right) = match task.await.unwrap() {
                    Ok(value) => value,
                    Err(RendererError::NativeProcessorNotFound) => return None,
                    Err(e) => return Some(Err(e)),
                };
                let left = left.upgrade().unwrap().read().await.cached_timeline_time();
                let right = right.upgrade().unwrap().read().await.cached_timeline_time();
                Some(Ok(frame_variable_value_into_input_buffer(value, left, right)))
            });
            match priority {
                VariableParameterPriority::PrioritizeManually => {
                    combine_params(
                        component_values.fold(Ok(empty_input_buffer(&param_type, left_time, right_time)), |acc, value| async move { Ok(combine_params(acc?, value?).await) }).await?,
                        nullable_value_as_input_buffer(params).await,
                    )
                    .await
                }
                VariableParameterPriority::PrioritizeComponent => component_values.fold(Ok(nullable_value_as_input_buffer(params).await), |acc, value| async move { Ok(combine_params(acc?, value?).await) }).await?,
            }
        }
    };
    Ok(param)
}

async fn collect_cached_time<T>(components: &[impl AsRef<StaticPointer<RwLock<ComponentInstance<T>>>>], links: &[impl AsRef<StaticPointer<RwLock<MarkerLink>>>]) {
    let links = links.iter().map(AsRef::as_ref).filter_map(StaticPointer::upgrade).collect::<Vec<_>>();
    let links = stream::iter(links.iter()).then(|link| link.read()).collect::<Vec<_>>().await;
    loop {
        let mut flg = true;
        for link in &links {
            let equals = link.from == link.to;
            if let Some((from, to)) = link.from.upgrade().zip(link.to.upgrade()) {
                let from = {
                    let guard = from.read().await;
                    let time = guard.cached_timeline_time();
                    drop(guard);
                    time
                };
                let len = link.len.value();
                let mut guard = to.write().await;
                if ((from.value() + len) - guard.cached_timeline_time().value()).abs() > 1e-6 {
                    guard.cache_timeline_time(TimelineTime::new(from.value() + len).unwrap());
                    flg = false;
                }
            }
        }
        if flg {
            break;
        }
    }
}
