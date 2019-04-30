use crate::delayqueue::DelayQueue;
use crate::delayqueue::Key as DelayQueueKey;
use crate::ieee802154::mac::frame::{AddressSpecification, Frame};
use crate::map_update::{CheckDefault, MapUpdate};
use futures::channel::oneshot;
use futures::future::{Future, TryFutureExt};
use futures::stream::Stream;
use futures::task::{Context, Poll, Waker};
use std::collections::{HashMap, VecDeque};
use std::pin::Pin;
use std::time::Duration;

#[derive(Debug)]
pub enum QueueError {
    DataRequestTimeout,
    RetryLimitReached,
    ChannelDropped,
}

#[derive(Debug)]
struct QueueItem {
    frame: Frame,
    wait_for_datarequest: bool,
    max_wait_for_datarequest: Option<Duration>, // Ignored if wait_for_datarequest
    retries: usize,                             // retries 0 means try only once.
    callback: oneshot::Sender<Result<(), QueueError>>,
}

#[derive(Debug)]
enum AddressQueue {
    Idle,
    // We're waiting for a data request for this node.
    WaitingForDataRequest {
        front: QueueItem,
        queue: VecDeque<QueueItem>,
    },
    // We've started sending a frame to the radio, but haven't heard back yet.
    SendStarted {
        front: QueueItem,
        queue: VecDeque<QueueItem>,
        wait_for_ack: Option<u8>, // After sending, should we still wait for an acknowledgement ?
    },
    // We've sent out the frame, but are waiting for an Ack.
    WaitingForAck {
        front: QueueItem,
        queue: VecDeque<QueueItem>,
        wait_for_ack: u8,
    },
}

impl Default for AddressQueue {
    fn default() -> Self {
        AddressQueue::Idle
    }
}

impl CheckDefault for AddressQueue {
    fn is_default(&self) -> bool {
        match self {
            AddressQueue::Idle => true,
            _ => false,
        }
    }
}

enum AddressQueueAction {
    StartTimer(Duration),
    StopTimerStartSend(Frame),
    StopTimer(),
}

impl AddressQueue {
    fn start_send(
        front: QueueItem,
        queue: VecDeque<QueueItem>,
    ) -> (Self, Option<AddressQueueAction>) {
        match front.wait_for_datarequest {
            false => {
                let frame = front.frame.clone();
                let wait_for_ack = front.frame.expect_ack();
                (
                    AddressQueue::SendStarted {
                        front,
                        queue,
                        wait_for_ack,
                    },
                    Some(AddressQueueAction::StopTimerStartSend(frame)),
                )
            }
            true => {
                let action = front
                    .max_wait_for_datarequest
                    .map(AddressQueueAction::StartTimer);
                (AddressQueue::WaitingForDataRequest { front, queue }, action)
            }
        }
    }

    fn restart_from_queue(queue: VecDeque<QueueItem>) -> (Self, Option<AddressQueueAction>) {
        let mut queue = queue;
        if let Some(front) = queue.pop_front() {
            AddressQueue::start_send(front, queue)
        } else {
            (AddressQueue::Idle, Some(AddressQueueAction::StopTimer()))
        }
    }

    fn retry_send(
        front: QueueItem,
        queue: VecDeque<QueueItem>,
    ) -> (Self, Option<AddressQueueAction>) {
        let mut front = front;
        if front.retries < 1 {
            if let Err(e) = front.callback.send(Err(QueueError::RetryLimitReached)) {
                eprintln!("Unable to report failed packet: {:?}", e);
            }
            AddressQueue::restart_from_queue(queue)
        } else {
            front.retries = front.retries - 1;
            AddressQueue::start_send(front, queue)
        }
    }

    fn enqueue(self, item: QueueItem) -> (Self, Option<AddressQueueAction>) {
        match self {
            AddressQueue::Idle => AddressQueue::start_send(item, VecDeque::new()),
            AddressQueue::WaitingForDataRequest { front, mut queue } => {
                queue.push_back(item);
                (AddressQueue::WaitingForDataRequest { front, queue }, None)
            }
            AddressQueue::SendStarted {
                front,
                mut queue,
                wait_for_ack,
            } => {
                queue.push_back(item);
                (
                    AddressQueue::SendStarted {
                        front,
                        queue,
                        wait_for_ack,
                    },
                    None,
                )
            }
            AddressQueue::WaitingForAck {
                front,
                mut queue,
                wait_for_ack,
            } => {
                queue.push_back(item);
                (
                    AddressQueue::WaitingForAck {
                        front,
                        queue,
                        wait_for_ack,
                    },
                    None,
                )
            }
        }
    }

    fn on_timer(self) -> (Self, Option<AddressQueueAction>) {
        match self {
            AddressQueue::WaitingForDataRequest { front, queue } => {
                if let Err(e) = front.callback.send(Err(QueueError::DataRequestTimeout)) {
                    eprintln!("Unable to report packet failed: {:?}", e);
                };
                AddressQueue::restart_from_queue(queue)
            }
            AddressQueue::WaitingForAck {
                front,
                queue,
                wait_for_ack: _,
            } => AddressQueue::retry_send(front, queue),
            x => {
                eprintln!("Timer not expected from this state: {:?}", x);
                (x, None)
            }
        }
    }

