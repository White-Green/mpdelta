use crate::component::class::ComponentClass;
use crate::component::instance::ComponentInstance;
use crate::component::link::MarkerLink;
use crate::component::marker_pin::{MarkerPin, MarkerTime};
use crate::component::parameter::{Parameter, ParameterSelect, ParameterType, ParameterValueRaw, ParameterValueType};
use crate::ptr::StaticPointer;
use crate::time::TimelineTime;
use async_trait::async_trait;
use dyn_eq::DynEq;
use dyn_hash::DynHash;
use std::any::{Any, TypeId};
use std::future::Future;
use std::hash::Hash;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::RwLock;

pub trait ComponentsLinksPair<T>: Send + Sync
where
    T: ParameterValueType,
{
    fn components(&self) -> impl DoubleEndedIterator<Item = &'_ ComponentInstance<T>> + Send + Sync + '_
    where
        Self: Sized;
    fn components_dyn(&self) -> Box<dyn DoubleEndedIterator<Item = &'_ ComponentInstance<T>> + Send + Sync + '_>;
    fn links(&self) -> impl DoubleEndedIterator<Item = &'_ MarkerLink> + Send + Sync + '_
    where
        Self: Sized;
    fn links_dyn(&self) -> Box<dyn DoubleEndedIterator<Item = &'_ MarkerLink> + Send + Sync + '_>;
    fn left(&self) -> &MarkerPin;
    fn right(&self) -> &MarkerPin;
}

impl<'a, T, C> ComponentsLinksPair<T> for &'a C
where
    T: ParameterValueType,
    C: ComponentsLinksPair<T>,
{
    fn components(&self) -> impl DoubleEndedIterator<Item = &ComponentInstance<T>> + Send + Sync + '_
    where
        Self: Sized,
    {
        C::components(self)
    }

    fn components_dyn(&self) -> Box<dyn DoubleEndedIterator<Item = &ComponentInstance<T>> + Send + Sync + '_> {
        C::components_dyn(self)
    }

    fn links(&self) -> impl DoubleEndedIterator<Item = &MarkerLink> + Send + Sync + '_
    where
        Self: Sized,
    {
        C::links(self)
    }

    fn links_dyn(&self) -> Box<dyn DoubleEndedIterator<Item = &MarkerLink> + Send + Sync + '_> {
        C::links_dyn(self)
    }

    fn left(&self) -> &MarkerPin {
        C::left(self)
    }

    fn right(&self) -> &MarkerPin {
        C::right(self)
    }
}

impl<'a, T> ComponentsLinksPair<T> for &'a dyn ComponentsLinksPair<T>
where
    T: ParameterValueType,
{
    fn components(&self) -> impl DoubleEndedIterator<Item = &ComponentInstance<T>> + Send + Sync + '_
    where
        Self: Sized,
    {
        (*self).components_dyn()
    }

    fn components_dyn(&self) -> Box<dyn DoubleEndedIterator<Item = &ComponentInstance<T>> + Send + Sync + '_> {
        (*self).components_dyn()
    }

    fn links(&self) -> impl DoubleEndedIterator<Item = &MarkerLink> + Send + Sync + '_
    where
        Self: Sized,
    {
        (*self).links_dyn()
    }

    fn links_dyn(&self) -> Box<dyn DoubleEndedIterator<Item = &MarkerLink> + Send + Sync + '_> {
        (*self).links_dyn()
    }

    fn left(&self) -> &MarkerPin {
        (*self).left()
    }

    fn right(&self) -> &MarkerPin {
        (*self).right()
    }
}

pub enum ComponentProcessorWrapper<T: ParameterValueType> {
    Native(Arc<dyn ComponentProcessorNativeDyn<T>>),
    Component(Arc<dyn ComponentProcessorComponent<T>>),
    GatherNative(Arc<dyn ComponentProcessorGatherNative<T>>),
    GatherComponent(Arc<dyn ComponentProcessorGatherComponent<T>>),
}

impl<T> Clone for ComponentProcessorWrapper<T>
where
    T: ParameterValueType,
{
    fn clone(&self) -> Self {
        match self {
            ComponentProcessorWrapper::Native(processor) => ComponentProcessorWrapper::Native(Arc::clone(processor)),
            ComponentProcessorWrapper::Component(processor) => ComponentProcessorWrapper::Component(Arc::clone(processor)),
            ComponentProcessorWrapper::GatherNative(processor) => ComponentProcessorWrapper::GatherNative(Arc::clone(processor)),
            ComponentProcessorWrapper::GatherComponent(processor) => ComponentProcessorWrapper::GatherComponent(Arc::clone(processor)),
        }
    }
}

