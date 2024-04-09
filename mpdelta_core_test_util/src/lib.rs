use async_trait::async_trait;
use mpdelta_core::component::class::{ComponentClass, ComponentClassIdentifier};
use mpdelta_core::component::instance::ComponentInstance;
use mpdelta_core::component::marker_pin::MarkerTime;
use mpdelta_core::component::parameter::{Parameter, ParameterSelect, ParameterType, ParameterValueRaw, ParameterValueType};
use mpdelta_core::component::processor::{ComponentProcessor, ComponentProcessorNative, ComponentProcessorWrapper, NativeProcessorInput, NativeProcessorRequest};
use mpdelta_core::project::RootComponentClassHandleOwned;
use mpdelta_core::ptr::{StaticPointer, StaticPointerOwned};
use mpdelta_core::time::TimelineTime;
use qcell::TCellOwner;
use std::collections::HashMap;
use std::fmt::Write;
use std::iter;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct NoopComponentClass;

#[async_trait]
impl<K, T> ComponentClass<K, T> for NoopComponentClass
where
    K: 'static,
    T: ParameterValueType,
{
    fn identifier(&self) -> ComponentClassIdentifier {
        unimplemented!()
    }

    fn processor(&self) -> ComponentProcessorWrapper<K, T> {
        unimplemented!()
    }

    async fn instantiate(&self, _: &StaticPointer<RwLock<dyn ComponentClass<K, T>>>) -> ComponentInstance<K, T> {
        unimplemented!()
    }
}

pub struct NoopProcessor;

#[async_trait]
impl<K, T> ComponentProcessor<K, T> for NoopProcessor
where
    K: 'static,
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
impl<K, T> ComponentProcessorNative<K, T> for NoopProcessor
where
    K: 'static,
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

pub async fn pretty_print_root_component_class<K, T>(value: &RootComponentClassHandleOwned<K, T>, key: &TCellOwner<K>) -> String
where
    K: 'static,
    T: ParameterValueType,
{
    let mut s = String::new();
    let value = value.read().await;
    let value = value.get().await;
    let pin_index_map = value
        .component()
        .iter()
        .enumerate()
        .flat_map(|(i, component)| {
            let component = component.ro(key);
            iter::once(component.marker_left())
                .chain(component.markers())
                .chain(iter::once(component.marker_right()))
                .enumerate()
                .map(move |(j, p)| (StaticPointerOwned::reference(p).clone(), (i, j)))
        })
        .map(|(p, id)| (p, format!("p{}_{}", id.0, id.1)))
        .chain([(StaticPointerOwned::reference(value.left()).clone(), "left".to_owned()), (StaticPointerOwned::reference(value.right()).clone(), "right".to_owned())])
        .collect::<HashMap<_, _>>();
    let left = value.left().ro(key);
    writeln!(s, "left: {:?}:{}", left.locked_component_time().map(|t| t.value()), left.cached_timeline_time().value()).unwrap();
    let right = value.right().ro(key);
    writeln!(s, "right: {:?}:{}", right.locked_component_time().map(|t| t.value()), right.cached_timeline_time().value()).unwrap();
    writeln!(s, "components: [").unwrap();
    for component in value.component() {
        write!(s, "    {{ markers: [").unwrap();
        let component = component.ro(key);
        let left = component.marker_left().ro(key);
        write!(s, "{:?}:{} => {}, ", left.locked_component_time().map(|t| t.value()), left.cached_timeline_time().value(), pin_index_map[component.marker_left().as_ref()]).unwrap();
        for marker in component.markers() {
            let marker_pin = marker.ro(key);
            write!(s, "{:?}:{} => {}, ", marker_pin.locked_component_time().map(|t| t.value()), marker_pin.cached_timeline_time().value(), pin_index_map[marker.as_ref()]).unwrap();
        }
        let right = component.marker_right().ro(key);
        writeln!(s, "{:?}:{} => {}] }},", right.locked_component_time().map(|t| t.value()), right.cached_timeline_time().value(), pin_index_map[component.marker_right().as_ref()]).unwrap();
    }
    writeln!(s, "],").unwrap();
    writeln!(s, "links: [").unwrap();
    for link in value.link() {
        let link = link.ro(key);
        writeln!(s, "    {} = {:?} => {},", pin_index_map[link.from()], link.len().value(), pin_index_map[link.to()]).unwrap();
    }
    write!(s, "]").unwrap();
    s
}

