use async_trait::async_trait;
use mpdelta_core::component::parameter::value::{DynEditableEasingValueIdentifier, DynEditableEasingValueManager, DynEditableSingleValueIdentifier, DynEditableSingleValueManager};
use mpdelta_core::core::ValueManagerLoader;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

pub struct InMemoryValueManagerLoader<T> {
    single_values: Vec<Arc<dyn DynEditableSingleValueManager<T>>>,
    single_values_map: HashMap<DynEditableSingleValueIdentifier<'static>, Arc<dyn DynEditableSingleValueManager<T>>>,
    easing_values: Vec<Arc<dyn DynEditableEasingValueManager<T>>>,
    easing_values_map: HashMap<DynEditableEasingValueIdentifier<'static>, Arc<dyn DynEditableEasingValueManager<T>>>,
}

impl<T> Default for InMemoryValueManagerLoader<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> InMemoryValueManagerLoader<T> {
    pub fn new() -> InMemoryValueManagerLoader<T> {
        InMemoryValueManagerLoader {
            single_values: Vec::new(),
            single_values_map: HashMap::new(),
            easing_values: Vec::new(),
            easing_values_map: HashMap::new(),
        }
    }

    pub fn add_single_value(&mut self, value: Arc<dyn DynEditableSingleValueManager<T>>) {
        let identifier = value.identifier().into_static();
        self.single_values.push(Arc::clone(&value));
        self.single_values_map.insert(identifier, value);
    }

    pub fn add_easing_value(&mut self, value: Arc<dyn DynEditableEasingValueManager<T>>) {
        let identifier = value.identifier().into_static();
        self.easing_values.push(Arc::clone(&value));
        self.easing_values_map.insert(identifier, value);
    }

    pub fn from_iter(single_values: impl IntoIterator<Item = Arc<dyn DynEditableSingleValueManager<T>>>, easing_values: impl IntoIterator<Item = Arc<dyn DynEditableEasingValueManager<T>>>) -> InMemoryValueManagerLoader<T> {
        let mut loader = InMemoryValueManagerLoader::new();
        for value in single_values {
            loader.add_single_value(value);
        }
        for value in easing_values {
            loader.add_easing_value(value);
        }
        loader
    }
}

#[async_trait]
impl<T> ValueManagerLoader<T> for InMemoryValueManagerLoader<T> {
    async fn get_available_single_value(&self) -> Cow<[Arc<dyn DynEditableSingleValueManager<T>>]> {
        Cow::Borrowed(&self.single_values)
    }

    async fn single_value_by_identifier(&self, identifier: DynEditableSingleValueIdentifier<'_>) -> Option<Arc<dyn DynEditableSingleValueManager<T>>> {
        self.single_values_map.get(&identifier).cloned()
    }

    async fn get_available_easing_value(&self) -> Cow<[Arc<dyn DynEditableEasingValueManager<T>>]> {
        Cow::Borrowed(&self.easing_values)
    }

    async fn easing_value_by_identifier(&self, identifier: DynEditableEasingValueIdentifier<'_>) -> Option<Arc<dyn DynEditableEasingValueManager<T>>> {
        self.easing_values_map.get(&identifier).cloned()
    }
}
