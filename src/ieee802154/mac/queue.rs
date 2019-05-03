use crate::delayqueue::DelayQueue;
use crate::delayqueue::Key as DelayQueueKey;
use crate::ieee802154::mac::frame::{AddressSpecification, Frame};
use futures::future::Future;
use futures::stream::Stream;
use futures::task::{Context, Poll, Waker};
use std::collections::{HashMap, HashSet, VecDeque};
use std::pin::Pin;
use std::time::Duration;

use crate::ieee802154::mac::device_queue;
use device_queue::DeviceQueue;
use device_queue::DeviceQueueAction;
pub use device_queue::DeviceQueueError as QueueError;
use device_queue::DeviceQueueItem as QueueItem;

pub struct Queue {
    queues: HashMap<AddressSpecification, DeviceQueue>,
    flushed: HashSet<AddressSpecification>,
    timers: DelayQueue<AddressSpecification>,
    running_timers: HashMap<AddressSpecification, DelayQueueKey>,
    outgoing: VecDeque<(AddressSpecification, Frame)>,
    waker: Option<Waker>,
}

impl Queue {
    pub fn new() -> Queue {
        Queue {
            queues: HashMap::new(),
            flushed: HashSet::new(),
            timers: DelayQueue::new(),
            running_timers: HashMap::new(),
            outgoing: VecDeque::new(),
            waker: None,
        }
    }
    fn wake(&mut self) {
        eprintln!("Waking");
        if let Some(waker) = self.waker.take() {
            waker.wake()
        }
    }

    fn handle_action(&mut self, destination: AddressSpecification, action: DeviceQueueAction) {
        match action {
            DeviceQueueAction::StartTimer(duration) => {
                if let Some(key) = self.running_timers.get(&destination) {
                    self.timers.reset(key, duration)
                } else {
                    self.running_timers
                        .insert(destination, self.timers.insert(destination, duration));
                }
            }
            DeviceQueueAction::StopTimer() => {
                if let Some(key) = self.running_timers.remove(&destination) {
                    self.timers.remove(&key);
                }
            }
            DeviceQueueAction::StopTimerStartSend(frame) => {
                if let Some(key) = self.running_timers.remove(&destination) {
                    self.timers.remove(&key);
                }
                self.outgoing.push_back((destination, frame));
                self.wake();
            }
        }
    }

    fn update_and_handle_action<F>(&mut self, destination: AddressSpecification, fun: F)
    where
        F: FnOnce(DeviceQueue) -> (DeviceQueue, Option<DeviceQueueAction>),
    {
        let (new_state, action) = fun(self.queues.remove(&destination).unwrap_or_default());
        if new_state.is_idle() {
            self.flushed.insert(destination.clone());
        } else {
            self.flushed.remove(&destination);
            self.queues.insert(destination.clone(), new_state);
        }
        if let Some(action) = action {
            self.handle_action(destination.clone(), action);
        }
    }

    pub fn enqueue(
        &mut self,
        frame: Frame,
        receiver_on_when_idle: bool,
    ) -> impl Future<Output = Result<(), QueueError>> {
        let destination = frame.destination.clone();
        let (item, retval) = QueueItem::new(
            frame,
            receiver_on_when_idle,
            Some(Duration::from_secs(10)),
            5,
        );
        self.update_and_handle_action(destination, move |x| DeviceQueue::enqueue(x, item));
        retval
    }

    pub fn on_ack(&mut self, sequence_nr: u8) {
        let mut actions = VecDeque::new();
        for (key, value) in self.queues.iter_mut() {
            let mut new_value = DeviceQueue::Idle;
            std::mem::swap(value, &mut new_value);
            let (mut new_value, action) = new_value.on_ack(sequence_nr);
            std::mem::swap(value, &mut new_value);
            if let Some(action) = action {
                actions.push_back((key.clone(), action));
            }
        }
        self.queues.retain(|_, value| !value.is_idle());
        for (destination, action) in actions.into_iter() {
            self.handle_action(destination, action);
        }
    }

    fn on_timer(&mut self, destination: AddressSpecification) {
        eprintln!("On timer: {:?}", destination);
        self.running_timers.remove(&destination);
        self.update_and_handle_action(destination, DeviceQueue::on_timer);
    }

    pub fn on_send_result(&mut self, destination: AddressSpecification, success: bool) {
        self.update_and_handle_action(destination, move |x| {
            DeviceQueue::on_send_result(x, success)
        });
    }

    /**
     * Returns true if there is still data pending for this address,
     * or false when no data is pending.
     */
    pub fn on_data_request(&mut self, destination: AddressSpecification) -> bool {
        self.update_and_handle_action(destination.clone(), DeviceQueue::on_data_request);
        self.queues
            .get(&destination)
            .map(|state| !state.is_idle())
            .unwrap_or(false)
    }
}

impl Stream for Queue {
    type Item = (AddressSpecification, Frame);

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let mut uself = self.get_mut();
        uself.waker = Some(cx.waker().clone());

        // Handle all timers
        while let Poll::Ready(Some(timer_event)) = Pin::new(&mut uself.timers).poll_next(cx) {
            match timer_event {
                Ok(expired) => uself.on_timer(expired.get_ref().clone()),
                Err(e) => eprintln!("DelayQueue error! {:?}", e),
            }
        }

        // Outgoing frames!
        if let Some(outgoing) = uself.outgoing.pop_front() {
            Poll::Ready(Some(outgoing))
        } else {
            Poll::Pending
        }
    }
}
