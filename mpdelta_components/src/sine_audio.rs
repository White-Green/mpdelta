use async_trait::async_trait;
use mpdelta_core::common::mixed_fraction::MixedFraction;
use mpdelta_core::component::class::ComponentClass;
use mpdelta_core::component::instance::ComponentInstance;
use mpdelta_core::component::marker_pin::{MarkerPin, MarkerTime};
use mpdelta_core::component::parameter::{ImageRequiredParams, Parameter, ParameterSelect, ParameterType, ParameterValueFixed, ParameterValueType};
use mpdelta_core::component::processor::{ComponentProcessor, ComponentProcessorNative};
use mpdelta_core::ptr::{StaticPointer, StaticPointerCow, StaticPointerOwned};
use mpdelta_core::time::TimelineTime;
use mpdelta_core_audio::multi_channel_audio::{MultiChannelAudioMutOp, MultiChannelAudioOp, MultiChannelAudioSliceMut};
use mpdelta_core_audio::{AudioProvider, AudioType};
use qcell::TCell;
use std::f64::consts::TAU;
use std::sync::Arc;
use std::time::Duration;
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
    async fn generate_image(&self) -> bool {
        false
    }

    async fn generate_audio(&self) -> bool {
        true
    }

    async fn fixed_parameter_type(&self) -> &[(String, ParameterType)] {
        &[]
    }

    async fn default_variable_parameter_type(&self) -> &[(String, ParameterType)] {
        &[]
    }

    async fn instantiate(&self, this: &StaticPointer<RwLock<dyn ComponentClass<K, T>>>) -> ComponentInstance<K, T> {
        let left = StaticPointerOwned::new(TCell::new(MarkerPin::new(TimelineTime::ZERO, MarkerTime::ZERO)));
        let right = StaticPointerOwned::new(TCell::new(MarkerPin::new(TimelineTime::new(MixedFraction::from_integer(1)), MarkerTime::new(MixedFraction::from_integer(1)).unwrap())));
        let image_required_params = ImageRequiredParams::new_default(StaticPointerOwned::reference(&left), StaticPointerOwned::reference(&right));
        ComponentInstance::new_no_param(
            this.clone(),
            StaticPointerCow::Owned(left),
            StaticPointerCow::Owned(right),
            Some(image_required_params),
            None,
            Arc::new(SineAudio::new()) as Arc<dyn ComponentProcessorNative<K, T>>,
        )
    }
}

#[async_trait]
impl<K, T: ParameterValueType<Audio = AudioType>> ComponentProcessor<K, T> for SineAudio {
    async fn update_variable_parameter(&self, _: &mut [ParameterValueFixed<T::Image, T::Audio>], _: &mut Vec<(String, ParameterType)>) {}

    async fn natural_length(&self, _: &[ParameterValueFixed<T::Image, T::Audio>]) -> Duration {
        Duration::from_secs(1)
    }
}

#[async_trait]
impl<K, T: ParameterValueType<Audio = AudioType>> ComponentProcessorNative<K, T> for SineAudio {
    fn supports_output_type(&self, out: Parameter<ParameterSelect>) -> bool {
        matches!(out, Parameter::Audio(_))
    }

    async fn process(&self, _: &[ParameterValueFixed<T::Image, T::Audio>], _: &[ParameterValueFixed<T::Image, T::Audio>], _: &[(String, ParameterType)], _: TimelineTime, _: Parameter<ParameterSelect>) -> ParameterValueFixed<T::Image, T::Audio> {
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
