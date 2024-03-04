use async_trait::async_trait;
use mpdelta_core::common::mixed_fraction::MixedFraction;
use mpdelta_core::component::class::{ComponentClass, ComponentClassIdentifier};
use mpdelta_core::component::instance::ComponentInstance;
use mpdelta_core::component::marker_pin::{MarkerPin, MarkerTime};
use mpdelta_core::component::parameter::{ImageRequiredParams, Parameter, ParameterSelect, ParameterType, ParameterValueRaw, ParameterValueType};
use mpdelta_core::component::processor::{ComponentProcessor, ComponentProcessorNative, ComponentProcessorWrapper};
use mpdelta_core::ptr::{StaticPointer, StaticPointerCow, StaticPointerOwned};
use mpdelta_core::time::TimelineTime;
use mpdelta_core_audio::multi_channel_audio::{MultiChannelAudioMutOp, MultiChannelAudioOp, MultiChannelAudioSliceMut};
use mpdelta_core_audio::{AudioProvider, AudioType};
use qcell::TCell;
use std::borrow::Cow;
use std::f64::consts::TAU;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Default)]
pub struct SineAudio();

impl SineAudio {
    pub fn new() -> SineAudio {
        SineAudio::default()
    }
}

#[async_trait]
impl<K, T: ParameterValueType<Audio = AudioType>> ComponentClass<K, T> for SineAudio {
    fn identifier(&self) -> ComponentClassIdentifier {
        ComponentClassIdentifier {
            namespace: Cow::Borrowed("mpdelta"),
            name: Cow::Borrowed("SineAudio"),
            inner_identifier: Default::default(),
        }
    }

    fn processor(&self) -> ComponentProcessorWrapper<K, T> {
        ComponentProcessorWrapper::Native(Arc::new(SineAudio::new()))
    }

    async fn instantiate(&self, this: &StaticPointer<RwLock<dyn ComponentClass<K, T>>>) -> ComponentInstance<K, T> {
        let left = StaticPointerOwned::new(TCell::new(MarkerPin::new(TimelineTime::ZERO, MarkerTime::ZERO)));
        let right = StaticPointerOwned::new(TCell::new(MarkerPin::new(TimelineTime::new(MixedFraction::from_integer(1)), MarkerTime::new(MixedFraction::from_integer(1)).unwrap())));
        let image_required_params = ImageRequiredParams::new_default(StaticPointerOwned::reference(&left), StaticPointerOwned::reference(&right));
        ComponentInstance::new(
            this.clone(),
            StaticPointerCow::Owned(left),
            StaticPointerCow::Owned(right),
            Vec::new(),
            Some(image_required_params),
            None,
            Box::new([]),
            Box::new([]),
            Arc::new(SineAudio::new()) as Arc<dyn ComponentProcessorNative<K, T>>,
        )
    }
}

#[async_trait]
impl<K, T: ParameterValueType<Audio = AudioType>> ComponentProcessor<K, T> for SineAudio {
    async fn fixed_parameter_types(&self) -> &[(String, ParameterType)] {
        &[]
    }

    async fn update_variable_parameter(&self, _: &[ParameterValueRaw<T::Image, T::Audio>], variable_parameters: &mut Vec<(String, ParameterType)>) {
        variable_parameters.clear();
    }

    async fn natural_length(&self, _: &[ParameterValueRaw<T::Image, T::Audio>]) -> MarkerTime {
        MarkerTime::new(MixedFraction::ONE).unwrap()
    }
}

#[async_trait]
impl<K, T: ParameterValueType<Audio = AudioType>> ComponentProcessorNative<K, T> for SineAudio {
    fn supports_output_type(&self, out: Parameter<ParameterSelect>) -> bool {
        matches!(out, Parameter::Audio(_))
    }

    async fn process(&self, _: &[ParameterValueRaw<T::Image, T::Audio>], _: &[ParameterValueRaw<T::Image, T::Audio>], _: &[(String, ParameterType)], _: TimelineTime, _: Parameter<ParameterSelect>) -> ParameterValueRaw<T::Image, T::Audio> {
        Parameter::Audio(AudioType::new(SineAudio::new()))
    }
}

impl AudioProvider for SineAudio {
    fn sample_rate(&self) -> u32 {
        96_000
    }

    fn channels(&self) -> usize {
        1
    }

    fn compute_audio(&mut self, begin: TimelineTime, mut dst: MultiChannelAudioSliceMut<f32>) -> usize {
        let begin = begin.value().into_f64();
        assert!(dst.channels() >= 1);
        if dst.channels() == 1 {
            for (i, line) in dst.iter_mut().enumerate() {
                let x = (begin + i as f64 / 96000.) * 440. * TAU;
                let value = f32::sin(x as f32);
                line[0] = value;
            }
        } else {
            for (i, line) in dst.iter_mut().enumerate() {
                let x = (begin + i as f64 / 96000.) * 440. * TAU;
                let value = f32::sin(x as f32);
                line[..2].fill(value);
            }
        }
        dst.len()
    }
}
