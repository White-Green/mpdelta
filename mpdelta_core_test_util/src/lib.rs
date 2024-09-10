use async_trait::async_trait;
use mpdelta_core::component::class::{ComponentClass, ComponentClassIdentifier};
use mpdelta_core::component::instance::ComponentInstance;
use mpdelta_core::component::marker_pin::MarkerTime;
use mpdelta_core::component::parameter::{Parameter, ParameterSelect, ParameterType, ParameterValueRaw, ParameterValueType};
use mpdelta_core::component::processor::{ComponentProcessor, ComponentProcessorNative, ComponentProcessorWrapper, NativeProcessorInput, NativeProcessorRequest};
use mpdelta_core::core::IdGenerator;
use mpdelta_core::project::RootComponentClassHandleOwned;
use mpdelta_core::ptr::StaticPointer;
use mpdelta_core::time::TimelineTime;
use std::collections::HashMap;
use std::fmt::{Display, Formatter, Write};
use std::iter;
use std::sync::atomic::AtomicUsize;
use std::sync::{atomic, Arc};
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Default)]
pub struct TestIdGenerator(AtomicUsize);

impl TestIdGenerator {
    pub fn new() -> Self {
        Self::default()
    }
}

impl IdGenerator for TestIdGenerator {
    fn generate_new(&self) -> Uuid {
        let id = self.0.fetch_add(1, atomic::Ordering::Relaxed);
        Uuid::from_u128(id as u128)
    }
}

pub struct NoopComponentClass;

#[async_trait]
impl<T> ComponentClass<T> for NoopComponentClass
where
    T: ParameterValueType,
{
    fn identifier(&self) -> ComponentClassIdentifier {
        unimplemented!()
    }

    fn processor(&self) -> ComponentProcessorWrapper<T> {
        unimplemented!()
    }

    async fn instantiate(&self, _: &StaticPointer<RwLock<dyn ComponentClass<T>>>, _: &dyn IdGenerator) -> ComponentInstance<T> {
        unimplemented!()
    }
}

pub struct NoopProcessor;

#[async_trait]
impl<T> ComponentProcessor<T> for NoopProcessor
where
    T: ParameterValueType,
{
    async fn fixed_parameter_types(&self) -> &[(String, ParameterType)] {
        unimplemented!()
    }

    async fn update_variable_parameter(&self, _: &[ParameterValueRaw<T::Image, T::Audio>], _: &mut Vec<(String, ParameterType)>) {
        unimplemented!()
    }
}

#[async_trait]
impl<T> ComponentProcessorNative<T> for NoopProcessor
where
    T: ParameterValueType,
{
    type WholeComponentCacheKey = ();
    type WholeComponentCacheValue = ();
    type FramedCacheKey = ();
    type FramedCacheValue = ();

    fn whole_component_cache_key(&self, _: &[ParameterValueRaw<T::Image, T::Audio>]) -> Option<Self::WholeComponentCacheKey> {
        unimplemented!()
    }

    fn framed_cache_key(&self, _: NativeProcessorInput<'_, T>, _: TimelineTime, _: Parameter<ParameterSelect>) -> Option<Self::FramedCacheKey> {
        unimplemented!()
    }

    async fn natural_length(&self, _: &[ParameterValueRaw<T::Image, T::Audio>], _: &mut Option<Arc<Self::WholeComponentCacheValue>>) -> Option<MarkerTime> {
        unimplemented!()
    }

    async fn supports_output_type(&self, _: &[ParameterValueRaw<T::Image, T::Audio>], _: Parameter<ParameterSelect>, _: &mut Option<Arc<Self::WholeComponentCacheValue>>) -> bool {
        unimplemented!()
    }

    async fn process(&self, _: NativeProcessorInput<'_, T>, _: TimelineTime, _: Parameter<NativeProcessorRequest>, _: &mut Option<Arc<Self::WholeComponentCacheValue>>, _: &mut Option<Arc<Self::FramedCacheValue>>) -> ParameterValueRaw<T::Image, T::Audio> {
        unimplemented!()
    }
}

