use crate::parameter::file_reader::FileReaderParam;
use async_trait::async_trait;
use crossbeam_queue::SegQueue;
use media_loader::{AudioReader, VideoReader};
use mpdelta_core::common::mixed_fraction::MixedFraction;
use mpdelta_core::component::class::{ComponentClass, ComponentClassIdentifier};
use mpdelta_core::component::instance::ComponentInstance;
use mpdelta_core::component::marker_pin::{MarkerPin, MarkerTime};
use mpdelta_core::component::parameter::value::DynEditableSingleValue;
use mpdelta_core::component::parameter::{AbstractFile, AudioRequiredParams, FileAbstraction, ImageRequiredParams, Parameter, ParameterSelect, ParameterType, ParameterValueRaw, ParameterValueType};
use mpdelta_core::component::processor::{ComponentProcessor, ComponentProcessorNative, ComponentProcessorNativeDyn, ComponentProcessorWrapper, NativeProcessorInput, NativeProcessorRequest};
use mpdelta_core::core::IdGenerator;
use mpdelta_core::ptr::StaticPointer;
use mpdelta_core::time::TimelineTime;
use mpdelta_core_audio::AudioType;
use mpdelta_core_vulkano::ImageType;
use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex as TokioMutex, RwLock};
use uuid::Uuid;
use vulkano::buffer::{Buffer, BufferCreateInfo, BufferUsage, Subbuffer};
use vulkano::command_buffer::allocator::StandardCommandBufferAllocator;
use vulkano::command_buffer::{AutoCommandBufferBuilder, CommandBufferUsage, CopyBufferToImageInfo, PrimaryCommandBufferAbstract};
use vulkano::device::Queue;
use vulkano::format::Format;
use vulkano::image::{Image, ImageCreateInfo, ImageUsage};
use vulkano::memory::allocator::{AllocationCreateInfo, FreeListAllocator, GenericMemoryAllocator, MemoryTypeFilter};
use vulkano::sync::{GpuFuture, HostAccessError};

mod media_loader;

pub struct FfmpegMultimediaLoaderClass {
    processor: Arc<FfmpegMultimediaLoader>,
}

struct FfmpegMultimediaLoader {
    parameter_type: Arc<[(String, ParameterType)]>,
    queue: Arc<Queue>,
    gpu_memory_allocator: Arc<GenericMemoryAllocator<FreeListAllocator>>,
    command_buffer_allocator: Arc<StandardCommandBufferAllocator>,
    image_buffer_queue: SegQueue<Subbuffer<[u8]>>,
}

impl FfmpegMultimediaLoaderClass {
    pub fn new(queue: &Arc<Queue>, gpu_memory_allocator: &Arc<GenericMemoryAllocator<FreeListAllocator>>, command_buffer_allocator: &Arc<StandardCommandBufferAllocator>) -> FfmpegMultimediaLoaderClass {
        FfmpegMultimediaLoaderClass {
            processor: Arc::new(FfmpegMultimediaLoader {
                parameter_type: Arc::new([("media_file".to_owned(), ParameterType::Binary(()))]),
                queue: Arc::clone(queue),
                gpu_memory_allocator: Arc::clone(gpu_memory_allocator) as Arc<_>,
                command_buffer_allocator: Arc::clone(command_buffer_allocator),
                image_buffer_queue: SegQueue::new(),
            }),
        }
    }
}

