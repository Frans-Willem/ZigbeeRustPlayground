use crate::ieee802154::mac::frame::Frame;
use futures::channel::oneshot;
use futures::future::{Future, TryFutureExt};
use std::collections::VecDeque;
use std::time::Duration;

/**
 * a state-machine implementing a per-device queue.
 * Handles waiting for DataRequest messages, message ordering, etc.
 */
#[derive(Debug)]
pub enum DeviceQueue {
    Idle,
    // We're waiting for a data request for this node.
    WaitingForDataRequest {
        front: DeviceQueueItem,
        queue: VecDeque<DeviceQueueItem>,
    },
    // We've started sending a frame to the radio, but haven't heard back yet.
    SendStarted {
        front: DeviceQueueItem,
        queue: VecDeque<DeviceQueueItem>,
        wait_for_ack: Option<u8>, // After sending, should we still wait for an acknowledgement ?
    },
    // We've sent out the frame, but are waiting for an Ack.
    WaitingForAck {
        front: DeviceQueueItem,
        queue: VecDeque<DeviceQueueItem>,
        wait_for_ack: u8,
    },
}

impl Default for DeviceQueue {
    fn default() -> Self {
        DeviceQueue::Idle
    }
}

/**
 * An event on a device queue may change state, as well as emit an action.
 * It is the responsibility of the caller of the event to do this action.
 */
pub enum DeviceQueueAction {
    /// Start a timer to call on_timer after a specific duration.
    StartTimer(Duration),
    /// Stop any timers set by StartTimer,
    StopTimer(),
    /// Stop any timers set by StartTimer, and start to send out a frame. call on_send_result after sending out the frame.
    StopTimerStartSend(Frame),
}

/**
 * Queued frames will either be resolved as OK, or will return an error:
 */
#[derive(Debug)]
pub enum DeviceQueueError {
    /**
     * It took to long for the device to wake up,
     * max_wait_for_datarequest has elapsed.
     */
    DataRequestTimeout,
    /**
     * Sending was attempted, but the retry limit was reached.
     */
    RetryLimitReached,
    /**
     * Somehow the sending end of the future was dropped.
     * This should realistically never happen.
     */
    ChannelDropped,
}

#[derive(Debug)]
pub struct DeviceQueueItem {
    frame: Frame,
    wait_for_datarequest: bool,
    max_wait_for_datarequest: Option<Duration>, // Ignored if wait_for_datarequest
    retries: usize,                             // retries 0 means try only once.
    callback: oneshot::Sender<Result<(), DeviceQueueError>>,
}

impl DeviceQueueItem {
    pub fn new(
        frame: Frame,
        receiver_on_when_idle: bool,
        max_wait_for_datarequest: Option<Duration>,
        retries: usize,
    ) -> (
        DeviceQueueItem,
        impl Future<Output = Result<(), DeviceQueueError>>,
    ) {
        let (sender, receiver) = oneshot::channel();
        (
            DeviceQueueItem {
                frame,
                wait_for_datarequest: !receiver_on_when_idle,
                max_wait_for_datarequest,
                retries,
                callback: sender,
            },
            receiver.unwrap_or_else(|_| Err(DeviceQueueError::ChannelDropped)),
        )
    }
}

impl DeviceQueue {
    pub fn is_idle(&self) -> bool {
        match self {
            DeviceQueue::Idle => true,
            _ => false,
        }
    }

    pub fn start_send(
        front: DeviceQueueItem,
        queue: VecDeque<DeviceQueueItem>,
    ) -> (Self, Option<DeviceQueueAction>) {
        match front.wait_for_datarequest {
            false => {
                let frame = front.frame.clone();
                let wait_for_ack = front.frame.expect_ack();
                (
                    DeviceQueue::SendStarted {
                        front,
                        queue,
                        wait_for_ack,
                    },
                    Some(DeviceQueueAction::StopTimerStartSend(frame)),
                )
            }
            true => {
                let action = front
                    .max_wait_for_datarequest
                    .map(DeviceQueueAction::StartTimer);
                (DeviceQueue::WaitingForDataRequest { front, queue }, action)
            }
        }
    }

    fn restart_from_queue(queue: VecDeque<DeviceQueueItem>) -> (Self, Option<DeviceQueueAction>) {
        let mut queue = queue;
        if let Some(front) = queue.pop_front() {
            DeviceQueue::start_send(front, queue)
        } else {
            (DeviceQueue::Idle, Some(DeviceQueueAction::StopTimer()))
        }
    }