pub async fn pretty_print_root_component_class<T>(value: &RootComponentClassHandleOwned<T>) -> String
where
    T: ParameterValueType,
{
    struct MaySome<T>(Option<T>);
    impl<T> Display for MaySome<T>
    where
        T: Display,
    {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            match &self.0 {
                None => write!(f, "###Unexpected None###"),
                Some(v) => write!(f, "{v}"),
            }
        }
    }

    let mut s = String::new();
    let value = value.read().await;
    let value = value.get();
    let pin_index_map = value
        .iter_components()
        .enumerate()
        .flat_map(|(i, component)| iter::once(component.marker_left()).chain(component.markers()).chain(iter::once(component.marker_right())).enumerate().map(move |(j, p)| (*p.id(), (i, j))))
        .map(|(p, id)| (p, format!("p{}_{}", id.0, id.1)))
        .chain([(*value.left().id(), "left".to_owned()), (*value.right().id(), "right".to_owned())])
        .collect::<HashMap<_, _>>();
    let left = value.left();
    writeln!(s, "left: {:?}:{}", left.locked_component_time().map(|t| t.value()), MaySome(value.time_of_pin(left.id()).map(|v| v.value()))).unwrap();
    let right = value.right();
    writeln!(s, "right: {:?}:{}", right.locked_component_time().map(|t| t.value()), MaySome(value.time_of_pin(right.id()).map(|v| v.value()))).unwrap();
    writeln!(s, "components: [").unwrap();
    for component in value.iter_components() {
        write!(s, "    {{ markers: [").unwrap();
        let left = component.marker_left();
        write!(s, "{:?}:{} => {}, ", left.locked_component_time().map(|t| t.value()), MaySome(value.time_of_pin(left.id()).map(|v| v.value())), pin_index_map[component.marker_left().id()]).unwrap();
        for marker_pin in component.markers() {
            write!(s, "{:?}:{} => {}, ", marker_pin.locked_component_time().map(|t| t.value()), MaySome(value.time_of_pin(marker_pin.id()).map(|v| v.value())), pin_index_map[marker_pin.id()]).unwrap();
        }
        let right = component.marker_right();
        writeln!(s, "{:?}:{} => {}] }},", right.locked_component_time().map(|t| t.value()), MaySome(value.time_of_pin(right.id()).map(|v| v.value())), pin_index_map[component.marker_right().id()]).unwrap();
    }
    writeln!(s, "],").unwrap();
    writeln!(s, "links: [").unwrap();
    for link in value.iter_links() {
        writeln!(s, "    {} = {:?} => {},", pin_index_map[link.from()], link.len().value(), pin_index_map[link.to()]).unwrap();
    }
    write!(s, "]").unwrap();
    s
}
pub async fn assert_eq_root_component_class<T>(a: &RootComponentClassHandleOwned<T>, b: &RootComponentClassHandleOwned<T>)
where
    T: ParameterValueType,
{
    let result = async {
        let (a, b) = tokio::join!(a.read(), b.read());
        let a = a.get();
        let b = b.get();
        (a.left().locked_component_time() == b.left().locked_component_time()).then_some(())?;
        (a.right().locked_component_time() == b.right().locked_component_time()).then_some(())?;
        (a.time_of_pin(a.left().id())? == b.time_of_pin(b.left().id())?).then_some(())?;
        (a.time_of_pin(a.right().id())? == b.time_of_pin(b.right().id())?).then_some(())?;
        (a.iter_components().count() == b.iter_components().count()).then_some(())?;
        let pin_index_map_a = a
            .iter_components()
            .enumerate()
            .flat_map(|(i, component)| iter::once(component.marker_left()).chain(component.markers()).chain(iter::once(component.marker_right())).enumerate().map(move |(j, p)| (*p.id(), (i, j))))
            .chain([(*a.left().id(), (usize::MAX, 0)), (*a.right().id(), (usize::MAX, 1))])
            .collect::<HashMap<_, _>>();
        let pin_index_map_b = b
            .iter_components()
            .enumerate()
            .flat_map(|(i, component)| iter::once(component.marker_left()).chain(component.markers()).chain(iter::once(component.marker_right())).enumerate().map(move |(j, p)| (*p.id(), (i, j))))
            .chain([(*b.left().id(), (usize::MAX, 0)), (*b.right().id(), (usize::MAX, 1))])
            .collect::<HashMap<_, _>>();
        a.iter_components().zip(b.iter_components()).try_for_each(|(instance_a, instance_b)| {
            (instance_a.markers().len() == instance_b.markers().len()).then_some(())?;
            let mut iter = iter::once(instance_a.marker_left())
                .chain(instance_a.markers())
                .chain(iter::once(instance_a.marker_right()))
                .zip(iter::once(instance_b.marker_left()).chain(instance_b.markers()).chain(iter::once(instance_b.marker_right())));
            iter.try_for_each(|(pin_a, pin_b)| (pin_a.locked_component_time() == pin_b.locked_component_time() && (a.time_of_pin(pin_a.id())? == b.time_of_pin(pin_b.id())?)).then_some(()))
        })?;
        (a.iter_links().count() == b.iter_links().count()).then_some(())?;
        a.iter_links().zip(b.iter_links()).try_for_each(|(a, b)| {
            (pin_index_map_a[a.from()] == pin_index_map_b[b.from()] && pin_index_map_a[a.to()] == pin_index_map_b[b.to()] && a.len() == b.len() || pin_index_map_a[a.from()] == pin_index_map_b[b.to()] && pin_index_map_a[a.to()] == pin_index_map_b[b.from()] && a.len() == -b.len()).then_some(())
        })?;
        Some(())
    }
    .await;
    if result.is_none() {
        let a = pretty_print_root_component_class(a).await;
        let b = pretty_print_root_component_class(b).await;
        panic!("assertion failed\n# left\n{a}\n# right\n{b}");
    }
}

