use super::*;
use mpdelta_core::common::mixed_fraction::MixedFraction;
use mpdelta_core::component::class::ComponentClass;
use mpdelta_core::component::parameter::{ParameterValueRaw, ParameterValueType};
use mpdelta_core::component::processor::{ComponentProcessor, ComponentProcessorNative, ComponentProcessorNativeDyn, NativeProcessorInput, NativeProcessorRequest};
use mpdelta_core::mfrac;
use mpdelta_core::ptr::StaticPointerOwned;
use mpdelta_core::time::TimelineTime;
use mpdelta_core_test_util::{root_component_class, TestIdGenerator};
use std::sync::Arc;
use std::time::Duration;

struct T;
impl ParameterValueType for T {
    type Image = Vec<MixedFraction>;
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

struct Processor;

#[async_trait]
impl ComponentProcessor<T> for Processor {
    async fn fixed_parameter_types(&self) -> &[(String, ParameterType)] {
        &[]
    }

    async fn update_variable_parameter(&self, _: &[ParameterValueRaw<<T as ParameterValueType>::Image, <T as ParameterValueType>::Audio>], _: &mut Vec<(String, ParameterType)>) {}
}

#[async_trait]
impl ComponentProcessorNative<T> for Processor {
    type WholeComponentCacheKey = ();
    type WholeComponentCacheValue = ();
    type FramedCacheKey = ();
    type FramedCacheValue = ();

    fn whole_component_cache_key(&self, _: &[ParameterValueRaw<<T as ParameterValueType>::Image, <T as ParameterValueType>::Audio>]) -> Option<Self::WholeComponentCacheKey> {
        None
    }

    fn framed_cache_key(&self, _: NativeProcessorInput<'_, T>, _: TimelineTime, _: Parameter<ParameterSelect>) -> Option<Self::FramedCacheKey> {
        None
    }

    async fn natural_length(&self, _: &[ParameterValueRaw<<T as ParameterValueType>::Image, <T as ParameterValueType>::Audio>], _: &mut Option<Arc<Self::WholeComponentCacheValue>>) -> Option<MarkerTime> {
        None
    }

    async fn supports_output_type(&self, _: &[ParameterValueRaw<<T as ParameterValueType>::Image, <T as ParameterValueType>::Audio>], out: Parameter<ParameterSelect>, _: &mut Option<Arc<Self::WholeComponentCacheValue>>) -> bool {
        out.equals_type(&Parameter::<ParameterSelect>::Image(()))
    }

    async fn process(
        &self,
        _: NativeProcessorInput<'_, T>,
        time: TimelineTime,
        _: Parameter<NativeProcessorRequest>,
        _: &mut Option<Arc<Self::WholeComponentCacheValue>>,
        _: &mut Option<Arc<Self::FramedCacheValue>>,
    ) -> ParameterValueRaw<<T as ParameterValueType>::Image, <T as ParameterValueType>::Audio> {
        Parameter::Image(vec![time.value()])
    }
}

struct VecCombinerBuilder;
struct VecCombiner {
    data: Vec<MixedFraction>,
}

impl CombinerBuilder<Vec<MixedFraction>> for VecCombinerBuilder {
    type Request = ImageCombinerRequest;
    type Param = ImageCombinerParam;
    type Combiner = VecCombiner;

    fn new_combiner(&self, _: Self::Request) -> Self::Combiner {
        VecCombiner { data: Vec::new() }
    }
}

impl Combiner<Vec<MixedFraction>> for VecCombiner {
    type Param = ImageCombinerParam;

    fn add(&mut self, data: Vec<MixedFraction>, _: Self::Param) {
        self.data.extend(data);
    }

    fn collect(self) -> Vec<MixedFraction> {
        self.data
    }
}

struct NoopAudioCombiner;

impl CombinerBuilder<()> for NoopAudioCombiner {
    type Request = AudioCombinerRequest;
    type Param = AudioCombinerParam;
    type Combiner = NoopAudioCombiner;

    fn new_combiner(&self, _: Self::Request) -> Self::Combiner {
        NoopAudioCombiner
    }
}

impl Combiner<()> for NoopAudioCombiner {
    type Param = AudioCombinerParam;

    fn add(&mut self, _: (), _: Self::Param) {}

    fn collect(self) -> () {}
}

struct NoopRenderingControllerBuilder;

impl MPDeltaRenderingControllerBuilder for NoopRenderingControllerBuilder {
    type Controller<F: Fn(RenderingControllerItem) + Send + Sync + 'static> = NoopRenderingController<F>;

    fn create<F: Fn(RenderingControllerItem) + Send + Sync + 'static>(&self, f: F) -> Self::Controller<F> {
        NoopRenderingController(f)
    }
}

struct NoopRenderingController<F>(F);

impl<F: Fn(RenderingControllerItem) + Send + Sync + 'static> MPDeltaRenderingController for NoopRenderingController<F> {
    fn on_request_render(&self, frame: usize) {
        self.0(RenderingControllerItem::RequestRender { frame });
    }
}

#[tokio::test]
async fn test_render() {
    let processor = Arc::new(Processor) as Arc<dyn ComponentProcessorNativeDyn<T>>;
    let id = TestIdGenerator::new();
    root_component_class! {
        root; <T>; id;
        left: left,
        right: right,
        components: [
            {
                markers: [marker!(locked: 0) => l1, marker!() => r1],
                processor: processor.clone()
            },
            {
                markers: [marker!(locked: 0) => l2, marker!(locked: 6) => r2],
                processor: processor.clone()
            },
        ],
        links: [
            left = 1 => l1,
            l1 = 2 => r1,
            l1 = 0.5 => l2,
            l2 = 3 => r2,
            r1 = 1 => right,
        ],
    }
    let instance = root.read().await.instantiate(&StaticPointerOwned::reference(&root).clone().map(|c| c as _), &id).await;
    let image_combiner_builder = Arc::new(VecCombinerBuilder);
    let audio_combiner_builder = Arc::new(NoopAudioCombiner);
    let rendering_controller_builder = Arc::new(NoopRenderingControllerBuilder);
    let runtime = Handle::current();
    let renderer_builder = MPDeltaRendererBuilder::new(image_combiner_builder, rendering_controller_builder, audio_combiner_builder, runtime);
    let renderer = renderer_builder.create_renderer(instance).await.unwrap();
    macro_rules! render_frame {
        ($frame:expr) => {{
            let frame = 'frame: {
                for _ in 0..10 {
                    match renderer.render_frame($frame) {
                        Ok(image) => {
                            break 'frame Some(image);
                        }
                        Err(RenderError::Timeout) => tokio::time::sleep(Duration::from_millis(100)).await,
                        Err(e) => panic!("{e}"),
                    }
                }
                None
            };
            frame.unwrap()
        }};
    }

    assert_eq!(render_frame!(0), vec![]);
    assert_eq!(render_frame!(59), vec![]);
    assert_eq!(render_frame!(60), vec![mfrac!(0, 60)]);
    assert_eq!(render_frame!(89), vec![mfrac!(29, 60)]);
    assert_eq!(render_frame!(90), vec![mfrac!(30, 60), mfrac!(0, 60)]);
    assert_eq!(render_frame!(179), vec![mfrac!(119, 60), mfrac!(178, 60)]);
    // assert_eq!(render_frame!(180), vec![mfrac!(180, 60)]); // TODO: これは未規定
    assert_eq!(render_frame!(181), vec![mfrac!(182, 60)]);
}