    fn retry_send(
        front: DeviceQueueItem,
        queue: VecDeque<DeviceQueueItem>,
    ) -> (Self, Option<DeviceQueueAction>) {
        let mut front = front;
        if front.retries < 1 {
            if let Err(e) = front
                .callback
                .send(Err(DeviceQueueError::RetryLimitReached))
            {
                eprintln!("Unable to report failed packet: {:?}", e);
            }
            DeviceQueue::restart_from_queue(queue)
        } else {
            front.retries = front.retries - 1;
            DeviceQueue::start_send(front, queue)
        }
    }

    pub fn enqueue(self, item: DeviceQueueItem) -> (Self, Option<DeviceQueueAction>) {
        match self {
            DeviceQueue::Idle => DeviceQueue::start_send(item, VecDeque::new()),
            DeviceQueue::WaitingForDataRequest { front, mut queue } => {
                queue.push_back(item);
                (DeviceQueue::WaitingForDataRequest { front, queue }, None)
            }
            DeviceQueue::SendStarted {
                front,
                mut queue,
                wait_for_ack,
            } => {
                queue.push_back(item);
                (
                    DeviceQueue::SendStarted {
                        front,
                        queue,
                        wait_for_ack,
                    },
                    None,
                )
            }
            DeviceQueue::WaitingForAck {
                front,
                mut queue,
                wait_for_ack,
            } => {
                queue.push_back(item);
                (
                    DeviceQueue::WaitingForAck {
                        front,
                        queue,
                        wait_for_ack,
                    },
                    None,
                )
            }
        }
    }

    pub fn on_timer(self) -> (Self, Option<DeviceQueueAction>) {
        match self {
            DeviceQueue::WaitingForDataRequest { front, queue } => {
                if let Err(e) = front
                    .callback
                    .send(Err(DeviceQueueError::DataRequestTimeout))
                {
                    eprintln!("Unable to report packet failed: {:?}", e);
                };
                DeviceQueue::restart_from_queue(queue)
            }
            DeviceQueue::WaitingForAck {
                front,
                queue,
                wait_for_ack: _,
            } => DeviceQueue::retry_send(front, queue),
            x => {
                eprintln!("Timer not expected from this state: {:?}", x);
                (x, None)
            }
        }
    }

    pub fn on_data_request(self) -> (Self, Option<DeviceQueueAction>) {
        match self {
            DeviceQueue::WaitingForDataRequest { front, queue } => {
                let frame = front.frame.clone();
                let wait_for_ack = frame.expect_ack();
                (
                    DeviceQueue::SendStarted {
                        front,
                        queue,
                        wait_for_ack,
                    },
                    Some(DeviceQueueAction::StopTimerStartSend(frame)),
                )
            }
            // TODO: Maybe do something in SendStarted or WaitingForAck ?
            x => (x, None),
        }
    }

    pub fn on_send_result(self, success: bool) -> (Self, Option<DeviceQueueAction>) {
        match self {
            DeviceQueue::SendStarted {
                front,
                queue,
                wait_for_ack,
            } => {
                if success == false {
                    DeviceQueue::retry_send(front, queue)
                } else if let Some(wait_for_ack) = wait_for_ack {
                    (
                        DeviceQueue::WaitingForAck {
                            front,
                            queue,
                            wait_for_ack,
                        },
                        Some(DeviceQueueAction::StartTimer(
                            std::time::Duration::from_millis(250),
                        )),
                    )
                } else {
                    if let Err(e) = front.callback.send(Ok(())) {
                        eprintln!("Unable to report packet sent: {:?}", e);
                    }
                    DeviceQueue::restart_from_queue(queue)
                }
            }
            x => {
                eprintln!("Send result not expected from this state: {:?}", x);
                (x, None)
            }
        }
    }

    pub fn on_ack(self, ack: u8) -> (Self, Option<DeviceQueueAction>) {
        match self {
            DeviceQueue::SendStarted {
                front,
                queue,
                mut wait_for_ack,
            } => {
                if wait_for_ack == Some(ack) {
                    wait_for_ack = None;
                }
                (
                    DeviceQueue::SendStarted {
                        front,
                        queue,
                        wait_for_ack,
                    },
                    None,
                )
            }
            DeviceQueue::WaitingForAck {
                front,
                queue,
                wait_for_ack,
            } => {
                if wait_for_ack == ack {
                    if let Err(e) = front.callback.send(Ok(())) {
                        eprintln!("Unable to report packet sent: {:?}", e);
                    }
                    DeviceQueue::restart_from_queue(queue)
                } else {
                    (
                        DeviceQueue::WaitingForAck {
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
