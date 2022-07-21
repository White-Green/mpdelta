use async_trait::async_trait;
use mpdelta_core::core::IdGenerator;
use std::sync::atomic;
use std::sync::atomic::AtomicU64;
use uuid::v1::Timestamp;
use uuid::Uuid;

#[derive(Debug)]
pub struct UniqueIdGenerator {
    context: uuid::v1::Context,
    counter: AtomicU64,
}

impl UniqueIdGenerator {
    pub fn new() -> UniqueIdGenerator {
        UniqueIdGenerator {
            context: uuid::v1::Context::new_random(),
            counter: AtomicU64::new(0),
        }
    }
}

impl Default for UniqueIdGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl IdGenerator for UniqueIdGenerator {
    async fn generate_new(&self) -> Uuid {
        let now = time::OffsetDateTime::now_utc();
        let secs = now.unix_timestamp();
        let nanos = now.unix_timestamp_nanos();
        let counter = self.counter.fetch_add(1, atomic::Ordering::AcqRel);
        Uuid::new_v1(Timestamp::from_unix(&self.context, secs as u64, (nanos % 1_000_000_000) as u32), <&[u8; 6]>::try_from(&counter.to_be_bytes()[2..]).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::iter;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_unique_id_generator() {
        let unique_id_generator = Arc::new(UniqueIdGenerator::new());
        let mut set = HashSet::new();
        let threads = iter::repeat(unique_id_generator).take(100_000).map(|gen| tokio::spawn(async move { gen.generate_new().await })).collect::<Vec<_>>();
        for t in threads {
            assert!(set.insert(t.await.unwrap()));
        }
    }
}