    fn on_data_request(self) -> (Self, Option<AddressQueueAction>) {
        match self {
            AddressQueue::WaitingForDataRequest { front, queue } => {
                let frame = front.frame.clone();
                let wait_for_ack = frame.expect_ack();
                (
                    AddressQueue::SendStarted {
                        front,
                        queue,
                        wait_for_ack,
                    },
                    Some(AddressQueueAction::StopTimerStartSend(frame)),
                )
            }
            // TODO: Maybe do something in SendStarted or WaitingForAck ?
            x => (x, None),
        }
    }

    fn on_send_result(self, success: bool) -> (Self, Option<AddressQueueAction>) {
        match self {
            AddressQueue::SendStarted {
                front,
                queue,
                wait_for_ack,
            } => {
                if success == false {
                    AddressQueue::retry_send(front, queue)
                } else if let Some(wait_for_ack) = wait_for_ack {
                    (
                        AddressQueue::WaitingForAck {
                            front,
                            queue,
                            wait_for_ack,
                        },
                        Some(AddressQueueAction::StartTimer(
                            std::time::Duration::from_millis(250),
                        )),
                    )
                } else {
                    if let Err(e) = front.callback.send(Ok(())) {
                        eprintln!("Unable to report packet sent: {:?}", e);
                    }
                    AddressQueue::restart_from_queue(queue)
                }
            }
            x => {
                eprintln!("Send result not expected from this state: {:?}", x);
                (x, None)
            }
        }
    }

    fn on_ack(self, ack: u8) -> (Self, Option<AddressQueueAction>) {
        match self {
            AddressQueue::SendStarted {
                front,
                queue,
                mut wait_for_ack,
            } => {
                if wait_for_ack == Some(ack) {
                    wait_for_ack = None;
                }
                (
                    AddressQueue::SendStarted {
                        front,
                        queue,
                        wait_for_ack,
                    },
                    None,
                )
            }
            AddressQueue::WaitingForAck {
                front,
                queue,
                wait_for_ack,
            } => {
                if wait_for_ack == ack {
                    if let Err(e) = front.callback.send(Ok(())) {
                        eprintln!("Unable to report packet sent: {:?}", e);
                    }
                    AddressQueue::restart_from_queue(queue)
                } else {
                    (
                        AddressQueue::WaitingForAck {
                            front,
                            queue,
                            wait_for_ack,
                        },
                        None,
                    )
                }
            }
            x => (x, None),
        }
    }
}

pub struct Queue {
    queues: HashMap<AddressSpecification, AddressQueue>,
    timers: DelayQueue<AddressSpecification>,
    running_timers: HashMap<AddressSpecification, DelayQueueKey>,
    outgoing: VecDeque<(AddressSpecification, Frame)>,
    waker: Option<Waker>,
}

impl Queue {
    pub fn new() -> Queue {
        Queue {
            queues: HashMap::new(),
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

    fn handle_action(&mut self, destination: AddressSpecification, action: AddressQueueAction) {
        match action {
            AddressQueueAction::StartTimer(duration) => {
                if let Some(key) = self.running_timers.get(&destination) {
                    self.timers.reset(key, duration)
                } else {
                    self.running_timers
                        .insert(destination, self.timers.insert(destination, duration));
                }
            }
            AddressQueueAction::StopTimer() => {
                if let Some(key) = self.running_timers.remove(&destination) {
                    self.timers.remove(&key);
                }
            }
            AddressQueueAction::StopTimerStartSend(frame) => {
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
        F: FnOnce(AddressQueue) -> (AddressQueue, Option<AddressQueueAction>),
    {
        if let Some(action) = self.queues.update_return(destination.clone(), fun) {
            self.handle_action(destination, action);
        }
    }

    pub fn enqueue(
        &mut self,
        frame: Frame,
        receiver_on_when_idle: bool,
    ) -> impl Future<Output = Result<(), QueueError>> {
        let (sender, receiver) = oneshot::channel();
        let destination = frame.destination.clone();
        let item = QueueItem {
            frame,
            wait_for_datarequest: !receiver_on_when_idle,
            max_wait_for_datarequest: Some(std::time::Duration::from_secs(10)),
            retries: 5,
            callback: sender,
        };
        self.update_and_handle_action(destination, move |x| AddressQueue::enqueue(x, item));
        receiver.unwrap_or_else(|_| Err(QueueError::ChannelDropped))
    }

    pub fn on_ack(&mut self, sequence_nr: u8) {
        let mut actions = VecDeque::new();
        for (key, value) in self.queues.iter_mut() {
            let mut new_value = AddressQueue::Idle;
            std::mem::swap(value, &mut new_value);
            let (mut new_value, action) = new_value.on_ack(sequence_nr);
            std::mem::swap(value, &mut new_value);
            if let Some(action) = action {
                actions.push_back((key.clone(), action));
            }
        }
        self.queues.retain(|_, value| !value.is_default());
        for (destination, action) in actions.into_iter() {
            self.handle_action(destination, action);
        }
    }

    fn on_timer(&mut self, destination: AddressSpecification) {
        eprintln!("On timer: {:?}", destination);
        self.running_timers.remove(&destination);
        self.update_and_handle_action(destination, AddressQueue::on_timer);
    }

    pub fn on_send_result(&mut self, destination: AddressSpecification, success: bool) {
        self.update_and_handle_action(destination, move |x| {
            AddressQueue::on_send_result(x, success)
        });
    }

    /**
     * Returns true if there is still data pending for this address,
     * or false when no data is pending.
     */
    pub fn on_data_request(&mut self, destination: AddressSpecification) -> bool {
        self.update_and_handle_action(destination.clone(), AddressQueue::on_data_request);
        self.queues
            .get(&destination)
            .map(|state| !state.is_default())
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