impl<T: ParameterValueType> ComponentProcessor<T> for ComponentProcessorWrapper<T> {
    fn fixed_parameter_types<'life0, 'async_trait>(&'life0 self) -> Pin<Box<dyn Future<Output = &'life0 [(String, ParameterType)]> + Send + 'async_trait>>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
    {
        match self {
            ComponentProcessorWrapper::Native(processor) => processor.fixed_parameter_types(),
            ComponentProcessorWrapper::Component(processor) => processor.fixed_parameter_types(),
            ComponentProcessorWrapper::GatherNative(processor) => processor.fixed_parameter_types(),
            ComponentProcessorWrapper::GatherComponent(processor) => processor.fixed_parameter_types(),
        }
    }

    fn update_variable_parameter<'life0, 'life1, 'life2, 'async_trait>(&'life0 self, fixed_params: &'life1 [ParameterValueRaw<T::Image, T::Audio>], variable_parameters: &'life2 mut Vec<(String, ParameterType)>) -> Pin<Box<dyn Future<Output = ()> + Send + 'async_trait>>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
        'life1: 'async_trait,
        'life2: 'async_trait,
    {
        match self {
            ComponentProcessorWrapper::Native(processor) => processor.update_variable_parameter(fixed_params, variable_parameters),
            ComponentProcessorWrapper::Component(processor) => processor.update_variable_parameter(fixed_params, variable_parameters),
            ComponentProcessorWrapper::GatherNative(processor) => processor.update_variable_parameter(fixed_params, variable_parameters),
            ComponentProcessorWrapper::GatherComponent(processor) => processor.update_variable_parameter(fixed_params, variable_parameters),
        }
    }
}

impl<T> From<Arc<dyn ComponentProcessorNativeDyn<T>>> for ComponentProcessorWrapper<T>
where
    T: ParameterValueType,
{
    fn from(value: Arc<dyn ComponentProcessorNativeDyn<T>>) -> ComponentProcessorWrapper<T> {
        ComponentProcessorWrapper::Native(value)
    }
}

impl<T> From<Arc<dyn ComponentProcessorComponent<T>>> for ComponentProcessorWrapper<T>
where
    T: ParameterValueType,
{
    fn from(value: Arc<dyn ComponentProcessorComponent<T>>) -> ComponentProcessorWrapper<T> {
        ComponentProcessorWrapper::Component(value)
    }
}

impl<T> From<Arc<dyn ComponentProcessorGatherNative<T>>> for ComponentProcessorWrapper<T>
where
    T: ParameterValueType,
{
    fn from(value: Arc<dyn ComponentProcessorGatherNative<T>>) -> ComponentProcessorWrapper<T> {
        ComponentProcessorWrapper::GatherNative(value)
    }
}

impl<T> From<Arc<dyn ComponentProcessorGatherComponent<T>>> for ComponentProcessorWrapper<T>
where
    T: ParameterValueType,
{
    fn from(value: Arc<dyn ComponentProcessorGatherComponent<T>>) -> ComponentProcessorWrapper<T> {
        ComponentProcessorWrapper::GatherComponent(value)
    }
}

pub trait CacheKey: Send + Sync + DynEq + DynHash + 'static {}
dyn_eq::eq_trait_object!(CacheKey);
dyn_hash::hash_trait_object!(CacheKey);

impl<T> CacheKey for T where T: Send + Sync + DynEq + DynHash + 'static {}

#[async_trait]
pub trait ComponentProcessor<T: ParameterValueType>: Send + Sync {
    async fn fixed_parameter_types(&self) -> &[(String, ParameterType)];
    async fn update_variable_parameter(&self, fixed_params: &[ParameterValueRaw<T::Image, T::Audio>], variable_parameters: &mut Vec<(String, ParameterType)>);
}

pub struct NativeProcessorInput<'a, T: ParameterValueType> {
    pub fixed_parameters: &'a [ParameterValueRaw<T::Image, T::Audio>],
    pub variable_parameters: &'a [ParameterValueRaw<T::Image, T::Audio>],
    pub variable_parameter_type: &'a [(String, ParameterType)],
}

impl<'a, T> Clone for NativeProcessorInput<'a, T>
where
    T: ParameterValueType,
{
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, T> Copy for NativeProcessorInput<'a, T> where T: ParameterValueType {}

pub struct NativeProcessorRequest;

impl ParameterValueType for NativeProcessorRequest {
    type Image = (u32, u32);
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

#[async_trait]
pub trait ComponentProcessorNative<T: ParameterValueType>: ComponentProcessor<T> {
    type WholeComponentCacheKey: Send + Sync + Eq + Hash + 'static;
    type WholeComponentCacheValue: Send + Sync + 'static;
    type FramedCacheKey: Send + Sync + Eq + Hash + 'static;
    type FramedCacheValue: Send + Sync + 'static;
    fn whole_component_cache_key(&self, fixed_parameters: &[ParameterValueRaw<T::Image, T::Audio>]) -> Option<Self::WholeComponentCacheKey>;
    fn framed_cache_key(&self, parameters: NativeProcessorInput<'_, T>, time: TimelineTime, output_type: Parameter<ParameterSelect>) -> Option<Self::FramedCacheKey>;
    async fn natural_length(&self, fixed_params: &[ParameterValueRaw<T::Image, T::Audio>], cache: &mut Option<Arc<Self::WholeComponentCacheValue>>) -> Option<MarkerTime>;
    async fn supports_output_type(&self, fixed_params: &[ParameterValueRaw<T::Image, T::Audio>], out: Parameter<ParameterSelect>, cache: &mut Option<Arc<Self::WholeComponentCacheValue>>) -> bool;
    async fn process(
        &self,
        parameters: NativeProcessorInput<'_, T>,
        time: TimelineTime,
        output_type: Parameter<NativeProcessorRequest>,
        whole_component_cache: &mut Option<Arc<Self::WholeComponentCacheValue>>,
        framed_cache: &mut Option<Arc<Self::FramedCacheValue>>,
    ) -> ParameterValueRaw<T::Image, T::Audio>;
}

#[async_trait]
pub trait ComponentProcessorNativeDyn<T: ParameterValueType>: ComponentProcessor<T> {
    fn whole_component_cache_key(&self, fixed_parameters: &[ParameterValueRaw<T::Image, T::Audio>]) -> Option<Box<dyn CacheKey>>;
    fn framed_cache_key(&self, parameters: NativeProcessorInput<'_, T>, time: TimelineTime, output_type: Parameter<ParameterSelect>) -> Option<Box<dyn CacheKey>>;
    async fn natural_length(&self, fixed_params: &[ParameterValueRaw<T::Image, T::Audio>], cache: &mut Option<Arc<dyn Any + Send + Sync>>) -> Option<MarkerTime>;
    async fn supports_output_type(&self, fixed_params: &[ParameterValueRaw<T::Image, T::Audio>], out: Parameter<ParameterSelect>, cache: &mut Option<Arc<dyn Any + Send + Sync>>) -> bool;
    async fn process(&self, parameters: NativeProcessorInput<'_, T>, time: TimelineTime, output_type: Parameter<NativeProcessorRequest>, whole_component_cache: &mut Option<Arc<dyn Any + Send + Sync>>, framed_cache: &mut Option<Arc<dyn Any + Send + Sync>>) -> ParameterValueRaw<T::Image, T::Audio>;
}

#[async_trait]
impl<T, P> ComponentProcessorNativeDyn<T> for P
where
    T: ParameterValueType,
    P: ComponentProcessorNative<T> + 'static,
{
    fn whole_component_cache_key(&self, fixed_parameters: &[ParameterValueRaw<T::Image, T::Audio>]) -> Option<Box<dyn CacheKey>> {
        let key = P::whole_component_cache_key(self, fixed_parameters)?;
        Some(Box::new((TypeId::of::<P>(), true, TypeId::of::<P::WholeComponentCacheValue>(), key)))
    }

    fn framed_cache_key(&self, parameters: NativeProcessorInput<'_, T>, time: TimelineTime, output_type: Parameter<ParameterSelect>) -> Option<Box<dyn CacheKey>> {
        let key = P::framed_cache_key(self, parameters, time, output_type)?;
        Some(Box::new((TypeId::of::<P>(), false, TypeId::of::<P::FramedCacheValue>(), key)))
    }

    async fn natural_length(&self, fixed_params: &[ParameterValueRaw<T::Image, T::Audio>], cache: &mut Option<Arc<dyn Any + Send + Sync>>) -> Option<MarkerTime> {
        let mut c = cache.take().and_then(|cache| Arc::downcast(cache).ok());
        let result = P::natural_length(self, fixed_params, &mut c).await;
        if let Some(c) = c {
            *cache = Some(c);
        }
        result
    }

    async fn supports_output_type(&self, fixed_params: &[ParameterValueRaw<T::Image, T::Audio>], out: Parameter<ParameterSelect>, cache: &mut Option<Arc<dyn Any + Send + Sync>>) -> bool {
        let mut c = cache.take().and_then(|cache| Arc::downcast(cache).ok());
        let result = P::supports_output_type(self, fixed_params, out, &mut c).await;
        if let Some(c) = c {
            *cache = Some(c);
        }
        result
    }

    async fn process(&self, parameters: NativeProcessorInput<'_, T>, time: TimelineTime, output_type: Parameter<NativeProcessorRequest>, whole_component_cache: &mut Option<Arc<dyn Any + Send + Sync>>, framed_cache: &mut Option<Arc<dyn Any + Send + Sync>>) -> ParameterValueRaw<T::Image, T::Audio> {
        let mut wc = whole_component_cache.take().and_then(|cache| Arc::downcast(cache).ok());
        let mut fc = framed_cache.take().and_then(|cache| Arc::downcast(cache).ok());
        let result = P::process(self, parameters, time, output_type, &mut wc, &mut fc).await;
        if let Some(c) = wc {
            *whole_component_cache = Some(c);
        }
        if let Some(c) = fc {
            *framed_cache = Some(c);
        }
        result
    }
}

#[async_trait]
pub trait ComponentProcessorComponent<T: ParameterValueType>: ComponentProcessor<T> {
    async fn natural_length(&self, fixed_params: &[ParameterValueRaw<T::Image, T::Audio>]) -> MarkerTime;
    async fn process(
        &self,
        fixed_parameters: &[ParameterValueRaw<T::Image, T::Audio>],
        fixed_parameters_component: &[StaticPointer<RwLock<dyn ComponentClass<T>>>],
        variable_parameters: &[StaticPointer<RwLock<dyn ComponentClass<T>>>],
        variable_parameter_type: &[(String, ParameterType)],
    ) -> Arc<dyn ComponentsLinksPair<T>>;
}

// TODO:
#[async_trait]
pub trait ComponentProcessorGatherNative<T: ParameterValueType>: ComponentProcessor<T> {
    fn supports_output_type(&self, out: Parameter<ParameterSelect>) -> bool;
    async fn process(&self) -> ();
}

#[async_trait]
pub trait ComponentProcessorGatherComponent<T: ParameterValueType>: ComponentProcessor<T> {
    async fn process(
        &self,
        fixed_parameters: &[ParameterValueRaw<T::Image, T::Audio>],
        fixed_parameters_component: &[StaticPointer<RwLock<dyn ComponentClass<T>>>],
        variable_parameters: &[StaticPointer<RwLock<dyn ComponentClass<T>>>],
        variable_parameter_type: &[(String, ParameterType)],
    ) -> Arc<dyn ComponentsLinksPair<T>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

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

    #[test]
    fn test_components_links_pair() {
        struct TestComponentsLinksPair {
            left: MarkerPin,
            right: MarkerPin,
        }
        impl ComponentsLinksPair<T> for TestComponentsLinksPair {
            fn components(&self) -> impl DoubleEndedIterator<Item = &'_ ComponentInstance<T>> + Send + Sync + '_
            where
                Self: Sized,
            {
                [].into_iter()
            }

            fn components_dyn(&self) -> Box<dyn DoubleEndedIterator<Item = &'_ ComponentInstance<T>> + Send + Sync + '_> {
                Box::new(self.components())
            }

            fn links(&self) -> impl DoubleEndedIterator<Item = &'_ MarkerLink> + Send + Sync + '_
            where
                Self: Sized,
            {
                [].into_iter()
            }

            fn links_dyn(&self) -> Box<dyn DoubleEndedIterator<Item = &'_ MarkerLink> + Send + Sync + '_> {
                Box::new(self.links())
            }

            fn left(&self) -> &MarkerPin {
                &self.left
            }

            fn right(&self) -> &MarkerPin {
                &self.right
            }
        }

        fn test<C: ComponentsLinksPair<T>>(c: C) {
            assert_eq!(c.components().count(), 0);
            assert_eq!(c.components_dyn().count(), 0);
            assert_eq!(c.links().count(), 0);
            assert_eq!(c.links_dyn().count(), 0);
            c.left();
            c.right();
        }
        let pair = TestComponentsLinksPair {
            left: MarkerPin::new_unlocked(Uuid::nil()),
            right: MarkerPin::new_unlocked(Uuid::nil()),
        };
        test::<&TestComponentsLinksPair>(&pair);
        test::<&dyn ComponentsLinksPair<T>>(&pair);
        test::<TestComponentsLinksPair>(pair);
    }
}
