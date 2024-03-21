use crate::serde_v0::{ComponentInstanceHandleForSerialize, MarkerPinHandleForSerialize};
use async_trait::async_trait;
use cgmath::Quaternion;
use futures::{stream, FutureExt, StreamExt, TryStreamExt};
use mpdelta_core::component::class::{ComponentClass, ComponentClassIdentifier};
use mpdelta_core::component::marker_pin::MarkerPinHandle;
use mpdelta_core::component::parameter::value::{DynEditableEasingValueIdentifier, DynEditableSingleValueIdentifier, EasingIdentifier};
use mpdelta_core::component::parameter::{ParameterAllValues, ParameterValueType, ValueRaw};
use mpdelta_core::core::{ComponentClassLoader, EasingLoader, ProjectSerializer, ValueManagerLoader};
use mpdelta_core::project::{ProjectHandle, ProjectHandleOwned, RootComponentClassHandle};
use mpdelta_core::ptr::{StaticPointer, StaticPointerOwned};
use mpdelta_differential::CollectCachedTimeError;
use qcell::TCellOwner;
use std::fmt::{Debug, Formatter};
use std::io::{Read, Write};
use std::sync::Arc;
use std::{fmt, io};
use thiserror::Error;
use tokio::runtime::Handle;
use tokio::sync::RwLock;

pub mod serde_v0;

pub struct MPDeltaProjectSerializer<K: 'static, C, P: ParameterValueType, Q, E> {
    key: Arc<RwLock<TCellOwner<K>>>,
    runtime: Handle,
    component_class_loader: C,
    value_managers: ParameterAllValues<P>,
    quaternion_manager: Q,
    easing_manager: E,
}

impl<K: 'static, C, P: ParameterValueType, Q, E> MPDeltaProjectSerializer<K, C, P, Q, E> {
    pub fn new<T>(key: Arc<RwLock<TCellOwner<K>>>, runtime: Handle, component_class_loader: C, value_managers: ParameterAllValues<P>, quaternion_manager: Q, easing_manager: E) -> MPDeltaProjectSerializer<K, C, P, Q, E>
    where
        K: 'static,
        T: ParameterValueType,
        C: ComponentClassLoader<K, T> + Clone + 'static,
        P: ParameterValueType,
        P::Image: ValueManagerLoader<<ValueRaw<T::Image, T::Audio> as ParameterValueType>::Image>,
        P::Audio: ValueManagerLoader<<ValueRaw<T::Image, T::Audio> as ParameterValueType>::Audio>,
        P::Binary: ValueManagerLoader<<ValueRaw<T::Image, T::Audio> as ParameterValueType>::Binary>,
        P::String: ValueManagerLoader<<ValueRaw<T::Image, T::Audio> as ParameterValueType>::String>,
        P::Integer: ValueManagerLoader<<ValueRaw<T::Image, T::Audio> as ParameterValueType>::Integer>,
        P::RealNumber: ValueManagerLoader<<ValueRaw<T::Image, T::Audio> as ParameterValueType>::RealNumber>,
        P::Boolean: ValueManagerLoader<<ValueRaw<T::Image, T::Audio> as ParameterValueType>::Boolean>,
        P::Dictionary: ValueManagerLoader<<ValueRaw<T::Image, T::Audio> as ParameterValueType>::Dictionary>,
        P::Array: ValueManagerLoader<<ValueRaw<T::Image, T::Audio> as ParameterValueType>::Array>,
        P::ComponentClass: ValueManagerLoader<<ValueRaw<T::Image, T::Audio> as ParameterValueType>::ComponentClass>,
        Q: ValueManagerLoader<Quaternion<f64>> + Clone + 'static,
        E: EasingLoader + Clone + 'static,
    {
        MPDeltaProjectSerializer {
            key,
            runtime,
            component_class_loader,
            value_managers,
            quaternion_manager,
            easing_manager,
        }
    }
}

