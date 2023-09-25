use crate::component::marker_pin::MarkerTime;
use crate::core::IdGenerator;
use std::cmp::Ordering;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::ops::Range;
use uuid::Uuid;

pub struct TagImage;

pub struct TagAudio;

pub struct TagVideo;

pub struct TagFile;

pub struct TagString;

pub struct TagSelect;

pub struct TagBoolean;

pub struct TagRadio;

pub struct TagInteger;

pub struct TagRealNumber;

pub struct TagVec2;

pub struct TagVec3;

pub struct TagDictionary;

#[derive(Debug)]
pub struct Placeholder<Tag> {
    id: Uuid,
    phantom: PhantomData<Tag>,
}

impl<Tag> Placeholder<Tag> {
    pub fn new(id: &impl IdGenerator) -> Placeholder<Tag> {
        Placeholder { id: id.generate_new(), phantom: PhantomData }
    }
}

impl<Tag> Clone for Placeholder<Tag> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<Tag> Copy for Placeholder<Tag> {}

impl<Tag> PartialEq for Placeholder<Tag> {
    fn eq(&self, other: &Self) -> bool {
        self.id.eq(&other.id)
    }
}

impl<Tag> Eq for Placeholder<Tag> {}

impl<Tag> PartialOrd for Placeholder<Tag> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.id.partial_cmp(&other.id)
    }
}

impl<Tag> Ord for Placeholder<Tag> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.id.cmp(&other.id)
    }
}

impl<Tag> Hash for Placeholder<Tag> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
        self.phantom.hash(state);
    }
}

#[derive(Debug)]
pub struct TimedPlaceholder<Tag> {
    pub id: Placeholder<Tag>,
    pub time: Vec<Option<Range<MarkerTime>>>,
}

impl<Tag> Clone for TimedPlaceholder<Tag> {
    fn clone(&self) -> Self {
        TimedPlaceholder { id: self.id, time: self.time.clone() }
    }
}

impl<Tag> PartialEq for TimedPlaceholder<Tag> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.time == other.time
    }
}

impl<Tag> Eq for TimedPlaceholder<Tag> {}

impl<Tag> Hash for TimedPlaceholder<Tag> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
        self.time.hash(state);
    }
}
