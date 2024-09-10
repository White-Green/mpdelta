use crate::common::mixed_fraction::MixedFraction;
use serde::{Deserialize, Serialize};
use std::borrow::Borrow;
use std::hash::{Hash, Hasher};
use uuid::Uuid;

/// コンポーネントの始点からの時間
/// \[0.0, ∞)
#[derive(Debug, Clone, Copy, Ord, PartialOrd, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[cfg_attr(any(feature = "proptest", test), derive(proptest_derive::Arbitrary))]
pub struct MarkerTime(MixedFraction);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkerPin {
    id: MarkerPinId,
    locked_component_time: Option<MarkerTime>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MarkerPinId {
    id: Uuid,
}

impl MarkerTime {
    pub const ZERO: MarkerTime = MarkerTime(MixedFraction::ZERO);

    pub fn new(value: MixedFraction) -> Option<MarkerTime> {
        (MixedFraction::ZERO <= value).then_some(MarkerTime(value))
    }

    pub fn value(&self) -> MixedFraction {
        self.0
    }
}

impl Hash for MarkerPin {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl Borrow<MarkerPinId> for MarkerPin {
    fn borrow(&self) -> &MarkerPinId {
        self.id()
    }
}

impl<'a> Borrow<MarkerPinId> for &'a MarkerPin {
    fn borrow(&self) -> &MarkerPinId {
        self.id()
    }
}

impl MarkerPin {
    pub fn new(id: Uuid, component_time: MarkerTime) -> MarkerPin {
        MarkerPin {
            id: MarkerPinId::new(id),
            locked_component_time: Some(component_time),
        }
    }

    pub fn new_unlocked(id: Uuid) -> MarkerPin {
        MarkerPin { id: MarkerPinId::new(id), locked_component_time: None }
    }

    pub fn id(&self) -> &MarkerPinId {
        &self.id
    }

    pub fn locked_component_time(&self) -> Option<MarkerTime> {
        self.locked_component_time
    }

    pub fn set_locked_component_time(&mut self, time: Option<MarkerTime>) {
        self.locked_component_time = time;
    }
}

impl MarkerPinId {
    fn new(id: Uuid) -> MarkerPinId {
        MarkerPinId { id }
    }
}