#[derive(Error)]
pub enum SerializeError<K: 'static, T: ParameterValueType> {
    #[error("invalid project handle: {0:?}")]
    InvalidProjectHandle(ProjectHandle<K, T>),
    #[error("invalid root component class handle: {0:?}")]
    InvalidRootComponentClassHandle(RootComponentClassHandle<K, T>),
    #[error("invalid component class handle: {0:?}")]
    InvalidComponentClassHandle(StaticPointer<RwLock<dyn ComponentClass<K, T>>>),
    #[error("invalid marker pin handle: {0:?}")]
    InvalidMarkerPinHandle(MarkerPinHandle<K>),
    #[error("io error: {0}")]
    IoError(#[from] io::Error),
    #[error("error in serialization: {0}")]
    CiboriumError(#[from] ciborium::ser::Error<io::Error>),
}

impl<K: 'static, T: ParameterValueType> Debug for SerializeError<K, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            SerializeError::InvalidProjectHandle(value) => f.debug_tuple("InvalidProjectHandle").field(value).finish(),
            SerializeError::InvalidRootComponentClassHandle(value) => f.debug_tuple("InvalidRootComponentClassHandle").field(value).finish(),
            SerializeError::InvalidComponentClassHandle(value) => f.debug_tuple("InvalidComponentClassHandle").field(value).finish(),
            SerializeError::InvalidMarkerPinHandle(value) => f.debug_tuple("InvalidMarkerPinHandle").field(value).finish(),
            SerializeError::IoError(value) => f.debug_tuple("IOError").field(value).finish(),
            SerializeError::CiboriumError(value) => f.debug_tuple("CiboriumError").field(value).finish(),
        }
    }
}