#[macro_export]
macro_rules! root_component_class {
    (
        custom_differential: $custom_differential:expr;
        $name:ident; <$t:ty>;
        $id:expr;
        $(left: $left:ident,)?
        $(right: $right:ident,)?
        components: [$({
            markers: [$($pin:expr$( => $pin_name:ident)?),*$(,)?]
            $(, processor: $processor:expr)?
        }$(;$component_name:ident)?),*$(,)?],
        links: [$($from:ident = $len:expr => $to:expr $(;$link_name:ident)?),*$(,)?]$(,)?
    ) => {
        $crate::root_component_class!(
            @inner;
            $custom_differential;
            $name; <$t>;
            $id;
            $(left: $left,)?
            $(right: $right,)?
            components: [$({
                markers: [$($pin$( => $pin_name)?),*]
                $(, processor: $processor)?
            }$(;$component_name)?),*],
            links: [$($from = $len => $to $(;$link_name)?),*],
        )
    };
    (
        $name:ident; <$t:ty>;
        $id:expr;
        $(left: $left:ident,)?
        $(right: $right:ident,)?
        components: [$({
            markers: [$($pin:expr$( => $pin_name:ident)?),*$(,)?]
            $(, processor: $processor:expr)?
        }$(;$component_name:ident)?),*$(,)?],
        links: [$($from:ident = $len:expr => $to:expr $(;$link_name:ident)?),*$(,)?]$(,)?
    ) => {
        $crate::root_component_class!(
            @inner;
            ::mpdelta_differential::collect_cached_time;
            $name; <$t>;
            $id;
            $(left: $left,)?
            $(right: $right,)?
            components: [$({
                markers: [$($pin$( => $pin_name)?),*]
                $(, processor: $processor)?
            }$(;$component_name)?),*],
            links: [$($from = $len => $to $(;$link_name)?),*],
        )
    };
    (
        @inner;
        $collect_cached_time:expr;
        $name:ident; <$t:ty>;
        $id:expr;
        $(left: $left:ident,)?
        $(right: $right:ident,)?
        components: [$({
            markers: [$($pin:expr$( => $pin_name:ident)?),*$(,)?]
            $(, processor: $processor:expr)?
        }$(;$component_name:ident)?),*$(,)?],
        links: [$($from:ident = $len:expr => $to:expr $(;$link_name:ident)?),*$(,)?]$(,)?
    ) => {
        macro_rules! marker {
            () => {
                ::mpdelta_core::component::marker_pin::MarkerPin::new_unlocked(::mpdelta_core::core::IdGenerator::generate_new(&$id))
            };
            (locked: $locked:expr) => {
                ::mpdelta_core::component::marker_pin::MarkerPin::new(
                    ::mpdelta_core::core::IdGenerator::generate_new(&$id),
                    ::mpdelta_core::component::marker_pin::MarkerTime::new(::mpdelta_core::common::mixed_fraction::MixedFraction::try_from($locked).unwrap()).unwrap(),
                )
            };
        }
        let root_component_class = ::mpdelta_core::project::RootComponentClass::<$t>::new_empty(
            ::mpdelta_core::core::IdGenerator::generate_new(&$id),
            ::mpdelta_core::ptr::StaticPointer::<::tokio::sync::RwLock<::mpdelta_core::project::Project<$t>>>::new(),
            ::mpdelta_core::core::IdGenerator::generate_new(&$id),
            &$id,
        );
        let read = root_component_class.read().await;
        let mut item = read.get_mut().await;
        let left = item.left().id();
        let right = item.right().id();
        $(#[allow(unused_assignments, unused_variables)] let $left = *left;)?
        $(#[allow(unused_assignments, unused_variables)] let $right = *right;)?
        $($($(#[allow(unused_variables)] let $pin_name;)?)*$(#[allow(unused_variables)] let $component_name;)?)*
        $($(#[allow(unused_variables)] let $link_name;)?)*
        #[allow(unused_assignments)]
        let components = [$({
            let mut markers = vec![$({
                let pin = $pin;
                $($pin_name = *pin.id();)?
                pin
            }),*];
            let marker_left = markers.remove(0);
            let marker_right = markers.pop().unwrap();
            let image_required_params = ::mpdelta_core::component::parameter::ImageRequiredParams::new_default(marker_left.id(), marker_right.id());
            let audio_required_params = ::mpdelta_core::component::parameter::AudioRequiredParams::new_default(marker_left.id(), marker_right.id(), 2);
            let [.., processor] = [::std::sync::Arc::new($crate::NoopProcessor) as ::std::sync::Arc<dyn ::mpdelta_core::component::processor::ComponentProcessorNativeDyn<$t>>, $($processor)?];
            let builder = ::mpdelta_core::component::instance::ComponentInstance::builder(
                ::mpdelta_core::ptr::StaticPointer::<::tokio::sync::RwLock<$crate::NoopComponentClass>>::new().map(|c| c as _),
                marker_left,
                marker_right,
                markers,
                ::mpdelta_core::component::processor::ComponentProcessorWrapper::from(processor),
            );
            let component = builder.image_required_params(image_required_params)
                .audio_required_params(audio_required_params)
                .build(&$id);
            $($component_name = *component.id();)?
            component
        }),*];
        #[allow(unused_assignments)]
        let links = [$({
                let len = ::mpdelta_core::time::TimelineTime::new(::mpdelta_core::common::mixed_fraction::MixedFraction::try_from($len).unwrap());
                let link = ::mpdelta_core::component::link::MarkerLink::new($from.clone(), $to.clone(), len);
                $($link_name = link.clone();)?
                link
            }),*];
        components.into_iter().for_each(|component| item.add_component(component));
        links.into_iter().for_each(|link| item.add_link(link));
        let time_map = $collect_cached_time(&*item).unwrap();
        ::mpdelta_core::project::RootComponentClassItemWrite::commit_changes(item, time_map);
        drop(read);
        let $name = root_component_class;
    }
}