#[async_trait]
impl<T> ComponentClass<T> for FfmpegMultimediaLoaderClass
where
    T: ParameterValueType<Image = ImageType, Audio = AudioType>,
{
    fn human_readable_identifier(&self) -> &str {
        "MultiMedia Loader (FFmpeg)"
    }

    fn identifier(&self) -> ComponentClassIdentifier {
        ComponentClassIdentifier {
            namespace: Cow::Borrowed("mpdelta"),
            name: Cow::Borrowed("FfmpegMultimediaLoader"),
            inner_identifier: Default::default(),
        }
    }

    fn processor(&self) -> ComponentProcessorWrapper<T> {
        ComponentProcessorWrapper::Native(Arc::clone(&self.processor) as _)
    }

    async fn instantiate(&self, this: &StaticPointer<RwLock<dyn ComponentClass<T>>>, id: &dyn IdGenerator) -> ComponentInstance<T> {
        let left = MarkerPin::new(id.generate_new(), MarkerTime::ZERO);
        let right = MarkerPin::new(id.generate_new(), MarkerTime::new(MixedFraction::from_integer(1)).unwrap());
        // TODO: Imageを含むかどうかや音声のチャンネル数はFixedParameterが決まらないと取得できないので良い感じにする
        let image_required_params = ImageRequiredParams::new_default(left.id(), right.id());
        let audio_required_params = AudioRequiredParams::new_default(left.id(), right.id(), 2);
        ComponentInstance::builder(this.clone(), left, right, Vec::new(), Arc::clone(&self.processor) as Arc<dyn ComponentProcessorNativeDyn<T>>)
            .image_required_params(image_required_params)
            .audio_required_params(audio_required_params)
            .fixed_parameters(Arc::clone(&self.processor.parameter_type), Arc::new([Parameter::Binary(DynEditableSingleValue::new(FileReaderParam::new(PathBuf::new())))]))
            .build(id)
    }
}

#[async_trait]
impl<T> ComponentProcessor<T> for FfmpegMultimediaLoader
where
    T: ParameterValueType<Image = ImageType, Audio = AudioType>,
{
    async fn fixed_parameter_types(&self) -> &[(String, ParameterType)] {
        &self.parameter_type
    }

    async fn update_variable_parameter(&self, _: &[ParameterValueRaw<T::Image, T::Audio>], variable_parameters: &mut Vec<(String, ParameterType)>) {
        variable_parameters.clear();
    }

    async fn num_interprocess_pins(&self, _: &[ParameterValueRaw<T::Image, T::Audio>]) -> usize {
        0
    }
}