#[derive(Error)]
pub enum DeserializeError<K> {
    #[error("Unknown pin: {0:?}")]
    UnknownPin(MarkerPinHandleForSerialize),
    #[error("Unknown component class: {0:?}")]
    UnknownComponentClass(ComponentClassIdentifier<'static>),
    #[error("Unknown single value: {0:?}")]
    UnknownSingleValue(DynEditableSingleValueIdentifier<'static>),
    #[error("error in deserialization: {0}")]
    ValueDeserializationError(#[from] erased_serde::Error),
    #[error("Unknown easing value: {0:?}")]
    UnknownEasingValue(DynEditableEasingValueIdentifier<'static>),
    #[error("Unknown easing: {0:?}")]
    UnknownEasing(EasingIdentifier<'static>),
    #[error("Unknown component instance: {0:?}")]
    UnknownComponentInstanceHandle(ComponentInstanceHandleForSerialize),
    #[error("io error: {0}")]
    IoError(#[from] io::Error),
    #[error("error in deserialization: {0}")]
    CiboriumError(#[from] ciborium::de::Error<io::Error>),
    #[error("mismatch magic number: {0:?}")]
    MismatchMagicNumber([u8; 4]),
    #[error("unknown format version: {0}")]
    UnknownFormatVersion(u32),
    #[error("error in differential calculation: {0}")]
    DifferentialError(#[from] CollectCachedTimeError<K>),
}

impl<K> Debug for DeserializeError<K> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            DeserializeError::UnknownPin(value) => f.debug_tuple("UnknownPin").field(value).finish(),
            DeserializeError::UnknownComponentClass(value) => f.debug_tuple("UnknownComponentClass").field(value).finish(),
            DeserializeError::UnknownSingleValue(value) => f.debug_tuple("UnknownSingleValue").field(value).finish(),
            DeserializeError::ValueDeserializationError(value) => f.debug_tuple("ValueDeserializationError").field(value).finish(),
            DeserializeError::UnknownEasingValue(value) => f.debug_tuple("UnknownEasingValue").field(value).finish(),
            DeserializeError::UnknownEasing(value) => f.debug_tuple("UnknownEasing").field(value).finish(),
            DeserializeError::UnknownComponentInstanceHandle(value) => f.debug_tuple("UnknownComponentInstanceHandle").field(value).finish(),
            DeserializeError::IoError(value) => f.debug_tuple("IoError").field(value).finish(),
            DeserializeError::CiboriumError(value) => f.debug_tuple("CiboriumError").field(value).finish(),
            DeserializeError::MismatchMagicNumber(value) => f.debug_tuple("MismatchMagicNumber").field(value).finish(),
            DeserializeError::UnknownFormatVersion(value) => f.debug_tuple("UnknownFormatVersion").field(value).finish(),
            DeserializeError::DifferentialError(value) => f.debug_tuple("DifferentialError").field(value).finish(),
        }
    }
}

const MAGIC_NUMBER: &[u8; 4] = b"mpdl";

fn write_project<K, T: ParameterValueType>(project: &serde_v0::ProjectForSerialize<T, serde_v0::Ser>, out: impl Write) -> Result<(), SerializeError<K, T>> {
    let mut out = io::BufWriter::new(out);
    out.write_all(MAGIC_NUMBER)?;
    out.write_all(&serde_v0::FORMAT_VERSION.to_be_bytes())?;
    out.write_all(&[0u8; 24])?; // reserved
    ciborium::ser::into_writer(&project, &mut out)?;
    out.flush()?;
    Ok(())
}

fn read_project<K, T: ParameterValueType>(mut read: impl Read) -> Result<serde_v0::ProjectForSerialize<T, serde_v0::De>, DeserializeError<K>> {
    let mut header = [0u8; 32];
    read.read_exact(&mut header)?;
    let (magic_number, header) = header.split_first_chunk().unwrap();
    if magic_number != MAGIC_NUMBER {
        return Err(DeserializeError::MismatchMagicNumber(*magic_number));
    }
    let (version, reserved) = header.split_first_chunk().unwrap();
    let version = u32::from_be_bytes(*version);
    assert_eq!(reserved.len(), 24);
    match version {
        serde_v0::FORMAT_VERSION => ciborium::de::from_reader(read).map_err(DeserializeError::CiboriumError),
        _ => Err(DeserializeError::UnknownFormatVersion(version)),
    }
}

#[async_trait]
impl<K, T, C, P, Q, E> ProjectSerializer<K, T> for MPDeltaProjectSerializer<K, C, P, Q, E>
where
    K: 'static,
    T: ParameterValueType,
    C: ComponentClassLoader<K, T> + Clone + 'static,
    P: ParameterValueType,
    P::Image: ValueManagerLoader<<ValueRaw<T::Image, T::Audio> as ParameterValueType>::Image>,
    P::Audio: ValueManagerLoader<<ValueRaw<T::Image, T::Audio> as ParameterValueType>::Audio>,
    P::Binary: ValueManagerLoader<<ValueRaw<T::Image, T::Audio> as ParameterValueType>::Binary>,
    P::String: ValueManagerLoader<<ValueRaw<T::Image, T::Audio> as ParameterValueType>::String>,
    P::Integer: ValueManagerLoader<<ValueRaw<T::Image, T::Audio> as ParameterValueType>::Integer>,
    P::RealNumber: ValueManagerLoader<<ValueRaw<T::Image, T::Audio> as ParameterValueType>::RealNumber>,
    P::Boolean: ValueManagerLoader<<ValueRaw<T::Image, T::Audio> as ParameterValueType>::Boolean>,
    P::Dictionary: ValueManagerLoader<<ValueRaw<T::Image, T::Audio> as ParameterValueType>::Dictionary>,
    P::Array: ValueManagerLoader<<ValueRaw<T::Image, T::Audio> as ParameterValueType>::Array>,
    P::ComponentClass: ValueManagerLoader<<ValueRaw<T::Image, T::Audio> as ParameterValueType>::ComponentClass>,
    Q: ValueManagerLoader<Quaternion<f64>> + Clone + 'static,
    E: EasingLoader + Clone + 'static,
{
    type SerializeError = SerializeError<K, T>;

    type DeserializeError = DeserializeError<K>;

    async fn serialize_project(&self, project: &ProjectHandle<K, T>, out: impl Write + Send) -> Result<(), Self::SerializeError> {
        let project = project.upgrade().ok_or_else(|| SerializeError::InvalidProjectHandle(project.clone()))?;
        let (project, key) = tokio::join!(project.read(), Arc::clone(&self.key).read_owned());
        let core = serde_v0::ProjectForSerialize::from_core(&project, &Arc::new(key), &self.runtime).await?;
        write_project(&core, out)
    }

    async fn deserialize_project(&self, data: impl Read + Send) -> Result<ProjectHandleOwned<K, T>, Self::DeserializeError> {
        let project = read_project(data)?;
        let project_handle = project.into_core(self.component_class_loader.clone(), &self.runtime, self.value_managers.clone(), self.quaternion_manager.clone(), self.easing_manager.clone(), &self.key).await?;
        let project_read = project_handle.read().await;
        let key = Arc::new(Arc::clone(&self.key).read_owned().await);
        stream::iter(project_read.children().iter())
            .map(Ok)
            .try_for_each_concurrent(16, |component_class| async {
                let component_class = component_class.read().await;
                let component_class = component_class.get_owned().await;
                let key = Arc::clone(&key);
                self.runtime
                    .spawn_blocking(move || mpdelta_differential::collect_cached_time(component_class.component(), component_class.link(), StaticPointerOwned::reference(component_class.left()), StaticPointerOwned::reference(component_class.right()), &key))
                    .map(Result::unwrap)
                    .await
            })
            .await?;
        drop(project_read);
        Ok(project_handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serde_v0::{
        AudioRequiredParamsForSerialize, ComponentInstanceForSerialize, De, EasingValueForSerialize, ImageRequiredParamsForSerialize, ImageRequiredParamsTransformForSerialize, ParameterNullableValueForSerialize, ParameterValueFixedForSerialize, ProjectForSerialize, RootComponentClassForSerialize,
        Ser, SerDeSelect, UnDeserialized, VariableParameterValueForSerialize, Vector3ParamsForSerialize,
    };
    use mpdelta_core::component::parameter::value::{DynEditableEasingValueMarker, DynEditableSingleValue, DynEditableSingleValueMarker};
    use proptest::proptest;

    fn vector3_params_into(params: Vector3ParamsForSerialize<Ser>) -> Vector3ParamsForSerialize<De> {
        params.map(|VariableParameterValueForSerialize { params, components, priority }| VariableParameterValueForSerialize {
            params: params.map_value(|value| value.map(easing_value_into)),
            components,
            priority,
        })
    }

    fn image_required_params_into(params: ImageRequiredParamsForSerialize<Ser>) -> ImageRequiredParamsForSerialize<De> {
        let ImageRequiredParamsForSerialize {
            aspect_ratio,
            transform,
            background_color,
            opacity,
            blend_mode,
            composite_operation,
        } = params;
        let transform = match transform {
            ImageRequiredParamsTransformForSerialize::Params { scale, translate, rotate, scale_center, rotate_center } => ImageRequiredParamsTransformForSerialize::Params {
                scale: vector3_params_into(scale),
                translate: vector3_params_into(translate),
                rotate: rotate.map_value(easing_value_into),
                scale_center: vector3_params_into(scale_center),
                rotate_center: vector3_params_into(rotate_center),
            },
            ImageRequiredParamsTransformForSerialize::Free { left_top, right_top, left_bottom, right_bottom } => ImageRequiredParamsTransformForSerialize::Free {
                left_top: vector3_params_into(left_top),
                right_top: vector3_params_into(right_top),
                left_bottom: vector3_params_into(left_bottom),
                right_bottom: vector3_params_into(right_bottom),
            },
        };
        ImageRequiredParamsForSerialize {
            aspect_ratio,
            transform,
            background_color,
            opacity: opacity.map_value(easing_value_into),
            blend_mode,
            composite_operation,
        }
    }

    fn audio_required_params_into(params: AudioRequiredParamsForSerialize<Ser>) -> AudioRequiredParamsForSerialize<De> {
        let AudioRequiredParamsForSerialize { volume } = params;
        AudioRequiredParamsForSerialize {
            volume: volume
                .into_iter()
                .map(|VariableParameterValueForSerialize { params, components, priority }| VariableParameterValueForSerialize {
                    params: params.map_value(|value| value.map(easing_value_into)),
                    components,
                    priority,
                })
                .collect(),
        }
    }

    fn single_value_into<V: 'static>(value: <Ser as SerDeSelect>::T<DynEditableSingleValue<V>>) -> <De as SerDeSelect>::T<DynEditableSingleValue<V>> {
        UnDeserialized {
            tag: value.0.manager().identifier().into_static(),
            value: serde_json::to_value(value.0).unwrap(),
        }
    }

    fn fixed_parameter_into<Image, Audio>(param: ParameterValueFixedForSerialize<Image, Audio, Ser>) -> ParameterValueFixedForSerialize<Image, Audio, De>
    where
        Image: Send + Sync + Clone + 'static,
        Audio: Send + Sync + Clone + 'static,
    {
        match param {
            ParameterValueFixedForSerialize::None => ParameterValueFixedForSerialize::None,
            ParameterValueFixedForSerialize::Image(value) => ParameterValueFixedForSerialize::Image(single_value_into(value)),
            ParameterValueFixedForSerialize::Audio(value) => ParameterValueFixedForSerialize::Audio(single_value_into(value)),
            ParameterValueFixedForSerialize::Binary(value) => ParameterValueFixedForSerialize::Binary(single_value_into(value)),
            ParameterValueFixedForSerialize::String(value) => ParameterValueFixedForSerialize::String(single_value_into(value)),
            ParameterValueFixedForSerialize::Integer(value) => ParameterValueFixedForSerialize::Integer(single_value_into(value)),
            ParameterValueFixedForSerialize::RealNumber(value) => ParameterValueFixedForSerialize::RealNumber(single_value_into(value)),
            ParameterValueFixedForSerialize::Boolean(value) => ParameterValueFixedForSerialize::Boolean(single_value_into(value)),
            ParameterValueFixedForSerialize::Dictionary(value) => ParameterValueFixedForSerialize::Dictionary(single_value_into(value)),
            ParameterValueFixedForSerialize::Array(value) => ParameterValueFixedForSerialize::Array(single_value_into(value)),
            ParameterValueFixedForSerialize::ComponentClass(value) => ParameterValueFixedForSerialize::ComponentClass(value),
        }
    }

    fn easing_value_into<V>(value: EasingValueForSerialize<V, Ser>) -> EasingValueForSerialize<V, De> {
        let EasingValueForSerialize { value, easing } = value;
        EasingValueForSerialize {
            value: UnDeserialized {
                tag: value.0.manager().identifier().into_static(),
                value: serde_json::to_value(value.0).unwrap(),
            },
            easing,
        }
    }

    fn parameter_nullable_value_into<T: ParameterValueType>(param: ParameterNullableValueForSerialize<T, Ser>) -> ParameterNullableValueForSerialize<T, De> {
        match param {
            ParameterNullableValueForSerialize::None => ParameterNullableValueForSerialize::None,
            ParameterNullableValueForSerialize::Image(value) => ParameterNullableValueForSerialize::Image(value.map_value(|value| value.map(easing_value_into))),
            ParameterNullableValueForSerialize::Audio(value) => ParameterNullableValueForSerialize::Audio(value.map_value(|value| value.map(easing_value_into))),
            ParameterNullableValueForSerialize::Binary(value) => ParameterNullableValueForSerialize::Binary(value.map_value(|value| value.map(easing_value_into))),
            ParameterNullableValueForSerialize::String(value) => ParameterNullableValueForSerialize::String(value.map_value(|value| value.map(easing_value_into))),
            ParameterNullableValueForSerialize::Integer(value) => ParameterNullableValueForSerialize::Integer(value.map_value(|value| value.map(easing_value_into))),
            ParameterNullableValueForSerialize::RealNumber(value) => ParameterNullableValueForSerialize::RealNumber(value.map_value(|value| value.map(easing_value_into))),
            ParameterNullableValueForSerialize::Boolean(value) => ParameterNullableValueForSerialize::Boolean(value.map_value(|value| value.map(easing_value_into))),
            ParameterNullableValueForSerialize::Dictionary(value) => ParameterNullableValueForSerialize::Dictionary(value),
            ParameterNullableValueForSerialize::Array(value) => ParameterNullableValueForSerialize::Array(value),
            ParameterNullableValueForSerialize::ComponentClass(value) => ParameterNullableValueForSerialize::ComponentClass(value),
        }
    }

    fn variable_parameter_into<T: ParameterValueType>(param: VariableParameterValueForSerialize<ParameterNullableValueForSerialize<T, Ser>>) -> VariableParameterValueForSerialize<ParameterNullableValueForSerialize<T, De>> {
        let VariableParameterValueForSerialize { params, components, priority } = param;
        VariableParameterValueForSerialize {
            params: parameter_nullable_value_into(params),
            components,
            priority,
        }
    }

    fn component_instance_into<T: ParameterValueType>(component: ComponentInstanceForSerialize<T, Ser>) -> ComponentInstanceForSerialize<T, De> {
        let ComponentInstanceForSerialize {
            left,
            right,
            markers,
            image_required_params,
            audio_required_params,
            fixed_parameters,
            variable_parameters,
            class,
        } = component;
        ComponentInstanceForSerialize {
            left,
            right,
            markers,
            image_required_params: image_required_params.map(image_required_params_into),
            audio_required_params: audio_required_params.map(audio_required_params_into),
            fixed_parameters: fixed_parameters.into_iter().map(fixed_parameter_into).collect(),
            variable_parameters: variable_parameters.into_iter().map(variable_parameter_into).collect(),
            class,
        }
    }

    fn root_component_class_into<T: ParameterValueType>(component: RootComponentClassForSerialize<T, Ser>) -> RootComponentClassForSerialize<T, De> {
        let RootComponentClassForSerialize { id, components, links, length } = component;
        RootComponentClassForSerialize {
            id,
            components: components.into_iter().map(component_instance_into).collect(),
            links,
            length,
        }
    }

    fn project_into<T: ParameterValueType>(project: ProjectForSerialize<T, Ser>) -> ProjectForSerialize<T, De> {
        let ProjectForSerialize { id, components } = project;
        ProjectForSerialize {
            id,
            components: components.into_iter().map(root_component_class_into).collect(),
        }
    }

    #[derive(Debug, PartialEq)]
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

    proptest! {
        #[test]
        fn test_serialize_deserialize_project(project in serde_v0::proptest::project::<T>()) {
            let mut data = Vec::new();
            write_project::<(), T>(&project, &mut data).unwrap();
            let project_deserialized = read_project::<(), T>(data.as_slice()).unwrap();
            assert_eq!(project_into(project), project_deserialized);
        }
    }
}