pub async fn assert_eq_root_component_class<K, T>(a: &RootComponentClassHandleOwned<K, T>, b: &RootComponentClassHandleOwned<K, T>, key: &TCellOwner<K>)
where
    K: 'static,
    T: ParameterValueType,
{
    let result = async {
        let (a, b) = tokio::join!(a.read(), b.read());
        let (a, b) = tokio::join!(a.get(), b.get());
        (a.left().ro(key).locked_component_time() == b.left().ro(key).locked_component_time()).then_some(())?;
        (a.right().ro(key).locked_component_time() == b.right().ro(key).locked_component_time()).then_some(())?;
        (a.component().len() == b.component().len()).then_some(())?;
        let pin_index_map_a = a
            .component()
            .iter()
            .enumerate()
            .flat_map(|(i, component)| {
                let component = component.ro(key);
                iter::once(component.marker_left())
                    .chain(component.markers())
                    .chain(iter::once(component.marker_right()))
                    .enumerate()
                    .map(move |(j, p)| (StaticPointerOwned::reference(p).clone(), (i, j)))
            })
            .chain([(StaticPointerOwned::reference(a.left()).clone(), (usize::MAX, 0)), (StaticPointerOwned::reference(a.right()).clone(), (usize::MAX, 1))])
            .collect::<HashMap<_, _>>();
        let pin_index_map_b = b
            .component()
            .iter()
            .enumerate()
            .flat_map(|(i, component)| {
                let component = component.ro(key);
                iter::once(component.marker_left())
                    .chain(component.markers())
                    .chain(iter::once(component.marker_right()))
                    .enumerate()
                    .map(move |(j, p)| (StaticPointerOwned::reference(p).clone(), (i, j)))
            })
            .chain([(StaticPointerOwned::reference(b.left()).clone(), (usize::MAX, 0)), (StaticPointerOwned::reference(b.right()).clone(), (usize::MAX, 1))])
            .collect::<HashMap<_, _>>();
        a.component().iter().zip(b.component()).try_for_each(|(a, b)| {
            let a = a.ro(key);
            let b = b.ro(key);
            (a.markers().len() == b.markers().len()).then_some(())?;
            iter::once(a.marker_left())
                .chain(a.markers())
                .chain(iter::once(a.marker_right()))
                .zip(iter::once(b.marker_left()).chain(b.markers()).chain(iter::once(b.marker_right())))
                .try_for_each(|(a, b)| (a.ro(key).locked_component_time() == b.ro(key).locked_component_time()).then_some(()))
        });
        (a.link().len() == b.link().len()).then_some(())?;
        a.link().iter().zip(b.link()).try_for_each(|(a, b)| {
            let a = a.ro(key);
            let b = b.ro(key);
            (pin_index_map_a[a.from()] == pin_index_map_b[b.from()] && pin_index_map_a[a.to()] == pin_index_map_b[b.to()] && a.len() == b.len() || pin_index_map_a[a.from()] == pin_index_map_b[b.to()] && pin_index_map_a[a.to()] == pin_index_map_b[b.from()] && a.len() == -b.len()).then_some(())
        })?;
        Some(())
    }
    .await;
    if result.is_none() {
        let a = pretty_print_root_component_class(a, key).await;
        let b = pretty_print_root_component_class(b, key).await;
        panic!("assertion failed\n# left\n{a}\n# right\n{b}");
    }
}

