use mpdelta_core::component::instance::ComponentInstanceHandle;
use mpdelta_core::component::link::{MarkerLink, MarkerLinkHandle};
use mpdelta_core::component::marker_pin::MarkerPinHandle;
use mpdelta_core::component::parameter::ParameterValueType;
use mpdelta_core::time::TimelineTime;
use qcell::TCellOwner;
use std::collections::HashSet;
use std::fmt::{Debug, Formatter};
use thiserror::Error;

#[derive(Clone, Error)]
pub enum CollectCachedTimeError<K> {
    #[error("invalid marker link {0:?}")]
    InvalidMarkerLink(MarkerLinkHandle<K>),
    #[error("invalid marker {0:?}")]
    InvalidMarker(MarkerPinHandle<K>),
    #[error("invalid link graph")]
    InvalidLinkGraph,
}

impl<K> Debug for CollectCachedTimeError<K> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            CollectCachedTimeError::InvalidMarkerLink(value) => f.debug_tuple("InvalidMarkerLink").field(value).finish(),
            CollectCachedTimeError::InvalidMarker(value) => f.debug_tuple("InvalidMarker").field(value).finish(),
            CollectCachedTimeError::InvalidLinkGraph => f.write_str("InvalidLinkGraph"),
        }
    }
}

pub fn collect_cached_time<K, T>(_components: &[impl AsRef<ComponentInstanceHandle<K, T>>], links: &[impl AsRef<MarkerLinkHandle<K>>], begin: &MarkerPinHandle<K>, end: &MarkerPinHandle<K>, key: &TCellOwner<K>) -> Result<(), CollectCachedTimeError<K>>
where
    T: ParameterValueType,
{
    let links = links.iter().map(|link| link.as_ref().upgrade().ok_or_else(|| CollectCachedTimeError::InvalidMarkerLink(link.as_ref().clone()))).collect::<Result<Vec<_>, _>>()?;
    let mut links = links.iter().map(|link| link.ro(key)).collect::<HashSet<&MarkerLink<K>>>();
    let mut locked = HashSet::from([begin, end]);

    loop {
        let process = 'block: {
            for &link in &links {
                match (locked.contains(&link.from), locked.contains(&link.to)) {
                    (false, false) => {}
                    (true, false) => break 'block Some((link, &link.from, &link.to, link.len)),
                    (false, true) => break 'block Some((link, &link.to, &link.from, -link.len)),
                    (true, true) => return Err(CollectCachedTimeError::InvalidLinkGraph),
                }
            }
            None
        };
        let Some((link, from, to, len)) = process else {
            break;
        };
        links.remove(&link);
        locked.insert(to);
        let from = from.upgrade().ok_or_else(|| CollectCachedTimeError::InvalidMarker(from.clone()))?;
        let to = to.upgrade().ok_or_else(|| CollectCachedTimeError::InvalidMarker(to.clone()))?;
        let from = from.ro(key);
        let to = to.ro(key);
        to.cache_timeline_time(TimelineTime::new(from.cached_timeline_time().value() + len.value()));
    }
    if links.is_empty() {
        Ok(())
    } else {
        Err(CollectCachedTimeError::InvalidLinkGraph)
    }
}
