use crate::signal_queue::split_ownership_array::SplitOwnershipArray;
use cpal::Sample;
use crossbeam_queue::ArrayQueue;
use std::sync::Arc;

mod split_ownership_array;

const QUEUE_SIZE: usize = 1 << 16;

pub struct SignalQueueSender<T> {
    array: SplitOwnershipArray<T, QUEUE_SIZE>,
    new_array: Arc<ArrayQueue<SplitOwnershipArray<T, QUEUE_SIZE>>>,
}

pub struct SignalQueueReceiver<T> {
    skip: usize,
    array: SplitOwnershipArray<T, QUEUE_SIZE>,
    new_array: Arc<ArrayQueue<SplitOwnershipArray<T, QUEUE_SIZE>>>,
}

impl<T: Sample> SignalQueueSender<T> {
    pub fn send_signal<I>(&mut self, value: I) -> Result<usize, ()>
    where
        I: IntoIterator<Item = T>,
        I::IntoIter: ExactSizeIterator,
    {
        let mut iter = value.into_iter();
        let iter_len = iter.len();
        let slice = self.array.get_slice_mut(iter_len + 1);
        let len = slice[0].len() + slice[1].len();
        if len <= iter_len {
            return Err(());
        }
        slice[0].iter_mut().zip(iter.by_ref()).for_each(|(s, v)| *s = v);
        slice[1].iter_mut().zip(iter).for_each(|(s, v)| *s = v);
        self.array.release_array(iter_len).unwrap();
        Ok(len.saturating_sub(iter_len).saturating_sub(1))
    }

    pub fn flush(&mut self) {
        let (new_left, new_right) = SplitOwnershipArray::new(|| T::EQUILIBRIUM);
        self.new_array.force_push(new_left);
        self.array = new_right;
    }
}

impl<T: Sample> SignalQueueReceiver<T> {
    pub fn receive_signal(&mut self, mut dst: &mut [T]) {
        while let Some(array) = self.new_array.pop() {
            self.skip = 1;
            self.array = array;
        }
        let mut data = self.array.get_slice_mut(dst.len() + self.skip + 1);
        let data_len = data[0].len() + data[1].len();
        if self.skip != 0 {
            let skip = self.skip.min(data_len - 1);
            self.skip -= skip;
            self.array.release_array(skip).unwrap();
            if self.skip != 0 {
                dst.fill(T::EQUILIBRIUM);
                self.skip += dst.len();
                return;
            }
            data = self.array.get_slice_mut(dst.len() + 1);
        }
        let data_len = data[0].len() + data[1].len();
        let request_len = dst.len();
        let len = data[0].len().min(dst.len());
        dst[..len].copy_from_slice(&data[0][..len]);
        dst = &mut dst[len..];
        let len = data[1].len().min(dst.len());
        dst[..len].copy_from_slice(&data[1][..len]);
        dst = &mut dst[len..];
        if (request_len - dst.len()) == data_len {
            self.skip += 1;
        }
        self.array.release_array((request_len - dst.len()).min(data_len - 1)).unwrap();
        dst.fill(T::EQUILIBRIUM);
        self.skip += dst.len();
    }
}

pub fn signal_queue<T: Sample>() -> (SignalQueueSender<T>, SignalQueueReceiver<T>) {
    let (left, right) = SplitOwnershipArray::new(|| T::EQUILIBRIUM);
    let new_array = Arc::new(ArrayQueue::new(1));
    let sender = SignalQueueSender { array: right, new_array: Arc::clone(&new_array) };
    let receiver = SignalQueueReceiver { skip: 1, array: left, new_array };
    (sender, receiver)
}

#[cfg(all(test, not(loom)))]
mod tests {
    use super::*;

    #[test]
    fn test_signal_queue() {
        let (mut sender, mut receiver) = signal_queue::<i32>();
        assert!(sender.send_signal([1, 2, 3]).unwrap() < QUEUE_SIZE - 3);
        assert!(sender.send_signal([4, 5, 6]).unwrap() < QUEUE_SIZE - 6);
        assert!(sender.send_signal([7, 8, 9]).unwrap() < QUEUE_SIZE - 9);
        let mut data = [0; 4];
        receiver.receive_signal(&mut data);
        assert_eq!(data, [1, 2, 3, 4]);
        receiver.receive_signal(&mut data);
        assert_eq!(data, [5, 6, 7, 8]);
        receiver.receive_signal(&mut data);
        assert_eq!(data, [9, 0, 0, 0]);
        assert!(sender.send_signal([10, 11, 12, 13, 14, 15, 16]).unwrap() < QUEUE_SIZE - 7);
        receiver.receive_signal(&mut data);
        assert_eq!(data, [13, 14, 15, 16]);
        assert!(sender.send_signal([17, 18, 19]).unwrap() < QUEUE_SIZE - 6);
        receiver.receive_signal(&mut data);
        assert_eq!(data, [17, 18, 19, 0]);
        assert!(sender.send_signal([20, 21, 22]).unwrap() < QUEUE_SIZE - 3);
        receiver.receive_signal(&mut data);
        assert_eq!(data, [21, 22, 0, 0]);
        assert!(sender.send_signal([23, 24, 25]).unwrap() < QUEUE_SIZE - 3);
        receiver.receive_signal(&mut data);
        assert_eq!(data, [25, 0, 0, 0]);
        receiver.receive_signal(&mut data);
        assert_eq!(data, [0, 0, 0, 0]);
        sender.flush();
        sender.flush();
        assert!(sender.send_signal([100, 101, 102]).unwrap() < QUEUE_SIZE - 3);
        receiver.receive_signal(&mut data);
        assert_eq!(data, [100, 101, 102, 0]);
        sender.flush();
        assert!(sender.send_signal([1000; QUEUE_SIZE - 2]).unwrap() < 1);
        sender.send_signal([1000; 1]).unwrap_err();
        receiver.receive_signal(&mut [0; QUEUE_SIZE / 4]);
        sender.send_signal([1000; QUEUE_SIZE / 8]).unwrap();
    }
}