#[macro_export]
macro_rules! marker {
    () => {
        ::mpdelta_core::ptr::StaticPointerOwned::new(::qcell::TCell::new(::mpdelta_core::component::marker_pin::MarkerPin::new_unlocked(::mpdelta_core::time::TimelineTime::ZERO)))
    };
    ($locked:expr$(,)?) => {
        ::mpdelta_core::ptr::StaticPointerOwned::new(::qcell::TCell::new(::mpdelta_core::component::marker_pin::MarkerPin::new(
            ::mpdelta_core::time::TimelineTime::ZERO,
            ::mpdelta_core::component::marker_pin::MarkerTime::new($locked).unwrap(),
        )))
    };
}

#[macro_export]
macro_rules! root_component_class {
    (
        $name:ident = <$k:ty, $t:ty> $key:expr;
        $(left: $left:ident,)?
        $(right: $right:ident,)?
        components: [$({ markers: [$($pin:expr$( => $pin_name:ident)?),*$(,)?] }$(;$component_name:ident)?),*$(,)?],
        links: [$($from:ident = $len:expr => $to:expr $(;$link_name:ident)?),*$(,)?]$(,)?
    ) => {
        let root_component_class = ::mpdelta_core::project::RootComponentClass::<$k, $t>::new_empty(
            ::uuid::Uuid::nil(),
            ::mpdelta_core::ptr::StaticPointer::<::tokio::sync::RwLock<::mpdelta_core::project::Project<$k, $t>>>::new(),
            ::uuid::Uuid::nil(),
        );
        let read = root_component_class.read().await;
        let mut item = read.get_mut().await;
        let left = ::mpdelta_core::ptr::StaticPointerOwned::reference(item.left());
        let right = ::mpdelta_core::ptr::StaticPointerOwned::reference(item.right());
        $(#[allow(unused_assignments, unused_variables)] let $left = left.clone();)?
        $(#[allow(unused_assignments, unused_variables)] let $right = right.clone();)?
        $($($(#[allow(unused_variables)] let $pin_name;)?)*$(#[allow(unused_variables)] let $component_name;)?)*
        $($(#[allow(unused_variables)] let $link_name;)?)*
        #[allow(unused_assignments)]
        let components = vec![$({
            let mut markers = vec![$({
                let pin = $pin;
                $($pin_name = ::mpdelta_core::ptr::StaticPointerOwned::reference(&pin).clone();)?
                pin
            }),*];
            let marker_left = markers.remove(0);
            let marker_right = markers.pop().unwrap();
            let builder = ::mpdelta_core::component::instance::ComponentInstance::builder(
                ::mpdelta_core::ptr::StaticPointer::<::tokio::sync::RwLock<$crate::NoopComponentClass>>::new().map(|c| c as _),
                marker_left,
                marker_right,
                markers,
                ::mpdelta_core::component::processor::ComponentProcessorWrapper::Native(::std::sync::Arc::new($crate::NoopProcessor) as ::std::sync::Arc<dyn ::mpdelta_core::component::processor::ComponentProcessorNativeDyn<$k, $t>>),
            );
            let component = ::mpdelta_core::ptr::StaticPointerOwned::new(::qcell::TCell::new(builder.build()));
            $($component_name = ::mpdelta_core::ptr::StaticPointerOwned::reference(&component).clone();)?
            component
        }),*];
        #[allow(unused_assignments)]
        let links = vec![$({
                let len = ::mpdelta_core::time::TimelineTime::new($len);
                let link = ::mpdelta_core::component::link::MarkerLink::new($from.clone(), $to.clone(), len);
                let link = ::mpdelta_core::ptr::StaticPointerOwned::new(::qcell::TCell::new(link));
                $($link_name = ::mpdelta_core::ptr::StaticPointerOwned::reference(&link).clone();)?
                link
            }),*];
        ::mpdelta_differential::collect_cached_time(&components, &links, left, right, &$key).unwrap();
        *item.component_mut() = components;
        *item.link_mut() = links;
        drop(item);
        drop(read);
        let $name = root_component_class;
    }
}