#[async_trait]
impl<T> ComponentProcessorNative<T> for FfmpegMultimediaLoader
where
    T: ParameterValueType<Image = ImageType, Audio = AudioType>,
{
    type WholeComponentCacheKey = Uuid;
    type WholeComponentCacheValue = CachePair;
    type FramedCacheKey = ();
    type FramedCacheValue = ();

    fn whole_component_cache_key(&self, fixed_parameters: &[ParameterValueRaw<T::Image, T::Audio>], _: &[TimelineTime]) -> Option<Self::WholeComponentCacheKey> {
        let [Parameter::Binary(file)] = fixed_parameters else { panic!() };
        Some(file.identifier())
    }

    fn framed_cache_key(&self, _parameters: NativeProcessorInput<'_, T>, _time: TimelineTime, _output_type: Parameter<ParameterSelect>) -> Option<Self::FramedCacheKey> {
        None
    }

    async fn natural_length(&self, fixed_params: &[ParameterValueRaw<T::Image, T::Audio>], cache: &mut Option<Arc<Self::WholeComponentCacheValue>>) -> Option<MarkerTime> {
        let [Parameter::Binary(file)] = fixed_params else { panic!() };
        let cache = setup_cache(cache, file);
        let duration = if let Some(video_reader) = &cache.video_reader { video_reader.lock().await.duration() } else { None };
        if let Some(audio_reader) = &cache.audio_reader {
            match (duration, audio_reader.duration()) {
                (Some(video_duration), Some(audio_duration)) => MarkerTime::new(video_duration.max(audio_duration)),
                (None, Some(duration)) | (Some(duration), None) => MarkerTime::new(duration),
                (None, None) => None,
            }
        } else {
            duration.and_then(MarkerTime::new)
        }
    }

    async fn supports_output_type(&self, fixed_params: &[ParameterValueRaw<T::Image, T::Audio>], out: Parameter<ParameterSelect>, cache: &mut Option<Arc<Self::WholeComponentCacheValue>>) -> bool {
        let [Parameter::Binary(file)] = fixed_params else { panic!() };
        let cache = setup_cache(cache, file);
        match out {
            Parameter::Image(_) => cache.video_reader.is_some(),
            Parameter::Audio(_) => cache.audio_reader.is_some(),
            _ => false,
        }
    }

    async fn process(
        &self,
        parameters: NativeProcessorInput<'_, T>,
        time: TimelineTime,
        output_type: Parameter<NativeProcessorRequest>,
        whole_component_cache: &mut Option<Arc<Self::WholeComponentCacheValue>>,
        _framed_cache: &mut Option<Arc<Self::FramedCacheValue>>,
    ) -> ParameterValueRaw<T::Image, T::Audio> {
        let NativeProcessorInput { fixed_parameters: [Parameter::Binary(file)], .. } = parameters else { panic!() };
        let cache = setup_cache(whole_component_cache, file);
        match output_type {
            Parameter::Image((_, _)) => {
                let mut guard = cache.video_reader.as_ref().unwrap().lock().await;
                let image = guard.read_image_at(time);
                drop(guard);
                let buffer_len = u64::from(image.width()) * u64::from(image.height()) * 4;

                let mut buffer;
                let mut buffer_lock = 'lock: {
                    if let Some(b) = self.image_buffer_queue.pop().and_then(|buffer| (buffer.len() >= buffer_len).then_some(buffer)) {
                        buffer = b;
                        match buffer.write() {
                            Ok(buffer) => break 'lock buffer,
                            Err(HostAccessError::AccessConflict(_)) => {}
                            Err(err) => panic!("Unexpected error: {}", err),
                        }
                        self.image_buffer_queue.push(buffer);
                    }
                    buffer = Buffer::new_slice::<u8>(
                        Arc::clone(&self.gpu_memory_allocator) as Arc<_>,
                        BufferCreateInfo {
                            usage: BufferUsage::TRANSFER_SRC,
                            ..BufferCreateInfo::default()
                        },
                        AllocationCreateInfo {
                            memory_type_filter: MemoryTypeFilter::HOST_RANDOM_ACCESS,
                            ..AllocationCreateInfo::default()
                        },
                        buffer_len,
                    )
                    .unwrap();
                    buffer.write().unwrap()
                };
                let flat_samples = image.as_flat_samples();
                buffer_lock[..buffer_len as usize].copy_from_slice(&flat_samples.samples[..buffer_len as usize]);
                drop(buffer_lock);
                let gpu_image = Image::new(
                    Arc::clone(&self.gpu_memory_allocator) as Arc<_>,
                    ImageCreateInfo {
                        format: Format::R8G8B8A8_UNORM,
                        extent: [image.width(), image.height(), 1],
                        usage: ImageUsage::SAMPLED | ImageUsage::TRANSFER_DST,
                        ..ImageCreateInfo::default()
                    },
                    AllocationCreateInfo::default(),
                )
                .unwrap();
                let command_buffer = {
                    let mut builder = AutoCommandBufferBuilder::primary(&self.command_buffer_allocator, self.queue.queue_family_index(), CommandBufferUsage::OneTimeSubmit).unwrap();
                    builder.copy_buffer_to_image(CopyBufferToImageInfo::buffer_image(buffer.slice(..buffer_len), Arc::clone(&gpu_image))).unwrap();
                    builder.build().unwrap()
                };
                command_buffer.execute(Arc::clone(&self.queue)).unwrap().then_signal_fence_and_flush().unwrap().await.unwrap();
                Parameter::Image(ImageType(gpu_image))
            }
            Parameter::Audio(()) => Parameter::Audio(cache.audio_reader.clone().map(AudioType::new).unwrap()),
            _ => unreachable!(),
        }
    }
}

fn setup_cache<'a>(cache: &'a mut Option<Arc<CachePair>>, file: &AbstractFile) -> &'a CachePair {
    cache.get_or_insert_with(|| {
        let video_reader = VideoReader::new(file.clone()).map(TokioMutex::new);
        let audio_reader = AudioReader::new(file.clone());
        Arc::new(CachePair { video_reader, audio_reader })
    })
}

struct CachePair {
    video_reader: Option<TokioMutex<VideoReader<AbstractFile>>>,
    audio_reader: Option<AudioReader<AbstractFile>>,
}
