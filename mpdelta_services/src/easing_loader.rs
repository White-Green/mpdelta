use async_trait::async_trait;
use mpdelta_core::component::parameter::value::{Easing, EasingIdentifier};
use mpdelta_core::core::EasingLoader;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

pub struct InMemoryEasingLoader {
    all: Vec<Arc<dyn Easing>>,
    map: HashMap<EasingIdentifier<'static>, Arc<dyn Easing>>,
}

impl Default for InMemoryEasingLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryEasingLoader {
    pub fn new() -> InMemoryEasingLoader {
        InMemoryEasingLoader { all: Vec::new(), map: HashMap::new() }
    }

    pub fn add(&mut self, easing: Arc<dyn Easing>) {
        let identifier = easing.identifier().into_static();
        self.all.push(Arc::clone(&easing));
        self.map.insert(identifier, easing);
    }
}

impl FromIterator<Arc<dyn Easing>> for InMemoryEasingLoader {
    fn from_iter<T: IntoIterator<Item = Arc<dyn Easing>>>(iter: T) -> Self {
        let mut loader = InMemoryEasingLoader::new();
        for easing in iter {
            loader.add(easing);
        }
        loader
    }
}

#[async_trait]
impl EasingLoader for InMemoryEasingLoader {
    async fn get_available_easing(&self) -> Cow<[Arc<dyn Easing>]> {
        Cow::Borrowed(&self.all)
    }

    async fn easing_by_identifier(&self, identifier: EasingIdentifier<'_>) -> Option<Arc<dyn Easing>> {
        self.map.get(&identifier).cloned()
    }
}
