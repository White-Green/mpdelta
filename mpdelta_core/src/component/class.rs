use crate::component::instance::ComponentInstance;
use crate::component::parameter::ParameterValueType;
use crate::component::processor::ComponentProcessorWrapper;
use crate::core::IdGenerator;
use crate::ptr::StaticPointer;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ComponentClassIdentifier<'a> {
    #[serde(rename = "ns")]
    pub namespace: Cow<'a, str>,
    #[serde(rename = "n")]
    pub name: Cow<'a, str>,
    #[serde(rename = "i")]
    pub inner_identifier: [Uuid; 2],
}

#[cfg(any(feature = "proptest", test))]
const _: () = {
    use proptest::arbitrary::StrategyFor;
    use proptest::prelude::*;
    impl<'a> Arbitrary for ComponentClassIdentifier<'a> {
        type Parameters = (<String as Arbitrary>::Parameters, <String as Arbitrary>::Parameters, <[u128; 2] as Arbitrary>::Parameters);
        type Strategy = proptest::strategy::Map<(StrategyFor<String>, StrategyFor<String>, <[u128; 2] as Arbitrary>::Strategy), fn((String, String, [u128; 2])) -> ComponentClassIdentifier<'a>>;

        fn arbitrary_with(args: Self::Parameters) -> Self::Strategy {
            let (namespace, name, inner_identifier) = args;
            (String::arbitrary_with(namespace), String::arbitrary_with(name), <[u128; 2]>::arbitrary_with(inner_identifier)).prop_map(|(namespace, name, inner_identifier)| ComponentClassIdentifier {
                namespace: namespace.into(),
                name: name.into(),
                inner_identifier: inner_identifier.map(Uuid::from_u128),
            })
        }
    }
};

impl ComponentClassIdentifier<'_> {
    pub fn into_static(self) -> ComponentClassIdentifier<'static> {
        ComponentClassIdentifier {
            namespace: Cow::Owned(self.namespace.into_owned()),
            name: Cow::Owned(self.name.into_owned()),
            inner_identifier: self.inner_identifier,
        }
    }

    pub fn as_ref(&self) -> ComponentClassIdentifier {
        ComponentClassIdentifier {
            namespace: Cow::Borrowed(&self.namespace),
            name: Cow::Borrowed(&self.name),
            inner_identifier: self.inner_identifier,
        }
    }
}

#[async_trait]
pub trait ComponentClass<T: ParameterValueType>: Send + Sync {
    fn human_readable_identifier(&self) -> &str;
    fn identifier(&self) -> ComponentClassIdentifier;
    fn processor(&self) -> ComponentProcessorWrapper<T>;
    async fn instantiate(&self, this: &StaticPointer<RwLock<dyn ComponentClass<T>>>, id: &dyn IdGenerator) -> ComponentInstance<T>;
}
